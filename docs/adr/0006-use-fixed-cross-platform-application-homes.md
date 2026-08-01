---
status: accepted
---

# Use fixed cross-platform application homes

a365dt stores release application state under `~/.a365dt`, with cache files in `cache/` and telemetry files in `data/`; development builds use the same layout under `~/.a365dt-dev`. These fixed, user-private homes deliberately replace OS-specific application directories so state has one predictable location for inspection, backup, diagnosis, and removal; downloaded media and OS-managed credentials remain outside, and the paths are not configurable.

Legacy OS-specific locations remain detectable indefinitely. Except for state-free help, version, and completion generation, a stateful invocation requires an explicit interactive `[Y/n]` migration before continuing; non-interactive invocations fail with instructions, while `purge` deletes legacy and current state without migrating. Migration stages and validates cache and telemetry together before deleting legacy files, stops when another process is active or either location conflicts or contains unknown files, automatically rebuilds a damaged cache, and lets the user recreate damaged telemetry enabled, recreate it disabled, or cancel without changing anything.
