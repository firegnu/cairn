//! SQLite 存储：位置、设置、表结构与迁移（DESIGN §6）。

use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};

mod schema;

pub const SCHEMA_VERSION: u64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("database schema version {found} is newer than supported version {SCHEMA_VERSION}")]
    NewerSchema { found: u64 },
    #[error("missing or invalid meta.schema_version")]
    InvalidSchemaVersion,
    #[error("XDG_STATE_HOME or HOME must supply a nonempty absolute state directory")]
    InvalidStateDirectory,
    #[error("permissions are too broad for database access: {0}")]
    InsecurePermissions(PathBuf),
    #[error("database path must be an owned directory/regular file, never a symlink: {0}")]
    UnsafePath(PathBuf),
    #[error("body is tombstoned but WAL truncation is busy; retry deletion to finish erasing it")]
    CheckpointBusy,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Compute the database path from XDG_STATE_HOME and HOME values without I/O.
/// Empty XDG_STATE_HOME falls back to HOME/.local/state. Relative roots are rejected.
/// Normalization is lexical: symlinks are not resolved and no path need exist.
pub fn database_path(xdg_state_home: Option<&Path>, home: Option<&Path>) -> Result<PathBuf> {
    let root = match xdg_state_home.filter(|path| !path.as_os_str().is_empty()) {
        Some(path) => path.to_path_buf(),
        None => home
            .filter(|path| path.is_absolute())
            .ok_or(Error::InvalidStateDirectory)?
            .join(".local/state"),
    };
    if !root.is_absolute() {
        return Err(Error::InvalidStateDirectory);
    }
    let mut normalized = PathBuf::new();
    for component in root.join("cairn/cairn.db").components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

#[derive(Clone, Copy, Debug)]
pub enum BusyTimeout {
    /// Hooks wait at most about 200 ms for each SQLite lock attempt.
    Hook,
    /// User commands may wait up to about two seconds per lock attempt.
    UserCommand,
}

impl BusyTimeout {
    fn duration(self) -> Duration {
        match self {
            Self::Hook => Duration::from_millis(200),
            Self::UserCommand => Duration::from_secs(2),
        }
    }
}

#[derive(Debug)]
pub struct Store {
    connection: Connection,
}

impl Store {
    /// Create/open and migrate the database with a 0700 directory and 0600 files.
    /// Existing paths must be owned by this user, have private permissions, and
    /// be real directories/regular files, not symlinks. Unsafe paths are rejected
    /// before SQLite access, without chmod; future schema versions are not modified.
    /// Concurrent first initialization may return SQLITE_BUSY immediately, even
    /// with a busy timeout. Callers must treat that error as retryable.
    pub fn open(path: impl AsRef<Path>, timeout: BusyTimeout) -> Result<Self> {
        let path =
            checked_database_path(path.as_ref(), true)?.ok_or(Error::InvalidStateDirectory)?;
        // The filesystem is already verified, including private permissions on
        // the main file from which SQLite derives new WAL/SHM permissions.
        // Use a read-only probe so rejecting a future schema cannot checkpoint it.
        {
            let probe = Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )?;
            probe.busy_timeout(timeout.duration())?;
            schema_version(&probe)?;
        }
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(timeout.duration())?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "secure_delete", true)?;
        let mut store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    /// Open an existing v1 WAL database without migration or chmod. Missing paths
    /// return None; the same path safety checks as open apply before SQLite access,
    /// including to sidecars even when the database is absent. SQLite may create WAL
    /// sidecars for an existing database, but never the database or its directory.
    pub fn open_read_only(path: impl AsRef<Path>, timeout: BusyTimeout) -> Result<Option<Self>> {
        let Some(path) = checked_database_path(path.as_ref(), false)? else {
            return Ok(None);
        };
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(timeout.duration())?;
        if schema_version(&connection)? != SCHEMA_VERSION {
            return Err(Error::InvalidSchemaVersion);
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "secure_delete", true)?;
        Ok(Some(Self { connection }))
    }

    /// SQL access for the business modules. Callers must preserve the schema,
    /// connection pragmas, and append-only record semantics; use delete_body to erase.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// A rusqlite transaction rolls back on drop unless explicitly committed.
    pub fn transaction(&mut self, behavior: TransactionBehavior) -> Result<Transaction<'_>> {
        Ok(self.connection.transaction_with_behavior(behavior)?)
    }

    /// Initialize an unversioned empty database atomically; v1 is a no-op.
    pub fn migrate(&mut self) -> Result<()> {
        if schema_version(&self.connection)? == SCHEMA_VERSION {
            return Ok(());
        }
        let tx = self.transaction(TransactionBehavior::Immediate)?;
        if schema_version(&tx)? == 0 {
            tx.execute_batch(schema::V1)?;
            tx.execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
                [SCHEMA_VERSION.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Erase a body's stored bytes, retaining its metadata and first deletion time.
    /// Returns false if the ID is absent. The timestamp is supplied by the caller.
    /// A checkpoint error occurs AFTER the tombstone is committed; retrying this
    /// operation on the same ID is safe and retries WAL truncation as well.
    pub fn delete_body(&mut self, id: &str, deleted_at: &str) -> Result<bool> {
        let tx = self.transaction(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE records SET body = NULL, deleted_at = COALESCE(deleted_at, ?2) WHERE id = ?1",
            [id, deleted_at],
        )?;
        tx.commit()?;
        if changed == 0 {
            return Ok(false);
        }
        let busy: i64 =
            self.connection
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            return Err(Error::CheckpointBusy);
        }
        Ok(true)
    }
}

// Check controlled entries without following symlinks. In particular, dangling
// symlinks must not be mistaken for absent files by Path::try_exists().
fn checked_metadata(path: &Path, directory: bool) -> Result<Option<fs::Metadata>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let correct_type = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    // SAFETY: geteuid has no arguments or memory preconditions.
    if !correct_type || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(Error::UnsafePath(path.to_path_buf()));
    }
    let allowed = if directory { 0o700 } else { 0o600 };
    if metadata.permissions().mode() & 0o7777 & !allowed != 0 {
        return Err(Error::InsecurePermissions(path.to_path_buf()));
    }
    Ok(Some(metadata))
}

fn checked_database_path(path: &Path, create: bool) -> Result<Option<PathBuf>> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or(Error::InvalidStateDirectory)?;
    let directory = match checked_metadata(parent, true)? {
        Some(metadata) => metadata,
        None if !create => return Ok(None),
        None => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)?;
            checked_metadata(parent, true)?.ok_or(Error::UnsafePath(parent.to_path_buf()))?
        }
    };
    // Resolve ancestor aliases (e.g. macOS /var -> /private/var), but never the
    // database or sidecars. The controlled directory itself must not be a link.
    let resolved_parent = fs::canonicalize(parent)?;
    let resolved =
        checked_metadata(&resolved_parent, true)?.ok_or(Error::UnsafePath(parent.to_path_buf()))?;
    if (directory.dev(), directory.ino()) != (resolved.dev(), resolved.ino()) {
        return Err(Error::UnsafePath(parent.to_path_buf()));
    }
    let path = resolved_parent.join(path.file_name().ok_or(Error::InvalidStateDirectory)?);
    let exists = checked_metadata(&path, false)?.is_some();
    for suffix in ["-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        checked_metadata(Path::new(&name), false)?;
    }
    if !exists {
        if !create {
            return Ok(None);
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
        {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        // A concurrent initializer may have created the file first. Recheck all
        // controlled entries before the first SQLite access in either case.
        return checked_database_path(&path, false);
    }
    Ok(Some(path))
}

fn schema_version(connection: &Connection) -> Result<u64> {
    let has_meta: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'meta')",
        [],
        |row| row.get(0),
    )?;
    if !has_meta {
        return Ok(0);
    }
    let version: Option<String> = connection
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let version: u64 = version
        .and_then(|value| value.parse().ok())
        .filter(|version| *version > 0)
        .ok_or(Error::InvalidSchemaVersion)?;
    if version > SCHEMA_VERSION {
        return Err(Error::NewerSchema { found: version });
    }
    Ok(version)
}
