//! Regression coverage for issue #13650 Face 2: a primitive union member must
//! survive union subtype reduction even when a sibling weak (all-optional)
//! object member structurally subsumes it.
//!
//! Rule: `tsc`'s `removeSubtypes` only drops a union member as a subtype when
//! that member is structured/instantiable, or when the union contains an empty
//! object type. A bare primitive keyword (`boolean`, `number`, `string`, …)
//! vacuously satisfies an all-optional object structurally, but `tsc` keeps it
//! in the union — so `boolean | { x?: T }` stays a two-member union and a
//! `boolean` argument is assignable to it. Previously tsz reduced the
//! instantiated parameter union `boolean | Opts` down to `Opts` (the primitive
//! was dropped), then rejected the `boolean`/`false` argument with a false
//! TS2345. The reduction only fired under a generic signature, because the
//! object member is a resolved structural type there rather than an opaque
//! `Lazy` reference.
//!
//! The behavior keys on type structure (primitive vs object, presence of an
//! all-optional sibling), never on identifier/property/type-parameter names, so
//! these tests vary all of those.

use tsz_checker::test_utils::check_source_code_messages;

fn ts2345_count(diags: &[(u32, String)]) -> usize {
    diags.iter().filter(|(code, _)| *code == 2345).count()
}

#[test]
fn boolean_arg_against_generic_boolean_or_weak_object_union_is_accepted() {
    // The exact `addEventListener`-shaped repro from the issue: a fresh `false`
    // against `boolean | Opts` under a generic signature.
    let diags = check_source_code_messages(
        r#"
interface Opts { capture?: boolean; once?: boolean }
declare function on<K extends string>(type: K, options?: boolean | Opts): void;
on("visibilitychange", false);
"#,
    );
    assert_eq!(
        ts2345_count(&diags),
        0,
        "a boolean is assignable to the `boolean` member of `boolean | Opts`: {diags:?}"
    );
}

#[test]
fn number_arg_against_generic_number_or_weak_object_union_is_accepted() {
    // The defect is not boolean-specific: any primitive paired with a weak
    // object member must survive the reduction.
    let diags = check_source_code_messages(
        r#"
interface Settings { retries?: number; cache?: boolean }
declare function run<T extends string>(name: T, settings?: number | Settings): void;
run("job", 5);
"#,
    );
    assert_eq!(
        ts2345_count(&diags),
        0,
        "a number is assignable to the `number` member of `number | Settings`: {diags:?}"
    );
}

#[test]
fn string_arg_against_generic_string_or_weak_object_union_is_accepted() {
    let diags = check_source_code_messages(
        r#"
interface Wibble { aaa?: string; bbb?: number }
declare function listen<X extends string>(channel: X, mode?: string | Wibble): void;
listen("evt", "fast");
"#,
    );
    assert_eq!(
        ts2345_count(&diags),
        0,
        "a string is assignable to the `string` member of `string | Wibble`: {diags:?}"
    );
}

#[test]
fn non_fresh_primitive_against_generic_primitive_or_weak_object_union_is_accepted() {
    // Freshness is not the trigger — an already-widened, non-fresh `boolean`
    // local must also be accepted under the generic signature.
    let diags = check_source_code_messages(
        r#"
interface Cfg { passive?: boolean; signal?: boolean }
declare function bind<E extends string>(event: E, cfg?: boolean | Cfg): void;
const flag: boolean = false;
bind("ready", flag);
bind("ready", true);
bind("ready", { passive: true });
"#,
    );
    assert_eq!(
        ts2345_count(&diags),
        0,
        "non-fresh boolean and the object member both stay assignable: {diags:?}"
    );
}

#[test]
fn weak_object_only_union_member_still_rejects_primitive_argument() {
    // Negative control: with NO sibling primitive member, a `boolean` argument
    // against a weak-object-only parameter must still be rejected (both tsc and
    // tsz reject — tsc TS2559, tsz TS2345). The fix must not over-correct into
    // accepting this.
    let diags = check_source_code_messages(
        r#"
interface Opts { capture?: boolean; once?: boolean }
declare function on<K extends string>(type: K, options?: Opts): void;
on("x", false);
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "a boolean against a weak-object-only parameter must still be rejected: {diags:?}"
    );
}

#[test]
fn plain_weak_object_from_function_value_still_reports_ts2559() {
    // Negative control: assigning a function value to a plain (non-intersection)
    // weak object must still produce the weak-type TS2559 diagnostic.
    let diags = check_source_code_messages(
        r#"
type Weak = { p?: number };
const y: Weak = () => {};
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2559),
        "a function value assigned to a plain weak object must still report TS2559: {diags:?}"
    );
}

#[test]
fn wrong_object_property_through_generic_union_still_errors() {
    // Negative control: the object union member is still checked structurally —
    // a wrong property type must still surface a diagnostic, proving the fix
    // only restores the primitive member rather than disabling the object arm.
    let diags = check_source_code_messages(
        r#"
interface Opts { capture: boolean }
declare function on<K extends string>(type: K, options?: boolean | Opts): void;
on("x", { capture: 1 });
"#,
    );
    assert!(
        !diags.is_empty(),
        "passing a wrong-typed object property must still error: {diags:?}"
    );
}

#[test]
fn boolean_or_empty_object_union_still_collapses_to_empty_object() {
    // The `has_empty_object` exception must remain: when the union literally
    // contains an empty object type, the primitive IS subsumed and collapsed,
    // matching tsc (`boolean | {}` → `{}`). A string is assignable to `{}`.
    let diags = check_source_code_messages(
        r#"
type U = boolean | {};
const a: U = false;
const b: U = {};
const c: U = "anything";
"#,
    );
    assert_eq!(
        ts2345_count(&diags),
        0,
        "boolean | {{}} collapses to {{}} which accepts any non-nullish value: {diags:?}"
    );
}
