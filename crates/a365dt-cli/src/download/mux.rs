use std::path::Path;

use tokio::{fs, fs::File, process::Command};

use crate::{error::Error, l10n::tr};

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
		.map_err(|error| Error::with_debug(tr("mux-start-error"), error))?;
	if !result.status.success() {
		let _ = fs::remove_file(&part).await;
		return Err(Error::with_debug(
			tr("mux-combine-error"),
			format!(
				"{}: {}",
				result.status,
				String::from_utf8_lossy(&result.stderr).trim()
			),
		));
	}
	let file = File::open(&part)
		.await
		.map_err(|error| Error::with_debug(tr("mux-verify-error"), error))?;
	if file
		.metadata()
		.await
		.map_err(|error| Error::with_debug(tr("mux-verify-error"), error))?
		.len() == 0
	{
		return Err(Error::new(tr("mux-empty-error")));
	}
	file.sync_all()
		.await
		.map_err(|error| Error::with_debug(tr("mux-write-error"), error))?;
	fs::rename(&part, output)
		.await
		.map_err(|error| Error::with_debug(tr("mux-save-error"), error))?;
	fs::remove_file(video).await.map_err(|error| {
		Error::with_debug(tr("mux-remove-video-error"), error)
	})?;
	fs::remove_file(subtitle).await.map_err(|error| {
		Error::with_debug(tr("mux-remove-subtitle-error"), error)
	})?;
	Ok(())
}
