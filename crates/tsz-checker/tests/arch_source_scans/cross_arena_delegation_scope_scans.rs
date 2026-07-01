//! Source ratchets for cross-arena delegation depth ownership.
//!
//! Cross-arena child checkers can bail through many different return paths.
//! The shared depth counter must therefore be owned by an RAII scope rather
//! than paired manual enter/leave calls at every delegation site.

use std::fs;
use std::path::{Path, PathBuf};

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_checker_source(relative: &str) -> String {
    fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"))
}

fn rust_sources_under(relative: &str) -> Vec<PathBuf> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap_or_else(|err| {
            panic!("failed to read source directory {}: {err}", dir.display())
        }) {
            let entry = entry.expect("failed to read source directory entry");
            let path = entry.path();
            if path.is_dir() {
                visit(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    let mut sources = Vec::new();
    visit(&checker_path(relative), &mut sources);
    sources
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn cross_arena_delegation_depth_is_raii_scoped() {
    let source = read_checker_source("src/state/state.rs");

    for required in [
        "struct CrossArenaDelegationGuard",
        "impl Drop for CrossArenaDelegationGuard",
        "-> Option<CrossArenaDelegationGuard>",
        "Some(CrossArenaDelegationGuard)",
    ] {
        assert!(
            source.contains(required),
            "cross-arena delegation depth must be represented by `{required}`"
        );
    }

    assert!(
        !source.contains("fn leave_cross_arena_delegation"),
        "cross-arena delegation depth must not expose a manual leave function"
    );
}

#[test]
fn cross_arena_delegation_callers_do_not_use_boolean_or_manual_leave_patterns() {
    let forbidden = [
        "leave_cross_arena_delegation(",
        "!Self::enter_cross_arena_delegation(",
        "!CheckerState::enter_cross_arena_delegation(",
        "&& Self::enter_cross_arena_delegation(",
        "&& CheckerState::enter_cross_arena_delegation(",
        "else if Self::enter_cross_arena_delegation(",
        "else if CheckerState::enter_cross_arena_delegation(",
    ];

    let mut violations = Vec::new();
    for path in rust_sources_under("src") {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let source = strip_line_comments(&source);
        for needle in forbidden {
            for (line_index, line) in source.lines().enumerate() {
                if line.contains(needle) {
                    let relative = path.strip_prefix(checker_path("")).unwrap_or(&path);
                    violations.push(format!(
                        "{}:{} contains `{needle}`",
                        relative.display(),
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "cross-arena delegation callers must hold `CrossArenaDelegationGuard` scopes:\n{}",
        violations.join("\n")
    );
}
