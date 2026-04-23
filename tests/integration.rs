use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
