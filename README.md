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
curl -fsSL https://github.com/trollefson/shipit/releases/latest/download/install.sh | bash
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

- A [GitLab](https://gitlab.com) account and [api token](https://docs.gitlab.com/user/profile/personal_access_tokens/) with merge request permissions is required for merge request creation with the `--dry-run` option disabled
- [Ollama](https://ollama.com) running locally with the model that matches your config is required for usage with the `--ai` option enabled

---

## Commands

### `b2b` — Branch to Branch

```
shipit b2b <source> <target> [--ai] [--dryrun] [--dir <path>]
```

| Argument / Flag | Description |
|-----------------|-------------|
| `source`        | Branch with new commits (e.g. `develop`) |
| `target`        | Destination branch (e.g. `main`) |
| `--ai`          | Enable Ollama LLM to generate categorized release notes |
| `--dryrun`      | Preview the merge request description without creating it |
| `--dir <path>`  | Path to the git repository (defaults to current directory) |

**What happens:**

1. Finds all commits on `source` that aren't on `target`
2. If `--ai` is set, sends the commit log to a local LLM running with Ollama and generate categorized release notes (features, fixes, infra, docs)
3. Opens a merge request on GitLab or Github with the description

**Examples:**

```bash
# Basic — raw commit list as MR description
shipit b2b develop main

# With AI-generated release notes
shipit b2b develop main --ai

# Dry run — see the description without creating the MR
shipit b2b develop main --ai --dryrun

# Point at a repo outside the current directory
shipit b2b develop main --dir /path/to/repo
```

---

### `config generate`

Writes or overwrites a default config file to the platform config directory and prints the path.

Default config location:
- **Linux:** `~/.config/shipit/default-config.toml`
- **macOS:** `~/Library/Application Support/shipit/default-config.toml`
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