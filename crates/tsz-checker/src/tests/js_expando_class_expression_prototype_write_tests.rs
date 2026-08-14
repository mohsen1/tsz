//! A class-expression host (`const C = class {}`) is a class root: its
//! `prototype` is the closed instance shape, exactly like a class
//! declaration's. So a `C.prototype.member = e` write for a member the class
//! never declares must stay `TS2339` (or `TS2551` when the class declares a
//! near-miss name), NOT be silently accepted as a new expando member. This is
//! the class-*expression*-variable form of the family fixed for class
//! *declarations* in #17493/#17496 (#17495): the declaration form was already
//! `TS2339`, but the `const C = class {}` / `var C = class {}` spelling slipped
//! through the binder's variable-root expando path, which treated a class
//! expression as a permissive expando host for its whole chain — including
//! `prototype`.
//!
//! Structural rule (oracle-pinned against typescript@7.0.2 — the pinned
//! conformance oracle — `--checkJs --allowJs --noImplicitAny`):
//!
//! > For a variable initialized with a **class** expression, a
//! > `C.prototype.member = e` write of an undeclared member is `TS2339`
//! > (`TS2551` with a near-miss), while a **static** write `C.member = e` is an
//! > accepted expando. For a variable initialized with a **function/arrow**
//! > expression, `C.prototype.member = e` stays the permissive ES5 constructor
//! > idiom and is accepted.
//!
//! The distinguishing operation is a binder record-time gate
//! (`expression_flow.rs`): a class-expression variable's `prototype` chain is
//! not recorded as an expando, mirroring the `is_class_prototype_chain` gate
//! the class-declaration/function root branch already applies.

use crate::CheckerOptions;
use crate::test_utils::check_source;

fn js_check_js_opts() -> CheckerOptions {
    CheckerOptions {
        no_implicit_any: true,
        check_js: true,
        allow_js: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    check_source(source, "a.js", js_check_js_opts())
        .iter()
        .map(|d| d.code)
        .collect()
}

fn code_messages(source: &str) -> Vec<(u32, String)> {
    check_source(source, "a.js", js_check_js_opts())
        .iter()
        .map(|d| (d.code, d.message_text.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// Negative direction: class-expression host prototype writes stay TS2339/2551.
// ---------------------------------------------------------------------------

/// `const C = class { method1 } ; C.prototype.method2 = e` — a near-miss
/// member write on a class-expression host's prototype stays TS2551 with the
/// `method1` suggestion, matching the class-declaration spelling.
#[test]
fn const_class_expression_prototype_near_miss_write_stays_ts2551() {
    let msgs = code_messages(
        "const C = class {\n  method1() {}\n};\nC.prototype.method2 = function () {};\n",
    );
    let ts2551 = msgs.iter().find(|(code, _)| *code == 2551);
    assert!(
        ts2551.is_some(),
        "a class-expression host's `.prototype` near-miss write must stay TS2551; got {msgs:?}"
    );
    let (_, message) = ts2551.unwrap();
    assert!(
        message.contains("method2") && message.contains("method1"),
        "TS2551 must name `method2` and suggest `method1`; got {message:?}"
    );
}

/// `const C = class {}; C.prototype.x = 5` — an absent member write with no
/// near-miss stays TS2339.
#[test]
fn const_class_expression_prototype_absent_write_stays_ts2339() {
    assert_eq!(
        codes("const C = class {};\nC.prototype.x = 5;\n"),
        vec![2339],
        "an undeclared `.prototype` member write on a class-expression host must stay TS2339"
    );
}

/// `var C = class {}; C.prototype.x = 5` — the `var` spelling behaves the same
/// as `const`; the gate is on the class-expression initializer, not the
/// binding keyword.
#[test]
fn var_class_expression_prototype_absent_write_stays_ts2339() {
    assert_eq!(
        codes("var C = class {};\nC.prototype.x = 5;\n"),
        vec![2339],
        "a `var`-bound class-expression host's `.prototype` write must stay TS2339"
    );
}

/// Renamed binders: the rule is structural (class-expression initializer),
/// not keyed on `C` / `x` / `method2`.
#[test]
fn class_expression_prototype_absent_write_stays_ts2339_renamed_binders() {
    assert_eq!(
        codes("const Widget = class {};\nWidget.prototype.render = function () {};\n"),
        vec![2339],
        "renamed-binder class-expression host must also keep TS2339 on the prototype write"
    );
}

/// A named class expression bound to a variable is still a class root.
#[test]
fn named_class_expression_prototype_absent_write_stays_ts2339() {
    assert_eq!(
        codes("const C = class Inner {};\nC.prototype.x = 5;\n"),
        vec![2339],
        "a named class expression is still a class root; the prototype write must stay TS2339"
    );
}

/// Two undeclared prototype writes each report independently.
#[test]
fn class_expression_prototype_multiple_absent_writes_each_ts2339() {
    assert_eq!(
        codes(
            "const C = class {};\nC.prototype.identifier = undefined;\nC.prototype.size = null;\n"
        ),
        vec![2339, 2339],
        "each undeclared class-expression prototype write must report TS2339"
    );
}

// ---------------------------------------------------------------------------
// Positive direction: what must still be accepted (no over-blocking).
// ---------------------------------------------------------------------------

/// Static (non-prototype) expando writes on a class-expression host are still
/// accepted — only the `prototype` chain is closed.
#[test]
fn class_expression_static_expando_write_is_clean() {
    assert_eq!(
        codes("const C = class {\n  method1() {}\n};\nC.staticProp = function () {};\n"),
        Vec::<u32>::new(),
        "a static expando write on a class-expression host must stay clean"
    );
}

/// A function-expression host keeps the permissive ES5 constructor prototype
/// idiom: `const C = function () {}; C.prototype.m = …` is a genuine expando.
#[test]
fn function_expression_host_prototype_write_is_clean() {
    assert_eq!(
        codes("const C = function () {};\nC.prototype.method2 = function () {};\n"),
        Vec::<u32>::new(),
        "a function-expression host's prototype write must stay clean (ES5 idiom)"
    );
}

/// An arrow-expression host is also permissive here (oracle: clean).
#[test]
fn arrow_expression_host_prototype_write_is_clean() {
    assert_eq!(
        codes("const C = () => {};\nC.prototype.x = 5;\n"),
        Vec::<u32>::new(),
        "an arrow-expression host's prototype write must stay clean"
    );
}

/// Control: a `function` declaration keeps the permissive prototype idiom, and
/// a `class` declaration stays TS2339 — the class-expression fix leaves both
/// unchanged.
#[test]
fn declaration_forms_unchanged_by_class_expression_gate() {
    assert_eq!(
        codes("function C() {}\nC.prototype.x = 5;\n"),
        Vec::<u32>::new(),
        "function-declaration prototype write stays clean"
    );
    assert_eq!(
        codes("class C {}\nC.prototype.x = 5;\n"),
        vec![2339],
        "class-declaration prototype write stays TS2339"
    );
}
