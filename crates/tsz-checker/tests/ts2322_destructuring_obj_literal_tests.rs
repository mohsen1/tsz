use tsz_checker::test_utils::check_source_diagnostics;

/// tsc reports per-property TS2322 errors in the RHS object literal when
/// a destructuring variable declaration has a type annotation and the
/// initializer is an object literal with mismatching property values.
/// Previously tsz emitted a single TS2322 at the binding pattern position.
#[test]
fn object_destructuring_with_annotation_reports_per_property_errors() {
    let source = r#"
var {a1, a2}: { a1: number, a2: string } = { a1: true, a2: 1 }
"#;
    let diags = check_source_diagnostics(source);
    // Two per-property errors: one for a1 (boolean→number), one for a2 (number→string).
    assert_eq!(
        diags.len(),
        2,
        "expected 2 per-property TS2322 diagnostics, got: {diags:?}"
    );
    assert!(diags.iter().all(|d| d.code == 2322));
    assert!(
        diags
            .iter()
            .any(|d| d.message_text.contains("'boolean'") && d.message_text.contains("'number'")),
        "expected a 'boolean' not assignable to 'number' error, got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message_text.contains("'number'") && d.message_text.contains("'string'")),
        "expected a 'number' not assignable to 'string' error, got: {diags:?}"
    );
}

/// Errors should be anchored at the property keys inside the object literal,
/// not at the binding pattern or the whole initializer.
#[test]
fn object_destructuring_per_property_error_positions() {
    let source = "var {a1, a2}: { a1: number, a2: string } = { a1: true, a2: 1 }";
    let diags = check_source_diagnostics(source);
    assert_eq!(diags.len(), 2);
    // The object literal `{ a1: true, a2: 1 }` starts after `= `.
    // `a1` is at col 46 (1-indexed) and `a2` is at col 56 (1-indexed).
    // In our 0-indexed byte positions: a1→45, a2→55.
    let positions: Vec<u32> = diags.iter().map(|d| d.start).collect();
    // Both errors must be inside the RHS object literal (pos > 42), not at the
    // binding pattern (pos 4..11) or the whole declaration (pos 0).
    assert!(
        positions.iter().all(|&p| p > 40),
        "expected errors inside the RHS object literal (pos>40), got positions: {positions:?}"
    );
}

/// When the destructuring assignment is fine (types match), no TS2322 should fire.
#[test]
fn object_destructuring_no_error_when_types_match() {
    let source = r#"
var {a1, a2}: { a1: number, a2: string } = { a1: 42, a2: "hello" }
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().all(|d| d.code != 2322),
        "expected no TS2322 when types match, got: {diags:?}"
    );
}
