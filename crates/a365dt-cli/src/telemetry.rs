use std::path::PathBuf;

use crate::{app_files, error::Error, ui};

mod display;
mod recording;
mod snapshot;
mod storage;
mod writer;

pub(crate) use display::format_timestamp;
pub(crate) use recording::{
	CatalogueUse, Command, CommandOutcome, InvocationId, Operation, Recorder,
};
use recording::{Observation, ObservationKind, now_ms};
pub(crate) use snapshot::{PerformanceMetric, Snapshot};
use storage::Store;
pub(crate) use writer::Writer;

#[derive(Clone, Debug)]
pub(super) struct Paths {
	data: PathBuf,
	disabled: PathBuf,
	lock: PathBuf,
}

impl Paths {
	fn discover() -> Result<Self, Error> {
		let directories = app_files::directories().ok_or_else(|| {
			Error::new("Could not resolve the local telemetry directory.")
		})?;
		let data_directory = directories.data_local_dir();
		Ok(Self {
			data: data_directory.join("telemetry.sqlite"),
			lock: data_directory.join("telemetry.lock"),
			disabled: directories.config_dir().join("telemetry-disabled"),
		})
	}
}

pub async fn show(invocation_id: InvocationId) -> Result<(), Error> {
	let store = Store::open(Paths::discover()?).await?;
	warn_cleanup(&store);
	let result = async {
		let snapshot = snapshot::capture(&store).await?;
		display::print(&snapshot);
		if snapshot.enabled {
			let mut watermark =
				store.collection_state().await?.last_cleared_at_ms;
			store
				.commit(
					&mut watermark,
					vec![Observation::command(
						invocation_id,
						Command::TelemetryShow,
						CommandOutcome::Success,
					)],
				)
				.await?;
		}
		Ok(())
	}
	.await;
	store.close().await;
	result
}

pub async fn clear() -> Result<(), Error> {
	let store = Store::open(Paths::discover()?).await?;
	warn_cleanup(&store);
	let result = store.clear().await;
	store.close().await;
	result?;
	ui::success("Local telemetry cleared");
	Ok(())
}

pub async fn disable(invocation_id: InvocationId) -> Result<(), Error> {
	let store = Store::open(Paths::discover()?).await?;
	warn_cleanup(&store);
	let result = store.disable(invocation_id).await;
	store.close().await;
	result?;
	ui::success("Local telemetry disabled");
	Ok(())
}

pub async fn enable(invocation_id: InvocationId) -> Result<(), Error> {
	let store = Store::open(Paths::discover()?).await?;
	warn_cleanup(&store);
	let result = store.enable(invocation_id).await;
	store.close().await;
	result?;
	ui::success("Local telemetry enabled");
	Ok(())
}

fn warn_cleanup(store: &Store) {
	if let Some(error) = store.warning() {
		ui::warning(error);
	}
}

#[cfg(test)]
#[path = "telemetry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "telemetry_process_tests.rs"]
mod process_tests;
