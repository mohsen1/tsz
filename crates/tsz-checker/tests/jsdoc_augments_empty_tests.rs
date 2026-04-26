use tsz_checker::context::CheckerOptions;

fn check_js_with_jsdoc(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "a.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
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
fn empty_augments_prevents_base_property_merge() {
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
        codes.contains(&2339),
        "expected TS2339 for this.x on B with empty @augments, got {codes:?}"
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
