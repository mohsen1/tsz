//! Source ratchets for checker lazy-resolution guard ownership.
//!
//! Global lazy-resolution fuel influences speculation rollback and whether
//! checker diagnostics may memoize failures, so it must be owned by an explicit
//! checker session instead of process/thread ambient state.

use std::fs;
use std::path::PathBuf;

fn checker_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn global_resolution_fuel_is_checker_session_owned() {
    let source_path = checker_path("src/state/type_environment/lazy_fuel.rs");
    let source = fs::read_to_string(&source_path)
        .unwrap_or_else(|_| panic!("failed to read {}", source_path.display()));
    let source = strip_line_comments(&source);

    assert!(
        !source.contains("thread_local!"),
        "global lazy-resolution fuel must be stored on an explicit checker-owned session, not thread_local! state",
    );
    assert!(
        !source.contains("GLOBAL_RESOLUTION_FUEL"),
        "global lazy-resolution fuel must be addressed through the checker session, not a module-global cell",
    );
}
