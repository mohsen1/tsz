use std::fs;
use std::path::{Path, PathBuf};

const RAW_RELATION_PATTERNS: &[&str] = &[
    ".is_assignable_to(",
    ".is_assignable_to_bivariant(",
    ".is_assignable_to_no_erase_generics(",
    ".is_assignable_to_no_weak_checks(",
    ".is_assignable_to_strict(",
    ".is_assignable_to_strict_null(",
    ".is_assignable_to_with_env(",
    ".is_subtype_of(",
    ".assign_relation_outcome(",
    ".assign_relation_outcome_with_env(",
];

fn rust_sources_under(dir: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    collect_rust_sources(dir, &mut sources);
    sources.sort();
    sources
}

fn collect_rust_sources(dir: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("failed to read source directory") {
        let path = entry.expect("failed to read source entry").path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) != Some("tests") {
                collect_rust_sources(&path, sources);
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            sources.push(path);
        }
    }
}

fn allowed_raw_relation(relative_path: &str, line: &str) -> bool {
    if relative_path.starts_with("src/assignability/") {
        return true;
    }

    if relative_path.starts_with("src/query_boundaries/")
        && line.contains(".assign_relation_outcome(")
    {
        return true;
    }

    relative_path == "src/types/computation/array_literal.rs"
        && line.contains("self.is_subtype_of(elem_type, context_element_type)")
}

#[test]
fn production_checker_relation_truth_uses_outcome_boundaries() {
    let mut violations = Vec::new();

    for path in rust_sources_under(Path::new("src")) {
        let relative_path = path.to_string_lossy().replace('\\', "/");
        let source = fs::read_to_string(&path).expect("failed to read Rust source");
        for (line_index, line) in source.lines().enumerate() {
            for pattern in RAW_RELATION_PATTERNS {
                if line.contains(pattern) && !allowed_raw_relation(&relative_path, line) {
                    violations.push(format!(
                        "{}:{} contains raw relation call `{}`",
                        relative_path,
                        line_index + 1,
                        pattern
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "checker production code should route relation truth through outcome-shaped boundaries\n{}",
        violations.join("\n")
    );
}
