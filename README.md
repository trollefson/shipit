```
███████╗██╗  ██╗██╗██████╗ ██╗████████╗
██╔════╝██║  ██║██║██╔══██╗██║╚══██╔══╝
███████╗███████║██║██████╔╝██║   ██║
╚════██║██╔══██║██║██╔═══╝ ██║   ██║
███████║██║  ██║██║██║     ██║   ██║
╚══════╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝   ╚═╝
```

**Shipit** is a Rust command line interface for managing merge requests, changelogs, tags, and releases using a plan and apply interface. Built with coding agent integration in mind. | [gitshipit.net](https://gitshipit.net)

[![Buy Me A Coffee](https://img.shields.io/badge/Buy%20Me%20A%20Coffee-support-%23FFDD00?style=flat&logo=buy-me-a-coffee&logoColor=black)](https://www.buymeacoffee.com/trollefson)
[![Crates.io](https://img.shields.io/crates/v/shipit)](https://crates.io/crates/shipit)
[![docs.rs](https://img.shields.io/docsrs/shipit)](https://docs.rs/shipit)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
---

## Why use shipit?

Most release tools execute immediately. Shipit uses a **plan/apply pattern** that separates "gather and review" from "execute." You see exactly what will be created before anything is pushed, which makes it safe to hand off to an AI agent without fear of a surprise tag or PR.

- **Agent-native by design.** Deterministic plan files, `--yaml` output, and a `CLAUDE.md` integration guide make shipit a first-class citizen in agentic coding workflows.
- **GitHub, GitLab, and more platforms to come in one binary.** No plugins, no config switching. Platform is detected automatically from your remote URL.
- **Multi-repo releases.** Coordinate releases across multiple repositories in a defined order from a single config file.
- **Commit enrichment.** Fetches PR/MR titles from the platform API so your changelogs read like release notes, not raw git log output.
- **No runtime required.** Single compiled Rust binary with no Node, Python, or dependency installation.

---

## Demo

![Shipit demo](site/demo.gif)

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

## AI-native workflow

See [AI.md](AI.md) for a full breakdown of how your agent is instructed to use shipit. The `shipit init` command will append these instructions to your CLAUDE.md file.

### Opening merge requests

Shipit integrates with your favorite agentic coding assistant. After installing shipit on your system, ask your agent to "create a merge request with shipit".

### Multi-project releases

To release across multiple repositories in a defined order: 

1. Run `shipit init --guide-only` in a directory you can revisit for running your multi-project release
2. Ask your agent to "run a multi-project release with shipit".
3. On the first run it will prompt you for your list of projects, their directory paths, and the environment pipeline for each (e.g. `dev → qa → main → tag`), and then save the config to `.shipit/multi-release.yml` in your current directory. On subsequent runs the agent reads that config, confirms whether you want a full release or a scoped one, walks you through each step, and waits for your approval before opening any PR or pushing any tag.

---

## CLI Docs

* [docs.rs - shipit](https://docs.rs/shipit/latest/shipit/)

## Platform support

| Platform | Architecture | Status |
|----------|--------------|--------|
| Linux    | x86_64       | ✓      |
| macOS    | x86_64       | ✓      |
| macOS    | aarch64      | ✓      |
| Windows  | x86_64       | ✓      |

---

## License

[MIT](LICENSE)
