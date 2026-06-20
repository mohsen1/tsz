//! `in`-operator narrowing must consult the *apparent* type of a union
//! constituent, not just its own structural object shape.
//!
//! Arrays, tuples, function types and primitives carry their members on their
//! apparent type — `Array<T>`/`ReadonlyArray<T>` (`push`, `length`, ...), the
//! global `Function` (`call`, `bind`, ...) and the boxed primitive wrappers
//! (`String#length`, `Number#toFixed`, ...). `tsc`'s `narrowTypeByInKeyword`
//! resolves the key against `getApparentType`, so `"push" in x` selects the
//! array constituent and `else` selects the others.
//!
//! Before the fix tsz only inspected raw object shapes, so it failed to narrow
//! these constituents in either branch and emitted spurious property/callability
//! errors on code `tsc` accepts:
//!   - TS2339 (property does not exist) on the un-narrowed union,
//!   - TS2349 (expression is not callable) on un-narrowed function constituents,
//!   - TS18046 (value is of type `unknown`) on apparent members.
//!
//! These tests pin the parity. They load the real libs because the apparent
//! members of arrays/tuples live on the `Array<T>`/`ReadonlyArray<T>` interface.
//! Binder names are varied across cases so the behavior is driven by the
//! structural shape of the receiver, not by any identifier spelling.

use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, load_default_lib_files, strict_checker_options};

/// Codes that this fix must stop emitting on the narrowed branches. Asserting
/// their *absence* (rather than an empty diagnostic set) keeps the tests robust
/// to unrelated lib-surface noise while still failing on the original bug.
const FALSE_POSITIVE_CODES: &[u32] = &[2339, 2349, 18046];

fn codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    let diags: Vec<Diagnostic> =
        check_source_with_libs(source, "test.ts", strict_checker_options(), &libs);
    diags.iter().map(|d| d.code).collect()
}

fn assert_no_false_positives(source: &str, label: &str) {
    let got = codes(source);
    for code in FALSE_POSITIVE_CODES {
        assert!(
            !got.contains(code),
            "{label}: expected no TS{code} after apparent-member `in` narrowing, got: {got:?}"
        );
    }
}

/// `"push" in x` selects the array; the `else` branch is the object.
#[test]
fn in_narrowing_selects_array_by_apparent_member() {
    let source = r#"
function pick(value: number[] | { tag: 1 }) {
    if ("push" in value) {
        value.push(1);
    } else {
        const t: 1 = value.tag;
    }
}
"#;
    assert_no_false_positives(source, "array push");
}

/// `"length" in x` is an apparent member of arrays; the `else` branch keeps the
/// object constituent.
#[test]
fn in_narrowing_array_length_apparent_member() {
    let source = r#"
function choose(input: string[] | { only: true }) {
    if ("length" in input) {
        const n: number = input.length;
    } else {
        const o: true = input.only;
    }
}
"#;
    assert_no_false_positives(source, "array length");
}

/// `"call" in x` is an apparent member of function types; the truthy branch must
/// narrow to the callable so the invocation is allowed, and the `else` branch to
/// the object so its property is allowed.
#[test]
fn in_narrowing_selects_function_by_apparent_member() {
    let source = r#"
function dispatch(handler: (() => void) | { id: 7 }) {
    if ("call" in handler) {
        handler();
    } else {
        const id: 7 = handler.id;
    }
}
"#;
    assert_no_false_positives(source, "function call");
}

/// The complement: testing for the object's own property must narrow the `else`
/// branch to the function and keep it callable.
#[test]
fn in_narrowing_else_branch_keeps_function_callable() {
    let source = r#"
function route(cb: ((n: number) => void) | { kind: "obj" }) {
    if ("kind" in cb) {
        const k: "obj" = cb.kind;
    } else {
        cb(42);
    }
}
"#;
    assert_no_false_positives(source, "else-branch function");
}

/// Tuple constituents also expose `length` through their apparent type.
#[test]
fn in_narrowing_selects_tuple_by_apparent_length() {
    let source = r#"
function take(pair: [number, string] | { single: 0 }) {
    if ("length" in pair) {
        const len: number = pair.length;
    } else {
        const s: 0 = pair.single;
    }
}
"#;
    assert_no_false_positives(source, "tuple length");
}

/// `readonly` arrays narrow identically — the wrapper around the array type must
/// not hide the apparent members.
#[test]
fn in_narrowing_selects_readonly_array_by_apparent_member() {
    let source = r#"
function scan(data: readonly number[] | { flag: false }) {
    if ("length" in data) {
        const n: number = data.length;
    } else {
        const f: false = data.flag;
    }
}
"#;
    assert_no_false_positives(source, "readonly array length");
}

/// Regression guard for plain object unions: the apparent-type fallback must not
/// disturb the existing own-property narrowing. Probing the *wrong* member in the
/// `else` branch still reports TS2339, exactly as before the fix.
#[test]
fn in_narrowing_object_union_still_reports_wrong_property() {
    let source = r#"
function classify(node: { a: 1 } | { b: 2 }) {
    if ("a" in node) {
        const a: 1 = node.a;
    } else {
        // `node` is `{ b: 2 }` here; `a` is genuinely absent.
        node.a;
    }
}
"#;
    let got = codes(source);
    assert!(
        got.contains(&2339),
        "expected TS2339 for the genuinely-absent property, got: {got:?}"
    );
}

/// `"p" in x` over a non-union receiver that already declares `p` as an
/// *optional* own property must not promote `p` to required. tsc's
/// `narrowTypeByInKeyword` adds no structural information when the property is
/// already known, so the property stays optional and `delete x.p` remains
/// legal. Promoting it to required produced a false TS2790 ("operand of a
/// 'delete' operator must be optional"), the ofetch `delete options.query`
/// witness. Binder names vary across cases so the behavior is driven by the
/// optional-own-property shape, not an identifier spelling.
#[test]
fn in_narrowing_keeps_optional_own_property_deletable() {
    let source = r#"
interface Opts { query?: Record<string, unknown>; headers: number; }
function configure(opts: Opts) {
    if ("query" in opts) {
        delete opts.query;
    }
}
"#;
    let got = codes(source);
    assert!(
        !got.contains(&2790),
        "expected no TS2790 after `in`-narrowing an optional own property, got: {got:?}"
    );
}

/// The negative soundness counterpart: `"p" in x` must *not* exclude `undefined`
/// from the value type of an already-present optional property. tsc keeps
/// `x.p` as `T | undefined` after the check, so assigning it to `T` still
/// reports TS2322. The earlier required-promotion wrongly stripped `undefined`,
/// silently accepting the unsound assignment.
#[test]
fn in_narrowing_preserves_undefined_in_optional_property_value() {
    let source = r#"
interface Box { item?: { n: number }; }
function read(box: Box) {
    if ("item" in box) {
        const x: { n: number } = box.item;
    }
}
"#;
    let got = codes(source);
    assert!(
        got.contains(&2322),
        "expected TS2322 — `in` must not exclude undefined from an optional property, got: {got:?}"
    );
}

/// Adjacent: a bare `delete x.p` without any preceding `in`-narrowing already
/// worked; this pins that the fix did not disturb it.
#[test]
fn delete_optional_property_without_in_narrowing_is_legal() {
    let source = r#"
interface Config { params?: number[]; name: string; }
function reset(config: Config) {
    delete config.params;
}
"#;
    let got = codes(source);
    assert!(
        !got.contains(&2790),
        "expected no TS2790 deleting an optional property, got: {got:?}"
    );
}

/// The negative `in` branch over a *non-union* receiver must leave an optional
/// property's value type unchanged, even when its declared type excludes
/// `undefined`. tsc's `narrowTypeByInKeyword` keeps the constituent (the
/// property may legitimately be absent at runtime), so a subsequent write of a
/// valid value to the property still type-checks. The previous behavior
/// intersected the receiver with a synthetic `{ p: undefined }`, collapsing the
/// property to `never` and emitting a spurious TS2322 — the ofetch witness.
#[test]
fn negative_in_keeps_non_union_optional_property_assignable() {
    let source = r#"
interface Opts {
  method?: string;
  duplex?: "half";
}
declare const options: Opts;
if (!("duplex" in options)) {
  options.duplex = "half";
}
"#;
    let got = codes(source);
    assert!(
        !got.contains(&2322),
        "expected no TS2322 — negative `in` must keep an optional property assignable, got: {got:?}"
    );
}

/// Binder-name and shape variation of the same rule: the behavior must be driven
/// by the optional-own-property shape, not by any identifier spelling, and must
/// hold when the property's declared type is a wider literal union that still
/// excludes `undefined`.
#[test]
fn negative_in_keeps_optional_property_assignable_varied_binders() {
    let source = r#"
interface Settings {
  retries?: number;
  mode?: "fast" | "slow";
}
declare const settings: Settings;
if (!("mode" in settings)) {
  settings.mode = "fast";
}
"#;
    let got = codes(source);
    assert!(
        !got.contains(&2322),
        "expected no TS2322 — varied-binder optional property must stay assignable, got: {got:?}"
    );
}

/// Negative counterpart for a *required* property: `!("p" in x)` is unreachable
/// when `p` is guaranteed present, so tsc narrows the receiver to `never`. The
/// fix preserves this — a required property still drops to `never` on the
/// negative branch (no spurious acceptance, no synthetic intersection).
#[test]
fn negative_in_required_property_narrows_to_never() {
    let source = r#"
interface Tagged { tag: "x"; value: number; }
declare const obj: Tagged;
if (!("tag" in obj)) {
    const unreachable: never = obj;
}
"#;
    let got = codes(source);
    assert!(
        !got.contains(&2322),
        "expected the receiver to be `never` on the negative branch of a required property, got: {got:?}"
    );
}

/// Union variant: when *every* constituent requires the property, the negative
/// branch filters them all out, so tsc narrows to `never`. Assigning the
/// narrowed receiver to a `never` annotation must therefore be accepted.
#[test]
fn negative_in_union_all_required_narrows_to_never() {
    let source = r#"
declare const node: { kind: "a"; a: 1 } | { kind: "b"; a: 2 };
if (!("a" in node)) {
    const unreachable: never = node;
}
"#;
    let got = codes(source);
    assert!(
        !got.contains(&2322),
        "expected `never` when every union member requires the property, got: {got:?}"
    );
}
