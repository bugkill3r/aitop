# CLAUDE.md

## Project Overview

**aitop** is a btop/htop-style terminal dashboard for monitoring AI token usage and costs. It reads Claude Code session files from `~/.claude/projects/` and presents them in a Ratatui-based TUI.

## Build & Run

```bash
cargo build           # dev build
cargo build --release # release build
cargo run             # launch TUI
cargo run -- --light  # non-interactive table output
cargo run -- --theme dracula
```

## Architecture

- **src/main.rs** — Entry point, event loop, key handling, status bar rendering
- **src/app.rs** — `AppState` struct and `View` enum, all UI state
- **src/config.rs** — Config file parsing (`~/.config/aitop/config.toml`)
- **src/data/** — Data layer
  - `scanner.rs` — Finds JSONL session files in `~/.claude/projects/`
  - `parser.rs` — Parses JSONL lines into typed structs
  - `db.rs` — SQLite database (WAL mode), file index, metadata table
  - `aggregator.rs` — Read-only queries (dashboard stats, model breakdown, sessions, trends, etc.)
  - `watcher.rs` — File system watcher using `notify` crate
- **src/ui/** — UI rendering modules
  - `theme.rs` — 6 color themes (ember, nord, dracula, gruvbox, catppuccin, mono)
  - `layout.rs` — Main layout with tab bar, content area, status bar
  - `dashboard.rs` — Dashboard view with stats, charts, budget gauge, delta banner
  - `sessions.rs` — Session table with sparklines, sort indicators, filter label
  - `models.rs` — Model breakdown table
  - `trends.rs` — Daily spend chart, heatmap, contribution calendar
  - `help.rs` — Help overlay popup
  - `filter.rs` — Search/filter overlay for sessions
  - `session_detail.rs` — Session detail popup (scrollable messages)
  - `widgets/` — Reusable widgets (cost_color gradient)

## Key Conventions

- Never add `Co-Authored-By` to commits
- Commit and push after completing each phase/feature
- SQLite uses WAL mode for concurrent read (aggregator) / write (db) access
- Two separate `Database` instances: one for writes, one read-only for `Aggregator`
- File watcher uses `notify` crate with tokio→std mpsc bridge for cross-thread events
- Cost calculations use hardcoded Anthropic pricing in `parser.rs`
