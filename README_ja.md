# easy-worktree-rs

[`easy-worktree`](https://github.com/igtm/easy-worktree) の Rust 版です。

[English README](./README.md)

![easy-worktree-rs hero](./hero.png)

`easy-worktree-rs` は Git worktree を管理する `wt` コマンドを提供します。Python 版と同じコマンド体系を目指しており、現在のバージョンは `0.2.23` です。

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
curl -fsSL https://raw.githubusercontent.com/igtm/easy-worktree-rs/main/install.sh | sh -s -- -v=v0.2.23
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
wt clone (cn) [--bare] <repository_url> [dest_dir]
wt init (in)
wt add (ad) [<work_name> [<base_branch>]] [--skip-setup|--no-setup] [--select [<command>...]]
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
| `hook` | `ho` |
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

対話形式で worktree を作成します。

```bash
wt add
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

対話形式で worktree を削除します。

```bash
wt rm
```

対話形式で worktree を選択します（`fzf` があれば利用し、なければ番号選択にフォールバックします）。

```bash
wt select
```

PR の head branch 名で worktree を作成し、その path を確認します。

```bash
wt pr add 123
wt pr co 123
```

## フック

`wt init` は `.wt/` 以下に実行可能な hook テンプレートを作成します。
hook は普通の実行可能スクリプトなので、shebang さえあれば言語は問いません。

| Hook | 実行タイミング | 発火するコマンド |
| --- | --- | --- |
| `.wt/post-add` | worktree の作成後 | `wt add`, `wt pr add`, `wt stash`, `wt setup` |
| `.wt/pre-rm` | worktree の削除前 | `wt rm`, `wt clean` |

どちらの hook にも同じ環境変数が渡されます。

| 変数 | 説明 |
| --- | --- |
| `WT_WORKTREE_PATH` | worktree の path |
| `WT_WORKTREE_NAME` | worktree 名 |
| `WT_BASE_DIR` | メインリポジトリの path |
| `WT_BRANCH` | ブランチ名 |
| `WT_ACTION` | `post-add` では `add`、`pre-rm` では `rm` |

作業ディレクトリは worktree 自身です。
`pre-rm` は worktree がまだ存在する状態で走るため、これから失われるファイルを参照できます。
worktree のために作った成果物、たとえば worktree ディレクトリの外に置かれた
build 出力、container、image、volume などを開放する場所として使えます。

hook の出力は stderr に流れ、hook は `wt` の stdin を継承しません。
終了コードが 0 以外の場合は警告として報告されるだけで、処理は止まりません。
とくに `pre-rm` が失敗しても worktree は削除されます。
また削除自体が失敗して再実行した場合は hook も再度走るため、`pre-rm` は冪等に保ってください。

### hook を単体で実行する

`wt hook` は、前後の処理を伴わずに hook だけを実行します。
worktree の作成も削除もしないので、hook の動作確認や、直したあとの再実行に使えます。

```bash
wt hook                     # hook の一覧と実行可否を表示
wt hook pre-rm              # いま居る worktree で pre-rm を実行
wt hook pre-rm feature-1    # worktree を指定して実行
wt hook post-add
```

`wt setup` はこれとは別物で、従来どおりの動作です。
`setup_files` を worktree にコピーし、**そのあとで** `post-add` を実行します。
hook だけを走らせたい場合は `wt hook post-add` を使ってください。

### hook をスキップする

`wt rm` と `wt clean` は `--skip-hook`（`--no-hook`）を受け付けます。
`pre-rm` を実行せずに worktree を削除でき、`wt add` の `--skip-setup` と対になります。

```bash
wt rm feature-1 --skip-hook
wt clean --all --yes --skip-hook
```

## パフォーマンス

Rust 版と元の Python 版を `0.2.13` で比較しました。その後の `0.2.x` patch release
では、測定対象の経路に大きな変更はありません。以下は Linux
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
