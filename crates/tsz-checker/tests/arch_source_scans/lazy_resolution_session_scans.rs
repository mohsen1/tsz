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

fn read_checker_source(relative: &str) -> String {
    let source_path = checker_path(relative);
    fs::read_to_string(&source_path)
        .unwrap_or_else(|_| panic!("failed to read {}", source_path.display()))
}

fn read_solver_source(relative: &str) -> String {
    let source_path = checker_path("../tsz-solver").join(relative);
    fs::read_to_string(&source_path)
        .unwrap_or_else(|_| panic!("failed to read {}", source_path.display()))
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
    let source = read_checker_source("src/state/type_environment/lazy_fuel.rs");
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

#[test]
fn lazy_readiness_guards_are_checker_session_owned() {
    let lazy = strip_line_comments(&read_checker_source("src/state/type_environment/lazy.rs"));
    let session = read_solver_source("src/evaluation/session.rs");
    let limits = read_solver_source("src/limits/mod.rs");

    assert!(
        !lazy.contains("thread_local!"),
        "checker lazy-readiness guards must not live in lazy.rs TLS state",
    );
    for retired_name in [
        "APP_SYMBOL_RESOLUTION_DEPTH",
        "APP_SYMBOL_RESOLUTION_FUEL",
        "REFS_RESOLUTION_FUEL",
        "REFS_RESOLUTION_ACTIVE",
        "EVAL_ENV_DEPTH",
    ] {
        assert!(
            !lazy.contains(retired_name),
            "{retired_name} should be owned by EvaluationSession, not lazy.rs"
        );
    }

    for session_member in [
        "checker_app_symbol_resolution_depth",
        "checker_app_symbol_resolution_fuel",
        "checker_refs_resolution_fuel",
        "checker_refs_resolution_active",
        "checker_eval_env_depth",
    ] {
        assert!(
            session.contains(session_member),
            "EvaluationSession should own {session_member}"
        );
    }
    for limit_name in [
        "MAX_CHECKER_APP_SYMBOL_RESOLUTION_DEPTH",
        "MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL",
        "MAX_CHECKER_REFS_RESOLUTION_FUEL",
        "MAX_CHECKER_EVAL_ENV_DEPTH",
    ] {
        assert!(
            limits.contains(limit_name),
            "{limit_name} should be part of the centralized solver limit inventory"
        );
    }
}

#[test]
fn relation_input_readiness_keeps_both_steps_without_outer_fuel_gate() {
    let assignability = strip_line_comments(&read_checker_source(
        "src/assignability/assignability_checker.rs",
    ));
    let start = assignability
        .find("pub(crate) fn ensure_relation_input_ready")
        .expect("ensure_relation_input_ready should exist");
    let rest = &assignability[start..];
    let end = rest
        .find("pub(crate) fn ensure_relation_inputs_ready")
        .expect("next relation-input helper should delimit the function");
    let function_body = &rest[..end];

    assert!(
        function_body.contains("self.ensure_refs_resolved(type_id);"),
        "relation input readiness should still force direct lazy refs"
    );
    assert!(
        function_body.contains("self.ensure_application_symbols_resolved(type_id);"),
        "relation input readiness should still force application symbols"
    );
    assert!(
        !function_body.contains("lazy_resolution_fuel_exhausted")
            && !function_body.contains("refs_resolution_fuel_exhausted"),
        "relation input readiness should not have an outer fuel gate before the readiness steps"
    );
}
