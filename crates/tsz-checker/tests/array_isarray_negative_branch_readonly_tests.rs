//! Tests for the negative `!Array.isArray(x)` branch keeping `readonly`
//! array-likes (issue #14782).
//!
//! Rule under test:
//!
//! > `Array.isArray`'s effective type predicate is `x is any[]` — a MUTABLE
//! > array. A `readonly` array/tuple (`readonly T[]`, `readonly [..]`,
//! > `ReadonlyArray<T>`) is NOT assignable to a mutable `any[]`, so tsc KEEPS
//! > those members in the negative (`else`) branch of `if (Array.isArray(x))`.
//! > Only mutable arrays/tuples are subtracted. tsz previously dropped readonly
//! > array-likes too, narrowing a single readonly array to `never` and masking
//! > downstream assignability/property errors (a soundness gap / false
//! > negative).
//!
//! The rule is *structural*, not name-bound: each case varies the binder and
//! element-type spelling so the behavior cannot be a fixture-name fast path.
//! All cases use the es5 lib so `Array.isArray` resolves to its real
//! `arg is any[]` predicate and `ReadonlyArray`/`Array` are defined.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_lib_files};

fn check_es5(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
}

// ─── readonly single member is kept (not collapsed to `never`) ───────────────

/// A lone `readonly T[]` source must stay `readonly T[]` in the `else` branch,
/// so assigning it to `never` is a TS2322. Pre-#14782 tsz narrowed it to
/// `never` and accepted the assignment (false negative).
#[test]
fn negative_branch_keeps_readonly_array_single() {
    let source = r#"
function probe(values: readonly number[]) {
    if (Array.isArray(values)) {
    } else {
        const sink: never = values;
    }
}
"#;
    let diags = check_es5(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "readonly number[] must be kept in the !Array.isArray branch (not narrowed to never), \
         so `const sink: never = values` should be TS2322; got: {diags:?}"
    );
}

/// `ReadonlyArray<T>` (the lib generic, distinct element spelling) is likewise
/// kept in the negative branch.
#[test]
fn negative_branch_keeps_readonly_array_generic_single() {
    let source = r#"
function inspect(items: ReadonlyArray<string>) {
    if (Array.isArray(items)) {
    } else {
        const out: number = items;
    }
}
"#;
    let diags = check_es5(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "ReadonlyArray<string> must be kept in the !Array.isArray branch, so \
         `const out: number = items` should be TS2322; got: {diags:?}"
    );
}

/// A `readonly` tuple (`ReadonlyType(Tuple)`) is kept too.
#[test]
fn negative_branch_keeps_readonly_tuple_single() {
    let source = r#"
function unpack(pair: readonly [number, string]) {
    if (Array.isArray(pair)) {
    } else {
        const gone: never = pair;
    }
}
"#;
    let diags = check_es5(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "readonly [number, string] must be kept in the !Array.isArray branch; \
         `const gone: never = pair` should be TS2322; got: {diags:?}"
    );
}

// ─── mutable members are still subtracted (controls that discriminate) ───────

/// A lone *mutable* `T[]` is still narrowed to `never` in the negative branch
/// (it IS assignable to the mutable `any[]` predicate). Assigning to `never`
/// must therefore be accepted — proving the fix discriminates readonly from
/// mutable rather than blanket-keeping every array.
#[test]
fn negative_branch_subtracts_mutable_array_single_to_never() {
    let source = r#"
function probe(values: number[]) {
    if (Array.isArray(values)) {
    } else {
        const sink: never = values;
    }
}
"#;
    let diags = check_es5(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "mutable number[] must still narrow to never in the !Array.isArray branch, \
         so `const sink: never = values` must be accepted; got: {diags:?}"
    );
}

/// A lone *mutable* tuple is also subtracted to `never`.
#[test]
fn negative_branch_subtracts_mutable_tuple_single_to_never() {
    let source = r#"
function unpack(pair: [number, string]) {
    if (Array.isArray(pair)) {
    } else {
        const gone: never = pair;
    }
}
"#;
    let diags = check_es5(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "mutable [number, string] must still narrow to never in the !Array.isArray branch; \
         `const gone: never = pair` must be accepted; got: {diags:?}"
    );
}

// ─── unions: readonly kept, mutable dropped ──────────────────────────────────

/// In `readonly T[] | scalar`, the `else` branch keeps both members, so the
/// whole source is not assignable to the scalar alone (TS2322).
#[test]
fn negative_branch_union_keeps_readonly_member() {
    let source = r#"
function handle(input: readonly string[] | number) {
    if (Array.isArray(input)) {
    } else {
        const n: number = input;
    }
}
"#;
    let diags = check_es5(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "readonly string[] must survive into the else branch alongside number, so \
         `const n: number = input` should be TS2322; got: {diags:?}"
    );
}

/// In `mutable T[] | scalar`, the mutable array is removed, leaving only the
/// scalar — so assigning to the scalar is accepted (no TS2322). Discriminating
/// control versus the readonly-union case above.
#[test]
fn negative_branch_union_drops_mutable_member() {
    let source = r#"
function handle(input: string[] | number) {
    if (Array.isArray(input)) {
    } else {
        const n: number = input;
    }
}
"#;
    let diags = check_es5(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "mutable string[] must be removed from the else branch, leaving only number, so \
         `const n: number = input` must be accepted; got: {diags:?}"
    );
}

// ─── downstream property access is no longer masked ──────────────────────────

/// With a readonly array kept in the else branch, a property that exists only
/// on the object member is a TS2339 (pre-#14782 the readonly member was dropped
/// and the access silently succeeded against the object alone).
#[test]
fn negative_branch_readonly_union_member_unmasks_property_error() {
    let source = r#"
function route(value: ReadonlyArray<string> | { tag: "obj" }) {
    if (Array.isArray(value)) {
    } else {
        const t: "obj" = value.tag;
    }
}
"#;
    let diags = check_es5(source);
    let ts2339: Vec<_> = diags.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "readonly string[] must remain in the else branch, so `value.tag` should be a \
         TS2339 on the readonly-array member; got: {diags:?}"
    );
}
