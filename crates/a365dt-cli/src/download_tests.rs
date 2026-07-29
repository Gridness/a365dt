use std::path::PathBuf;

use pretty_assertions::assert_eq;

use super::{Job, sanitize};
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
