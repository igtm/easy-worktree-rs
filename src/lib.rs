use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use serde_json::Value as JsonValue;
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::Display;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;

static GLOBAL_GIT_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

const POST_ADD_TEMPLATE: &str = r#"#!/bin/bash
# Post-add hook for easy-worktree
# This script is automatically executed after creating a new worktree
#
# Available environment variables:
#   WT_WORKTREE_PATH  - Path to the created worktree
#   WT_WORKTREE_NAME  - Name of the worktree
#   WT_BASE_DIR       - Path to the main repository directory
#   WT_BRANCH         - Branch name
#   WT_ACTION         - Action name (add)
#
# Example: Install dependencies and copy configuration files
#
# set -e
#
# echo "Initializing worktree: $WT_WORKTREE_NAME"
#
# # Install npm packages
# if [ -f package.json ]; then
#     npm install
# fi
#
# # Copy .env file
# if [ -f "$WT_BASE_DIR/.env.example" ]; then
#     cp "$WT_BASE_DIR/.env.example" .env
# fi
#
# echo "Setup completed!"
"#;

#[derive(Debug, Clone)]
struct CmdResult {
    stdout: String,
    stderr: String,
    status: i32,
}

#[derive(Debug, Clone)]
struct WorktreeEntry {
    path: PathBuf,
    branch: Option<String>,
    is_bare: bool,
}

#[derive(Debug, Clone)]
struct WorktreeInfo {
    path: PathBuf,
    head: Option<String>,
    branch: String,
    created: Option<NaiveDateTime>,
    last_commit: Option<NaiveDateTime>,
    is_clean: bool,
    has_untracked: bool,
    insertions: usize,
    deletions: usize,
    pr_info: String,
    reason: String,
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
enum ShellMode {
    UnixLike,
    Cmd,
}

fn is_japanese() -> bool {
    env::var("LANG")
        .unwrap_or_default()
        .to_lowercase()
        .contains("ja")
}

fn template(key: &str) -> (&'static str, &'static str) {
    match key {
        "error" => ("Error: {}", "エラー: {}"),
        "usage" => (
            "Usage: wt clone [--bare] <repository_url> [dest_dir]",
            "使用方法: wt clone [--bare] <repository_url> [dest_dir]",
        ),
        "usage_add" => (
            "Usage: wt add (ad) <work_name> [<base_branch>] [--skip-setup|--no-setup] [--select [<command>...]]",
            "使用方法: wt add (ad) <作業名> [<base_branch>] [--skip-setup|--no-setup] [--select [<コマンド>...]]",
        ),
        "usage_select" => (
            "Usage: wt select (sl) [<name>|-] [<command>...]",
            "使用方法: wt select (sl) [<名前>|-] [<コマンド>...]",
        ),
        "usage_diff" => (
            "Usage: wt diff (df) [<name>] [args...]",
            "使用方法: wt diff (df) [<名前>] [引数...]",
        ),
        "usage_config" => (
            "Usage: wt config [--global|--local] [<key> [<value>]]",
            "使用方法: wt config [--global|--local] [<キー> [<値>]]",
        ),
        "usage_list" => (
            "Usage: wt list (ls) [--pr] [--quiet|-q] [--days N] [--merged] [--closed] [--all] [--sort created|last-commit|name|branch] [--asc|--desc]",
            "使用方法: wt list (ls) [--pr] [--quiet|-q] [--days N] [--merged] [--closed] [--all] [--sort created|last-commit|name|branch] [--asc|--desc]",
        ),
        "usage_run" => (
            "Usage: wt run <name> <command>...",
            "使用方法: wt run <名前> <コマンド>...",
        ),
        "usage_rm" => ("Usage: wt rm <work_name>", "使用方法: wt rm <作業名>"),
        "base_not_found" => (
            "Main repository directory not found",
            "メインリポジトリのディレクトリが見つかりません",
        ),
        "run_in_wt_dir" => (
            "Please run inside WT_<repository_name>/ directory",
            "WT_<repository_name>/ ディレクトリ内で実行してください",
        ),
        "wt_home_not_found" => (
            "No non-bare worktree found for this bare repository. Create a base-branch worktree first.",
            "この bare リポジトリで利用可能な non-bare worktree が見つかりません。先にベースブランチの worktree を作成してください。",
        ),
        "suggest_init" => ("Initialize wt config with: {}", "wt 設定を初期化: {}"),
        "did_you_mean" => ("Did you mean: {}", "もしかして: {}"),
        "available_worktrees" => ("Available worktrees: {}", "利用可能な worktree: {}"),
        "already_exists" => ("{} already exists", "{} はすでに存在します"),
        "cloning" => ("Cloning: {} -> {}", "クローン中: {} -> {}"),
        "completed_clone" => ("Completed: cloned to {}", "完了: {} にクローンしました"),
        "not_git_repo" => (
            "Current directory is not a git repository",
            "現在のディレクトリは git リポジトリではありません",
        ),
        "fetching" => (
            "Fetching latest information from remote...",
            "リモートから最新情報を取得中...",
        ),
        "creating_worktree" => ("Creating worktree: {}", "worktree を作成中: {}"),
        "completed_worktree" => (
            "Completed: created worktree at {}",
            "完了: {} に worktree を作成しました",
        ),
        "removing_worktree" => ("Removing worktree: {}", "worktree を削除中: {}"),
        "completed_remove" => ("Completed: removed {}", "完了: {} を削除しました"),
        "creating_branch" => (
            "Creating new branch '{}' from '{}'",
            "ブランチ '{}' を '{}' から作成しています",
        ),
        "default_branch_not_found" => (
            "Could not find default branch (main/master)",
            "デフォルトブランチ (main/master) が見つかりません",
        ),
        "running_hook" => ("Running post-add hook: {}", "post-add hook を実行中: {}"),
        "hook_not_executable" => (
            "Warning: hook is not executable: {}",
            "警告: hook が実行可能ではありません: {}",
        ),
        "hook_failed" => (
            "Warning: hook exited with code {}",
            "警告: hook が終了コード {} で終了しました",
        ),
        "usage_clean" => (
            "Usage: wt clean (cl) [--days N] [--merged] [--closed] [--all] [--yes|-y]",
            "使用方法: wt clean (cl) [--days N] [--merged] [--closed] [--all] [--yes|-y]",
        ),
        "no_clean_targets" => (
            "No worktrees to clean",
            "クリーンアップ対象の worktree がありません",
        ),
        "clean_confirm" => (
            "Remove {} worktree(s)? [y/N]: ",
            "{} 個の worktree を削除しますか？ [y/N]: ",
        ),
        "worktree_name" => ("Worktree", "Worktree"),
        "branch_name" => ("Branch", "ブランチ"),
        "created_at" => ("Created", "作成日時"),
        "last_commit" => ("Last Commit", "最終コミット"),
        "changes_label" => ("Changes", "変更"),
        "usage_pr" => ("Usage: wt pr add <number>", "使用方法: wt pr add <number>"),
        "usage_setup" => ("Usage: wt setup (su)", "使用方法: wt setup (su)"),
        "usage_stash" => (
            "Usage: wt stash (st) <work_name> [<base_branch>]",
            "使用方法: wt stash (st) <work_name> [<base_branch>]",
        ),
        "usage_completion" => (
            "Usage: wt completion <bash|zsh>",
            "使用方法: wt completion <bash|zsh>",
        ),
        "stashing_changes" => (
            "Stashing local changes...",
            "ローカルの変更をスタッシュ中...",
        ),
        "popping_stash" => (
            "Moving changes to new worktree...",
            "変更を新しい worktree に移動中...",
        ),
        "nothing_to_stash" => (
            "No local changes to stash.",
            "スタッシュする変更がありません",
        ),
        "select_switched" => (
            "Switched worktree to: {}",
            "作業ディレクトリを切り替えました: {}",
        ),
        "select_not_found" => ("Worktree not found: {}", "worktree が見つかりません: {}"),
        "select_no_last" => ("No previous selection found", "以前の選択が見つかりません"),
        "setting_up" => ("Setting up: {} -> {}", "セットアップ中: {} -> {}"),
        "completed_setup" => (
            "Completed setup of {} files",
            "{} 個のファイルをセットアップしました",
        ),
        "using_setup_source" => ("Using setup source: {}", "セットアップコピー元: {}"),
        "setup_source_not_found" => (
            "Setup source directory not found. Skipping file copy.",
            "セットアップコピー元が見つからないため、ファイルコピーをスキップします。",
        ),
        "suggest_setup" => (
            "Some setup files are missing. Run 'wt setup' to initialize this worktree.",
            "一部のセットアップファイルが不足しています。'wt setup' を実行して初期化してください。",
        ),
        "nesting_error" => (
            "Error: Already in a wt subshell ({}). Please 'exit' before switching.",
            "エラー: すでに wt のサブシェル ({}) 内にいます。切り替える前に 'exit' してください。",
        ),
        "jump_instruction" => (
            "Jumping to '{}' ({}). Type 'exit' or Ctrl-D to return.",
            "'{}' ({}) にジャンプします。戻るには 'exit' または Ctrl-D を入力してください。",
        ),
        _ => ("", ""),
    }
}

fn format_template(template: &str, args: &[String]) -> String {
    let mut out = template.to_string();
    for arg in args {
        if let Some(pos) = out.find("{}") {
            out.replace_range(pos..pos + 2, arg);
        }
    }
    out
}

fn msg(key: &str, args: &[String]) -> String {
    let (en, ja) = template(key);
    let template = if en.is_empty() {
        key
    } else if is_japanese() {
        ja
    } else {
        en
    };
    format_template(template, args)
}

fn m0(key: &str) -> String {
    msg(key, &[])
}

fn m1<T: Display>(key: &str, a: T) -> String {
    msg(key, &[a.to_string()])
}

fn m2<T: Display, U: Display>(key: &str, a: T, b: U) -> String {
    msg(key, &[a.to_string(), b.to_string()])
}

fn fatal(message: impl Display) -> ! {
    eprintln!("{}", message);
    std::process::exit(1);
}

fn fatal_error(message: impl Display) -> ! {
    fatal(m1("error", message));
}

fn global_git_dir() -> Option<PathBuf> {
    GLOBAL_GIT_DIR.lock().ok().and_then(|guard| guard.clone())
}

fn set_global_git_dir(path: PathBuf) {
    if let Ok(mut guard) = GLOBAL_GIT_DIR.lock() {
        *guard = Some(path);
    }
}

fn run_command(
    cmd: Vec<String>,
    cwd: Option<&Path>,
    check: bool,
    apply_global_git_dir: bool,
) -> CmdResult {
    if cmd.is_empty() {
        fatal_error("empty command");
    }

    let mut final_cmd = cmd.clone();
    if apply_global_git_dir
        && cmd.first().is_some_and(|first| first == "git")
        && global_git_dir().is_some()
        && !cmd
            .iter()
            .any(|arg| arg == "--git-dir" || arg.starts_with("--git-dir="))
    {
        let git_dir = global_git_dir().unwrap();
        final_cmd = vec![
            "git".to_string(),
            format!("--git-dir={}", git_dir.display()),
        ];
        final_cmd.extend(cmd.into_iter().skip(1));
    }

    let mut command = Command::new(&final_cmd[0]);
    command.args(&final_cmd[1..]);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let output = match command.output() {
        Ok(output) => output,
        Err(err) => fatal_error(err),
    };
    let result = CmdResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status: output.status.code().unwrap_or(1),
    };

    if check && result.status != 0 {
        fatal_error(result.stderr.trim_end());
    }

    result
}

fn path_abs(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn expand_user(path: &str) -> PathBuf {
    if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(path)
    }
}

fn path_resolve(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path_abs(path))
}

fn same_path(a: &Path, b: &Path) -> bool {
    path_resolve(a) == path_resolve(b)
}

fn path_starts_with(path: &Path, base: &Path) -> bool {
    path_resolve(path).starts_with(path_resolve(base))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn xdg_config_home() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
}

#[cfg(windows)]
fn interactive_shell() -> (String, ShellMode) {
    if let Ok(shell) = env::var("SHELL") {
        (shell, ShellMode::UnixLike)
    } else {
        (
            env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
            ShellMode::Cmd,
        )
    }
}

#[cfg(not(windows))]
fn interactive_shell() -> (String, ShellMode) {
    (
        env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
        ShellMode::UnixLike,
    )
}

fn which_cmd(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return path.exists().then(|| path.to_path_buf());
    }
    let paths = env::var_os("PATH")?;
    for dir in env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn get_repository_name(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    if let Some(last) = trimmed.rsplit('/').next() {
        if last != trimmed || trimmed.contains('/') {
            let clean = last.strip_suffix(".git").unwrap_or(last);
            return clean.rsplit(':').next().unwrap_or(clean).to_string();
        }
    }
    Path::new(trimmed)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| trimmed.to_string())
}

fn get_default_branch_for_bare_git_dir(git_dir: &Path) -> Option<String> {
    let result = run_command(
        vec![
            "git".into(),
            format!("--git-dir={}", git_dir.display()),
            "symbolic-ref".into(),
            "refs/remotes/origin/HEAD".into(),
        ],
        None,
        false,
        false,
    );
    if result.status == 0 && !result.stdout.trim().is_empty() {
        return result.stdout.trim().rsplit('/').next().map(str::to_string);
    }

    for branch in ["main", "master"] {
        let result = run_command(
            vec![
                "git".into(),
                format!("--git-dir={}", git_dir.display()),
                "show-ref".into(),
                "--verify".into(),
                format!("refs/heads/{branch}"),
            ],
            None,
            false,
            false,
        );
        if result.status == 0 {
            return Some(branch.to_string());
        }
    }

    let result = run_command(
        vec![
            "git".into(),
            format!("--git-dir={}", git_dir.display()),
            "symbolic-ref".into(),
            "HEAD".into(),
        ],
        None,
        false,
        false,
    );
    if result.status == 0 {
        let out = result.stdout.trim();
        if let Some(branch) = out.strip_prefix("refs/heads/") {
            return Some(branch.to_string());
        }
    }
    None
}

fn print_init_suggestion() {
    if let Some(git_dir) = global_git_dir() {
        eprintln!(
            "{}",
            m1(
                "suggest_init",
                format!("wt --git-dir={} init", git_dir.display())
            )
        );
    } else {
        eprintln!("{}", m1("suggest_init", "wt init"));
    }
}

fn find_base_dir() -> Option<PathBuf> {
    if let Some(git_dir) = global_git_dir() {
        if git_dir.file_name().is_some_and(|name| name == ".git") {
            return git_dir.parent().map(Path::to_path_buf);
        }
        return Some(git_dir);
    }

    let result = run_command(
        vec!["git".into(), "rev-parse".into(), "--git-common-dir".into()],
        None,
        false,
        true,
    );
    if result.status == 0 && !result.stdout.trim().is_empty() {
        let mut git_common_dir = PathBuf::from(result.stdout.trim());
        if !git_common_dir.is_absolute() {
            git_common_dir = env::current_dir().ok()?.join(git_common_dir);
        }
        let git_common_dir = path_resolve(&git_common_dir);
        if git_common_dir
            .file_name()
            .is_some_and(|name| name == ".git")
        {
            return git_common_dir.parent().map(Path::to_path_buf);
        }
        return Some(git_common_dir);
    }

    let result = run_command(
        vec!["git".into(), "rev-parse".into(), "--show-toplevel".into()],
        None,
        false,
        true,
    );
    if result.status == 0 && !result.stdout.trim().is_empty() {
        return Some(PathBuf::from(result.stdout.trim()));
    }

    let current = env::current_dir().ok()?;
    for candidate in std::iter::once(current.as_path()).chain(current.ancestors().skip(1)) {
        if candidate.join(".git").exists() {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn is_bare_repository(base_dir: &Path) -> bool {
    let result = run_command(
        vec![
            "git".into(),
            "rev-parse".into(),
            "--is-bare-repository".into(),
        ],
        Some(base_dir),
        false,
        true,
    );
    result.status == 0 && result.stdout.trim().eq_ignore_ascii_case("true")
}

fn get_worktree_entries(base_dir: &Path) -> Vec<WorktreeEntry> {
    let result = run_command(
        vec![
            "git".into(),
            "worktree".into(),
            "list".into(),
            "--porcelain".into(),
        ],
        Some(base_dir),
        true,
        true,
    );
    let mut entries = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    let mut is_bare = false;

    for line in result.stdout.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(path) = path.take() {
                entries.push(WorktreeEntry {
                    path,
                    branch: branch.take(),
                    is_bare,
                });
                is_bare = false;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = Some(rest.strip_prefix("refs/heads/").unwrap_or(rest).to_string());
        } else if line.trim() == "bare" {
            is_bare = true;
        }
    }
    entries
}

fn get_default_branch(base_dir: &Path) -> Option<String> {
    let result = run_command(
        vec![
            "git".into(),
            "rev-parse".into(),
            "--abbrev-ref".into(),
            "origin/HEAD".into(),
        ],
        Some(base_dir),
        false,
        true,
    );
    if result.status == 0 && !result.stdout.trim().is_empty() {
        return Some(
            result
                .stdout
                .trim()
                .trim_start_matches("origin/")
                .to_string(),
        );
    }

    for branch in ["main", "master"] {
        let result = run_command(
            vec![
                "git".into(),
                "rev-parse".into(),
                "--verify".into(),
                branch.into(),
            ],
            Some(base_dir),
            false,
            true,
        );
        if result.status == 0 {
            return Some(branch.to_string());
        }
    }

    let result = run_command(
        vec![
            "git".into(),
            "rev-parse".into(),
            "--abbrev-ref".into(),
            "HEAD".into(),
        ],
        Some(base_dir),
        false,
        true,
    );
    if result.status == 0 && !result.stdout.trim().is_empty() {
        return Some(result.stdout.trim().to_string());
    }
    None
}

fn get_preferred_non_bare_worktree(base_dir: &Path) -> Option<PathBuf> {
    let default_branch = get_default_branch(base_dir);
    let entries = get_worktree_entries(base_dir);
    if let Some(default_branch) = default_branch {
        for entry in &entries {
            if !entry.is_bare && entry.branch.as_deref() == Some(default_branch.as_str()) {
                let candidate = path_resolve(&entry.path);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    for entry in entries {
        if !entry.is_bare {
            let candidate = path_resolve(&entry.path);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn get_wt_home_dir(base_dir: &Path) -> Option<PathBuf> {
    if !is_bare_repository(base_dir) {
        Some(base_dir.to_path_buf())
    } else {
        get_preferred_non_bare_worktree(base_dir)
    }
}

fn require_wt_home_dir(base_dir: &Path) -> PathBuf {
    if let Some(wt_home) = get_wt_home_dir(base_dir) {
        wt_home
    } else {
        eprintln!("{}", m1("error", m0("wt_home_not_found")));
        print_init_suggestion();
        std::process::exit(1);
    }
}

fn get_wt_dir(base_dir: &Path) -> PathBuf {
    require_wt_home_dir(base_dir).join(".wt")
}

fn ensure_base_worktree_for_bare(base_dir: &Path) -> PathBuf {
    if let Some(existing) = get_preferred_non_bare_worktree(base_dir) {
        return existing;
    }

    let default_branch = match get_default_branch_for_bare_git_dir(base_dir) {
        Some(branch) => branch,
        None => fatal(m1("error", m0("default_branch_not_found"))),
    };

    let stem = base_dir
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    let base_worktree_path = base_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(stem)
        .join(&default_branch);
    if base_worktree_path.exists() {
        fatal(m1(
            "error",
            m1("already_exists", base_worktree_path.display()),
        ));
    }
    eprintln!("{}", m1("creating_worktree", base_worktree_path.display()));
    run_command(
        vec![
            "git".into(),
            format!("--git-dir={}", base_dir.display()),
            "worktree".into(),
            "add".into(),
            base_worktree_path.display().to_string(),
            default_branch,
        ],
        None,
        true,
        false,
    );
    eprintln!("{}", m1("completed_worktree", base_worktree_path.display()));
    base_worktree_path
}

fn default_config() -> TomlValue {
    let mut root = TomlMap::new();
    root.insert(
        "worktrees_dir".into(),
        TomlValue::String(".worktrees".into()),
    );
    root.insert(
        "setup_files".into(),
        TomlValue::Array(vec![TomlValue::String(".env".into())]),
    );
    let mut diff = TomlMap::new();
    diff.insert("tool".into(), TomlValue::String("git".into()));
    root.insert("diff".into(), TomlValue::Table(diff));
    TomlValue::Table(root)
}

fn deep_merge(target: &mut TomlValue, source: TomlValue) {
    match (target, source) {
        (TomlValue::Table(target), TomlValue::Table(source)) => {
            for (key, value) in source {
                if let Some(existing) = target.get_mut(&key) {
                    deep_merge(existing, value);
                } else {
                    target.insert(key, value);
                }
            }
        }
        (target, source) => *target = source,
    }
}

fn resolve_templates(value: &mut TomlValue, repo_name: &str) {
    match value {
        TomlValue::String(s) if s.contains("{repo_name}") => {
            *s = s.replace("{repo_name}", repo_name);
        }
        TomlValue::Array(items) => {
            for item in items {
                resolve_templates(item, repo_name);
            }
        }
        TomlValue::Table(table) => {
            for (_, value) in table.iter_mut() {
                resolve_templates(value, repo_name);
            }
        }
        _ => {}
    }
}

fn load_config(base_dir: &Path) -> TomlValue {
    let mut config = default_config();
    let mut files = Vec::new();
    if let Some(wt_home) = get_wt_home_dir(base_dir) {
        let wt_dir = wt_home.join(".wt");
        files.push(wt_dir.join("config.toml"));
        files.push(wt_dir.join("config.local.toml"));
    }
    files.push(xdg_config_home().join("easy-worktree").join("config.toml"));

    for file in files {
        if file.exists() {
            match fs::read_to_string(&file)
                .ok()
                .and_then(|content| content.parse::<TomlValue>().ok())
            {
                Some(user_config) => deep_merge(&mut config, user_config),
                None => eprintln!(
                    "{}",
                    m1("error", format!("Failed to load config {}", file.display()))
                ),
            }
        }
    }

    let mut repo_name = base_dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if repo_name.ends_with(".git") {
        repo_name.truncate(repo_name.len() - 4);
    }
    if !repo_name.is_empty() {
        resolve_templates(&mut config, &repo_name);
    }
    config
}

fn config_get_str(config: &TomlValue, key: &str, default: &str) -> String {
    config
        .get(key)
        .and_then(TomlValue::as_str)
        .unwrap_or(default)
        .to_string()
}

fn config_setup_files(config: &TomlValue) -> Vec<String> {
    match config.get("setup_files") {
        Some(TomlValue::Array(items)) => items
            .iter()
            .filter_map(TomlValue::as_str)
            .map(str::to_string)
            .collect(),
        Some(TomlValue::String(item)) if !item.is_empty() => vec![item.clone()],
        _ => Vec::new(),
    }
}

fn config_diff_tool(config: &TomlValue) -> String {
    config
        .get("diff")
        .and_then(|v| v.get("tool"))
        .and_then(TomlValue::as_str)
        .unwrap_or("git")
        .to_string()
}

fn save_config_to_file(file_path: &Path, config_updates: TomlValue) {
    let mut config = if file_path.exists() {
        fs::read_to_string(file_path)
            .ok()
            .and_then(|content| content.parse::<TomlValue>().ok())
            .unwrap_or_else(|| TomlValue::Table(TomlMap::new()))
    } else {
        TomlValue::Table(TomlMap::new())
    };
    deep_merge(&mut config, config_updates);
    if let Some(parent) = file_path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            fatal_error(err);
        }
    }
    let rendered = toml::to_string_pretty(&config).unwrap_or_default();
    if let Err(err) = fs::write(file_path, rendered) {
        fatal_error(err);
    }
}

fn save_config(base_dir: &Path, config_updates: TomlValue) {
    let wt_dir = get_wt_dir(base_dir);
    if let Err(err) = fs::create_dir_all(&wt_dir) {
        fatal_error(err);
    }
    save_config_to_file(&wt_dir.join("config.toml"), config_updates);
}

fn metadata_created_now() -> String {
    Local::now()
        .naive_local()
        .format("%Y-%m-%dT%H:%M:%S%.6f")
        .to_string()
}

fn parse_datetime(value: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Local).naive_local());
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(value, fmt) {
            return Some(dt);
        }
    }
    None
}

fn get_metadata_file(base_dir: &Path) -> PathBuf {
    let mut app_dir = xdg_config_home().join("easy-worktree");
    if fs::create_dir_all(&app_dir).is_err() {
        app_dir = PathBuf::from("/tmp/easy-worktree");
        let _ = fs::create_dir_all(&app_dir);
    }

    let result = run_command(
        vec!["git".into(), "rev-parse".into(), "--git-common-dir".into()],
        Some(base_dir),
        false,
        true,
    );
    let git_common_dir = if result.status == 0 && !result.stdout.trim().is_empty() {
        let mut path = PathBuf::from(result.stdout.trim());
        if !path.is_absolute() {
            path = base_dir.join(path);
        }
        path_resolve(&path)
    } else {
        path_resolve(base_dir)
    };

    let mut hasher = Sha1::new();
    hasher.update(git_common_dir.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    app_dir.join(format!("worktree_metadata_{}.toml", &hex[..16]))
}

fn load_worktree_metadata(base_dir: &Path) -> TomlValue {
    let file = get_metadata_file(base_dir);
    if file.exists() {
        if let Ok(content) = fs::read_to_string(&file) {
            if let Ok(data) = content.parse::<TomlValue>() {
                if data
                    .get("worktrees")
                    .and_then(TomlValue::as_array)
                    .is_some()
                {
                    return data;
                }
            }
        }
    }
    let mut table = TomlMap::new();
    table.insert("worktrees".into(), TomlValue::Array(Vec::new()));
    TomlValue::Table(table)
}

fn save_worktree_metadata(base_dir: &Path, metadata: &TomlValue) {
    let file = get_metadata_file(base_dir);
    if let Some(parent) = file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let rendered = toml::to_string_pretty(metadata).unwrap_or_default();
    if let Err(err) = fs::write(file, rendered) {
        fatal_error(err);
    }
}

fn record_worktree_created(
    base_dir: &Path,
    worktree_path: &Path,
    created_at: Option<NaiveDateTime>,
) {
    create_hook_template(base_dir);
    let mut metadata = load_worktree_metadata(base_dir);
    let target = path_resolve(worktree_path).to_string_lossy().into_owned();
    let created_value = created_at
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6f").to_string())
        .unwrap_or_else(metadata_created_now);

    let table = metadata.as_table_mut().unwrap();
    let entry = table
        .entry("worktrees")
        .or_insert_with(|| TomlValue::Array(Vec::new()));
    let items = entry.as_array_mut().unwrap();
    for item in items.iter_mut() {
        if item.get("path").and_then(TomlValue::as_str) == Some(target.as_str()) {
            if item.get("created_at").is_none() {
                if let Some(item_table) = item.as_table_mut() {
                    item_table.insert("created_at".into(), TomlValue::String(created_value));
                    save_worktree_metadata(base_dir, &metadata);
                }
            }
            return;
        }
    }

    let mut item = TomlMap::new();
    item.insert("path".into(), TomlValue::String(target));
    item.insert("created_at".into(), TomlValue::String(created_value));
    items.push(TomlValue::Table(item));
    save_worktree_metadata(base_dir, &metadata);
}

fn get_recorded_worktree_created(base_dir: &Path, worktree_path: &Path) -> Option<NaiveDateTime> {
    let metadata = load_worktree_metadata(base_dir);
    let target = path_resolve(worktree_path).to_string_lossy().into_owned();
    for item in metadata
        .get("worktrees")
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
    {
        if item.get("path").and_then(TomlValue::as_str) == Some(target.as_str()) {
            return item
                .get("created_at")
                .and_then(TomlValue::as_str)
                .and_then(parse_datetime);
        }
    }
    None
}

fn remove_worktree_metadata(base_dir: &Path, worktree_path: &Path) {
    let mut metadata = load_worktree_metadata(base_dir);
    let target = path_resolve(worktree_path).to_string_lossy().into_owned();
    if let Some(items) = metadata
        .get_mut("worktrees")
        .and_then(TomlValue::as_array_mut)
    {
        let before = items.len();
        items.retain(|item| item.get("path").and_then(TomlValue::as_str) != Some(target.as_str()));
        if items.len() != before {
            save_worktree_metadata(base_dir, &metadata);
        }
    }
}

fn default_config_updates() -> TomlValue {
    let mut root = TomlMap::new();
    root.insert(
        "worktrees_dir".into(),
        TomlValue::String(".worktrees".into()),
    );
    root.insert(
        "setup_files".into(),
        TomlValue::Array(vec![TomlValue::String(".env".into())]),
    );
    TomlValue::Table(root)
}

fn create_hook_template(base_dir: &Path) {
    let wt_home = require_wt_home_dir(base_dir);
    let wt_dir = wt_home.join(".wt");
    if let Err(err) = fs::create_dir_all(&wt_dir) {
        fatal_error(err);
    }

    let config_file = wt_dir.join("config.toml");
    if !config_file.exists() {
        save_config(base_dir, default_config_updates());
    }

    let config = load_config(base_dir);
    let worktrees_dir_name = config_get_str(&config, "worktrees_dir", ".worktrees");
    let root_gitignore = wt_home.join(".gitignore");
    let entries = [".wt/".to_string(), format!("{worktrees_dir_name}/")];
    if root_gitignore.exists() {
        let mut content = fs::read_to_string(&root_gitignore).unwrap_or_default();
        let mut updated = false;
        for entry in entries {
            if !content.contains(&entry) {
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(&entry);
                content.push('\n');
                updated = true;
            }
        }
        if updated {
            let _ = fs::write(&root_gitignore, content);
        }
    } else {
        let _ = fs::write(&root_gitignore, format!("{}\n{}\n", entries[0], entries[1]));
    }

    let hook_file = wt_dir.join("post-add");
    if !hook_file.exists() {
        let _ = fs::write(&hook_file, POST_ADD_TEMPLATE);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&hook_file, fs::Permissions::from_mode(0o755));
        }
    }

    let wt_gitignore = wt_dir.join(".gitignore");
    let ignores = ["post-add.local", "config.local.toml", "last_selection"];
    if wt_gitignore.exists() {
        let mut content = fs::read_to_string(&wt_gitignore).unwrap_or_default();
        let mut updated = false;
        for ignore in ignores {
            if !content.contains(ignore) {
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(ignore);
                content.push('\n');
                updated = true;
            }
        }
        if updated {
            let _ = fs::write(&wt_gitignore, content);
        }
    } else {
        let _ = fs::write(&wt_gitignore, ignores.join("\n") + "\n");
    }

    let readme_file = wt_dir.join("README.md");
    if !readme_file.exists() {
        let readme = if is_japanese() {
            "# easy-worktree フック\n\nこのディレクトリには easy-worktree の設定と post-add フックが格納されています。\n"
        } else {
            "# easy-worktree Hooks\n\nThis directory contains easy-worktree configuration and post-add hooks.\n"
        };
        let _ = fs::write(readme_file, readme);
    }
}

fn resolve_setup_source_dir(
    base_dir: &Path,
    target_path: &Path,
    config: &TomlValue,
) -> Option<PathBuf> {
    if let Some(configured_source) = config.get("setup_source_dir").and_then(TomlValue::as_str) {
        if !configured_source.is_empty() {
            let source = PathBuf::from(configured_source);
            return Some(if source.is_absolute() {
                source
            } else {
                path_resolve(&base_dir.join(source))
            });
        }
    }

    if !is_bare_repository(base_dir) {
        return Some(base_dir.to_path_buf());
    }

    let resolved_target = path_resolve(target_path);
    if let Some(preferred) = get_preferred_non_bare_worktree(base_dir) {
        if preferred != resolved_target {
            return Some(preferred);
        }
    }

    for entry in get_worktree_entries(base_dir) {
        if entry.is_bare {
            continue;
        }
        let candidate = path_resolve(&entry.path);
        if candidate.exists() && candidate != resolved_target {
            return Some(candidate);
        }
    }
    None
}

fn copy_setup_files(
    base_dir: &Path,
    target_path: &Path,
    setup_files: &[String],
    config: &TomlValue,
) -> usize {
    let Some(source_dir) = resolve_setup_source_dir(base_dir, target_path, config) else {
        eprintln!("{}", m0("setup_source_not_found"));
        return 0;
    };
    if !source_dir.exists() {
        eprintln!("{}", m0("setup_source_not_found"));
        return 0;
    }

    eprintln!("{}", m1("using_setup_source", source_dir.display()));
    let mut count = 0;
    for file_name in setup_files {
        let src = source_dir.join(file_name);
        let dst = target_path.join(file_name);
        if src.exists() && path_resolve(&src) != path_resolve(&dst) {
            eprintln!("{}", m2("setting_up", src.display(), dst.display()));
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if fs::copy(&src, &dst).is_ok() {
                count += 1;
            }
        }
    }
    count
}

fn run_post_add_hook(worktree_path: &Path, work_name: &str, base_dir: &Path, branch: Option<&str>) {
    let hook_path = get_wt_dir(base_dir).join("post-add");
    if !hook_path.is_file() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&hook_path)
            .map(|metadata| metadata.permissions().mode() & 0o111 == 0)
            .unwrap_or(true)
        {
            eprintln!("{}", m1("hook_not_executable", hook_path.display()));
            return;
        }
    }

    eprintln!("{}", m1("running_hook", hook_path.display()));
    let status = Command::new(&hook_path)
        .current_dir(worktree_path)
        .env("WT_WORKTREE_PATH", worktree_path)
        .env("WT_WORKTREE_NAME", work_name)
        .env("WT_BASE_DIR", base_dir)
        .env("WT_BRANCH", branch.unwrap_or(work_name))
        .env("WT_ACTION", "add")
        .output();
    match status {
        Ok(output) => {
            let _ = io::stderr().write_all(&output.stdout);
            let _ = io::stderr().write_all(&output.stderr);
            if !output.status.success() {
                eprintln!("{}", m1("hook_failed", output.status.code().unwrap_or(1)));
            }
        }
        Err(err) => eprintln!("{}", m1("error", err)),
    }
}

fn cmd_clone(args: &[String]) {
    if args.is_empty() {
        eprintln!("{}", m0("usage"));
        std::process::exit(1);
    }
    let mut bare_mode = false;
    let mut clean_args = Vec::new();
    for arg in args {
        if arg == "--bare" {
            bare_mode = true;
        } else {
            clean_args.push(arg.clone());
        }
    }
    if clean_args.is_empty() {
        eprintln!("{}", m0("usage"));
        std::process::exit(1);
    }

    let repo_url = clean_args[0].clone();
    let repo_name = get_repository_name(&repo_url);
    let dest_dir = if clean_args.len() > 1 {
        PathBuf::from(&clean_args[1])
    } else if bare_mode {
        PathBuf::from(format!("{repo_name}.git"))
    } else {
        PathBuf::from(repo_name)
    };
    if dest_dir.exists() {
        fatal(m1("error", m1("already_exists", dest_dir.display())));
    }

    eprintln!("{}", m2("cloning", &repo_url, dest_dir.display()));
    let mut clone_cmd = vec!["git".into(), "clone".into()];
    if bare_mode {
        clone_cmd.push("--bare".into());
    }
    clone_cmd.push(repo_url);
    clone_cmd.push(dest_dir.display().to_string());
    run_command(clone_cmd, None, true, false);
    eprintln!("{}", m1("completed_clone", dest_dir.display()));

    if bare_mode {
        ensure_base_worktree_for_bare(&dest_dir);
    }
    create_hook_template(&dest_dir);
}

fn cmd_init(_args: &[String]) {
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("not_git_repo")));
    };
    if is_bare_repository(&base_dir) {
        ensure_base_worktree_for_bare(&base_dir);
    }
    create_hook_template(&base_dir);
}

fn add_worktree(
    work_name: &str,
    branch_to_use: Option<&str>,
    new_branch_base: Option<&str>,
    base_dir: Option<PathBuf>,
    skip_setup: bool,
) -> PathBuf {
    let base_dir = match base_dir.or_else(find_base_dir) {
        Some(path) => path,
        None => {
            eprintln!("{}", m1("error", m0("base_not_found")));
            eprintln!("{}", m0("run_in_wt_dir"));
            std::process::exit(1);
        }
    };

    let config = load_config(&base_dir);
    let is_bare = is_bare_repository(&base_dir);
    let worktrees_dir_name = config_get_str(&config, "worktrees_dir", ".worktrees");
    let worktrees_dir = if is_bare {
        let wt_home = require_wt_home_dir(&base_dir);
        let base_parent = wt_home.parent().unwrap_or_else(|| Path::new("."));
        if worktrees_dir_name.is_empty() {
            base_parent.to_path_buf()
        } else {
            let dir = base_parent.join(&worktrees_dir_name);
            let _ = fs::create_dir_all(&dir);
            dir
        }
    } else {
        let dir = base_dir.join(&worktrees_dir_name);
        let _ = fs::create_dir_all(&dir);
        dir
    };

    let worktree_path = worktrees_dir.join(work_name);
    if worktree_path.exists() {
        fatal(m1("error", m1("already_exists", worktree_path.display())));
    }

    eprintln!("{}", m0("fetching"));
    run_command(
        vec!["git".into(), "fetch".into(), "--all".into()],
        Some(&base_dir),
        true,
        true,
    );

    let current_branch = run_command(
        vec![
            "git".into(),
            "rev-parse".into(),
            "--abbrev-ref".into(),
            "HEAD".into(),
        ],
        Some(&base_dir),
        false,
        true,
    );
    if current_branch.status == 0 {
        let branch = current_branch.stdout.trim();
        let origin = run_command(
            vec![
                "git".into(),
                "rev-parse".into(),
                "--verify".into(),
                format!("origin/{branch}"),
            ],
            Some(&base_dir),
            false,
            true,
        );
        if origin.status == 0 {
            run_command(
                vec!["git".into(), "pull".into(), "origin".into(), branch.into()],
                Some(&base_dir),
                false,
                true,
            );
        }
    }

    let mut final_branch_name: String;
    let result = if let Some(new_branch_base) = new_branch_base {
        final_branch_name = work_name.to_string();
        eprintln!(
            "{}",
            m2("creating_branch", &final_branch_name, new_branch_base)
        );
        run_command(
            vec![
                "git".into(),
                "worktree".into(),
                "add".into(),
                "-b".into(),
                final_branch_name.clone(),
                worktree_path.display().to_string(),
                new_branch_base.into(),
            ],
            Some(&base_dir),
            false,
            true,
        )
    } else if let Some(branch_to_use) = branch_to_use {
        final_branch_name = branch_to_use.to_string();
        if branch_to_use.contains('/') {
            let mut split = branch_to_use.splitn(2, '/');
            let remote_name = split.next().unwrap_or("");
            let short_branch_name = split.next().unwrap_or("");
            let remotes = run_command(
                vec!["git".into(), "remote".into()],
                Some(&base_dir),
                false,
                true,
            );
            if remotes.status == 0 && remotes.stdout.split_whitespace().any(|r| r == remote_name) {
                let check_local = run_command(
                    vec![
                        "git".into(),
                        "rev-parse".into(),
                        "--verify".into(),
                        short_branch_name.into(),
                    ],
                    Some(&base_dir),
                    false,
                    true,
                );
                final_branch_name = short_branch_name.to_string();
                eprintln!("{}", m1("creating_worktree", worktree_path.display()));
                if check_local.status != 0 {
                    eprintln!(
                        "Creating tracking branch {} for {}...",
                        final_branch_name, branch_to_use
                    );
                    run_command(
                        vec![
                            "git".into(),
                            "worktree".into(),
                            "add".into(),
                            "-b".into(),
                            final_branch_name.clone(),
                            worktree_path.display().to_string(),
                            branch_to_use.into(),
                        ],
                        Some(&base_dir),
                        false,
                        true,
                    )
                } else {
                    eprintln!(
                        "Local branch {} already exists. Using it instead of {}.",
                        final_branch_name, branch_to_use
                    );
                    run_command(
                        vec![
                            "git".into(),
                            "worktree".into(),
                            "add".into(),
                            worktree_path.display().to_string(),
                            final_branch_name.clone(),
                        ],
                        Some(&base_dir),
                        false,
                        true,
                    )
                }
            } else {
                eprintln!("{}", m1("creating_worktree", worktree_path.display()));
                run_command(
                    vec![
                        "git".into(),
                        "worktree".into(),
                        "add".into(),
                        worktree_path.display().to_string(),
                        final_branch_name.clone(),
                    ],
                    Some(&base_dir),
                    false,
                    true,
                )
            }
        } else {
            eprintln!("{}", m1("creating_worktree", worktree_path.display()));
            run_command(
                vec![
                    "git".into(),
                    "worktree".into(),
                    "add".into(),
                    worktree_path.display().to_string(),
                    final_branch_name.clone(),
                ],
                Some(&base_dir),
                false,
                true,
            )
        }
    } else {
        final_branch_name = work_name.to_string();
        let check_local = run_command(
            vec![
                "git".into(),
                "rev-parse".into(),
                "--verify".into(),
                final_branch_name.clone(),
            ],
            Some(&base_dir),
            false,
            true,
        );
        let check_remote = run_command(
            vec![
                "git".into(),
                "rev-parse".into(),
                "--verify".into(),
                format!("origin/{final_branch_name}"),
            ],
            Some(&base_dir),
            false,
            true,
        );
        if check_local.status == 0 || check_remote.status == 0 {
            eprintln!("{}", m1("creating_worktree", worktree_path.display()));
            if check_local.status == 0 {
                run_command(
                    vec![
                        "git".into(),
                        "worktree".into(),
                        "add".into(),
                        worktree_path.display().to_string(),
                        final_branch_name.clone(),
                    ],
                    Some(&base_dir),
                    false,
                    true,
                )
            } else {
                eprintln!(
                    "Creating tracking branch {} for origin/{}...",
                    final_branch_name, final_branch_name
                );
                run_command(
                    vec![
                        "git".into(),
                        "worktree".into(),
                        "add".into(),
                        "-b".into(),
                        final_branch_name.clone(),
                        worktree_path.display().to_string(),
                        format!("origin/{final_branch_name}"),
                    ],
                    Some(&base_dir),
                    false,
                    true,
                )
            }
        } else {
            let sym = run_command(
                vec![
                    "git".into(),
                    "symbolic-ref".into(),
                    "refs/remotes/origin/HEAD".into(),
                    "--short".into(),
                ],
                Some(&base_dir),
                false,
                true,
            );
            let mut detected_base = if sym.status == 0 && !sym.stdout.trim().is_empty() {
                Some(sym.stdout.trim().to_string())
            } else {
                None
            };
            if detected_base.is_none() {
                for branch in ["origin/main", "origin/master", "main", "master"] {
                    let result = run_command(
                        vec![
                            "git".into(),
                            "rev-parse".into(),
                            "--verify".into(),
                            branch.into(),
                        ],
                        Some(&base_dir),
                        false,
                        true,
                    );
                    if result.status == 0 {
                        detected_base = Some(branch.to_string());
                        break;
                    }
                }
            }
            if detected_base.is_none() {
                let current = run_command(
                    vec![
                        "git".into(),
                        "rev-parse".into(),
                        "--abbrev-ref".into(),
                        "HEAD".into(),
                    ],
                    Some(&base_dir),
                    false,
                    true,
                );
                if current.status == 0 && !current.stdout.trim().is_empty() {
                    detected_base = Some(current.stdout.trim().to_string());
                }
            }
            let Some(detected_base) = detected_base else {
                fatal(m1("error", m0("default_branch_not_found")));
            };
            eprintln!(
                "{}",
                m2("creating_branch", &final_branch_name, &detected_base)
            );
            run_command(
                vec![
                    "git".into(),
                    "worktree".into(),
                    "add".into(),
                    "-b".into(),
                    final_branch_name.clone(),
                    worktree_path.display().to_string(),
                    detected_base,
                ],
                Some(&base_dir),
                false,
                true,
            )
        }
    };

    if result.status == 0 {
        record_worktree_created(&base_dir, &worktree_path, None);
        if !skip_setup {
            let setup_files = config_setup_files(&config);
            copy_setup_files(&base_dir, &worktree_path, &setup_files, &config);
            run_post_add_hook(
                &worktree_path,
                work_name,
                &base_dir,
                Some(&final_branch_name),
            );
        }
        worktree_path
    } else {
        if !result.stderr.is_empty() {
            eprint!("{}", result.stderr);
        }
        std::process::exit(1);
    }
}

fn cmd_add(args: &[String]) {
    if args.is_empty() {
        eprintln!("{}", m0("usage_add"));
        std::process::exit(1);
    }
    let mut clean_args = Vec::new();
    let mut skip_setup = false;
    let mut select = false;
    let mut select_command: Option<Vec<String>> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--skip-setup" || arg == "--no-setup" {
            skip_setup = true;
        } else if arg == "--select" {
            select = true;
            if i + 1 < args.len() {
                select_command = Some(args[i + 1..].to_vec());
            }
            break;
        } else {
            clean_args.push(arg.clone());
        }
        i += 1;
    }
    if clean_args.is_empty() {
        if let Some(command) = select_command.as_mut() {
            if !command.is_empty() {
                clean_args.push(command.remove(0));
                if command.is_empty() {
                    select_command = None;
                }
            }
        }
    }
    if clean_args.is_empty() {
        eprintln!("{}", m0("usage_add"));
        std::process::exit(1);
    }
    let work_name = clean_args[0].clone();
    let branch_to_use = clean_args.get(1).map(String::as_str);
    let base_dir = find_base_dir();
    let wt_path = add_worktree(
        &work_name,
        branch_to_use,
        None,
        base_dir.clone(),
        skip_setup,
    );
    if select {
        let base_dir = base_dir.unwrap_or_else(|| {
            find_base_dir().unwrap_or_else(|| fatal(m1("error", m0("base_not_found"))))
        });
        create_hook_template(&base_dir);
        let wt_dir = get_wt_dir(&base_dir);
        let last_sel_file = wt_dir.join("last_selection");
        let mut current_sel = env::var("WT_SESSION_NAME").ok();
        if current_sel.is_none() {
            if let Ok(cwd) = env::current_dir() {
                let resolved_base = path_resolve(&base_dir);
                for wt in get_worktree_info(&base_dir) {
                    let p = path_resolve(&wt.path);
                    if path_starts_with(&cwd, &p) {
                        current_sel = Some(if p == resolved_base {
                            "main".to_string()
                        } else {
                            p.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        });
                        break;
                    }
                }
            }
        }
        let _ = wt_path;
        switch_selection(
            &work_name,
            &base_dir,
            current_sel.as_deref(),
            &last_sel_file,
            select_command,
        );
    }
}

fn cmd_stash(args: &[String]) {
    if args.is_empty() {
        eprintln!("{}", m0("usage_stash"));
        std::process::exit(1);
    }
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    let status = run_command(
        vec!["git".into(), "status".into(), "--porcelain".into()],
        Some(&base_dir),
        false,
        true,
    );
    let has_changes = !status.stdout.trim().is_empty();
    if has_changes {
        eprintln!("{}", m0("stashing_changes"));
        run_command(
            vec![
                "git".into(),
                "stash".into(),
                "push".into(),
                "-u".into(),
                "-m".into(),
                format!("easy-worktree stash for {}", args[0]),
            ],
            Some(&base_dir),
            true,
            true,
        );
    } else {
        eprintln!("{}", m0("nothing_to_stash"));
    }

    let mut new_branch_base = args.get(1).cloned();
    if new_branch_base.is_none() {
        let current = run_command(
            vec![
                "git".into(),
                "rev-parse".into(),
                "--abbrev-ref".into(),
                "HEAD".into(),
            ],
            Some(&base_dir),
            false,
            true,
        );
        if current.status == 0 {
            new_branch_base = Some(current.stdout.trim().to_string());
        }
    }
    let wt_path = add_worktree(
        &args[0],
        None,
        new_branch_base.as_deref(),
        Some(base_dir),
        false,
    );
    if has_changes {
        eprintln!("{}", m0("popping_stash"));
        run_command(
            vec!["git".into(), "stash".into(), "pop".into()],
            Some(&wt_path),
            true,
            false,
        );
    }
}

fn cmd_pr(args: &[String]) {
    if args.len() < 2 {
        eprintln!("{}", m0("usage_pr"));
        std::process::exit(1);
    }
    let subcommand = &args[0];
    let pr_number = &args[1];
    if !pr_number.chars().all(|c| c.is_ascii_digit()) {
        fatal(m1(
            "error",
            format!("PR number must be a digit: {pr_number}"),
        ));
    }
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    if subcommand == "add" {
        if which_cmd("gh").is_none() {
            fatal(m1("error", "GitHub CLI (gh) is required for this command"));
        }
        eprintln!("Verifying PR #{}...", pr_number);
        let result = run_command(
            vec![
                "gh".into(),
                "pr".into(),
                "view".into(),
                pr_number.clone(),
                "--json".into(),
                "number".into(),
            ],
            Some(&base_dir),
            false,
            true,
        );
        if result.status != 0 {
            fatal(m1(
                "error",
                format!("PR #{pr_number} not found (or access denied)"),
            ));
        }
        let branch_name = format!("pr-{pr_number}");
        let worktree_name = format!("pr@{pr_number}");
        eprintln!("Fetching PR #{} contents...", pr_number);
        run_command(
            vec![
                "git".into(),
                "fetch".into(),
                "origin".into(),
                format!("pull/{pr_number}/head:{branch_name}"),
            ],
            Some(&base_dir),
            true,
            true,
        );
        eprintln!("Creating worktree {}...", worktree_name);
        add_worktree(
            &worktree_name,
            Some(&branch_name),
            None,
            Some(base_dir),
            false,
        );
    } else if subcommand == "co" {
        cmd_checkout(&[format!("pr@{pr_number}")]);
    } else {
        eprintln!("{}", m0("usage_pr"));
        std::process::exit(1);
    }
}

fn get_worktree_info(base_dir: &Path) -> Vec<WorktreeInfo> {
    let result = run_command(
        vec![
            "git".into(),
            "worktree".into(),
            "list".into(),
            "--porcelain".into(),
        ],
        Some(base_dir),
        true,
        true,
    );
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in result.stdout.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(path) = current_path.take() {
                worktrees.push(WorktreeInfo {
                    path,
                    head: current_head.take(),
                    branch: current_branch.take().unwrap_or_else(|| "N/A".into()),
                    created: None,
                    last_commit: None,
                    is_clean: false,
                    has_untracked: false,
                    insertions: 0,
                    deletions: 0,
                    pr_info: String::new(),
                    reason: String::new(),
                });
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            current_head = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("branch ") {
            current_branch = Some(rest.strip_prefix("refs/heads/").unwrap_or(rest).to_string());
        } else if line.starts_with("detached") {
            current_branch = Some("DETACHED".into());
        }
    }

    for wt in &mut worktrees {
        let mut created = get_recorded_worktree_created(base_dir, &wt.path);
        if created.is_none() && wt.path.exists() {
            if let Ok(metadata) = fs::metadata(&wt.path) {
                let sys_time = metadata.created().or_else(|_| metadata.modified()).ok();
                if let Some(sys_time) = sys_time {
                    let dt: DateTime<Local> = sys_time.into();
                    created = Some(dt.naive_local());
                    record_worktree_created(base_dir, &wt.path, created);
                }
            }
        }
        wt.created = created;

        let head = wt.head.clone().unwrap_or_else(|| "HEAD".into());
        let log = run_command(
            vec![
                "git".into(),
                "log".into(),
                "-1".into(),
                "--format=%ct".into(),
                head,
            ],
            Some(base_dir),
            false,
            true,
        );
        if log.status == 0 {
            if let Ok(ts) = log.stdout.trim().parse::<i64>() {
                wt.last_commit = Local
                    .timestamp_opt(ts, 0)
                    .single()
                    .map(|dt| dt.naive_local());
            }
        }

        let status = run_command(
            vec!["git".into(), "status".into(), "--porcelain".into()],
            Some(&wt.path),
            false,
            false,
        );
        wt.is_clean = status.status == 0 && status.stdout.trim().is_empty();
        wt.has_untracked = status.stdout.contains("??");

        let diff = run_command(
            vec![
                "git".into(),
                "diff".into(),
                "HEAD".into(),
                "--shortstat".into(),
            ],
            Some(&wt.path),
            false,
            false,
        );
        if diff.status == 0 {
            let out = diff.stdout.trim();
            wt.insertions = parse_shortstat_count(out, "insertion");
            wt.deletions = parse_shortstat_count(out, "deletion");
        }
    }
    worktrees
}

fn parse_shortstat_count(text: &str, label: &str) -> usize {
    let words: Vec<&str> = text.split_whitespace().collect();
    for window in words.windows(2) {
        if window[1].starts_with(label) {
            if let Ok(value) = window[0].parse() {
                return value;
            }
        }
    }
    0
}

fn get_relative_time(dt: Option<NaiveDateTime>) -> String {
    let Some(dt) = dt else {
        return "N/A".into();
    };
    let now = Local::now().naive_local();
    let diff = now.signed_duration_since(dt);
    let seconds = diff.num_seconds();
    let days = diff.num_days();
    if days < 0 {
        return "just now".into();
    }
    if days == 0 {
        if seconds < 60 {
            return "just now".into();
        }
        if seconds < 3600 {
            return format!("{}m ago", seconds / 60);
        }
        return format!("{}h ago", seconds / 3600);
    }
    if days == 1 {
        return "yesterday".into();
    }
    if days < 30 {
        return format!("{days}d ago");
    }
    if days < 365 {
        return format!("{}mo ago", days / 30);
    }
    format!("{}y ago", days / 365)
}

fn get_pr_info(branch: &str, cwd: &Path) -> String {
    if branch.is_empty() || branch == "HEAD" || branch == "DETACHED" || which_cmd("gh").is_none() {
        return String::new();
    }
    let result = run_command(
        vec![
            "gh".into(),
            "pr".into(),
            "list".into(),
            "--head".into(),
            branch.into(),
            "--state".into(),
            "all".into(),
            "--json".into(),
            "state,isDraft,url,createdAt,number".into(),
        ],
        Some(cwd),
        false,
        true,
    );
    if result.status != 0 || result.stdout.trim().is_empty() {
        return String::new();
    }
    let Ok(JsonValue::Array(prs)) = serde_json::from_str::<JsonValue>(&result.stdout) else {
        return String::new();
    };
    let Some(pr) = prs.first() else {
        return String::new();
    };
    let state = pr.get("state").and_then(JsonValue::as_str).unwrap_or("");
    let is_draft = pr
        .get("isDraft")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let url = pr.get("url").and_then(JsonValue::as_str).unwrap_or("");
    let number = pr
        .get("number")
        .and_then(JsonValue::as_i64)
        .unwrap_or_default();
    let rel_time = pr
        .get("createdAt")
        .and_then(JsonValue::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| get_relative_time(Some(dt.with_timezone(&Local).naive_local())))
        .unwrap_or_else(|| "N/A".into());

    let green = "\x1b[32m";
    let gray = "\x1b[90m";
    let magenta = "\x1b[35m";
    let red = "\x1b[31m";
    let reset = "\x1b[0m";
    let symbol = if is_draft {
        format!("{gray}◌{reset}")
    } else if state == "OPEN" {
        format!("{green}●{reset}")
    } else if state == "MERGED" {
        format!("{magenta}✔{reset}")
    } else {
        format!("{red}✘{reset}")
    };
    format!("{symbol} \x1b]8;;{url}\x1b\\#{number}\x1b]8;;\x1b\\ ({rel_time})")
}

fn parse_clean_filter_options(args: &[String]) -> (bool, bool, bool, Option<i64>) {
    let clean_all = args.iter().any(|a| a == "--all");
    let clean_merged = args.iter().any(|a| a == "--merged");
    let clean_closed = args.iter().any(|a| a == "--closed");
    let mut days = None;
    for (idx, arg) in args.iter().enumerate() {
        if arg == "--days" && idx + 1 < args.len() {
            match args[idx + 1].parse::<i64>() {
                Ok(value) => days = Some(value),
                Err(_) => fatal_error("Invalid days value"),
            }
        }
    }
    (clean_all, clean_merged, clean_closed, days)
}

fn resolve_clean_targets(
    base_dir: &Path,
    worktrees: &[WorktreeInfo],
    args: &[String],
) -> Vec<WorktreeInfo> {
    let (clean_all, clean_merged, clean_closed, days) = parse_clean_filter_options(args);
    let mut aliased_worktrees = HashSet::new();
    if let Ok(entries) = fs::read_dir(base_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false) {
                if let Ok(target) = fs::canonicalize(entry.path()) {
                    aliased_worktrees.insert(target);
                }
            }
        }
    }

    let mut merged_branches = HashSet::new();
    let mut merged_pr_branches = HashSet::new();
    let default_branch = get_default_branch(base_dir);
    let mut default_branch_sha = None;
    if let Some(default_branch) = &default_branch {
        let sha = run_command(
            vec!["git".into(), "rev-parse".into(), default_branch.clone()],
            Some(base_dir),
            false,
            true,
        );
        if sha.status == 0 {
            default_branch_sha = Some(sha.stdout.trim().to_string());
        }
    }

    if clean_merged {
        if let Some(default_branch) = &default_branch {
            let result = run_command(
                vec![
                    "git".into(),
                    "branch".into(),
                    "--merged".into(),
                    default_branch.clone(),
                ],
                Some(base_dir),
                false,
                true,
            );
            if result.status == 0 {
                for line in result.stdout.lines() {
                    let mut line = line.trim().to_string();
                    if line.starts_with("* ") || line.starts_with("+ ") {
                        line = line[2..].trim().to_string();
                    }
                    if !line.is_empty() {
                        merged_branches.insert(line);
                    }
                }
            }
        }
        if which_cmd("gh").is_some() {
            let result = run_command(
                vec![
                    "gh".into(),
                    "pr".into(),
                    "list".into(),
                    "--state".into(),
                    "merged".into(),
                    "--limit".into(),
                    "100".into(),
                    "--json".into(),
                    "headRefName".into(),
                ],
                Some(base_dir),
                false,
                true,
            );
            if result.status == 0 {
                if let Ok(JsonValue::Array(items)) =
                    serde_json::from_str::<JsonValue>(&result.stdout)
                {
                    for item in items {
                        if let Some(name) = item.get("headRefName").and_then(JsonValue::as_str) {
                            merged_pr_branches.insert(name.to_string());
                        }
                    }
                }
            }
        }
    }

    let mut closed_pr_branches = HashSet::new();
    if clean_closed && which_cmd("gh").is_some() {
        let result = run_command(
            vec![
                "gh".into(),
                "pr".into(),
                "list".into(),
                "--state".into(),
                "closed".into(),
                "--limit".into(),
                "100".into(),
                "--json".into(),
                "headRefName".into(),
            ],
            Some(base_dir),
            false,
            true,
        );
        if result.status == 0 {
            if let Ok(JsonValue::Array(items)) = serde_json::from_str::<JsonValue>(&result.stdout) {
                for item in items {
                    if let Some(name) = item.get("headRefName").and_then(JsonValue::as_str) {
                        closed_pr_branches.insert(name.to_string());
                    }
                }
            }
        }
    }

    let now = Local::now().naive_local();
    let mut targets = Vec::new();
    for wt in worktrees {
        let path = path_resolve(&wt.path);
        if same_path(&path, base_dir) || aliased_worktrees.contains(&path) {
            continue;
        }
        let mut reason = None;
        let is_merged =
            merged_branches.contains(&wt.branch) || merged_pr_branches.contains(&wt.branch);
        if clean_merged && is_merged {
            if let Some(default_sha) = &default_branch_sha {
                if !merged_pr_branches.contains(&wt.branch) {
                    let wt_sha = run_command(
                        vec!["git".into(), "rev-parse".into(), wt.branch.clone()],
                        Some(base_dir),
                        false,
                        true,
                    )
                    .stdout
                    .trim()
                    .to_string();
                    if &wt_sha == default_sha {
                        continue;
                    }
                }
            }
            if wt.is_clean {
                reason = Some("merged".to_string());
            }
        }
        if reason.is_none()
            && clean_closed
            && closed_pr_branches.contains(&wt.branch)
            && wt.is_clean
        {
            reason = Some("closed".to_string());
        }
        if reason.is_none() && wt.is_clean {
            if let Some(days) = days {
                if let Some(created) = wt.created {
                    if now.signed_duration_since(created).num_days() >= days {
                        reason = Some(format!("older than {days} days"));
                    }
                }
            } else if clean_all {
                reason = Some("clean".to_string());
            }
        }
        if let Some(reason) = reason {
            let mut item = wt.clone();
            item.reason = reason;
            targets.push(item);
        }
    }
    targets
}

fn sort_worktrees(worktrees: &mut [WorktreeInfo], sort_key: &str, descending: bool) {
    match sort_key {
        "last-commit" => worktrees.sort_by_key(|wt| wt.last_commit),
        "name" => worktrees.sort_by_key(|wt| {
            wt.path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
        }),
        "branch" => worktrees.sort_by_key(|wt| wt.branch.to_lowercase()),
        _ => worktrees.sort_by_key(|wt| wt.created),
    }
    if descending {
        worktrees.reverse();
    }
}

fn get_worktree_names(base_dir: &Path) -> Vec<String> {
    get_worktree_info(base_dir)
        .into_iter()
        .map(|wt| {
            if same_path(&wt.path, base_dir) {
                "main".into()
            } else {
                wt.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            }
        })
        .collect()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut costs: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut last = i;
        costs[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let old = costs[j + 1];
            costs[j + 1] = if ca == *cb {
                last
            } else {
                1 + last.min(costs[j]).min(costs[j + 1])
            };
            last = old;
        }
    }
    *costs.last().unwrap_or(&0)
}

fn suggest_worktree_name(base_dir: &Path, typed_name: &str) {
    let names = get_worktree_names(base_dir);
    if names.is_empty() {
        return;
    }
    let mut scored: Vec<(usize, String)> = names
        .iter()
        .map(|name| (levenshtein(typed_name, name), name.clone()))
        .filter(|(dist, name)| {
            let max_len = typed_name.chars().count().max(name.chars().count()).max(1);
            (*dist as f64 / max_len as f64) <= 0.6
        })
        .collect();
    scored.sort_by_key(|(dist, _)| *dist);
    if scored.is_empty() {
        eprintln!("{}", m1("available_worktrees", names.join(", ")));
    } else {
        let matches = scored
            .into_iter()
            .take(3)
            .map(|(_, name)| name)
            .collect::<Vec<_>>();
        eprintln!("{}", m1("did_you_mean", matches.join(", ")));
    }
}

fn cmd_list(args: &[String]) {
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("{}", m0("usage_list"));
        return;
    }
    let quiet = args.iter().any(|a| a == "--quiet" || a == "-q");
    let show_pr = args.iter().any(|a| a == "--pr");
    let mut sort_key = "created".to_string();
    if let Some(i) = args.iter().position(|a| a == "--sort") {
        if i + 1 >= args.len() {
            eprintln!("{}", m0("usage_list"));
            std::process::exit(1);
        }
        sort_key = args[i + 1].clone();
        if !["created", "last-commit", "name", "branch"].contains(&sort_key.as_str()) {
            eprintln!("{}", m1("error", format!("Invalid sort key: {sort_key}")));
            eprintln!("{}", m0("usage_list"));
            std::process::exit(1);
        }
    }
    let mut descending = true;
    if args.iter().any(|a| a == "--asc") {
        descending = false;
    }
    if args.iter().any(|a| a == "--desc") {
        descending = true;
    }

    let mut worktrees = get_worktree_info(&base_dir);
    let (clean_all, clean_merged, clean_closed, days) = parse_clean_filter_options(args);
    if clean_all || clean_merged || clean_closed || days.is_some() {
        worktrees = resolve_clean_targets(&base_dir, &worktrees, args);
    }
    sort_worktrees(&mut worktrees, &sort_key, descending);

    if show_pr {
        for wt in &mut worktrees {
            wt.pr_info = get_pr_info(&wt.branch, &base_dir);
        }
    }

    if quiet {
        for wt in &worktrees {
            if same_path(&wt.path, &base_dir) {
                println!("main");
            } else {
                println!(
                    "{}",
                    wt.path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
        }
        return;
    }

    let green = "\x1b[32m";
    let red = "\x1b[31m";
    let gray = "\x1b[90m";
    let reset = "\x1b[0m";
    let cyan = "\x1b[36m";
    let bold = "\x1b[1m";

    let mut changes_display = HashMap::new();
    let mut changes_len = HashMap::new();
    for (idx, wt) in worktrees.iter().enumerate() {
        let mut parts = Vec::new();
        let mut clean_parts = Vec::new();
        if wt.insertions > 0 {
            parts.push(format!("{green}+{}{reset}", wt.insertions));
            clean_parts.push(format!("+{}", wt.insertions));
        }
        if wt.deletions > 0 {
            parts.push(format!("{red}-{}{reset}", wt.deletions));
            clean_parts.push(format!("-{}", wt.deletions));
        }
        if wt.has_untracked {
            parts.push(format!("{gray}??{reset}"));
            clean_parts.push("??".into());
        }
        if parts.is_empty() {
            changes_display.insert(idx, "-".to_string());
            changes_len.insert(idx, 1usize);
        } else {
            changes_display.insert(idx, parts.join(" "));
            changes_len.insert(idx, clean_parts.join(" ").chars().count());
        }
    }

    let name_w = worktrees
        .iter()
        .map(|wt| {
            wt.path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .count()
        })
        .max()
        .unwrap_or(0)
        .max(m0("worktree_name").chars().count())
        + 2;
    let branch_w = worktrees
        .iter()
        .map(|wt| wt.branch.chars().count())
        .max()
        .unwrap_or(0)
        .max(m0("branch_name").chars().count())
        + 2;
    let created_values: Vec<String> = worktrees
        .iter()
        .map(|wt| get_relative_time(wt.created))
        .collect();
    let last_values: Vec<String> = worktrees
        .iter()
        .map(|wt| get_relative_time(wt.last_commit))
        .collect();
    let created_w = created_values
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        .max(m0("created_at").chars().count())
        + 2;
    let last_w = last_values
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        .max(m0("last_commit").chars().count())
        + 2;
    let status_w = changes_len
        .values()
        .copied()
        .max()
        .unwrap_or(0)
        .max(m0("changes_label").chars().count())
        + 2;
    let base_header = format!(
        "{:<name_w$} {:<branch_w$} {:<created_w$} {:<last_w$} {:<status_w$}",
        m0("worktree_name"),
        m0("branch_name"),
        m0("created_at"),
        m0("last_commit"),
        m0("changes_label")
    );
    if show_pr {
        println!("{bold}{base_header}   PR{reset}");
        println!("{}", "-".repeat(base_header.len() + 5));
    } else {
        println!("{bold}{}{reset}", base_header.trim_end());
        println!("{}", "-".repeat(base_header.trim_end().len()));
    }

    for (idx, wt) in worktrees.iter().enumerate() {
        let is_main = same_path(&wt.path, &base_dir);
        let raw_name = wt
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let name_display = if is_main {
            format!("{cyan}(main){reset}")
        } else {
            raw_name.clone()
        };
        let name_clean_len = if is_main { 6 } else { raw_name.chars().count() };
        let name_padding = " ".repeat(name_w.saturating_sub(name_clean_len));
        let changes = changes_display
            .get(&idx)
            .cloned()
            .unwrap_or_else(|| "-".into());
        let change_len = *changes_len.get(&idx).unwrap_or(&1);
        let changes_padding = " ".repeat(status_w.saturating_sub(change_len));
        print!(
            "{}{} {:<branch_w$} {:<created_w$} {:<last_w$} {}{}",
            name_display,
            name_padding,
            wt.branch,
            created_values[idx],
            last_values[idx],
            changes,
            changes_padding
        );
        if show_pr {
            print!("   {}", wt.pr_info);
        }
        println!();
    }
}

fn cmd_diff(args: &[String]) {
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    let config = load_config(&base_dir);
    let mut name_map = HashMap::new();
    for wt in get_worktree_info(&base_dir) {
        let name = if same_path(&wt.path, &base_dir) {
            "main".into()
        } else {
            wt.path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        name_map.insert(name, wt.path);
    }
    let mut target_path: Option<PathBuf> = None;
    let mut remaining_args = Vec::new();
    for arg in args {
        if target_path.is_none() && name_map.contains_key(arg) {
            target_path = name_map.get(arg).cloned();
        } else {
            remaining_args.push(arg.clone());
        }
    }
    let target_path =
        target_path.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let diff_tool = config_diff_tool(&config);
    for (tool, mut cmd) in [
        (
            "lumen",
            vec![
                "lumen".to_string(),
                "diff".to_string(),
                "--watch".to_string(),
            ],
        ),
        ("gitui", vec!["gitui".to_string()]),
        ("tig", vec!["tig".to_string()]),
    ] {
        if diff_tool == tool {
            if which_cmd(tool).is_none() {
                fatal(m1(
                    "error",
                    format!("{tool} not found. Please install it first."),
                ));
            }
            cmd.extend(remaining_args);
            let _ = Command::new(&cmd[0])
                .args(&cmd[1..])
                .current_dir(&target_path)
                .status();
            return;
        }
    }

    if remaining_args.is_empty() {
        if let Some(base_branch) = get_default_branch(&base_dir) {
            let current = run_command(
                vec![
                    "git".into(),
                    "rev-parse".into(),
                    "--abbrev-ref".into(),
                    "HEAD".into(),
                ],
                Some(&target_path),
                false,
                false,
            );
            let current_branch = current.stdout.trim();
            if current.status == 0 && !current_branch.is_empty() && current_branch != base_branch {
                remaining_args.push(base_branch);
            }
        }
    }
    let mut cmd = Command::new("git");
    cmd.arg("diff")
        .args(&remaining_args)
        .current_dir(&target_path);
    let _ = cmd.status();
}

fn config_key_get<'a>(config: &'a TomlValue, key: &str) -> Option<&'a TomlValue> {
    let mut value = config;
    for part in key.split('.') {
        value = value.get(part)?;
    }
    Some(value)
}

fn scalar_from_arg(value: &str) -> TomlValue {
    if value.eq_ignore_ascii_case("true") {
        TomlValue::Boolean(true)
    } else if value.eq_ignore_ascii_case("false") {
        TomlValue::Boolean(false)
    } else if value.chars().all(|c| c.is_ascii_digit()) {
        value
            .parse::<i64>()
            .map(TomlValue::Integer)
            .unwrap_or_else(|_| TomlValue::String(value.into()))
    } else {
        TomlValue::String(value.into())
    }
}

fn nested_update(key: &str, value: TomlValue) -> TomlValue {
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = value;
    for part in parts.into_iter().rev() {
        let mut table = TomlMap::new();
        table.insert(part.into(), current);
        current = TomlValue::Table(table);
    }
    current
}

fn cmd_config(args: &[String]) {
    let base_dir = find_base_dir();
    let is_global = args.iter().any(|a| a == "--global");
    let is_local = args.iter().any(|a| a == "--local");
    let remaining_args: Vec<String> = args
        .iter()
        .filter(|a| *a != "--global" && *a != "--local")
        .cloned()
        .collect();
    let target_file = if is_global {
        xdg_config_home().join("easy-worktree").join("config.toml")
    } else if is_local {
        let Some(base_dir) = &base_dir else {
            fatal(m1("error", m0("base_not_found")));
        };
        get_wt_dir(base_dir).join("config.local.toml")
    } else {
        let Some(base_dir) = &base_dir else {
            fatal(m1("error", m0("base_not_found")));
        };
        get_wt_dir(base_dir).join("config.toml")
    };

    if remaining_args.is_empty() {
        let config = if let Some(base_dir) = &base_dir {
            load_config(base_dir)
        } else {
            let mut cfg = default_config();
            if target_file.exists() {
                if let Ok(value) = fs::read_to_string(&target_file)
                    .unwrap_or_default()
                    .parse::<TomlValue>()
                {
                    deep_merge(&mut cfg, value);
                }
            }
            cfg
        };
        print!(
            "{}",
            toml::to_string_pretty(&config).unwrap_or_default().trim()
        );
        println!();
        return;
    }

    let key = &remaining_args[0];
    if remaining_args.len() == 1 {
        let config = if is_global || is_local {
            if target_file.exists() {
                fs::read_to_string(&target_file)
                    .ok()
                    .and_then(|content| content.parse::<TomlValue>().ok())
                    .unwrap_or_else(|| TomlValue::Table(TomlMap::new()))
            } else {
                TomlValue::Table(TomlMap::new())
            }
        } else {
            load_config(base_dir.as_ref().unwrap())
        };
        if let Some(value) = config_key_get(&config, key) {
            match value {
                TomlValue::Table(_) | TomlValue::Array(_) => {
                    let mut table = TomlMap::new();
                    table.insert(key.clone(), value.clone());
                    print!(
                        "{}",
                        toml::to_string_pretty(&TomlValue::Table(table))
                            .unwrap_or_default()
                            .trim()
                    );
                    println!();
                }
                TomlValue::String(s) => println!("{s}"),
                other => println!("{other}"),
            }
        }
        return;
    }

    let update = nested_update(key, scalar_from_arg(&remaining_args[1]));
    save_config_to_file(&target_file, update);
}

fn cmd_remove(args: &[String]) {
    if args.is_empty() {
        eprintln!("{}", m0("usage_rm"));
        std::process::exit(1);
    }
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    let mut flags = Vec::new();
    let mut work_name: Option<String> = None;
    for arg in args {
        if arg == "-f" || arg == "--force" {
            flags.push(arg.clone());
        } else if work_name.is_none() {
            work_name = Some(arg.clone());
        }
    }
    let Some(work_name) = work_name else {
        eprintln!("{}", m0("usage_rm"));
        std::process::exit(1);
    };
    let target = get_worktree_info(&base_dir).into_iter().find(|wt| {
        wt.path.file_name().is_some_and(|n| n == work_name.as_str())
            || wt.path.to_string_lossy() == work_name
    });
    let Some(target) = target else {
        eprintln!("{}", m1("error", m1("select_not_found", &work_name)));
        suggest_worktree_name(&base_dir, &work_name);
        std::process::exit(1);
    };
    eprintln!("{}", m1("removing_worktree", &work_name));
    let mut cmd = vec!["git".into(), "worktree".into(), "remove".into()];
    cmd.extend(flags);
    cmd.push(target.path.display().to_string());
    let result = run_command(cmd, Some(&base_dir), false, true);
    if result.status == 0 {
        remove_worktree_metadata(&base_dir, &target.path);
    } else {
        if !result.stderr.is_empty() {
            eprint!("{}", result.stderr);
        }
        std::process::exit(1);
    }
}

fn cmd_checkout(args: &[String]) {
    if args.is_empty() {
        return;
    }
    let work_name = &args[0];
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    for wt in get_worktree_info(&base_dir) {
        if wt.path.file_name().is_some_and(|n| n == work_name.as_str())
            || (same_path(&wt.path, &base_dir) && work_name == "main")
        {
            println!("{}", wt.path.display());
            return;
        }
    }
    eprintln!("{}", m1("error", m1("select_not_found", work_name)));
    suggest_worktree_name(&base_dir, work_name);
    std::process::exit(1);
}

fn target_path_for_selection(base_dir: &Path, target: &str) -> PathBuf {
    if target == "main" {
        return base_dir.to_path_buf();
    }
    let config = load_config(base_dir);
    let worktrees_dir_name = config_get_str(&config, "worktrees_dir", ".worktrees");
    if is_bare_repository(base_dir) {
        let wt_home = require_wt_home_dir(base_dir);
        let base_parent = wt_home.parent().unwrap_or_else(|| Path::new("."));
        if worktrees_dir_name.is_empty() {
            base_parent.join(target)
        } else {
            base_parent.join(worktrees_dir_name).join(target)
        }
    } else {
        base_dir.join(worktrees_dir_name).join(target)
    }
}

fn switch_selection(
    target: &str,
    base_dir: &Path,
    current_sel: Option<&str>,
    last_sel_file: &Path,
    command: Option<Vec<String>>,
) {
    let target_path = target_path_for_selection(base_dir, target);
    if !target_path.exists() {
        eprintln!("{}", m1("error", m1("select_not_found", target)));
        std::process::exit(1);
    }

    if current_sel != Some(target) {
        if let Some(current_sel) = current_sel {
            let _ = fs::write(last_sel_file, current_sel);
        }
        eprintln!("{}", m1("select_switched", target));
    }

    let config = load_config(base_dir);
    let setup_files = config_setup_files(&config);
    if setup_files
        .iter()
        .any(|file| !target_path.join(file).exists())
    {
        eprintln!("\x1b[33m{}\x1b[0m", m0("suggest_setup"));
    }

    if io::stdout().is_terminal() {
        if let Ok(current_session) = env::var("WT_SESSION_NAME") {
            eprintln!("\x1b[31m{}\x1b[0m", m1("nesting_error", current_session));
            std::process::exit(1);
        }
        let (shell, shell_mode) = interactive_shell();
        eprintln!("{}", m2("jump_instruction", target, target_path.display()));
        let mut cmd = Command::new(&shell);
        cmd.current_dir(&target_path)
            .env("WT_SESSION_NAME", target)
            .env(
                "PS1",
                format!(
                    "(wt:{target}) {}",
                    env::var("PS1").unwrap_or_else(|_| "$ ".into())
                ),
            );
        eprint!("\x1b]0;wt:{target}\x07");
        let _ = io::stderr().flush();
        if env::var_os("TMUX").is_some() {
            let _ = Command::new("tmux")
                .args(["rename-window", &format!("wt:{target}")])
                .status();
        }
        if let Some(command) = command {
            if !command.is_empty() {
                match shell_mode {
                    ShellMode::UnixLike => {
                        cmd.args(["-c", &format!("{}; exec {}", command.join(" "), shell)]);
                    }
                    ShellMode::Cmd => {
                        cmd.args(["/K", &command.join(" ")]);
                    }
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = cmd.exec();
            fatal_error(err);
        }
        #[cfg(not(unix))]
        {
            let status = cmd.status().unwrap_or_else(|err| fatal_error(err));
            std::process::exit(status.code().unwrap_or(1));
        }
    } else {
        println!("{}", path_resolve(&target_path).display());
        if let Some(command) = command {
            if !command.is_empty() {
                let status = Command::new(&command[0])
                    .args(&command[1..])
                    .current_dir(&target_path)
                    .status()
                    .unwrap_or_else(|err| fatal_error(err));
                if !status.success() {
                    std::process::exit(status.code().unwrap_or(1));
                }
            }
        }
    }
}

fn cmd_select(args: &[String]) {
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    create_hook_template(&base_dir);
    let wt_dir = get_wt_dir(&base_dir);
    let last_sel_file = wt_dir.join("last_selection");
    let mut current_sel = env::var("WT_SESSION_NAME").ok();
    if current_sel.is_none() {
        if let Ok(cwd) = env::current_dir() {
            let resolved_base = path_resolve(&base_dir);
            for wt in get_worktree_info(&base_dir) {
                let wt_path = path_resolve(&wt.path);
                if path_starts_with(&cwd, &wt_path) {
                    current_sel = Some(if wt_path == resolved_base {
                        "main".into()
                    } else {
                        wt_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    });
                    break;
                }
            }
        }
    }

    let names = get_worktree_names(&base_dir);
    if args.is_empty() {
        if which_cmd("fzf").is_some() && io::stdin().is_terminal() {
            let mut fzf_input = String::new();
            for name in &names {
                if Some(name.as_str()) == current_sel.as_deref() {
                    fzf_input.push_str(&format!("{name} (*)\n"));
                } else {
                    fzf_input.push_str(&format!("{name}\n"));
                }
            }
            let mut child = Command::new("fzf")
                .args([
                    "--height",
                    "40%",
                    "--reverse",
                    "--header",
                    "Select Worktree",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap_or_else(|err| fatal_error(err));
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(fzf_input.as_bytes());
            }
            let output = child
                .wait_with_output()
                .unwrap_or_else(|err| fatal_error(err));
            if output.status.success() {
                let selected = String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();
                if !selected.is_empty() {
                    switch_selection(
                        &selected,
                        &base_dir,
                        current_sel.as_deref(),
                        &last_sel_file,
                        None,
                    );
                }
            }
            return;
        }
        let yellow = "\x1b[33m";
        let reset = "\x1b[0m";
        let bold = "\x1b[1m";
        for name in names {
            if Some(name.as_str()) == current_sel.as_deref() {
                println!("{yellow}{bold}{name}{reset}");
            } else {
                println!("{name}");
            }
        }
        return;
    }

    let mut target = args[0].clone();
    let command = if args.len() > 1 {
        Some(args[1..].to_vec())
    } else {
        None
    };
    if target == "-" {
        if !last_sel_file.exists() {
            fatal(m1("error", m0("select_no_last")));
        }
        target = fs::read_to_string(&last_sel_file)
            .unwrap_or_default()
            .trim()
            .to_string();
        if target.is_empty() {
            fatal(m1("error", m0("select_no_last")));
        }
    }
    if !names.contains(&target) {
        eprintln!("{}", m1("error", m1("select_not_found", &target)));
        suggest_worktree_name(&base_dir, &target);
        std::process::exit(1);
    }
    switch_selection(
        &target,
        &base_dir,
        current_sel.as_deref(),
        &last_sel_file,
        command,
    );
}

fn cmd_run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("{}", m0("usage_run"));
        std::process::exit(1);
    }
    let work_name = &args[0];
    let command = &args[1..];
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    let target_path = target_path_for_selection(&base_dir, work_name);
    if !target_path.exists() {
        eprintln!("{}", m1("error", m1("select_not_found", work_name)));
        suggest_worktree_name(&base_dir, work_name);
        std::process::exit(1);
    }
    let status = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(&target_path)
        .env("WT_SESSION_NAME", work_name)
        .status()
        .unwrap_or_else(|err| fatal_error(err));
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn cmd_current(_args: &[String]) {
    if let Ok(name) = env::var("WT_SESSION_NAME") {
        println!("{name}");
        return;
    }
    let Some(base_dir) = find_base_dir() else {
        return;
    };
    let Ok(cwd) = env::current_dir() else {
        return;
    };
    let resolved_base = path_resolve(&base_dir);
    for wt in get_worktree_info(&base_dir) {
        let wt_path = path_resolve(&wt.path);
        if path_resolve(&cwd) == wt_path {
            if wt_path == resolved_base {
                println!("main");
            } else {
                println!(
                    "{}",
                    wt_path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            return;
        }
    }
}

fn cmd_setup(_args: &[String]) {
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    let target_path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = load_config(&base_dir);
    let setup_files = config_setup_files(&config);
    let count = copy_setup_files(&base_dir, &target_path, &setup_files, &config);
    if count > 0 {
        eprintln!("{}", m1("completed_setup", count));
    }
    let work_name = if same_path(&target_path, &base_dir) {
        "main".to_string()
    } else {
        target_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    let branch = run_command(
        vec!["git".into(), "branch".into(), "--show-current".into()],
        Some(&target_path),
        false,
        false,
    );
    let branch_name = if branch.status == 0 {
        let branch = branch.stdout.trim().to_string();
        (!branch.is_empty()).then_some(branch)
    } else {
        None
    };
    run_post_add_hook(&target_path, &work_name, &base_dir, branch_name.as_deref());
}

fn cmd_clean(args: &[String]) {
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    let (clean_all, _, _, _) = parse_clean_filter_options(args);
    let force_yes = args.iter().any(|a| a == "--yes" || a == "-y");
    let worktrees = get_worktree_info(&base_dir);
    let targets = resolve_clean_targets(&base_dir, &worktrees, args);
    if targets.is_empty() {
        eprintln!("{}", m0("no_clean_targets"));
        return;
    }
    for wt in &targets {
        let created = wt
            .created
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "N/A".into());
        eprintln!(
            "{} (reason: {}, created: {})",
            wt.path.file_name().unwrap_or_default().to_string_lossy(),
            wt.reason,
            created
        );
    }
    if !clean_all && !force_yes {
        print!("{}", m1("clean_confirm", targets.len()));
        let _ = io::stdout().flush();
        let mut response = String::new();
        if io::stdin().read_line(&mut response).is_err()
            || !["y", "yes"].contains(&response.trim().to_lowercase().as_str())
        {
            println!("Cancelled.");
            return;
        }
    }
    for wt in targets {
        eprintln!(
            "{}",
            m1(
                "removing_worktree",
                wt.path.file_name().unwrap_or_default().to_string_lossy()
            )
        );
        let result = run_command(
            vec![
                "git".into(),
                "worktree".into(),
                "remove".into(),
                wt.path.display().to_string(),
            ],
            Some(&base_dir),
            false,
            true,
        );
        if result.status == 0 {
            remove_worktree_metadata(&base_dir, &wt.path);
        } else if !result.stderr.is_empty() {
            eprint!("{}", result.stderr);
        }
    }
}

fn cmd_passthrough(args: &[String]) {
    let Some(base_dir) = find_base_dir() else {
        fatal(m1("error", m0("base_not_found")));
    };
    let mut cmd = vec!["git".into(), "worktree".into()];
    cmd.extend(args.iter().cloned());
    let result = run_command(cmd, Some(&base_dir), false, true);
    print!("{}", result.stdout);
    if !result.stderr.is_empty() {
        eprint!("{}", result.stderr);
    }
    std::process::exit(result.status);
}

fn bash_completion_script() -> &'static str {
    r#"_wt_completions() {
    local cur prev words cword
    _init_completion || return

    local wt_bin="${words[0]}"
    local commands="clone init add ad select sl list ls co checkout current cur stash st pr rm remove clean cl setup su run completion"

    if [[ ${cword} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "${commands} --git-dir --help --version" -- "${cur}") )
        return 0
    fi

    if [[ "${prev}" == "--git-dir" ]]; then
        COMPREPLY=( $(compgen -d -- "${cur}") )
        return 0
    fi

    local subcmd=""
    for w in "${words[@]:1}"; do
        if [[ "${w}" != --* ]]; then
            subcmd="${w}"
            break
        fi
    done

    local wt_names="$(${wt_bin} list --quiet 2>/dev/null)"

    case "${subcmd}" in
        add|ad)
            COMPREPLY=( $(compgen -W "--skip-setup --no-setup --select ${wt_names}" -- "${cur}") )
            ;;
        select|sl|co|checkout|run|rm|remove)
            COMPREPLY=( $(compgen -W "${wt_names}" -- "${cur}") )
            ;;
        clean|cl)
            COMPREPLY=( $(compgen -W "--days --merged --closed --all --yes -y" -- "${cur}") )
            ;;
        list|ls)
            COMPREPLY=( $(compgen -W "--pr --quiet -q --days --merged --closed --all --sort --asc --desc created last-commit name branch" -- "${cur}") )
            ;;
        stash|st)
            COMPREPLY=( $(compgen -W "${wt_names}" -- "${cur}") )
            ;;
        pr)
            COMPREPLY=( $(compgen -W "add co" -- "${cur}") )
            ;;
        completion)
            COMPREPLY=( $(compgen -W "bash zsh" -- "${cur}") )
            ;;
        *)
            COMPREPLY=( $(compgen -W "${commands}" -- "${cur}") )
            ;;
    esac
}
complete -F _wt_completions wt
"#
}

fn cmd_completion(args: &[String]) {
    if args.len() != 1 || !["bash", "zsh"].contains(&args[0].as_str()) {
        eprintln!("{}", m0("usage_completion"));
        std::process::exit(1);
    }
    if args[0] == "bash" {
        print!("{}", bash_completion_script());
    } else {
        print!(
            "autoload -U +X bashcompinit && bashcompinit\n{}compdef _wt_completions wt\n",
            bash_completion_script()
        );
    }
}

fn show_help() {
    if is_japanese() {
        println!("easy-worktree - Git worktree を簡単に管理するための CLI ツール");
        println!("\n使用方法:\n  wt <command> [options]\n");
        println!("コマンド:");
        println!(
            "  {:<55} - リポジトリをクローン",
            "clone [--bare] <repository_url> [dest_dir]"
        );
        println!(
            "  {:<55} - 既存リポジトリをメインリポジトリとして構成",
            "init"
        );
        println!(
            "  {:<55} - worktree を追加",
            "add (ad) <作業名> [<base_branch>] [--skip-setup|--no-setup] [--select [<コマンド>...]]"
        );
        println!(
            "  {:<55} - 作業ディレクトリを切り替え（fzf対応）",
            "select (sl) [<作業名>|-] [<コマンド>...]"
        );
        println!(
            "  {:<55} - worktree 一覧を表示",
            "list (ls) [--pr] [--quiet|-q] [--days N] [--merged] [--closed] [--all] [--sort ...] [--asc|--desc]"
        );
        println!(
            "  {:<55} - 変更を表示 (git diff)",
            "diff (df) [<作業名>] [引数...]"
        );
        println!(
            "  {:<55} - 設定の取得/設定",
            "config [<キー> [<値>]] [--global|--local]"
        );
        println!("  {:<55} - worktree のパスを表示", "co/checkout <作業名>");
        println!("  {:<55} - 現在の worktree 名を表示", "current (cur)");
        println!(
            "  {:<55} - 現在の変更をスタッシュして新規 worktree に移動",
            "stash (st) <作業名> [<base_branch>]"
        );
        println!(
            "  {:<55} - GitHub PR を取得して worktree を作成/パス表示",
            "pr add <番号>"
        );
        println!(
            "  {:<55} - worktree を削除",
            "rm/remove <作業名> [-f|--force]"
        );
        println!(
            "  {:<55} - 不要な worktree を削除",
            "clean (cl) [--days N] [--merged] [--closed] [--all] [--yes|-y]"
        );
        println!(
            "  {:<55} - 作業ディレクトリを初期化（ファイルコピー・フック実行）",
            "setup (su)"
        );
        println!("  {:<55} - システム情報と環境の確認", "doctor");
        println!(
            "  {:<55} - シェル補完スクリプトを出力",
            "completion <bash|zsh>"
        );
        println!("\nオプション:");
        println!("  {:<55} - このヘルプメッセージを表示", "-h, --help");
        println!("  {:<55} - バージョン情報を表示", "-v, --version");
        println!("  {:<55} - 指定したディレクトリに移動して実行", "-C <path>");
        println!("  {:<55} - Git ディレクトリを明示指定", "--git-dir <path>");
    } else {
        println!("easy-worktree - Simple CLI tool for managing Git worktrees");
        println!("\nUsage:\n  wt <command> [options]\n");
        println!("Commands:");
        println!(
            "  {:<55} - Clone a repository",
            "clone [--bare] <repository_url> [dest_dir]"
        );
        println!("  {:<55} - Configure existing repository as main", "init");
        println!(
            "  {:<55} - Add a worktree",
            "add (ad) <work_name> [<base_branch>] [--skip-setup|--no-setup] [--select [<command>...]]"
        );
        println!(
            "  {:<55} - Switch worktree selection (fzf support)",
            "select (sl) [<name>|-] [<command>...]"
        );
        println!(
            "  {:<55} - List worktrees",
            "list (ls) [--pr] [--quiet|-q] [--days N] [--merged] [--closed] [--all] [--sort ...] [--asc|--desc]"
        );
        println!(
            "  {:<55} - Show changes (git diff)",
            "diff (df) [<name>] [args...]"
        );
        println!(
            "  {:<55} - Get/Set configuration",
            "config [<key> [<value>]] [--global|--local]"
        );
        println!(
            "  {:<55} - Show path to a worktree",
            "co/checkout <work_name>"
        );
        println!("  {:<55} - Show current worktree name", "current (cur)");
        println!(
            "  {:<55} - Stash current changes and move to new worktree",
            "stash (st) <work_name> [<base_branch>]"
        );
        println!(
            "  {:<55} - Manage GitHub PRs as worktrees",
            "pr add <number>"
        );
        println!(
            "  {:<55} - Remove a worktree",
            "rm/remove <work_name> [-f|--force]"
        );
        println!(
            "  {:<55} - Remove unused/merged worktrees",
            "clean (cl) [--days N] [--merged] [--closed] [--all] [--yes|-y]"
        );
        println!(
            "  {:<55} - Setup worktree (copy files and run hooks)",
            "setup (su)"
        );
        println!(
            "  {:<55} - Show system information and check environment",
            "doctor"
        );
        println!(
            "  {:<55} - Print shell completion script",
            "completion <bash|zsh>"
        );
        println!("\nOptions:");
        println!("  {:<55} - Show this help message", "-h, --help");
        println!("  {:<55} - Show version information", "-v, --version");
        println!(
            "  {:<55} - Run as if wt was started in <path> instead of the current working directory",
            "-C <path>"
        );
        println!(
            "  {:<55} - Explicitly set git directory",
            "--git-dir <path>"
        );
    }
}

fn show_version() {
    println!("easy-worktree version {}", env!("CARGO_PKG_VERSION"));
}

fn parse_global_args(argv: Vec<String>) -> Vec<String> {
    let mut cleaned = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        if let Some(value) = arg.strip_prefix("--git-dir=") {
            if value.is_empty() {
                fatal_error("Missing value for --git-dir");
            }
            set_global_git_dir(path_resolve(&expand_user(value)));
        } else if arg == "--git-dir" {
            if i + 1 >= argv.len() {
                fatal_error("Missing value for --git-dir");
            }
            set_global_git_dir(path_resolve(&expand_user(&argv[i + 1])));
            i += 1;
        } else if arg == "-C" {
            if i + 1 >= argv.len() {
                fatal_error("Missing value for -C");
            }
            let path = path_resolve(&expand_user(&argv[i + 1]));
            if !path.exists() {
                fatal_error(format!("Directory does not exist: {}", path.display()));
            }
            if !path.is_dir() {
                fatal_error(format!("Not a directory: {}", path.display()));
            }
            if let Err(err) = env::set_current_dir(path) {
                fatal_error(err);
            }
            i += 1;
        } else {
            cleaned.push(arg.clone());
        }
        i += 1;
    }
    cleaned
}

fn cmd_doctor(_args: &[String]) {
    println!("🩺 easy-worktree doctor");
    println!("=======================");
    println!("\n[System]");
    println!(
        "Rust: {}",
        option_env!("RUSTC_VERSION").unwrap_or("unknown")
    );
    if let Some(git_path) = which_cmd("git") {
        let out = run_command(vec!["git".into(), "--version".into()], None, false, true).stdout;
        println!("Git: {} ({})", out.trim(), git_path.display());
    } else {
        println!("Git: Not found ❌");
    }
    for (label, cmd, note) in [
        ("GitHub CLI (gh)", "gh", "Optional"),
        ("fzf", "fzf", "Optional, used for interactive selection"),
        ("GitUI", "gitui", "Optional, used for UI diffs"),
        ("Tig", "tig", "Optional, used for UI diffs"),
    ] {
        if let Some(path) = which_cmd(cmd) {
            let out = run_command(vec![cmd.into(), "--version".into()], None, false, true).stdout;
            let first = out.lines().next().unwrap_or("").trim();
            println!("{label}: {first} ({})", path.display());
        } else {
            println!("{label}: Not found ({note})");
        }
    }

    println!("\n[Environment]");
    let base_dir = find_base_dir();
    if let Some(base_dir) = &base_dir {
        println!("Project Root: {}", base_dir.display());
        let is_bare = is_bare_repository(base_dir);
        println!(
            "Repository Type: {}",
            if is_bare { "Bare" } else { "Normal" }
        );
        let wt_home = get_wt_home_dir(base_dir);
        println!(
            "WT Home: {}",
            wt_home
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not found".into())
        );
        if let Some(wt_home) = &wt_home {
            let wt_dir = wt_home.join(".wt");
            if wt_dir.exists() {
                println!(".wt Directory: {} (Exists)", wt_dir.display());
            } else {
                println!(".wt Directory: {} (Not found ❌)", wt_dir.display());
            }
            let config = load_config(base_dir);
            println!("Active Config:");
            println!(
                "  - worktrees_dir: {}",
                config_get_str(&config, "worktrees_dir", ".worktrees")
            );
            println!("  - setup_files: {:?}", config_setup_files(&config));
            println!(
                "  - setup_source_dir: {}",
                config
                    .get("setup_source_dir")
                    .and_then(TomlValue::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Auto-detect")
            );
            println!("  - diff.tool: {}", config_diff_tool(&config));
        }
    } else {
        println!("Project Root: Not in a git repository");
    }

    println!("\n[Configuration Files]");
    fn check_config_file(file_path: &Path, label: &str) {
        if !file_path.exists() {
            println!("{label}: {} (Not found)", file_path.display());
            return;
        }
        println!("{label}: {} (Exists)", file_path.display());
        let valid_root_keys = ["worktrees_dir", "setup_files", "setup_source_dir", "diff"];
        if let Ok(content) = fs::read_to_string(file_path) {
            match content.parse::<TomlValue>() {
                Ok(TomlValue::Table(table)) if table.is_empty() => println!("  (Empty)"),
                Ok(TomlValue::Table(table)) => {
                    for (key, value) in table {
                        if !valid_root_keys.contains(&key.as_str()) {
                            println!("  ⚠️  Warning: Unknown configuration key '{key}'");
                        } else if key == "diff" {
                            println!("  - diff:");
                            if let TomlValue::Table(diff) = value {
                                for (sub_key, sub_value) in diff {
                                    if sub_key != "tool" {
                                        println!("    ⚠️  Warning: Unknown key 'diff.{sub_key}'");
                                    } else {
                                        println!("    - {sub_key}: {sub_value}");
                                    }
                                }
                            }
                        } else {
                            println!("  - {key}: {value}");
                        }
                    }
                }
                Ok(_) => println!("  Error reading config: root must be a table"),
                Err(err) => println!("  Error reading config: {err}"),
            }
        }
    }
    check_config_file(
        &xdg_config_home().join("easy-worktree").join("config.toml"),
        "Global Config",
    );
    if let Some(base_dir) = &base_dir {
        if let Some(wt_home) = get_wt_home_dir(base_dir) {
            let wt_dir = wt_home.join(".wt");
            check_config_file(&wt_dir.join("config.toml"), "Project Config");
            check_config_file(&wt_dir.join("config.local.toml"), "Local Config");
        }
    }
}

pub fn main_entry() -> i32 {
    let raw_args = parse_global_args(env::args().skip(1).collect());
    if raw_args.is_empty() {
        show_help();
        return 1;
    }
    let command = &raw_args[0];
    let args = &raw_args[1..];
    match command.as_str() {
        "-h" | "--help" => {
            show_help();
            0
        }
        "-v" | "--version" => {
            show_version();
            0
        }
        "clone" => {
            cmd_clone(args);
            0
        }
        "init" => {
            cmd_init(args);
            0
        }
        "add" | "ad" => {
            cmd_add(args);
            0
        }
        "list" | "ls" => {
            cmd_list(args);
            0
        }
        "diff" | "df" => {
            cmd_diff(args);
            0
        }
        "config" => {
            cmd_config(args);
            0
        }
        "rm" | "remove" => {
            cmd_remove(args);
            0
        }
        "clean" | "cl" => {
            cmd_clean(args);
            0
        }
        "setup" | "su" => {
            cmd_setup(args);
            0
        }
        "stash" | "st" => {
            cmd_stash(args);
            0
        }
        "pr" => {
            cmd_pr(args);
            0
        }
        "select" | "sl" => {
            cmd_select(args);
            0
        }
        "current" | "cur" => {
            cmd_current(args);
            0
        }
        "co" | "checkout" => {
            cmd_checkout(args);
            0
        }
        "run" => {
            cmd_run(args);
            0
        }
        "completion" => {
            cmd_completion(args);
            0
        }
        "doctor" => {
            cmd_doctor(args);
            0
        }
        _ => {
            cmd_passthrough(&raw_args);
            0
        }
    }
}
