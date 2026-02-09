use serde::Deserialize;

// ── Timestamps ──────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct ProjectTime {
    pub created: Option<u64>,
    pub updated: Option<u64>,
    pub initialized: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct SessionTime {
    pub created: Option<u64>,
    pub updated: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct MessageTime {
    pub created: Option<u64>,
    pub completed: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct PartTime {
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub compacted: Option<u64>,
}

// ── Project ─────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Project {
    pub id: String,
    pub worktree: String,
    pub vcs: Option<String>,
    #[serde(rename = "vcsDir")]
    pub vcs_dir: Option<String>,
    pub sandboxes: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub time: ProjectTime,
    pub icon: Option<serde_json::Value>,
}

impl Project {
    /// Derive a short human-readable name from the worktree path.
    pub fn display_name(&self) -> String {
        if self.id == "global" {
            return "_global".to_string();
        }
        let path = self.worktree.trim_end_matches('/');
        path.rsplit('/').next().unwrap_or(&self.id[..8]).to_string()
    }
}

// ── Session ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SessionSummary {
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub files: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Session {
    pub id: String,
    pub slug: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "projectID")]
    pub project_id: String,
    pub directory: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub time: SessionTime,
    #[serde(default)]
    pub summary: SessionSummary,
}

impl Session {
    /// Filename-safe slug for the session, using the slug field or title.
    pub fn file_stem(&self, date_str: &str) -> String {
        let name = self
            .slug
            .as_deref()
            .or(self.title.as_deref())
            .unwrap_or(&self.id);
        let sanitized: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let truncated = if sanitized.len() > 60 {
            &sanitized[..60]
        } else {
            &sanitized
        };
        format!("{}_{}", date_str, truncated.trim_end_matches('-'))
    }
}

// ── Message ─────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct MessageModel {
    #[serde(rename = "providerID")]
    pub provider_id: Option<String>,
    #[serde(rename = "modelID")]
    pub model_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct TokenCache {
    pub read: Option<u64>,
    pub write: Option<u64>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Tokens {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub reasoning: Option<u64>,
    #[serde(default)]
    pub cache: TokenCache,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct MessagePath {
    pub cwd: Option<String>,
    pub root: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Message {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub role: String,
    #[serde(default)]
    pub time: MessageTime,

    // user-specific fields
    pub model: Option<MessageModel>,

    // assistant-specific fields
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    #[serde(rename = "modelID")]
    pub model_id: Option<String>,
    #[serde(rename = "providerID")]
    pub provider_id: Option<String>,
    pub mode: Option<String>,
    pub agent: Option<String>,
    pub path: Option<MessagePath>,
    pub cost: Option<f64>,
    pub tokens: Option<Tokens>,
    pub finish: Option<String>,
}

impl Message {
    /// Get the model ID regardless of whether this is a user or assistant message.
    pub fn effective_model(&self) -> Option<&str> {
        self.model_id
            .as_deref()
            .or(self.model.as_ref().and_then(|m| m.model_id.as_deref()))
    }
}

// ── Part ────────────────────────────────────────────────────────────

/// Represents the `state` object on tool-type parts.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct ToolState {
    pub status: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub time: Option<PartTime>,
}

/// A part is the atomic content unit. Tagged on `type`.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum PartKind {
    #[serde(rename = "text")]
    Text {
        text: String,
        time: Option<PartTime>,
    },
    #[serde(rename = "tool")]
    Tool {
        #[serde(rename = "callID")]
        call_id: Option<String>,
        tool: String,
        state: ToolState,
    },
    #[serde(rename = "step-start")]
    StepStart { snapshot: Option<String> },
    #[serde(rename = "step-finish")]
    StepFinish {
        reason: Option<String>,
        snapshot: Option<String>,
        cost: Option<f64>,
        tokens: Option<Tokens>,
    },
    #[serde(rename = "reasoning")]
    Reasoning {
        text: Option<String>,
        metadata: Option<serde_json::Value>,
        time: Option<PartTime>,
    },
    #[serde(rename = "patch")]
    Patch {
        hash: Option<String>,
        files: Option<Vec<String>>,
    },
    #[serde(other)]
    Unknown,
}

/// Wrapper that carries the common fields plus the type-specific kind.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Part {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(flatten)]
    pub kind: PartKind,
}

// ── Session Diff ────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct DiffEntry {
    pub file: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub status: Option<String>,
}

// ── Todo ────────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct TodoEntry {
    pub id: String,
    pub content: String,
    pub status: String,
    pub priority: Option<String>,
}

// ── Resolved types (used by resolver) ───────────────────────────────

/// A fully resolved message with its parts inlined.
#[derive(Debug, Clone)]
pub struct ResolvedMessage {
    pub message: Message,
    pub parts: Vec<Part>,
}

/// A resolved session: sub-agent sessions are inlined at the correct position.
#[derive(Debug, Clone)]
pub struct ResolvedSession {
    pub session: Session,
    pub messages: Vec<ResolvedConversationItem>,
    pub diffs: Vec<DiffEntry>,
    pub todos: Vec<TodoEntry>,
    pub token_totals: Tokens,
}

/// An item in the conversation flow — either a normal message or an inlined sub-agent.
#[derive(Debug, Clone)]
pub enum ResolvedConversationItem {
    Message(ResolvedMessage),
    SubAgent {
        session: Session,
        messages: Vec<ResolvedConversationItem>,
    },
}

/// A project with all its resolved sessions.
#[derive(Debug)]
pub struct ResolvedProject {
    pub project: Project,
    pub sessions: Vec<ResolvedSession>,
}
