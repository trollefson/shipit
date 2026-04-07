# Contributing

## Toolchain

`rust-toolchain.toml` pins the toolchain and installs the `llvm-tools-preview` component automatically when you run any `cargo` command.

## Coverage

Install `cargo-llvm-cov` once:

```bash
cargo install cargo-llvm-cov
```

Then run:

```bash
cargo llvm-cov --open
```

## Submitting your changes

Use your local build to open an MR when you're done contributing:

```bash
cargo run -- init
cargo run -- b2b plan <feature_branch> main
cargo run -- b2b apply .shipit/plans/<plan_file>.yml
```

## Pre-commit hooks

Install [pre-commit](https://pre-commit.com), then install the hooks:

```bash
pip install pre-commit
pre-commit install
```

The hooks run `cargo clippy`, `cargo test`, and `cargo llvm-cov` on each commit.
