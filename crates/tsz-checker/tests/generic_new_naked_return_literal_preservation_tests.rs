//! Regression tests for a `new`-expression literal-widening bug.
//!
//! `tsc` decides whether a generic construct signature's scalar literal
//! arguments feed inference as their literal type or their widened primitive
//! based on whether the type parameter occurs *naked* (unwrapped by any
//! application) at the top level of the signature's return type:
//!
//!   interface Ctor { new <T>(a: T, b: T): T; }     // T naked in return
//!   declare var Ctor: Ctor;
//!   new Ctor("", 0);
//!   // tsc: T is pinned to the literal `""` from the first argument, so the
//!   // second argument reports `Argument of type '0' is not assignable to
//!   // parameter of type '"\""'.` — the *literal*, not `string`.
//!
//! vs.
//!
//!   class Ctor2<T> { constructor(a: T, b: T) {} }  // T only reachable
//!   new Ctor2("", 0);                               // through `Ctor2<T>`
//!   // tsc: `Argument of type 'number' is not assignable to parameter of
//!   // type 'string'.` — widened, because the construct signature's
//!   // (implicit) return type is the class instance type `Ctor2<T>`, not a
//!   // naked `T`.
//!
//! tsz's `is_generic_new` post-argument-collection widening
//! (`complex.rs`, `get_type_of_new_expression_with_request`) previously
//! widened every scalar literal argument unconditionally (gated only by
//! whether the *parameter's* type had a primitive-literal-preserving
//! constraint, e.g. `T extends string`), so it always took the `Ctor2`-style
//! widened branch — even for `new Ctor("", 0)`, which should match the plain
//! function-call behavior of `declare function f<T>(a: T, b: T): T; f("", 0)`.
//!
//! The fix (`generic_new_inference.rs`,
//! `generic_new_param_preserves_literal`) additionally preserves the literal
//! when the type parameter is naked at the top level of the construct
//! signature's return type, reusing the same
//! `tsz_solver::visitor::is_type_parameter_at_top_level` query the solver's
//! own inference finalize step uses for its narrower (conditional-reducing)
//! literal-preservation stopgap.
//!
//! Every case below is oracle-verified against a pinned `tsc` (`/opt/node22/bin/tsc`,
//! 6.0.2): TS2345's parameter-type text in the message directly reveals
//! whether the argument was widened (`'string'`/`'boolean'`) or the literal
//! survived (`'"\""'`/`'true'`).

use tsz_checker::test_utils::check_source_code_messages;

fn messages(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

fn ts2345_message(source: &str) -> String {
    let msgs = messages(source);
    let hit = msgs
        .iter()
        .find(|(code, _)| *code == 2345)
        .unwrap_or_else(|| panic!("expected a TS2345 diagnostic, got {msgs:?}"));
    hit.1.clone()
}

// ── Naked-top-level-return: literal is preserved (matches a plain call) ──

#[test]
fn interface_construct_signature_naked_return_preserves_literal() {
    let source = r#"
interface Ctor { new <T>(a: T, b: T): T; }
declare var Ctor: Ctor;
new Ctor("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type '\"\"'"));
}

#[test]
fn function_typed_variable_new_signature_naked_return_preserves_literal() {
    let source = r#"
declare var Ctor: new <T>(a: T, b: T) => T;
new Ctor("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type '\"\"'"));
}

#[test]
fn object_type_literal_new_signature_naked_return_preserves_literal() {
    let source = r#"
declare const Ctor: { new <T>(a: T, b: T): T };
new Ctor("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type '\"\"'"));
}

#[test]
fn naked_return_union_with_undefined_preserves_literal() {
    let source = r#"
interface Ctor { new <T>(a: T, b: T): T | undefined; }
declare var Ctor: Ctor;
new Ctor("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type '\"\"'"));
}

#[test]
fn renamed_binder_naked_return_preserves_literal() {
    // Same shape as the core case, with the type parameter and the
    // constructor's own binder renamed — the rule must key on structural
    // position, not on any specific identifier.
    let source = r#"
interface Maker { new <U>(first: U, second: U): U; }
declare var Maker: Maker;
new Maker("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type '\"\"'"));
}

#[test]
fn naked_return_preserves_boolean_literal() {
    let source = r#"
interface Ctor { new <T>(a: T, b: T): T; }
declare var Ctor: Ctor;
new Ctor(true, 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type 'true'"));
}

// ── Wrapped/nominal return: literal still widens (unchanged, class-like) ──

#[test]
fn real_class_constructor_widens_literal() {
    let source = r#"
class Ctor2<T> { constructor(a: T, b: T) {} }
new Ctor2("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type 'string'"));
}

#[test]
fn ambient_class_constructor_widens_literal() {
    let source = r#"
declare class Ctor3<T> { constructor(a: T, b: T); }
new Ctor3("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type 'string'"));
}

#[test]
fn class_expression_constructor_widens_literal() {
    let source = r#"
const Ctor6 = class Inner<T> { constructor(a: T, b: T) {} };
new Ctor6("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type 'string'"));
}

#[test]
fn derived_class_constructor_widens_literal() {
    let source = r#"
abstract class Base<T> { constructor(a: T, b: T) {} }
class Derived<T> extends Base<T> {}
new Derived("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type 'string'"));
}

#[test]
fn interface_construct_signature_structural_wrapped_return_widens_literal() {
    // Same params as the naked-return case, but the return type wraps `T`
    // inside `Box<T>` instead of returning it bare — not an identity
    // signature, so tsc's normal widened-inference result applies.
    let source = r#"
interface Box<T> { val: T }
interface CtorX { new <T>(a: T, b: T): Box<T>; }
declare var CtorX: CtorX;
new CtorX("", 0);
"#;
    assert!(ts2345_message(source).contains("parameter of type 'string'"));
}

// ── Positive control: parameter-constraint-based preservation is untouched ──

#[test]
fn primitive_constrained_type_param_with_wrapped_return_still_preserves_literal() {
    // The return type wraps `T` in `Box<T>` (not naked), so this exercises
    // only the pre-existing, unmodified constraint-based branch of
    // `generic_new_param_preserves_literal` (`T extends string`), not the new
    // naked-top-level-return branch. If `T` were widened to `string` here,
    // `Box<string>` would not be assignable to the annotated `Box<"a">` and
    // this would report TS2322; tsc reports nothing.
    let source = r#"
interface Box<T> { val: T }
interface Ctor { new <T extends string>(a: T): Box<T>; }
declare var Ctor: Ctor;
const b = new Ctor("a");
const c: Box<"a"> = b;
"#;
    assert!(
        messages(source).is_empty(),
        "expected no diagnostics (T preserved as the literal \"a\"), got {:?}",
        messages(source)
    );
}
