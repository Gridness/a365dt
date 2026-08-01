use std::{
	collections::HashMap,
	fs, io,
	path::{Path, PathBuf},
	time::Duration,
};

use sqlx::{
	Sqlite, SqlitePool, Transaction,
	migrate::Migrator,
	sqlite::{
		SqliteConnectOptions, SqliteJournalMode, SqliteLockingMode,
		SqlitePoolOptions, SqliteSynchronous,
	},
};

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
pub(crate) enum Durability {
	Cache,
	Telemetry,
}

#[derive(Clone, Copy)]
pub(crate) enum OpenMode {
	Existing,
	Initialize,
}

#[derive(Clone, Copy)]
pub(crate) enum FailureContext {
	Opening,
	Schema,
}

pub(crate) enum MigrationError {
	Database(sqlx::Error),
	Invalid(String),
}

pub(crate) fn is_structural(
	error: &sqlx::Error,
	context: FailureContext,
) -> bool {
	let code = result_code(error);
	matches!(code, Some(11 | 26))
		|| (matches!(context, FailureContext::Schema)
			&& matches!(code, Some(1 | 17 | 19 | 20 | 24)))
}

fn result_code(error: &sqlx::Error) -> Option<i32> {
	match error {
		sqlx::Error::Database(error) => error
			.code()
			.and_then(|code| code.parse::<i32>().ok())
			.map(|code| code & 0xff),
		sqlx::Error::Io(_) | _ => None,
	}
}

pub(crate) async fn connect(
	path: &Path,
	mode: OpenMode,
	durability: Durability,
) -> Result<SqlitePool, sqlx::Error> {
	let mut options = SqliteConnectOptions::new()
		.filename(path)
		.create_if_missing(true)
		.locking_mode(SqliteLockingMode::Normal)
		.synchronous(match durability {
			Durability::Cache => SqliteSynchronous::Normal,
			Durability::Telemetry => SqliteSynchronous::Full,
		})
		.shared_cache(false)
		.foreign_keys(true)
		.busy_timeout(TIMEOUT);
	if matches!(mode, OpenMode::Initialize) {
		options = options.journal_mode(SqliteJournalMode::Wal);
	}
	SqlitePoolOptions::new()
		.max_connections(1)
		.acquire_timeout(TIMEOUT)
		.connect_with(options)
		.await
}

pub(crate) async fn begin_migrations<'a>(
	pool: &'a SqlitePool,
	migrator: &'static Migrator,
	name: &str,
) -> Result<(Transaction<'a, Sqlite>, bool), MigrationError> {
	let mut transaction = pool
		.begin_with("BEGIN IMMEDIATE")
		.await
		.map_err(MigrationError::Database)?;
	sqlx::raw_sql(
		"CREATE TABLE IF NOT EXISTS _sqlx_migrations (\
		 version BIGINT PRIMARY KEY, \
		 description TEXT NOT NULL, \
		 installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, \
		 success BOOLEAN NOT NULL, \
		 checksum BLOB NOT NULL, \
		 execution_time BIGINT NOT NULL\
		 )",
	)
	.execute(&mut *transaction)
	.await
	.map_err(MigrationError::Database)?;
	let applied = sqlx::query_as::<_, (i64, Vec<u8>, bool)>(
		"SELECT version, checksum, success FROM _sqlx_migrations",
	)
	.fetch_all(&mut *transaction)
	.await
	.map_err(MigrationError::Database)?
	.into_iter()
	.map(|(version, checksum, success)| (version, (checksum, success)))
	.collect::<HashMap<_, _>>();
	if applied
		.keys()
		.any(|version| !migrator.version_exists(*version))
	{
		return Err(MigrationError::Invalid(format!(
			"unknown {name} migration"
		)));
	}
	let initialize = applied.is_empty();
	for migration in migrator
		.iter()
		.filter(|migration| migration.migration_type.is_up_migration())
	{
		if migration.no_tx {
			return Err(MigrationError::Invalid(format!(
				"{name} migrations must be transactional"
			)));
		}
		if let Some((checksum, success)) = applied.get(&migration.version) {
			if !success || checksum.as_slice() != migration.checksum.as_ref() {
				return Err(MigrationError::Invalid(format!(
					"{name} migration {} does not match",
					migration.version
				)));
			}
			continue;
		}
		sqlx::raw_sql(migration.sql.as_str())
			.execute(&mut *transaction)
			.await
			.map_err(MigrationError::Database)?;
		sqlx::query(
			"INSERT INTO _sqlx_migrations \
			 (version, description, success, checksum, execution_time) \
			 VALUES (?, ?, TRUE, ?, 0)",
		)
		.bind(migration.version)
		.bind(migration.description.as_ref())
		.bind(migration.checksum.as_ref())
		.execute(&mut *transaction)
		.await
		.map_err(MigrationError::Database)?;
	}
	Ok((transaction, initialize))
}

pub(crate) fn files(path: &Path) -> [PathBuf; 3] {
	[
		path.to_owned(),
		sidecar(path, "-wal"),
		sidecar(path, "-shm"),
	]
}

pub(crate) fn size(path: &Path) -> io::Result<u64> {
	files(path).into_iter().try_fold(0_u64, |total, path| {
		match fs::metadata(path) {
			Ok(metadata) => Ok(total.saturating_add(metadata.len())),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(total),
			Err(error) => Err(error),
		}
	})
}

pub(crate) fn remove_new_database(path: &Path) {
	for path in files(path) {
		if let Err(error) = fs::remove_file(path)
			&& error.kind() != io::ErrorKind::NotFound
		{
			break;
		}
	}
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
	let mut path = path.as_os_str().to_owned();
	path.push(suffix);
	PathBuf::from(path)
}
