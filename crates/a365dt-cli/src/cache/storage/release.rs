use std::time::Duration;

use sqlx::SqlitePool;

use super::{
	CompletedRelease, Release, ReleaseState, i64_from, now_ms, read_error,
	u64_from, write_error,
};

const TTL: Duration = Duration::from_secs(10 * 60);

pub(super) async fn load(
	pool: &SqlitePool,
) -> Result<ReleaseState, crate::error::Error> {
	let row = sqlx::query_as::<_, (String, String, i64)>(
		"SELECT tag_name, html_url, completed_at_ms FROM release \
		 WHERE singleton = 1",
	)
	.fetch_optional(pool)
	.await
	.map_err(read_error)?;
	let Some((tag_name, html_url, completed_at_ms)) = row else {
		return Ok(ReleaseState::Missing);
	};
	let completed_at_ms = u64_from(completed_at_ms, "release completion time")?;
	let release = Release { tag_name, html_url };
	let now = now_ms();
	Ok(
		if completed_at_ms <= now
			&& now - completed_at_ms < TTL.as_millis() as u64
		{
			ReleaseState::Fresh(release)
		} else {
			ReleaseState::Stale(release)
		},
	)
}

pub(super) async fn save(
	pool: &SqlitePool,
	completed: CompletedRelease,
) -> Result<(), crate::error::Error> {
	let mut transaction = pool
		.begin_with("BEGIN IMMEDIATE")
		.await
		.map_err(write_error)?;
	sqlx::query(
		"INSERT INTO release \
		 (singleton, tag_name, html_url, completed_at_ms) \
		 VALUES (1, ?, ?, ?) \
		 ON CONFLICT(singleton) DO UPDATE SET \
		 tag_name = excluded.tag_name, \
		 html_url = excluded.html_url, \
		 completed_at_ms = excluded.completed_at_ms \
		 WHERE excluded.completed_at_ms > release.completed_at_ms",
	)
	.bind(completed.release.tag_name)
	.bind(completed.release.html_url)
	.bind(i64_from(
		completed.completed_at_ms,
		"release completion time",
	)?)
	.execute(&mut *transaction)
	.await
	.map_err(write_error)?;
	transaction.commit().await.map_err(write_error)
}
