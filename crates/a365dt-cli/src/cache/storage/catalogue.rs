use std::collections::{BTreeMap, HashMap};

use super::{Store, read_error, u64_from};
use crate::{
	api::Series,
	cache::{catalogue::Catalogue, writer::LoadedCatalogue},
	error::Error,
};

struct StoredSeries {
	id: i64,
	title: String,
	year: Option<i64>,
	type_title: Option<String>,
	episode_count: Option<i64>,
}

impl Store {
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
