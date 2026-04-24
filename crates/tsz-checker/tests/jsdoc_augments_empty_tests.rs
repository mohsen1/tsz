use tsz_checker::context::CheckerOptions;

fn check_js_with_jsdoc(source: &str) -> Vec<(u32, String)> {
    let mut parser = tsz_parser::parser::ParserState::new("a.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = tsz_solver::TypeInterner::new();
    let options = CheckerOptions {
        check_js: true,
        ..CheckerOptions::default()
    };
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "a.js".to_string(),
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
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
