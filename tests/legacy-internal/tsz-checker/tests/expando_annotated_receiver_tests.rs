//! Expando property assignment onto an explicitly annotated receiver.
//!
//! `tsc` accepts `fn.prop = value` as an expando *declaration* only when the
//! receiver's type is inferred from a function declaration or function-valued
//! initializer. An explicit type annotation makes the declared type
//! authoritative, so the assignment is an ordinary property write and reports
//! TS2339 when the property is absent — verified against the pinned tsc 7.0.2:
//!
//! ```text
//! const a1 = () => {};            a1.f = 1;   // accepted (inferred)
//! const a2: () => void = () => {}; a2.f = 1;  // TS2339
//! declare const a3: () => void;    a3.f = 1;  // TS2339
//! ```
//!
//! tsz previously ignored the annotation whenever an initializer was present,
//! so `a2.f = 1` silently created a property on `() => void`.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn ts_codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn js_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// --- Annotated receivers: the declared type wins, so TS2339. ---

#[test]
fn annotated_function_typed_const_rejects_expando_write() {
    let source = "const a2: () => void = () => {};\na2.f = 1;\n";
    assert!(ts_codes(source).contains(&2339));
}

/// Same rule with a different binder name and a different function type, so the
/// behaviour is structural rather than tied to a spelling.
#[test]
fn annotated_function_typed_const_rejects_expando_write_renamed() {
    let source = "const handler: (n: number) => string = n => String(n);\nhandler.cache = 1;\n";
    assert!(ts_codes(source).contains(&2339));
}

#[test]
fn annotated_declare_without_initializer_still_rejects() {
    let source = "declare const a3: () => void;\na3.f = 1;\n";
    assert!(ts_codes(source).contains(&2339));
}

/// A function expression initializer does not rescue an annotated receiver.
#[test]
fn annotated_receiver_with_function_expression_initializer_rejects() {
    let source = "const a4: () => void = function () {};\na4.f = 1;\n";
    assert!(ts_codes(source).contains(&2339));
}

// --- Inferred receivers: the expando pattern still works. ---

#[test]
fn inferred_arrow_still_accepts_expando_write() {
    let source = "const a1 = () => {};\na1.f = 1;\na1.f;\n";
    assert!(!ts_codes(source).contains(&2339));
}

#[test]
fn function_declaration_still_accepts_expando_write() {
    let source = "function C() {}\nC.f = 1;\nC.f;\n";
    assert!(!ts_codes(source).contains(&2339));
}

#[test]
fn function_expression_initializer_still_accepts_expando_write() {
    let source = "const h = function () {};\nh.f = 1;\nh.f;\n";
    assert!(!ts_codes(source).contains(&2339));
}

/// The JS expando pattern is unaffected: a JS function declaration still takes
/// properties, which is what `checkJs` sources rely on.
#[test]
fn js_function_declaration_still_accepts_expando_write() {
    let source = "function C() {}\nC.f = 1;\nC.f;\n";
    assert!(!js_codes(source).contains(&2339));
}

/// An annotated receiver whose declared type *does* have the property stays
/// silent — the guard rejects unknown properties, not every annotated write.
#[test]
fn annotated_receiver_with_declared_property_is_accepted() {
    let source = concat!(
        "interface Callable { (): void; f: number }\n",
        "declare const c: Callable;\n",
        "c.f = 1;\n",
    );
    assert!(!ts_codes(source).contains(&2339));
}

/// An annotated object receiver was already checked; keep it that way.
#[test]
fn annotated_object_receiver_still_rejects_unknown_property() {
    let source = "const p: { a: number } = { a: 1 };\np.b = 1;\n";
    assert!(ts_codes(source).contains(&2339));
}
