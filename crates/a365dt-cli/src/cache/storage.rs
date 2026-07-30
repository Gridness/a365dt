use std::{
	collections::BTreeMap,
	fs, io,
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use super::{catalogue::Catalogue, writer::LoadedCatalogue};
use crate::{api::Series, app_files, error::Error};

const SERIES_FILE: &str = "series.json";
const RELEASE_FILE: &str = "latest-release.json";
const RELEASE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug)]
pub(crate) struct Store {
	directory: Result<PathBuf, Error>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
	Missing(PathBuf),
	Broken {
		path: PathBuf,
		detail: String,
	},
}

#[derive(Default, Deserialize, Serialize)]
struct ReleaseCache {
	release: Option<Release>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	completed_at_ms: Option<u64>,
}

#[derive(Deserialize, Serialize)]
struct PersistedCatalogue {
	refreshed_at: u64,
	series: Vec<Series>,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	aliases: BTreeMap<String, u64>,
}

pub(crate) enum RebuildPermission {
	Ask,
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
			Some(directory) => Self::at(directory),
			None => Self {
				directory: Err(Error::new(
					"Could not resolve the user cache directory; check OS configuration.",
				)),
			},
		}
	}

	pub(super) fn at(directory: PathBuf) -> Self {
		Self {
			directory: Ok(directory),
		}
	}

	pub(crate) async fn load_catalogue(
		&self,
	) -> Result<LoadedCatalogue, Error> {
		let path = match self.path(SERIES_FILE) {
			Ok(path) => path,
			Err(_) => return Ok(LoadedCatalogue::unavailable()),
		};
		run_blocking(move || read_catalogue(&path))
			.await
			.map(LoadedCatalogue::new)
	}

	pub(super) async fn save_catalogue(
		&self,
		catalogue: &Catalogue,
	) -> Result<(), Error> {
		let path = self.path(SERIES_FILE)?;
		let catalogue = catalogue.clone();
		run_blocking(move || write_catalogue(&path, &catalogue)).await
	}

	pub(crate) async fn load_release(&self) -> Result<ReleaseState, Error> {
		let path = match self.path(RELEASE_FILE) {
			Ok(path) => path,
			Err(_) => return Ok(ReleaseState::Missing),
		};
		run_blocking(move || read_release(&path)).await
	}

	pub(crate) async fn save_release(
		&self,
		release: CompletedRelease,
	) -> Result<(), Error> {
		let path = match self.path(RELEASE_FILE) {
			Ok(path) => path,
			Err(_) => return Ok(()),
		};
		run_blocking(move || write_release(&path, release)).await
	}

	pub(crate) async fn inspect(&self) -> Inspection {
		let path = match self.path(SERIES_FILE) {
			Ok(path) => path,
			Err(error) => {
				return Inspection::Broken {
					path: PathBuf::from("<unresolved>"),
					detail: error.to_string(),
				};
			}
		};
		let broken_path = path.clone();
		match run_blocking(move || Ok(inspect_path(path))).await {
			Ok(inspection) => inspection,
			Err(error) => Inspection::Broken {
				path: broken_path,
				detail: error.to_string(),
			},
		}
	}

	pub(crate) async fn close(self) {}

	pub(crate) fn initialization_warning(&self) -> Option<Error> {
		self.directory.as_ref().err().cloned()
	}

	fn path(&self, file: &str) -> Result<PathBuf, Error> {
		Ok(self.directory.clone()?.join(file))
	}
}

pub(crate) async fn prune(_permission: RebuildPermission) -> Result<(), Error> {
	let Some(path) = app_files::cache_directory() else {
		return Ok(());
	};
	run_blocking(move || {
		prune_directory(&path).map_err(|error| {
			Error::with_debug("Could not clear the local cache.", error)
		})
	})
	.await
}

fn read_catalogue(path: &Path) -> Result<Catalogue, Error> {
	match fs::read(path) {
		Ok(contents) => {
			decode(&contents).map_err(|error| read_error(path, error))
		}
		Err(error) if error.kind() == io::ErrorKind::NotFound => {
			Ok(Catalogue::default())
		}
		Err(error) => Err(read_error(path, error)),
	}
}

fn write_catalogue(path: &Path, catalogue: &Catalogue) -> Result<(), Error> {
	let contents =
		encode(catalogue).map_err(|error| write_error(path, error))?;
	write(path, &contents)
}

fn read_release(path: &Path) -> Result<ReleaseState, Error> {
	let contents = match fs::read(path) {
		Ok(contents) => contents,
		Err(error) if error.kind() == io::ErrorKind::NotFound => {
			return Ok(ReleaseState::Missing);
		}
		Err(error) => return Err(read_error(path, error)),
	};
	let cache: ReleaseCache = serde_json::from_slice(&contents)
		.map_err(|error| read_error(path, error))?;
	let Some(release) = cache.release else {
		return Ok(ReleaseState::Missing);
	};
	let fresh = fs::metadata(path)
		.and_then(|metadata| metadata.modified())
		.ok()
		.and_then(|modified| modified.elapsed().ok())
		.is_some_and(|age| age < RELEASE_TTL);
	Ok(if fresh {
		ReleaseState::Fresh(release)
	} else {
		ReleaseState::Stale(release)
	})
}

fn write_release(
	path: &Path,
	completed_release: CompletedRelease,
) -> Result<(), Error> {
	if let Ok(contents) = fs::read(path)
		&& let Ok(stored) = serde_json::from_slice::<ReleaseCache>(&contents)
		&& stored
			.completed_at_ms
			.is_some_and(|stored| stored > completed_release.completed_at_ms)
	{
		return Ok(());
	}
	let contents = serde_json::to_vec(&ReleaseCache {
		release: Some(completed_release.release),
		completed_at_ms: Some(completed_release.completed_at_ms),
	})
	.map_err(|error| write_error(path, error))?;
	write(path, &contents)
}

fn write(path: &Path, contents: &[u8]) -> Result<(), Error> {
	let Some(directory) = path.parent() else {
		return Err(write_error(path, "cache path has no parent directory"));
	};
	fs::create_dir_all(directory).map_err(|error| write_error(path, error))?;
	fs::write(path, contents).map_err(|error| write_error(path, error))
}

fn inspect_path(path: PathBuf) -> Inspection {
	let contents = match fs::read(&path) {
		Ok(contents) => contents,
		Err(error) if error.kind() == io::ErrorKind::NotFound => {
			return Inspection::Missing(path);
		}
		Err(error) => {
			return Inspection::Broken {
				path,
				detail: error.to_string(),
			};
		}
	};
	match decode(&contents) {
		Ok(catalogue) => Inspection::Ready {
			path,
			refreshed_at: catalogue.refreshed_at(),
			series: catalogue.len(),
			bytes: u64::try_from(contents.len()).unwrap_or(u64::MAX),
			fresh: catalogue.is_fresh(),
			age: Duration::from_secs(
				now().saturating_sub(catalogue.refreshed_at()),
			),
		},
		Err(error) => Inspection::Broken {
			path,
			detail: error.to_string(),
		},
	}
}

fn prune_directory(path: &Path) -> io::Result<()> {
	match fs::remove_dir_all(path) {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
		Err(error) => Err(error),
	}
}

fn decode(contents: &[u8]) -> serde_json::Result<Catalogue> {
	let persisted = serde_json::from_slice::<PersistedCatalogue>(contents)?;
	Ok(Catalogue::from_parts(
		persisted.refreshed_at,
		persisted.series,
		persisted.aliases,
	))
}

fn encode(catalogue: &Catalogue) -> serde_json::Result<Vec<u8>> {
	let mut persisted = PersistedCatalogue {
		refreshed_at: catalogue.refreshed_at,
		series: catalogue.series.clone(),
		aliases: catalogue.aliases.clone(),
	};
	for series in &mut persisted.series {
		series.poster_url_small = None;
		series.episodes.clear();
	}
	serde_json::to_vec(&persisted)
}

fn read_error(path: &Path, error: impl std::fmt::Display) -> Error {
	Error::with_debug(
		"Could not read the local cache; run `a365dt cache prune` to reset it.",
		format!("{}: {error}", path.display()),
	)
}

fn write_error(path: &Path, error: impl std::fmt::Display) -> Error {
	Error::with_debug(
		"Could not update the local cache; run `a365dt cache prune` to reset it.",
		format!("{}: {error}", path.display()),
	)
}

async fn run_blocking<T>(
	task: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error>
where
	T: Send + 'static,
{
	tokio::task::spawn_blocking(task).await.map_err(|error| {
		Error::with_debug("A local cache task stopped unexpectedly.", error)
	})?
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
