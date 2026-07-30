use std::{
	collections::{BTreeMap, HashMap, HashSet},
	io::{self, IsTerminal},
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::{catalogue::Catalogue, writer::LoadedCatalogue};
use crate::{
	api::Series, app_files, error::Error, search::normalize_query, ui,
};

const UPSERT_CHUNK_SIZE: usize = 100;

mod database;
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
		detail: String,
	},
}

pub(crate) enum RebuildPermission {
	Ask,
	Preauthorized,
}

struct StoredSeries {
	id: i64,
	title: String,
	year: Option<i64>,
	type_title: Option<String>,
	episode_count: Option<i64>,
}

struct StoredRevision {
	discovery_order: i64,
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

	pub(crate) async fn load_catalogue(
		&self,
	) -> Result<LoadedCatalogue, Error> {
		let Ok(available) = &self.available else {
			return Ok(LoadedCatalogue::unavailable());
		};
		let mut transaction =
			available.pool.begin().await.map_err(read_error)?;
		let (revision, generation, refreshed_at): (i64, i64, Option<i64>) =
			sqlx::query_as(
				"SELECT revision, current_generation, refreshed_at \
				 FROM catalogue_state WHERE singleton = 1",
			)
			.fetch_one(&mut *transaction)
			.await
			.map_err(read_error)?;
		let rows = sqlx::query_as::<
			_,
			(i64, String, Option<i64>, Option<String>, Option<i64>, i64),
		>(
			"SELECT id, title, year, type_title, episode_count, revision \
			 FROM series \
			 ORDER BY CASE WHEN refresh_generation = ? THEN 0 ELSE 1 END, \
			 refresh_position, discovery_order, id",
		)
		.bind(generation)
		.fetch_all(&mut *transaction)
		.await
		.map_err(read_error)?;
		let aliases = sqlx::query_as::<_, (String, i64)>(
			"SELECT query, series_id FROM aliases ORDER BY query",
		)
		.fetch_all(&mut *transaction)
		.await
		.map_err(read_error)?;
		transaction.commit().await.map_err(read_error)?;

		let revisions = rows
			.iter()
			.map(|row| -> Result<_, Error> {
				Ok((u64_from(row.0, "Series ID")?, row.5))
			})
			.collect::<Result<HashMap<_, _>, Error>>()?;
		let series = rows
			.into_iter()
			.map(|row| {
				series_from(StoredSeries {
					id: row.0,
					title: row.1,
					year: row.2,
					type_title: row.3,
					episode_count: row.4,
				})
			})
			.collect::<Result<Vec<_>, _>>()?;
		let aliases = aliases
			.into_iter()
			.map(|(query, id)| Ok((query, u64_from(id, "alias Series ID")?)))
			.collect::<Result<BTreeMap<_, _>, Error>>()?;
		Ok(LoadedCatalogue::new(
			Catalogue::from_parts(
				u64_from(refreshed_at.unwrap_or_default(), "refresh time")?,
				series,
				aliases,
			),
			revision,
			revisions,
		))
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
					detail: error.render(true),
				};
			}
		};
		match inspect(available, &self.path).await {
			Ok(inspection) => inspection,
			Err(error) => Inspection::Broken {
				path: self.path.clone(),
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

	pub(super) async fn discover(
		&self,
		series: Vec<Series>,
	) -> Result<Option<i64>, Error> {
		let series = deduplicate_last(series);
		if series.is_empty() {
			return Ok(None);
		}
		let available = self.available.as_ref().map_err(Clone::clone)?;
		let mut transaction = available
			.pool
			.begin_with("BEGIN IMMEDIATE")
			.await
			.map_err(write_error)?;
		let (revision, mut next_order) =
			next_revision(&mut transaction).await?;
		for series in &series {
			let id = series_id(series.id)?;
			let stored_order = sqlx::query_scalar::<_, i64>(
				"SELECT discovery_order FROM series WHERE id = ?",
			)
			.bind(id)
			.fetch_optional(&mut *transaction)
			.await
			.map_err(write_error)?;
			let order = if let Some(order) = stored_order {
				order
			} else {
				let order = next_order;
				next_order = increment_order(next_order)?;
				order
			};
			upsert_incremental(&mut transaction, series, revision, order)
				.await?;
		}
		set_next_order(&mut transaction, next_order).await?;
		transaction.commit().await.map_err(write_error)?;
		Ok(Some(revision))
	}

	pub(super) async fn remember_alias(
		&self,
		query: String,
		series: Series,
	) -> Result<Option<i64>, Error> {
		let query = normalize_query(&query);
		if query.is_empty() {
			return Ok(None);
		}
		let available = self.available.as_ref().map_err(Clone::clone)?;
		let mut transaction = available
			.pool
			.begin_with("BEGIN IMMEDIATE")
			.await
			.map_err(write_error)?;
		let (revision, mut next_order) =
			next_revision(&mut transaction).await?;
		let id = series_id(series.id)?;
		let stored_order = sqlx::query_scalar::<_, i64>(
			"SELECT discovery_order FROM series WHERE id = ?",
		)
		.bind(id)
		.fetch_optional(&mut *transaction)
		.await
		.map_err(write_error)?;
		let order = if let Some(order) = stored_order {
			order
		} else {
			let order = next_order;
			next_order = increment_order(next_order)?;
			order
		};
		upsert_incremental(&mut transaction, &series, revision, order).await?;
		sqlx::query(
			"INSERT INTO aliases(query, series_id) VALUES (?, ?) \
			 ON CONFLICT(query) DO UPDATE SET series_id = excluded.series_id",
		)
		.bind(query)
		.bind(id)
		.execute(&mut *transaction)
		.await
		.map_err(write_error)?;
		set_next_order(&mut transaction, next_order).await?;
		transaction.commit().await.map_err(write_error)?;
		Ok(Some(revision))
	}

	pub(super) async fn remove_missing(
		&self,
		id: u64,
		expected_revision: Option<i64>,
	) -> Result<(), Error> {
		let Some(expected_revision) = expected_revision else {
			return Ok(());
		};
		let available = self.available.as_ref().map_err(Clone::clone)?;
		let mut transaction = available
			.pool
			.begin_with("BEGIN IMMEDIATE")
			.await
			.map_err(write_error)?;
		let deleted =
			sqlx::query("DELETE FROM series WHERE id = ? AND revision = ?")
				.bind(series_id(id)?)
				.bind(expected_revision)
				.execute(&mut *transaction)
				.await
				.map_err(write_error)?
				.rows_affected();
		if deleted != 0 {
			next_revision(&mut transaction).await?;
		}
		transaction.commit().await.map_err(write_error)
	}

	pub(super) async fn commit_refresh(
		&self,
		series: Vec<Series>,
		base_revision: i64,
	) -> Result<Option<i64>, Error> {
		let available = self.available.as_ref().map_err(Clone::clone)?;
		let mut transaction = available
			.pool
			.begin_with("BEGIN IMMEDIATE")
			.await
			.map_err(write_error)?;
		let (last_refresh, generation, mut next_order): (i64, i64, i64) =
			sqlx::query_as(
				"SELECT last_refresh_revision, current_generation, \
				 next_discovery_order FROM catalogue_state \
				 WHERE singleton = 1",
			)
			.fetch_one(&mut *transaction)
			.await
			.map_err(write_error)?;
		if last_refresh > base_revision {
			transaction.commit().await.map_err(write_error)?;
			return Ok(None);
		}
		let series = deduplicate_first(series);
		let existing = load_stored_series(&mut transaction).await?;
		let revision = next_revision(&mut transaction).await?.0;
		let generation = generation
			.checked_add(1)
			.ok_or_else(|| write_error("cache generation is out of range"))?;
		let mut rows = Vec::with_capacity(series.len());
		for (position, series) in series.iter().enumerate() {
			let id = series_id(series.id)?;
			let position = i64::try_from(position).map_err(write_error)?;
			let order = if let Some(series) = existing.get(&id) {
				series.discovery_order
			} else {
				let order = next_order;
				next_order = increment_order(next_order)?;
				order
			};
			rows.push((id, position, series, order));
		}
		for chunk in rows.chunks(UPSERT_CHUNK_SIZE) {
			upsert_refresh(
				&mut transaction,
				chunk,
				revision,
				generation,
				base_revision,
			)
			.await?;
		}
		sqlx::query(
			"DELETE FROM series \
			 WHERE refresh_generation IS NOT ? AND revision <= ? \
			 AND NOT EXISTS (\
				SELECT 1 FROM aliases \
				WHERE aliases.series_id = series.id\
			 )",
		)
		.bind(generation)
		.bind(base_revision)
		.execute(&mut *transaction)
		.await
		.map_err(write_error)?;
		sqlx::query(
			"UPDATE series SET revision = ?, \
			 refresh_generation = NULL, refresh_position = NULL \
			 WHERE refresh_generation IS NOT ?",
		)
		.bind(revision)
		.bind(generation)
		.execute(&mut *transaction)
		.await
		.map_err(write_error)?;
		sqlx::query(
			"UPDATE catalogue_state SET \
			 current_generation = ?, last_refresh_revision = ?, \
			 refreshed_at = ?, next_discovery_order = ? \
			 WHERE singleton = 1",
		)
		.bind(generation)
		.bind(revision)
		.bind(i64_from(now(), "refresh time")?)
		.bind(next_order)
		.execute(&mut *transaction)
		.await
		.map_err(write_error)?;
		transaction.commit().await.map_err(write_error)?;
		Ok(Some(revision))
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

async fn next_revision(
	transaction: &mut sqlx::Transaction<'_, Sqlite>,
) -> Result<(i64, i64), Error> {
	sqlx::query_as(
		"UPDATE catalogue_state SET revision = revision + 1 \
		 WHERE singleton = 1 RETURNING revision, next_discovery_order",
	)
	.fetch_one(&mut **transaction)
	.await
	.map_err(write_error)
}

async fn set_next_order(
	transaction: &mut sqlx::Transaction<'_, Sqlite>,
	next_order: i64,
) -> Result<(), Error> {
	sqlx::query(
		"UPDATE catalogue_state SET next_discovery_order = ? \
		 WHERE singleton = 1",
	)
	.bind(next_order)
	.execute(&mut **transaction)
	.await
	.map_err(write_error)?;
	Ok(())
}

async fn upsert_incremental(
	transaction: &mut sqlx::Transaction<'_, Sqlite>,
	series: &Series,
	revision: i64,
	order: i64,
) -> Result<(), Error> {
	sqlx::query(
		"INSERT INTO series \
		 (id, title, year, type_title, episode_count, revision, \
		 refresh_generation, refresh_position, discovery_order) \
		 VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, ?) \
		 ON CONFLICT(id) DO UPDATE SET \
		 title = excluded.title, year = excluded.year, \
		 type_title = excluded.type_title, \
		 episode_count = excluded.episode_count, \
		 revision = excluded.revision",
	)
	.bind(series_id(series.id)?)
	.bind(&series.title)
	.bind(series.year.map(i64::from))
	.bind(&series.type_title)
	.bind(series.number_of_episodes.map(i64::from))
	.bind(revision)
	.bind(order)
	.execute(&mut **transaction)
	.await
	.map_err(write_error)?;
	Ok(())
}

async fn upsert_refresh(
	transaction: &mut sqlx::Transaction<'_, Sqlite>,
	rows: &[(i64, i64, &Series, i64)],
	revision: i64,
	generation: i64,
	base_revision: i64,
) -> Result<(), Error> {
	let mut query = QueryBuilder::<Sqlite>::new(
		"INSERT INTO series \
		 (id, title, year, type_title, episode_count, revision, \
		 refresh_generation, refresh_position, discovery_order) ",
	);
	query.push_values(rows, |mut row, (id, position, series, order)| {
		row.push_bind(*id)
			.push_bind(&series.title)
			.push_bind(series.year.map(i64::from))
			.push_bind(&series.type_title)
			.push_bind(series.number_of_episodes.map(i64::from))
			.push_bind(revision)
			.push_bind(generation)
			.push_bind(*position)
			.push_bind(*order);
	});
	query.push(
		" ON CONFLICT(id) DO UPDATE SET \
		 title = CASE WHEN series.revision > ",
	);
	query
		.push_bind(base_revision)
		.push(
			" THEN series.title ELSE excluded.title END, \
			 year = CASE WHEN series.revision > ",
		)
		.push_bind(base_revision)
		.push(
			" THEN series.year ELSE excluded.year END, \
			 type_title = CASE WHEN series.revision > ",
		)
		.push_bind(base_revision)
		.push(
			" THEN series.type_title ELSE excluded.type_title END, \
			 episode_count = CASE WHEN series.revision > ",
		)
		.push_bind(base_revision)
		.push(
			" THEN series.episode_count ELSE excluded.episode_count END, \
			 revision = excluded.revision, \
			 refresh_generation = excluded.refresh_generation, \
			 refresh_position = excluded.refresh_position",
		);
	query
		.build()
		.execute(&mut **transaction)
		.await
		.map_err(write_error)?;
	Ok(())
}

async fn load_stored_series(
	transaction: &mut sqlx::Transaction<'_, Sqlite>,
) -> Result<HashMap<i64, StoredRevision>, Error> {
	let rows = sqlx::query_as::<_, (i64, i64)>(
		"SELECT id, discovery_order FROM series",
	)
	.fetch_all(&mut **transaction)
	.await
	.map_err(write_error)?;
	Ok(rows
		.into_iter()
		.map(|row| {
			(
				row.0,
				StoredRevision {
					discovery_order: row.1,
				},
			)
		})
		.collect())
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

fn deduplicate_last(series: Vec<Series>) -> Vec<Series> {
	let mut positions = HashMap::new();
	let mut unique = Vec::new();
	for series in series {
		if let Some(position) = positions.get(&series.id).copied() {
			unique[position] = series;
		} else {
			positions.insert(series.id, unique.len());
			unique.push(series);
		}
	}
	unique
}

fn deduplicate_first(series: Vec<Series>) -> Vec<Series> {
	let mut seen = HashSet::new();
	series
		.into_iter()
		.filter(|series| seen.insert(series.id))
		.collect()
}

fn series_from(series: StoredSeries) -> Result<Series, Error> {
	Ok(Series {
		id: u64_from(series.id, "Series ID")?,
		title: series.title,
		year: series
			.year
			.map(|year| u16::try_from(year).map_err(read_error))
			.transpose()?,
		type_title: series.type_title,
		number_of_episodes: series
			.episode_count
			.map(|count| u32::try_from(count).map_err(read_error))
			.transpose()?,
		poster_url_small: None,
		episodes: Vec::new(),
	})
}

fn series_id(id: u64) -> Result<i64, Error> {
	i64_from(id, "Series ID")
}

fn increment_order(order: i64) -> Result<i64, Error> {
	order
		.checked_add(1)
		.ok_or_else(|| write_error("cache discovery order is out of range"))
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
