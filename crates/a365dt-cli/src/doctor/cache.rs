use std::{
	fs,
	path::PathBuf,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::series_cache::{self, Catalogue};

pub(crate) enum Inspection {
	Ready {
		path: PathBuf,
		cache: Catalogue,
		bytes: u64,
	},
	Missing(PathBuf),
	Broken {
		path: PathBuf,
		detail: String,
	},
}

pub(crate) fn inspect() -> Inspection {
	let Some(path) = series_cache::cache_path() else {
		return Inspection::Broken {
			path: PathBuf::from("<unresolved>"),
			detail: "Could not resolve the OS cache directory.".into(),
		};
	};
	let contents = match fs::read(&path) {
		Ok(contents) => contents,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
			return Inspection::Missing(path);
		}
		Err(error) => {
			return Inspection::Broken {
				path,
				detail: error.to_string(),
			};
		}
	};
	match series_cache::decode(&contents) {
		Ok(cache) => Inspection::Ready {
			path,
			cache,
			bytes: u64::try_from(contents.len()).unwrap_or(u64::MAX),
		},
		Err(error) => Inspection::Broken {
			path,
			detail: error.to_string(),
		},
	}
}

pub(super) fn age(cache: &Catalogue) -> Duration {
	Duration::from_secs(now().saturating_sub(cache.refreshed_at()))
}

fn now() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs()
}
