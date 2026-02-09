# AGENTS.md - oc-export

Rust CLI tool that exports OpenCode conversation histories from on-disk JSON storage to Markdown files. One `.md` per session, organized by project.

## Build / Run / Test Commands

```bash
# Build (debug)
cargo build

# Build (release, optimized)
cargo build --release

# Run directly
cargo run -- --list
cargo run -- --all -o ./opencode-export
cargo run -- --project <name> -o ./export
cargo run -- --session ses_<id>

# Check compilation without producing a binary
cargo check

# Run all tests
cargo test

# Run a single test by name (substring match)
cargo test <test_name>

# Run tests in a specific module
cargo test loader::
cargo test resolver::

# Run tests with output visible
cargo test -- --nocapture

# Lint
cargo clippy

# Lint strictly (deny all warnings)
cargo clippy -- -D warnings

# Format
cargo fmt

# Format check (CI-friendly, no writes)
cargo fmt -- --check
```

There are no tests yet in this codebase. When adding tests, place unit tests in a `#[cfg(test)] mod tests` block at the bottom of each source file, and integration tests in a top-level `tests/` directory.

## Architecture

Four-stage pipeline: `loader -> resolver -> renderer -> main (IO)`

```
src/
  main.rs       CLI parsing (clap), orchestration, file writing
  types.rs      All serde structs + resolved output types
  loader.rs     Reads JSON files from storage/ into StorageData
  resolver.rs   Builds ResolvedProject trees from raw data
  renderer.rs   Renders ResolvedSession -> Markdown string
```

### Data flow

1. `loader::load_all()` reads all JSON from `~/.local/share/opencode/storage/` into HashMaps
2. `resolver::resolve()` builds conversation trees, inlining sub-agent sessions chronologically
3. `renderer::render_session()` converts each resolved session to Markdown
4. `main.rs` writes files to `<output>/<project-name>/<date>_<slug>.md`

### Entity hierarchy

```
Project (1) -> Session (many) -> Message (many) -> Part (many)
```

Sub-agent sessions have a `parentID` pointing to the parent session and are inlined at the correct chronological position during resolution.

## Dependencies

| Crate | Purpose |
|---|---|
| `serde` + `serde_json` | JSON deserialization with derive macros |
| `chrono` | Timestamp formatting (epoch ms -> human dates) |
| `clap` | CLI argument parsing with derive macros |
| `indicatif` | Progress bar during export |
| `anyhow` | Error handling with context |

## Code Style

### Edition & Toolchain

- Rust edition 2021
- No `rustfmt.toml` or `clippy.toml` -- use default `rustfmt` and `clippy` settings

### Formatting

- Default `rustfmt` style (4-space indent, trailing commas in multi-line constructs)
- Section comments use box-style headers: `// -- Section Name --...` with em-dash separators
- Line length: follow rustfmt defaults (~100 chars soft wrap)

### Imports

- Group imports in this order: (1) external crates, (2) `std`, (3) crate-internal
- Use `use crate::types::*` for the types module (glob import, since types are shared everywhere)
- Use specific imports for other internal modules: `use crate::loader::StorageData`
- External crate imports are specific: `use anyhow::{Context, Result}`, `use std::collections::HashMap`

### Naming

- **Structs/Enums**: `PascalCase` (`ResolvedSession`, `PartKind`, `ToolState`)
- **Fields**: `snake_case` with `#[serde(rename = "camelCase")]` for JSON mapping (e.g., `project_id` with `#[serde(rename = "projectID")]`)
- **Functions**: `snake_case` (`load_all`, `render_session`, `format_timestamp`)
- **Modules**: `snake_case` matching filename (`loader`, `renderer`, `resolver`, `types`)
- **Constants**: not used; prefer inline values or function defaults

### Types

- All JSON-deserialized fields that may be absent use `Option<T>`
- Freeform JSON uses `serde_json::Value` (e.g., tool input, metadata)
- Timestamps are `Option<u64>` (Unix milliseconds)
- Enums use serde's internally tagged representation: `#[serde(tag = "type")]`
- Unknown/future enum variants use `#[serde(other)] Unknown` as a catch-all (must be the last variant)
- Struct field defaults via `#[serde(default)]` for nested structs that may be missing
- Use `#[allow(dead_code)]` on structs that deserialize fields not yet used in rendering
- Resolved types (output of resolver) do not derive `Deserialize` -- they are constructed in code

### Error Handling

- Use `anyhow::Result` for all fallible functions
- Use `.context("description")` / `.with_context(|| format!(...))` to annotate errors
- **Individual file parse failures are warnings, not fatal errors**: log to stderr with `eprintln!("warn: skipping <entity> {:?}: {}", path, e)` and continue
- Only abort (via `bail!`) for truly unrecoverable conditions: missing storage directory, invalid CLI args, no matching sessions
- CLI validation uses `anyhow::bail!` with user-facing help text

### String Building

- Use `std::fmt::Write` trait with `writeln!(md, ...)` for building Markdown strings
- Pre-allocate with `String::with_capacity(8192)` for output buffers
- Use `.unwrap()` on `writeln!` to a `String` (infallible in practice)

### Pattern: Prefix-based nesting

All renderer functions accept a `prefix: &str` parameter (`""` at depth 0, `"> "` at depth > 0). Every `writeln!` call must include this prefix to support Markdown blockquote nesting for sub-agent content.

## Adding New Part Types

1. Add a new variant to `PartKind` enum in `types.rs` **before** the `Unknown` catch-all
2. Use `#[serde(rename = "json-type-name")]` to match the JSON `"type"` field
3. All fields should be `Option<T>` for resilience
4. Add a match arm in `render_part()` in `renderer.rs`
5. Rebuild and verify: `cargo build --release && ./target/release/oc-export --all 2>&1 | grep warn:`

## Adding New Tool Renderers

1. Add a match arm in `render_tool_input()` in `renderer.rs`
2. Use `input.get("key").and_then(|v| v.as_str())` for safe JSON field access
3. Always use the `prefix` parameter in every `writeln!` call
4. Use `<details>` for content > 30 lines
5. Use fenced code blocks with language hints for syntax highlighting
6. Unknown tools fall through to the default case which dumps input as formatted JSON

## Key Design Decisions

- **All data loaded into memory at once** -- acceptable for the expected scale (~33K part directories, a few hundred MB)
- **Warn-and-skip on parse errors** -- critical for forward compatibility as OpenCode evolves
- **Sub-agent inlining** -- child sessions are inserted into parent conversation flow at the correct chronological position, with recursive support for nested sub-agents
- **No database** -- reads raw JSON files directly from OpenCode's storage directory
- **Platform detection at compile time** -- `cfg!(target_os = ...)` for storage path resolution
