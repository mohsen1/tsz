//! Lifetime-owned checker context shells.
//!
//! These no-behavior shells reserve explicit homes for state that will move out
//! of `CheckerContext` in later pooling work. Keeping them empty in this slice
//! makes the lifetime boundary visible without changing checker behavior.

/// Worker-reusable checker state for future pooled checking.
///
/// `ProgramContext` owns project-wide immutable inputs. `WorkerContext` is the
/// next narrower scope: state that can be reused by one worker across many
/// files, but should not be shared process-wide. Fields will move here in later
/// PRs; this shell is intentionally empty for now.
#[derive(Clone, Debug, Default)]
pub struct WorkerContext {
    _private: (),
}

impl WorkerContext {
    /// Create an empty worker context shell.
    pub const fn new() -> Self {
        Self { _private: () }
    }

    /// Start a per-file session owned by this worker.
    pub const fn begin_file_session(&self) -> FileSession {
        FileSession::new()
    }
}

/// Per-file checker state for future resettable checking.
///
/// A `FileSession` is the intended home for state that is rebuilt for every
/// checked file. It deliberately carries no data in this PR, preserving current
/// behavior while documenting the next lifetime boundary after `WorkerContext`.
#[derive(Clone, Debug, Default)]
pub struct FileSession {
    _private: (),
}

impl FileSession {
    /// Create an empty per-file session shell.
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkerContext;

    #[test]
    fn worker_context_starts_empty_file_session() {
        let worker = WorkerContext::new();
        let _session = worker.begin_file_session();
    }
}
