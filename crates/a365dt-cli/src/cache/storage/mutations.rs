use std::collections::{HashMap, HashSet};

use sqlx::{QueryBuilder, Sqlite};

use super::{Store, i64_from, now, write_error};
use crate::{api::Series, error::Error, search::normalize_query};

const UPSERT_CHUNK_SIZE: usize = 100;

impl Store {
	pub(in crate::cache) async fn discover(
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
			let order =
				discovery_order(&mut transaction, id, &mut next_order).await?;
			upsert_incremental(&mut transaction, series, revision, order)
				.await?;
		}
		set_next_order(&mut transaction, next_order).await?;
		transaction.commit().await.map_err(write_error)?;
		Ok(Some(revision))
	}

	pub(in crate::cache) async fn remember_alias(
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
		let order =
			discovery_order(&mut transaction, id, &mut next_order).await?;
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

	pub(in crate::cache) async fn remove_missing(
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

	pub(in crate::cache) async fn commit_refresh(
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
		let existing = load_discovery_orders(&mut transaction).await?;
		let revision = next_revision(&mut transaction).await?.0;
		let generation = generation
			.checked_add(1)
			.ok_or_else(|| write_error("cache generation is out of range"))?;
		let mut rows = Vec::with_capacity(series.len());
		for (position, series) in series.iter().enumerate() {
			let id = series_id(series.id)?;
			let position = i64::try_from(position).map_err(write_error)?;
			let order = if let Some(order) = existing.get(&id) {
				*order
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

async fn discovery_order(
	transaction: &mut sqlx::Transaction<'_, Sqlite>,
	id: i64,
	next_order: &mut i64,
) -> Result<i64, Error> {
	if let Some(order) =
		sqlx::query_scalar("SELECT discovery_order FROM series WHERE id = ?")
			.bind(id)
			.fetch_optional(&mut **transaction)
			.await
			.map_err(write_error)?
	{
		return Ok(order);
	}
	let order = *next_order;
	*next_order = increment_order(*next_order)?;
	Ok(order)
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

async fn load_discovery_orders(
	transaction: &mut sqlx::Transaction<'_, Sqlite>,
) -> Result<HashMap<i64, i64>, Error> {
	sqlx::query_as("SELECT id, discovery_order FROM series")
		.fetch_all(&mut **transaction)
		.await
		.map(|rows| rows.into_iter().collect())
		.map_err(write_error)
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

fn series_id(id: u64) -> Result<i64, Error> {
	i64_from(id, "Series ID")
}

fn increment_order(order: i64) -> Result<i64, Error> {
	order
		.checked_add(1)
		.ok_or_else(|| write_error("cache discovery order is out of range"))
}
