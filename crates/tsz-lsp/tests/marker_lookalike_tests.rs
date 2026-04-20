//! Regression tests: production LSP providers must treat marker-looking
//! comments as ordinary TypeScript block comments.
//!
//! A user may legitimately write a source file containing comments that look
//! like fourslash markers (e.g. `/*1*/`, `/**/`, `/*completion*/`). Such
//! comments must not alter completion, hover, signature help, rename,
//! definition, or diagnostic results.
//!
//! These tests compare behavior between a plain source file and the same
//! file with marker-looking comments interleaved, asserting that the results
//! are equivalent (modulo legitimate offset shifts caused by the inserted
//! comment text).

use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;
use tsz_solver::TypeInterner;

/// Completion labels at `position` for the given source string.
fn completion_labels(source: &str, position: Position) -> Vec<String> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    let items = completions
        .get_completions(root, position)
        .unwrap_or_default();
    let mut labels: Vec<String> = items.into_iter().map(|i| i.label).collect();
    labels.sort();
    labels
}

fn hover_display_at(source: &str, position: Position) -> Option<String> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let provider = hover::HoverProvider::new(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    provider
        .get_hover(root, position, &mut cache)
        .map(|info| info.display_string)
}

#[test]
fn marker_lookalike_comments_do_not_affect_member_completions() {
    // Plain file: member completions after `obj.` include declared properties.
    let plain = "const obj = { hello: 1, world: 2 };\nobj.\n";
    // Same file with a marker-looking comment sprinkled in.
    let annotated = "const obj = { hello: 1, world: 2 };\nobj./*1*/\n";

    // Cursor immediately after `obj.` on line 1 col 4.
    let plain_labels = completion_labels(plain, Position::new(1, 4));
    let annotated_labels = completion_labels(annotated, Position::new(1, 4));

    assert!(
        plain_labels.contains(&"hello".to_string()),
        "plain member completions must include 'hello'; got {plain_labels:?}"
    );
    assert!(
        annotated_labels.contains(&"hello".to_string()),
        "annotated member completions must also include 'hello'; got {annotated_labels:?}"
    );
    assert!(
        annotated_labels.contains(&"world".to_string()),
        "annotated member completions must also include 'world'; got {annotated_labels:?}"
    );
}

#[test]
fn marker_lookalike_comment_does_not_enable_completion_inside_comment() {
    // If the cursor is *inside* a block comment, completions must be
    // suppressed. That the comment looks like a fourslash marker
    // (`/*1*/`, `/**/`) must not re-enable completion.
    let source = "const x = 42;\n/*1*/\n";
    // Cursor sits inside `/*1*/` (between the `*` and `1`).
    let position = Position::new(1, 3);

    let labels = completion_labels(source, position);
    assert!(
        labels.is_empty(),
        "expected no completions while cursor is inside a block comment, got {labels:?}"
    );
}

#[test]
fn marker_lookalike_comments_do_not_affect_hover() {
    let plain = "const greeting = 'hello';\ngreeting;\n";
    // Same code with marker-looking comments near each identifier.
    let annotated = "const /*def*/greeting = 'hello';\n/*ref*/greeting;\n";

    // Hover on the use of `greeting` on line 1.
    let plain_display = hover_display_at(plain, Position::new(1, 0));
    // In the annotated source `greeting` starts at column 7 (after `/*ref*/`).
    let annotated_display = hover_display_at(annotated, Position::new(1, 7));

    let plain_display = plain_display.expect("plain hover should return info");
    let annotated_display = annotated_display.expect("annotated hover should return info");
    assert_eq!(
        plain_display, annotated_display,
        "marker-looking comments must not change hover output"
    );
}

#[test]
fn marker_lookalike_identifier_suffix_does_not_filter_members() {
    // Strings have many apparent members (toString, charAt, replace, ...).
    // A marker-looking comment after the value must not filter the set.
    let source = "const s = 'a';\ns.\n";
    let annotated = "const s = 'a';\ns./*after*/\n";

    let plain_labels = completion_labels(source, Position::new(1, 2));
    let annotated_labels = completion_labels(annotated, Position::new(1, 2));

    for expected in ["toString", "charAt", "replace", "split", "trim", "length"] {
        assert!(
            plain_labels.iter().any(|l| l == expected),
            "plain string member completions must include '{expected}', got {plain_labels:?}"
        );
        assert!(
            annotated_labels.iter().any(|l| l == expected),
            "annotated string member completions must include '{expected}', got {annotated_labels:?}"
        );
    }
}

#[test]
fn completion_ordering_is_independent_of_marker_names() {
    // Two functionally identical files that differ only in marker content.
    // Completion labels must match exactly.
    let a = "const apple = 1;\nconst banana = 2;\n/*a*/\n";
    let b = "const apple = 1;\nconst banana = 2;\n/*zzz*/\n";

    let labels_a = completion_labels(a, Position::new(2, 3));
    let labels_b = completion_labels(b, Position::new(2, 3));

    // Cursor inside a block comment → no completions from either source.
    assert_eq!(labels_a, labels_b);
    assert!(labels_a.is_empty());
}

#[test]
fn production_code_does_not_special_case_slashslashslashslash_prefix() {
    // Real user code can start a line with `////`. It is just a line comment
    // with two forward slashes at the start of the comment body — nothing
    // more. Completions must be suppressed on that line exactly as for any
    // other line comment.
    let source = "const x = 1;\n//// typed a thought\n";
    // Cursor after the `//// typed a thought` text.
    let position = Position::new(1, 20);

    let labels = completion_labels(source, position);
    assert!(
        labels.is_empty(),
        "line starting with `////` must suppress completions like any other `//` comment; got {labels:?}"
    );
}
