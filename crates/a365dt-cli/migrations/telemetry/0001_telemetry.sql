CREATE TABLE collection_state (
	singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
	enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
	last_enabled_at_ms INTEGER CHECK (last_enabled_at_ms >= 0),
	last_disabled_at_ms INTEGER CHECK (last_disabled_at_ms >= 0),
	last_cleared_at_ms INTEGER CHECK (last_cleared_at_ms >= 0)
) STRICT;

INSERT INTO collection_state VALUES (1, 1, NULL, NULL, NULL);

CREATE TABLE command_events (
	id INTEGER PRIMARY KEY,
	invocation_id TEXT NOT NULL CHECK (length(invocation_id) = 36),
	observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
	command TEXT NOT NULL CHECK (command IN (
		'cache_prune', 'completions', 'doctor', 'download', 'stats',
		'telemetry_disable', 'telemetry_enable', 'telemetry_show', 'update'
	)),
	outcome TEXT NOT NULL CHECK (outcome IN (
		'success', 'failure', 'cancelled'
	))
) STRICT;

CREATE TABLE series_selection_events (
	id INTEGER PRIMARY KEY,
	invocation_id TEXT NOT NULL CHECK (length(invocation_id) = 36),
	observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
	series_id INTEGER NOT NULL CHECK (series_id > 0),
	series_title TEXT NOT NULL CHECK (series_title <> ''),
	catalogue_result TEXT CHECK (catalogue_result IN ('hit', 'miss'))
) STRICT;

CREATE TABLE download_batches (
	id INTEGER PRIMARY KEY,
	invocation_id TEXT NOT NULL CHECK (length(invocation_id) = 36),
	observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
	series_id INTEGER NOT NULL CHECK (series_id > 0),
	series_title TEXT NOT NULL CHECK (series_title <> ''),
	duration_us INTEGER NOT NULL CHECK (duration_us >= 0)
) STRICT;

CREATE TABLE download_outcomes (
	id INTEGER PRIMARY KEY,
	batch_id INTEGER NOT NULL
		REFERENCES download_batches(id) ON DELETE CASCADE,
	status TEXT NOT NULL CHECK (status IN (
		'downloaded', 'skipped', 'failed', 'mux_failed', 'interrupted'
	)),
	downloaded_bytes INTEGER CHECK (downloaded_bytes >= 0),
	CHECK (
		(status = 'downloaded') = (downloaded_bytes IS NOT NULL)
	)
) STRICT;

CREATE TABLE performance_events (
	id INTEGER PRIMARY KEY,
	invocation_id TEXT NOT NULL CHECK (length(invocation_id) = 36),
	observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
	operation TEXT NOT NULL CHECK (operation IN (
		'request.api.embed', 'request.api.search', 'request.api.series',
		'request.api.series_page', 'request.api.translations',
		'request.api.validate', 'request.asset.get', 'request.asset.head',
		'request.asset.resume', 'cache.retrieve', 'cache.store',
		'search.index', 'search.rank'
	)),
	duration_us INTEGER NOT NULL CHECK (duration_us >= 0),
	work_units INTEGER CHECK (work_units >= 0)
) STRICT;

CREATE INDEX command_events_by_time
	ON command_events(observed_at_ms, id);
CREATE INDEX command_events_by_invocation
	ON command_events(invocation_id, observed_at_ms, id);
CREATE INDEX series_selection_events_by_time
	ON series_selection_events(observed_at_ms, id);
CREATE INDEX series_selection_events_by_invocation
	ON series_selection_events(invocation_id, observed_at_ms, id);
CREATE INDEX download_batches_by_time
	ON download_batches(observed_at_ms, id);
CREATE INDEX download_batches_by_invocation
	ON download_batches(invocation_id, observed_at_ms, id);
CREATE INDEX download_outcomes_by_batch
	ON download_outcomes(batch_id);
CREATE INDEX performance_events_by_time
	ON performance_events(observed_at_ms, id);
CREATE INDEX performance_events_by_invocation
	ON performance_events(invocation_id, observed_at_ms, id);
CREATE INDEX performance_events_by_operation
	ON performance_events(operation, observed_at_ms, id);
