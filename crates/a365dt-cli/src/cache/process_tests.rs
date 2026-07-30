use std::{
	fs::{self, OpenOptions},
	io::{BufRead, BufReader, Write},
	path::{Path, PathBuf},
	process::{self, Child, ChildStdin, ChildStdout, Command, Stdio},
	time::SystemTime,
};

use pretty_assertions::assert_eq;

use super::{
	Catalogue, CompletedRelease, RebuildPermission, Release, ReleaseState,
	Store, storage::prune_at,
};
use crate::{api::Series, telemetry::Recorder};

const FIRST_OPEN_WORKER: &str =
	"cache::process_tests::worker_concurrent_first_open";
const REFRESH_WORKER: &str = "cache::process_tests::worker_stale_refresh";
const DELETE_WORKER: &str = "cache::process_tests::worker_stale_delete";
const RELEASE_WORKER: &str = "cache::process_tests::worker_release_completion";
const LOCK_WORKER: &str = "cache::process_tests::worker_lifecycle_lock";

#[tokio::test]
async fn cache_process_safety() {
	concurrent_first_open().await;
	revision_interleaving().await;
	lifecycle_rebuild().await;
}

async fn concurrent_first_open() {
	let directory = temporary_directory("first-open");
	fs::create_dir_all(&directory).unwrap();
	fs::write(directory.join("series.json"), b"legacy").unwrap();
	fs::write(directory.join("latest-release.json"), b"legacy").unwrap();
	let mut first = Worker::spawn(FIRST_OPEN_WORKER, &directory);
	let mut second = Worker::spawn(FIRST_OPEN_WORKER, &directory);

	first.send("OPEN");
	second.send("OPEN");
	first.wait_for("OPENED");
	second.wait_for("OPENED");
	assert!(directory.join("cache.sqlite").exists());
	assert!(!directory.join("series.json").exists());
	assert!(!directory.join("latest-release.json").exists());

	first.finish();
	second.finish();
	let store = Store::at(directory.clone()).await;
	assert!(matches!(
		store.inspect().await,
		super::Inspection::Missing { .. }
	));
	store.close().await;
	fs::remove_dir_all(directory).unwrap();
}

async fn revision_interleaving() {
	let directory = temporary_directory("revision");
	fs::create_dir_all(&directory).unwrap();
	let store = Store::at(directory.clone()).await;
	let (_, seed) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	seed.discover(vec![series(10, "Original")]);
	seed.finish().await.unwrap();
	store.close().await;

	let mut deletion = Worker::spawn(DELETE_WORKER, &directory);
	deletion.wait_for("LOADED");
	let store = Store::at(directory.clone()).await;
	let (_, update) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	update.remember_alias("known".into(), series(10, "Updated"));
	update.finish().await.unwrap();
	store.close().await;
	deletion.send("REMOVE");
	deletion.wait_for("REMOVED");
	deletion.finish();

	let mut stale = Worker::spawn(REFRESH_WORKER, &directory);
	let mut older = Worker::spawn(REFRESH_WORKER, &directory);
	stale.wait_for("LOADED");
	older.wait_for("LOADED");

	let store = Store::at(directory.clone()).await;
	let (_, writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	writer.discover(vec![
		series(1, "Concurrent update"),
		series(2, "Concurrent discovery"),
	]);
	writer.finish().await.unwrap();
	store.close().await;

	stale.send("Stale refresh");
	stale.wait_for("REFRESHED");
	stale.finish();
	older.send("Older refresh");
	older.wait_for("REFRESHED");
	older.finish();

	let store = Store::at(directory.clone()).await;
	let (mut catalogue, writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	writer.finish().await.unwrap();
	assert_eq!(
		(
			matching_series(&mut catalogue, ""),
			matching_series(&mut catalogue, "known"),
		),
		(
			vec![
				series(1, "Concurrent update"),
				series(10, "Updated"),
				series(2, "Concurrent discovery"),
			],
			vec![series(10, "Updated")],
		)
	);
	store.close().await;

	let mut first_release = Worker::spawn(RELEASE_WORKER, &directory);
	let mut newest_release = Worker::spawn(RELEASE_WORKER, &directory);
	first_release.wait_for("OPENED");
	newest_release.wait_for("OPENED");
	first_release.send("v1.0.0");
	first_release.wait_for("COMPLETED");
	newest_release.send("v2.0.0");
	newest_release.wait_for("COMPLETED");
	newest_release.send("SAVE");
	newest_release.wait_for("SAVED");
	first_release.send("SAVE");
	first_release.wait_for("SAVED");
	first_release.finish();
	newest_release.finish();

	let store = Store::at(directory.clone()).await;
	assert_eq!(
		store.load_release().await.unwrap(),
		ReleaseState::Fresh(Release {
			tag_name: "v2.0.0".into(),
			html_url: "https://example.com/v2.0.0".into(),
		})
	);
	store.close().await;

	let mut stale_after_prune = Worker::spawn(REFRESH_WORKER, &directory);
	stale_after_prune.wait_for("LOADED");
	prune_at(&directory, RebuildPermission::Preauthorized)
		.await
		.unwrap();
	stale_after_prune.send("After prune");
	stale_after_prune.wait_for("REFRESHED");
	stale_after_prune.finish();
	let store = Store::at(directory.clone()).await;
	assert!(matches!(
		store.inspect().await,
		super::Inspection::Missing { .. }
	));
	store.close().await;
	fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
#[ignore]
async fn worker_stale_delete() {
	let store = Store::at(std::env::current_dir().unwrap()).await;
	let (_, writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	barrier("LOADED");
	wait_for_input("REMOVE");
	writer.remove_missing(10);
	writer.finish().await.unwrap();
	barrier("REMOVED");
	store.close().await;
}

#[tokio::test]
#[ignore]
async fn worker_release_completion() {
	let store = Store::at(std::env::current_dir().unwrap()).await;
	barrier("OPENED");
	let tag_name = read_input();
	let completed = CompletedRelease::now(Release {
		tag_name: tag_name.clone(),
		html_url: format!("https://example.com/{tag_name}"),
	});
	barrier("COMPLETED");
	wait_for_input("SAVE");
	store.save_release(completed).await.unwrap();
	barrier("SAVED");
	store.close().await;
}

#[tokio::test]
#[ignore]
async fn worker_stale_refresh() {
	let directory = std::env::current_dir().unwrap();
	let store = Store::at(directory).await;
	let (_, writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	barrier("LOADED");
	let title = read_input();
	writer.commit_refresh(vec![series(1, &title)]);
	writer.finish().await.unwrap();
	store.close().await;
	barrier("REFRESHED");
}

#[tokio::test]
#[ignore]
async fn worker_concurrent_first_open() {
	wait_for_input("OPEN");
	let store = Store::at(std::env::current_dir().unwrap()).await;
	if let Some(error) = store.initialization_warning() {
		panic!("{}", error.render(true));
	}
	barrier("OPENED");
	store.close().await;
}

#[test]
#[ignore]
fn worker_lifecycle_lock() {
	let file = OpenOptions::new()
		.create(true)
		.truncate(false)
		.read(true)
		.write(true)
		.open(std::env::current_dir().unwrap().join("cache.lock"))
		.unwrap();
	file.try_lock_shared().unwrap();
	barrier("LOCKED");
	wait_for_input("RELEASE");
}

async fn lifecycle_rebuild() {
	let directory = temporary_directory("lifecycle");
	fs::create_dir_all(&directory).unwrap();
	let path = directory.join("cache.sqlite");
	fs::write(&path, b"damaged").unwrap();
	let mut lock = Worker::spawn(LOCK_WORKER, &directory);
	lock.wait_for("LOCKED");

	let error = prune_at(&directory, RebuildPermission::Preauthorized)
		.await
		.unwrap_err();
	assert!(error.to_string().contains("while it is in use"));
	assert_eq!(fs::read(&path).unwrap(), b"damaged");

	lock.send("RELEASE");
	lock.finish();
	prune_at(&directory, RebuildPermission::Preauthorized)
		.await
		.unwrap();
	let store = Store::at(directory.clone()).await;
	assert!(matches!(
		store.inspect().await,
		super::Inspection::Missing { .. }
	));
	store.close().await;
	fs::remove_dir_all(directory).unwrap();
}

struct Worker {
	child: Child,
	input: ChildStdin,
	output: BufReader<ChildStdout>,
}

impl Worker {
	fn spawn(test: &str, directory: &Path) -> Self {
		let mut child = Command::new(std::env::current_exe().unwrap())
			.args([
				"--exact",
				test,
				"--ignored",
				"--nocapture",
				"--test-threads=1",
			])
			.current_dir(directory)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit())
			.spawn()
			.unwrap();
		let input = child.stdin.take().unwrap();
		let output = BufReader::new(child.stdout.take().unwrap());
		Self {
			child,
			input,
			output,
		}
	}

	fn send(&mut self, token: &str) {
		writeln!(self.input, "{token}").unwrap();
		self.input.flush().unwrap();
	}

	fn wait_for(&mut self, token: &str) {
		let mut line = String::new();
		loop {
			line.clear();
			assert_ne!(self.output.read_line(&mut line).unwrap(), 0);
			if line.trim() == token {
				return;
			}
		}
	}

	fn finish(mut self) {
		drop(self.input);
		assert!(self.child.wait().unwrap().success());
	}
}

fn wait_for_input(expected: &str) {
	assert_eq!(read_input(), expected);
}

fn read_input() -> String {
	let mut line = String::new();
	std::io::stdin().read_line(&mut line).unwrap();
	line.trim().into()
}

fn barrier(token: &str) {
	println!("\n{token}");
	std::io::stdout().flush().unwrap();
}

fn matching_series(catalogue: &mut Catalogue, query: &str) -> Vec<Series> {
	let suggestions = catalogue.suggestions(query, &[], &Recorder::default());
	(0..suggestions.matches().len())
		.filter_map(|position| suggestions.series(position))
		.cloned()
		.collect()
}

fn series(id: u64, title: &str) -> Series {
	Series {
		id,
		title: title.into(),
		year: Some(2024),
		type_title: Some("TV".into()),
		number_of_episodes: Some(12),
		poster_url_small: None,
		episodes: Vec::new(),
	}
}

fn temporary_directory(name: &str) -> PathBuf {
	std::env::temp_dir().join(format!(
		"a365dt-cache-process-{name}-{}-{}",
		process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	))
}
