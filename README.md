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

Shipit integrates with your favorite agentic coding assistant. Install shipit, then just ask your agent to create a merge request. It will follow the instructions in [AI.md](AI.md) automatically.

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
