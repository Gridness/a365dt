use std::{
	fs,
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use clap::Subcommand;

use crate::{error::Error, ui};

use super::{
	FilePreferences, Inspection, Overrides, Preferences, Source, Store,
};

#[derive(Clone, Copy)]
enum ConfirmDefault {
	No,
	Yes,
}

#[derive(Subcommand)]
pub(crate) enum ConfigCommand {
	/// Restore built-in Download preferences.
	Reset {
		/// Reset without asking for confirmation.
		#[arg(short, long, conflicts_with = "query")]
		yes: bool,

		#[arg(
			value_name = "QUERY",
			num_args = 0..,
			hide = true,
			conflicts_with = "yes"
		)]
		query: Vec<String>,
	},

	/// Show effective Download preferences.
	Show {
		#[arg(value_name = "QUERY", num_args = 0.., hide = true)]
		query: Vec<String>,
	},

	#[command(external_subcommand)]
	Query(Vec<String>),
}

#[derive(Clone, Copy)]
pub(crate) enum ResetPermission {
	Ask,
	Preauthorized,
}

impl Source {
	fn label(self) -> &'static str {
		match self {
			Self::BuiltIn => "built-in",
			Self::Config => "config",
			Self::CommandLine => "command line",
		}
	}
}

impl Store {
	pub(crate) fn configure(&self) -> Result<(), Error> {
		if !ui::selector::interactive_terminal() {
			return Err(Error::new(format!(
				"Interactive configuration requires a terminal. Edit {} directly.",
				self.file().display()
			)));
		}
		let current = match self.inspect() {
			Inspection::Missing { snapshot, .. }
			| Inspection::Ready { snapshot, .. } => snapshot.preferences,
			Inspection::Invalid { error, .. } => {
				ui::warning(error.message());
				if !confirm(
					"Replace the invalid configuration?",
					ConfirmDefault::No,
				)? {
					ui::note("Configuration left unchanged.");
					return Ok(());
				}
				self.resolve(FilePreferences::default(), Overrides::default())?
					.preferences
			}
			Inspection::Unreadable { error, .. } => return Err(error),
		};

		ui::heading("Download preferences");
		let output = self.prompt_output(&current.output)?;
		let jobs = prompt_jobs(current.jobs)?;
		let mux = confirm(
			"Mux separate subtitles without confirmation?",
			if current.mux {
				ConfirmDefault::Yes
			} else {
				ConfirmDefault::No
			},
		)?;
		self.save(&Preferences { output, jobs, mux })?;
		ui::success(format!("Saved {}", self.file().display()));
		Ok(())
	}

	pub(crate) fn show(&self) -> Result<(), Error> {
		let (path, snapshot, state) = match self.inspect() {
			Inspection::Missing { path, snapshot } => {
				(path, snapshot, "missing")
			}
			Inspection::Ready { path, snapshot } => (path, snapshot, "loaded"),
			Inspection::Invalid { error, .. }
			| Inspection::Unreadable { error, .. } => return Err(error),
		};
		ui::heading("Download preferences");
		ui::grid(&[
			["Config".into(), path.display().to_string(), state.into()],
			[
				"Output".into(),
				snapshot.preferences.output.display().to_string(),
				snapshot.sources.output.label().into(),
			],
			[
				"Jobs".into(),
				snapshot.preferences.jobs.to_string(),
				snapshot.sources.jobs.label().into(),
			],
			[
				"Mux".into(),
				if snapshot.preferences.mux {
					"Without confirmation"
				} else {
					"Ask"
				}
				.into(),
				snapshot.sources.mux.label().into(),
			],
		]);
		Ok(())
	}

	pub(crate) fn reset_command(
		&self,
		permission: ResetPermission,
	) -> Result<(), Error> {
		let path = self.file();
		let exists = path.try_exists().map_err(|error| {
			Error::with_debug(
				format!("Could not inspect {}.", path.display()),
				error,
			)
		})?;
		if !exists {
			ui::note("Built-in Download preferences are already active.");
			return Ok(());
		}
		let confirmed = match permission {
			ResetPermission::Ask => confirm(
				"Remove saved Download preferences?",
				ConfirmDefault::No,
			)?,
			ResetPermission::Preauthorized => true,
		};
		if !confirmed {
			ui::note("Configuration reset cancelled.");
			return Ok(());
		}
		self.reset()?;
		ui::success("Built-in Download preferences restored");
		Ok(())
	}

	fn prompt_output(&self, current: &Path) -> Result<PathBuf, Error> {
		loop {
			let input = ui::prompt(&format!(
				"Output directory [{}] (Enter to keep):",
				current.display()
			))?;
			let output = if input.is_empty() {
				current.to_owned()
			} else {
				match self.resolve_configured_output(input) {
					Ok(output) => output,
					Err(error) => {
						ui::warning(error.message());
						continue;
					}
				}
			};
			if self.ensure_outside_application_home(&output).is_err() {
				ui::warning(
					"The output directory cannot be inside the Application home.",
				);
				continue;
			}
			match fs::metadata(&output) {
				Ok(metadata) if metadata.is_dir() => {
					return self.prepare_output(&output);
				}
				Ok(_) => ui::warning("The output path is not a directory."),
				Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
					if !confirm(
						"Create the output directory?",
						ConfirmDefault::Yes,
					)? {
						continue;
					}
					match self.prepare_output(&output) {
						Ok(output) => return Ok(output),
						Err(error) => ui::warning(error.render(true)),
					}
				}
				Err(error) => ui::warning(format!(
					"Could not inspect output directory {}: {error}",
					output.display()
				)),
			}
		}
	}
}

fn confirm(label: &str, default: ConfirmDefault) -> Result<bool, Error> {
	ui::confirm(label, matches!(default, ConfirmDefault::Yes))
}

fn prompt_jobs(current: NonZeroUsize) -> Result<NonZeroUsize, Error> {
	loop {
		let input = ui::prompt(&format!(
			"Concurrent downloads [{current}] (Enter to keep):"
		))?;
		if input.is_empty() {
			return Ok(current);
		}
		match input.parse() {
			Ok(jobs) => return Ok(jobs),
			Err(_) => ui::warning("Enter a positive whole number."),
		}
	}
}
