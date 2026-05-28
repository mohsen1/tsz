//! Property access on the result of a self-referential generic call.
//!
//! A generic value function whose body returns an object that recursively
//! calls the same function (`const f = <T>(p: T) => ({ deeper: <U>(c: U) =>
//! f<T & U>(...) })`) has its inner call typed, during return-type inference,
//! as a deferred placeholder `Application(Lazy(f), [T & U])` — `f` being the
//! value symbol, the recursion target. When such a call result is later used
//! (property access, assignment), the placeholder must resolve to the call
//! signature's RETURN type instantiated with the type arguments — the object
//! the call returns — not to the instantiated function value. `tsc` resolves
//! the recursive object lazily (bottoming out to `any` only at its display
//! depth limit), so member access at any finite depth succeeds.
//!
//! Before the fix the placeholder evaluated to the instantiated function, so
//! `f(...).deeper(...).result` reported a spurious TS2339. These tests pin the
//! parity and cover renamed type parameters, the function-declaration form,
//! deeper nesting, the non-recursive control, and the negative case where a
//! genuinely-absent property must still report TS2339.

use tsz_checker::context::CheckerOptions;

fn diagnostics_for(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
}

fn ts2339_messages(source: &str) -> Vec<String> {
    diagnostics_for(source)
        .into_iter()
        .filter(|d| d.code == 2339)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn recursive_generic_call_return_resolves_to_object() {
    let ts2339 = ts2339_messages(
        r#"
const f = <T extends object>(parent: T) => {
    return {
        result: parent,
        deeper: <U extends object>(child: U) => f<T & U>({ ...parent, ...child })
    };
};
let p1 = f({ one: '1' });
let p2 = p1.deeper({ two: '2' });
const a = p2.result;
const b = p2.deeper;
"#,
    );
    assert!(
        ts2339.is_empty(),
        "recursive generic call result should resolve to its return object, \
         so property access must not report TS2339; got: {ts2339:?}"
    );
}

#[test]
fn recursive_generic_call_return_renamed_type_params() {
    // Renaming the bound type parameters (`T`->`A`, `U`->`B`) must not change
    // the behavior: the fix keys on structure, never on identifier spelling.
    let ts2339 = ts2339_messages(
        r#"
const g = <A extends object>(parent: A) => {
    return {
        result: parent,
        deeper: <B extends object>(child: B) => g<A & B>({ ...parent, ...child })
    };
};
let q1 = g({ one: '1' });
let q2 = q1.deeper({ two: '2' });
const a = q2.result;
const b = q2.deeper;
"#,
    );
    assert!(
        ts2339.is_empty(),
        "renamed type parameters must behave identically; got: {ts2339:?}"
    );
}

#[test]
fn recursive_generic_call_return_function_declaration() {
    // The same recursion through a `function` declaration (not a const arrow)
    // must also resolve to the returned object.
    let ts2339 = ts2339_messages(
        r#"
function rec<T extends object>(parent: T) {
    return {
        value: parent,
        next: <U extends object>(child: U) => rec<T & U>({ ...parent, ...child })
    };
}
let r1 = rec({ a: 1 });
let r2 = r1.next({ b: 2 });
const v = r2.value;
const n = r2.next;
"#,
    );
    assert!(
        ts2339.is_empty(),
        "function-declaration recursion should resolve to its return object; got: {ts2339:?}"
    );
}

#[test]
fn recursive_generic_call_return_deeper_nesting() {
    // Accessing members two levels into the recursion must still succeed.
    let ts2339 = ts2339_messages(
        r#"
const f = <T extends object>(parent: T) => {
    return {
        result: parent,
        deeper: <U extends object>(child: U) => f<T & U>({ ...parent, ...child })
    };
};
let p1 = f({ one: '1' });
let p2 = p1.deeper({ two: '2' });
let p3 = p2.deeper({ three: '3' });
const a = p3.result;
const b = p3.deeper;
"#,
    );
    assert!(
        ts2339.is_empty(),
        "deeper recursion levels must still resolve to objects; got: {ts2339:?}"
    );
}

#[test]
fn non_recursive_generic_call_unaffected() {
    // A non-recursive generic value function never produces the placeholder;
    // its call result is the ordinary object and stays accessible.
    let ts2339 = ts2339_messages(
        r#"
const id = <T>(x: T) => ({ wrapped: x });
let a = id<number>(5);
const w = a.wrapped;
"#,
    );
    assert!(
        ts2339.is_empty(),
        "non-recursive generic calls must be unaffected; got: {ts2339:?}"
    );
}

#[test]
fn recursive_generic_call_return_missing_property_still_errors() {
    // The resolved return object exposes exactly `result` and `deeper`; a
    // genuinely-absent property must still report TS2339 (the fix resolves the
    // placeholder, it does not blanket-suppress property errors).
    let ts2339 = ts2339_messages(
        r#"
const h = <T extends object>(parent: T) => {
    return {
        result: parent,
        deeper: <U extends object>(child: U) => h<T & U>({ ...parent, ...child })
    };
};
let n1 = h({ one: '1' });
let n2 = n1.deeper({ two: '2' });
const bad = n2.nonexistent;
"#,
    );
    assert_eq!(
        ts2339.len(),
        1,
        "a genuinely-missing property on the resolved object must still report TS2339; got: {ts2339:?}"
    );
    assert!(
        ts2339[0].contains("nonexistent"),
        "the TS2339 should name the missing property; got: {ts2339:?}"
    );
}
