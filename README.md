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
[![codecov](https://codecov.io/gh/trollefson/shipit/branch/main/graph/badge.svg)](https://codecov.io/gh/trollefson/shipit)

---

## Why use shipit?

Most release tools execute immediately. Shipit uses a **plan/apply pattern** that separates "gather and review" from "execute." You see exactly what will be created before anything is pushed, which makes it safe to hand off to an AI agent without fear of a surprise tag or PR.

- **Agent-native by design.** Deterministic plan files and `--yaml` output make shipit a first-class citizen in agentic coding workflows while still being a standalone tool meant for humans.
- **Claude Code skill included.** Run `shipit claude` to install the shipit skill globally. Then type `/shipit` in any Claude Code session to load the full workflow guide. No per-project setup required.
- **GitHub, GitLab, and more platforms to come in one binary.** No plugins, no config switching, and no git platform lock-in. Platform is detected automatically from your remote URL.
- **Multi-repo releases.** Coordinate releases across multiple repositories in a defined order from a single config file using an agent or write your own script.
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

### Pre-built Releases

See the [releases page](https://github.com/trollefson/shipit/releases) for the binaries installed by gitshipit.net/install

### Install Claude Code Skill

```bash
shipit claude
```

---

## Claude Code Skill Usage

![Shipit claude demo](site/demo-claude.gif)

The `/shipit` skill loads the full workflow guide into Claude's context. Claude will confirm your intent, generate a plan (title, description, and commit list) before touching anything, and ask for your approval before creating any PR/MR or tag.

### Opening merge requests

```
/shipit create a merge request
```

Claude will run `shipit b2b plan` to collect commits between your source and target branches, enrich the description from PR/MR titles, present the plan for review, and open the pull/merge request only after you approve.

### Creating tags

```
/shipit create a tag
```

Claude will run `shipit b2t plan` to collect commits since the last tag, suggest the next semantic version, present the annotated tag plan for review, and push the tag only after you approve.

### Multi-project releases

```
/shipit run a multi-project release
```

Claude will check for a `.shipit/multi-release.yml` config (creating one interactively on first run), walk through each project and pipeline step in order, present every plan before applying it, and produce a Markdown release summary with links to every PR/MR and tag created. These summaries are a great way to build a static site of release notes.

---

## CLI Docs

* [docs.rs - shipit](https://docs.rs/shipit/latest/shipit/)

## Platform support

| Platform | Architecture | Status |
|----------|--------------|--------|
| Linux    | x86_64       | ✅      |
| macOS    | x86_64       | ✅      |
| macOS    | aarch64      | ✅      |
| Windows  | x86_64       | ✅      |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for local development setup, coverage, and pre-commit hooks.

## License

[MIT](LICENSE)
