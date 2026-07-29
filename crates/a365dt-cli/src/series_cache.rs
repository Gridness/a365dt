use std::{
	collections::{BTreeMap, HashMap, HashSet},
	fs, io,
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
	api::Series,
	app_files,
	search::{Search, normalize_query},
	telemetry::{CatalogueUse, Operation, Recorder},
};

pub(crate) const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

type Row = [String; 4];

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct PersistedCatalogue {
	refreshed_at: u64,
	series: Vec<Series>,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	aliases: BTreeMap<String, u64>,
}

#[derive(Debug)]
struct Index {
	rows: Vec<Row>,
	search: Search,
}

#[derive(Debug, Default)]
pub struct Catalogue {
	persisted: PersistedCatalogue,
	started_with: HashSet<u64>,
	index: Option<Index>,
}

impl Clone for Catalogue {
	fn clone(&self) -> Self {
		Self {
			persisted: self.persisted.clone(),
			started_with: self.started_with.clone(),
			index: None,
		}
	}
}

pub struct Suggestions<'a> {
	series: &'a [Series],
	rows: &'a [Row],
	matches: Vec<usize>,
}

impl Catalogue {
	pub fn new(series: Vec<Series>) -> Self {
		let mut catalogue = Self::default();
		catalogue.upsert(series);
		catalogue.started_with = catalogue.ids();
		catalogue
	}

	pub fn refreshed(series: Vec<Series>) -> Self {
		let mut seen = HashSet::new();
		let series = series
			.into_iter()
			.filter(|series| seen.insert(series.id))
			.collect();
		let mut catalogue = Self::new(series);
		catalogue.persisted.refreshed_at = now();
		catalogue
	}

	pub fn is_fresh(&self) -> bool {
		self.is_fresh_at(now())
	}

	pub fn is_empty(&self) -> bool {
		self.persisted.series.is_empty()
	}

	pub fn len(&self) -> usize {
		self.persisted.series.len()
	}

	pub fn refreshed_at(&self) -> u64 {
		self.persisted.refreshed_at
	}

	pub fn series(&self, row: usize) -> &Series {
		&self.persisted.series[row]
	}

	pub fn row_of(&self, series_id: u64) -> Option<usize> {
		self.persisted
			.series
			.iter()
			.position(|series| series.id == series_id)
	}

	pub fn suggestions<'a>(
		&'a mut self,
		query: &str,
		server_matches: &[u64],
		telemetry: &Recorder,
	) -> Suggestions<'a> {
		let preferred = self.preferred_rows(query, server_matches);
		self.ensure_index(telemetry);
		let index = self.index.as_ref().expect("catalogue index exists");
		let _measurement =
			telemetry.measure_items(Operation::SearchRank, index.search.len());
		let mut seen = HashSet::new();
		let matches = preferred
			.into_iter()
			.chain(index.search.ranked(query))
			.filter(|index| seen.insert(*index))
			.collect();
		Suggestions {
			series: &self.persisted.series,
			rows: &index.rows,
			matches,
		}
	}

	pub fn ranked(
		series: &[Series],
		query: &str,
		limit: usize,
		telemetry: &Recorder,
	) -> Vec<Series> {
		let rows = series_rows(series);
		let measurement =
			telemetry.measure_items(Operation::SearchIndex, rows.len());
		let search = Search::new(&rows);
		drop(measurement);
		let _measurement =
			telemetry.measure_items(Operation::SearchRank, search.len());
		search
			.ranked(query)
			.into_iter()
			.take(limit)
			.map(|index| series[index].clone())
			.collect()
	}

	pub fn upsert(&mut self, incoming: Vec<Series>) {
		let mut positions = self
			.persisted
			.series
			.iter()
			.enumerate()
			.map(|(index, series)| (series.id, index))
			.collect::<HashMap<_, _>>();
		for series in incoming {
			if let Some(index) = positions.get(&series.id).copied() {
				self.persisted.series[index] = series;
			} else {
				positions.insert(series.id, self.persisted.series.len());
				self.persisted.series.push(series);
			}
		}
		self.index = None;
	}

	pub fn remember_alias(&mut self, query: &str, series_id: u64) {
		let query = normalize_query(query);
		if !query.is_empty() {
			self.persisted.aliases.insert(query, series_id);
		}
	}

	pub fn remove_series(&mut self, series_id: u64) {
		self.persisted
			.series
			.retain(|series| series.id != series_id);
		self.persisted.aliases.retain(|_, id| *id != series_id);
		self.index = None;
	}

	pub fn merge_refresh(
		&mut self,
		mut refreshed: Self,
		preserved_series: &HashSet<u64>,
	) {
		let mut preserved_series = preserved_series.clone();
		preserved_series.extend(self.persisted.aliases.values().copied());
		let mut ids = refreshed.ids();
		refreshed.persisted.series.extend(
			self.persisted
				.series
				.iter()
				.filter(|series| {
					preserved_series.contains(&series.id)
						&& ids.insert(series.id)
				})
				.cloned(),
		);
		refreshed
			.persisted
			.aliases
			.clone_from(&self.persisted.aliases);
		refreshed.persisted.aliases.retain(|_, id| ids.contains(id));
		self.persisted = refreshed.persisted;
		self.index = None;
	}

	pub fn catalogue_use(&self, selected_id: u64) -> CatalogueUse {
		if self.started_with.contains(&selected_id) {
			CatalogueUse::Hit
		} else {
			CatalogueUse::Miss
		}
	}

	fn preferred_rows(
		&self,
		query: &str,
		server_matches: &[u64],
	) -> Vec<usize> {
		let mut ids = self
			.persisted
			.aliases
			.get(&normalize_query(query))
			.copied()
			.into_iter()
			.collect::<Vec<_>>();
		ids.extend_from_slice(server_matches);
		let mut seen = HashSet::new();
		// ponytail: at most 11 priorities; add an ID index if that limit grows.
		ids.into_iter()
			.filter(|id| seen.insert(*id))
			.filter_map(|id| self.row_of(id))
			.collect()
	}

	fn ensure_index(&mut self, telemetry: &Recorder) {
		if self.index.is_some() {
			return;
		}
		let rows = series_rows(&self.persisted.series);
		let measurement =
			telemetry.measure_items(Operation::SearchIndex, rows.len());
		let search = Search::new(&rows);
		drop(measurement);
		self.index = Some(Index { rows, search });
	}

	fn ids(&self) -> HashSet<u64> {
		self.persisted
			.series
			.iter()
			.map(|series| series.id)
			.collect()
	}

	fn is_fresh_at(&self, now: u64) -> bool {
		now.saturating_sub(self.persisted.refreshed_at) < MAX_AGE.as_secs()
	}
}

impl Suggestions<'_> {
	pub fn is_empty(&self) -> bool {
		self.matches.is_empty()
	}

	pub fn rows(&self) -> &[Row] {
		self.rows
	}

	pub fn matches(&self) -> &[usize] {
		&self.matches
	}

	pub fn matching_rows(&self, limit: usize) -> Vec<Row> {
		self.matches
			.iter()
			.take(limit)
			.map(|index| self.rows[*index].clone())
			.collect()
	}

	pub fn matching_series(&self) -> impl Iterator<Item = &Series> {
		self.matches.iter().map(|index| &self.series[*index])
	}

	pub fn series(&self, position: usize) -> Option<&Series> {
		self.matches.get(position).map(|index| &self.series[*index])
	}
}

pub fn load() -> Catalogue {
	cache_path()
		.and_then(|path| fs::read(path).ok())
		.and_then(|contents| decode(&contents).ok())
		.unwrap_or_default()
}

pub fn store(catalogue: &Catalogue) {
	let Some(path) = cache_path() else {
		return;
	};
	let Some(directory) = path.parent() else {
		return;
	};
	let Ok(contents) = encode(catalogue) else {
		return;
	};
	if fs::create_dir_all(directory).is_ok() {
		let _ = fs::write(path, contents);
	}
}

pub fn prune() -> io::Result<()> {
	app_files::cache_directory().map_or(Ok(()), |path| prune_directory(&path))
}

fn prune_directory(path: &Path) -> io::Result<()> {
	match fs::remove_dir_all(path) {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
		Err(error) => Err(error),
	}
}

pub(crate) fn cache_path() -> Option<PathBuf> {
	app_files::cache_directory().map(|directory| directory.join("series.json"))
}

pub(crate) fn decode(contents: &[u8]) -> serde_json::Result<Catalogue> {
	let persisted: PersistedCatalogue = serde_json::from_slice(contents)?;
	let started_with =
		persisted.series.iter().map(|series| series.id).collect();
	Ok(Catalogue {
		persisted,
		started_with,
		index: None,
	})
}

fn encode(catalogue: &Catalogue) -> serde_json::Result<Vec<u8>> {
	serde_json::to_vec(&catalogue.persisted)
}

fn series_rows(series: &[Series]) -> Vec<Row> {
	series
		.iter()
		.map(|item| {
			[
				item.title.clone(),
				item.year
					.map_or_else(|| "?".into(), |year| year.to_string()),
				item.type_title.as_deref().unwrap_or("Unknown type").into(),
				format!(
					"{} episodes",
					item.number_of_episodes
						.map_or_else(|| "?".into(), |count| count.to_string())
				),
			]
		})
		.collect()
}

fn now() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs()
}

#[cfg(test)]
#[path = "series_cache_tests.rs"]
mod tests;
