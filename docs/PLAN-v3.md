# Plan v3: aitop in the Harness Era

> Draft. Successor to `PLAN-v2.md` and `roadmap.md`. Target window: v0.7 → v1.0.

## tl;dr

Coding agents in 2026 are no longer single prompts — they are **harnesses**: long-running loops that spawn subagents, compact context, call tools, and resume across sessions. The model is a commodity input; the harness is where capability lives ("Opus 4.5 scored 78% on CORE with Claude Code's harness vs 42% with Smolagents"). aitop has shipped a clean btop-style cost view of the *old* world (per-session token + dollar totals). The next chapter is to make aitop the **terminal-native telemetry surface for harness behaviour**: subagent cost trees, context-window pressure, compaction events, hook timelines, and rate-limit headroom — all rendered locally from data we already have on disk.

This plan does not propose turning aitop into Grafana, LangSmith, or BurnRate. It proposes keeping the existing constraints (zero auth, local-only, single 3 MB binary, btop feel) and growing the data layer to match where coding agents have moved.

## Where we are (v0.6.7)

Shipped:

- Dashboard / Sessions / Models / Trends, sortable tables, sparklines, heatmap, contribution calendar
- SQLite WAL pipeline with file watcher; sub-200 ms warm start
- Live Token Flow chart with in/out gradient bars
- Budget gauge, burn rate card, cache savings split
- Configurable pricing registry, 6 themes, `--light`, `--tz`, `--theme`, `--refresh`
- Reads Claude Code, Gemini CLI, OpenClaw JSONL — zero auth

Recently reverted (kept on the shelf): multi-provider for Amp/RooCode/Mux/Kimi/Qwen+LiteLLM, `--serve` HTTP API + `--json` + iOS widget, menubar Package.resolved. These are signal that scope discipline matters more than feature count.

## The shift: what changed between PLAN-v2 and now

Five things that the existing plan does not yet account for:

1. **Harness engineering is the new layer.** AI Engineer Europe (Apr 2026) and the upcoming Code Summit (Nov 2026) both headline "Harness Engineering". Anthropic's own *Effective harnesses for long-running agents* and the Claude Agent SDK reframe the agent as a harness + model, not a prompt + model.
2. **Subagents are first-class.** SubagentStart/SubagentStop hooks exist; the Claude Agent SDK parallelises into isolated context windows; orchestrator-worker patterns are the default for non-trivial tasks. Cost is now a **tree**, not a flat list of sessions.
3. **Context is a budget.** Sonnet/Haiku 4.5+ track remaining context; PreCompact fires before auto-compaction; the four context techniques (offload / reduce / retrieve / isolate) all leave parseable footprints in JSONL.
4. **Rate limits are layered.** 5-hour rolling window + weekly cap + Sonnet-specific weekly cap (since Aug 2025). Users care about *time-to-reset* more than $/hr now that the new desktop can burn a Pro quota in 8 minutes.
5. **Hooks and OTEL are universal.** Claude Code emits 12 hook events and a full OTEL stream (`claude_code.token.usage`, `claude_code.cost.usage`, traces). Tools like `claude-code-otel`, `claude-code-hooks-multi-agent-observability`, `claude_telemetry`, and BurnRate are all reading the same firehose. None of them are a single-binary terminal tool.

The white space: **a btop-shaped, local, zero-auth view of harness telemetry.** Hook timelines, subagent cost trees, context-pressure gauges, and reset countdowns — without spinning up Grafana + Prometheus + Loki.

## North star

> A developer should be able to run `aitop`, see at a glance: how much they've spent today, how close they are to their weekly cap, which subagent in which session is currently burning tokens, when context will compact, and whether anything is anomalous — all without leaving the terminal and without configuring anything.

Constraints we will not break:

- Zero network calls by default. Admin API stays opt-in.
- Single static binary, < 5 MB.
- Sub-200 ms warm start; sub-2 s cold index of a year of history.
- 80×24 must still render something useful.
- No telemetry exporter, no daemon. aitop reads; it does not write back.

## Themes for v0.7 → v1.0

### Theme 1 — Subagent cost trees (the headline feature)

Today the Sessions view is flat. In a harness world each "session" is a tree: an orchestrator with N subagents, each with their own context, model, tools, and dollars. Claude Code's JSONL already carries `parentUuid`, agent IDs in tool_use blocks, and Task tool invocations. We have everything we need to reconstruct the tree.

What to build:

- **`agents` table** keyed by `(session_id, agent_id)`, with parent_agent_id, depth, tool counts, model, $ totals, started_at, ended_at.
- **Tree-rendered Sessions view** (toggle with `g` for *graph*): orchestrator at root, indented children, $ + tokens + duration per node, weighted bar showing cost share.
- **Sub-session drill-down**: `Enter` on a parent agent opens the existing session detail popup scoped to that agent's slice.
- **"Hot subagent" badge** in the dashboard top-right when a subagent in an active session exceeds 1.5× its session's median spend.

This alone is the differentiator: BurnRate has it but is SaaS + web; disler/claude-code-hooks-multi-agent-observability has it but needs a web UI + WebSocket server; nobody has it in 3 MB of Rust.

### Theme 2 — Context-window pressure

Cost is a lagging indicator; context pressure is a leading one. Compaction destroys work and burns retries.

- **Parse context-window state from JSONL.** Sonnet/Haiku 4.5+ track remaining context; assistant `usage` blocks carry the totals we need to compute fill %.
- **PreCompact event ingestion.** Detect compaction events from transcript patterns (or, optionally, ingest a hook log if the user has wired one).
- **Per-session context gauge** in the Sessions view: `■■■■■■■□□□ 71%` with colour thresholds; turns red and pulses when compaction is imminent.
- **Compaction timeline** on the Trends view: vertical ticks per compaction event so users can see the "I keep getting compacted" pattern.

### Theme 3 — Rate-limit headroom

ccusage and Claude-Code-Usage-Monitor both show the 5-hour window. Nobody combines 5-hour + weekly + Sonnet-weekly with a btop feel.

- **Top-of-dashboard reset strip**: three thin gauges — 5h, week, week-sonnet — each labelled with $ remaining and time-to-reset.
- **Burn-to-cap projection**: at current burn rate you will hit the weekly cap at HH:MM on DDD.
- **`--guard` flag** (read-only signal, no enforcement): exits non-zero if any cap is above a configurable threshold. Useful as a pre-launch check in shell wrappers or pre-commit. Stays consistent with "aitop is observability, enforcement is external".

### Theme 4 — Hook event timeline (opt-in)

Hooks are the universal API. Most users don't have them wired, so this is opt-in via config.

- **`[hooks] log_dir = "..."`** in config.toml. If present, aitop tails the directory for JSON lines emitted by user hook scripts.
- **Hook timeline view** (new tab `h`): scrolling event feed, emoji per event type, filter by session/agent/event-kind, jump-to-session via `Enter`.
- **Shipping helper:** a tiny `aitop hooks install` command writes a minimal Claude Code `.claude/settings.json` snippet that appends JSONL to the configured log dir. No daemon, no curl-to-localhost.

### Theme 5 — Anomaly + diff intelligence

The "Since last check" banner is a hint. Push it further.

- **Cost-spike detection** (z-score over rolling 14-day window) per project/model/agent — flagged in the Sessions table with a small `▲` tag.
- **Model-drift alert**: notice when a project that usually runs Haiku suddenly uses Opus (or vice versa).
- **Compaction-loop detection**: ≥3 compactions in a single session triggers an inline warning row.
- **`/why` filter**: in any view, press `?` on a row to get a one-screen explanation ("this session is 4.1× your median because of 3 subagents that each spent >$2 in a 12-minute window").

### Theme 6 — Distribution & UX polish

Carrying over the unfinished tail from PLAN-v2 and roadmap.

- Persistent status bar with view-aware hints (carry over from v0.2 plan; partially shipped).
- Compact-mode pass for 80×24 (still rough on Trends).
- Light-mode (`--light`) parity: ensure new dashboards round-trip to non-interactive output.
- Crash-proof sort/filter on partial data (recent fix in ea4ad12 — extend its property tests).

## Out of scope (deliberate)

These are tempting and would dilute the product. Defer or refuse.

- **OTEL collector**. Reading OTEL would mean running a network listener. Out of scope for v1. A user with OTEL set up already has Grafana; we serve the *no-setup* user.
- **Admin API / org dashboards**. Stays opt-in behind `admin_api_key`. Not the v1 story.
- **Web UI / `--serve`**. Already reverted once; do not re-add until there is a clearly different audience.
- **Other providers beyond Claude Code / Gemini CLI / OpenClaw**. Reverted in 0fbe3fa. Re-enter only with a clean adapter trait and at most 2 new providers in a single release.
- **Routines / Dispatch / cloud-only sessions**. They live on Anthropic's infra; without an Admin API key they are invisible. Note this in `--help` so users aren't confused, but do not chase the API.
- **Alerting / Slack / webhooks**. aitop is observability; enforcement and notification are external. `--guard` is the maximum surface area we expose.

## Phases

### Phase 1 — Harness data model (v0.7)

Foundational. No user-visible features land until this is done.

- [ ] Schema migration: `agents` table, indexes on `(session_id, agent_id)`, `(parent_agent_id)`.
- [ ] Parser extension in `src/data/parser.rs`: extract agent_id, parent agent, Task tool spawns, subagent stop markers.
- [ ] Aggregator queries: `agent_tree(session_id)`, `agent_rollup(session_id)`, `hot_subagents()`.
- [ ] Tests in `tests/`: fixtures from a real multi-subagent session.

### Phase 2 — Subagent tree UI (v0.7)

- [ ] Tree renderer in `src/ui/sessions.rs` (toggle `g`).
- [ ] Tree-scoped session detail popup.
- [ ] Hot-subagent badge on Dashboard.
- [ ] Light-mode output for trees (indented plain text).

### Phase 3 — Context pressure (v0.8)

- [ ] Compute per-session context fill % from `usage` blocks.
- [ ] Detect compaction events from transcript patterns.
- [ ] Context gauge column in Sessions; compaction ticks on Trends.
- [ ] Compaction-loop anomaly tag.

### Phase 4 — Rate-limit headroom (v0.8)

- [ ] 5h / weekly / weekly-sonnet rolling windows.
- [ ] Top-of-dashboard reset strip.
- [ ] Burn-to-cap projection text on dashboard.
- [ ] `--guard` flag with exit codes and `--guard-threshold`.

### Phase 5 — Hook timeline (v0.9, opt-in)

- [ ] `[hooks]` config block + log-dir tailer.
- [ ] `aitop hooks install` helper.
- [ ] Hook timeline view (tab `h`).
- [ ] Filter + jump-to-session.

### Phase 6 — Anomaly intelligence (v0.9 → v1.0)

- [ ] Rolling z-score cost spikes.
- [ ] Model-drift detector.
- [ ] `/why` row explainer.
- [ ] Property tests on anomaly thresholds to keep false-positive rate sane.

### Phase 7 — v1.0 polish

- [ ] Compact-mode pass.
- [ ] Status-bar finalisation.
- [ ] Light-mode parity audit.
- [ ] Docs refresh: README screenshots, `docs/architecture.md` update, `docs/research.md` refresh with harness landscape.
- [ ] Cut v1.0 with semver guarantees on `--json` (if/when it returns) and config schema.

## Open questions

These are decisions to make explicitly before each phase starts; do not silently default them.

1. **Tree heuristic vs hook truth.** Without hooks, we infer the subagent tree from `parentUuid` + Task tool invocations. How wrong is the inference in the wild? Need to ground-truth on a captured hook log before promoting trees to the default view.
2. **Context fill computation** depends on JSONL fields that may vary across Claude Code versions. Where do we cap the supported version range?
3. **`--guard` semantics.** Exit codes only, or also a one-line summary on stderr? Settle before shipping; once shell scripts depend on it we are stuck.
4. **Hook log format.** Define a minimal JSONL schema we own; do not invent a new one if Anthropic ships an official "hook log" format first. Watch the Claude Code changelog.
5. **Distribution.** Crates.io + Homebrew are solid; npm shim and shell installer exist. Do we keep all four or trim to the two most-used?

## What this is not

- Not a pivot. The core (zero auth, JSONL → SQLite → Ratatui) is untouched.
- Not a SaaS play. No accounts, no web UI, no cloud sync.
- Not an alerting tool. Anomalies render in the UI; we do not page anyone.
- Not a replacement for OTEL stacks. If you already run Grafana, keep it.

## Research notes

Primary sources that shaped this plan (saved here so future-us can re-trace the reasoning):

- *The Next Evolution of AI Coding Is Harnesses — Here's How to Build Them* (YouTube, Apr 9 2026) — framing of harness as the new engineering surface.
- AI Engineer Europe 2026 schedule (Apr 8–10, London) — "Harness Engineering" as a top-level track alongside Context Engineering and Evals.
- AI Engineer Code Summit (Nov 19–22 2026, NYC) — confirms harness/coding-agent focus for the back half of 2026.
- Anthropic, *Effective harnesses for long-running agents* — orchestrator/worker patterns, context isolation, subagent telemetry.
- Anthropic, *Building agents with the Claude Agent SDK* — same harness as Claude Code, exposed as an SDK; multi-agent in beta.
- Claude Code monitoring docs + SigNoz / Logfire / Grafana write-ups — confirm OTEL is the upstream firehose; we choose not to depend on it.
- `disler/claude-code-hooks-multi-agent-observability` — 12 hook events; event-table schema; multi-agent timeline UI (web). Inspiration for our hook view.
- `ColeMurray/claude-code-otel` — OTEL → Prometheus → Grafana stack. Confirms the heavy-infra alternative; we are the no-infra alternative.
- `affaan-m/everything-claude-code` — harness optimization with local SQLite + skills/instincts. Confirms the local-SQLite-as-source-of-truth pattern.
- BurnRate (getburnrate.io) — SaaS subagent cost trees; the closest competitor in feature shape, opposite in deployment model.
- ccusage, tokscale, tu, Claude-Code-Usage-Monitor — existing local TUIs; all flat-session, none tree-aware.
- Claude Code rate-limit docs (5h + weekly + Sonnet-weekly) — drives Theme 3.
- PreCompact hook references (Anthropic issues #43946, #43733, #34299) — drives Theme 2.
- Claude Code Q1 2026 changelog (Routines, Dispatch, Auto Mode, desktop redesign) — context for why budgets matter more in 2026.
