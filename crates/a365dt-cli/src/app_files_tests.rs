use std::{fs, process, time::SystemTime};

use pretty_assertions::assert_eq;

use super::purge_directories;

#[test]
fn purges_every_owned_directory_idempotently() {
	let base = std::env::temp_dir().join(format!(
		"a365dt-purge-{}-{}",
		process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	let directories = [base.join("cache"), base.join("data")];
	for directory in &directories {
		fs::create_dir_all(directory.join("nested")).unwrap();
		fs::write(directory.join("nested/file"), b"owned").unwrap();
	}

	purge_directories(&directories).unwrap();
	purge_directories(&directories).unwrap();

	assert_eq!(directories.map(|directory| directory.exists()), [false; 2]);
	fs::remove_dir(base).unwrap();
}
