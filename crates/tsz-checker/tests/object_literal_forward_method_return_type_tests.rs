//! `this.<sibling>()` inside an object-literal method/`function`-property must
//! carry the *sibling's inferred return type*, even when the sibling is an
//! unannotated method declared **later** in the same literal.
//!
//! tsz builds the object-literal synthetic `this` incrementally, so a sibling
//! declared after the current method was spliced in with a hardcoded `any`
//! return type. Any diagnostic depending on the call's result (TS2322/TS2345/…)
//! was therefore silently dropped, and declaration order flipped the outcome:
//!
//! ```ts
//! const obj = { foo() { return this.bar(); }, bar() { return 1; } };
//! const t: string = obj.foo(); // tsc: TS2322; tsz (before fix): no error
//! ```
//!
//! The fix infers an acyclic unannotated sibling's real return type on demand
//! (memoized per object literal), while keeping `any` for a sibling that is part
//! of a genuine circular-return cycle so the TS7023 diagnostic still fires.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    let mut c: Vec<u32> = get_diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    c.sort_unstable();
    c
}

/// The reported witness: `foo` calls a *later* unannotated method `bar`; the
/// `number` result must flow so the `string` assignment is TS2322.
#[test]
fn method_calls_later_method_return_type_flows() {
    let source = r#"
const obj = { foo() { return this.bar(); }, bar() { return 1; } };
const t: string = obj.foo();
"#;
    assert_eq!(
        codes(source),
        vec![2322u32],
        "a later sibling method's inferred return type must reach the call site"
    );
}

/// Control: sibling declared *before* the caller already worked; it must keep
/// working (same diagnostic, order-independent).
#[test]
fn method_calls_earlier_method_return_type_flows() {
    let source = r#"
const obj = { bar() { return 1; }, foo() { return this.bar(); } };
const t: string = obj.foo();
"#;
    assert_eq!(codes(source), vec![2322u32]);
}

/// Structural, not name-based: renamed binders produce the identical result.
#[test]
fn forward_method_return_type_renamed_binders() {
    let source = r#"
const widget = { alpha() { return this.beta(); }, beta() { return "hi"; } };
const t: number = widget.alpha();
"#;
    assert_eq!(
        codes(source),
        vec![2322u32],
        "the method names must not matter (no name-based fast path)"
    );
}

/// A `function`-expression property (which binds the object as `this`) calling a
/// later method resolves the same way.
#[test]
fn function_property_calls_later_method_return_type_flows() {
    let source = r#"
const obj = { foo: function () { return this.bar(); }, bar() { return 2; } };
const t: string = obj.foo();
"#;
    assert_eq!(codes(source), vec![2322u32]);
}

/// A method calling a later `function`-expression-property sibling also resolves
/// the sibling's inferred return type.
#[test]
fn method_calls_later_function_property_return_type_flows() {
    let source = r#"
const obj = { foo() { return this.bar(); }, bar: function () { return 3; } };
const t: string = obj.foo();
"#;
    assert_eq!(codes(source), vec![2322u32]);
}

/// The result is the concrete inferred type (`number`), not `any`: assigning it
/// to an unrelated object type must be rejected.
#[test]
fn forward_method_return_type_is_concrete_not_any() {
    let source = r#"
const obj = { foo() { return this.bar(); }, bar() { return 1; } };
const t: { z: number } = obj.foo();
"#;
    assert_eq!(
        codes(source),
        vec![2322u32],
        "obj.foo() must be `number`, not `any` — the object-type assignment errors"
    );
}

/// A genuine circular return (`foo -> bar -> foo`) must still yield TS7023 on
/// both members and must not loop; the acyclic-inference path must not swallow
/// it.
#[test]
fn mutual_circular_return_still_ts7023() {
    let source = r#"
const obj = { foo() { return this.bar(); }, bar() { return this.foo(); } };
const t = obj.foo();
"#;
    assert_eq!(
        codes(source),
        vec![7023u32, 7023u32],
        "a real cycle must keep both TS7023 diagnostics"
    );
}

/// A sibling whose real return type is itself `any` stays `any`: no new error
/// when its result is assigned to an unrelated annotation.
#[test]
fn sibling_returning_any_stays_any() {
    let source = r#"
const obj = { foo() { const s: string = this.bar(); return s; }, bar(): any { return 1; } };
obj;
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "an `any`-returning sibling must not introduce a spurious mismatch"
    );
}

/// An explicitly annotated later sibling uses its annotation (regression guard
/// for the annotation path).
#[test]
fn annotated_later_sibling_uses_annotation() {
    let source = r#"
const obj = { foo() { return this.bar(); }, bar(): number { return 1; } };
const t: string = obj.foo();
"#;
    assert_eq!(codes(source), vec![2322u32]);
}
