use std::{
	fs::{self, OpenOptions},
	io::{BufRead, BufReader, Write},
	path::{Path, PathBuf},
	process::{self, Child, ChildStdin, ChildStdout, Command, Stdio},
	time::SystemTime,
};

use pretty_assertions::assert_eq;

use super::{Catalogue, RebuildPermission, Store, storage::prune_at};
use crate::{api::Series, telemetry::Recorder};

const FIRST_OPEN_WORKER: &str =
	"cache::process_tests::worker_concurrent_first_open";
const REFRESH_WORKER: &str = "cache::process_tests::worker_stale_refresh";
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
	fs::remove_dir_all(directory).unwrap();
}

async fn revision_interleaving() {
	let directory = temporary_directory("revision");
	fs::create_dir_all(&directory).unwrap();
	let mut stale = Worker::spawn(REFRESH_WORKER, &directory);
	stale.wait_for("LOADED");

	let store = Store::at(directory.clone()).await;
	let (_, writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	writer.discover(vec![series(2, "Concurrent discovery")]);
	writer.finish().await.unwrap();
	store.close().await;

	stale.send("REFRESH");
	stale.wait_for("REFRESHED");
	stale.finish();

	let store = Store::at(directory.clone()).await;
	let (mut catalogue, writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	writer.finish().await.unwrap();
	assert_eq!(
		matching_series(&mut catalogue),
		vec![
			series(1, "Stale refresh"),
			series(2, "Concurrent discovery"),
		]
	);
	store.close().await;
	fs::remove_dir_all(directory).unwrap();
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
	wait_for_input("REFRESH");
	writer.commit_refresh(vec![series(1, "Stale refresh")]);
	writer.finish().await.unwrap();
	store.close().await;
	barrier("REFRESHED");
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
	let mut line = String::new();
	std::io::stdin().read_line(&mut line).unwrap();
	assert_eq!(line.trim(), expected);
}

fn barrier(token: &str) {
	println!("\n{token}");
	std::io::stdout().flush().unwrap();
}

fn matching_series(catalogue: &mut Catalogue) -> Vec<Series> {
	let suggestions = catalogue.suggestions("", &[], &Recorder::default());
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
