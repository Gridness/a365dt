use std::path::Path;

use tokio::{fs, fs::File, process::Command};

use crate::error::Error;
use crate::preferences::MuxFormat;

pub(super) fn part_path(output: &Path) -> std::path::PathBuf {
	output.with_extension(format!("part.{}", extension(output)))
}

pub(super) async fn reconcile(
	format: MuxFormat,
	video: &Path,
	subtitle: &Path,
	output: &Path,
) -> Result<bool, Error> {
	if format == MuxFormat::Mp4
		&& output.exists()
		&& !video.exists()
		&& has_subtitle(output).await == Some(false)
	{
		fs::rename(output, video).await.map_err(|error| {
			Error::with_debug(
				"Could not prepare the existing MP4 for muxing.",
				error,
			)
		})?;
	}
	if !output.exists() {
		return Ok(false);
	}
	let _ = fs::remove_file(video).await;
	let _ = fs::remove_file(subtitle).await;
	let _ = fs::remove_file(part_path(output)).await;
	Ok(true)
}

pub(super) async fn finish_without_subtitle(
	format: Option<MuxFormat>,
	video: &Path,
	output: &Path,
) -> Result<std::path::PathBuf, Error> {
	if format != Some(MuxFormat::Mp4) {
		return Ok(video.to_owned());
	}
	fs::rename(video, output).await.map_err(|error| {
		Error::with_debug("Could not save the downloaded MP4 file.", error)
	})?;
	Ok(output.to_owned())
}

pub(super) fn output_path(
	format: MuxFormat,
	base: &Path,
) -> std::path::PathBuf {
	base.with_extension(match format {
		MuxFormat::Mp4 => "mp4",
		MuxFormat::Mkv => "mkv",
	})
}

pub(super) async fn prepare_source(
	format: MuxFormat,
	video: &Path,
	output: &Path,
) -> Result<std::path::PathBuf, Error> {
	if format != MuxFormat::Mp4 || video != output {
		return Ok(video.to_owned());
	}
	let staged = output.with_extension("video.mp4");
	fs::rename(video, &staged).await.map_err(|error| {
		Error::with_debug("Could not prepare the MP4 for muxing.", error)
	})?;
	Ok(staged)
}

async fn has_subtitle(path: &Path) -> Option<bool> {
	Command::new("ffmpeg")
		.args(["-nostdin", "-hide_banner", "-loglevel", "error"])
		.arg("-i")
		.arg(path)
		.args(["-map", "0:s:0", "-c", "copy", "-t", "0", "-f", "null"])
		.arg("-")
		.output()
		.await
		.ok()
		.map(|output| output.status.success())
}

fn extension(path: &Path) -> &str {
	path.extension()
		.and_then(|extension| extension.to_str())
		.unwrap_or("mkv")
}

pub async fn run(
	format: MuxFormat,
	video: &Path,
	subtitle: &Path,
	output: &Path,
) -> Result<(), Error> {
	let part = part_path(output);
	let _ = fs::remove_file(&part).await;
	let result = run_inner(format, video, subtitle, output, &part).await;
	if result.is_err() {
		let _ = fs::remove_file(&part).await;
	}
	result
}

async fn run_inner(
	format: MuxFormat,
	video: &Path,
	subtitle: &Path,
	output: &Path,
	part: &Path,
) -> Result<(), Error> {
	let mut command = Command::new("ffmpeg");
	command
		.args(["-nostdin", "-hide_banner", "-loglevel", "error"])
		.arg("-i")
		.arg(video)
		.arg("-i")
		.arg(subtitle);
	let (source_maps, subtitle_codec, container) = match format {
		MuxFormat::Mp4 => (
			&["-map", "0:v", "-map", "0:a?"][..],
			Some("mov_text"),
			"mp4",
		),
		MuxFormat::Mkv => (&["-map", "0"][..], None, "matroska"),
	};
	command.args(source_maps);
	command.args(["-map", "1:0", "-disposition:s:0", "default", "-c", "copy"]);
	if let Some(codec) = subtitle_codec {
		command.args(["-c:s", codec]);
	}
	command.args(["-f", container]);
	let result =
		command.arg(part).output().await.map_err(|error| {
			Error::with_debug("Could not start ffmpeg.", error)
		})?;
	if !result.status.success() {
		return Err(Error::with_debug(
			"ffmpeg could not combine the video and subtitles.",
			format!(
				"{}: {}",
				result.status,
				String::from_utf8_lossy(&result.stderr).trim()
			),
		));
	}
	let file = File::open(part).await.map_err(|error| {
		Error::with_debug("Could not verify the muxed file.", error)
	})?;
	if file
		.metadata()
		.await
		.map_err(|error| {
			Error::with_debug("Could not verify the muxed file.", error)
		})?
		.len() == 0
	{
		return Err(Error::new("ffmpeg created an empty muxed file."));
	}
	file.sync_all().await.map_err(|error| {
		Error::with_debug("Could not finish writing the muxed file.", error)
	})?;
	fs::rename(part, output).await.map_err(|error| {
		Error::with_debug("Could not save the muxed file.", error)
	})?;
	fs::remove_file(video).await.map_err(|error| {
		Error::with_debug(
			"The muxed file was saved, but the source video could not be removed.",
			error,
		)
	})?;
	fs::remove_file(subtitle).await.map_err(|error| {
		Error::with_debug(
			"The muxed file was saved, but the source subtitles could not be removed.",
			error,
		)
	})?;
	Ok(())
}
