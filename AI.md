# Shipit — AI Agent Guide

This document describes how a coding agent can use shipit to drive a
**git flow lite** release process: feature branches land in `dev`, `dev`
promotes to `main`, and `main` is tagged for release.

---

## Workflow Overview

```
feature/my-feature  ──b2b──►  dev  ──b2b──►  main  ──b2t──►  v1.2.3
```

Each stage follows the same two-step **plan / apply** pattern:

1. **Plan** — collect commits, generate a description/title/notes, write a
   YAML file to `.shipit/plans/<hash>.yml`. Nothing is created on the platform yet.
2. **Apply** — read the plan file and execute: open a PR/MR, or create and push
   the annotated tag.

The plan file is the agent's opportunity to review, enrich, or rewrite any field
before anything is published.

---

## Setup

```bash
shipit init
```

This writes `shipit.toml` and creates `.shipit/plans/`.

Both `--platform-domain` and `--platform-token` are optional — shipit discovers
sensible defaults automatically:

- **Domain** — inferred from the `origin` remote URL (or the remote named by
  `--remote`). SSH (`git@github.com:…`) and HTTPS (`https://github.com/…`)
  formats are both recognised.
- **Token** — looked up from the environment based on the resolved domain:
  - GitHub: `GITHUB_TOKEN`, then `GH_TOKEN`
  - GitLab: `GITLAB_TOKEN`, then `GITLAB_PRIVATE_TOKEN`

When running `shipit init` non-interactively (e.g. in CI or as part of an
agent workflow), pass the flags explicitly to skip all prompts:

```bash
shipit init --platform-domain github.com --platform-token "$GITHUB_TOKEN"
```

---

## Stage 1 — Feature Branch → Dev

### 1a. Generate a plan

```bash
shipit b2b plan feature/my-feature dev
```

Shipit collects commits on `feature/my-feature` that are not yet on `dev`,
enriches them with PR/MR titles from the platform API, and writes a plan file.

**Conventional-commit structured description** (recommended when commits follow
the conventional-commit convention):

```bash
shipit b2b plan feature/my-feature dev --conventional-commits
```

The description will be grouped into sections (`## Features`, `## Bug Fixes`,
`## Infrastructure`, etc.).

**Agent-provided title and description** (skip commit collection entirely):

```bash
shipit b2b plan feature/my-feature dev \
  --title "feat: add payment integration" \
  --description "$(cat <<'EOF'
## Summary
- Adds Stripe checkout flow
- Introduces `PaymentService` with retry logic
- Updates API contract in `openapi.yml`
EOF
)"
```

When both `--title` and `--description` are supplied, no commits are collected
and the plan is written immediately.

### 1b. Apply the plan

```bash
shipit b2b apply <plan-filename>.yml
```

`<plan-filename>` is the filename (not a full path) of the file written to
`.shipit/plans/` in the previous step. Shipit opens the pull/merge request
and prints the URL.

**Capturing the plan for agent use** — pass `--yaml` to receive the full plan
on stdout. The output includes a `plan_file` field with the filename ready for
`apply`, and the `commits` list for extracting commit SHAs:

```bash
PLAN=$(shipit b2b plan feature/my-feature dev --yaml -y)
PLAN_FILE=$(echo "$PLAN" | yq '.plan_file')
shipit b2b apply "$PLAN_FILE"
```

---

## Stage 2 — Dev → Main

Same commands, different branches:

```bash
# Auto-generate from commits (conventional-commit format)
PLAN=$(shipit b2b plan dev main --conventional-commits -y --yaml)
PLAN_FILE=$(echo "$PLAN" | yq '.plan_file')

# Apply
shipit b2b apply "$PLAN_FILE"
```

The `-y` flag skips the interactive title prompt and accepts the suggested
`"Release Candidate vX.Y.Z"` title derived from the commit history.

---

## Stage 3 — Main → Tag

### 3a. Generate a tag plan

```bash
shipit b2t plan main
```

Shipit finds the most recent tag reachable from `main`, collects commits since
that tag, and suggests the next semantic version.

**Conventional-commit structured notes:**

```bash
shipit b2t plan main --conventional-commits -y
```

**Agent-provided tag name and notes:**

```bash
shipit b2t plan main v1.2.3 \
  --description "$(cat <<'EOF'
## What's Changed
- New payment integration (#42)
- Fixed session timeout bug (#38)
EOF
)"
```

### 3b. Apply the tag plan

```bash
shipit b2t apply <plan-filename>.yml
```

Creates an annotated local tag and pushes `refs/tags/<name>` to the remote.

**Capturing the tag plan for agent use:**

```bash
PLAN=$(shipit b2t plan main --conventional-commits -y --yaml)
PLAN_FILE=$(echo "$PLAN" | yq '.plan_file')
shipit b2t apply "$PLAN_FILE"
```

---

## Agent-Enriched Plans (Recommended Pattern)

> **Important for AI agents:** Always present the final plan to the user and
> wait for explicit approval before running `apply`. Opening a pull/merge
> request or pushing a tag is irreversible — the plan step exists precisely
> to give the user a review checkpoint. Never call `apply` autonomously.

The most powerful use of shipit for an agent is:

1. Run `b2b plan` or `b2t plan` with `--yaml` to collect commits, write the
   plan file, and receive the plan on stdout.
2. Use `yq` to extract the `plan_file` name (for the `apply` step) and the
   boundary commit SHAs (for `git diff`).
3. Run `git diff <first-sha>^..<last-sha>` to get the full diff for the range.
4. Summarise the diff and rerun `plan` with `--description` and/or `--title` to
   overwrite the auto-generated content with a human-quality summary.
5. Run `apply` on the enriched plan.

### Example: agent-enriched b2b plan

```bash
# Step 1 — write the initial plan and capture the YAML output
PLAN=$(shipit b2b plan feature/payments dev --conventional-commits -y --yaml --allow-dirty)

# Step 2 — extract the plan filename and boundary commit SHAs with yq
PLAN_FILE=$(echo "$PLAN" | yq '.plan_file')
LAST_SHA=$(echo "$PLAN"  | yq '.commits[0]'  | awk '{print $NF}')
FIRST_SHA=$(echo "$PLAN" | yq '.commits[-1]' | awk '{print $NF}')

# Step 3 — get the diff
DIFF=$(git diff "${FIRST_SHA}^".."${LAST_SHA}")

# Step 4 — ask the agent to summarise the diff, then rerun plan with the result
SUMMARY="<agent-generated summary goes here>"

PLAN=$(shipit b2b plan feature/payments dev \
  --title "feat(payments): Stripe checkout integration" \
  --description "$SUMMARY" \
  --yaml --yes --allow-dirty)
PLAN_FILE=$(echo "$PLAN" | yq '.plan_file')

# Step 5 — present the plan to the user for confirmation before applying
echo "$PLAN"

# Step 6 — apply only after the user approves
shipit b2b apply "$PLAN_FILE" --allow-dirty
```

### Commit ordering in the `commits` list

Commits are stored newest-first under the `commits:` key. Each entry is the
string `"<message> <sha>"` — the SHA is always the last whitespace-separated
token. Use index `0` for the newest commit and `-1` for the oldest:

```bash
PLAN=$(shipit b2b plan feature/payments dev --yaml -y)

LAST_SHA=$(echo "$PLAN"  | yq '.commits[0]'  | awk '{print $NF}')   # newest
FIRST_SHA=$(echo "$PLAN" | yq '.commits[-1]' | awk '{print $NF}')   # oldest

git diff "${FIRST_SHA}^".."${LAST_SHA}"
```

The diff can then be passed to the agent's language model to generate a
structured description before calling `plan` again with `--description`.

---

## Flag Reference

### `shipit init`

| Flag | Description |
|---|---|
| `--platform-domain <domain>` | Platform domain (default: inferred from remote URL) |
| `--platform-token <token>` | Platform personal access token (default: inferred from env — see below) |
| `--remote <name>` | Git remote to infer the domain from (default: `origin`) |
| `--dir <path>` | Directory to write config to (default: cwd) |

**Token environment variable lookup order:**

| Platform | Variables checked (in order) |
|---|---|
| GitHub | `GITHUB_TOKEN`, `GH_TOKEN` |
| GitLab | `GITLAB_TOKEN`, `GITLAB_PRIVATE_TOKEN` |

### `shipit b2b plan <source> <target>`

| Flag | Short | Description |
|---|---|---|
| `--conventional-commits` | `-c` | Group description by commit type |
| `--title <text>` | | Override the suggested PR/MR title |
| `--description <text>` | | Override the auto-generated description |
| `--only-merges` | | Restrict commit collection to merge commits |
| `--no-sign` | | Omit the "generated by Shipit" footer |
| `--yes` | `-y` | Accept all prompts non-interactively |
| `--yaml` | | Emit the plan as YAML to stdout (includes `plan_file` field) |
| `--allow-dirty` | | Continue even if the working directory has uncommitted changes |
| `--remote <name>` | | Git remote name (default: `origin`) |
| `--dir <path>` | | Repository root (default: cwd) |

### `shipit b2b apply <plan-file>`

| Flag | Description |
|---|---|
| `--allow-dirty` | Continue even if the working directory has uncommitted changes |
| `--remote <name>` | Git remote name (default: `origin`) |
| `--dir <path>` | Repository root (default: cwd) |

### `shipit b2t plan <branch> [tag]`

| Argument | Description |
|---|---|
| `[tag]` | Tag name to create (default: next semantic version derived from commits) |

| Flag | Short | Description |
|---|---|---|
| `--conventional-commits` | `-c` | Group notes by commit type |
| `--description <text>` | | Override the auto-generated tag notes |
| `--latest-tag <name>` | | Compare against a specific tag instead of auto-detecting |
| `--only-merges` | | Restrict commit collection to merge commits |
| `--no-sign` | | Omit the "generated by Shipit" footer |
| `--yes` | `-y` | Accept all prompts non-interactively |
| `--yaml` | | Emit the plan as YAML to stdout (includes `plan_file` field) |
| `--allow-dirty` | | Continue even if the working directory has uncommitted changes |
| `--remote <name>` | | Git remote name (default: `origin`) |
| `--dir <path>` | | Repository root (default: cwd) |

### `shipit b2t apply <plan-file>`

| Flag | Description |
|---|---|
| `--allow-dirty` | Continue even if the working directory has uncommitted changes |
| `--remote <name>` | Git remote name (default: `origin`) |
| `--dir <path>` | Repository root (default: cwd) |

---

## Plan File Format

Files written to `.shipit/plans/` look like this:

```yaml
# Shipit Plan - Generated by shipit v0.5.0 on 2024-06-01T12:00:00Z
shipit_version: 0.5.0
generated_at: "2024-06-01T12:00:00Z"
source: feature/payments
target: dev
title:
  value: "Release Candidate v1.2.0"
  generated_by: default        # "user" | "default" | "conventional-commits" | "raw"
description:
  value: |
    ## Features
    - feat: add Stripe checkout flow abc123
  generated_by: conventional-commits
commits:
  - "feat: add Stripe checkout flow abc123 a1b2c3d4"
  - "fix: handle webhook timeout def456 e5f6a7b8"
```

The `generated_by` field records what produced each value so downstream
tooling (and the agent) can decide whether to trust it or regenerate it.

When `--yaml` is passed, the stdout output adds a `plan_file` field not
present in the written file:

```yaml
plan_file: 3f9a1c2e4d7b0e5f.yml
```

Use this field to drive the `apply` step without filesystem globbing.

---

## Complete Git Flow Lite Example

```bash
# ── Feature → Dev ──────────────────────────────────────────────────────────
PLAN=$(shipit b2b plan feature/payments dev --conventional-commits -y --yaml --allow-dirty)
shipit b2b apply "$(echo "$PLAN" | yq '.plan_file')" --allow-dirty

# ── Dev → Main ─────────────────────────────────────────────────────────────
PLAN=$(shipit b2b plan dev main --conventional-commits -y --yaml --allow-dirty)
shipit b2b apply "$(echo "$PLAN" | yq '.plan_file')" --allow-dirty

# ── Main → Tag ─────────────────────────────────────────────────────────────
PLAN=$(shipit b2t plan main --conventional-commits -y --yaml --allow-dirty)
shipit b2t apply "$(echo "$PLAN" | yq '.plan_file')" --allow-dirty
```
