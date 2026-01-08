# Nexus Architecture
_Status: draft (v0.1)_

This document is the **implementation guide** for Nexus: a **single-screen conversational coding terminal** that feels like one assistant, while transparently orchestrating multiple specialist agents (research, planning, execution) behind the scenes—under a Claude‑Code‑style **permission system**.

Nexus’s core bet is that **software-building agents succeed when the “control plane” is deterministic** (policies, tools, approvals, logs) and the “data plane” is probabilistic (LLM reasoning).

---

## 0) Vision and non-negotiables

### 0.1 Vision (what users experience)
- One conversational REPL.
- User types normal language (“please implement X”), optionally with inline delegation (“have Codex analyze…, send to Claude…”).
- Nexus **automatically delegates** to the right specialist(s) and stitches the work together.
- User stays in control via **approval prompts** before any meaningful boundary crossing:
  - executing commands
  - writing files (patches)
  - transferring control between agents (handoffs)
  - starting autopilot batching
- Nexus keeps long projects coherent via a **doc graph** (Agenda → Phases → Plans → Runs), optimized for agent interpretability and token economy.

### 0.2 Non-negotiables (system invariants)
1. **Tools are never executed by agents.**  
   Agents only emit *Proposed Actions*. The Tool Gateway executes actions **only** after policy approval.
2. **Everything side-effectful is auditable.**  
   “What happened?” must be answerable by replaying an append-only event log.
3. **Docs are the source of truth; chat is the control plane.**  
   The project’s intent and constraints live in structured Markdown nodes and plan artifacts.
4. **Bounded context per step.**  
   Nexus must not rely on a growing chat context to “remember” state.
5. **Resume is deterministic.**  
   Long-running workflows resume from event history + definition hash, not “best effort.”

---

## 1) High-level architecture

### 1.1 Conceptual diagram

```
┌───────────────────────────────────────────────────────────────────────┐
│                               Nexus CLI                               │
│  (single-screen REPL + cards + status line + trace toggle)             │
└───────────────┬───────────────────────────────┬───────────────────────┘
                │                               │
                │ user prompts                   │ approvals / policy edits
                ▼                               ▼
┌───────────────────────────┐         ┌───────────────────────────┐
│  Router (small local model)│         │ Permission Gate (determin.)│
│  - parses intent           │         │ - Allow/Ask/Deny rules      │
│  - proposes workflow graph │         │ - issues approval tokens    │
└───────────────┬───────────┘         └───────────────┬───────────┘
                │ workflow definition                  │ tokens
                ▼                                      ▼
┌───────────────────────────────────────────────────────────────────────┐
│                       Orchestration Engine (determin.)                │
│  - executes workflow nodes (research/plan/exec)                        │
│  - builds context packs                                                │
│  - collects artifacts & updates docs                                   │
│  - writes run events + supports replay/resume                           │
└───────────────┬───────────────────────────────────────────────────────┘
                │ ProposedActions (patch/command/handoff) + artifacts
                ▼
┌───────────────────────────────────────────────────────────────────────┐
│                          Tool Gateway (determin.)                      │
│  - repo.read/search/list                                               │
│  - repo.apply_patch                                                    │
│  - cmd.run / tests.run                                                 │
│  - sandbox (worktree/container)                                        │
│  - redaction + denylist                                                │
└───────────────────────────────────────────────────────────────────────┘

Specialist Agents (probabilistic, replaceable via adapters):
  - Research agent (Gemini or equivalent)
  - Planner agent (Claude or equivalent)
  - Executor agent (Codex / GPT coding model or equivalent)
```

### 1.2 Core idea: “one assistant, three specialists”
Nexus should **feel** like a single assistant because:
- The CLI presents *one conversation stream* and *one cohesive narrative*.
- Specialist outputs are stored as **artifacts** and summarized by Nexus into the stream.
- A `/trace on` view optionally reveals the delegation graph and raw model outputs.

---

## 2) Repository layout and “doc graph”

Nexus is anchored to a predictable on-disk structure. This makes it portable, debuggable, and friendly to git.

### 2.1 Recommended layout

```
.
├─ AGENDA.md
├─ docs/
│  ├─ phases/
│  │  ├─ core_loop.md
│  │  ├─ doc_graph.md
│  │  └─ providers.md
│  └─ architecture.md                <-- this file
├─ .nexus/
│  ├─ settings.json                  # user + repo settings (merged)
│  ├─ policy.md                      # permission policy and denylist
│  ├─ schemas/
│  │  ├─ router.workflow.schema.json
│  │  ├─ plan.schema.json
│  │  ├─ research.schema.json
│  │  ├─ exec.result.schema.json
│  │  └─ proposed_action.schema.json
│  ├─ baml/                          # BAML prompt library
│  │  ├─ router.baml
│  │  ├─ research.baml
│  │  ├─ plan.baml
│  │  ├─ exec.baml
│  │  └─ agenda_patch.baml
│  ├─ plans/
│  ├─ research/
│  ├─ runs/
│  ├─ cache/
│  └─ commands/
│     ├─ builtins/
│     └─ custom/
```

### 2.2 Doc graph principles
- **AGENDA.md stays tiny.** It is an index + invariant constraints + pointers.
- **Phases are separate nodes.** Each phase links to the plan(s) relevant when active.
- **Plans are machine-first JSON** (schemas enforced). Markdown is for memory and linking.
- **Artifacts are referenced, not inlined.** Messages should carry pointers, not blobs.

### 2.3 Strict Markdown structure (agent-parseable)
All “Nexus canonical” Markdown docs should follow these rules:
- A single top-level header
- Short labeled sections with fixed headings
- Minimal prose; prefer bullet lists
- Stable identifiers in YAML frontmatter when helpful

Example pattern:

```md
---
ID: phase.core_loop
STATUS: active
LINKS:
  - plan: .nexus/plans/plan-core_loop-v1.json
  - research: .nexus/research/research-batching.md
---

GOAL:
- ...

DELIVERABLES:
- ...

CONSTRAINTS:
- ...

OPEN_QUESTIONS:
- ...
```

---

## 3) Key data models

### 3.1 ProposedAction (the universal boundary object)
Everything the system *might do* is represented as a ProposedAction.

Kinds (minimum set):
- `handoff` (control transfer between agent ops)
- `patch` (repo.apply_patch)
- `command` (cmd.run / tests.run)
- `plan_patch` (mutate a plan definition)
- `agenda_patch` (mutate AGENDA/phase docs)

Example:

```json
{
  "id": "act_123",
  "kind": "patch",
  "summary": "Refactor frontend bundle splitting",
  "details": {
    "diff": "diff --git ..."
  },
  "risk": 2,
  "policy_tags": ["writes_repo"],
  "requires_approval": true
}
```

**Architectural reason:** This ensures a single, consistent permission UX and audit story across providers and feature sets.

### 3.2 ApprovalToken (unforgeable permission binding)
An approval token is produced by the Permission Gate and consumed by the Tool Gateway.

Token must bind to:
- the **canonical tool request object** (not an ambiguous string)
- the **normalized diff** for patches
- the **argv array** for commands (never run shell strings)
- relevant scope fields (cwd, env allowlist, timeouts)

Design requirement: **If a token approved X, the system cannot execute Y.**

### 3.3 WorkflowDefinition (first-class orchestration graph)
A workflow is what the router proposes and what the engine executes.

Minimal form (sequence) is enough for v0; DAG is for later.

```json
{
  "workflow_id": "wf_2026_01_08_001",
  "definition_hash": "sha256:...",
  "nodes": [
    {"id":"N1", "agent":"executor", "op":"ANALYZE", "inputs": {...}},
    {"id":"N2", "agent":"planner", "op":"PLAN_PATCH", "inputs": {"uses":["N1"]}},
    {"id":"N3", "agent":"executor", "op":"EXECUTE_PLAN", "inputs": {"uses":["N2"]}}
  ],
  "edges": [["N1","N2"],["N2","N3"]],
  "budgets": {"max_batch_cu": 40, "max_ctx_kib": 160}
}
```

**Architectural reason:** WorkflowDefinition is the glue that makes “one mode, auto transfer” debuggable, resumable, and user-controllable.

### 3.4 WorkflowRun and Event Log (append-only, replayable)
A WorkflowRun is derived from the workflow definition and an append-only event log:

- `.nexus/runs/<run_id>/events.jsonl`
- `.nexus/runs/<run_id>/artifacts/` (logs, diffs, reports)

Events include:
- `workflow.started`
- `node.started / node.completed / node.blocked`
- `action.proposed / permission.granted / tool.executed`
- `artifact.written`
- `policy.changed`
- `run.checkpointed`

**Architectural reason:** Replay gives deterministic resume without depending on model memory.

---

## 4) Control flow: one-mode delegation and seamless handoffs

### 4.1 Standard turn loop

1. User enters a prompt.
2. Router produces a `WorkflowDefinition` (and confidence).
3. Nexus renders a **Workflow Proposal card** (unless policy allows auto-start).
4. If approved, the Orchestration Engine executes nodes in order:
   - calls the appropriate agent adapter
   - validates output schemas
   - writes artifacts
   - collects ProposedActions
5. Each ProposedAction is evaluated by the Permission Gate:
   - auto-allow
   - prompt user
   - deny
6. Approved actions are executed by the Tool Gateway.
7. Results are written as artifacts and summarized to the user.
8. The workflow continues until completion or a stop condition.

### 4.2 Blocked execution: the “seamless transfer” loop
When the Executor cannot proceed, it must emit a structured `BLOCKED` result:
- `kind`: `needs_research` | `needs_plan_patch` | `needs_user_input`
- `question`: the precise missing info
- `suggested_ctx`: what files/docs might help

Nexus then proposes a subflow:

`executor(BLOCKED) → research(agent) → planner(plan_patch) → executor(resume)`

**Key UX rule:** The user is prompted before the handoff happens, and prompted again before applying plan/code changes, unless policy/autopilot says otherwise.

### 4.3 “One voice” behavior
Nexus should present:
- concise narrative of what is happening
- expandable “cards” for:
  - workflow proposal
  - handoffs
  - diffs/patches
  - commands
  - plan patches

Optional:
- `/trace on` shows raw agent messages and node-level timing.

---

## 5) Router design (local small model + deterministic fallback)

### 5.1 Router responsibilities (keep it narrow)
The router model should:
- detect explicit delegation cues in user text
- classify intent (research vs plan vs exec)
- propose a workflow macro or graph
- emit confidence + required approvals

It should NOT:
- write code
- generate implementation plans
- decide tool parameters in detail

### 5.2 Router inputs
- user prompt
- current repo policy mode
- pointers to AGENDA + active phase
- recent run summaries (small)
- optional: “project vocabulary” (names of systems/modules)

### 5.3 Router outputs (must be schema-valid)
- WorkflowDefinition (or 2–3 alternatives when ambiguous)
- confidence score
- short explanation for the user-facing card

### 5.4 Deterministic fallback rules
If the router fails schema validation or is unavailable:
- Use keyword heuristics:
  - “latest”, “best practice”, “compare”, “verify” → include research
  - “plan”, “architecture”, “milestone”, “steps” → include planning
  - “implement”, “refactor”, “fix”, “tests”, “diff” → include execution
- Default workflow macro: `research → plan → exec` (safe baseline)

---

## 6) Specialist agent roles (provider-agnostic)

Nexus’s pipeline assumes three roles. Providers/models can be swapped via adapters.

### 6.1 Researcher (high recall, evidence)
Typical backend: Gemini or equivalent.

Outputs:
- `research.json` (claims list + citations + unresolved questions)
- `research.md` (human-readable summary)
- “constraints” extracted for the planner

Tools:
- ideally none; if using web search tooling, treat it as an internal capability of the research adapter and store citations.

### 6.2 Planner (decomposition + step budgets)
Typical backend: Claude or equivalent.

Outputs:
- `plan.json` (executable steps with context selectors + budgets)
- optional `plan_patch.json` (when modifying an existing plan)
- a list of “open questions” to surface to the user

Planner should optimize for:
- atomic steps
- minimal context selectors
- explicit validation commands
- risk tagging

### 6.3 Executor (patches + verification)
Typical backend: Codex / GPT coding model or equivalent.

Outputs:
- ProposedActions: patch + commands
- exec summaries
- BLOCKED events when missing info

Executor must be guided by:
- plan step boundaries
- budgets (max files, max diff, max context)
- “do not improvise plan changes” rule (request plan patch instead)

---

## 7) BAML integration (prompt library as typed functions)

### 7.1 Why BAML
BAML gives Nexus:
- “prompt functions” with typed outputs
- centralized prompt versioning
- consistent schema validation and retry hooks
- clearer separation of concerns between roles

### 7.2 Recommended BAML functions (v0)
- `RoutePrompt(prompt, project_state) -> WorkflowDefinition`
- `Research(question, context_refs) -> ResearchBundle`
- `MakePlan(agenda_refs, research_refs, repo_refs) -> Plan`
- `PlanPatch(plan_ref, new_constraints, blocker) -> PlanPatch`
- `ExecStep(plan_ref, step_id, context_pack) -> ExecResult`
- `AgendaPatch(run_summary) -> Patch`

### 7.3 BAML inside plans (context pointers)
Each plan step should include:
- `ctx.docs` (doc references + section anchors)
- `ctx.paths` (repo paths)
- `ctx.grep` terms
- `ctx.commands`
- `baml.fn` pointer (e.g. `ExecStep`) with inputs

This is how plans “tell agents where to look,” while keeping token cost bounded.

---

## 8) Permission Gate & policy system (Claude Code-like)

### 8.1 Policy model
Nexus implements a 3-way decision for any ProposedAction:
- `allow`
- `ask`
- `deny`

Rules should be composable and ordered by precedence:
- deny overrides everything
- ask overrides allow
- allow is the default for matching actions

### 8.2 Permission scopes
At each approval prompt, offer a scope ladder:
- once
- for this session
- for this repo
- for this repo + this path (writes)
- for this repo + this argv signature (commands)
- for this workflow instance (handoffs)

### 8.3 Permission modes (fast toggles)
- `default`: ask for writes/exec/handoffs
- `acceptEdits`: auto-approve safe patches under allowlist
- `autopilot`: auto-approve a configured set of actions for batching
- `bypass`: dangerous; disable by default; require explicit config to enable

### 8.4 Canonicalization rules (critical)
- Commands are executed as `argv[]`, not shell strings.
- Patches are applied as normalized unified diffs.
- Hashes are computed from canonical request objects.
- Patch application failures require re-approval (no fuzzy apply under old token).

---

## 9) Tool Gateway (deterministic side effects)

### 9.1 Tool set (minimum viable)
Read:
- `repo.list_tree`
- `repo.search`
- `repo.read_file` (bounded)
Write:
- `repo.apply_patch` (unified diff)
Exec:
- `cmd.run` (argv)
- `tests.run` (wrapper w/ structured output)

Artifacts:
- `artifact.write` (store logs/reports/diffs)
- `git.status`, `git.diff` (optional, but extremely useful)

### 9.2 Sandbox model
Recommended:
- per-run git worktree or branch
- optional containerization for command execution
- env var allowlist
- denylist sensitive paths (`.env*`, `~/.ssh`, cloud creds, etc.)

### 9.3 Redaction and leakage control
All tool outputs that may enter prompts or artifacts should be filtered for:
- obvious secrets patterns
- config tokens
- private keys

The default posture should be “better safe than sorry,” with an explicit override if needed.

---

## 10) Context Packs (bounded, reproducible prompt context)

Nexus’s Context Builder produces a ContextPack per node/step:

```json
{
  "id": "ctx_456",
  "refs": [
    {"type":"doc", "path":"AGENDA.md", "slice":"GLOBAL_CONSTRAINTS"},
    {"type":"file", "path":"src/ui/app.tsx", "bytes": 12000},
    {"type":"grep", "query":"bundle analyzer", "results_ref":"artifact://..."}
  ],
  "budgets": {"max_ctx_kib": 160},
  "hash": "sha256:..."
}
```

Rules:
- Only include what the plan step says is relevant.
- Prefer slicing by headings/regions.
- Prefer grep results and short excerpts to full files.
- Cache packs by hash; reuse when possible.

---

## 11) Batching and complexity budgets

### 11.1 Two budgets
- **Context budget**: bytes/tokens of prompt context
- **Complexity budget**: expected risk/size of the step

### 11.2 Complexity Units (CU) starter metric
Nexus computes CU from plan metadata + repo structure:

- `F` = expected files touched
- `D` = distinct directories/packages touched
- `C` = commands count
- `R` = risk class (0–3)
- `U` = unresolved research dependencies (0+)

Example:
- `CU = 2F + 3D + C + 5R + 4U`

### 11.3 Batch packing algorithm (sequential)
Pack steps until adding the next step would exceed:
- `MAX_BATCH_CU`
- `MAX_BATCH_STEPS`
- `MAX_TOTAL_CTX_KIB` (optional)
- risk threshold

### 11.4 Step splitting protocol
If a plan step exceeds budgets:
- Nexus requests a **PlanPatch** from the Planner to split it into smaller steps.
- Execution does not proceed until the patch is approved (or policy auto-allows).

---

## 12) Observability, replay, and evaluation

### 12.1 Event log is the system of record
Every action should have an event trail:
- who proposed it
- why it was proposed
- what was approved
- what executed
- outputs and artifact refs

### 12.2 Replay tooling
Nexus should support:
- `nexus replay <run_id>`: reconstruct state and show the timeline
- `nexus resume <run_id>`: continue from the first incomplete node
- `nexus diff <run_id>`: show code deltas

### 12.3 Quality metrics (pragmatic)
Track per agent/adapter:
- schema compliance rate
- patch apply success rate
- tests-pass-after-change rate
- average retries per step
- average CU per successful batch

Use these metrics to improve routing and policies empirically.

---

## 13) Extensibility roadmap (don’t overbuild early)

### 13.1 v0: single-process, file-backed state
- No message bus.
- Router + engine + gateway all in one process.
- Event logs + artifacts stored on disk.
- One provider per role.

Goal: ship the end-to-end loop and UX.

### 13.2 v1: better doc graph + workflow macros
- robust doc slicing and link semantics
- workflow “macros” and editable workflow cards
- batch autopilot controls
- agenda/phase sync patches

### 13.3 v2: multi-provider adapters + caching
- multiple agents per role
- capability hints + empirical routing
- research caching and “grounding bundles”

### 13.4 future: generalized platform
- tool packs
- remote workers
- optional message bus
- multi-repo / mono-repo support

---

## 14) Open questions (intentionally left for ongoing design)
- How to best standardize doc anchors (heading-based vs explicit IDs).
- How to calibrate CU weights with real run telemetry.
- How to implement safe “remember” policies that reduce fatigue without trapping the user.
- What default sandbox model is acceptable for your target environments (local dev vs CI).
- How to merge multiple plan patches cleanly when multiple blockers arise in one batch.

---

## 15) Implementation checklist (first sprint)
- [ ] Define JSON Schemas: ProposedAction, WorkflowDefinition, Plan, ResearchBundle, ExecResult
- [ ] Build the CLI skeleton (REPL, status line, cards)
- [ ] Implement on-disk stores: settings, runs, artifacts, plans, research
- [ ] Implement Permission Gate (Allow/Ask/Deny + scope ladder)
- [ ] Implement Tool Gateway: apply_patch, cmd.run (argv), tests.run, repo.read/search
- [ ] Implement Router adapter (local model + deterministic fallback)
- [ ] Implement Planner adapter (plan JSON + step budgets)
- [ ] Implement Executor adapter (patch proposals + BLOCKED protocol)
- [ ] Implement replay/resume from events.jsonl
