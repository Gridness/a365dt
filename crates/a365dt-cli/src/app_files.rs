use std::{fs, io, path::PathBuf};

use directories::ProjectDirs;

pub(crate) const APPLICATION_ID: &str = if cfg!(debug_assertions) {
	"a365dt-dev"
} else {
	"a365dt"
};

pub fn directories() -> Option<ProjectDirs> {
	ProjectDirs::from("", "", APPLICATION_ID)
}

pub fn cache_directory() -> Option<PathBuf> {
	directories().map(|directories| directories.cache_dir().to_owned())
}

pub fn purge() -> io::Result<()> {
	let Some(directories) = directories() else {
		return Ok(());
	};
	purge_directories(&application_roots(&directories))
}

fn application_roots(directories: &ProjectDirs) -> Vec<PathBuf> {
	let mut paths = vec![
		directories.cache_dir(),
		directories.config_dir(),
		directories.config_local_dir(),
		directories.data_dir(),
		directories.data_local_dir(),
		directories.preference_dir(),
	];
	paths.extend(directories.runtime_dir());
	paths.extend(directories.state_dir());

	let project_path = directories.project_path();
	let mut roots = paths
		.into_iter()
		.map(|path| {
			path.ancestors()
				.find(|ancestor| ancestor.ends_with(project_path))
				.unwrap_or(path)
				.to_owned()
		})
		.collect::<Vec<_>>();
	roots.sort_unstable();
	roots.dedup();
	roots
}

fn purge_directories(directories: &[PathBuf]) -> io::Result<()> {
	let failures = directories
		.iter()
		.filter_map(|directory| match fs::remove_dir_all(directory) {
			Ok(()) => None,
			Err(error) if error.kind() == io::ErrorKind::NotFound => None,
			Err(error) => Some(format!("{}: {error}", directory.display())),
		})
		.collect::<Vec<_>>();
	if failures.is_empty() {
		Ok(())
	} else {
		Err(io::Error::other(failures.join("\n")))
	}
}

#[cfg(test)]
#[path = "app_files_tests.rs"]
mod tests;
