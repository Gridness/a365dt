use std::{
	collections::HashMap,
	fs::{self, File, OpenOptions},
	io,
	path::{Path, PathBuf},
	sync::Arc,
	time::Duration,
};

use sqlx::{
	SqlitePool,
	migrate::Migrator,
	sqlite::{
		SqliteConnectOptions, SqliteJournalMode, SqliteLockingMode,
		SqlitePoolOptions, SqliteSynchronous,
	},
};

use crate::error::Error;

pub(super) const FILE: &str = "cache.sqlite";
const LOCK_FILE: &str = "cache.lock";
const INITIALIZATION_LOCK_FILE: &str = "cache-initialization.lock";
const LEGACY_FILES: [&str; 2] = ["series.json", "latest-release.json"];
const TIMEOUT: Duration = Duration::from_secs(5);
static MIGRATOR: Migrator = sqlx::migrate!("./migrations/cache");

#[derive(Clone, Debug)]
pub(super) struct Database {
	pub(super) pool: SqlitePool,
	_lock: Arc<File>,
}

pub(super) struct OpenFailure {
	pub(super) error: Error,
	pub(super) rebuildable: bool,
}

#[derive(Clone, Copy)]
enum OpenMode {
	Existing,
	Initialize,
}

#[derive(Clone, Copy)]
enum FailureContext {
	Opening,
	Schema,
}

pub(super) async fn open(directory: &Path) -> Result<Database, OpenFailure> {
	fs::create_dir_all(directory).map_err(|error| OpenFailure {
		error: Error::with_debug("Could not open the local cache.", error),
		rebuildable: false,
	})?;
	let path = directory.join(FILE);
	let cache_lock =
		Arc::new(shared_lock(directory).map_err(|error| OpenFailure {
			error,
			rebuildable: false,
		})?);
	let _initialization_lock =
		initialization_lock(directory).map_err(|error| OpenFailure {
			error,
			rebuildable: false,
		})?;
	if !path.exists() {
		match open_database(
			path.clone(),
			Arc::clone(&cache_lock),
			OpenMode::Initialize,
		)
		.await
		{
			Ok(database) => database.pool.close().await,
			Err(failure) => {
				remove_new_database(&path);
				return Err(failure);
			}
		}
	}
	open_database(path, cache_lock, OpenMode::Existing).await
}

pub(super) async fn rebuild(directory: &Path) -> Result<(), Error> {
	let cache_lock = exclusive_lock(directory)?;
	let path = directory.join(FILE);
	for candidate in files(&path) {
		match fs::remove_file(&candidate) {
			Ok(()) => {}
			Err(error) if error.kind() == io::ErrorKind::NotFound => {}
			Err(error) => {
				return Err(Error::with_debug(
					"Could not rebuild the local cache.",
					format!("{}: {error}", candidate.display()),
				));
			}
		}
	}
	let database =
		open_database(path, Arc::new(cache_lock), OpenMode::Initialize)
			.await
			.map_err(|failure| failure.error)?;
	database.pool.close().await;
	Ok(())
}

pub(super) fn retire_legacy_files(directory: &Path) -> Result<(), Error> {
	for file in LEGACY_FILES {
		let path = directory.join(file);
		match fs::remove_file(&path) {
			Ok(()) => {}
			Err(error) if error.kind() == io::ErrorKind::NotFound => {}
			Err(error) => {
				return Err(Error::with_debug(
					"Could not retire an obsolete local cache file; it will be ignored.",
					format!("{}: {error}", path.display()),
				));
			}
		}
	}
	Ok(())
}

pub(super) fn size(path: &Path) -> Result<u64, Error> {
	files(path).into_iter().try_fold(0_u64, |total, path| {
		match fs::metadata(&path) {
			Ok(metadata) => Ok(total.saturating_add(metadata.len())),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(total),
			Err(error) => {
				Err(read_error(format!("{}: {error}", path.display())))
			}
		}
	})
}

async fn open_database(
	path: PathBuf,
	cache_lock: Arc<File>,
	mode: OpenMode,
) -> Result<Database, OpenFailure> {
	let mut options = SqliteConnectOptions::new()
		.filename(&path)
		.create_if_missing(true)
		.locking_mode(SqliteLockingMode::Normal)
		.synchronous(SqliteSynchronous::Normal)
		.shared_cache(false)
		.foreign_keys(true)
		.busy_timeout(TIMEOUT);
	if matches!(mode, OpenMode::Initialize) {
		options = options.journal_mode(SqliteJournalMode::Wal);
	}
	let pool = SqlitePoolOptions::new()
		.max_connections(1)
		.acquire_timeout(TIMEOUT)
		.connect_with(options)
		.await
		.map_err(|error| open_failure(&path, error, FailureContext::Opening))?;
	if let Err(failure) = migrate(&pool, &path).await {
		pool.close().await;
		return Err(failure);
	}
	if let Err(failure) = validate_schema(&pool, &path).await {
		pool.close().await;
		return Err(failure);
	}
	Ok(Database {
		pool,
		_lock: cache_lock,
	})
}

async fn migrate(pool: &SqlitePool, path: &Path) -> Result<(), OpenFailure> {
	let mut transaction = pool
		.begin_with("BEGIN IMMEDIATE")
		.await
		.map_err(|error| open_failure(path, error, FailureContext::Opening))?;
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
	.map_err(|error| open_failure(path, error, FailureContext::Schema))?;
	let applied = sqlx::query_as::<_, (i64, Vec<u8>, bool)>(
		"SELECT version, checksum, success FROM _sqlx_migrations",
	)
	.fetch_all(&mut *transaction)
	.await
	.map_err(|error| open_failure(path, error, FailureContext::Schema))?
	.into_iter()
	.map(|(version, checksum, success)| (version, (checksum, success)))
	.collect::<HashMap<_, _>>();
	if applied
		.keys()
		.any(|version| !MIGRATOR.version_exists(*version))
	{
		return Err(schema_failure(path, "unknown cache migration"));
	}
	for migration in MIGRATOR
		.iter()
		.filter(|migration| migration.migration_type.is_up_migration())
	{
		if migration.no_tx {
			return Err(schema_failure(
				path,
				"cache migrations must be transactional",
			));
		}
		if let Some((checksum, success)) = applied.get(&migration.version) {
			if !success || checksum.as_slice() != migration.checksum.as_ref() {
				return Err(schema_failure(
					path,
					format!(
						"cache migration {} does not match",
						migration.version
					),
				));
			}
			continue;
		}
		sqlx::raw_sql(migration.sql.as_str())
			.execute(&mut *transaction)
			.await
			.map_err(|error| {
				open_failure(path, error, FailureContext::Schema)
			})?;
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
		.map_err(|error| open_failure(path, error, FailureContext::Schema))?;
	}
	transaction
		.commit()
		.await
		.map_err(|error| open_failure(path, error, FailureContext::Schema))
}

async fn validate_schema(
	pool: &SqlitePool,
	path: &Path,
) -> Result<(), OpenFailure> {
	let state = sqlx::query(
		"SELECT revision, current_generation, last_refresh_revision, \
		 refreshed_at, next_discovery_order, \
		 (SELECT COUNT(*) FROM series), \
		 (SELECT COUNT(*) FROM aliases), \
		 (SELECT COUNT(*) FROM release) \
		 FROM catalogue_state WHERE singleton = 1",
	)
	.fetch_optional(pool)
	.await
	.map_err(|error| open_failure(path, error, FailureContext::Schema))?;
	if state.is_none() {
		return Err(schema_failure(path, "cache state is missing"));
	}
	Ok(())
}

fn shared_lock(directory: &Path) -> Result<File, Error> {
	let file = lock_file(directory, LOCK_FILE)?;
	file.lock_shared().map_err(|error| {
		Error::with_debug("Could not open the local cache.", error)
	})?;
	Ok(file)
}

fn initialization_lock(directory: &Path) -> Result<File, Error> {
	let file = lock_file(directory, INITIALIZATION_LOCK_FILE)?;
	file.lock().map_err(|error| {
		Error::with_debug("Could not initialize the local cache.", error)
	})?;
	Ok(file)
}

fn exclusive_lock(directory: &Path) -> Result<File, Error> {
	let file = lock_file(directory, LOCK_FILE)?;
	file.try_lock().map_err(|error| {
		Error::with_debug(
			"Could not rebuild the local cache while it is in use.",
			error,
		)
	})?;
	Ok(file)
}

fn lock_file(directory: &Path, name: &str) -> Result<File, Error> {
	fs::create_dir_all(directory).map_err(|error| {
		Error::with_debug("Could not open the local cache.", error)
	})?;
	OpenOptions::new()
		.create(true)
		.truncate(false)
		.read(true)
		.write(true)
		.open(directory.join(name))
		.map_err(|error| {
			Error::with_debug("Could not open the local cache.", error)
		})
}

fn files(path: &Path) -> [PathBuf; 3] {
	[
		path.to_owned(),
		sidecar(path, "-wal"),
		sidecar(path, "-shm"),
	]
}

fn remove_new_database(path: &Path) {
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

fn open_failure(
	path: &Path,
	error: sqlx::Error,
	context: FailureContext,
) -> OpenFailure {
	let code = match &error {
		sqlx::Error::Database(error) => error
			.code()
			.and_then(|code| code.parse::<i32>().ok())
			.map(primary_result_code),
		sqlx::Error::Io(_) | _ => None,
	};
	OpenFailure {
		error: Error::with_debug(
			"Could not open the local cache; run `a365dt cache prune` to inspect or reset it.",
			format!("{}: {error}", path.display()),
		),
		rebuildable: is_structural(code, context),
	}
}

fn primary_result_code(code: i32) -> i32 {
	code & 0xff
}

fn is_structural(code: Option<i32>, context: FailureContext) -> bool {
	matches!(code, Some(11 | 26))
		|| (matches!(context, FailureContext::Schema)
			&& matches!(code, Some(1 | 17 | 19 | 20 | 24)))
}

fn schema_failure(path: &Path, detail: impl std::fmt::Display) -> OpenFailure {
	OpenFailure {
		error: Error::with_debug(
			"Could not open the local cache; run `a365dt cache prune` to inspect or reset it.",
			format!("{}: {detail}", path.display()),
		),
		rebuildable: true,
	}
}

fn read_error(error: impl std::fmt::Display) -> Error {
	Error::with_debug(
		"Could not read the local cache; run `a365dt cache prune` to reset it.",
		error,
	)
}

#[cfg(test)]
#[path = "database_tests.rs"]
mod tests;
