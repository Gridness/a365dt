use std::{fs, process, time::SystemTime};

use super::{
	FailureContext, is_structural, open, primary_result_code, rebuild,
};

#[test]
fn extended_result_codes_keep_their_primary_classification() {
	assert!(!is_structural(
		Some(primary_result_code(266)),
		FailureContext::Schema
	));
	assert!(is_structural(
		Some(primary_result_code(267)),
		FailureContext::Opening
	));
}

#[tokio::test]
async fn rebuilds_after_a_shared_lock_was_inherited() {
	let directory = std::env::temp_dir().join(format!(
		"a365dt-cache-inherited-lock-{}-{}",
		process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	let database = open(&directory)
		.await
		.unwrap_or_else(|failure| panic!("{}", failure.error.render(true)));
	let inherited_lock = database._lock.0.try_clone().unwrap();
	database.pool.close().await;
	drop(database);

	rebuild(&directory).await.unwrap();

	drop(inherited_lock);
	fs::remove_dir_all(directory).unwrap();
}
