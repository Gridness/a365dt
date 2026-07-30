use std::{
	io::{self, IsTerminal},
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use sqlx::SqlitePool;

use crate::{app_files, error::Error, ui};

mod catalogue;
mod database;
mod mutations;
mod release;

use database::Database;

#[derive(Clone, Debug)]
pub(crate) struct Store {
	available: Result<Database, Error>,
	path: PathBuf,
	warning: Option<Error>,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq)]
pub(crate) struct Release {
	pub(crate) tag_name: String,
	pub(crate) html_url: String,
}

pub(crate) struct CompletedRelease {
	release: Release,
	completed_at_ms: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ReleaseState {
	Fresh(Release),
	Stale(Release),
	Missing,
}

pub(crate) enum Inspection {
	Ready {
		path: PathBuf,
		refreshed_at: u64,
		series: usize,
		bytes: u64,
		fresh: bool,
		age: Duration,
	},
	Missing {
		path: PathBuf,
		bytes: u64,
	},
	Broken {
		path: PathBuf,
		bytes: Option<u64>,
		detail: String,
	},
}

pub(crate) enum RebuildPermission {
	Ask,
	Preauthorized,
}

impl CompletedRelease {
	pub(crate) fn now(release: Release) -> Self {
		Self {
			release,
			completed_at_ms: now_ms(),
		}
	}
}

impl Store {
	pub(crate) async fn open() -> Self {
		match app_files::cache_directory() {
			Some(directory) => Self::at(directory).await,
			None => {
				let error = Error::new(
					"Could not resolve the user cache directory; check OS configuration.",
				);
				Self {
					available: Err(error),
					path: PathBuf::from("<unresolved>"),
					warning: None,
				}
			}
		}
	}

	pub(super) async fn at(directory: PathBuf) -> Self {
		let path = directory.join(database::FILE);
		match database::open(&directory).await {
			Ok(available) => Self {
				available: Ok(available),
				path,
				warning: database::retire_legacy_files(&directory).err(),
			},
			Err(failure) => Self {
				available: Err(failure.error),
				path,
				warning: None,
			},
		}
	}

	pub(crate) async fn load_release(&self) -> Result<ReleaseState, Error> {
		let Ok(available) = &self.available else {
			return Ok(ReleaseState::Missing);
		};
		release::load(&available.pool).await
	}

	pub(crate) async fn save_release(
		&self,
		completed: CompletedRelease,
	) -> Result<(), Error> {
		let Ok(available) = &self.available else {
			return Ok(());
		};
		release::save(&available.pool, completed).await
	}

	pub(crate) async fn inspect(&self) -> Inspection {
		let available = match &self.available {
			Ok(available) => available,
			Err(error) => {
				return Inspection::Broken {
					path: self.path.clone(),
					bytes: database::size(&self.path).ok(),
					detail: error.render(true),
				};
			}
		};
		match inspect(available, &self.path).await {
			Ok(inspection) => inspection,
			Err(error) => Inspection::Broken {
				path: self.path.clone(),
				bytes: database::size(&self.path).ok(),
				detail: error.render(true),
			},
		}
	}

	pub(crate) async fn close(self) {
		if let Ok(available) = self.available {
			available.pool.close().await;
		}
	}

	pub(crate) fn initialization_warning(&self) -> Option<Error> {
		self.available
			.as_ref()
			.err()
			.cloned()
			.or_else(|| self.warning.clone())
	}
}

pub(crate) async fn prune(permission: RebuildPermission) -> Result<(), Error> {
	let Some(directory) = app_files::cache_directory() else {
		return Ok(());
	};
	prune_at(&directory, permission).await
}

pub(super) async fn prune_at(
	directory: &Path,
	permission: RebuildPermission,
) -> Result<(), Error> {
	match database::open(directory).await {
		Ok(available) => {
			prune_healthy(&available.pool).await?;
			available.pool.close().await;
		}
		Err(failure) if failure.rebuildable => {
			authorize_rebuild(permission)?;
			database::rebuild(directory).await?;
		}
		Err(failure) => return Err(failure.error),
	}
	if let Err(error) = database::retire_legacy_files(directory) {
		ui::warning(error);
	}
	Ok(())
}

async fn inspect(
	available: &Database,
	path: &Path,
) -> Result<Inspection, Error> {
	let (refreshed_at, series): (Option<i64>, i64) = sqlx::query_as(
		"SELECT refreshed_at, (SELECT COUNT(*) FROM series) \
		 FROM catalogue_state WHERE singleton = 1",
	)
	.fetch_one(&available.pool)
	.await
	.map_err(read_error)?;
	if series == 0 {
		return Ok(Inspection::Missing {
			path: path.to_owned(),
			bytes: database::size(path)?,
		});
	}
	let refreshed_at =
		u64_from(refreshed_at.unwrap_or_default(), "refresh time")?;
	let age = Duration::from_secs(now().saturating_sub(refreshed_at));
	Ok(Inspection::Ready {
		path: path.to_owned(),
		refreshed_at,
		series: usize::try_from(series).map_err(read_error)?,
		bytes: database::size(path)?,
		fresh: age < super::MAX_AGE,
		age,
	})
}

async fn prune_healthy(pool: &SqlitePool) -> Result<(), Error> {
	let mut transaction = pool
		.begin_with("BEGIN IMMEDIATE")
		.await
		.map_err(write_error)?;
	sqlx::query("DELETE FROM series")
		.execute(&mut *transaction)
		.await
		.map_err(write_error)?;
	sqlx::query("DELETE FROM release")
		.execute(&mut *transaction)
		.await
		.map_err(write_error)?;
	sqlx::query(
		"UPDATE catalogue_state SET revision = revision + 1, \
		 current_generation = current_generation + 1, \
		 last_refresh_revision = revision + 1, refreshed_at = NULL, \
		 next_discovery_order = 0 WHERE singleton = 1",
	)
	.execute(&mut *transaction)
	.await
	.map_err(write_error)?;
	transaction.commit().await.map_err(write_error)
}

fn authorize_rebuild(permission: RebuildPermission) -> Result<(), Error> {
	match permission {
		RebuildPermission::Preauthorized => Ok(()),
		RebuildPermission::Ask
			if io::stdin().is_terminal() && io::stdout().is_terminal() =>
		{
			if ui::confirm("The local cache is damaged. Rebuild it?", false)? {
				Ok(())
			} else {
				Err("Cancelled.".into())
			}
		}
		RebuildPermission::Ask => Err(Error::new(
			"The local cache is damaged; run `a365dt cache prune --yes` to rebuild it.",
		)),
	}
}

fn i64_from(value: u64, name: &str) -> Result<i64, Error> {
	i64::try_from(value).map_err(|error| {
		write_error(format!("{name} is out of range: {error}"))
	})
}

fn u64_from(value: i64, name: &str) -> Result<u64, Error> {
	u64::try_from(value)
		.map_err(|error| read_error(format!("{name} is out of range: {error}")))
}

fn read_error(error: impl std::fmt::Display) -> Error {
	Error::with_debug(
		"Could not read the local cache; run `a365dt cache prune` to reset it.",
		error,
	)
}

fn write_error(error: impl std::fmt::Display) -> Error {
	Error::with_debug(
		"Could not update the local cache; run `a365dt cache prune` to reset it.",
		error,
	)
}

fn now() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs()
}

fn now_ms() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |duration| {
			u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
		})
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
