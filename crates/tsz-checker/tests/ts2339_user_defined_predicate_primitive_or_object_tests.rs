//! Regression tests for issue #10680.
//!
//! Structural rule: a user-defined type predicate of the form
//! `function p(x: ...): x is T`, where `T` is an object/indexed-access shape
//! (`{ [K in S]: V }`, `Record<S, V>`, a plain object literal, an alias to
//! either, or an intersection containing one), must eliminate every union
//! member that is not assignable to `T` — including primitive members such
//! as `string` / `number` / `boolean`. After narrowing, property access on
//! the remaining (object) arms must not emit TS2339.
//!
//! The bug was: `string | NoMigrations` narrowed by `isObject(x): x is
//! ShallowRecord<string, unknown>` left the union unchanged because the
//! narrowing union-filter treated the primitive `string` arm as assignable
//! to an `Application(ShallowRecord, …)` target. Accessing the brand
//! property `__noMigrations__` then tripped TS2339 on `string | NoMigrations`.
//!
//! Cases below vary names (`P`/`K`/`X`), brands, alias shapes, and union
//! members to keep the fix from regressing into a name-keyed special case
//! (§25 / §26 of CLAUDE.md).

use tsz_checker::context::CheckerOptions;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
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

/// Assert that some diagnostic has `code` AND its message contains `fragment`.
///
/// Used by negative narrowing tests to distinguish "the diagnostic fired
/// because the access is genuinely invalid on the narrowed type" from "the
/// diagnostic fired because narrowing was broken and the access is invalid
/// on the original union" — both emit TS2339 with different messages.
fn assert_has_code_message(diags: &[(u32, String)], code: u32, fragment: &str) {
    let same_code: Vec<&str> = diags
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        same_code.iter().any(|m| m.contains(fragment)),
        "expected at least one TS{code} diagnostic whose message contains {fragment:?}; \
         TS{code} messages: {same_code:#?}\nall diagnostics: {diags:#?}",
    );
}

// ---------------------------------------------------------------------------
// Case 0 (KYSELY EXACT REPRO): predicate target is a conditional-wrapped
// mapped type — kysely's `ShallowRecord` is `DrainOuterGeneric<{ [P in K]: T
// }>` where `DrainOuterGeneric<X> = [X] extends [unknown] ? X : never`. The
// indirection through the conditional was previously masking the index
// signature from the narrowing union-filter, so `string` was not
// recognised as non-assignable to the predicate target and the union was
// returned unchanged.
// ---------------------------------------------------------------------------
#[test]
fn case_0_conditional_wrapped_mapped_predicate_target() {
    let source = r#"
type DrainOuterGeneric<T> = [T] extends [unknown] ? T : never;
type ShallowRecord<K extends string | number | symbol, V> =
    DrainOuterGeneric<{ [P in K]: V }>;

interface NoMigrations {
    readonly __noMigrations__: true;
}

declare function isObject(o: unknown): o is ShallowRecord<string, unknown>;

function f(targetMigrationName: string | NoMigrations) {
    if (isObject(targetMigrationName) && targetMigrationName.__noMigrations__ === true) {
        return targetMigrationName;
    }
    return null;
}
"#;
    let d = diagnostics(source);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 1: the exact kysely / #10680 shape, transliterated.
// `isObject` is a user-defined predicate whose target is `Record<string,
// unknown>`. Narrowing `string | NoMigrations` through it must drop the
// `string` arm so `.__noMigrations__` resolves on `NoMigrations`.
// ---------------------------------------------------------------------------
#[test]
fn case_1_kysely_no_migrations_shape() {
    let source = r#"
type Rec<K extends string | number | symbol, V> = { [P in K]: V };

interface NoMigrations {
    readonly __noMigrations__: true;
}

declare function isObject(o: unknown): o is Rec<string, unknown>;

function f(targetMigrationName: string | NoMigrations) {
    if (isObject(targetMigrationName) && targetMigrationName.__noMigrations__ === true) {
        return targetMigrationName;
    }
    return null;
}
"#;
    let d = diagnostics(source);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 2: same shape but with K and the brand renamed. If the fix is
// hardcoded against the kysely names, this case fails.
// ---------------------------------------------------------------------------
#[test]
fn case_2_renamed_brand_and_iteration_variable() {
    let source = r#"
type R<X extends string | number | symbol, T> = { [Q in X]: T };

interface Brand {
    readonly tag: true;
}

declare function check(o: unknown): o is R<string, unknown>;

function f(v: string | Brand) {
    if (check(v) && v.tag === true) {
        return v;
    }
    return null;
}
"#;
    let d = diagnostics(source);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 3: predicate target is a bare `Record<string, unknown>` where
// `Record` is declared locally with the lib-equivalent shape
// (`type Record<K extends keyof any, T> = { [P in K]: T }`). Tests do not
// load lib, so the alias is provided in-source rather than imported from
// `lib.es5.d.ts`; the structural rule is the same. Same union; same
// expected narrowing.
// ---------------------------------------------------------------------------
#[test]
fn case_3_builtin_record_alias() {
    let source = r#"
type Record<K extends keyof any, T> = { [P in K]: T };

interface Brand {
    readonly tag: true;
}

declare function isObj(o: unknown): o is Record<string, unknown>;

function f(v: string | Brand) {
    if (isObj(v) && v.tag === true) {
        return v;
    }
    return null;
}
"#;
    let d = diagnostics(source);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 4: predicate target is an inline index signature, not a generic
// alias. Confirms the rule applies to structural index signatures, not
// just `Record<…>`-shaped applications.
// ---------------------------------------------------------------------------
#[test]
fn case_4_inline_string_index_signature() {
    let source = r#"
interface Brand {
    readonly tag: true;
}

declare function isObj(o: unknown): o is { [k: string]: unknown };

function f(v: string | Brand) {
    if (isObj(v) && v.tag === true) {
        return v;
    }
    return null;
}
"#;
    let d = diagnostics(source);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 5: number / boolean / bigint / symbol primitives in the union must
// also be eliminated. None of these are assignable to `Record<string,
// unknown>`.
// ---------------------------------------------------------------------------
#[test]
fn case_5_other_primitives_in_union() {
    let source = r#"
type Record<K extends keyof any, T> = { [P in K]: T };

interface Brand {
    readonly tag: true;
}

declare function isObj(o: unknown): o is Record<string, unknown>;

function f(v: number | boolean | bigint | symbol | Brand) {
    if (isObj(v) && v.tag === true) {
        return v;
    }
    return null;
}
"#;
    let d = diagnostics(source);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 6: the predicate target is an *intersection* whose object component
// carries the brand. Narrowing must still eliminate `string` so that the
// brand on the object arm becomes accessible.
// ---------------------------------------------------------------------------
#[test]
fn case_6_intersection_predicate_target() {
    let source = r#"
type Record<K extends keyof any, T> = { [P in K]: T };

interface Tag {
    readonly tag: true;
}

declare function isTagged(o: unknown): o is Record<string, unknown> & Tag;

function f(v: string | Tag) {
    if (isTagged(v)) {
        return v.tag;
    }
    return null;
}
"#;
    let d = diagnostics(source);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 7 (NEGATIVE): if the predicate target IS a primitive-friendly type
// (e.g. `string`), the narrowing should NOT drop the `string` arm. This
// proves the fix is keyed on the predicate target's object-ness, not a
// blanket "kill primitives" rule.
// ---------------------------------------------------------------------------
#[test]
fn case_7_negative_predicate_target_is_string_keeps_string_arm() {
    let source = r#"
interface Brand {
    readonly tag: true;
}

declare function isStr(o: unknown): o is string;

function f(v: string | Brand): string {
    if (isStr(v)) {
        return v;
    }
    return "";
}
"#;
    let d = diagnostics(source);
    // After `isStr(v)`, v must remain typed as `string`. Returning it from a
    // `string` function must not raise TS2322.
    assert_no_code(&d, 2322);
    assert_no_code(&d, 2339);
}

// ---------------------------------------------------------------------------
// Case 8 (NEGATIVE): the access AFTER narrowing must still error when the
// access is not legitimate — i.e. the property genuinely does not exist on
// the narrowed object type and no index signature covers it. This proves
// the fix narrows the union but does not silence real TS2339.
//
// The diagnostic message MUST reference the narrowed type (`NotIndexed`),
// not the original union (`string | NotIndexed`). A bare `assert_has_code`
// would pass even if narrowing silently regressed, because the unnarrowed
// access also emits TS2339 — just against the union.
// ---------------------------------------------------------------------------
#[test]
fn case_8_negative_property_genuinely_missing_still_errors() {
    let source = r#"
interface NotIndexed {
    readonly tag: true;
}

declare function isTagged(o: unknown): o is NotIndexed;

function f(v: string | NotIndexed) {
    if (isTagged(v)) {
        // `tag` is fine; `noSuchProp` is not on NotIndexed and there is no
        // index signature. tsc emits TS2339 for this access.
        return v.noSuchProp;
    }
    return null;
}
"#;
    let d = diagnostics(source);
    assert_has_code_message(&d, 2339, "'NotIndexed'");
}
