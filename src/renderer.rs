use chrono::{TimeZone, Utc};
use std::fmt::Write;

use crate::types::*;

/// Render a resolved session to a formatted Markdown string.
pub fn render_session(resolved: &ResolvedSession, project: &Project) -> String {
    let mut md = String::with_capacity(8192);

    // ── Header ──────────────────────────────────────────────────────
    let title = resolved
        .session
        .title
        .as_deref()
        .unwrap_or("Untitled Session");

    let date = format_timestamp(resolved.session.time.created);
    let version = resolved.session.version.as_deref().unwrap_or("unknown");

    // Detect primary model from first assistant message
    let primary_model = resolved
        .messages
        .iter()
        .filter_map(|item| match item {
            ResolvedConversationItem::Message(rm) => {
                if rm.message.role == "assistant" {
                    rm.message.effective_model().map(|s| s.to_string())
                } else {
                    None
                }
            }
            _ => None,
        })
        .next()
        .unwrap_or_else(|| "unknown".to_string());

    writeln!(md, "# {}\n", title).unwrap();
    writeln!(md, "| | |").unwrap();
    writeln!(md, "|---|---|").unwrap();
    writeln!(md, "| **Project** | `{}` |", project.worktree).unwrap();
    writeln!(md, "| **Date** | {} |", date).unwrap();
    writeln!(md, "| **Model** | {} |", primary_model).unwrap();
    writeln!(md, "| **Version** | opencode {} |", version).unwrap();
    if let Some(ref slug) = resolved.session.slug {
        writeln!(md, "| **Slug** | {} |", slug).unwrap();
    }
    writeln!(md, "| **Session** | `{}` |", resolved.session.id).unwrap();
    writeln!(md).unwrap();
    writeln!(md, "---\n").unwrap();

    // ── Conversation ────────────────────────────────────────────────
    render_conversation_items(&mut md, &resolved.messages, 0);

    // ── Todos ───────────────────────────────────────────────────────
    if !resolved.todos.is_empty() {
        writeln!(md, "---\n").unwrap();
        writeln!(md, "## Task List\n").unwrap();
        for todo in &resolved.todos {
            let check = match todo.status.as_str() {
                "completed" => "[x]",
                "in_progress" => "[-]",
                "cancelled" => "[~]",
                _ => "[ ]",
            };
            let priority_badge = match todo.priority.as_deref() {
                Some("high") => " `HIGH`",
                Some("medium") => " `MED`",
                Some("low") => " `LOW`",
                _ => "",
            };
            writeln!(md, "- {} {}{}", check, todo.content, priority_badge).unwrap();
        }
        writeln!(md).unwrap();
    }

    // ── File Changes ────────────────────────────────────────────────
    if !resolved.diffs.is_empty() {
        writeln!(md, "---\n").unwrap();
        writeln!(md, "## Files Changed\n").unwrap();
        for diff in &resolved.diffs {
            let status = diff.status.as_deref().unwrap_or("modified");
            let adds = diff.additions.unwrap_or(0);
            let dels = diff.deletions.unwrap_or(0);
            writeln!(md, "- **{}** ({}) +{} / -{}", diff.file, status, adds, dels).unwrap();
        }
        writeln!(md).unwrap();
    }

    // ── Token Summary ───────────────────────────────────────────────
    let t = &resolved.token_totals;
    let total_in = t.input.unwrap_or(0);
    let total_out = t.output.unwrap_or(0);
    let total_reason = t.reasoning.unwrap_or(0);
    let cache_r = t.cache.read.unwrap_or(0);
    let cache_w = t.cache.write.unwrap_or(0);

    if total_in + total_out > 0 {
        writeln!(md, "---\n").unwrap();
        writeln!(md, "## Token Usage\n").unwrap();
        writeln!(md, "| Metric | Count |").unwrap();
        writeln!(md, "|---|---:|").unwrap();
        writeln!(md, "| Input | {} |", format_number(total_in)).unwrap();
        writeln!(md, "| Output | {} |", format_number(total_out)).unwrap();
        if total_reason > 0 {
            writeln!(md, "| Reasoning | {} |", format_number(total_reason)).unwrap();
        }
        writeln!(md, "| Cache Read | {} |", format_number(cache_r)).unwrap();
        writeln!(md, "| Cache Write | {} |", format_number(cache_w)).unwrap();
        let summary_adds = resolved.session.summary.additions.unwrap_or(0);
        let summary_dels = resolved.session.summary.deletions.unwrap_or(0);
        let summary_files = resolved.session.summary.files.unwrap_or(0);
        if summary_files > 0 {
            writeln!(
                md,
                "| Files Changed | {} (+{} / -{}) |",
                summary_files, summary_adds, summary_dels
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }

    md
}

// ── Conversation rendering ──────────────────────────────────────────

fn render_conversation_items(md: &mut String, items: &[ResolvedConversationItem], depth: usize) {
    for item in items {
        match item {
            ResolvedConversationItem::Message(rm) => {
                render_message(md, rm, depth);
            }
            ResolvedConversationItem::SubAgent { session, messages } => {
                render_sub_agent(md, session, messages, depth);
            }
        }
    }
}

fn render_message(md: &mut String, rm: &ResolvedMessage, depth: usize) {
    let prefix = if depth > 0 { "> " } else { "" };
    let role = &rm.message.role;

    if role == "user" {
        writeln!(md, "{}## User\n", prefix).unwrap();
    } else if role == "assistant" {
        let model = rm.message.effective_model().unwrap_or("assistant");
        let mode = rm.message.mode.as_deref().unwrap_or("");
        let mode_badge = if !mode.is_empty() && mode != "code" {
            format!(" `{}`", mode)
        } else {
            String::new()
        };
        writeln!(md, "{}## Assistant ({}){}\n", prefix, model, mode_badge).unwrap();
    }

    // Render parts
    for part in &rm.parts {
        render_part(md, part, prefix);
    }

    writeln!(md, "{}---\n", prefix).unwrap();
}

fn render_part(md: &mut String, part: &Part, prefix: &str) {
    match &part.kind {
        PartKind::Text { text, .. } => {
            if !text.is_empty() {
                // Prefix each line for blockquote nesting
                if prefix.is_empty() {
                    writeln!(md, "{}\n", text).unwrap();
                } else {
                    for line in text.lines() {
                        writeln!(md, "{}{}", prefix, line).unwrap();
                    }
                    writeln!(md).unwrap();
                }
            }
        }
        PartKind::Tool { tool, state, .. } => {
            render_tool(md, tool, state, prefix);
        }
        PartKind::StepStart { .. } => {
            // Visual step separator (subtle)
        }
        PartKind::StepFinish {
            tokens,
            cost,
            reason,
            ..
        } => {
            // Optionally show step token counts as a small annotation
            if let Some(t) = tokens {
                let out = t.output.unwrap_or(0);
                if out > 0 {
                    let reason_str = reason.as_deref().unwrap_or("done");
                    writeln!(
                        md,
                        "{}*Step: {} output tokens, {}*\n",
                        prefix, out, reason_str
                    )
                    .unwrap();
                }
            }
        }
        PartKind::Reasoning { ref text, .. } => {
            if let Some(t) = text {
                if !t.is_empty() {
                    writeln!(md, "{}<details>", prefix).unwrap();
                    writeln!(md, "{}<summary>Thinking...</summary>\n", prefix).unwrap();
                    if prefix.is_empty() {
                        writeln!(md, "{}\n", t).unwrap();
                    } else {
                        for line in t.lines() {
                            writeln!(md, "{}{}", prefix, line).unwrap();
                        }
                        writeln!(md).unwrap();
                    }
                    writeln!(md, "{}</details>\n", prefix).unwrap();
                }
            }
        }
        PartKind::Patch { ref files, .. } => {
            if let Some(f) = files {
                if !f.is_empty() {
                    writeln!(md, "{}*Patched files:*", prefix).unwrap();
                    for file in f {
                        writeln!(md, "{}- `{}`", prefix, file).unwrap();
                    }
                    writeln!(md).unwrap();
                }
            }
        }
        PartKind::Unknown => {}
    }
}

fn render_tool(md: &mut String, tool: &str, state: &ToolState, prefix: &str) {
    let status = state.status.as_deref().unwrap_or("unknown");
    let title = state.title.as_deref().unwrap_or(tool);

    // Tool header
    let status_indicator = if status == "error" { " **ERROR**" } else { "" };
    writeln!(
        md,
        "{}### Tool: `{}` - {}{}\n",
        prefix, tool, title, status_indicator
    )
    .unwrap();

    // Input
    if let Some(ref input) = state.input {
        render_tool_input(md, tool, input, prefix);
    }

    // Output or Error
    if let Some(ref error) = state.error {
        writeln!(md, "{}**Error:**", prefix).unwrap();
        writeln!(md, "{}```", prefix).unwrap();
        for line in error.lines() {
            writeln!(md, "{}{}", prefix, line).unwrap();
        }
        writeln!(md, "{}```\n", prefix).unwrap();
    } else if let Some(ref output) = state.output {
        if !output.is_empty() {
            render_tool_output(md, tool, output, prefix);
        }
    }
}

fn render_tool_input(md: &mut String, tool: &str, input: &serde_json::Value, prefix: &str) {
    match tool {
        "bash" => {
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                let desc = input
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !desc.is_empty() {
                    writeln!(md, "{}> {}\n", prefix, desc).unwrap();
                }
                writeln!(md, "{}```bash", prefix).unwrap();
                for line in cmd.lines() {
                    writeln!(md, "{}{}", prefix, line).unwrap();
                }
                writeln!(md, "{}```\n", prefix).unwrap();
            }
        }
        "read" => {
            if let Some(path) = input.get("filePath").and_then(|v| v.as_str()) {
                writeln!(md, "{}**File:** `{}`\n", prefix, path).unwrap();
            }
        }
        "write" => {
            if let Some(path) = input.get("filePath").and_then(|v| v.as_str()) {
                writeln!(md, "{}**Write to:** `{}`\n", prefix, path).unwrap();
            }
            if let Some(content) = input.get("content").and_then(|v| v.as_str()) {
                let ext = input
                    .get("filePath")
                    .and_then(|v| v.as_str())
                    .and_then(|p| p.rsplit('.').next())
                    .unwrap_or("");
                writeln!(md, "{}<details>", prefix).unwrap();
                writeln!(
                    md,
                    "{}<summary>File content ({} lines)</summary>\n",
                    prefix,
                    content.lines().count()
                )
                .unwrap();
                writeln!(md, "{}```{}", prefix, ext).unwrap();
                for line in content.lines() {
                    writeln!(md, "{}{}", prefix, line).unwrap();
                }
                writeln!(md, "{}```\n", prefix).unwrap();
                writeln!(md, "{}</details>\n", prefix).unwrap();
            }
        }
        "edit" => {
            if let Some(path) = input.get("filePath").and_then(|v| v.as_str()) {
                writeln!(md, "{}**Edit:** `{}`\n", prefix, path).unwrap();
            }
            if let Some(old) = input.get("oldString").and_then(|v| v.as_str()) {
                writeln!(md, "{}```diff", prefix).unwrap();
                for line in old.lines() {
                    writeln!(md, "{}- {}", prefix, line).unwrap();
                }
                if let Some(new) = input.get("newString").and_then(|v| v.as_str()) {
                    for line in new.lines() {
                        writeln!(md, "{}+ {}", prefix, line).unwrap();
                    }
                }
                writeln!(md, "{}```\n", prefix).unwrap();
            }
        }
        "glob" => {
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                writeln!(md, "{}**Pattern:** `{}` in `{}`\n", prefix, pattern, path).unwrap();
            }
        }
        "grep" => {
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                writeln!(md, "{}**Search:** `{}` in `{}`\n", prefix, pattern, path).unwrap();
            }
        }
        "todowrite" | "todoread" => {
            // Skip rendering todo tool calls — they show up in the task list section
        }
        _ => {
            // Generic: dump input as JSON
            if let Ok(pretty) = serde_json::to_string_pretty(input) {
                writeln!(md, "{}```json", prefix).unwrap();
                for line in pretty.lines() {
                    writeln!(md, "{}{}", prefix, line).unwrap();
                }
                writeln!(md, "{}```\n", prefix).unwrap();
            }
        }
    }
}

fn render_tool_output(md: &mut String, tool: &str, output: &str, prefix: &str) {
    // For write tool, output is often diagnostics — wrap in details
    // For read tool, output can be very long — wrap in details
    let wrap_in_details = matches!(tool, "write" | "read") && output.lines().count() > 30;

    if wrap_in_details {
        writeln!(md, "{}<details>", prefix).unwrap();
        writeln!(
            md,
            "{}<summary>Output ({} lines)</summary>\n",
            prefix,
            output.lines().count()
        )
        .unwrap();
    } else {
        writeln!(md, "{}**Output:**", prefix).unwrap();
    }

    writeln!(md, "{}```", prefix).unwrap();
    for line in output.lines() {
        writeln!(md, "{}{}", prefix, line).unwrap();
    }
    writeln!(md, "{}```\n", prefix).unwrap();

    if wrap_in_details {
        writeln!(md, "{}</details>\n", prefix).unwrap();
    }
}

// ── Sub-agent rendering ─────────────────────────────────────────────

fn render_sub_agent(
    md: &mut String,
    session: &Session,
    messages: &[ResolvedConversationItem],
    depth: usize,
) {
    let title = session.title.as_deref().unwrap_or("Sub-agent");
    let agent_type = session.slug.as_deref().unwrap_or("agent");

    writeln!(md, "---\n").unwrap();
    writeln!(md, "> ### Sub-agent: {} (`{}`)\n", title, agent_type).unwrap();

    render_conversation_items(md, messages, depth + 1);

    writeln!(md, "> *End of sub-agent*\n").unwrap();
    writeln!(md, "---\n").unwrap();
}

// ── Utility ─────────────────────────────────────────────────────────

fn format_timestamp(ts: Option<u64>) -> String {
    match ts {
        Some(ms) => {
            let secs = (ms / 1000) as i64;
            let nanos = ((ms % 1000) * 1_000_000) as u32;
            match Utc.timestamp_opt(secs, nanos) {
                chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M UTC").to_string(),
                _ => format!("{}ms", ms),
            }
        }
        None => "unknown".to_string(),
    }
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
