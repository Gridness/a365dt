use std::path::Path;

use tokio::{fs, fs::File, process::Command};

use crate::error::Error;

pub async fn run(
	video: &Path,
	subtitle: &Path,
	output: &Path,
) -> Result<(), Error> {
	let part = output.with_extension("part.mkv");
	let _ = fs::remove_file(&part).await;
	let result = Command::new("ffmpeg")
		.arg("-nostdin")
		.arg("-hide_banner")
		.arg("-loglevel")
		.arg("error")
		.arg("-i")
		.arg(video)
		.arg("-i")
		.arg(subtitle)
		.arg("-map")
		.arg("0")
		.arg("-map")
		.arg("1:0")
		.arg("-disposition:s:0")
		.arg("default")
		.arg("-c")
		.arg("copy")
		.arg("-f")
		.arg("matroska")
		.arg(&part)
		.output()
		.await
		.map_err(|error| Error::with_debug("Could not start ffmpeg.", error))?;
	if !result.status.success() {
		let _ = fs::remove_file(&part).await;
		return Err(Error::with_debug(
			"ffmpeg could not combine the video and subtitles.",
			format!(
				"{}: {}",
				result.status,
				String::from_utf8_lossy(&result.stderr).trim()
			),
		));
	}
	let file = File::open(&part).await.map_err(|error| {
		Error::with_debug("Could not verify the muxed MKV file.", error)
	})?;
	if file
		.metadata()
		.await
		.map_err(|error| {
			Error::with_debug("Could not verify the muxed MKV file.", error)
		})?
		.len() == 0
	{
		return Err(Error::new("ffmpeg created an empty MKV file."));
	}
	file.sync_all().await.map_err(|error| {
		Error::with_debug("Could not finish writing the muxed MKV file.", error)
	})?;
	fs::rename(&part, output).await.map_err(|error| {
		Error::with_debug("Could not save the muxed MKV file.", error)
	})?;
	fs::remove_file(video).await.map_err(|error| {
		Error::with_debug(
			"The MKV was saved, but the source video could not be removed.",
			error,
		)
	})?;
	fs::remove_file(subtitle).await.map_err(|error| {
		Error::with_debug(
			"The MKV was saved, but the source subtitles could not be removed.",
			error,
		)
	})?;
	Ok(())
}
