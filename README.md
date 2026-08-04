# easy-worktree-rs

Rust port of [`easy-worktree`](https://github.com/igtm/easy-worktree).

[日本語版 README](./README_ja.md)

![easy-worktree-rs hero](./hero.png)

`easy-worktree-rs` provides the `wt` command for managing Git worktrees with the same command surface as the Python package. The current version is `0.2.24`.

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
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh -s -- -v=v0.2.24
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
wt add (ad) [<work_name> [<base_branch>]] [--skip-setup|--no-setup] [--skip-hook|--no-hook] [--select [<command>...]]
wt list (li, ls) [--pr] [--quiet|-q] [--days N] [--merged] [--closed] [--all]
wt diff (di, df) [<name>] [args...]
wt config (cf) [--global|--local] [<key> [<value>]]
wt rm/remove [<work_name>] [-f|--force] [--skip-hook|--no-hook]
wt clean (cl) [--days N] [--merged] [--closed] [--all] [--yes|-y] [--skip-hook|--no-hook]
wt setup (su)
wt hook (ho) [<hook_name> [<work_name>]]
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
| `hook` | `ho` |
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

Create a worktree interactively:

```bash
wt add
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

Remove a worktree interactively:

```bash
wt rm
```

Interactively select a worktree (`fzf` when available, otherwise a numbered prompt):

```bash
wt select
```

Create a worktree from a PR using the PR head branch name:

```bash
wt pr add 123
wt pr co 123
```

## Hooks

`wt init` creates executable hook templates under `.wt/`. Each hook is an
ordinary executable script, so any language works as long as it has a shebang.

| Hook | Runs | Triggered by |
| --- | --- | --- |
| `.wt/post-add` | After a worktree is created | `wt add`, `wt pr add`, `wt stash`, `wt setup` |
| `.wt/pre-rm` | Before a worktree is removed | `wt rm`, `wt clean` |

Both hooks receive the same environment variables:

| Variable | Description |
| --- | --- |
| `WT_WORKTREE_PATH` | Path to the worktree |
| `WT_WORKTREE_NAME` | Name of the worktree |
| `WT_BASE_DIR` | Path to the main repository directory |
| `WT_BRANCH` | Branch name |
| `WT_ACTION` | `add` for `post-add`, `rm` for `pre-rm` |

The working directory is the worktree itself. `pre-rm` runs while the worktree
still exists, so it can inspect the files it is about to lose — which makes it
the place to release anything created for that worktree, such as build
outputs, containers, images or volumes that live outside the worktree
directory.

Hook output is streamed to stderr, and the hook does not inherit `wt`'s stdin.
A non-zero exit status is reported as a warning and does not stop the
operation; in particular a failing `pre-rm` still removes the worktree, and the
hook runs again on a retry when the removal itself fails, so keep `pre-rm`
idempotent.

### Running a hook on its own

`wt hook` runs a single hook without the surrounding operation. It never
creates or removes a worktree, which makes it the way to test a hook or to
re-run one after fixing it.

```bash
wt hook                     # list hooks and whether each one is runnable
wt hook pre-rm              # run pre-rm for the worktree you are in
wt hook pre-rm feature-1    # run pre-rm for a named worktree
wt hook post-add
```

`wt setup` is a different thing and stays as it is: it copies `setup_files`
into the worktree **and then** runs `post-add`. Use `wt hook post-add` when you
want the hook alone.

### Skipping a hook

`wt add`, `wt rm` and `wt clean` accept `--skip-hook` (or `--no-hook`) to run
without their hook.

```bash
wt add feature-1 --skip-hook       # copy setup_files, but do not run post-add
wt rm feature-1 --skip-hook
wt clean --all --yes --skip-hook
```

On `wt add` this is narrower than `--skip-setup`. "Setup" means two things —
copying `setup_files` into the worktree and then running `post-add` — and the
two flags let you skip either the whole step or just the hook:

| | `setup_files` copied | `post-add` run |
| --- | --- | --- |
| `wt add` | yes | yes |
| `wt add --skip-hook` | yes | no |
| `wt add --skip-setup` | no | no |

Skipping the hook is worth it when the hook is the slow part — installing
dependencies, building images — and you want the worktree usable immediately.
Run `wt hook post-add` later to catch up.

## Performance

The Rust binary was benchmarked against the original Python package at
`0.2.13`; later `0.2.x` patch releases have not materially changed these
measured paths. Results below are from one local run on Linux aarch64
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
