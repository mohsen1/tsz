//! Tests for JS static-block `this.prop = value` property inference.
//!
//! In JS/checkJs mode, `this.prop = value` inside a class `static { }` block
//! serves as an implicit *static* property declaration, mirroring the
//! existing constructor-body `this.prop = value` handling for *instance*
//! members. See `js_constructor_property_tests.rs` for the instance-side
//! coverage this mirrors.

use tsz_checker::context::CheckerOptions;

fn check_js(source: &str) -> Vec<(u32, String)> {
    check_with_options_and_file(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn check_ts(source: &str) -> Vec<(u32, String)> {
    check_with_options_and_file(source, "test.ts", CheckerOptions::default())
}

fn check_with_options_and_file(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser =
        tsz_parser::parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// Basic case from the conformance fixture: `this.prop = value` inside a
/// static block creates a static member, so a later `Class.prop` access
/// does not false-positive TS2339.
#[test]
fn static_block_this_prop_no_false_ts2339() {
    let source = r#"
class Thing {
    static {
        this.doSomething = () => {};
    }
}
Thing.doSomething();
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "expected static-block `this.prop=` to declare a static member, got: {diagnostics:?}"
    );
}

/// Renamed binder: the class name and property name must not matter.
#[test]
fn static_block_this_prop_renamed_binders_no_false_ts2339() {
    let source = r#"
class zzTop {
    static {
        this.frobnicate = 42;
    }
}
zzTop.frobnicate;
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "expected renamed static-block this-assignment to declare a static member, got: {diagnostics:?}"
    );
}

/// Multiple `this.prop = value` statements in one static block all become
/// static members.
#[test]
fn static_block_multiple_this_props_no_false_ts2339() {
    let source = r#"
class K {
    static {
        this.a = 1;
        this.b = "x";
    }
}
K.a;
K.b;
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "expected all static-block this-assignments to declare static members, got: {diagnostics:?}"
    );
}

/// Multiple static blocks in the same class each contribute members.
#[test]
fn static_block_two_blocks_no_false_ts2339() {
    let source = r#"
class K {
    static {
        this.a = 1;
    }
    static {
        this.b = 2;
    }
}
K.a;
K.b;
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "expected members from every static block to be visible, got: {diagnostics:?}"
    );
}

/// A subclass extending a lib/user base still gets its own static-block
/// implicit members (checks the fix does not depend on a bare base class).
#[test]
fn static_block_this_prop_extends_no_false_ts2339() {
    let source = r#"
class Base {}
class ElementsArray extends Base {
    static {
        this.isArray = (arg) => false;
    }
}
ElementsArray.isArray(1);
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "expected static-block this-assignment on a derived class to declare a static member, got: {diagnostics:?}"
    );
}

/// An already-declared static property is not overridden by a static-block
/// assignment of the same name — the explicit declaration still wins and no
/// spurious diagnostic is introduced either way.
#[test]
fn static_block_this_prop_does_not_override_explicit_declaration() {
    let source = r#"
class K {
    static p = "declared";
    static {
        this.p = "assigned";
    }
}
K.p;
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "expected explicit static declaration + static-block assignment of the same name to coexist without TS2339, got: {diagnostics:?}"
    );
}

/// Negative control: a member that is genuinely never assigned anywhere
/// must still report TS2339 — the fix must not become a blanket suppression.
#[test]
fn static_block_this_prop_genuinely_missing_member_still_reports_ts2339() {
    let source = r#"
class K {
    static {
        this.a = 1;
    }
}
K.doesNotExist;
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 2339),
        1,
        "expected a genuinely absent static member to still report TS2339, got: {diagnostics:?}"
    );
}

/// Negative control: the implicit-member-from-assignment JS special case
/// must not leak into TS files. A `.ts` class doing `this.prop = value` in
/// a static block with no declared member for `prop` still errors on both
/// the assignment and the later read.
#[test]
fn static_block_this_prop_ts_file_still_reports_ts2339() {
    let source = r#"
class K {
    static {
        this.a = 1;
    }
}
K.a;
"#;
    let diagnostics = check_ts(source);
    assert!(
        count_code(&diagnostics, 2339) >= 1,
        "expected TS files to keep reporting TS2339 for an undeclared static member \
         written via `this.prop=` in a static block (JS-only special case), got: {diagnostics:?}"
    );
}
