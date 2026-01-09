## Claude Code GitHub Action Setup

Use this guide to enable Claude Code on the Nexus Rust CLI repository.

## Prerequisites

- Repository admin access (needed to add secrets and install the GitHub App).
- An Anthropic API key from `console.anthropic.com` or a Claude Max subscription.

## Quick Setup (GitHub App)

1) Open the Claude Code terminal in this repository.
2) Run the install command:

```bash
/install-github-app
```

3) Follow the interactive prompts to authorize the GitHub App.
4) Verify workflow files appear in `.github/workflows/`.

## Manual Setup (API Key Secret)

1) In GitHub, go to `Settings > Secrets and variables > Actions`.
2) Add a new repository secret named `ANTHROPIC_API_KEY` with your API key value.
3) Verify workflow files exist in `.github/workflows/`.

## Usage Guide

Claude runs when you mention `@claude` in a PR or issue comment. Use concise, specific requests.

```text
@claude fix the null pointer dereference in src/executor/client.rs line 42
```

```text
@claude refactor this function to use proper error handling with thiserror
```

```text
@claude make this more idiomatic Rust using iterators
```

```text
@claude what does the Executor trait do and how is it implemented?
```

```text
@claude optimize this parsing loop to avoid allocations
```

Review: Automatic on PR open.

## Troubleshooting

- Workflow not triggering: check repo permissions and ensure workflow files exist in `.github/workflows/`.
- Claude not responding: confirm the `ANTHROPIC_API_KEY` secret is set correctly.
- Rate limiting: workflows include exponential backoff; wait and retry.
- Wrong branch: ensure the PR targets the correct base branch.
