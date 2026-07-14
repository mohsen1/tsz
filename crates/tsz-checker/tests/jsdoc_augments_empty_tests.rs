use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_js_source_code_messages_with_options;

fn check_js_with_jsdoc(source: &str) -> Vec<(u32, String)> {
    check_js_source_code_messages_with_options(
        source,
        "a.js",
        CheckerOptions {
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn empty_augments_emits_ts1003_and_ts8023() {
    let source = r#"
class A { constructor() { this.x = 0; } }
/** @augments */
class B extends A {
    m() {
        this.x
    }
}
"#;
    let diags = check_js_with_jsdoc(source);
    let codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(codes.contains(&1003), "expected TS1003, got {codes:?}");
    assert!(codes.contains(&8023), "expected TS8023, got {codes:?}");
}

#[test]
fn empty_augments_keeps_base_property_merge() {
    // Oracle-refuted premise (#15752): an empty `@augments` does NOT sever the
    // real `extends A` clause — tsc 7.0.2 keeps the heritage edge (it reports
    // only the malformed-tag TS8023/TS1003), so `this.x` resolves through the
    // base with no TS2339.
    let source = r#"
class A { constructor() { this.x = 0; } }
/** @augments */
class B extends A {
    m() {
        this.x
    }
}
"#;
    let diags = check_js_with_jsdoc(source);
    let codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        !codes.contains(&2339),
        "empty @augments keeps the extends edge, so base property access is allowed, got {codes:?}"
    );
}

#[test]
fn valid_augments_allows_base_property_access() {
    let source = r#"
class A { constructor() { this.x = 0; } }
/** @augments {A} */
class B extends A {
    m() {
        this.x
    }
}
"#;
    let diags = check_js_with_jsdoc(source);
    let codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        !codes.contains(&2339),
        "should NOT emit TS2339 when @augments is valid, got {codes:?}"
    );
}

#[test]
fn no_augments_allows_base_property_access() {
    let source = r#"
class A { constructor() { this.x = 0; } }
class B extends A {
    m() {
        this.x
    }
}
"#;
    let diags = check_js_with_jsdoc(source);
    let codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        !codes.contains(&2339),
        "should NOT emit TS2339 when no @augments tag, got {codes:?}"
    );
}
