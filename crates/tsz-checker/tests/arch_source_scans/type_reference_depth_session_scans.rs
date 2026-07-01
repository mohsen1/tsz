//! Type-reference depth guard source scans (issue #14351).

use std::fs;
use std::path::Path;

fn read_checker_source(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {relative}: {err}"))
}

#[test]
fn type_reference_depth_guard_is_checker_session_owned() {
    let local_guard_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/type_resolution/symbol_types_depth.rs");
    let modules = read_checker_source("src/state/type_resolution/mod.rs");
    let symbol_types = read_checker_source("src/state/type_resolution/symbol_types.rs");
    let reset = read_checker_source("src/context/file_session_reset.rs");
    let session = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-solver/src/evaluation/session.rs"),
    )
    .expect("failed to read solver evaluation session source");
    let limits = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-solver/src/limits/mod.rs"),
    )
    .expect("failed to read solver limits source");

    assert!(
        !local_guard_path.exists(),
        "type-reference alias-forwarding depth should not live in a checker-local guard module"
    );
    assert!(
        !modules.contains("symbol_types_depth"),
        "type-resolution modules should not reintroduce the checker-local depth guard"
    );
    assert!(
        symbol_types.contains("eval_session.enter_type_reference_resolution_depth()"),
        "type-reference resolution should enter through the shared evaluation session"
    );
    assert!(
        session.contains("type_reference_resolution_depth: Cell<u32>"),
        "EvaluationSession should own the type-reference depth counter"
    );
    assert!(
        session.contains("pub fn enter_type_reference_resolution_depth("),
        "EvaluationSession should expose a typed entrypoint for the guard"
    );
    assert!(
        session.contains("crate::limits::MAX_TYPE_REFERENCE_RESOLUTION_DEPTH"),
        "EvaluationSession should use the centralized solver limit"
    );
    assert!(
        limits.contains("MAX_TYPE_REFERENCE_RESOLUTION_DEPTH"),
        "type-reference depth should be recorded in the solver limit inventory"
    );
    assert!(
        reset.contains("self.eval_session.reset_type_reference_resolution_depth();"),
        "file-session reset should clear the type-reference depth counter"
    );
}
