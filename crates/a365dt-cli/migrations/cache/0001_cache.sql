CREATE TABLE catalogue_state (
	singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
	revision INTEGER NOT NULL CHECK (revision >= 0),
	current_generation INTEGER NOT NULL CHECK (current_generation >= 0),
	last_refresh_revision INTEGER NOT NULL CHECK (last_refresh_revision >= 0),
	refreshed_at INTEGER CHECK (refreshed_at >= 0),
	next_discovery_order INTEGER NOT NULL CHECK (next_discovery_order >= 0)
) STRICT;

INSERT INTO catalogue_state VALUES (1, 0, 0, 0, NULL, 0);

CREATE TABLE series (
	id INTEGER PRIMARY KEY CHECK (id > 0),
	title TEXT NOT NULL CHECK (title <> ''),
	year INTEGER CHECK (year BETWEEN 0 AND 65535),
	type_title TEXT,
	episode_count INTEGER CHECK (
		episode_count BETWEEN 0 AND 4294967295
	),
	revision INTEGER NOT NULL CHECK (revision >= 0),
	refresh_generation INTEGER CHECK (refresh_generation >= 0),
	refresh_position INTEGER CHECK (refresh_position >= 0),
	discovery_order INTEGER NOT NULL CHECK (discovery_order >= 0),
	CHECK (
		(refresh_generation IS NULL) = (refresh_position IS NULL)
	)
) STRICT;

CREATE TABLE aliases (
	query TEXT PRIMARY KEY CHECK (query <> ''),
	series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE
) STRICT;

CREATE INDEX aliases_by_series ON aliases(series_id);

CREATE INDEX series_by_refresh
	ON series(refresh_generation, refresh_position, id)
	WHERE refresh_generation IS NOT NULL;

CREATE TABLE release (
	singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
	tag_name TEXT NOT NULL CHECK (tag_name <> ''),
	html_url TEXT NOT NULL CHECK (html_url <> ''),
	completed_at_ms INTEGER NOT NULL CHECK (completed_at_ms >= 0)
) STRICT;
