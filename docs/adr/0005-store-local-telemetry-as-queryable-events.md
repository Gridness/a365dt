---
status: accepted
---

# Store local telemetry as queryable events

a365dt stores immutable, timestamped telemetry events in a local SQLite database and derives counters and statistics when queried so future interfaces can explore the underlying history. Collection remains optional and never transmits data; events may identify a selected Series by title and Anime365 Series ID, while search text, candidates, Episode identity, URLs, paths, and tokens remain excluded. Events are retained until the user explicitly clears all or part of the history.
