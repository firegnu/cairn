//! 判定项目与工作线（DESIGN §3、§5）。

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub project_key: PathBuf,
    pub line_path: PathBuf,
    pub is_git: bool,
}

#[derive(Debug)]
pub enum GitError {
    Io(io::Error),
    Timeout(Duration),
    Failed { code: Option<i32>, stderr: String },
    InvalidOutput(&'static str),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "git I/O error: {error}"),
            Self::Timeout(timeout) => write!(f, "git timed out after {timeout:?}"),
            Self::Failed { code, stderr } => write!(f, "git failed ({code:?}): {stderr}"),
            Self::InvalidOutput(message) => write!(f, "invalid git output: {message}"),
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for GitError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Shared local-only Git access. The timeout applies to each individual command.
pub struct Git {
    timeout: Duration,
}

impl Default for Git {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl Git {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub fn resolve(&self, directory: &Path) -> Result<Scope, GitError> {
        let directory = directory.canonicalize()?;
        let common = self.run(
            &directory,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        if !common.status.success() {
            // Git has no distinct exit code for discovery failure. Match only the
            // C-locale diagnostics for absent repositories, never all exit-128 errors.
            let stderr = String::from_utf8_lossy(&common.stderr);
            if common.status.code() == Some(128)
                && (stderr.starts_with(
                    "fatal: not a git repository (or any of the parent directories): .git\n",
                ) || stderr
                    .starts_with("fatal: not a git repository (or any parent up to mount point "))
            {
                return Ok(Scope {
                    project_key: directory.clone(),
                    line_path: directory,
                    is_git: false,
                });
            }
            return Err(failed(common));
        }
        let top = self.checked(&directory, &["rev-parse", "--show-toplevel"])?;
        Ok(Scope {
            project_key: canonical_git_path(common.stdout)?,
            line_path: canonical_git_path(top)?,
            is_git: true,
        })
    }

    pub(crate) fn run(&self, directory: &Path, args: &[&str]) -> Result<Output, GitError> {
        // Files avoid pipe backpressure without reader threads that could outlive a timeout.
        let mut stdout = tempfile::tempfile()?;
        let mut stderr = tempfile::tempfile()?;
        let mut command = Command::new("git");
        command
            .args([
                "--no-optional-locks",
                "--no-pager",
                "-c",
                "core.fsmonitor=false",
            ])
            .args(args)
            .current_dir(directory)
            .env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .stdin(Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?);
        // The supplied directory, not an enclosing hook's Git environment, owns the scope.
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_PREFIX",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CEILING_DIRECTORIES",
        ] {
            command.env_remove(key);
        }
        let started = Instant::now();
        let mut child = command.spawn()?;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error.into());
                }
            }
            if started.elapsed() >= self.timeout {
                let kill_result = child.kill();
                let wait_result = child.wait();
                kill_result?;
                wait_result?;
                return Err(GitError::Timeout(self.timeout));
            }
            std::thread::sleep(
                Duration::from_millis(5).min(self.timeout.saturating_sub(started.elapsed())),
            );
        };
        stdout.seek(SeekFrom::Start(0))?;
        stderr.seek(SeekFrom::Start(0))?;
        let mut output = Output {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        stdout.read_to_end(&mut output.stdout)?;
        stderr.read_to_end(&mut output.stderr)?;
        Ok(output)
    }

    pub(crate) fn checked(&self, directory: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
        let output = self.run(directory, args)?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(failed(output))
        }
    }
}

pub(crate) fn failed(output: Output) -> GitError {
    GitError::Failed {
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr)
            .trim_end()
            .to_owned(),
    }
}

fn canonical_git_path(mut bytes: Vec<u8>) -> Result<PathBuf, GitError> {
    if bytes.pop() != Some(b'\n') {
        return Err(GitError::InvalidOutput(
            "expected a path terminated by newline",
        ));
    }
    Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)).canonicalize()?)
}
