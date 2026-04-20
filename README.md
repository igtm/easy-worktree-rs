# easy-worktree-rs

Rust port of [`easy-worktree`](https://github.com/igtm/easy-worktree).

[日本語版 README](./README_ja.md)

`easy-worktree-rs` provides the `wt` command for managing Git worktrees with the same command surface as the Python package. The current version is `0.2.14`.

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
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh -s -- -v=v0.2.14
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
wt clone (cn) [--bare] <repository_url> [dest_dir]
wt init (in)
wt add (ad) <work_name> [<base_branch>] [--skip-setup|--no-setup] [--select [<command>...]]
wt list (li, ls) [--pr] [--quiet|-q] [--days N] [--merged] [--closed] [--all]
wt diff (di, df) [<name>] [args...]
wt config (cf) [--global|--local] [<key> [<value>]]
wt rm/remove <work_name> [-f|--force]
wt clean (cl) [--days N] [--merged] [--closed] [--all] [--yes|-y]
wt setup (su)
wt stash (st) <work_name> [<base_branch>]
wt pr add <number>
wt select (se, sl) [<name>|-] [<command>...]
wt current (cu, cur)
wt co/checkout <work_name>
wt run (ru) <name> <command>...
wt completion (cm) <bash|zsh>
wt doctor (dr)
```

## Two-Letter Aliases

All primary commands have two-letter aliases. Existing aliases are kept for
compatibility.

| Command | Alias |
| --- | --- |
| `clone` | `cn` |
| `init` | `in` |
| `add` | `ad` |
| `list` | `li`, `ls` |
| `diff` | `di`, `df` |
| `config` | `cf` |
| `rm` / `remove` | `rm` |
| `clean` | `cl` |
| `setup` | `su` |
| `stash` | `st` |
| `pr` | `pr` |
| `select` | `se`, `sl` |
| `current` | `cu`, `cur` |
| `checkout` | `co` |
| `run` | `ru` |
| `completion` | `cm` |
| `doctor` | `dr` |

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

## Performance

The Rust binary was benchmarked against the original Python package at
`0.2.13`, before the alias-only `0.2.14` bump. Results below are from one local run on Linux aarch64
(`Linux-6.12.62+rpt-rpi-2712-aarch64-with-glibc2.41`). The Rust command was
`target/release/wt` built with `cargo build --release --locked`; the Python
command was the original package installed into a temporary `uv` virtualenv.
Each command used 8 warmup runs and 50 measured runs. Repository commands ran
against the same temporary Git repository with 20 worktrees.

| Command | Rust mean | Python mean | Speedup |
| --- | ---: | ---: | ---: |
| `wt --version` | 0.54 ms | 75.04 ms | 139.9x |
| `wt completion bash` | 0.57 ms | 72.68 ms | 126.9x |
| `wt list --quiet` | 95.06 ms | 204.65 ms | 2.2x |
| `wt current` | 85.59 ms | 210.32 ms | 2.5x |
| `wt select feature-10` | 173.92 ms | 339.16 ms | 2.0x |

Commands that call `git` are dominated by Git subprocess time, so their speedup
is smaller than pure startup-heavy commands.

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
