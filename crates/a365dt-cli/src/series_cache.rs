use std::{
	collections::BTreeMap,
	fs, io,
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{api::Series, app_files, search::normalize_query};

pub(crate) const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Cache {
	pub refreshed_at: u64,
	pub series: Vec<Series>,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub aliases: BTreeMap<String, u64>,
}

impl Cache {
	pub fn is_fresh(&self) -> bool {
		self.is_fresh_at(now())
	}

	pub fn mark_refreshed(&mut self) {
		self.refreshed_at = now();
	}

	pub fn alias(&self, query: &str) -> Option<u64> {
		self.aliases.get(&normalize_query(query)).copied()
	}

	pub fn remember_alias(&mut self, query: &str, series_id: u64) {
		let query = normalize_query(query);
		if !query.is_empty() {
			self.aliases.insert(query, series_id);
		}
	}

	pub fn remove_series(&mut self, series_id: u64) {
		self.series.retain(|series| series.id != series_id);
		self.aliases.retain(|_, id| *id != series_id);
	}

	fn is_fresh_at(&self, now: u64) -> bool {
		now.saturating_sub(self.refreshed_at) < MAX_AGE.as_secs()
	}
}

pub fn load() -> Cache {
	cache_path()
		.and_then(|path| fs::read(path).ok())
		.and_then(|contents| serde_json::from_slice(&contents).ok())
		.unwrap_or_default()
}

pub fn store(cache: &Cache) {
	let Some(path) = cache_path() else {
		return;
	};
	let Some(directory) = path.parent() else {
		return;
	};
	let Ok(contents) = serde_json::to_vec(cache) else {
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

fn now() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs()
}

#[cfg(test)]
#[path = "series_cache_tests.rs"]
mod tests;
