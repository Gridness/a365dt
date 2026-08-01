use std::path::PathBuf;

use pretty_assertions::assert_eq;

use super::{Files, Job, Mux, sanitize};
use crate::{
	api::{Episode, Translation},
	preferences::MuxFormat,
	select::PlannedRelease,
};

fn job(episode_int: &str, episode_full: &str, mux: Mux) -> Job {
	Job {
		release: PlannedRelease {
			episode: Episode {
				id: 1,
				episode_int: episode_int.into(),
				episode_full: episode_full.into(),
			},
			translation: Translation {
				id: 1,
				episode_id: 1,
				kind: "sub".into(),
				language: "ru".into(),
				authors_summary: "Team".into(),
			},
			height: 1080,
			media_url: String::new(),
			subtitle_url: None,
		},
		directory: PathBuf::from("Anime"),
		mux,
	}
}

#[test]
fn creates_cross_platform_safe_names() {
	assert_eq!(sanitize("A/B: Team", 64), "A_B_ Team");
	assert_eq!(sanitize("CON", 64), "_CON");
}

#[test]
fn distinguishes_full_episode_labels_in_file_names() {
	assert_eq!(
		[
			job("5", "5 серия", Mux::Disabled).stem(),
			job("5", "TV SP 5 серия", Mux::Disabled).stem(),
			job("6.5", "6.5 серия", Mux::Disabled).stem(),
		],
		[
			"E05 [sub-ru] [Team] [1080p]",
			"TV SP 5 [sub-ru] [Team] [1080p]",
			"E06.5 [sub-ru] [Team] [1080p]",
		]
	);
}

#[test]
fn plans_format_specific_mux_files() {
	let directory = PathBuf::from("Anime");
	let mut separate = job("1", "1 серия", Mux::Enabled(MuxFormat::Mp4));
	separate.release.subtitle_url = Some(String::new());
	let mp4 = separate.files();
	separate.mux = Mux::Enabled(MuxFormat::Mkv);
	let mkv = separate.files();
	let self_contained =
		job("1", "1 серия", Mux::Enabled(MuxFormat::Mp4)).files();

	assert_eq!(
		[mp4, mkv, self_contained],
		[
			Files {
				video: directory.join("E01 [sub-ru] [Team] [1080p].video.mp4"),
				subtitle: directory.join("E01 [sub-ru] [Team] [1080p].ass"),
				output: directory.join("E01 [sub-ru] [Team] [1080p].mp4"),
			},
			Files {
				video: directory.join("E01 [sub-ru] [Team] [1080p].mp4"),
				subtitle: directory.join("E01 [sub-ru] [Team] [1080p].ass"),
				output: directory.join("E01 [sub-ru] [Team] [1080p].mkv"),
			},
			Files {
				video: directory.join("E01 [sub-ru] [Team] [1080p].mp4"),
				subtitle: directory.join("E01 [sub-ru] [Team] [1080p].ass"),
				output: directory.join("E01 [sub-ru] [Team] [1080p].mp4"),
			},
		]
	);
}
