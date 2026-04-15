---
name: shipit
description: Drive git flow lite releases with the shipit CLI: promote feature branches to dev, dev to main, and create annotated version tags. Invoke before any release, branch promotion, PR/MR creation, or version tagging workflow.
disable-model-invocation: true
allowed-tools: Bash(shipit *), Bash(git *), Bash(yq *)
---

# Shipit — AI Agent Guide

This document describes how a coding agent can use shipit to drive a
**git flow lite** release process: feature branches land in `dev`, `dev`
promotes to `main`, and `main` is tagged for release.

---

## Workflow Overview

```
feature/my-feature  ──b2b──►  dev  ──b2b──►  main  ──b2t──►  v1.2.3
```

**When a user asks you to use shipit to do something** (e.g. "use shipit to
release X"), begin your response by telling them: "I'll show you a plan before
applying anything — nothing will be created on the platform until you approve."

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

| Flag | Short | Description |
|---|---|---|
| `--platform-domain <domain>` | | Platform domain (default: inferred from remote URL) |
| `--platform-token <token>` | | Platform personal access token (default: inferred from env — see below) |
| `--remote <name>` | | Git remote to infer the domain from (default: `origin`) |
| `--dir <path>` | | Directory to write config to (default: cwd) |
| `--yes` | `-y` | Accept all prompts using inferred or discovered defaults |

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

---

## Multi-Project Release Workflow

This section describes how an agent can coordinate releases across **multiple
repositories** in a defined order. Each project has its own environment
pipeline (the ordered sequence of branch-to-branch promotions and/or
branch-to-tag operations). A shared config file persists these settings
across workflow runs.

---

### Config File

The multi-project config lives at `.shipit/multi-release.yml` relative to the
directory where the agent is invoked (typically a workspace or monorepo root,
but it can also be any convenient location).

**Format:**

```yaml
# .shipit/multi-release.yml
# Projects are released in the order they appear in this list.
projects:
  - name: api-service
    dir: /absolute/path/to/api-service   # directory that contains shipit.toml
    pipeline:
      - type: b2b        # branch-to-branch PR/MR
        source: dev
        target: qa
      - type: b2b
        source: qa
        target: main
      - type: b2t        # branch-to-tag
        source: main

  - name: frontend
    dir: /absolute/path/to/frontend
    pipeline:
      - type: b2b
        source: dev
        target: staging
      - type: b2b
        source: staging
        target: main
      - type: b2t
        source: main

  - name: infra
    dir: /absolute/path/to/infra
    pipeline:
      - type: b2b
        source: dev
        target: main
      - type: b2t
        source: main
```

**Field reference:**

| Field | Description |
|---|---|
| `name` | Human-readable project label used in agent prompts |
| `dir` | Absolute path to the project root (must contain `shipit.toml`) |
| `pipeline` | Ordered list of release steps for this project |
| `pipeline[].type` | `b2b` (branch-to-branch) or `b2t` (branch-to-tag) |
| `pipeline[].source` | Source branch |
| `pipeline[].target` | Target branch (`b2b` only) |

---

### Agent Decision Tree

Every time the multi-project workflow is triggered, the agent follows this
decision tree before executing any release steps.

```
START
  │
  ▼
Ask the user:
  "Do you have an existing multi-release config file (multi-release.yml),
   or would you like me to generate one?
   If you already have one, please provide the path to it."
  │
  ├─ HAS EXISTING FILE (user provides path)
  │       │
  │       ▼
  │     Read the file at the provided path and continue to [Review Config]
  │
  └─ GENERATE NEW (or no path provided)
         │
         ▼
       Does .shipit/multi-release.yml exist in the current directory?
         │
         ├─ YES ──► Read it and continue to [Review Config]
         │
         └─ NO ──► [First-Run Setup] ──► write config ──► continue

[Review Config]
  │
  ▼
Ask the user:
  "I found the following release config:
     1. api-service  (dev → qa → main, tag)
     2. frontend     (dev → staging → main, tag)
     3. infra        (dev → main, tag)
   Do you want to run the full release for all projects, or is this
   an atypical run (e.g. a subset of projects, or fewer environment
   steps)?"
  │
  ├─ FULL ──► use config as-is ──► execute
  │
  └─ ATYPICAL
         │
         ▼
       Collect overrides from user:
         - Which projects? (subset / reordering)
         - For each project, which pipeline steps? (subset / reordering)
       Do NOT write changes back to config.
         │
         ▼
       Execute with overrides only
```

---

### First-Run Setup

When `.shipit/multi-release.yml` does not exist, the agent must collect
configuration from the user interactively before proceeding.

**Prompts to ask (in order):**

1. "How many projects do you want to include in the multi-project release
   workflow? Please list them in release order."
2. For each project in order:
   - "What is the name of this project?"
   - "What is the absolute path to this project's directory?"
   - "What is the environment pipeline for this project?  
     List the steps in order, e.g.:  
     `dev → qa` (b2b), `qa → main` (b2b), `main → tag` (b2t)"
3. Show the agent's interpretation of the collected config in YAML form and
   ask the user to confirm before writing the file.

Once confirmed, write `.shipit/multi-release.yml` and proceed with the
release using the new config.

---

### Executing the Workflow

**Before iterating over pipeline steps**, verify that every project in the
effective run has been initialised with `shipit init` by checking for a
`shipit.toml` file in its `dir`:

```bash
# For each project
test -f <project-dir>/shipit.toml || echo "MISSING"
```

If any project is missing `shipit.toml`, run `shipit init` for it before
continuing:

```bash
shipit init --dir <project-dir> -y
```

The `-y` flag accepts all inferred defaults (domain from the remote URL, token
from the environment) non-interactively. If no defaults can be inferred and
`-y` is set, shipit skips that field — in that case, pass the values explicitly:

```bash
shipit init --dir <project-dir> --platform-domain github.com --platform-token "$GITHUB_TOKEN" -y
```

Do not proceed with any pipeline steps until all projects report a valid
`shipit.toml`.

---

For each project (in config order, or user-specified order for atypical runs),
for each pipeline step (in config order, or user-specified subset):

1. `cd` into the project's `dir`.
2. Check out the source branch for the current step:
   ```bash
   git -C <project-dir> checkout <source>
   ```
3. If the step is `b2b`:
   ```bash
   PLAN=$(shipit b2b plan <source> <target> \
     --conventional-commits -y --yaml --allow-dirty \
     --dir <project-dir>)
   PLAN_FILE=$(echo "$PLAN" | yq '.plan_file')
   ```
   If the step is `b2t`:
   ```bash
   PLAN=$(shipit b2t plan <source> \
     --conventional-commits -y --yaml --allow-dirty \
     --dir <project-dir>)
   PLAN_FILE=$(echo "$PLAN" | yq '.plan_file')
   ```
4. Record the plan outcome for the summary table (see step 9). A step has:
   - **No changes** — the plan's `commits` list is empty.
   - **Success** — the plan was generated without error and has commits.
   - **Failed** — the plan command exited with an error.
5. Present the plan to the user and **wait for explicit approval** before
   calling `apply`. This is mandatory — see the warning in the
   [Agent-Enriched Plans](#agent-enriched-plans-recommended-pattern) section.
   Every plan summary presented to the user **must** include:
   - **Plan ID** — the `plan_file` value from the YAML output (e.g. `3f9a1c2e4d7b0e5f.yml`)
   - **Plan path** — the full path to the written plan file (e.g. `<project-dir>/.shipit/plans/3f9a1c2e4d7b0e5f.yml`)
6. On approval:
   ```bash
   shipit b2b apply "$PLAN_FILE" --allow-dirty --dir <project-dir>
   # or
   shipit b2t apply "$PLAN_FILE" --allow-dirty --dir <project-dir>
   ```
7. After **all** pipeline steps across **all** projects have been planned
   (regardless of how many were applied), output a Markdown summary table:

   | Project | Step | Plan ID | Result | Title / Tag |
   |---|---|---|---|---|
   | api-service | dev → qa | `3f9a1c2e4d7b0e5f.yml` | ✓ success | feat: add payment integration |
   | api-service | qa → main | `a1b2c3d4e5f60718.yml` | ✓ success | Release Candidate v1.4.0 |
   | api-service | main → tag | `9e8d7c6b5a4f3e2d.yml` | ✓ success | v1.4.0 |
   | frontend | dev → staging | — | — no changes | — |
   | infra | dev → main | — | ✗ failed | — |

   **Result values:**
   - `✓ success` — plan generated with commits present
   - `— no changes` — plan's `commits` list was empty; nothing to release
   - `✗ failed` — plan command exited with an error (include the error message in a note below the table)

   The **Plan ID** column shows the `plan_file` value from the plan YAML (e.g.
   `3f9a1c2e4d7b0e5f.yml`). Use `—` when no plan was generated (no changes or
   failed).

   The **Title / Tag** column shows the `title.value` from the plan YAML for
   `b2b` steps, or the tag name for `b2t` steps. Use `—` when there is
   nothing to show (no changes or failed).

8. After all projects and pipeline steps have been processed, generate a
   release summary file and write it to the directory where
   `.shipit/multi-release.yml` lives.

   > **Important for AI agents:** Every tag and every pull/merge request
   > created during the release **must** appear as a clickable Markdown link
   > in the summary. Never write a bare tag name or PR number — always wrap it
   > in a hyperlink. Use the URL printed by `b2b apply` for pull/merge requests
   > and construct the tag URL from the remote host
   > (e.g. `https://github.com/org/repo/tree/v1.4.0`). A summary
   > without links is incomplete.

   **File naming:** `release-summary-<ISO-8601-timestamp>.md`  
   Use the current local time in `YYYYMMDDTHHMMSS` format (no colons or spaces):
   ```
   release-summary-20240601T143022.md
   ```

   **Summary file structure:**

   ```markdown
   # 🚀 Release Summary — <human-readable date and time>

   ## ✅ Projects With Changes

   | Project | Step | Tag / PR | Title |
   |---|---|---|---|
   | api-service | main → tag | [v1.4.0](https://github.com/org/api-service/tree/v1.4.0) | — |
   | api-service | dev → qa | [#42](https://github.com/org/api-service/pull/42) | feat: add payment integration |
   | infra | dev → main | [#11](https://github.com/org/infra/pull/11) | chore: update terraform modules |

   ## ⏩ Projects Without Changes

   - **frontend** — no commits found across all pipeline steps; nothing released.

   ## 🔍 Tagged Release Highlights

   ### ✨ New Features
   - <concise bullet summarising feature commits across all projects>

   ### 🐛 Bug Fixes
   - <concise bullet summarising fix commits across all projects>

   ### 🔧 Infrastructure
   - <concise bullet summarising chore/infra commits across all projects>

   ### 📄 Documentation
   - <concise bullet summarising docs commits across all projects>

   > Sections with no entries should be omitted entirely.
   ```

   **How to populate the summary:**
   - **Projects With Changes** — one row per pipeline step that produced a
     successful apply. Use the PR/MR URL returned by `b2b apply`, or the tag
     name returned by `b2t apply`, as the link/value in the **Tag / PR** column.
     Link to the created resource whenever possible: use a Markdown hyperlink
     for pull/merge requests (e.g. `[#42](https://github.com/org/repo/pull/42)`)
     and for tags if the remote host provides a tag URL
     (e.g. `[v1.4.0](https://github.com/org/repo/tree/v1.4.0)`).
   - **Projects Without Changes** — list every project (or individual step)
     where the plan's `commits` list was empty across the entire session.
   - **Release Highlights** — aggregate the `commits` lists from every
     successful b2t plan across all projects. Only changes associated with 
     tags should appear here. Group them by conventional-commit
     prefix (`feat`, `fix`, `chore`/`infra`, `docs`, etc.) and write one
     concise bullet per logical theme. Mention the project the highlight is 
     relevant to in the bullet point description. Omit any section that has 
     no entries.

   Present the generated file path to the user once it has been written.

**Do not proceed to the next pipeline step or the next project until the
current step succeeds and the user approves.**

---

### Handling Atypical Runs

When the user indicates a non-standard run (a subset of projects, fewer
environment steps, a different order), the agent must:

1. Collect the exact scope from the user — confirm each override explicitly.
2. Echo back the effective plan ("I will release `api-service` dev→qa only,
   then `infra` dev→main, tag") and wait for confirmation.
3. Execute using the collected overrides.
4. **Never write overrides back to `.shipit/multi-release.yml`.**  The
   persisted config always reflects the canonical full-release workflow.

---

### Deferred Tagging

When starting a multi-project release session, inform the user of the following
option before executing any steps:

> "Sometimes you may want a `b2b` release (a pull/merge request) to be reviewed
> and deployed before creating the tag with `b2t`. If that applies to any
> project in this run, let me know and I will skip the `b2t` step for it now.
> I will add it to a **Pending** section in the release summary so you can
> resume your session later and I will run the tag creation at that point."

When a `b2t` step is deferred:

1. Skip the `b2t` plan and apply for that project entirely.
2. Add a `## ⏳ Pending Tag Creation` section to the release summary file
   beneath the "Projects Without Changes" section:

   ```markdown
   ## ⏳ Pending Tag Creation

   The following projects had their `b2t` step deferred pending review and
   deployment of the `b2b` release above. Resume your session and reference
   the release summary file to trigger tag creation when ready.

   | Project | Deferred Step | Release Summary File |
   |---|---|---|
   | api-service | main → tag | `release-summary-20240601T143022.md` |
   ```

3. Tell the user:
   > "The `b2t` step for **\<project\>** has been deferred. When you are ready
   > to create the tag, start a new session and tell me to resume from
   > `release-summary-<timestamp>.md`. I will move the pending entry up into
   > the **Projects With Changes** section once the tag has been created."

**Resuming a deferred tag session:**

When the user references a release summary file to resume a deferred `b2t`
step, the agent must:

1. Read the release summary file to identify all entries in the
   `## ⏳ Pending Tag Creation` section.
2. For each pending entry, run the `b2t` plan and apply flow exactly as
   described in [Stage 3 — Main → Tag](#stage-3--main--tag), presenting the
   plan to the user for approval before applying.
3. On success, update the release summary file:
   - Move the completed entry from `## ⏳ Pending Tag Creation` into the
     `## ✅ Projects With Changes` table, adding the tag link in the
     **Tag / PR** column.
   - Remove the `## ⏳ Pending Tag Creation` section entirely if no pending
     entries remain.
4. Present the updated summary file path to the user.

---

### Example: Full Release Session

```
Agent: I found the following release config:
         1. api-service  — dev → qa (b2b) → main (b2b), tag (b2t)
         2. frontend     — dev → staging (b2b) → main (b2b), tag (b2t)
         3. infra        — dev → main (b2b), tag (b2t)
       Is this a full release across all projects and environments, or do
       you want to adjust the scope?

User:  Full release.

Agent: Starting release for project 1/3: api-service

       Step 1/3: dev → qa
       [generates plan, shows summary]
       Proceed with applying this plan? (yes/no)

User:  yes

Agent: ✓ PR opened: https://github.com/org/api-service/pull/42

       Step 2/3: qa → main
       [generates plan, shows summary]
       Proceed with applying this plan? (yes/no)

User:  yes

Agent: ✓ PR opened: https://github.com/org/api-service/pull/43

       Step 3/3: main → tag
       [generates tag plan, shows summary]
       Proceed with applying this plan? (yes/no)

User:  yes

Agent: ✓ Tag v1.4.0 created and pushed.

       Moving to project 2/3: frontend
       ...
```

---

### Example: Atypical Run Session

```
Agent: I found the following release config:
         1. api-service  — dev → qa (b2b) → main (b2b), tag (b2t)
         2. frontend     — dev → staging (b2b) → main (b2b), tag (b2t)
         3. infra        — dev → main (b2b), tag (b2t)
       Is this a full release across all projects and environments, or do
       you want to adjust the scope?

User:  Just release api-service and infra, skip tagging infra.

Agent: Got it — atypical run (will not update your saved config).
       Here is the effective plan:
         1. api-service  — dev → qa (b2b) → main (b2b), tag (b2t)
         2. infra        — dev → main (b2b)  [tag step skipped]
       Confirm? (yes/no)

User:  yes

Agent: Starting release for project 1/2: api-service
       ...
```
