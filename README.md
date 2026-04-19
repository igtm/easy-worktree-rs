# easy-worktree-rs

Rust port of [`easy-worktree`](https://github.com/igtm/easy-worktree).

[日本語版 README](./README_ja.md)

`easy-worktree-rs` provides the `wt` command for managing Git worktrees with the same command surface as the Python package. The current version is `0.2.13`.

## Install

Install the latest GitHub Release on Linux or macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh
```

Install to a custom directory:

```bash
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh -s -- -b=$HOME/.local/bin
```

Install a specific release:

```bash
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh -s -- -v=v0.2.13
```

Install from GitHub with Cargo:

```bash
cargo install --git https://github.com/igtm/easy-worktree-rs.git --locked
```

Install from a local checkout:

```bash
cargo install --path . --locked
```

## Usage

The CLI binary is `wt`:

```bash
wt clone [--bare] <repository_url> [dest_dir]
wt init
wt add <work_name> [<base_branch>] [--skip-setup|--no-setup] [--select [<command>...]]
wt list [--pr] [--quiet|-q] [--days N] [--merged] [--closed] [--all]
wt diff [<name>] [args...]
wt config [--global|--local] [<key> [<value>]]
wt rm <work_name> [-f|--force]
wt clean [--days N] [--merged] [--closed] [--all] [--yes|-y]
wt setup
wt stash <work_name> [<base_branch>]
wt pr add <number>
wt select [<name>|-] [<command>...]
wt current
wt run <name> <command>...
wt completion <bash|zsh>
wt doctor
```

## Examples

Initialize an existing repository:

```bash
cd my-repo
wt init
```

Create a worktree:

```bash
wt add feature-1
```

Create and immediately select a worktree:

```bash
wt add feature-1 --select
```

List worktrees:

```bash
wt list
wt list --quiet
wt list --pr
```

Remove a worktree:

```bash
wt rm feature-1
```

## Development

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings -A clippy::collapsible-if
```

## Release Automation

GitHub Actions runs CI on pull requests and pushes to `main`. A push to `main`, including a merged pull request, also runs the release workflow.

The release workflow reads `version` from `Cargo.toml`, creates or refreshes the `vX.Y.Z` GitHub Release, and uploads cross-built assets:

- `wt_vX.Y.Z_x86_64-unknown-linux-gnu.tar.gz`
- `wt_vX.Y.Z_aarch64-unknown-linux-gnu.tar.gz`
- `wt_vX.Y.Z_x86_64-apple-darwin.tar.gz`
- `wt_vX.Y.Z_aarch64-apple-darwin.tar.gz`
- `wt_vX.Y.Z_x86_64-pc-windows-msvc.tar.gz`
