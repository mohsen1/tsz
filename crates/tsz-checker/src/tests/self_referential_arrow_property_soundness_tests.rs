//! Regression coverage for issue #14784: a generic class's arrow-function
//! *property* whose return-type annotation references the enclosing class
//! must not silently resolve to `any`.
//!
//! Structural rule: when the type of a class member is computed by typing its
//! initializer (an arrow/function expression), resolving a return annotation
//! that references the enclosing class would re-enter the class's own
//! instance-type build while that member node is still in flight on the
//! resolution stack. tsz's `get_type_of_node` cycle guard returns a transient
//! `ERROR` placeholder for the in-flight node, which the re-entrant build then
//! baked into the cached instance type — so the property degraded to
//! `ERROR`/`any` and the call result was accepted as assignable to *any* other
//! instantiation (a soundness false-negative for both TS2322 and TS2345).
//!
//! Owner layer: checker class-instance construction (`class_type::entry`). The
//! fix defers a *fresh* instance-type build that is re-entered from within one
//! of the class's own member nodes to a deferred self-reference (an
//! already-built valid instance when available, else a lazy reference) without
//! caching it, mirroring `tsc`, which represents `C` inside `C`'s own member
//! signatures as a deferred reference rather than its resolved members.
//!
//! The method form (`m(): C`) already resolved correctly because method
//! signatures are resolved lazily on demand; the bug was specific to the
//! eagerly-typed arrow/function *property* form. The cases below vary the
//! binder names and shapes so the coverage is structural, not repro-scoped.

use crate::test_utils::check_source_strict_codes;

fn has_code(src: &str, code: u32) -> bool {
    check_source_strict_codes(src).contains(&code)
}

// --- Soundness: the self-referential arrow property must be checked ----------

#[test]
fn arrow_property_self_ref_conditional_return_flags_ts2322() {
    // The canonical witness from #14784. `b.m(...)` yields `Box<string>`; the
    // result must NOT be silently assignable to `Box<number>` / `string`.
    let src = r#"
class Box<T> {
  m = <U>(f: (t: T) => U): U extends Promise<any> ? Box<U> : Box<U> => { return null as any; };
}
declare const b: Box<number>;
const r = b.m(x => x.toString());
const bad1: Box<number> = r;
const bad2: string = r;
"#;
    assert!(
        has_code(src, 2322),
        "expected TS2322 on the unsound assignment of the arrow-property result, got {:?}",
        check_source_strict_codes(src)
    );
}

#[test]
fn arrow_property_self_ref_nongeneric_return_flags_ts2322() {
    // Non-generic arrow property: trigger is purely the self-referential return
    // annotation, independent of any method type parameter.
    let src = r#"
class Crate<T> {
  pack = (n: number): Crate<number> => { return null as any; };
}
declare const c: Crate<number>;
const r = c.pack(1);
const bad: 1 = r;
"#;
    assert!(
        has_code(src, 2322),
        "expected TS2322; the property must not degrade to any, got {:?}",
        check_source_strict_codes(src)
    );
}

#[test]
fn arrow_property_self_ref_argument_position_flags_ts2345() {
    // Same false-negative reproduces in argument position (TS2345).
    let src = r#"
class Holder<T> {
  wrap = <U>(f: (t: T) => U): Holder<U> => { return null as any; };
}
declare const h: Holder<number>;
declare function want(x: Holder<number>): void;
const r = h.wrap(x => x.toString());
want(r);
"#;
    assert!(
        has_code(src, 2345),
        "expected TS2345 on the unsound argument, got {:?}",
        check_source_strict_codes(src)
    );
}

#[test]
fn arrow_property_self_ref_result_is_not_any() {
    // A property read on the (correctly typed) result must error — proving the
    // result is a real `Box<number>`, not `any`/`ERROR`.
    let src = r#"
class Vault<T> {
  open = (): Vault<number> => { return null as any; };
}
declare const v: Vault<number>;
const r = v.open();
r.definitelyMissing;
"#;
    assert!(
        has_code(src, 2339),
        "expected TS2339 (result is a real Vault<number>, not any), got {:?}",
        check_source_strict_codes(src)
    );
}

// --- Adjacent: the fix must not regress sound cases --------------------------

#[test]
fn method_form_self_ref_still_flags_ts2322() {
    // The method form already worked; keep it covered so the deferral does not
    // regress it.
    let src = r#"
class Box<T> {
  m<U>(f: (t: T) => U): Box<U> { return null as any; }
}
declare const b: Box<number>;
const r = b.m(x => x.toString());
const bad: Box<number> = r;
"#;
    assert!(
        has_code(src, 2322),
        "expected TS2322 for the method form, got {:?}",
        check_source_strict_codes(src)
    );
}

#[test]
fn arrow_property_returning_unrelated_class_still_flags_ts2322() {
    // Return type references a *different* class: this path was already sound;
    // ensure it stays sound (control case).
    let src = r#"
class Other<T> { o: T = null as any; }
class Box<T> {
  m = <U>(f: (t: T) => U): Other<U> => { return null as any; };
}
declare const b: Box<number>;
const r = b.m(x => x.toString());
const bad: Other<number> = r;
"#;
    assert!(
        has_code(src, 2322),
        "expected TS2322 for the unrelated-class return, got {:?}",
        check_source_strict_codes(src)
    );
}

#[test]
fn fluent_builder_arrow_properties_typecheck() {
    // Real-world fluent/builder shape: arrow properties returning the enclosing
    // generic class must thread the type parameter through the chain.
    let src = r#"
class Query<T> {
  where = (cond: string): Query<T> => this;
  select = <U>(proj: (t: T) => U): Query<U> => null as any;
}
declare const q: Query<number>;
const q2 = q.where("x").select(n => n.toFixed());
const bad: Query<number> = q2;
"#;
    assert!(
        has_code(src, 2322),
        "expected TS2322: Query<string> is not assignable to Query<number>, got {:?}",
        check_source_strict_codes(src)
    );
}

#[test]
fn self_referential_arrow_property_does_not_emit_circularity() {
    // The self-reference is benign (resolvable); it must not be misreported as
    // a circular-type/implicit-any diagnostic (TS7022/7023/7024 or TS2577).
    let src = r#"
class Box<T> {
  m = (n: number): Box<number> => { return null as any; };
}
declare const b: Box<number>;
const r = b.m(1);
"#;
    let codes = check_source_strict_codes(src);
    assert!(
        !codes.iter().any(|&c| matches!(c, 7022..=7024 | 2577)),
        "self-referential arrow property must not emit a circularity diagnostic, got {codes:?}"
    );
}
