# AI Orchestrator / Router — Research & Plan

> Evolving aitop from passive observability into an active orchestration layer
> for multi-agent AI workflows.

## Problem Statement

Power users run 12+ concurrent AI agents (each with sub-agents) against shared
provider rate limits. Today:

1. **Blind competition** — Every agent competes for the same token pool with no
   awareness of other agents' consumption
2. **Cliff-edge failures** — When Anthropic's 5-hour rolling window limit hits,
   ALL agents stall simultaneously. No graceful degradation.
3. **No priority system** — A background linting agent consumes the same quota
   as a critical implementation agent
4. **Wasted capacity** — Simple tasks (grep-like lookups, formatting) burn
   expensive Opus tokens when Haiku would suffice
5. **Single-provider lock-in** — No automatic failover when one provider
   throttles

**The core insight**: aitop already *sees* everything (token flows, costs, burn
rates, sessions). The next step is to *act* on what it sees.

---

## Current aitop Architecture — Extensibility Points

| Component | What it does today | Orchestrator hook |
|-----------|-------------------|-------------------|
| `PricingRegistry` | Multi-model cost computation | Route decisions based on cost |
| `Provider` enum | Claude, Gemini, OpenClaw | Add providers as routing targets |
| `Aggregator` | Real-time token/cost queries | Budget health signals |
| SQLite DB (WAL) | Session + message tracking | Store routing decisions, agent registry |
| File watcher | Event-driven data refresh | Trigger re-routing on budget changes |
| `config.toml` | Themes, budgets, pricing overrides | Orchestrator config section |
| `admin_api_key` | Placeholder in config | Provider API key management |

The architecture's clean layering (Parser → DB → Aggregator → UI) means we can
insert an orchestration layer between providers and agents without touching the
monitoring stack.

---

## Competitive Landscape

| Tool | What it does | Gap vs our vision |
|------|-------------|-------------------|
| **LiteLLM** | OpenAI-compatible proxy, 100+ providers | Cloud-focused, no local-first TUI, no agent-priority routing, no budget-aware degradation |
| **Portkey** | AI gateway with observability | SaaS-only, team-focused, no terminal UI, no agent hierarchy |
| **Helicone** | Request logging + analytics | Logging-only, no active routing/throttling |
| **Braintrust** | Eval + proxy | Eval-focused, routing is secondary |
| **OpenRouter** | Multi-model routing | Separate API key economy, no local agent awareness |
| **Martian** | Intelligent model router | Cloud API, no local context, no budget management |

**Our differentiation:**
- **Local-first**: Zero-auth, runs on your machine, reads your session files
- **Already has observability**: aitop's monitoring is the foundation — competitors start from scratch
- **Agent-topology-aware**: Understands parent/child agent relationships (aligns with ACE's sub-agent philosophy)
- **Budget-aware routing**: Not just cheapest model — smartest allocation given remaining capacity
- **Terminal-native**: Power users live in the terminal

---

## Architecture

### Core Concept: Local AI Traffic Controller

A local HTTP proxy sits between agents and AI providers. Agents connect via
standard environment variables (`ANTHROPIC_BASE_URL=http://localhost:8420/v1`).
Completely transparent — no agent code changes required.

```
┌─────────────────────────────────────────────────────────────┐
│                     Agent Ecosystem                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ Agent 1  │  │ Agent 2  │  │ Agent 3  │  │ Agent N  │   │
│  │ (main)   │  │ (main)   │  │ (sub)    │  │ (bg)     │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
│       │              │              │              │         │
│       └──────────────┴──────┬───────┴──────────────┘         │
│                             │                                │
│                    ANTHROPIC_BASE_URL                        │
│                    http://localhost:8420                     │
└─────────────────────────────┬───────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────┐
│                   aitop Orchestrator                         │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Agent        │  │ Token Budget │  │ Intelligent  │      │
│  │ Registry     │  │ Manager      │  │ Router       │      │
│  │              │  │              │  │              │      │
│  │ • Discovery  │  │ • 5hr window │  │ • Priority   │      │
│  │ • Priority   │  │ • Burn rate  │  │ • Model pick │      │
│  │ • Hierarchy  │  │ • Prediction │  │ • Degradation│      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│                                                              │
│  ┌──────────────────────────────────────────────────┐       │
│  │              Observability (existing aitop)        │       │
│  │   Dashboard │ Sessions │ Models │ Trends │ Agents │       │
│  └──────────────────────────────────────────────────┘       │
└─────────────────────────────┬───────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────┐
│                      AI Providers                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │Anthropic │  │ Google   │  │ OpenAI   │                  │
│  │  API     │  │ Gemini   │  │  API     │                  │
│  └──────────┘  └──────────┘  └──────────┘                  │
└─────────────────────────────────────────────────────────────┘
```

### Layer 1: Proxy Server

- Local HTTP/HTTPS proxy using `axum` (Rust-native, async, excellent middleware)
- Intercepts requests to `api.anthropic.com`, `generativelanguage.googleapis.com`
- **Transparent**: Agents just set `ANTHROPIC_BASE_URL` env var
- Passes through API keys from agent configs (or manages them centrally)
- Request/response logging to existing SQLite DB

### Layer 2: Token Budget Manager

- Tracks token consumption per provider's rolling window (Anthropic = 5 hours)
- Reads rate limit headers from responses (`x-ratelimit-limit-tokens`,
  `x-ratelimit-remaining-tokens`, `x-ratelimit-reset-tokens`)
- Also learns from 429 responses (back-pressure signal)
- Maintains running totals per model tier (Opus/Sonnet/Haiku have separate limits)
- **Burn rate prediction**: at current rate, when will we hit the wall?
- Exposes budget health as queryable metric for router + TUI

### Layer 3: Agent Registry & Priority

Agents are identified and prioritized:

```
Discovery methods (layered):
1. X-Agent-Id header (explicit, agents opt-in)
2. API key fingerprint (different keys = different agents)
3. Source port / PID tracking (automatic, no agent changes)
4. Manual registration via local API
```

Priority tiers:

| Tier | Priority | Examples | Behavior when budget tight |
|------|----------|----------|---------------------------|
| Critical | 0-19 | User-facing main agents | Never throttled |
| Primary | 20-39 | Core implementation agents | Last to be throttled |
| Secondary | 40-59 | Research/exploration agents | Downgraded to cheaper models |
| Background | 60-79 | Linting, formatting, tests | Queued/paused |
| Idle | 80-100 | Speculative prefetch | Killed first |

Parent-child relationships tracked — sub-agents inherit parent context but
get lower default priority.

### Layer 4: Intelligent Router

Routes each request based on multiple signals:

```
┌─────────────────────────────────────────┐
│           Routing Decision Tree          │
│                                          │
│  1. Budget check → remaining capacity?   │
│     ├─ >70% → route as requested         │
│     ├─ 30-70% → apply strategy           │
│     └─ <30% → conservation mode          │
│                                          │
│  2. Agent priority → who's asking?       │
│     ├─ Critical → always gets best model │
│     ├─ Primary → gets requested or -1    │
│     └─ Background → cheapest available   │
│                                          │
│  3. Task complexity → what kind of work? │
│     ├─ High thinking → Opus/Sonnet       │
│     ├─ Code gen → Sonnet                 │
│     └─ Simple lookup → Haiku             │
│                                          │
│  4. Cost optimization → ROI check        │
│     └─ Is this task worth Opus tokens?   │
└─────────────────────────────────────────┘
```

Routing strategies (configurable):

| Strategy | Behavior |
|----------|----------|
| `performance` | Always use best available model |
| `balanced` | Route by task complexity + budget |
| `budget` | Maximize tokens-per-dollar |
| `survival` | Near limits — aggressive downgrade for non-critical agents |

**Model downgrade chain**: `opus → sonnet → haiku` (configurable)

### Layer 5: ACE Framework Integration

ACE's 8-step workflow maps naturally to routing priorities:

| ACE Phase | Default Priority | Default Model |
|-----------|-----------------|---------------|
| Questions | Secondary (40) | Sonnet |
| Research | Secondary (50) | Sonnet/Haiku |
| Design | Primary (25) | Opus |
| Structure | Primary (30) | Sonnet |
| Plan | Primary (20) | Opus |
| Worktree | Background (60) | — (git ops) |
| Implement | Critical (10) | Opus |
| PR | Secondary (45) | Sonnet |

ACE's philosophy — "sub-agents are for context control, not org-chart
cosplay" — aligns perfectly. Sub-agents doing research don't need Opus.
The orchestrator enforces this automatically.

---

## Data Model Extensions

```sql
-- Agent registry
CREATE TABLE agents (
    id TEXT PRIMARY KEY,           -- auto-discovered or manually set
    name TEXT NOT NULL,
    parent_agent_id TEXT REFERENCES agents(id),
    priority INTEGER DEFAULT 50,   -- 0 = highest, 100 = lowest
    status TEXT DEFAULT 'active',  -- active | throttled | paused | killed
    ace_phase TEXT,                -- questions | research | implement | etc.
    registered_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);

-- Every proxied request logged
CREATE TABLE routing_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT REFERENCES agents(id),
    requested_model TEXT NOT NULL,
    routed_model TEXT NOT NULL,    -- may differ if downgraded
    reason TEXT,                   -- 'passthrough' | 'budget_conservation' | 'priority_downgrade'
    input_tokens INTEGER,
    output_tokens INTEGER,
    cost_usd REAL,
    latency_ms INTEGER,
    timestamp TEXT NOT NULL
);

-- Rolling window token budget tracking
CREATE TABLE token_windows (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,        -- 'anthropic' | 'google' | 'openai'
    model_tier TEXT NOT NULL,      -- 'opus' | 'sonnet' | 'haiku'
    window_start TEXT NOT NULL,
    tokens_used INTEGER DEFAULT 0,
    tokens_limit INTEGER,         -- learned from response headers
    requests_used INTEGER DEFAULT 0,
    requests_limit INTEGER,
    updated_at TEXT NOT NULL
);

-- Per-agent quota allocation
CREATE TABLE agent_quotas (
    agent_id TEXT REFERENCES agents(id),
    model_tier TEXT NOT NULL,
    max_tokens_per_window INTEGER,
    max_requests_per_window INTEGER,
    PRIMARY KEY (agent_id, model_tier)
);
```

---

## Configuration Extensions

```toml
# ~/.config/aitop/config.toml

[orchestrator]
enabled = true
listen_addr = "127.0.0.1"
listen_port = 8420
strategy = "balanced"  # performance | balanced | budget | survival

[orchestrator.providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"        # read key from env var
base_url = "https://api.anthropic.com"
rate_limit_window_hours = 5

[orchestrator.providers.anthropic.limits]
# Known limits (auto-detected from headers where possible)
opus_tpm = 40000
sonnet_tpm = 80000
haiku_tpm = 200000

[orchestrator.routing]
conservation_threshold = 30              # % remaining triggers conservation
default_model = "claude-sonnet-4-6"
downgrade_chain = ["claude-opus-4-6", "claude-sonnet-4-6", "claude-haiku-4-5"]

[orchestrator.priorities]
# Agent name patterns → priority (lower number = higher priority)
"implement-*" = 10
"main-*" = 20
"research-*" = 40
"test-*" = 50
"lint-*" = 70
"background-*" = 90
```

---

## Implementation Phases

### Phase 1: Proxy Foundation (MVP)
- `axum`-based HTTP proxy server (new binary: `aitop-proxy` or flag `aitop --proxy`)
- Pass-through mode — no routing logic, just forward requests
- Log all requests/responses to SQLite (reuse existing DB infrastructure)
- Parse token counts from API response `usage` fields
- Basic TUI integration: new "Proxy" tab showing live request log
- **Deliverable**: Working proxy that agents can point at via env var

### Phase 2: Token Budget Tracking
- Parse rate limit headers from Anthropic responses
- Implement 5-hour rolling window accumulator
- Burn rate prediction (tokens/min extrapolated to window end)
- Dashboard gauge: "Budget Health" — remaining capacity visualization
- Alerts when approaching limits (desktop notification reuse)
- **Deliverable**: Users see exactly how much capacity remains

### Phase 3: Agent Registry & Priority
- Auto-discover agents from request patterns (API key, headers, source)
- Manual registration endpoint (`POST /agents`)
- Parent-child relationship tracking
- Priority assignment: manual config + pattern matching
- New TUI view: "Agents" tab showing all registered agents + their consumption
- **Deliverable**: Visibility into per-agent resource consumption

### Phase 4: Intelligent Routing
- Model selection based on budget remaining + agent priority
- Request queuing for throttled agents (with configurable timeout)
- Automatic model downgrade when budget is tight
- Configurable routing strategies (performance/balanced/budget/survival)
- Routing decision log in TUI
- **Deliverable**: Smart resource allocation across agents

### Phase 5: Multi-Provider Routing
- Add Google Gemini and OpenAI as routing targets
- Cross-provider failover (Anthropic throttled → route to Gemini)
- Unified token budget view across all providers
- Cost-optimized routing (same task, cheapest provider)
- **Deliverable**: Provider-agnostic agent infrastructure

### Phase 6: Advanced Intelligence
- Thinking effort detection from request structure (system prompt length,
  tool count, conversation depth)
- Automatic thinking effort adjustment (reduce `max_tokens` for simple queries)
- Request deduplication (same prompt from multiple sub-agents → cache & share)
- Predictive rate limit avoidance (throttle preemptively before hitting limits)
- Historical pattern learning (this agent always peaks at 3pm → pre-allocate)
- **Deliverable**: Self-optimizing orchestration

---

## Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Proxy framework | `axum` | Rust-native, async, excellent middleware ecosystem |
| Token counting | Response `usage` field | Authoritative — comes from provider |
| Agent discovery | Layered (header → key → port) | Works with zero agent changes |
| Config format | Extend `config.toml` | Consistent with existing aitop |
| Storage | Extend existing SQLite | WAL mode proven, concurrent R/W |
| Proxy ↔ TUI IPC | Shared SQLite + file watcher | Already proven in aitop |
| Binary | Single binary, `--proxy` flag | Simpler distribution than two binaries |

---

## Business Viability

### Why This Works as a Premium Feature

1. **Natural upgrade path**: Free monitoring → paid orchestration. Users who
   already track their spend are primed to want to *control* it.

2. **Quantifiable ROI**: "Save 40-60% on AI API costs while running more
   agents simultaneously." Easy to measure, easy to justify.

3. **High switching cost**: Once workflows depend on the orchestrator for
   priority routing, switching means reconfiguring all agent infrastructure.

4. **Underserved niche**: No one does local-first, terminal-native,
   agent-priority-aware orchestration. LiteLLM/Portkey/Helicone are all
   cloud-first team tools.

5. **Compound moat**: Observability data makes routing smarter over time.
   The more you use it, the better it gets at predicting and allocating.

### Pricing Models

| Model | Price | Pros | Cons |
|-------|-------|------|------|
| **Open core** | Free monitoring, $29/mo orchestrator | Recurring revenue, natural funnel | Churn risk |
| **Lifetime license** | $199 one-time | Appeals to developers, viral potential | No recurring |
| **Usage-based** | Free to N requests/mo, then metered | Low friction entry | Complex billing |
| **Team tier** | $99/mo (shared budgets, agent pools) | Higher ARPU | Smaller market |

**Recommendation**: Open core + lifetime license option.
- Monitoring: free forever (grows the funnel)
- Orchestrator individual: $29/mo or $199 lifetime
- Team features: $99/mo (shared budgets, cross-machine agent pools, audit log)

### Risks

| Risk | Mitigation |
|------|-----------|
| Providers change rate limit structures | Abstract window logic, auto-learn from headers |
| Providers build their own orchestration | Focus on multi-provider + local-first (they won't do cross-provider) |
| API compatibility maintenance | Start with Anthropic only, add providers incrementally |
| Complexity for solo developers | Phase 1-2 are useful standalone (proxy + budget tracking) |
| Open source competitors | Speed + integration depth + Rust performance as moat |

### Target Users

1. **Power users**: Running multiple Claude Code instances daily ($100+/mo spend)
2. **AI-native dev teams**: 3-10 developers sharing org API limits
3. **Agentic framework builders**: Anyone building on Claude/Gemini who needs
   smart resource management
4. **Enterprise**: Teams with compliance needs (audit log, budget controls)

---

## Open Questions

1. **Agent discovery UX**: How do agents register? Pure auto-discovery from
   request patterns, or explicit opt-in via headers/config?

2. **Claude Code integration depth**: Can we hook into Claude Code's own
   session management, or is the proxy approach cleaner?

3. **Rate limit opacity**: Anthropic doesn't publish exact per-tier limits.
   How much can we learn from response headers vs. needing user input?

4. **Cross-machine orchestration**: Multiple machines sharing an org quota —
   worth solving in v1, or defer to team tier?

5. **Thinking effort classification**: How reliably can we infer task
   complexity from the request alone (system prompt, tools, message count)?

---

*This document is a living plan. Phases 1-2 are well-defined; phases 3+
will be refined based on learnings from the proxy MVP.*
