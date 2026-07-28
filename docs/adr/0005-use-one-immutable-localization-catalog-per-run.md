# Use one immutable localization catalog per run

a365dt initializes one process-wide Fluent catalog before full command-line parsing and uses it for the entire run. A run has one Run language, and immutable shared localization keeps help, errors, background tasks, and UI helpers consistent without threading a catalog through nearly every call site; tests may construct isolated catalogs directly.
