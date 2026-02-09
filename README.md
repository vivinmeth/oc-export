# oc-export Developer Wiki

A Rust CLI tool that reads OpenCode's on-disk JSON storage and exports every conversation session into human-readable Markdown files. One `.md` file per session, organised by project.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [How OpenCode Stores Data](#how-opencode-stores-data)
3. [Architecture Overview](#architecture-overview)
4. [Data Model (types.rs)](#data-model-typesrs)
5. [Storage Loader (loader.rs)](#storage-loader-loaderrs)
6. [Conversation Resolver (resolver.rs)](#conversation-resolver-resolverrs)
7. [Markdown Renderer (renderer.rs)](#markdown-renderer-rendererrs)
8. [CLI Entry Point (main.rs)](#cli-entry-point-mainrs)
9. [Output Format](#output-format)
10. [Adding New Part Types](#adding-new-part-types)
11. [Adding New Tool Renderers](#adding-new-tool-renderers)
12. [Known Limitations & Future Work](#known-limitations--future-work)

---

## Quick Start

```bash
# Build
cd oc-export
cargo build --release

# List all projects
./target/release/oc-export --list

# Export everything
./target/release/oc-export --all -o ./opencode-export

# Export one project
./target/release/oc-export --project escape-hatch -o ./export

# Export single session by ID
./target/release/oc-export --session ses_3be2dc7faffeD5cOFeAaoN5BAV

# Only sessions after a date
./target/release/oc-export --all --since 2026-01-01

# Custom storage path (if not default)
./target/release/oc-export --all --storage /path/to/opencode/storage

# Install globally
cp ./target/release/oc-export ~/.local/bin/
```

### CLI Flags

| Flag | Type | Default | Description |
|---|---|---|---|
| `--all` | bool | `false` | Export all projects and sessions |
| `--project <name>` | string | - | Filter by project name, worktree path substring, or project ID prefix |
| `--session <id>` | string | - | Export a single session by its `ses_` ID |
| `--output`, `-o` | path | `./opencode-export` | Output directory |
| `--since <YYYY-MM-DD>` | string | - | Only sessions created on or after this date |
| `--storage` | path | auto-detected | Override the opencode storage directory |
| `--list` | bool | `false` | Print projects and session counts, then exit |

You must provide one of `--all`, `--project`, or `--session` (unless using `--list`).

---

## How OpenCode Stores Data

OpenCode persists all session data as **plain JSON files** on disk. There is no SQLite or binary database. Everything lives under a single root:

| Platform | Default Path |
|---|---|
| macOS / Linux | `~/.local/share/opencode/storage/` |
| Windows | `%USERPROFILE%\.local\share\opencode\storage\` |

### Directory Layout

```
~/.local/share/opencode/
├── auth.json                          # API keys & OAuth tokens (DO NOT export)
├── log/                               # Application log files
├── bin/                               # Language server binaries
├── snapshot/                          # Bare git repos for file-change snapshots
│   └── <project-hash>/
├── project/                           # Legacy app config (empty objects)
│   └── <slug>/app.json
├── tool-output/                       # Cached tool invocation results
│   └── tool_*
└── storage/                           # ★ PRIMARY DATA STORE ★
    ├── migration                      # Schema version (currently "2")
    ├── project/                       # Project metadata
    │   ├── global.json
    │   └── <sha1-hash>.json
    ├── session/                       # Sessions grouped by project
    │   └── <project-hash>/
    │       └── ses_*.json
    ├── message/                       # Messages grouped by session
    │   └── ses_<id>/
    │       └── msg_*.json
    ├── part/                          # Message content parts grouped by message
    │   └── msg_<id>/
    │       └── prt_*.json
    ├── session_diff/                  # File diffs per session (flat)
    │   └── ses_*.json
    └── todo/                          # Task lists per session (flat)
        └── ses_*.json
```

### Entity Relationships

```
Project (1) ──> Session (many) ──> Message (many) ──> Part (many)
                    │                    │
                    ├── parentID ──> Session   (sub-agent link)
                    │   (child sessions)
                    ├── session_diff/ses_*.json  (1:1 file diffs)
                    └── todo/ses_*.json          (1:1 task lists)
```

- **Project** is identified by a SHA-1 hash of the `worktree` path (or `"global"` for the catch-all).
- **Session** references its project via `projectID`. Sub-agent sessions have a `parentID` pointing to the parent session.
- **Message** references its session via `sessionID`. Assistant messages link to the triggering user message via `parentID`.
- **Part** references its message via `messageID`. Parts are the atomic content units.
- **Session Diff** and **Todo** are standalone arrays keyed by session ID (filename = `ses_<id>.json`).

### Scale Reference (from real usage)

| Entity | Approximate Count |
|---|---|
| Projects | ~10 |
| Sessions | ~791 |
| Message directories | ~767 |
| Part directories (one per message) | ~33,573 |
| Session diff files | ~758 |
| Todo files | ~160 |

---

## Architecture Overview

The tool follows a four-stage pipeline:

```
┌──────────┐    ┌───────────┐    ┌────────────┐    ┌────────────┐
│  loader   │───>│  resolver  │───>│  renderer  │───>│  main (IO) │
│           │    │            │    │            │    │            │
│ Reads all │    │ Builds     │    │ Converts   │    │ Applies    │
│ JSON from │    │ conversation│    │ resolved   │    │ filters,   │
│ disk into │    │ trees,     │    │ trees to   │    │ writes .md │
│ HashMaps  │    │ inlines    │    │ formatted  │    │ files to   │
│           │    │ sub-agents │    │ Markdown   │    │ disk       │
└──────────┘    └───────────┘    └────────────┘    └────────────┘
```

### Source Files

```
src/
├── main.rs       # CLI parsing (clap), orchestration, file writing
├── types.rs      # All serde structs + resolved output types
├── loader.rs     # Reads JSON files from storage/ into StorageData
├── resolver.rs   # Builds ResolvedProject trees from raw data
└── renderer.rs   # Renders ResolvedSession -> Markdown string
```

### Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `serde` + `serde_json` | 1.x | JSON deserialization with derive macros |
| `chrono` | 0.4 | Timestamp formatting (epoch ms -> human dates) |
| `clap` | 4.x | CLI argument parsing with derive macros |
| `walkdir` | 2.x | Directory traversal (declared but not currently used directly; `fs::read_dir` suffices) |
| `indicatif` | 0.17 | Progress bar during export |
| `anyhow` | 1.x | Error handling with context |

---

## Data Model (types.rs)

`src/types.rs:1-301`

This file defines all the serde structs for deserialization and the resolved types used downstream.

### Raw JSON Types (Deserialized)

#### Timestamp Structs

All timestamps in OpenCode are **Unix milliseconds** (`u64`). Every entity has a different set of optional timestamp fields:

```rust
ProjectTime { created, updated, initialized }   // types.rs:7
SessionTime { created, updated }                  // types.rs:14
MessageTime { created, completed }                // types.rs:20
PartTime    { start, end, compacted }             // types.rs:26
```

All fields are `Option<u64>`.

#### Project (`types.rs:35`)

```rust
struct Project {
    id: String,          // SHA-1 hash of worktree, or "global"
    worktree: String,    // Absolute path to the project root
    vcs: Option<String>, // "git" or absent
    // ...
}
```

The `display_name()` method extracts the last path component of `worktree` for use as a folder name (e.g., `/foo/bar/repos` -> `repos`). The global project maps to `_global`.

#### Session (`types.rs:68`)

```rust
struct Session {
    id: String,               // "ses_..." prefix
    slug: Option<String>,     // Human-readable slug like "misty-comet"
    version: Option<String>,  // OpenCode version, e.g. "1.1.53"
    project_id: String,       // References Project.id (JSON: "projectID")
    parent_id: Option<String>,// For sub-agent sessions (JSON: "parentID")
    title: Option<String>,    // AI-generated summary title
    // ...
}
```

The `file_stem()` method generates a filename-safe string: `<date>_<slug-or-title>`, truncated to 60 chars, with non-alphanumeric chars replaced by hyphens.

**Sub-agent sessions:** When OpenCode spawns a sub-agent (e.g., `@explore`, `@task`), it creates a child session with `parentID` pointing back to the parent session. These are separate JSON files in the same project directory.

#### Message (`types.rs:142`)

User and assistant messages share the same struct but use different fields:

| Field | User | Assistant |
|---|---|---|
| `role` | `"user"` | `"assistant"` |
| `model` (nested object) | Present | Absent |
| `model_id` (flat string) | Absent | Present |
| `parent_id` | Absent | Points to the user message it replies to |
| `tokens` | Absent | Token counts for the response |
| `mode` / `agent` | Absent | `"code"`, `"explore"`, `"build"`, etc. |
| `finish` | Absent | `"stop"` or `"tool-calls"` |
| `path` | Absent | Working directory info |
| `cost` | Absent | Dollar cost (usually 0) |

The `effective_model()` method normalises model ID access: it checks `model_id` first (assistant), then falls back to `model.model_id` (user).

#### Part (`types.rs:192-242`) -- The Core Content

Parts are the atomic content units. Each part has common fields (`id`, `sessionID`, `messageID`) plus a type-specific payload. The `PartKind` enum uses serde's **internally tagged** representation (`#[serde(tag = "type")]`):

| Type | JSON `"type"` | Key Fields | Description |
|---|---|---|---|
| `Text` | `"text"` | `text: String` | Actual user or assistant text content |
| `Tool` | `"tool"` | `tool: String`, `state: ToolState` | Tool invocation with input/output |
| `StepStart` | `"step-start"` | `snapshot: Option<String>` | Marks beginning of an LLM inference step |
| `StepFinish` | `"step-finish"` | `reason`, `tokens`, `cost` | Marks end of inference step with token counts |
| `Reasoning` | `"reasoning"` | `text: Option<String>` | Extended thinking / chain-of-thought |
| `Patch` | `"patch"` | `hash`, `files: Vec<String>` | File snapshot reference |
| `Unknown` | anything else | (none) | Catch-all for future types (`#[serde(other)]`) |

**ToolState** (`types.rs:182`):

```rust
struct ToolState {
    status: Option<String>,              // "completed" or "error"
    input: Option<serde_json::Value>,    // Freeform JSON (varies per tool)
    output: Option<String>,              // Present when status="completed"
    error: Option<String>,               // Present when status="error"
    title: Option<String>,               // Short display title
    metadata: Option<serde_json::Value>, // Tool-specific metadata (freeform)
    time: Option<PartTime>,              // Execution timing
}
```

The `input` field is `serde_json::Value` (freeform) because every tool has different input parameters:

| Tool | Input Fields |
|---|---|
| `bash` | `{ command, description }` |
| `read` | `{ filePath }` |
| `write` | `{ filePath, content }` |
| `edit` | `{ filePath, oldString, newString }` |
| `glob` | `{ pattern, path }` |
| `grep` | `{ pattern, path }` |
| `todowrite` | `{ todos: [...] }` |
| others | Varying JSON objects |

#### DiffEntry (`types.rs:247`)

Session diffs are stored as JSON arrays. Each entry:

```rust
struct DiffEntry {
    file: String,              // Relative file path
    before: Option<String>,    // Old content (empty for new files)
    after: Option<String>,     // New content
    additions: Option<u64>,
    deletions: Option<u64>,
    status: Option<String>,    // "added", "modified", etc.
}
```

#### TodoEntry (`types.rs:259`)

```rust
struct TodoEntry {
    id: String,
    content: String,            // Task description
    status: String,             // "pending", "in_progress", "completed", "cancelled"
    priority: Option<String>,   // "high", "medium", "low"
}
```

### Resolved Types (Output of Resolver)

These are not deserialized from JSON. They're constructed by `resolver.rs`:

```rust
// A message with its parts attached
struct ResolvedMessage {
    message: Message,
    parts: Vec<Part>,
}

// A conversation item: either a regular message or an inlined sub-agent
enum ResolvedConversationItem {
    Message(ResolvedMessage),
    SubAgent {
        session: Session,
        messages: Vec<ResolvedConversationItem>,  // recursive!
    },
}

// A session with everything resolved
struct ResolvedSession {
    session: Session,
    messages: Vec<ResolvedConversationItem>,
    diffs: Vec<DiffEntry>,
    todos: Vec<TodoEntry>,
    token_totals: Tokens,
}

// A project with all its sessions
struct ResolvedProject {
    project: Project,
    sessions: Vec<ResolvedSession>,
}
```

---

## Storage Loader (loader.rs)

`src/loader.rs:1-252`

### Purpose

Reads every JSON file from the `storage/` directory into memory and returns a `StorageData` struct containing HashMaps for fast lookup.

### StorageData (`loader.rs:9`)

```rust
struct StorageData {
    projects: Vec<Project>,
    sessions: HashMap<String, Session>,                // session_id -> Session
    messages_by_session: HashMap<String, Vec<Message>>, // session_id -> messages (sorted)
    parts_by_message: HashMap<String, Vec<Part>>,       // message_id -> parts (sorted)
    diffs_by_session: HashMap<String, Vec<DiffEntry>>,  // session_id -> diffs
    todos_by_session: HashMap<String, Vec<TodoEntry>>,  // session_id -> todos
    sessions_by_project: HashMap<String, Vec<String>>,  // project_id -> session IDs
}
```

### How Loading Works

`load_all()` (`loader.rs:46`) calls six loader functions sequentially:

1. **`load_projects`** (`loader.rs:67`) -- Reads `storage/project/*.json`. Sorted by `time.created`.

2. **`load_sessions`** (`loader.rs:88`) -- Reads `storage/session/<project-hash>/ses_*.json`. Two-level directory scan. Returns both a flat HashMap and a project-grouped index.

3. **`load_messages`** (`loader.rs:125`) -- Reads `storage/message/ses_<id>/msg_*.json`. Messages within each session are sorted by `time.created`.

4. **`load_parts`** (`loader.rs:162`) -- Reads `storage/part/msg_<id>/prt_*.json`. Parts within each message are sorted by **part ID** (lexicographic sort = chronological, because the IDs are time-based).

5. **`load_session_diffs`** (`loader.rs:196`) -- Reads `storage/session_diff/ses_*.json`. Each file is a JSON array. Empty arrays are skipped.

6. **`load_todos`** (`loader.rs:222`) -- Reads `storage/todo/ses_*.json`. Same pattern as diffs.

### Error Handling

Every individual file load is wrapped in a match. Parse failures emit a `warn:` message to stderr and skip the file rather than aborting the entire export. This is critical because:
- OpenCode may introduce new fields or types in future versions
- Some files may be corrupted or partially written
- The `#[serde(other)]` catch-all on `PartKind` handles unknown part types

### Platform Detection

`default_storage_path()` (`loader.rs:26`) uses `cfg!(target_os = ...)` at compile time to determine the correct path. On macOS/Linux it reads `$HOME`, on Windows it reads `$USERPROFILE`.

---

## Conversation Resolver (resolver.rs)

`src/resolver.rs:1-212`

### Purpose

Transforms raw `StorageData` into a tree of `ResolvedProject` objects. The key complexity here is **sub-agent inlining**: child sessions need to be inserted into the parent session's message flow at the correct chronological position.

### Main Entry Point

```rust
pub fn resolve(
    data: &StorageData,
    project_filter: Option<&str>,  // --project flag
    session_filter: Option<&str>,  // --session flag
    since_ms: Option<u64>,         // --since flag (epoch ms)
) -> Vec<ResolvedProject>
```

### Algorithm (`resolver.rs:7-88`)

For each project:

1. **Apply project filter** -- Matches on `worktree.contains(filter)`, `id.starts_with(filter)`, or `display_name()` case-insensitive equality.

2. **Collect all sessions** for this project, sorted by creation time.

3. **Identify sub-agent sessions** -- Any session with a `parent_id` is a sub-agent. Build a `HashSet` of these IDs and a `HashMap<parent_id -> Vec<child_session>>`.

4. **Iterate top-level sessions only** (those NOT in the sub-agent set). Apply `--session` and `--since` filters.

5. **For each top-level session, call `resolve_session()`**.

### Session Resolution (`resolver.rs:90-131`)

1. Get messages for this session from `messages_by_session`.
2. Get child sessions from the `children_by_parent` map.
3. Call `build_conversation()` to interleave messages and sub-agents.
4. Attach diffs and todos.
5. Sum tokens across all messages.

### Conversation Building (`resolver.rs:133-189`)

This is the core algorithm for sub-agent inlining:

1. Sort child sessions by `time.created`.
2. Walk through the parent session's messages in order.
3. Before each message, insert any child sessions whose `time.created` is <= the message's `time.created`.
4. Each child session is recursively resolved (so nested sub-agents work).
5. After all messages, append any remaining child sessions.

```
Parent messages:    M1 (t=100)    M2 (t=300)    M3 (t=500)
Child sessions:         C1 (t=200)         C2 (t=400)

Result: M1, C1, M2, C2, M3
```

### Token Summation (`resolver.rs:191-211`)

Iterates all messages in the session and sums up `tokens.input`, `tokens.output`, `tokens.reasoning`, `tokens.cache.read`, `tokens.cache.write`. Only assistant messages have token data.

---

## Markdown Renderer (renderer.rs)

`src/renderer.rs:1-440`

### Purpose

Takes a `ResolvedSession` and produces a complete Markdown string. One call per output file.

### Document Structure

Each exported `.md` file has this structure:

```
# <Session Title>

| Metadata table |
|---|

---

## User
<text content>

---

## Assistant (<model>) `<mode>`
<text content>
### Tool: `bash` - <title>
<input / output>

---

(repeat for all messages)

---

## Task List
- [x] completed task `HIGH`
- [ ] pending task `MED`

---

## Files Changed
- **src/foo.ts** (modified) +10 / -3

---

## Token Usage
| Metric | Count |
| Input  | 1.2K  |
| Output | 568   |
```

### Key Functions

#### `render_session()` (`renderer.rs:7`)

Top-level function. Builds the full document:
1. Extracts metadata (title, date, model, version, slug, session ID).
2. Renders the metadata table.
3. Calls `render_conversation_items()` for the message flow.
4. Renders todos, file changes, and token usage sections.

#### `render_conversation_items()` (`renderer.rs:128`)

Dispatches each `ResolvedConversationItem` to either `render_message()` or `render_sub_agent()`.

#### `render_message()` (`renderer.rs:141`)

1. Outputs role heading (`## User` or `## Assistant (<model>) <mode>`).
2. Iterates all parts and calls `render_part()` for each.
3. Appends a horizontal rule separator.

The `prefix` parameter controls blockquote nesting. At depth 0 it's empty; at depth > 0 it's `"> "`, which makes sub-agent content appear as Markdown blockquotes.

#### `render_part()` (`renderer.rs:166`)

Dispatches by `PartKind`:

| Part Type | Rendering |
|---|---|
| `Text` | Plain text (line-by-line with prefix for nesting) |
| `Tool` | Delegated to `render_tool()` |
| `StepStart` | Silent (no output) |
| `StepFinish` | Italic annotation: `*Step: 568 output tokens, stop*` |
| `Reasoning` | Wrapped in `<details><summary>Thinking...</summary>` collapsible |
| `Patch` | Italic list: `*Patched files:* - \`path\`` |
| `Unknown` | Silent (no output) |

#### `render_tool()` (`renderer.rs:239`)

1. Outputs heading: `### Tool: \`<name>\` - <title>`
2. Calls `render_tool_input()` for structured input rendering.
3. Calls `render_tool_output()` for output, or renders error block.

#### `render_tool_input()` (`renderer.rs:272`)

Tool-specific rendering logic:

| Tool | Rendering |
|---|---|
| `bash` | Description as blockquote, command in ```bash fenced block |
| `read` | `**File:** \`<path>\`` |
| `write` | `**Write to:** \`<path>\``, content in `<details>` collapsible with syntax-highlighted fenced block |
| `edit` | `**Edit:** \`<path>\``, old/new strings as ```diff block with `-` / `+` prefixes |
| `glob` | `**Pattern:** \`<glob>\` in \`<dir>\`` |
| `grep` | `**Search:** \`<regex>\` in \`<dir>\`` |
| `todowrite` / `todoread` | Skipped (rendered in Task List section instead) |
| Anything else | Input dumped as formatted JSON |

#### `render_tool_output()` (`renderer.rs:366`)

- Short outputs (< 30 lines): inline with `**Output:**` heading.
- Long `read` or `write` outputs: wrapped in `<details>` collapsible.

#### `render_sub_agent()` (`renderer.rs:397`)

Renders a sub-agent session as a blockquoted section:
```markdown
---
> ### Sub-agent: <title> (`<slug>`)
> (all messages inside, recursively)
> *End of sub-agent*
---
```

### Utility Functions

- `format_timestamp()` (`renderer.rs:417`) -- Converts epoch ms to `"2025-12-15 14:30 UTC"`.
- `format_number()` (`renderer.rs:431`) -- Formats large numbers: 1234 -> `1.2K`, 1234567 -> `1.2M`.

---

## CLI Entry Point (main.rs)

`src/main.rs:1-176`

### Flow

1. Parse CLI args with `clap::Parser`.
2. Determine storage path (flag or auto-detect).
3. Call `loader::load_all()` to read everything into memory.
4. If `--list`, print project table and exit.
5. Validate that one of `--all`, `--project`, or `--session` was provided.
6. Parse `--since` date string to epoch ms.
7. Call `resolver::resolve()` with filters.
8. For each `ResolvedProject` / `ResolvedSession`, call `renderer::render_session()` and write the result to `<output>/<project-name>/<date>_<slug>.md`.
9. Display progress bar via `indicatif`.

### Output File Naming

```
<output_dir>/
  <project_display_name>/
    <YYYY-MM-DD>_<session-slug-or-title-truncated-60-chars>.md
```

Examples:
```
opencode-export/repos/2025-12-15_misty-comet.md
opencode-export/escape-hatch/2026-01-20_Landing-pages-complete--domain-strategy-locked.md
opencode-export/_global/2025-11-16_New-session---2025-11-16.md
```

---

## Output Format

### Example Exported Session

```markdown
# Fix OAuth token refresh

| | |
|---|---|
| **Project** | `/home/user/repos/myapp` |
| **Date** | 2025-12-15 14:30 UTC |
| **Model** | claude-opus-4-5 |
| **Version** | opencode 1.1.53 |
| **Slug** | misty-comet |
| **Session** | `ses_3be2dc7faffeD5cOFeAaoN5BAV` |

---

## User

Can you fix the OAuth token refresh logic? It's not handling expired tokens.

---

## Assistant (claude-opus-4-5) `build`

Let me look at the current implementation.

### Tool: `read` - src/auth.rs

**File:** `/home/user/repos/myapp/src/auth.rs`

<details>
<summary>Output (142 lines)</summary>

(file content here)

</details>

### Tool: `bash` - Check test output

> Run the auth tests

```bash
cargo test auth:: --no-capture
```

**Output:**
```
running 3 tests
test auth::test_refresh_token ... FAILED
```

I see the issue. The refresh function doesn't handle the 401 response...

### Tool: `edit` - src/auth.rs

**Edit:** `/home/user/repos/myapp/src/auth.rs`

```diff
- fn refresh(&self) -> Result<Token> {
-     let resp = self.client.post(&self.token_url)
+ fn refresh(&self) -> Result<Token> {
+     let resp = self.client.post(&self.token_url)
+         .bearer_auth(&self.refresh_token)
```

*Step: 568 output tokens, stop*

---

## Task List

- [x] Investigate failing refresh logic `HIGH`
- [x] Fix token refresh endpoint auth `HIGH`
- [x] Verify tests pass `HIGH`

---

## Files Changed

- **src/auth.rs** (modified) +15 / -8

---

## Token Usage

| Metric | Count |
|---|---:|
| Input | 1.2K |
| Output | 568 |
| Cache Read | 72.9K |
| Cache Write | 464 |
| Files Changed | 1 (+15 / -8) |
```

---

## Adding New Part Types

When OpenCode introduces a new part type, the tool will log `warn: skipping part` messages. To add support:

### Step 1: Identify the new type

Check a warning file to see the JSON structure:
```bash
cat ~/.local/share/opencode/storage/part/msg_<id>/prt_<id>.json | jq .type
```

### Step 2: Add variant to PartKind enum

In `src/types.rs`, add a new variant **before** the `Unknown` catch-all:

```rust
#[serde(rename = "your-new-type")]
YourNewType {
    field1: Option<String>,
    field2: Option<serde_json::Value>,
},
#[serde(other)]      // <-- must remain last
Unknown,
```

Field names must match the JSON keys exactly. Use `Option<T>` for anything that might be absent. Use `serde_json::Value` for freeform/unknown structures.

### Step 3: Add rendering logic

In `src/renderer.rs`, add a match arm in `render_part()`:

```rust
PartKind::YourNewType { ref field1, .. } => {
    if let Some(val) = field1 {
        writeln!(md, "{}*Your new type: {}*\n", prefix, val).unwrap();
    }
}
```

### Step 4: Rebuild and test

```bash
cargo build --release
./target/release/oc-export --all 2>&1 | grep "warn:" | head -5
# Should show 0 warnings for the new type
```

---

## Adding New Tool Renderers

When OpenCode adds new tools (e.g., a new MCP tool), they automatically get the fallback JSON rendering. To add proper formatting:

### In `render_tool_input()` (`renderer.rs:272`)

Add a new match arm:

```rust
"your_tool" => {
    if let Some(param) = input.get("paramName").and_then(|v| v.as_str()) {
        writeln!(md, "{}**Param:** `{}`\n", prefix, param).unwrap();
    }
}
```

### Tips

- Always use the `prefix` parameter for every `writeln!` call (enables blockquote nesting in sub-agents).
- Use `<details>` for long content (> 30 lines).
- Use fenced code blocks with language hints for syntax highlighting.
- Use `input.get("key").and_then(|v| v.as_str())` for safe JSON field access.

---

## Known Limitations & Future Work

### Current Limitations

1. **Memory usage** -- All data is loaded into memory at once. With ~33K part directories this works fine (a few hundred MB), but could be an issue with very large histories. A streaming approach (load per-session) would fix this.

2. **No PDF export** -- Only Markdown output is supported. PDF can be achieved externally:
   ```bash
   # Using pandoc
   for f in opencode-export/**/*.md; do
     pandoc "$f" -o "${f%.md}.pdf" --pdf-engine=wkhtmltopdf
   done
   ```

3. **No incremental export** -- Every run re-exports everything matching the filters. A `--skip-existing` flag could check for existing files and skip them.

4. **Filename collisions** -- If two sessions have the same date and slug, the second will overwrite the first. Adding a short ID suffix would fix this.

5. **`<details>` in blockquotes** -- Nested `<details>` tags inside Markdown blockquotes (`>`) don't render well in all Markdown previews. GitHub renders them correctly; some other viewers may not.

6. **Token cost is always 0** -- OpenCode stores `cost: 0` in messages (cost is computed client-side, not persisted). The tool includes the field but it's not useful yet.

### Future Improvements

- **`--format pdf`** flag with built-in pandoc invocation
- **`--compact`** flag to skip tool calls and only show text
- **`--skip-existing`** for incremental exports
- **`--json`** flag to output structured JSON instead of Markdown
- **Parallel loading** with rayon for faster startup on large histories
- **Config file** (`.oc-export.toml`) for default flags
- **Index page** -- Generate a `README.md` per project with a table of all sessions (date, title, model, token count, link to file)
- **Search** -- Full-text search across exported markdown (or build a search index during export)

---

## Appendix: OpenCode ID Format

All OpenCode entity IDs follow the pattern `<prefix>_<base62-encoded-timestamp+random>`:

| Prefix | Entity |
|---|---|
| `ses_` | Session |
| `msg_` | Message |
| `prt_` | Part |
| `toolu_` or `call_` | Tool call (from the LLM provider) |

The IDs are lexicographically sortable (earlier IDs sort before later ones), which is why part sorting by ID works as chronological sorting.

### Project IDs

Project IDs are SHA-1 hashes of the worktree path:
```
sha1("/Volumes/muesync_store_3/Workspace/Surkyl/repos") 
  = "dafbf55cfe3ca21c3f575cab2b40902b13fbc4f2"
```

The special project `"global"` is used for sessions not tied to a specific git repository.
