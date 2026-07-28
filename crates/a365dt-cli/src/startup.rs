use std::{
	collections::hash_map::RandomState,
	fs,
	hash::BuildHasher,
	io::{self, IsTerminal},
	path::{Path, PathBuf},
	process,
	time::{Duration, SystemTime},
};

use console::{Style, style};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::app_files;

const TIPS: &str = include_str!("../tips.txt");
const LATEST_RELEASE_URL: &str =
	"https://api.github.com/repos/Gridness/a365dt/releases/latest";
const CACHE_FILE: &str = "latest-release.json";
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn show() {
	if !io::stdout().is_terminal() {
		return;
	}

	let update = latest_release().await.and_then(available_update);
	println!();
	if let Some(update) = update {
		show_update(&update);
		println!();
	}
	if let Some(tip) = random_tip() {
		println!("{}: {}", style("Tip").bold(), render_markdown(tip));
		println!();
	}
}

async fn latest_release() -> Option<Release> {
	let Some(cache_path) = app_files::cache_directory()
		.map(|directory| directory.join(CACHE_FILE))
	else {
		return fetch_release().await;
	};
	match read_cache(&cache_path) {
		Cache::Fresh(contents) => contents.release,
		Cache::Stale(contents) => refresh_release(&cache_path, contents).await,
		Cache::Missing => {
			refresh_release(&cache_path, CacheContents::default()).await
		}
	}
}

async fn refresh_release(
	cache_path: &Path,
	cached: CacheContents,
) -> Option<Release> {
	let contents = CacheContents {
		release: fetch_release().await.or(cached.release),
	};
	write_cache(cache_path, &contents);
	contents.release
}

async fn fetch_release() -> Option<Release> {
	let client = reqwest::Client::builder()
		.user_agent(concat!("a365dt/", env!("CARGO_PKG_VERSION")))
		.timeout(REQUEST_TIMEOUT)
		.build()
		.ok()?;
	let release = client
		.get(LATEST_RELEASE_URL)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await
		.ok()?
		.error_for_status()
		.ok()?
		.json::<Release>()
		.await
		.ok()?;
	Some(release)
}

fn read_cache(path: &Path) -> Cache {
	let Some(contents) = fs::read(path)
		.ok()
		.and_then(|contents| serde_json::from_slice(&contents).ok())
	else {
		return Cache::Missing;
	};
	let fresh = fs::metadata(path)
		.and_then(|metadata| metadata.modified())
		.ok()
		.and_then(|modified| modified.elapsed().ok())
		.is_some_and(|age| age < CACHE_TTL);
	if fresh {
		Cache::Fresh(contents)
	} else {
		Cache::Stale(contents)
	}
}

fn write_cache(path: &Path, cache: &CacheContents) {
	let Some(contents) = serde_json::to_vec(cache).ok() else {
		return;
	};
	let Some(directory) = path.parent() else {
		return;
	};
	if fs::create_dir_all(directory).is_ok() {
		// ponytail: a torn cache only skips one notice; use atomic replacement
		// if this cache ever becomes consequential.
		let _ = fs::write(path, contents);
	}
}

fn available_update(release: Release) -> Option<Update> {
	update_from(env!("CARGO_PKG_VERSION"), release)
}

fn update_from(installed: &str, release: Release) -> Option<Update> {
	let installed = Version::parse(installed).ok()?;
	let available = Version::parse(
		release
			.tag_name
			.strip_prefix('v')
			.unwrap_or(release.tag_name.as_str()),
	)
	.ok()?;
	if available <= installed || !available.pre.is_empty() {
		return None;
	}
	Some(Update {
		installed,
		available,
		release_url: release.html_url,
	})
}

fn show_update(update: &Update) {
	println!(
		"{} {} {} {}",
		style("💫 Upgrade available:").blue().bold(),
		style(format!("v{}", update.installed)).blue(),
		style("→").green(),
		style(format!("v{}", update.available)).blue()
	);
	println!(
		"   Upgrade: {}",
		upgrade_instruction(
			installation_channel(),
			update.release_url.as_str()
		)
	);
}

fn upgrade_instruction(
	channel: InstallationChannel,
	release_url: &str,
) -> String {
	match channel {
		InstallationChannel::Homebrew => {
			"brew upgrade Gridness/oosama/a365dt".to_owned()
		}
		InstallationChannel::WinGet => {
			"winget upgrade --id Gridness.a365dt --exact".to_owned()
		}
		InstallationChannel::Cargo => concat!(
			"cargo install --git https://github.com/Gridness/a365dt ",
			"--bin a365dt"
		)
		.to_owned(),
		InstallationChannel::Manual => {
			let executable = if cfg!(windows) {
				"a365dt.exe"
			} else {
				"a365dt"
			};
			format!("Download {release_url} and replace {executable}.")
		}
	}
}

fn installation_channel() -> InstallationChannel {
	let Ok(executable) = std::env::current_exe() else {
		return InstallationChannel::Manual;
	};
	installation_channel_from_path(&executable, &cargo_bin_directories())
}

fn installation_channel_from_path(
	executable: &Path,
	cargo_bin_directories: &[PathBuf],
) -> InstallationChannel {
	let executable = executable
		.canonicalize()
		.unwrap_or_else(|_| executable.into());
	let executable = normalized_path(&executable);
	if executable.contains("/cellar/a365dt/") {
		return InstallationChannel::Homebrew;
	}
	if executable.contains("/microsoft/winget/packages/gridness.a365dt_") {
		return InstallationChannel::WinGet;
	}
	let parent = executable.rsplit_once('/').map(|(parent, _)| parent);
	if cargo_bin_directories
		.iter()
		.map(|directory| normalized_path(directory))
		.any(|directory| Some(directory.trim_end_matches('/')) == parent)
	{
		return InstallationChannel::Cargo;
	}
	InstallationChannel::Manual
}

fn cargo_bin_directories() -> Vec<PathBuf> {
	let mut directories = Vec::new();
	if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
		directories.push(PathBuf::from(cargo_home).join("bin"));
	}
	for home in ["HOME", "USERPROFILE"]
		.into_iter()
		.filter_map(std::env::var_os)
	{
		directories.push(PathBuf::from(home).join(".cargo").join("bin"));
	}
	directories.sort_unstable();
	directories.dedup();
	directories
}

fn normalized_path(path: &Path) -> String {
	path.to_string_lossy()
		.replace('\\', "/")
		.to_ascii_lowercase()
}

fn random_tip() -> Option<&'static str> {
	let tips = TIPS
		.lines()
		.map(str::trim)
		.filter(|tip| !tip.is_empty())
		.collect::<Vec<_>>();
	if tips.is_empty() {
		return None;
	}
	let now = SystemTime::now()
		.duration_since(SystemTime::UNIX_EPOCH)
		.map_or(0, |duration| duration.as_nanos());
	let index =
		RandomState::new().hash_one((now, process::id())) as usize % tips.len();
	Some(tips[index])
}

fn render_markdown(markdown: &str) -> String {
	let mut output = String::new();
	let mut state = MarkdownStyle::default();
	for event in Parser::new(markdown) {
		match event {
			Event::Start(Tag::Strong) => state.strong += 1,
			Event::End(TagEnd::Strong) => {
				state.strong = state.strong.saturating_sub(1);
			}
			Event::Start(Tag::Emphasis) => state.emphasis += 1,
			Event::End(TagEnd::Emphasis) => {
				state.emphasis = state.emphasis.saturating_sub(1);
			}
			Event::Start(Tag::Link { dest_url, .. }) => {
				state.links.push(dest_url.into_string());
			}
			Event::End(TagEnd::Link) => {
				if let Some(url) = state.links.pop() {
					output.push_str(" (");
					output.push_str(&url);
					output.push(')');
				}
			}
			Event::Text(text) => {
				push_markdown_text(
					&mut output,
					&text,
					&state,
					MarkdownText::Normal,
				);
			}
			Event::Code(code) => {
				push_markdown_text(
					&mut output,
					&code,
					&state,
					MarkdownText::Code,
				);
			}
			Event::InlineMath(text)
			| Event::DisplayMath(text)
			| Event::Html(text)
			| Event::InlineHtml(text)
			| Event::FootnoteReference(text) => {
				push_markdown_text(
					&mut output,
					&text,
					&state,
					MarkdownText::Normal,
				);
			}
			Event::SoftBreak | Event::HardBreak => output.push(' '),
			Event::Rule => output.push('—'),
			Event::TaskListMarker(checked) => {
				output.push_str(if checked { "[x] " } else { "[ ] " });
			}
			Event::Start(_) | Event::End(_) => {}
		}
	}
	output
}

fn push_markdown_text(
	output: &mut String,
	text: &str,
	state: &MarkdownStyle,
	kind: MarkdownText,
) {
	let mut markdown_style = Style::new();
	if state.strong > 0 {
		markdown_style = markdown_style.bold();
	}
	if state.emphasis > 0 {
		markdown_style = markdown_style.italic();
	}
	if kind == MarkdownText::Code {
		markdown_style = markdown_style.cyan();
	}
	if !state.links.is_empty() {
		markdown_style = markdown_style.blue().underlined();
	}
	output.push_str(&markdown_style.apply_to(text).to_string());
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Release {
	tag_name: String,
	html_url: String,
}

#[derive(Default, Deserialize, Serialize)]
struct CacheContents {
	release: Option<Release>,
}

enum Cache {
	Fresh(CacheContents),
	Stale(CacheContents),
	Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Update {
	installed: Version,
	available: Version,
	release_url: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstallationChannel {
	Homebrew,
	WinGet,
	Cargo,
	Manual,
}

#[derive(Default)]
struct MarkdownStyle {
	strong: usize,
	emphasis: usize,
	links: Vec<String>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum MarkdownText {
	Normal,
	Code,
}

#[cfg(test)]
#[path = "startup_tests.rs"]
mod tests;
