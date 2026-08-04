use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::fs::symlink;

fn wt_bin() -> &'static str {
    env!("CARGO_BIN_EXE_wt")
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("easy-worktree-rs-{name}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(cmd: &str, args: &[&str], cwd: &Path) -> Output {
    let output = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{cmd} {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_wt(args: &[&str], cwd: &Path, xdg: &Path) -> Output {
    let output = Command::new(wt_bin())
        .args(args)
        .current_dir(cwd)
        .env("LANG", "en")
        .env("LC_ALL", "C")
        .env("XDG_CONFIG_HOME", xdg)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_wt_with_path(args: &[&str], cwd: &Path, xdg: &Path, path: &Path) -> Output {
    let output = Command::new(wt_bin())
        .args(args)
        .current_dir(cwd)
        .env("LANG", "en")
        .env("LC_ALL", "C")
        .env("XDG_CONFIG_HOME", xdg)
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_wt_with_home(args: &[&str], cwd: &Path, xdg: &Path, home: &Path) -> Output {
    let output = Command::new(wt_bin())
        .args(args)
        .current_dir(cwd)
        .env("LANG", "en")
        .env("LC_ALL", "C")
        .env("XDG_CONFIG_HOME", xdg)
        .env("HOME", home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_wt_with_stdin(args: &[&str], cwd: &Path, xdg: &Path, stdin: &str) -> Output {
    let mut child = Command::new(wt_bin())
        .args(args)
        .current_dir(cwd)
        .env("LANG", "en")
        .env("LC_ALL", "C")
        .env("XDG_CONFIG_HOME", xdg)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin.write_all(stdin.as_bytes()).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "wt {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_wt_with_stdin_and_path(
    args: &[&str],
    cwd: &Path,
    xdg: &Path,
    stdin: &str,
    path: &Path,
) -> Output {
    let mut child = Command::new(wt_bin())
        .args(args)
        .current_dir(cwd)
        .env("LANG", "en")
        .env("LC_ALL", "C")
        .env("XDG_CONFIG_HOME", xdg)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin.write_all(stdin.as_bytes()).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "wt {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(unix)]
fn run_wt_with_tty_stdin_and_path_and_shell(
    args: &[&str],
    cwd: &Path,
    xdg: &Path,
    stdin: &str,
    path: &Path,
    shell: &Path,
) -> Output {
    let shell = shell_quote(&shell.display().to_string());
    let command = std::iter::once(wt_bin())
        .chain(args.iter().copied())
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let command = format!("SHELL={shell} {command} 2>&1");
    let mut child = Command::new(find_cmd_path("script"))
        .args(["-qec", &command, "/dev/null"])
        .current_dir(cwd)
        .env("LANG", "en")
        .env("LC_ALL", "C")
        .env("XDG_CONFIG_HOME", xdg)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin.write_all(stdin.as_bytes()).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "tty wt {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn find_cmd_path(name: &str) -> PathBuf {
    for dir in env::split_paths(&env::var_os("PATH").expect("PATH should be set")) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return candidate;
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    panic!("could not find command in PATH: {name}");
}

fn path_with_git_only(root: &Path) -> PathBuf {
    let bin_dir = root.join("git-only-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let git_path = find_cmd_path("git");
    #[cfg(unix)]
    {
        symlink(&git_path, bin_dir.join("git")).unwrap();
    }
    #[cfg(not(unix))]
    {
        let file_name = git_path
            .file_name()
            .expect("git path should have a file name");
        fs::copy(&git_path, bin_dir.join(file_name)).unwrap();
    }
    bin_dir
}

#[cfg(unix)]
fn write_executable_script(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn path_with_git_and_script(root: &Path, name: &str, content: &str) -> PathBuf {
    let bin_dir = path_with_git_only(root);
    write_executable_script(&bin_dir.join(name), content);
    bin_dir
}

#[cfg(unix)]
fn path_with_logging_git(root: &Path, log_file: &Path) -> PathBuf {
    let bin_dir = root.join("git-logging-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let git_path = find_cmd_path("git");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nexec {} \"$@\"\n",
        shell_quote(&log_file.display().to_string()),
        shell_quote(&git_path.display().to_string())
    );
    write_executable_script(&bin_dir.join("git"), &script);
    bin_dir
}

#[cfg(unix)]
fn create_test_shell(root: &Path) -> PathBuf {
    let shell = root.join("test-shell.sh");
    write_executable_script(&shell, "#!/bin/sh\npwd\n");
    shell
}

fn init_repo(repo: &Path) {
    fs::create_dir_all(repo).unwrap();
    run("git", &["init", "-b", "main"], repo);
    run("git", &["config", "user.email", "test@example.com"], repo);
    run("git", &["config", "user.name", "Test User"], repo);
    fs::write(repo.join("README.md"), "Hello\n").unwrap();
    run("git", &["add", "README.md"], repo);
    run("git", &["commit", "-m", "init"], repo);
}

#[test]
fn version_matches_python_package() {
    let output = Command::new(wt_bin())
        .arg("--version")
        .env("LANG", "en")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("easy-worktree version {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_mentions_two_letter_aliases() {
    let expected = [
        "clone (cn)",
        "init (in)",
        "add (ad)",
        "list (li, ls)",
        "diff (di, df)",
        "config (cf)",
        "rm/remove",
        "clean (cl)",
        "setup (su)",
        "stash (st)",
        "pr add",
        "select (se, sl)",
        "current (cu, cur)",
        "co/checkout",
        "run (ru)",
        "completion (cm)",
        "doctor (dr)",
    ];

    for lang in ["en", "ja_JP.UTF-8"] {
        let output = Command::new(wt_bin())
            .arg("--help")
            .env("LANG", lang)
            .output()
            .unwrap();
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in expected {
            assert!(
                stdout.contains(expected),
                "{lang} help output did not contain {expected:?}\n{stdout}"
            );
        }
    }
}

#[test]
fn init_add_select_run_and_remove() {
    let root = temp_dir("basic");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);

    run_wt(&["init"], &repo, &xdg);
    assert!(repo.join(".wt/config.toml").exists());

    fs::write(repo.join("setup.txt"), "copy me\n").unwrap();
    fs::write(
        repo.join(".wt/config.toml"),
        "worktrees_dir = \".worktrees\"\nsetup_files = [\"setup.txt\"]\n",
    )
    .unwrap();

    run_wt(&["add", "feature-one"], &repo, &xdg);
    let wt_path = repo.join(".worktrees/feature-one");
    assert!(wt_path.join("setup.txt").exists());

    let current = run_wt(&["current"], &wt_path, &xdg);
    assert_eq!(
        String::from_utf8_lossy(&current.stdout).trim(),
        "feature-one"
    );

    let selected = run_wt(&["select", "feature-one"], &repo, &xdg);
    assert!(
        String::from_utf8_lossy(&selected.stdout).contains(&wt_path.to_string_lossy().to_string())
    );

    run_wt(&["run", "feature-one", "touch", "run-ok.txt"], &repo, &xdg);
    assert!(wt_path.join("run-ok.txt").exists());

    fs::write(wt_path.join("README.md"), "changed\n").unwrap();
    let diff = run_wt(&["diff", "feature-one", "--", "README.md"], &repo, &xdg);
    assert!(String::from_utf8_lossy(&diff.stdout).contains("changed"));

    run_wt(&["rm", "--force", "feature-one"], &repo, &xdg);
    assert!(!wt_path.exists());
}

#[test]
fn slash_worktree_names_are_preserved_across_commands() {
    let root = temp_dir("slash-name");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);

    run_wt(&["add", "feature/topic"], &repo, &xdg);
    let wt_path = repo.join(".worktrees/feature/topic");
    assert!(wt_path.exists());

    let list = run_wt(&["list", "--quiet"], &repo, &xdg);
    assert!(String::from_utf8_lossy(&list.stdout).contains("feature/topic"));

    let checkout = run_wt(&["checkout", "feature/topic"], &repo, &xdg);
    assert!(
        String::from_utf8_lossy(&checkout.stdout).contains(&wt_path.to_string_lossy().to_string())
    );

    let selected = run_wt(&["select", "feature/topic"], &repo, &xdg);
    assert!(
        String::from_utf8_lossy(&selected.stdout).contains(&wt_path.to_string_lossy().to_string())
    );

    let current = run_wt(&["current"], &wt_path, &xdg);
    assert_eq!(
        String::from_utf8_lossy(&current.stdout).trim(),
        "feature/topic"
    );

    fs::write(wt_path.join("README.md"), "slash path changed\n").unwrap();
    let diff = run_wt(&["diff", "feature/topic", "--", "README.md"], &repo, &xdg);
    assert!(String::from_utf8_lossy(&diff.stdout).contains("slash path changed"));

    run_wt(&["rm", "--force", "feature/topic"], &repo, &xdg);
    assert!(!wt_path.exists());
}

#[test]
fn list_marks_missing_worktrees_without_failing() {
    let root = temp_dir("missing-worktree-list");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "keep-me"], &repo, &xdg);
    run_wt(&["add", "gone-away"], &repo, &xdg);

    let keep_path = repo.join(".worktrees/keep-me");
    let missing_path = repo.join(".worktrees/gone-away");
    assert!(keep_path.exists());
    fs::remove_dir_all(&missing_path).unwrap();

    let output = run_wt(&["list"], &repo, &xdg);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("keep-me"));
    assert!(stdout.contains("gone-away"));
    assert!(stdout.contains("missing"));
}

#[test]
fn add_without_name_prompts_and_can_select_created_worktree() {
    let root = temp_dir("interactive-add");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);

    let output = run_wt_with_stdin(&["add"], &repo, &xdg, "interactive-one\n\n");
    let wt_path = repo.join(".worktrees/interactive-one");
    assert!(wt_path.exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains(&wt_path.to_string_lossy().to_string()));
    assert!(stderr.contains("Worktree name:"));
    assert!(stderr.contains("Select the new worktree now? [Y/n]:"));
}

#[test]
fn rm_without_name_prompts_for_worktree() {
    let root = temp_dir("interactive-rm");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "remove-me"], &repo, &xdg);
    run_wt(&["add", "keep-me"], &repo, &xdg);

    let remove_path = repo.join(".worktrees/remove-me");
    let keep_path = repo.join(".worktrees/keep-me");
    assert!(remove_path.exists());
    assert!(keep_path.exists());

    let output = run_wt_with_stdin(&["rm", "--force"], &repo, &xdg, "remove-me\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Select Worktree to Remove"));
    assert!(!remove_path.exists());
    assert!(keep_path.exists());
}

#[cfg(unix)]
#[test]
fn rm_avoids_full_worktree_status_scans() {
    let root = temp_dir("rm-fast");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "remove-me"], &repo, &xdg);
    run_wt(&["add", "keep-me"], &repo, &xdg);

    let log_file = root.join("git.log");
    let path = path_with_logging_git(&root, &log_file);
    run_wt_with_path(&["rm", "--force", "remove-me"], &repo, &xdg, &path);

    let log = fs::read_to_string(&log_file).unwrap();
    assert_eq!(
        log.lines()
            .filter(|line| *line == "worktree list --porcelain")
            .count(),
        1
    );
    assert!(log.contains("worktree remove --force"));
    assert!(!log.contains("status --porcelain"));
    assert!(!log.contains("diff HEAD --shortstat"));
    assert!(!log.contains("log -1 --format=%ct"));
}

#[cfg(unix)]
#[test]
fn pr_add_uses_head_branch_name_for_branch_and_worktree() {
    let root = temp_dir("pr-branch-name");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    let origin = root.join("origin.git");
    init_repo(&repo);

    run("git", &["init", "--bare", origin.to_str().unwrap()], &root);
    run(
        "git",
        &["remote", "add", "origin", origin.to_str().unwrap()],
        &repo,
    );
    run("git", &["push", "-u", "origin", "main"], &repo);

    run("git", &["checkout", "-b", "feature/pr-branch"], &repo);
    fs::write(repo.join("pr.txt"), "from pr\n").unwrap();
    run("git", &["add", "pr.txt"], &repo);
    run("git", &["commit", "-m", "pr branch"], &repo);
    run("git", &["push", "-u", "origin", "feature/pr-branch"], &repo);
    run("git", &["push", "origin", "HEAD:refs/pull/123/head"], &repo);
    run("git", &["checkout", "main"], &repo);
    run("git", &["branch", "-D", "feature/pr-branch"], &repo);

    run_wt(&["init"], &repo, &xdg);

    let mock_path = path_with_git_and_script(
        &root,
        "gh",
        "#!/bin/sh\nprintf '{\"number\":123,\"headRefName\":\"feature/pr-branch\"}\\n'\n",
    );
    run_wt_with_path(&["pr", "add", "123"], &repo, &xdg, &mock_path);

    let wt_path = repo.join(".worktrees/feature/pr-branch");
    assert!(wt_path.exists());
    assert_eq!(
        String::from_utf8_lossy(&run("git", &["branch", "--show-current"], &wt_path).stdout).trim(),
        "feature/pr-branch"
    );

    let list = run_wt(&["list", "--quiet"], &repo, &xdg);
    assert!(String::from_utf8_lossy(&list.stdout).contains("feature/pr-branch"));

    let checkout = run_wt_with_path(&["pr", "co", "123"], &repo, &xdg, &mock_path);
    assert!(
        String::from_utf8_lossy(&checkout.stdout).contains(&wt_path.to_string_lossy().to_string())
    );
}

#[cfg(unix)]
#[test]
fn add_does_not_track_origin_main_as_upstream() {
    let root = temp_dir("add-no-upstream");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    let origin = root.join("origin.git");
    init_repo(&repo);

    run("git", &["init", "--bare", origin.to_str().unwrap()], &root);
    run(
        "git",
        &["remote", "add", "origin", origin.to_str().unwrap()],
        &repo,
    );
    run("git", &["push", "-u", "origin", "main"], &repo);

    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "feature1"], &repo, &xdg);

    let wt_path = repo.join(".worktrees/feature1");
    assert!(wt_path.exists());
    assert_eq!(
        String::from_utf8_lossy(&run("git", &["branch", "--show-current"], &wt_path).stdout).trim(),
        "feature1"
    );

    // The new branch must not track origin/main, which would make a bare
    // `git push` potentially update the remote main branch.
    let upstream = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(
        !upstream.status.success(),
        "feature1 should not have an upstream, but got: {}",
        String::from_utf8_lossy(&upstream.stdout)
    );
}

#[cfg(unix)]
#[test]
fn list_pr_batches_gh_lookup_and_falls_back_for_misses() {
    let root = temp_dir("list-pr-batch");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    let log_file = root.join("gh.log");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "feature/batched"], &repo, &xdg);
    run_wt(&["add", "feature/missing"], &repo, &xdg);
    run_wt(&["add", "feature/other"], &repo, &xdg);

    let script = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> {}
case "$*" in
  "pr list --state all --limit 12 --json headRefName,state,isDraft,url,createdAt,number")
    printf '%s\n' '[{{"headRefName":"feature/batched","state":"OPEN","isDraft":false,"url":"https://example.test/pull/7","createdAt":"2026-06-01T00:00:00Z","number":7}},{{"headRefName":"feature/other","state":"MERGED","isDraft":false,"url":"https://example.test/pull/8","createdAt":"2026-06-01T00:00:00Z","number":8}}]'
    ;;
  "pr list --head feature/missing --state all --json state,isDraft,url,createdAt,number")
    printf '%s\n' '[{{"state":"OPEN","isDraft":true,"url":"https://example.test/pull/9","createdAt":"2026-06-02T00:00:00Z","number":9}}]'
    ;;
  *)
    printf '%s\n' '[]'
    ;;
esac
"#,
        shell_quote(&log_file.display().to_string())
    );
    let mock_path = path_with_git_and_script(&root, "gh", &script);

    let output = run_wt_with_path(&["list", "--pr"], &repo, &xdg, &mock_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PR"));
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("feature/batched") && line.contains("#7"))
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("feature/missing") && line.contains("#9"))
    );

    let log = fs::read_to_string(&log_file).unwrap();
    assert_eq!(
        log.matches(
            "pr list --state all --limit 12 --json headRefName,state,isDraft,url,createdAt,number"
        )
        .count(),
        1
    );
    assert!(!log.contains("pr list --head feature/batched"));
    assert!(!log.contains("pr list --head feature/other"));
    assert!(log.contains(
        "pr list --head feature/missing --state all --json state,isDraft,url,createdAt,number"
    ));
}

#[cfg(unix)]
#[test]
fn list_quiet_pr_does_not_call_gh() {
    let root = temp_dir("list-quiet-pr-no-gh");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    let log_file = root.join("gh.log");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "feature/quiet"], &repo, &xdg);

    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nprintf '%s\\n' '[]'\n",
        shell_quote(&log_file.display().to_string())
    );
    let mock_path = path_with_git_and_script(&root, "gh", &script);

    let output = run_wt_with_path(&["list", "--quiet", "--pr"], &repo, &xdg, &mock_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.lines().any(|line| line == "feature/quiet"));
    assert!(!stdout.contains("PR"));
    assert!(!log_file.exists());
}

#[cfg(unix)]
#[test]
fn select_without_fzf_falls_back_to_numbered_prompt() {
    let root = temp_dir("interactive-select-no-fzf");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "fallback-one"], &repo, &xdg);

    let output = run_wt_with_tty_stdin_and_path_and_shell(
        &["select"],
        &repo,
        &xdg,
        "2\n",
        &path_with_git_only(&root),
        &create_test_shell(&root),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    let selected_path = repo.join(".worktrees/fallback-one");

    assert!(combined.contains("Warning: fzf was not found in PATH."));
    assert!(combined.contains("Select Worktree"));
    assert!(combined.contains("1) main (*)"));
    assert!(combined.contains("2) fallback-one"));
    assert!(combined.contains("Choice:"));
    assert!(combined.contains(&selected_path.to_string_lossy().to_string()));
}

#[cfg(unix)]
#[test]
fn select_without_fzf_prefers_exact_numeric_worktree_name() {
    let root = temp_dir("interactive-select-numeric-name");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "1"], &repo, &xdg);
    run_wt(&["add", "other"], &repo, &xdg);

    let output = run_wt_with_tty_stdin_and_path_and_shell(
        &["select"],
        &repo,
        &xdg,
        "1\n",
        &path_with_git_only(&root),
        &create_test_shell(&root),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    let selected_path = repo.join(".worktrees/1");

    assert!(combined.contains("Choice:"));
    assert!(combined.contains(&selected_path.to_string_lossy().to_string()));
    assert!(!combined.contains(&format!("\r\n{}\r\n", repo.display())));
}

#[test]
fn select_without_fzf_preserves_non_interactive_listing() {
    let root = temp_dir("non-interactive-select-no-fzf");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "fallback-one"], &repo, &xdg);

    let output =
        run_wt_with_stdin_and_path(&["select"], &repo, &xdg, "", &path_with_git_only(&root));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.contains("main"));
    assert!(stdout.contains("fallback-one"));
    assert!(!stderr.contains("Warning: fzf was not found in PATH."));
    assert!(!stderr.contains("Choice:"));
}

#[test]
fn two_letter_aliases_dispatch() {
    let root = temp_dir("aliases");
    let repo = root.join("repo");
    let source = root.join("source");
    let xdg = root.join("xdg");
    init_repo(&repo);
    init_repo(&source);

    let cloned = root.join("cloned-with-cn");
    run_wt(
        &["cn", source.to_str().unwrap(), cloned.to_str().unwrap()],
        &root,
        &xdg,
    );
    assert!(cloned.join(".wt/config.toml").exists());

    run_wt(&["in"], &repo, &xdg);
    assert!(repo.join(".wt/config.toml").exists());

    run_wt(&["cf", "worktrees_dir", ".worktrees"], &repo, &xdg);
    run_wt(&["su"], &repo, &xdg);

    run_wt(&["ad", "alias-one"], &repo, &xdg);
    let wt_path = repo.join(".worktrees/alias-one");
    assert!(wt_path.exists());

    let list = run_wt(&["li", "--quiet"], &repo, &xdg);
    assert!(String::from_utf8_lossy(&list.stdout).contains("alias-one"));

    fs::write(wt_path.join("README.md"), "alias changed\n").unwrap();
    let diff = run_wt(&["di", "alias-one", "--", "README.md"], &repo, &xdg);
    assert!(String::from_utf8_lossy(&diff.stdout).contains("alias changed"));

    let selected = run_wt(&["se", "alias-one"], &repo, &xdg);
    assert!(
        String::from_utf8_lossy(&selected.stdout).contains(&wt_path.to_string_lossy().to_string())
    );

    run_wt(&["ru", "alias-one", "touch", "ru-ok.txt"], &repo, &xdg);
    assert!(wt_path.join("ru-ok.txt").exists());

    let current = run_wt(&["cu"], &wt_path, &xdg);
    assert_eq!(String::from_utf8_lossy(&current.stdout).trim(), "alias-one");

    let checkout = run_wt(&["co", "alias-one"], &repo, &xdg);
    assert!(
        String::from_utf8_lossy(&checkout.stdout).contains(&wt_path.to_string_lossy().to_string())
    );

    let completion = run_wt(&["cm", "bash"], &repo, &xdg);
    assert!(String::from_utf8_lossy(&completion.stdout).contains("complete -F"));

    let doctor = run_wt(&["dr"], &repo, &xdg);
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("easy-worktree doctor"));

    run_wt(&["rm", "--force", "alias-one"], &repo, &xdg);
    assert!(!wt_path.exists());

    run_wt(&["ad", "clean-alias"], &repo, &xdg);
    let clean_path = repo.join(".worktrees/clean-alias");
    assert!(clean_path.exists());
    run_wt(&["cl", "--all", "--yes"], &repo, &xdg);
    assert!(!clean_path.exists());

    fs::write(repo.join("alias-stash.txt"), "stash through alias\n").unwrap();
    run_wt(&["st", "stash-alias"], &repo, &xdg);
    let stash_path = repo.join(".worktrees/stash-alias");
    assert!(stash_path.join("alias-stash.txt").exists());
    run_wt(&["rm", "--force", "stash-alias"], &repo, &xdg);
    assert!(!stash_path.exists());
}

#[test]
fn clone_initializes_regular_and_bare_repositories() {
    let root = temp_dir("clone");
    let source = root.join("source");
    let xdg = root.join("xdg");
    init_repo(&source);

    let cloned = root.join("cloned");
    run_wt(
        &["clone", source.to_str().unwrap(), cloned.to_str().unwrap()],
        &root,
        &xdg,
    );
    assert!(cloned.join(".wt/config.toml").exists());

    let bare = root.join("bare-clone.git");
    run_wt(
        &[
            "clone",
            "--bare",
            source.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
        &root,
        &xdg,
    );
    assert!(bare.exists());
    assert!(root.join("bare-clone/main/.wt/config.toml").exists());
}

#[test]
fn stash_moves_uncommitted_changes_to_new_worktree() {
    let root = temp_dir("stash");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);

    fs::write(repo.join("untracked.txt"), "unstaged\n").unwrap();
    run_wt(&["stash", "stash-work"], &repo, &xdg);

    assert!(repo.join(".worktrees/stash-work/untracked.txt").exists());
    assert!(!repo.join("untracked.txt").exists());
}

#[test]
fn global_paths_expand_home_directory() {
    let root = temp_dir("home-expand");
    let home = root.join("home");
    let repo = home.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);

    run_wt_with_home(&["-C", "~/repo", "init"], &root, &xdg, &home);
    assert!(repo.join(".wt/config.toml").exists());

    let list = run_wt_with_home(
        &["--git-dir=~/repo/.git", "list", "--quiet"],
        &root,
        &xdg,
        &home,
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains("main"));
}

#[test]
fn post_add_hook_output_is_routed_to_stderr() {
    let root = temp_dir("hook-output");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);

    let hook = repo.join(".wt/post-add");
    fs::write(&hook, "#!/bin/sh\necho HOOK-OUT\necho HOOK-ERR >&2\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let output = run_wt(&["add", "hook-test"], &repo, &xdg);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains("HOOK-OUT"));
    assert!(stderr.contains("HOOK-OUT"));
    assert!(stderr.contains("HOOK-ERR"));
}

#[test]
fn post_add_hook_output_is_streamed_before_hook_exits() {
    let root = temp_dir("hook-stream");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);

    let hook = repo.join(".wt/post-add");
    fs::write(
        &hook,
        "#!/bin/sh\n\
         echo HOOK-START\n\
         while [ ! -f continue-hook ]; do sleep 0.05; done\n\
         echo HOOK-DONE\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut child = Command::new(wt_bin())
        .args(["add", "stream-hook"])
        .current_dir(&repo)
        .env("LANG", "en")
        .env("LC_ALL", "C")
        .env("XDG_CONFIG_HOME", &xdg)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stderr = child.stderr.take().unwrap();
    let mut reader = BufReader::new(stderr);
    let mut seen = String::new();
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).unwrap();
        assert!(bytes > 0, "wt exited before streaming hook output\n{seen}");
        seen.push_str(&line);
        if line.contains("HOOK-START") {
            break;
        }
    }

    assert!(
        seen.contains("Running post-add hook"),
        "hook start message was not emitted before hook output\n{seen}"
    );
    assert!(
        child.try_wait().unwrap().is_none(),
        "hook output was only observed after the hook exited\n{seen}"
    );

    let wt_path = repo.join(".worktrees/stream-hook");
    fs::write(wt_path.join("continue-hook"), "").unwrap();

    let mut rest = String::new();
    reader.read_to_string(&mut rest).unwrap();
    seen.push_str(&rest);
    let status = child.wait().unwrap();
    assert!(status.success(), "wt add failed\n{seen}");
    assert!(seen.contains("HOOK-DONE"));
}

#[test]
fn post_add_hook_does_not_inherit_wt_stdin() {
    let root = temp_dir("hook-stdin");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);

    let hook = repo.join(".wt/post-add");
    fs::write(
        &hook,
        "#!/bin/sh\n\
         if read value; then\n\
           echo UNEXPECTED-STDIN\n\
         else\n\
           echo STDIN-EOF\n\
         fi\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut child = Command::new(wt_bin())
        .args(["add", "stdin-hook"])
        .current_dir(&repo)
        .env("LANG", "en")
        .env("LC_ALL", "C")
        .env("XDG_CONFIG_HOME", &xdg)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _stdin_guard = child.stdin.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut output = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut output);
        let _ = tx.send(output);
    });

    let stderr = match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(stderr) => stderr,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("wt add blocked while hook waited on inherited stdin: {err}");
        }
    };
    let status = child.wait().unwrap();
    assert!(status.success(), "wt add failed\n{stderr}");
    assert!(stderr.contains("STDIN-EOF"), "{stderr}");
    assert!(!stderr.contains("UNEXPECTED-STDIN"), "{stderr}");
}

#[test]
fn setup_hook_uses_worktree_name_when_detached() {
    let root = temp_dir("detached-setup");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);

    let hook = repo.join(".wt/post-add");
    fs::write(&hook, "#!/bin/sh\necho \"$WT_BRANCH\" > branch.txt\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let worktrees = repo.join(".worktrees");
    fs::create_dir_all(&worktrees).unwrap();
    let detached = worktrees.join("detached");
    run(
        "git",
        &[
            "worktree",
            "add",
            "--detach",
            detached.to_str().unwrap(),
            "HEAD",
        ],
        &repo,
    );

    run_wt(&["setup"], &detached, &xdg);
    assert_eq!(
        fs::read_to_string(detached.join("branch.txt"))
            .unwrap()
            .trim(),
        "detached"
    );
}

#[cfg(unix)]
#[test]
fn init_creates_executable_pre_rm_hook_template() {
    let root = temp_dir("pre-rm-template");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);

    let hook = repo.join(".wt/pre-rm");
    assert!(hook.is_file(), "pre-rm template was not created");
    let mode = fs::metadata(&hook).unwrap().permissions().mode();
    assert_ne!(mode & 0o111, 0, "pre-rm template is not executable");

    let ignore = fs::read_to_string(repo.join(".wt/.gitignore")).unwrap();
    assert!(ignore.contains("pre-rm.local"), "{ignore}");
}

#[cfg(unix)]
#[test]
fn pre_rm_hook_runs_before_worktree_is_removed() {
    let root = temp_dir("pre-rm-run");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "doomed"], &repo, &xdg);

    let worktree = repo.join(".worktrees/doomed");
    assert!(worktree.exists());
    // Resolve before removal; the path is gone by the time the assertions run.
    let worktree_real = fs::canonicalize(&worktree).unwrap();

    write_executable_script(
        &repo.join(".wt/pre-rm"),
        "#!/bin/sh\n\
         {\n\
           echo \"name=$WT_WORKTREE_NAME\"\n\
           echo \"branch=$WT_BRANCH\"\n\
           echo \"action=$WT_ACTION\"\n\
           echo \"base=$WT_BASE_DIR\"\n\
           echo \"path=$WT_WORKTREE_PATH\"\n\
           echo \"cwd=$PWD\"\n\
           [ -d \"$WT_WORKTREE_PATH\" ] && echo worktree-still-present\n\
         } > \"$WT_BASE_DIR/pre-rm.log\"\n",
    );

    let output = run_wt(&["rm", "--force", "doomed"], &repo, &xdg);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Running pre-rm hook"), "{stderr}");
    assert!(!worktree.exists(), "worktree was not removed");

    let log = fs::read_to_string(repo.join("pre-rm.log")).unwrap();
    assert!(log.contains("name=doomed"), "{log}");
    assert!(log.contains("branch=doomed"), "{log}");
    assert!(log.contains("action=rm"), "{log}");
    assert!(
        log.contains("worktree-still-present"),
        "hook ran after the worktree was removed\n{log}"
    );
    let cwd = log
        .lines()
        .find_map(|line| line.strip_prefix("cwd="))
        .unwrap();
    assert_eq!(
        Path::new(cwd),
        worktree_real,
        "hook did not run inside the worktree\n{log}"
    );
}

#[cfg(unix)]
#[test]
fn pre_rm_hook_failure_does_not_block_removal() {
    let root = temp_dir("pre-rm-fail");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "doomed"], &repo, &xdg);

    write_executable_script(
        &repo.join(".wt/pre-rm"),
        "#!/bin/sh\necho HOOK-BOOM >&2\nexit 3\n",
    );

    let output = run_wt(&["rm", "--force", "doomed"], &repo, &xdg);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("HOOK-BOOM"), "{stderr}");
    assert!(stderr.contains("exited with code 3"), "{stderr}");
    assert!(
        !repo.join(".worktrees/doomed").exists(),
        "worktree removal was blocked by a failing hook"
    );
}

#[cfg(unix)]
#[test]
fn clean_runs_pre_rm_hook_for_each_worktree() {
    let root = temp_dir("pre-rm-clean");
    let repo = root.join("repo");
    let xdg = root.join("xdg");
    init_repo(&repo);
    run_wt(&["init"], &repo, &xdg);
    run_wt(&["add", "first"], &repo, &xdg);
    run_wt(&["add", "second"], &repo, &xdg);

    write_executable_script(
        &repo.join(".wt/pre-rm"),
        "#!/bin/sh\necho \"$WT_WORKTREE_NAME:$WT_ACTION\" >> \"$WT_BASE_DIR/cleaned.log\"\n",
    );

    run_wt(&["clean", "--all", "--yes"], &repo, &xdg);

    let log = fs::read_to_string(repo.join("cleaned.log")).unwrap();
    assert!(log.contains("first:rm"), "{log}");
    assert!(log.contains("second:rm"), "{log}");
    assert!(!repo.join(".worktrees/first").exists());
    assert!(!repo.join(".worktrees/second").exists());
}

#[test]
fn global_git_dir_bare_repo_uses_existing_base_worktree() {
    let root = temp_dir("bare");
    let xdg = root.join("xdg");
    let source = root.join("source");
    init_repo(&source);

    let bare = root.join("repo.git");
    run(
        "git",
        &[
            "clone",
            "--bare",
            source.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
        &root,
    );
    let main_wt = root.join("repo-main");
    run(
        "git",
        &[
            &format!("--git-dir={}", bare.display()),
            "worktree",
            "add",
            main_wt.to_str().unwrap(),
            "main",
        ],
        &root,
    );

    run_wt(
        &[&format!("--git-dir={}", bare.display()), "init"],
        &root,
        &xdg,
    );
    assert!(main_wt.join(".wt/config.toml").exists());

    fs::write(main_wt.join("shared.txt"), "shared\n").unwrap();
    run(
        "git",
        &["config", "user.email", "test@example.com"],
        &main_wt,
    );
    run("git", &["config", "user.name", "Test User"], &main_wt);
    run("git", &["add", "shared.txt"], &main_wt);
    run("git", &["commit", "-m", "shared"], &main_wt);
    fs::write(
        main_wt.join(".wt/config.toml"),
        "setup_files = [\"shared.txt\"]\n",
    )
    .unwrap();

    run_wt(
        &[
            &format!("--git-dir={}", bare.display()),
            "add",
            "feature-bare",
        ],
        &root,
        &xdg,
    );
    assert!(root.join(".worktrees/feature-bare/shared.txt").exists());
}
