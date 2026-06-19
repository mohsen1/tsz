//! `this` inside an object-literal method/accessor/`function`-property must be
//! typed as the *complete* object literal, independent of member declaration
//! order.
//!
//! tsz builds the object-literal type incrementally, so a member declared
//! *after* a method was invisible to that method's synthetic `this`:
//!
//! ```ts
//! const obj = { method() { return this.value; }, value: 42 };
//! //                            ^^^^^^^^^^ TS2339 + TS7023 (tsz, before fix)
//! ```
//!
//! `tsc` resolves `this.value` to `number` because `this` is the whole object.
//! The fix prescans the non-method members declared after the first
//! `this`-capturing callable and splices them into the synthetic `this`. The
//! prescan is side-effect-free, so it never perturbs the authoritative
//! diagnostics emitted when each member is actually checked.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    let mut c: Vec<u32> = get_diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    c.sort_unstable();
    c
}

/// The reported witness: a method reading a data property declared after it.
#[test]
fn method_reads_later_data_property_no_spurious_error() {
    let source = r#"
const obj = { method() { return this.value; }, value: 42 };
const n: number = obj.method();
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "method reading a later data property must not emit TS2339/TS7023"
    );
}

/// Same shape, but the binder names are different to prove the fix is structural
/// (no name-based fast path).
#[test]
fn method_reads_later_data_property_renamed_binders() {
    let source = r#"
const widget = { render() { return this.label + this.count; }, label: "x", count: 3 };
const text: string = widget.render();
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

/// The precise (widened) type of the later property must still flow through, so
/// a genuine mismatch in the method body is still reported.
#[test]
fn method_reads_later_data_property_keeps_precise_type() {
    // `this.value` is `number`; assigning it to `string` must still be TS2322.
    let source = r#"
const obj = { method() { const s: string = this.value; return s; }, value: 42 };
"#;
    assert_eq!(
        codes(source),
        vec![2322u32],
        "the later property's precise type must still catch real mismatches"
    );
}

/// A genuinely-missing member is still TS2339 — the prescan only adds declared
/// members, never invents them.
#[test]
fn method_reads_genuinely_missing_property_still_errors() {
    let source = r#"
const obj = { method() { return this.typo; }, value: 42 };
"#;
    let c = codes(source);
    assert!(
        c.contains(&2339u32),
        "reading an undeclared member must still emit TS2339, got: {c:?}"
    );
}

/// Writes through `this` to a later property must type-check against it.
#[test]
fn method_writes_later_data_property() {
    let source = r#"
const counter = { increment() { this.value++; }, reset() { this.value = 0; }, value: 0 };
counter.increment();
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

/// A `function`-expression property (which binds the object as `this`, unlike an
/// arrow) can read a later data property.
#[test]
fn function_property_reads_later_data_property() {
    let source = r#"
const handler = { run: function () { return this.payload; }, payload: { id: 1 } };
const id: number = handler.run().id;
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

/// A getter can read a later data property.
#[test]
fn getter_reads_later_data_property() {
    let source = r#"
const point = { get magnitude() { return this.x + this.y; }, x: 3, y: 4 };
const m: number = point.magnitude;
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

/// `return this` exposes later members on the returned receiver type.
#[test]
fn method_returns_this_exposes_later_members() {
    let source = r#"
const builder = { self() { return this; }, name: "b" };
const name: string = builder.self().name;
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

/// The pre-existing "earlier member" ordering must remain correct.
#[test]
fn method_reads_earlier_data_property_unchanged() {
    let source = r#"
const obj = { value: 42, method() { return this.value; } };
const n: number = obj.method();
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

/// Nested access through a later member resolves end-to-end.
#[test]
fn method_reads_later_nested_member() {
    let source = r#"
const config = { read() { return this.nested.deep; }, nested: { deep: 7 } };
const n: number = config.read();
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}
