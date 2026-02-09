use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::*;

/// All raw data loaded from disk, keyed for fast lookup.
pub struct StorageData {
    pub projects: Vec<Project>,
    /// session_id -> Session
    pub sessions: HashMap<String, Session>,
    /// session_id -> Vec<Message> (sorted by time)
    pub messages_by_session: HashMap<String, Vec<Message>>,
    /// message_id -> Vec<Part> (sorted by part id)
    pub parts_by_message: HashMap<String, Vec<Part>>,
    /// session_id -> Vec<DiffEntry>
    pub diffs_by_session: HashMap<String, Vec<DiffEntry>>,
    /// session_id -> Vec<TodoEntry>
    pub todos_by_session: HashMap<String, Vec<TodoEntry>>,
    /// project_id -> Vec<session_id>
    pub sessions_by_project: HashMap<String, Vec<String>>,
}

/// Detect the default opencode storage path for this platform.
pub fn default_storage_path() -> PathBuf {
    if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("opencode")
            .join("storage")
    } else {
        // Windows
        let profile = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(profile)
            .join(".local")
            .join("share")
            .join("opencode")
            .join("storage")
    }
}

/// Load all data from the storage directory.
pub fn load_all(storage_dir: &Path) -> Result<StorageData> {
    let projects = load_projects(&storage_dir.join("project"))?;
    let (sessions, sessions_by_project) = load_sessions(&storage_dir.join("session"))?;
    let messages_by_session = load_messages(&storage_dir.join("message"))?;
    let parts_by_message = load_parts(&storage_dir.join("part"))?;
    let diffs_by_session = load_session_diffs(&storage_dir.join("session_diff"))?;
    let todos_by_session = load_todos(&storage_dir.join("todo"))?;

    Ok(StorageData {
        projects,
        sessions,
        messages_by_session,
        parts_by_message,
        diffs_by_session,
        todos_by_session,
        sessions_by_project,
    })
}

// ── Projects ────────────────────────────────────────────────────────

fn load_projects(dir: &Path) -> Result<Vec<Project>> {
    let mut projects = Vec::new();
    if !dir.exists() {
        return Ok(projects);
    }
    for entry in fs::read_dir(dir).context("reading project dir")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            match load_json::<Project>(&path) {
                Ok(p) => projects.push(p),
                Err(e) => eprintln!("warn: skipping project {:?}: {}", path, e),
            }
        }
    }
    projects.sort_by_key(|p| p.time.created.unwrap_or(0));
    Ok(projects)
}

// ── Sessions ────────────────────────────────────────────────────────

fn load_sessions(dir: &Path) -> Result<(HashMap<String, Session>, HashMap<String, Vec<String>>)> {
    let mut sessions = HashMap::new();
    let mut by_project: HashMap<String, Vec<String>> = HashMap::new();

    if !dir.exists() {
        return Ok((sessions, by_project));
    }
    for project_entry in fs::read_dir(dir).context("reading session dir")? {
        let project_entry = project_entry?;
        let project_dir = project_entry.path();
        if !project_dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&project_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match load_json::<Session>(&path) {
                    Ok(s) => {
                        by_project
                            .entry(s.project_id.clone())
                            .or_default()
                            .push(s.id.clone());
                        sessions.insert(s.id.clone(), s);
                    }
                    Err(e) => {
                        eprintln!("warn: skipping session {:?}: {}", path, e)
                    }
                }
            }
        }
    }
    Ok((sessions, by_project))
}

// ── Messages ────────────────────────────────────────────────────────

fn load_messages(dir: &Path) -> Result<HashMap<String, Vec<Message>>> {
    let mut by_session: HashMap<String, Vec<Message>> = HashMap::new();
    if !dir.exists() {
        return Ok(by_session);
    }
    for session_entry in fs::read_dir(dir).context("reading message dir")? {
        let session_entry = session_entry?;
        let session_dir = session_entry.path();
        if !session_dir.is_dir() {
            continue;
        }
        let session_id = session_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let mut msgs = Vec::new();
        for entry in fs::read_dir(&session_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match load_json::<Message>(&path) {
                    Ok(m) => msgs.push(m),
                    Err(e) => {
                        eprintln!("warn: skipping message {:?}: {}", path, e)
                    }
                }
            }
        }
        msgs.sort_by_key(|m| m.time.created.unwrap_or(0));
        by_session.insert(session_id, msgs);
    }
    Ok(by_session)
}

// ── Parts ───────────────────────────────────────────────────────────

fn load_parts(dir: &Path) -> Result<HashMap<String, Vec<Part>>> {
    let mut by_message: HashMap<String, Vec<Part>> = HashMap::new();
    if !dir.exists() {
        return Ok(by_message);
    }
    for msg_entry in fs::read_dir(dir).context("reading part dir")? {
        let msg_entry = msg_entry?;
        let msg_dir = msg_entry.path();
        if !msg_dir.is_dir() {
            continue;
        }
        let message_id = msg_dir.file_name().unwrap().to_string_lossy().to_string();
        let mut parts = Vec::new();
        for entry in fs::read_dir(&msg_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match load_json::<Part>(&path) {
                    Ok(p) => parts.push(p),
                    Err(e) => {
                        eprintln!("warn: skipping part {:?}: {}", path, e)
                    }
                }
            }
        }
        // Sort parts by their ID (lexicographic = chronological for these IDs)
        parts.sort_by(|a, b| a.id.cmp(&b.id));
        by_message.insert(message_id, parts);
    }
    Ok(by_message)
}

// ── Session Diffs ───────────────────────────────────────────────────

fn load_session_diffs(dir: &Path) -> Result<HashMap<String, Vec<DiffEntry>>> {
    let mut by_session = HashMap::new();
    if !dir.exists() {
        return Ok(by_session);
    }
    for entry in fs::read_dir(dir).context("reading session_diff dir")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let session_id = path.file_stem().unwrap().to_string_lossy().to_string();
            match load_json::<Vec<DiffEntry>>(&path) {
                Ok(diffs) if !diffs.is_empty() => {
                    by_session.insert(session_id, diffs);
                }
                Ok(_) => {} // empty array, skip
                Err(e) => {
                    eprintln!("warn: skipping session_diff {:?}: {}", path, e)
                }
            }
        }
    }
    Ok(by_session)
}

// ── Todos ───────────────────────────────────────────────────────────

fn load_todos(dir: &Path) -> Result<HashMap<String, Vec<TodoEntry>>> {
    let mut by_session = HashMap::new();
    if !dir.exists() {
        return Ok(by_session);
    }
    for entry in fs::read_dir(dir).context("reading todo dir")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let session_id = path.file_stem().unwrap().to_string_lossy().to_string();
            match load_json::<Vec<TodoEntry>>(&path) {
                Ok(todos) if !todos.is_empty() => {
                    by_session.insert(session_id, todos);
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("warn: skipping todo {:?}: {}", path, e)
                }
            }
        }
    }
    Ok(by_session)
}

// ── Helpers ─────────────────────────────────────────────────────────

fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let data = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parsing {}", path.display()))
}
