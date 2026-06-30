//! Class-instance traversal-state source scans (issue #14351).

use std::fs;
use std::path::{Path, PathBuf};

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_checker_source(relative: &str) -> String {
    fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"))
}

#[test]
fn class_instance_walk_state_owns_symbol_and_node_tracking() {
    let source = read_checker_source("src/types/class_type/walk_state.rs");
    for required in [
        "struct ClassInstanceWalkState",
        "fn enter_symbol(",
        "fn enter_node(",
        "fn contains_base_symbol(",
        "fn contains_node(",
        "fn node_depth(",
        "fn leave_class(",
    ] {
        assert!(
            source.contains(required),
            "class instance traversal state must own `{required}`"
        );
    }
}

#[test]
fn class_instance_orchestration_threads_named_walk_state() {
    let entry = read_checker_source("src/types/class_type/entry.rs");
    assert!(
        entry.contains("let mut walk_state = ClassInstanceWalkState::default();"),
        "class instance entrypoint should seed the named walk state"
    );

    for (relative, required) in [
        (
            "src/types/class_type/core.rs",
            "walk_state: &mut ClassInstanceWalkState",
        ),
        (
            "src/types/class_type/instance_merge.rs",
            "walk_state: &mut ClassInstanceWalkState",
        ),
    ] {
        let source = read_checker_source(relative);
        assert!(
            source.contains(required),
            "{relative} should thread class instance traversal through ClassInstanceWalkState"
        );
    }
}

#[test]
fn class_instance_orchestration_does_not_reintroduce_raw_walk_sets() {
    const CLEAN_MODULES: &[&str] = &[
        "src/types/class_type/entry.rs",
        "src/types/class_type/core.rs",
        "src/types/class_type/instance_merge.rs",
    ];
    const FORBIDDEN: &[&str] = &[
        "visited: &mut FxHashSet",
        "visited_nodes",
        "let mut visited = FxHashSet",
        "let mut visited_nodes = FxHashSet",
        "FxHashSet<SymbolId>",
        "FxHashSet<NodeIndex>",
    ];

    let mut violations = Vec::new();
    for relative in CLEAN_MODULES {
        let source = read_checker_source(relative);
        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for pattern in FORBIDDEN {
                if line.contains(pattern) {
                    violations.push(format!(
                        "{relative}:{} contains `{pattern}`",
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "class instance type construction should use ClassInstanceWalkState \
         instead of paired raw visited sets:\n{}",
        violations.join("\n")
    );
}
