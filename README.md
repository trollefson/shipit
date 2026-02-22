```
███████╗██╗  ██╗██╗██████╗ ██╗████████╗
██╔════╝██║  ██║██║██╔══██╗██║╚══██╔══╝
███████╗███████║██║██████╔╝██║   ██║
╚════██║██╔══██║██║██╔═══╝ ██║   ██║
███████║██║  ██║██║██║     ██║   ██║
╚══════╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝   ╚═╝
```

**Shipit** is a Rust CLI that automates merge request creation on your favorite platforms with optional AI-generated notes | [gitshipit.net](https://gitshipit.net)

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
shipit b2b develop main --dryrun
```

---

## Optional Requirements

- A [GitHub](https://github.com) or [GitLab](https://gitlab.com) account and api token ([GitHub](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) / [GitLab](https://docs.gitlab.com/user/profile/personal_access_tokens/)) with merge request permissions is required for merge request creation with the `--dry-run` option disabled
- [Ollama](https://ollama.com) running locally with the model that matches your config is required for usage with the `--ai` option enabled

---

## Commands

### `b2b` — Branch to Branch

```
shipit b2b <source> <target> [--ai] [--dryrun] [--dir <path>] [--id <identifier>] [--platform <github|gitlab>] [--remote <name>] [--prompt <text>]
```

| Argument / Flag                   | Description |
|-----------------------------------|-------------|
| `source`                          | Branch with new commits (e.g. `develop`) |
| `target`                          | Destination branch (e.g. `main`) |
| `--ai`                            | Enable Ollama LLM to generate categorized release notes |
| `--dryrun`                        | Preview the merge request description without creating it |
| `--dir <path>`                    | Path to the git repository (defaults to current directory) |
| `--id <identifier>`               | Project identifier — `owner/repo` for GitHub, numeric ID for GitLab (auto-detected from remote URL if omitted) |
| `--platform <github\|gitlab>`     | Force a specific platform (overrides auto-detection from the remote URL) |
| `--remote <name>`                 | Git remote to push to and detect platform from (defaults to `origin`) |
| `--prompt <text>`                 | Prompt prefix sent to Ollama when `--ai` is set (overrides the `ollama.prompt` config value) |

**What happens:**

1. Finds all commits on `source` that aren't on `target`
2. If `--ai` is set, sends the commit log to a local LLM running with Ollama and generates categorized release notes (features, fixes, infra, docs)
3. Pushes the `source` branch to the remote
4. Opens a pull/merge request on GitHub or GitLab with the description

**Examples:**

```bash
# Auto-detect platform and project from the origin remote URL
shipit b2b develop main

# With llm generated release notes
shipit b2b develop main --ai

# Preview the description without creating the request
shipit b2b develop main --ai --dryrun

# Explicitly specify the project identifier
shipit b2b develop main --id owner/repo
shipit b2b develop main --id 12345678

# Override the detected platform
shipit b2b develop main --platform github
shipit b2b develop main --platform gitlab

# Use a different remote
shipit b2b develop main --remote upstream

# Point at a repo outside the current directory
shipit b2b develop main --dir /path/to/repo
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
