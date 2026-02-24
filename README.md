```
███████╗██╗  ██╗██╗██████╗ ██╗████████╗
██╔════╝██║  ██║██║██╔══██╗██║╚══██╔══╝
███████╗███████║██║██████╔╝██║   ██║
╚════██║██╔══██║██║██╔═══╝ ██║   ██║
███████║██║  ██║██║██║     ██║   ██║
╚══════╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝   ╚═╝
```

**Shipit** is a Rust command line interface for managing merge requests, changelogs, tags, and releases. | [gitshipit.net](https://gitshipit.net)

[![Buy Me A Coffee](https://img.shields.io/badge/Buy%20Me%20A%20Coffee-support-%23FFDD00?style=flat&logo=buy-me-a-coffee&logoColor=black)](https://www.buymeacoffee.com/trollefson)
[![Crates.io](https://img.shields.io/crates/v/shipit)](https://crates.io/crates/shipit)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Installation

### Install Script (macOS/Linux/Windows)

```bash
curl -fsSL gitshipit.net/install | bash
```

### Cargo

```bash
cargo install shipit --locked
```

### Homebrew (macOS)

```bash
brew tap trollefson/shipit && brew install shipit
```

### From Source

```bash
git clone https://github.com/trollefson/shipit
cd shipit
cargo build --release --locked
```

Or grab a pre-built binary from the [releases page](https://github.com/trollefson/shipit/releases).

---

## Quick start

```bash
# 1. Generate a config file at the platform default location
shipit config generate

# 2. Check the config out and edit settings with your editor
shipit config show

# 3. Ship it from the root of your project. See the command docs below for more options
shipit b2b develop main --dry-run
```

---

## Optional Requirements

- A [GitHub](https://github.com) or [GitLab](https://gitlab.com) account and api token ([GitHub](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) / [GitLab](https://docs.gitlab.com/user/profile/personal_access_tokens/)) with merge request permissions is required for merge request creation with the `--dry-run` option disabled
- [Ollama](https://ollama.com) running locally with the model that matches your config is required for usage with the `--agent ollama` option

---

## Commands

### `b2b` — Branch to Branch

```
shipit b2b <source> <target> [--agent <agent>] [--dry-run] [--dir <path>] [--id <identifier>] [--remote <name>] [--prompt <text>] [--title <text>] [--description <text>]
```

| Argument / Flag                   | Description |
|-----------------------------------|-------------|
| `source`                          | Branch with new commits (e.g. `develop`) |
| `target`                          | Destination branch (e.g. `main`) |
| `--agent <agent>`                 | Agent to use for generating the merge/pull request description (`ollama`, `shipit`) |
| `--dry-run`                       | Preview the merge request description without creating it |
| `--dir <path>`                    | Path to the git repository (defaults to current directory) |
| `--id <identifier>`               | Project identifier — `owner/repo` for GitHub, numeric id for GitLab (auto-detected from remote url if omitted) |
| `--remote <name>`                 | Git remote to detect platform from (defaults to `origin`) |
| `--prompt <text>`                 | Prompt prefix sent to Ollama when `--agent ollama` is set (overrides the `ollama.prompt` config value) |
| `--title <text>`                  | Override the merge/pull request title (defaults to `<source> to <target>`) |
| `--description <text>`            | Use a custom description for the merge/pull request (skips commit discovery and ai summary) |

**What happens:**

1. Finds all commits on `source` that aren't on `target`
2. If `--agent` is set, generates categorized release notes (features, fixes, infra, docs) — using Ollama or the built-in shipit conventional commit categorizer
3. If the local `source` branch is ahead of the remote, prompts you to push it before continuing
4. Opens a pull/merge request on GitHub or GitLab with the description

**Examples:**

```bash
# Auto-detect platform and project from the origin remote URL
shipit b2b develop main

# With ollama-generated release notes
shipit b2b develop main --agent ollama

# With shipit's built-in conventional commit categorizer
shipit b2b develop main --agent shipit

# Preview the description without creating the request
shipit b2b develop main --agent shipit --dry-run

# Explicitly specify the project identifier
shipit b2b develop main --id owner/repo
shipit b2b develop main --id 12345678

# Use a different remote
shipit b2b develop main --remote upstream

# Point at a repo outside the current directory
shipit b2b develop main --dir /path/to/repo

# Pass a custom description directly (skips commit discovery and ai)
shipit b2b develop main --description "<your own mr/pr description>""
```

---

### `config generate`

Writes or overwrites a default config file to the platform config directory and prints the path.

Default config location:
- **Linux & macOS:** `~/.config/shipit/default-config.toml`
- **Windows:** `%APPDATA%\shipit\default-config.toml`

```bash
shipit config generate
```

---

### `config show`

Prints the current config file path and its contents as TOML.

```bash
shipit config show
```

## Platform support

| Platform | Architecture | Status |
|----------|--------------|--------|
| Linux    | x86_64       | ✓      |
| macOS    | x86_64       | ✓      |
| macOS    | aarch64      | ✓      |
| Windows  | x86_64       | ✓      |

---

---

## Support

If shipit saves you time, a coffee goes a long way.

[![Buy Me A Coffee](https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png)](https://www.buymeacoffee.com/trollefson)

---

## License

[MIT](LICENSE)
