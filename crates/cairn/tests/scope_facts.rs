use cairn::facts::{Divergence, GitFacts, Upstream};
use cairn::scope::Git;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tempfile::TempDir;

// Environment changes are confined to this integration-test process and serialized.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

struct Fixture {
    root: TempDir,
    saved_env: Vec<(OsString, OsString)>,
    _guard: MutexGuard<'static, ()>,
}

fn isolated_key(key: &std::ffi::OsStr) -> bool {
    key.to_string_lossy().starts_with("GIT_")
        || ["HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "PATH"]
            .contains(&key.to_str().unwrap_or(""))
}

impl Fixture {
    fn new() -> Self {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let saved_env: Vec<_> = std::env::vars_os()
            .filter(|(k, _)| isolated_key(k))
            .collect();
        for (key, _) in &saved_env {
            if key != "PATH" {
                std::env::remove_var(key);
            }
        }
        for key in ["HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME"] {
            std::env::set_var(key, root.path());
        }
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CONFIG_COUNT", "0");
        Self {
            root,
            saved_env,
            _guard: guard,
        }
    }

    fn repo(&self) -> PathBuf {
        let repo = self.root.path().join("repo");
        fs::create_dir(&repo).unwrap();
        self.git(&repo, &["init", "--initial-branch=main", "--template="]);
        self.git(&repo, &["config", "user.name", "Synthetic Test"]);
        self.git(
            &repo,
            &["config", "user.email", "synthetic@example.invalid"],
        );
        repo
    }

    fn git(&self, directory: &Path, args: &[&str]) -> String {
        let mut output = tempfile::tempfile().unwrap();
        let mut errors = tempfile::tempfile().unwrap();
        let mut child = Command::new("git")
            .args([
                "--no-optional-locks",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(directory)
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(errors.try_clone().unwrap())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(5) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("fixture git timed out: {args:?}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let mut stdout = String::new();
        let mut stderr = String::new();
        output.seek(SeekFrom::Start(0)).unwrap();
        errors.seek(SeekFrom::Start(0)).unwrap();
        output.read_to_string(&mut stdout).unwrap();
        errors.read_to_string(&mut stderr).unwrap();
        assert!(status.success(), "git {args:?}: {stderr}");
        stdout.trim_end().to_owned()
    }

    fn commit(&self, repo: &Path, name: &str) {
        self.git(repo, &["commit", "--allow-empty", "-m", name]);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for (key, _) in std::env::vars_os().filter(|(k, _)| isolated_key(k)) {
            std::env::remove_var(key);
        }
        for (key, value) in &self.saved_env {
            std::env::set_var(key, value);
        }
    }
}

#[test]
fn subdirectories_and_worktrees_share_project_but_not_work_line() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    fixture.commit(&repo, "base");
    let subdirectory = repo.join("nested");
    fs::create_dir(&subdirectory).unwrap();
    let worktree = fixture.root.path().join("linked");
    fixture.git(
        &repo,
        &["worktree", "add", "-b", "other", worktree.to_str().unwrap()],
    );
    let git = Git::default();
    let main = git.resolve(&repo).unwrap();
    let nested = git.resolve(&subdirectory).unwrap();
    let linked = git.resolve(&worktree).unwrap();
    assert!(main.is_git && nested.is_git && linked.is_git);
    assert_eq!(main.project_key, repo.join(".git").canonicalize().unwrap());
    assert_eq!(nested, main);
    assert_eq!(linked.project_key, main.project_key);
    assert_eq!(main.line_path, repo.canonicalize().unwrap());
    assert_eq!(linked.line_path, worktree.canonicalize().unwrap());
    assert_ne!(linked.line_path, main.line_path);
}

#[test]
fn git_timeout_returns_error() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let bin = fixture.root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let fake_git = bin.join("git");
    fs::write(&fake_git, "#!/bin/sh\nexec /bin/sleep 1\n").unwrap();
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o700)).unwrap();
    std::env::set_var("PATH", &bin);
    let start = Instant::now();
    let result = Git::new(Duration::from_millis(50)).resolve(fixture.root.path());
    assert!(result.is_err(), "slow git must return an error");
    assert!(result.unwrap_err().to_string().contains("timed out"));
    assert!(start.elapsed() < Duration::from_millis(800));
}

#[test]
fn non_git_directory_uses_its_canonical_path() {
    let fixture = Fixture::new();
    let directory = fixture.root.path().join("plain");
    fs::create_dir(&directory).unwrap();
    let scope = Git::default().resolve(&directory.join(".")).unwrap();
    assert!(!scope.is_git);
    assert_eq!(scope.project_key, directory.canonicalize().unwrap());
    assert_eq!(scope.line_path, scope.project_key);
}

#[test]
fn rewritten_history_uses_the_prescribed_wording() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    fixture.commit(&repo, "base");
    let base = fixture.git(&repo, &["rev-parse", "HEAD"]);
    fixture.commit(&repo, "recorded");
    let recorded = GitFacts {
        head: Some(fixture.git(&repo, &["rev-parse", "HEAD"])),
        branch: Some("main".to_owned()),
        upstream: None,
        staged: 0,
        unstaged: 0,
        untracked: 0,
        collected_at_unix_ms: 0,
    };
    fixture.git(&repo, &["reset", "--hard", &base]);
    fixture.commit(&repo, "replacement");
    let lines = Git::default().compare(&recorded, &repo).unwrap();
    assert_eq!(
        lines,
        [
            "记录时的 HEAD 已不在当前分支历史中",
            "当前有 0 个已暂存 / 0 个未暂存 / 0 个未跟踪文件",
            "现场变化不说明记录叙述过时。",
        ]
    );
}

#[test]
fn upstream_comparison_uses_only_a_local_ref_without_any_remote() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    fixture.commit(&repo, "base");
    let git = Git::default();
    let recorded = git.collect(&repo).unwrap().unwrap();
    // Exercise the JSON boundary used by the next stage's records.facts storage.
    let recorded: GitFacts =
        serde_json::from_str(&serde_json::to_string(&recorded).unwrap()).unwrap();
    fixture.git(&repo, &["checkout", "-b", "upstream"]);
    fixture.commit(&repo, "local upstream change");
    fixture.git(&repo, &["checkout", "main"]);
    fixture.git(&repo, &["branch", "--set-upstream-to=upstream"]);
    fixture.commit(&repo, "local work line change");
    assert_eq!(fixture.git(&repo, &["remote"]), "");
    let current = git.collect(&repo).unwrap().unwrap();
    assert_eq!(
        current.upstream,
        Some(Upstream {
            name: "upstream".to_owned(),
            divergence: Some(Divergence {
                ahead: 1,
                behind: 1
            }),
        })
    );
    assert_eq!(git.compare(&recorded, &repo).unwrap(), [
        "HEAD 比记录时多 1 个提交",
        "相对本地 upstream ref upstream ahead 1 / behind 1（只基于本地 ref，不代表远端实际状态）",
        "当前有 0 个已暂存 / 0 个未暂存 / 0 个未跟踪文件",
        "现场变化不说明记录叙述过时。",
    ]);
}
