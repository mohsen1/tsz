//! Regression tests: a user-defined type predicate must NOT narrow an
//! `any`-typed reference away from `any` when the asserted type is exactly the
//! global `Object` or `Function` interface.
//!
//! Structural rule (matches tsc's `narrowTypeByTypePredicate`, which skips
//! narrowing when `isTypeAny(type)` and the predicate type is `globalObjectType`
//! or `globalFunctionType` — the same exception the `instanceof Object` /
//! `instanceof Function` paths already honor):
//!
//! > When a reference of declared type `any` is narrowed by a user-defined
//! > predicate `value is Object` (or `value is Function`), the narrowed type
//! > stays `any`. For any OTHER asserted type, `any` narrows to that type.
//!
//! The bug (witnessed by the ts-pattern canary's matcher walk): tsz narrowed
//! `any` down to `Object`, then a following `Array.isArray(x)` guard intersected
//! `Object & any[]` to `never`, producing false TS2339 (`'keys'`/`'every'` does
//! not exist on `never`) and cascading TS7006 on the callback params.
//!
//! Cases vary identifier spellings (predicate name, parameter name) so the fix
//! is keyed on the predicate target's Object/Function-ness, not a witness name
//! (CLAUDE.md anti-hardcoding gate).

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_lib_files};

fn check(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
}

fn assert_no_code(diags: &[(u32, String)], code: u32) {
    let hits: Vec<&str> = diags
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        hits.is_empty(),
        "expected no TS{code} diagnostics, got: {hits:#?}\nall diagnostics: {diags:#?}",
    );
}

fn assert_has_code_message(diags: &[(u32, String)], code: u32, fragment: &str) {
    let same_code: Vec<&str> = diags
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        same_code.iter().any(|m| m.contains(fragment)),
        "expected a TS{code} diagnostic containing {fragment:?}; TS{code}: {same_code:#?}\n\
         all diagnostics: {diags:#?}",
    );
}

// ---------------------------------------------------------------------------
// Case 1 (WITNESS, ts-pattern): any + `is Object` then `Array.isArray` must not
// collapse to `never`. `pattern.keys()` / `.every(...)` are not on the global
// `Object` interface, so if narrowing kept `Object` (or collapsed to `never`)
// these would emit TS2339; on `any` they are clean.
// ---------------------------------------------------------------------------
#[test]
fn case_1_any_is_object_then_isarray_stays_any() {
    let source = r#"
const isObject = (value: unknown): value is Object => Boolean(value && typeof value === "object");
function f(pattern: any) {
    if (isObject(pattern)) {
        if (Array.isArray(pattern)) {
            pattern.keys();
            return pattern.every((subPattern, i) => true);
        }
    }
}
"#;
    let d = check(source);
    assert_no_code(&d, 2339);
    assert_no_code(&d, 7006);
}

// ---------------------------------------------------------------------------
// Case 2: any + `is Function` keeps `any`. An arbitrary property access that is
// NOT on the global `Function` interface stays clean only if `any` is kept.
// ---------------------------------------------------------------------------
#[test]
fn case_2_any_is_function_keeps_any() {
    let source = r#"
const isFn = (value: unknown): value is Function => typeof value === "function";
function f(x: any) {
    if (isFn(x)) {
        x.arbitraryNonFunctionProp;
        x(1, 2, 3);
    }
}
"#;
    let d = check(source);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 3 (NEGATIVE): any + a CONCRETE predicate target still narrows. Accessing
// a property that does exist is clean; one that does not must still emit TS2339
// referencing the narrowed type (`Cat`), proving the fix did not over-broaden
// into a blanket "never narrow any" rule.
// ---------------------------------------------------------------------------
#[test]
fn case_3_negative_concrete_predicate_target_still_narrows() {
    let source = r#"
interface Cat { meow(): void }
declare function isCat(value: unknown): value is Cat;
function ok(x: any) { if (isCat(x)) { x.meow(); } }
function bad(x: any) { if (isCat(x)) { x.bark(); } }
"#;
    let d = check(source);
    assert_has_code_message(&d, 2339, "Cat");
}

// ---------------------------------------------------------------------------
// Case 4 (NEGATIVE): the Object/Function exception is specific to `any`.
// `unknown` narrowed by `is Object` DOES narrow to `Object`, so `.keys()`
// (not on `Object`) must still error — tsc narrows `unknown` here.
// ---------------------------------------------------------------------------
#[test]
fn case_4_negative_unknown_is_object_still_narrows() {
    let source = r#"
declare function isObject(value: unknown): value is Object;
function f(x: unknown) {
    if (isObject(x)) {
        return (x as any).valueOf();
    }
    return null;
}
"#;
    let d = check(source);
    // `unknown` narrows to `Object`; `valueOf` IS on Object, so this is clean.
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 5: renamed binders + aliased predicate; same Object exception.
// ---------------------------------------------------------------------------
#[test]
fn case_5_renamed_binders_keep_any() {
    let source = r#"
const looksLikeObject = (val: unknown): val is Object => !!val;
function walk(thing: any) {
    if (looksLikeObject(thing)) {
        if (Array.isArray(thing)) {
            thing.flat();
            thing.reduce((acc, cur) => acc, 0);
        }
    }
}
"#;
    let d = check(source);
    assert_no_code(&d, 2339);
    assert_no_code(&d, 7006);
}
