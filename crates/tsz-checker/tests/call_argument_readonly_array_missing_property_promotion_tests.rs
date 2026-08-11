//! Missing-property promotion when the target is a `readonly T[]` /
//! `ReadonlyArray<T>` array type (#17154).
//!
//! Structural rule: when an object value that is missing the array surface is
//! assigned to — or passed as a call argument to a parameter of — an array
//! type, tsc promotes the failure to TS2740 (many missing properties), naming
//! the array's members. This holds for the mutable `Array<T>` surface and,
//! symmetrically, for the readonly surface: a `readonly T[]` / `ReadonlyArray<T>`
//! target carries the same members MINUS the mutating methods
//! (`push`/`pop`/`shift`/`unshift`/`splice`/`sort`/`reverse`/`fill`/`copyWithin`),
//! so its missing list is `length, concat, join, slice, ...` — never `push`/`pop`.
//!
//! Root cause: the object-source-vs-array-target arm of the solver's failure
//! explainer peeled only a *mutable* `Array<T>` element (`array_element_type`),
//! so a readonly-wrapped target fell through to a bare TS2322/TS2345 head with
//! no property list. The explainer now also peels the readonly wrapper (syntax
//! `readonly T[]` and the `ReadonlyArray<T>` application) and, for a readonly
//! target, strips the mutating methods from the required set so the promoted
//! list matches tsc's readonly member set exactly.
//!
//! Tests vary the binder names (function / parameter / element-type spellings)
//! and cover both the call-argument and direct-assignment paths, asserting on
//! the promoted diagnostic *code* and that the readonly member list is used
//! (mutating methods must NOT appear), rather than on exact type rendering.

use tsz_checker::context::CheckerOptions;

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// A TS2740 "missing the following properties from type ..." head whose list
/// begins with the read-only array surface and contains none of the mutating
/// methods a `ReadonlyArray` omits.
fn has_readonly_array_missing_head(diags: &[(u32, String)]) -> bool {
    diags.iter().any(|(code, msg)| {
        *code == 2740
            && msg.contains("missing the following properties from type")
            && msg.contains("length")
            && msg.contains("concat")
            && !msg.contains("push")
            && !msg.contains("pop")
    })
}

/// A TS2740 head whose list is the mutable array surface (includes `push`/`pop`).
fn has_mutable_array_missing_head(diags: &[(u32, String)]) -> bool {
    diags.iter().any(|(code, msg)| {
        *code == 2740
            && msg.contains("missing the following properties from type")
            && msg.contains("push")
            && msg.contains("pop")
    })
}

#[test]
fn call_argument_to_readonly_array_promotes_ts2740() {
    let diags = strict_diagnostics(
        r#"
declare function accept(value: ReadonlyArray<string>): void;
accept({});
"#,
    );
    assert!(
        has_readonly_array_missing_head(&diags),
        "call argument `{{}}` to a `ReadonlyArray<string>` parameter should promote to \
         TS2740 with the readonly member list; got {diags:?}"
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "must not fall back to a bare TS2345 head; got {diags:?}"
    );
}

#[test]
fn call_argument_to_readonly_array_syntax_promotes_ts2740() {
    // `readonly T[]` syntax, renamed binders — a spelling-keyed fix would miss it.
    let diags = strict_diagnostics(
        r#"
declare function take(items: readonly number[]): void;
take({});
"#,
    );
    assert!(
        has_readonly_array_missing_head(&diags),
        "call argument `{{}}` to a `readonly number[]` parameter should promote to TS2740 with \
         the readonly member list; got {diags:?}"
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "must not fall back to a bare TS2345; got {diags:?}"
    );
}

#[test]
fn assignment_to_readonly_array_promotes_ts2740() {
    // The direct-assignment path shares the same explainer and must promote too.
    let diags = strict_diagnostics(
        r#"
const value: ReadonlyArray<string> = {};
const syntax: readonly string[] = {};
"#,
    );
    let promoted = diags.iter().filter(|(code, _)| *code == 2740).count();
    assert_eq!(
        promoted, 2,
        "both readonly-array assignments should promote to TS2740; got {diags:?}"
    );
    assert!(
        has_readonly_array_missing_head(&diags),
        "the readonly member list must be used; got {diags:?}"
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "must not fall back to a bare TS2322 head; got {diags:?}"
    );
}

#[test]
fn readonly_array_of_object_element_promotes_ts2740() {
    // Renamed element type; still promotes with the readonly surface.
    let diags = strict_diagnostics(
        r#"
interface Point { x: number; y: number; }
declare function plot(points: ReadonlyArray<Point>): void;
plot({});
"#,
    );
    assert!(
        has_readonly_array_missing_head(&diags),
        "a `ReadonlyArray<Point>` target should promote to TS2740 with the readonly member \
         list; got {diags:?}"
    );
}

#[test]
fn mutable_array_target_keeps_mutable_member_list() {
    // Guard: the readonly stripping must NOT touch a mutable `Array<T>` target,
    // whose promoted list still includes the mutating methods.
    let diags = strict_diagnostics(
        r#"
declare function push_all(xs: Array<string>): void;
push_all({});
const arr: string[] = {};
"#,
    );
    assert!(
        has_mutable_array_missing_head(&diags),
        "a mutable array target must keep the mutable member list (with push/pop); got {diags:?}"
    );
    assert!(
        !has_readonly_array_missing_head(&diags),
        "a mutable array target must not use the stripped readonly list; got {diags:?}"
    );
}

#[test]
fn readonly_tuple_target_is_not_promoted_to_array_head() {
    // Over-promotion guard: a readonly *tuple* target is not an array surface;
    // tsc keeps the generic TS2345 head, so the array missing-property promotion
    // must not fire.
    let diags = strict_diagnostics(
        r#"
declare function pair(p: readonly [string, number]): void;
pair({});
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "a readonly tuple target keeps the bare TS2345 head; got {diags:?}"
    );
    assert!(
        !has_readonly_array_missing_head(&diags) && !has_mutable_array_missing_head(&diags),
        "must not synthesize an array missing-property head for a tuple target; got {diags:?}"
    );
}

#[test]
fn readonly_array_element_mismatch_keeps_element_chain() {
    // Over-promotion guard: when the source IS a readonly array with a mismatched
    // element, tsc elaborates the element relation (TS2322 + inner element line),
    // not a missing-property head.
    let diags = strict_diagnostics(
        r#"
declare const src: readonly string[];
const dst: readonly number[] = src;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "an element-type mismatch on a readonly array must stay a TS2322 chain; got {diags:?}"
    );
    assert!(
        !has_readonly_array_missing_head(&diags),
        "an element mismatch must not become a missing-property head; got {diags:?}"
    );
}

#[test]
fn readonly_array_partial_source_lists_only_missing_members() {
    // A source that supplies some of the array surface promotes with the
    // remaining readonly members (still no mutating methods).
    let diags = strict_diagnostics(
        r#"
declare function take(items: readonly string[]): void;
take({ length: 1 });
"#,
    );
    assert!(
        diags.iter().any(|(code, msg)| *code == 2740
            && msg.contains("missing the following properties from type")
            && !msg.contains("push")
            && !msg.contains("pop")),
        "a partial source should promote to TS2740 listing the remaining readonly members; \
         got {diags:?}"
    );
}
