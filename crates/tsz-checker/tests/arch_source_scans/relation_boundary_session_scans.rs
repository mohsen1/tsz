//! Relation-boundary evaluation-session source scans (issue #14346).
//!
//! The solver's `EvaluationSession` owns cross-evaluator guard state
//! (conditional-subtype depth, infer-match expansion depth, the cross-eval
//! active set, and the per-query fresh-evaluator memo). A checker relation
//! boundary that passes `evaluation_session: None` runs the solver against
//! the thread-local fallback session instead of `ctx.eval_session` — so the
//! same recursive descent accrues guard state on two different sessions
//! depending on entry point (the #14346 split-brain). #15168 threaded the
//! subtype-identity boundary; this ratchet keeps every relation boundary
//! threaded.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_sources_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn relation_boundaries_thread_the_checker_evaluation_session() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    rust_sources_under(&src, &mut sources);
    assert!(
        sources.len() > 100,
        "checker source scan walked implausibly few files ({})",
        sources.len()
    );

    let mut offenders = Vec::new();
    for path in sources {
        // Test modules construct standalone inputs without a live checker
        // session; the split-brain concern is production boundaries only.
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with("_tests.rs") || path.components().any(|c| c.as_os_str() == "tests") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            if line.contains("evaluation_session: None") {
                offenders.push(format!("{}:{}", path.display(), i + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "checker relation boundaries passing `evaluation_session: None` (the \
         #14346 split-brain): {offenders:?}. Thread `ctx.eval_session` (see \
         `AssignabilityQueryInputs::evaluation_session` and #15168) instead of \
         letting the solver fall back to the thread-local session."
    );
}
