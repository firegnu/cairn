//! Descriptor-relative private spool access (macOS, DESIGN §6.5).
//!
//! v1 is one JSON object. Its first line is `{"header":{...},`; the
//! following line holds `"payload":{...}}`. This framing lets readers check
//! destination and priority without reading body bytes from a foreign operation.

use crate::save::{Header, Operation, Payload};
use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString, OsString};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("spool directory must be owned by this uid with safe permissions")]
    UnsafeDirectory,
    #[error("invalid spool operation ID or destination")]
    InvalidOperation,
    #[error("no private temporary root available")]
    NoTemporaryRoot,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Resolve the system private root; validation happens when Spool::open opens it.
pub fn trusted_root() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    unsafe {
        let size = libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null_mut(), 0);
        if size > 1 {
            let mut bytes = vec![0u8; size];
            let written = libc::confstr(
                libc::_CS_DARWIN_USER_TEMP_DIR,
                bytes.as_mut_ptr().cast(),
                size,
            );
            if written > 1 && written <= size {
                bytes.truncate(written - 1);
                return Ok(PathBuf::from(OsString::from_vec(bytes)));
            }
        }
    }
    std::env::var_os("TMPDIR")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or(Error::NoTemporaryRoot)
}

pub struct Spool {
    directory: File,
    path: PathBuf,
    database_path: PathBuf,
}

#[derive(Debug)]
pub struct Status {
    pub path: PathBuf,
    pub pending_json: usize,
    pub residual_tmp: usize,
}

impl Spool {
    /// `database_path` must be the lexical absolute path from store::database_path.
    /// Callers/tests may supply an explicit private root; the same checks apply.
    pub fn open(root: &Path, database_path: &Path) -> Result<Self> {
        if !root.is_absolute() || !database_path.is_absolute() {
            return Err(Error::UnsafeDirectory);
        }
        let root_fd = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(root)?;
        check_directory(&root_fd, 0o022)?;
        let base = open_directory(&root_fd, "cairn-spool")?;
        let ns =
            format!("{:x}", Sha256::digest(database_path.as_os_str().as_bytes()))[..16].to_owned();
        let directory = open_directory(&base, &ns)?;
        Ok(Self {
            directory,
            path: root.join("cairn-spool").join(ns),
            database_path: database_path.into(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn status(&self) -> Result<Status> {
        let mut status = Status {
            path: self.path.clone(),
            pending_json: 0,
            residual_tmp: 0,
        };
        for name in self.names(None)? {
            if (name.ends_with(".json") || name.ends_with(".tmp"))
                && self.open_entry(&name)?.is_some()
            {
                status.pending_json += usize::from(name.ends_with(".json"));
                status.residual_tmp += usize::from(name.ends_with(".tmp"));
            }
        }
        Ok(status)
    }

    pub(crate) fn names(&self, deadline: Option<Instant>) -> Result<Vec<String>> {
        // Open a new directory description, not dup(): readdir must not share
        // an offset with the retained namespace handle or another invocation.
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let dir = unsafe { libc::fdopendir(fd) };
        if dir.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(error.into());
        }
        let directory = DirectoryStream(dir);
        let mut names = Vec::new();
        loop {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                break;
            }
            // readdir signals end with NULL and errno=0 on macOS.
            unsafe {
                *libc::__error() = 0;
            }
            let entry = unsafe { libc::readdir(directory.0) };
            if entry.is_null() {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(0) {
                    return Err(error.into());
                }
                break;
            }
            if let Ok(name) = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_str() {
                if name != "." && name != ".." {
                    names.push(name.to_owned());
                }
            }
        }
        names.sort_unstable();
        Ok(names)
    }

    fn open_entry(&self, name: &str) -> Result<Option<File>> {
        let name = CString::new(name).map_err(|_| Error::InvalidOperation)?;
        let file = match file_from_fd(unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        }) {
            Ok(file) => file,
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(libc::ENOENT | libc::ELOOP | libc::EOPNOTSUPP | libc::ENXIO)
                ) =>
            {
                return Ok(None)
            }
            Err(error) => return Err(error.into()),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } {
            return Ok(None);
        }
        Ok(Some(file))
    }

    /// Return a verified destination header and the file positioned at payload.
    /// Capacity 1 is intentional: a foreign body's bytes must not be read ahead.
    pub(crate) fn candidate(
        &self,
        name: &str,
        deadline: Instant,
    ) -> Result<Option<(Header, File)>> {
        let Some(file) = self.open_entry(name)? else {
            return Ok(None);
        };
        let mut reader = BufReader::with_capacity(1, DeadlineFile { file, deadline });
        let mut first = Vec::new();
        if let Err(error) = reader.read_until(b'\n', &mut first) {
            if error.kind() == io::ErrorKind::TimedOut {
                return Ok(None);
            }
            return Err(error.into());
        }
        if !first.ends_with(b",\n") {
            return Ok(None);
        }
        first.truncate(first.len() - 2);
        first.push(b'}');
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            header: Header,
        }
        let Ok(envelope) = serde_json::from_slice::<Envelope>(&first) else {
            return Ok(None);
        };
        if envelope.header.database_path != self.database_path {
            return Ok(None);
        }
        Ok(Some((envelope.header, reader.into_inner().file)))
    }

    pub(crate) fn payload(file: File, deadline: Instant) -> serde_json::Result<Payload> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            payload: Payload,
        }
        serde_json::from_reader::<_, Envelope>(BufReader::new(
            b"{".as_slice().chain(DeadlineFile { file, deadline }),
        ))
        .map(|envelope| envelope.payload)
    }

    pub(crate) fn remove(&self, name: &str) -> Result<()> {
        let name = CString::new(name).map_err(|_| Error::InvalidOperation)?;
        if unsafe { libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::NotFound {
                return Err(error.into());
            }
        }
        Ok(())
    }

    /// Publish once. Neither temporary nor final existing names are overwritten.
    pub fn publish(&self, operation: &Operation) -> Result<()> {
        if !valid_id(&operation.header.op_id)
            || operation.header.database_path != self.database_path
        {
            return Err(Error::InvalidOperation);
        }
        let temp = CString::new(format!(".{}.tmp", operation.header.op_id)).unwrap();
        let final_name = CString::new(format!("{}.json", operation.header.op_id)).unwrap();
        let mut file = file_from_fd(unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                temp.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        })?;
        file.write_all(b"{\"header\":")?;
        serde_json::to_writer(&mut file, &operation.header)?;
        file.write_all(b",\n\"payload\":")?;
        serde_json::to_writer(&mut file, &operation.payload)?;
        file.write_all(b"}\n")?;
        file.sync_all()?;
        #[cfg(target_os = "macos")]
        let renamed = unsafe {
            libc::renameatx_np(
                self.directory.as_raw_fd(),
                temp.as_ptr(),
                self.directory.as_raw_fd(),
                final_name.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(not(target_os = "macos"))]
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "spool publication requires macOS RENAME_EXCL",
        )
        .into());
        #[cfg(target_os = "macos")]
        if renamed != 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }
}

struct DeadlineFile {
    file: File,
    deadline: Instant,
}

impl Read for DeadlineFile {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "spool collection budget exhausted",
            ));
        }
        self.file.read(bytes)
    }
}

struct DirectoryStream(*mut libc::DIR);
impl Drop for DirectoryStream {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}

pub(crate) fn valid_id(id: &str) -> bool {
    id.parse::<ulid::Ulid>()
        .is_ok_and(|value| value.to_string() == id)
}

fn file_from_fd(fd: RawFd) -> io::Result<File> {
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn check_directory(file: &File, forbidden_mode: u32) -> Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & forbidden_mode != 0
    {
        return Err(Error::UnsafeDirectory);
    }
    Ok(())
}

fn open_directory(parent: &File, name: &str) -> Result<File> {
    let name = CString::new(name).unwrap();
    let open = || {
        file_from_fd(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })
    };
    let file = match open() {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(error.into());
                }
            }
            open()?
        }
        Err(error) => return Err(error.into()),
    };
    check_directory(&file, 0o077)?;
    Ok(file)
}
