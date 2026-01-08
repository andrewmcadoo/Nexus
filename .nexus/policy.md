# Nexus Policy (permission + safety)
_Status: draft (v0.1)_

This file defines **how Nexus asks for approval** before doing anything that can affect your repo, machine, network, or secrets.

Nexus is designed around one invariant:

> **Agents only propose actions.**  
> Only the **Tool Gateway** can execute actions, and only after the **Permission Gate** allows them.

This policy is meant to be:
- **Simple enough** to understand at a glance.
- **Strict enough** to be trustworthy.
- **Configurable enough** to avoid “approval fatigue.”

---

## 1) Vocabulary

### 1.1 ProposedAction kinds
Nexus reduces every meaningful operation to a `ProposedAction`:

- `patch` — apply a unified diff to the repo
- `command` — run a local command (argv-based, no shell strings)
- `handoff` — transfer control to another specialist agent (researcher/planner/executor)
- `plan_patch` — modify/replace a plan definition
- `agenda_patch` — update AGENDA/phase docs (docs are the project brain)

### 1.2 Policy decision outcomes
For each proposed action, the Permission Gate decides:

- **allow** — execute immediately
- **ask** — show a prompt card to the user
- **deny** — block execution

Precedence:
1) `deny` wins
2) `ask` overrides `allow`
3) default is `ask` for risky actions

### 1.3 Permission modes (fast toggles)
Nexus uses a global “mode” to reduce repetitive prompts:

- **default**: ask for writes/commands/handoffs; allow bounded reads
- **acceptEdits**: auto-allow *safe* patches within allowlisted paths; still ask for commands
- **autopilot**: auto-allow a configured set of actions for batching (still respects deny rules)
- **bypass**: dangerous; everything allowed (recommended disabled by default)

---

## 2) Safe defaults (recommended starting posture)

### 2.1 Reads (generally safe, still bounded)
Default: **allow** bounded reads of repo content needed for planning/execution.

Always enforce:
- max bytes per file
- max total bytes per step
- denylisted path globs (see below)

### 2.2 Writes (patches)
Default: **ask** for all patches.

`acceptEdits` may allow patches **only** if:
- all touched paths match `ALLOW_WRITE_PATHS`
- diff size is under `MAX_DIFF_LINES`
- risk is not elevated (optional)

### 2.3 Commands
Default: **ask** for all commands.

Recommended:
- allowlist only `tests`, `lint`, `typecheck` commands early
- treat package installs and networked commands as high-risk

### 2.4 Network & secrets
Default: **deny** unless explicitly enabled for a session.

---

## 3) Denylist (paths, commands, and data)

### 3.1 Denylisted paths (never read/write by default)
Suggested initial denylist:

- `.env*`
- `**/.ssh/**`
- `**/.aws/**`
- `**/.npmrc`
- `**/.pypirc`
- `**/*secret*`
- `**/*credential*`
- `**/*private_key*`

### 3.2 Denylisted command patterns
Nexus should never auto-run these without explicit, one-time approval:

- `rm`, `del`, `rmdir`
- `sudo`
- `chmod`, `chown`
- `curl`, `wget` (unless tightly constrained)
- `sh -c ...` (shell strings are disallowed; use argv)

---

## 4) Allow/Ask/Deny rules (machine-readable guidelines)

Nexus’s policy engine should match actions using:
- action kind (`patch`, `command`, `handoff`, ...)
- touched paths (for patches)
- argv signature (for commands)
- risk level
- policy tags

### 4.1 Example rule table (human-readable)

| Priority | Rule |
|---:|---|
| 1 | **deny** any read/write under denylisted paths |
| 2 | **deny** commands with `sudo` |
| 3 | **ask** all commands not explicitly allowlisted |
| 4 | **ask** all patches by default |
| 5 | **allow** bounded reads required by the current step |
| 6 | **allow** plan-only operations (no side effects) |

### 4.2 Example config block (copy into .nexus/settings.json)
```json
{
  "permission_mode": "default",
  "deny_paths": [".env*", "**/.ssh/**", "**/.aws/**", "**/.npmrc", "**/.pypirc"],
  "allow_paths_write": ["src/**", "docs/**", ".nexus/**"],
  "allow_commands": [
    ["pnpm", "test"],
    ["pnpm", "lint"],
    ["pnpm", "typecheck"]
  ],
  "deny_commands": [
    ["sudo"],
    ["rm"]
  ],
  "autopilot": {
    "max_batch_cu": 40,
    "max_batch_steps": 8,
    "auto_approve_patches": false,
    "auto_approve_tests": false,
    "auto_handoffs": false
  }
}
```

---

## 5) Approval prompts and “remember” scopes

When Nexus asks, it should offer a scope ladder (like Claude Code):

1) **Once**
2) **This session**
3) **This repo**
4) **This repo + this path** (for patches)
5) **This repo + this argv** (for commands)
6) **This workflow instance** (for handoffs)

Guideline:
- default “remember” for patches should be session-scoped
- default “remember” for test commands can be repo-scoped if argv is stable

---

## 6) Autopilot semantics (how to stay safe while batching)

Autopilot is NOT “agents execute tools.”
Autopilot is: the Permission Gate automatically grants approvals that match your configured rules.

Recommended autopilot configuration progression:

- Start with: autopilot allows **handoffs + tests only**
- Then allow: low-risk patches under `src/**` with a strict diff-size budget
- Only later consider: networked commands

Hard stops that should break autopilot:
- tests fail
- executor returns BLOCKED
- patch touches non-allowlisted paths
- risk class ≥ 2 (configurable)

---

## 7) Redaction rules (prevent accidental leakage)

Nexus should redact secrets from:
- command stdout/stderr logs before storing artifacts
- context packs before sending to models
- summaries shown to the user

Suggested redaction patterns:
- strings that match common token formats (API keys)
- PEM blocks
- private key headers
- long base64-like blobs

---

## 8) Policy as a product surface
This file is intentionally readable and editable. Nexus should also expose:

- `/policy show`
- `/policy mode default|acceptEdits|autopilot`
- `/policy allow path src/**`
- `/policy allow cmd ["pnpm","test"]`
- `/policy deny path .env*`
- `/policy reset`

Every policy change should be written to the run event log as `policy.changed`.

