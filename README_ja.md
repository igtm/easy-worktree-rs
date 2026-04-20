# easy-worktree-rs

[`easy-worktree`](https://github.com/igtm/easy-worktree) の Rust 版です。

[English README](./README.md)

`easy-worktree-rs` は Git worktree を管理する `wt` コマンドを提供します。Python 版と同じコマンド体系を目指しており、現在のバージョンは `0.2.14` です。

## インストール

Linux または macOS で最新の GitHub Release をインストールします。

```bash
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh
```

インストール先を指定する場合:

```bash
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh -s -- -b=$HOME/.local/bin
```

バージョンを指定する場合:

```bash
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh -s -- -v=v0.2.14
```

Cargo で GitHub からインストールする場合:

```bash
cargo install --git https://github.com/igtm/easy-worktree-rs.git --locked
```

ローカル checkout からインストールする場合:

```bash
cargo install --path . --locked
```

## 使い方

バイナリ名は `wt` です。

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

## 2 文字エイリアス

主要コマンドには 2 文字エイリアスがあります。既存のエイリアスも互換性のため残しています。

| コマンド | エイリアス |
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

## 例

既存リポジトリを初期化します。

```bash
cd my-repo
wt init
```

worktree を作成します。

```bash
wt add feature-1
```

worktree を作成してすぐに選択します。

```bash
wt add feature-1 --select
```

worktree 一覧を表示します。

```bash
wt list
wt list --quiet
wt list --pr
```

worktree を削除します。

```bash
wt rm feature-1
```

## パフォーマンス

Rust 版と元の Python 版を、2 文字エイリアスのみを追加した `0.2.14` bump 前の
`0.2.13` で比較しました。以下は Linux
aarch64 (`Linux-6.12.62+rpt-rpi-2712-aarch64-with-glibc2.41`) でのローカル測定です。
Rust 版は `cargo build --release --locked` でビルドした `target/release/wt`、Python
版は元のパッケージを一時 `uv` virtualenv にインストールした `wt` を使いました。
各コマンドは warmup 8 回、測定 50 回です。リポジトリ操作は、worktree を 20 個持つ同じ一時 Git
リポジトリで測定しました。

| コマンド | Rust 平均 | Python 平均 | 高速化 |
| --- | ---: | ---: | ---: |
| `wt --version` | 0.54 ms | 75.04 ms | 139.9x |
| `wt completion bash` | 0.57 ms | 72.68 ms | 126.9x |
| `wt list --quiet` | 95.06 ms | 204.65 ms | 2.2x |
| `wt current` | 85.59 ms | 210.32 ms | 2.5x |
| `wt select feature-10` | 173.92 ms | 339.16 ms | 2.0x |

`git` を呼び出すコマンドは Git subprocess の実行時間が支配的なため、起動コスト中心のコマンドより高速化幅は小さくなります。

## 開発

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings -A clippy::collapsible-if
```

## リリース自動化

GitHub Actions は pull request と `main` への push で CI を実行します。pull request の merge も GitHub 上では `main` への push になるため、release workflow が実行されます。

release workflow は `Cargo.toml` の `version` を読み取り、`vX.Y.Z` の GitHub Release を作成または更新し、以下のクロスビルド成果物をアップロードします。

- `wt_vX.Y.Z_x86_64-unknown-linux-gnu.tar.gz`
- `wt_vX.Y.Z_aarch64-unknown-linux-gnu.tar.gz`
- `wt_vX.Y.Z_x86_64-apple-darwin.tar.gz`
- `wt_vX.Y.Z_aarch64-apple-darwin.tar.gz`
- `wt_vX.Y.Z_x86_64-pc-windows-msvc.tar.gz`
