use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

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
