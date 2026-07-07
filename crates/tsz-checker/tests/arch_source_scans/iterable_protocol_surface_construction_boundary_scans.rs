//! Iterable protocol surface construction boundary scans.
//!
//! Iterable checking owns AST position, diagnostics, property-access fallback,
//! and ES5/downlevel iteration policy. Solver object-shape facts, atom
//! resolution, callable signature probing, and iterator-result evaluation for
//! iteration protocol surfaces belong in `query_boundaries::checkers::iterable`.

use std::fs;
use std::path::{Path, PathBuf};

const ITERABLE_CHECKER: &str = "src/checkers/iterable_checker.rs";
const ITERABLE_BOUNDARY: &str = "src/query_boundaries/checkers/iterable.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn production_source_without_comments(source: &str) -> String {
    let mut stripped = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut block_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if block_depth > 0 {
            match (ch, chars.peek().copied()) {
                ('/', Some('*')) => {
                    chars.next();
                    block_depth += 1;
                }
                ('*', Some('/')) => {
                    chars.next();
                    block_depth -= 1;
                }
                ('\n', _) => stripped.push('\n'),
                _ => {}
            }
            continue;
        }

        if in_string {
            stripped.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match (ch, chars.peek().copied()) {
            ('"', _) => {
                in_string = true;
                stripped.push(ch);
            }
            ('/', Some('/')) => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        stripped.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                block_depth = 1;
            }
            _ => stripped.push(ch),
        }
    }

    stripped
}

fn compact(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn contains_pattern(source: &str, pattern: &str) -> bool {
    source.contains(pattern) || compact(source).contains(&compact(pattern))
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

fn defines_any_fn(source: &str, names: &[&str]) -> bool {
    names.iter().any(|name| defines_fn(source, name))
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    let source = production_source_without_comments(&source);
    for pattern in patterns {
        if contains_pattern(&source, pattern) {
            violations.push(format!("{relative} contains `{pattern}`"));
        }
    }
}

#[test]
fn iterable_checker_routes_protocol_surface_facts_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "self.ctx.types.object_shape(",
        "self.ctx.types.resolve_atom_ref(",
        "self.ctx.types.intern_string(\"then\")",
        "self.ctx.types.intern_string(\"return\")",
        "function_shape_for_type(self.ctx.types,",
        "call_signatures_for_type(self.ctx.types,",
        "self.ctx.types.evaluate_type(",
    ];

    let mut violations = Vec::new();
    scan_for_patterns(ITERABLE_CHECKER, FORBIDDEN_PATTERNS, &mut violations);
    assert!(
        violations.is_empty(),
        "iterable protocol surface construction must route solver object/signature \
         facts through query_boundaries::checkers::iterable:\n{}",
        violations.join("\n")
    );
}

#[test]
fn iterable_boundary_owns_protocol_surface_helpers() {
    let source = fs::read_to_string(checker_path(ITERABLE_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/iterable.rs");
    let source = production_source_without_comments(&source);

    for (surface, helpers) in [
        (
            "sync iterator method status",
            &["iterator_method_status"][..],
        ),
        (
            "async iterator method status",
            &["async_iterator_method_status"],
        ),
        (
            "zero-argument callable check",
            &["callable_accepts_no_required_args"],
        ),
        ("callable return type", &["callable_return_type"]),
        (
            "first callable parameter type",
            &["first_callable_param_type"],
        ),
        (
            "promise-like awaited type",
            &["promise_like_awaited_type", "awaited_promise_like_type"],
        ),
        (
            "evaluated iterator-result value extraction",
            &["evaluated_iterator_result_value_types"],
        ),
        (
            "iterator return property status",
            &["iterator_return_property_status"],
        ),
        ("next method fact", &["type_has_next_method"]),
        (
            "numeric index signature fact",
            &[
                "has_numeric_index_signature",
                "numeric_index_signature_fact",
            ],
        ),
    ] {
        assert!(
            defines_any_fn(&source, helpers),
            "query_boundaries::checkers::iterable must own a helper for {surface}; \
             accepted helper names: {}",
            helpers.join(", ")
        );
    }

    for owned_pattern in [
        "db.object_shape(",
        "db.resolve_atom_ref(",
        "db.intern_string(\"then\")",
        "db.evaluate_type(",
        "function_shape_for_type(db,",
        "call_signatures_for_type(db,",
    ] {
        assert!(
            contains_pattern(&source, owned_pattern),
            "query_boundaries::checkers::iterable should own `{owned_pattern}`"
        );
    }
}
