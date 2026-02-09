mod loader;
mod renderer;
mod resolver;
mod types;

use anyhow::{bail, Result};
use chrono::NaiveDate;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "oc-export",
    about = "Export OpenCode conversation histories to readable Markdown",
    version
)]
struct Cli {
    /// Export all projects and sessions
    #[arg(long, default_value_t = false)]
    all: bool,

    /// Filter to a specific project (matches on worktree path, project ID, or name)
    #[arg(long)]
    project: Option<String>,

    /// Export a single session by ID
    #[arg(long)]
    session: Option<String>,

    /// Output directory
    #[arg(long, short, default_value = "./opencode-export")]
    output: PathBuf,

    /// Only export sessions created after this date (YYYY-MM-DD)
    #[arg(long)]
    since: Option<String>,

    /// Path to the opencode storage directory (auto-detected by default)
    #[arg(long)]
    storage: Option<PathBuf>,

    /// List available projects and exit
    #[arg(long, default_value_t = false)]
    list: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let storage_dir = cli
        .storage
        .unwrap_or_else(|| loader::default_storage_path());

    if !storage_dir.exists() {
        bail!(
            "Storage directory not found: {}\nSpecify with --storage <path>",
            storage_dir.display()
        );
    }

    // ── Load ────────────────────────────────────────────────────────
    eprintln!("Loading data from {} ...", storage_dir.display());

    let data = loader::load_all(&storage_dir)?;

    eprintln!(
        "  {} projects, {} sessions loaded",
        data.projects.len(),
        data.sessions.len()
    );

    // ── List mode ───────────────────────────────────────────────────
    if cli.list {
        println!("{:<12}  {:<40}  {}", "NAME", "WORKTREE", "SESSIONS");
        println!("{}", "-".repeat(80));
        for project in &data.projects {
            let name = project.display_name();
            let count = data
                .sessions_by_project
                .get(&project.id)
                .map(|v| v.len())
                .unwrap_or(0);
            println!("{:<12}  {:<40}  {}", name, project.worktree, count);
        }
        return Ok(());
    }

    // Must specify --all, --project, or --session
    if !cli.all && cli.project.is_none() && cli.session.is_none() {
        bail!(
            "Specify --all, --project <name>, or --session <id>.\n\
             Use --list to see available projects."
        );
    }

    // ── Parse --since ───────────────────────────────────────────────
    let since_ms = match cli.since {
        Some(ref date_str) => {
            let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|e| {
                anyhow::anyhow!(
                    "Invalid --since date '{}': {} (expected YYYY-MM-DD)",
                    date_str,
                    e
                )
            })?;
            let dt = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
            Some(dt.timestamp_millis() as u64)
        }
        None => None,
    };

    // ── Resolve ─────────────────────────────────────────────────────
    let resolved = resolver::resolve(
        &data,
        cli.project.as_deref(),
        cli.session.as_deref(),
        since_ms,
    );

    if resolved.is_empty() {
        bail!("No matching sessions found.");
    }

    let total_sessions: usize = resolved.iter().map(|p| p.sessions.len()).sum();
    eprintln!("Exporting {} sessions ...", total_sessions);

    // ── Render & write ──────────────────────────────────────────────
    let pb = ProgressBar::new(total_sessions as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
            .progress_chars("=> "),
    );

    let mut files_written = 0;

    for rp in &resolved {
        let project_name = rp.project.display_name();
        let project_dir = cli.output.join(&project_name);
        fs::create_dir_all(&project_dir)?;

        for rs in &rp.sessions {
            let date_str = match rs.session.time.created {
                Some(ms) => {
                    let secs = (ms / 1000) as i64;
                    let dt = chrono::DateTime::from_timestamp(secs, 0).unwrap_or_default();
                    dt.format("%Y-%m-%d").to_string()
                }
                None => "unknown".to_string(),
            };

            let filename = format!("{}.md", rs.session.file_stem(&date_str));
            pb.set_message(format!("{}/{}", project_name, filename));

            let markdown = renderer::render_session(rs, &rp.project);

            let filepath = project_dir.join(&filename);
            fs::write(&filepath, &markdown)?;
            files_written += 1;

            pb.inc(1);
        }
    }

    pb.finish_with_message("done");
    eprintln!(
        "\nWrote {} files to {}",
        files_written,
        cli.output.display()
    );

    Ok(())
}
