use std::{
	fs,
	path::PathBuf,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
	l10n::tr,
	series_cache::{self, Cache},
};

pub(super) enum Inspection {
	Ready {
		path: PathBuf,
		cache: Cache,
		bytes: u64,
	},
	Missing(PathBuf),
	Broken {
		path: PathBuf,
		detail: String,
	},
}

pub(super) fn inspect() -> Inspection {
	let Some(path) = series_cache::cache_path() else {
		return Inspection::Broken {
			path: PathBuf::from("<unresolved>"),
			detail: tr("doctor-cache-directory-error"),
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
	match serde_json::from_slice(&contents) {
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

pub(super) fn age(cache: &Cache) -> Duration {
	Duration::from_secs(now().saturating_sub(cache.refreshed_at))
}

fn now() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs()
}
