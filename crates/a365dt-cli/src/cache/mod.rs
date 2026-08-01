mod catalogue;
mod storage;
mod writer;

pub(crate) use catalogue::{Catalogue, MAX_AGE};
pub(crate) use storage::{
	CompletedRelease, Inspection, MigrationPreparation, RebuildPermission,
	Release, ReleaseState, Store, prepare_migration_at, prune,
};
pub(crate) use writer::{LoadedCatalogue, Writer};

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "process_tests.rs"]
mod process_tests;
