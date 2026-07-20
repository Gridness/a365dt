use std::path::PathBuf;

use pretty_assertions::assert_eq;

use super::{
	Job, finalize, first_nonzero, part_path, protect_mismatch, retryable,
	sanitize, valid_content_range, verified_size,
};
use crate::{
	api::{Episode, Translation},
	select::PlannedRelease,
};

#[test]
fn creates_cross_platform_safe_names() {
	assert_eq!(sanitize("A/B: Team", 64), "A_B_ Team");
	assert_eq!(sanitize("CON", 64), "_CON");
}

#[test]
fn distinguishes_full_episode_labels_in_file_names() {
	let job = |id, episode_int: &str, episode_full: &str| Job {
		release: PlannedRelease {
			episode: Episode {
				id,
				episode_int: episode_int.into(),
				episode_full: episode_full.into(),
			},
			translation: Translation {
				id,
				episode_id: id,
				kind: "sub".into(),
				language: "ru".into(),
				authors_summary: "Team".into(),
			},
			height: 1080,
			media_url: String::new(),
			subtitle_url: None,
		},
		directory: PathBuf::new(),
		mux: false,
	};

	assert_eq!(
		[
			job(1, "5", "5 серия").stem(),
			job(2, "5", "TV SP 5 серия").stem(),
			job(3, "6.5", "6.5 серия").stem(),
		],
		[
			"E05 [sub-ru] [Team] [1080p]",
			"TV SP 5 [sub-ru] [Team] [1080p]",
			"E06.5 [sub-ru] [Team] [1080p]",
		]
	);
}

#[test]
fn validates_resumed_content_range() {
	assert_eq!(valid_content_range("bytes 50-99/100", 50, 100), true);
	assert_eq!(valid_content_range("bytes 0-99/100", 50, 100), false);
	assert_eq!(valid_content_range("bytes 50-99/101", 50, 100), false);
}

#[test]
fn accepts_transfer_without_declared_size() {
	assert_eq!(verified_size(None, 42).unwrap(), 42);
}

#[test]
fn preserves_known_size_when_retry_reports_zero() {
	assert_eq!(
		[
			first_nonzero(Some(200), Some(100)),
			first_nonzero(Some(0), Some(100)),
			first_nonzero(None, Some(100)),
			first_nonzero(Some(0), None),
			first_nonzero(None, None),
		],
		[Some(200), Some(100), Some(100), None, None]
	);
}

#[test]
fn rejects_empty_transfer_with_relevant_error() {
	assert_eq!(
		verified_size(Some(0), 0).unwrap_err(),
		retryable("The media server returned an empty file.", None)
	);
}

#[tokio::test]
async fn replaces_mismatched_file_and_cleans_backup() {
	let unique = format!(
		"a365dt-test-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	);
	let directory = std::env::temp_dir().join(unique);
	tokio::fs::create_dir(&directory).await.unwrap();
	let final_path = directory.join("episode.mp4");
	tokio::fs::write(&final_path, b"bad").await.unwrap();

	protect_mismatch(&final_path, 4).await.unwrap();
	let part = part_path(&final_path);
	tokio::fs::write(&part, b"good").await.unwrap();
	finalize(&part, &final_path).await.unwrap();

	assert_eq!(tokio::fs::read(&final_path).await.unwrap(), b"good");
	assert_eq!(directory.join("episode.mp4.corrupt").exists(), false);
	tokio::fs::remove_dir_all(directory).await.unwrap();
}
