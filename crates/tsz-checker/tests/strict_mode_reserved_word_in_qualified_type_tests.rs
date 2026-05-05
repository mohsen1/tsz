//! Regression test: strict-mode reserved words at the lead of a qualified
//! type name (`var b: public.bar;`, `function f(x: private.x) {}`) should
//! emit TS1212/TS1213/TS1214 alongside TS2503 (cannot find namespace).
//!
//! Before this fix, the bare-identifier `TypeReference` path emitted
//! TS1212 in `state/type_resolution/core.rs`, but the qualified-name
//! resolver in `state/type_analysis/core.rs` only emitted TS2503 — the
//! lead-identifier reserved-word check was missing. tsc emits both.
//!
//! The check keys off the structural condition (name is in the
//! strict-mode-reserved set AND we are in strict mode at the node), not
//! on which specific reserved word appears, so any of `public`,
//! `private`, `protected`, `package`, `interface`, `implements`,
//! `static`, `let`, `yield` triggers it.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn checker_diag_codes(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = CheckerOptions::default();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

const TS1212: u32 = 1212;
const TS1213: u32 = 1213;
const TS1214: u32 = 1214;
const TS2300: u32 = 2300;
const TS2503: u32 = 2503;
const TS2507: u32 = 2507;

#[test]
fn public_at_head_of_qualified_type_emits_strict_mode_error() {
    // `function foo() { "use strict"; var b: public.bar; }`
    // tsc emits TS1212 ("'public' is a reserved word in strict mode.")
    // alongside TS2503 ("Cannot find namespace 'public'.").
    let src = "function foo() { \"use strict\"; var b: public.bar; }";
    let codes = checker_diag_codes(src);
    assert!(
        codes.contains(&TS1212) || codes.contains(&TS1213) || codes.contains(&TS1214),
        "expected TS1212/TS1213/TS1214 for `public` at lead of qualified type, got: {codes:?}"
    );
    assert!(
        codes.contains(&TS2503),
        "expected TS2503 (Cannot find namespace) to coexist with the strict-mode error, got: {codes:?}"
    );
}

#[test]
fn shape_holds_for_multiple_strict_mode_reserved_words() {
    // The fix keys off the strict-mode-reserved-set, not on a specific
    // identifier; switching the spelling must not change the outcome.
    for word in ["private", "protected", "package", "interface", "implements"] {
        let src = format!("function foo() {{ \"use strict\"; var b: {word}.x; }}");
        let codes = checker_diag_codes(&src);
        assert!(
            codes.contains(&TS1212) || codes.contains(&TS1213) || codes.contains(&TS1214),
            "word `{word}`: expected TS1212/TS1213/TS1214 alongside TS2503, got: {codes:?}"
        );
        assert!(
            codes.contains(&TS2503),
            "word `{word}`: expected TS2503, got: {codes:?}"
        );
    }
}

#[test]
fn non_reserved_lead_does_not_emit_strict_mode_error() {
    // `MyNs.bar` should still emit TS2503 (cannot find namespace) but
    // NOT TS1212/TS1213/TS1214 because `MyNs` is not a strict-mode
    // reserved word.
    let src = "function foo() { \"use strict\"; var b: MyNs.bar; }";
    let codes = checker_diag_codes(src);
    assert!(
        codes.contains(&TS2503),
        "expected TS2503 for unresolved `MyNs.bar`, got: {codes:?}"
    );
    assert!(
        !codes.contains(&TS1212) && !codes.contains(&TS1213) && !codes.contains(&TS1214),
        "must NOT emit TS1212/TS1213/TS1214 for non-reserved-word lead identifier, got: {codes:?}"
    );
}

#[test]
fn class_context_uses_class_specific_message() {
    // Inside a class body the strict-mode-reserved error is TS1213, not
    // TS1212. The general TypeReference path already chooses the right
    // code via `emit_strict_mode_reserved_word_error_with_ast_walk`; the
    // qualified-name path must do the same — the helper this fix calls
    // is the same one, so the class-context routing comes for free.
    let src = "class C { f(): public.bar { return null!; } }";
    let codes = checker_diag_codes(src);
    assert!(
        codes.contains(&TS1213),
        "expected TS1213 (class-specific reserved-word message) inside class body, got: {codes:?}"
    );
}

#[test]
fn recovered_strict_reserved_declarations_keep_duplicate_and_heritage_errors() {
    let src = r#"
function foo() {
    "use strict";
    var public = 10;
    var package = "hello";
    function package() { }
    var myClass = class package extends public {}
    var b: public.bar;
    let b: interface.package.implements.B;
}
"#;
    let codes = checker_diag_codes(src);
    let ts1213 = codes.iter().filter(|&&code| code == TS1213).count();
    let ts2300 = codes.iter().filter(|&&code| code == TS2300).count();

    assert!(
        ts1213 >= 2,
        "expected TS1213 for both `class package` and `extends public`, got: {codes:?}"
    );
    assert!(
        ts2300 >= 4,
        "expected TS2300 for both `package` declarations and both `b` declarations, got: {codes:?}"
    );
    assert!(
        codes.contains(&TS2507),
        "expected TS2507 when `extends public` resolves to the local number variable, got: {codes:?}"
    );
}
