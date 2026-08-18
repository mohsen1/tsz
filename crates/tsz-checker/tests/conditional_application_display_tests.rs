//! A conditional-bodied generic type-alias application loses tsc's `aliasSymbol`
//! once the conditional reduces, so the solver formatter renders the evaluated
//! result structurally for any concrete shape (tuple, array, object, primitive,
//! `never`) in the nested elaboration positions of assignment diagnostics —
//! `{ p: TupleBox<string> }` shows `{ p: [string]; }`, not
//! `{ p: TupleBox<string>; }`. Previously only object results expanded.
//!
//! Two boundaries keep this honest:
//! * Bare literal / union results stay on the application surface because tsc
//!   applies literal-union display widening there (a separate display concern).
//! * A non-converged recursive reduction (a truncated cycle) keeps the alias
//!   name rather than rendering a partial expansion.
//!
//! Verified against `tsc` 6.0.2. Binder names are varied across the matrix so the
//! rule is proven structural, not keyed on a particular identifier.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_diagnostics, check_source_strict};

fn collect(diags: Vec<Diagnostic>) -> Vec<String> {
    let mut out = Vec::new();
    for d in diags {
        out.push(d.message_text.clone());
        for r in &d.related_information {
            out.push(r.message_text.clone());
        }
    }
    out
}

/// Collect the rendered messages (primary + nested) for inspection.
#[track_caller]
fn messages(source: &str) -> Vec<String> {
    collect(check_source_diagnostics(source))
}

/// As [`messages`], but under `strict` + `strictNullChecks` — required for the
/// nullish-stripping cases (`NonNullable<…>`) whose mismatch only surfaces when
/// `undefined` is a tracked member.
#[track_caller]
fn messages_strict(source: &str) -> Vec<String> {
    collect(check_source_strict(source))
}

#[track_caller]
fn assert_msgs_any_contains(msgs: &[String], needle: &str) {
    assert!(
        msgs.iter().any(|m| m.contains(needle)),
        "expected a diagnostic containing {needle:?}, got: {msgs:#?}",
    );
}

#[track_caller]
fn assert_msgs_none_contains(msgs: &[String], needle: &str) {
    assert!(
        !msgs.iter().any(|m| m.contains(needle)),
        "expected no diagnostic containing {needle:?}, got: {msgs:#?}",
    );
}

#[track_caller]
fn assert_any_contains(source: &str, needle: &str) {
    assert_msgs_any_contains(&messages(source), needle);
}

#[track_caller]
fn assert_none_contains(source: &str, needle: &str) {
    assert_msgs_none_contains(&messages(source), needle);
}

// ── Nested elaboration positions (rendered by the solver formatter) ──

#[test]
fn nested_conditional_application_tuple_renders_structurally() {
    let source = r#"
type TupleBox<T> = T extends string ? [T] : never;
declare const x: { p: TupleBox<string> };
const y: { p: number } = x;
"#;
    assert_any_contains(source, "[string]");
    assert_none_contains(source, "TupleBox<string>");
}

#[test]
fn nested_conditional_application_object_renders_structurally() {
    // Renamed binder (`Cell`/`E`) — structural, not identifier-keyed.
    let source = r#"
type Cell<E> = E extends number ? { v: E } : never;
declare const x: { p: Cell<1> };
const y: { p: string } = x;
"#;
    assert_any_contains(source, "{ v: 1; }");
    assert_none_contains(source, "Cell<1>");
}

#[test]
fn nested_conditional_application_array_renders_structurally() {
    let source = r#"
type Arr<T> = T extends number ? T[] : never;
declare const x: { p: Arr<1> };
const y: { p: string } = x;
"#;
    assert_any_contains(source, "1[]");
    assert_none_contains(source, "Arr<1>");
}

// ── Negative controls: mapped/object-bodied applications keep their name ──

#[test]
fn nested_mapped_application_keeps_alias_name() {
    // A mapped body (not conditional) keeps tsc's alias symbol, so the
    // application surface is preserved rather than expanded to its structural
    // object. Defined locally so the harness does not depend on `lib.es5`.
    let source = r#"
type Keep<T> = { [K in keyof T]: T[K] };
declare const x: { q: Keep<{ a: 1 }> };
const y: { q: { z: 1 } } = x;
"#;
    assert_any_contains(source, "Keep<");
}

#[test]
fn generic_conditional_application_keeps_alias_name() {
    // `Extract<Extract<T, Foo>, Bar>` is generic (free `T`): tsc keeps the
    // deferred `Extract<…>` application form even though the display-time
    // evaluator may collapse the unconstrained conditional to `never`. The
    // input-genericity guard prevents that `never` from leaking into display.
    // (Conformance fixture `conditionalTypes2.ts`.)
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type Foo = { foo: string };
type Bar = { bar: string };
declare function fooBat(x: { foo: string; bat: string }): void;
function f<T>(x: Extract<Extract<T, Foo>, Bar>) {
  fooBat(x);
}
"#;
    assert_any_contains(source, "Extract<Extract<T, Foo>, Bar>");
    assert_none_contains(source, "type 'never'");
}

#[test]
fn deferred_generic_conditional_keeps_branch_union() {
    // Still generic (free `T`), target concrete (`number`): tsc expands the
    // source to its branch union `string | boolean` for display rather than
    // keeping the alias `F<T>` — the conditional's two branches are fully
    // concrete, so there is nothing left to defer.
    let source = r#"
type F<T> = T extends number ? string : boolean;
function g<T>(p: F<T>): void { const y: number = p; }
"#;
    assert_any_contains(source, "string | boolean");
    assert_none_contains(source, "F<T>");
}

#[test]
fn deferred_generic_conditional_keeps_branch_union_renamed_binders() {
    // Same structural rule as `deferred_generic_conditional_keeps_branch_union`,
    // different identifiers — proves it is not keyed on a particular alias or
    // type-parameter name.
    let source = r#"
type G<S> = S extends string ? number : boolean;
function h<S>(p: G<S>): void { const z: string = p; }
"#;
    assert_any_contains(source, "number | boolean");
    assert_none_contains(source, "G<S>");
}

#[test]
fn deferred_conditional_branch_wrapping_check_param_keeps_alias() {
    // A branch that merely *wraps* the check type parameter (`T[]`, not the
    // bare parameter itself) is not a fully concrete branch union — tsc
    // renders a partial union `boolean | T[]` here, which tsz does not
    // reproduce; the safe fallback is to keep the deferred alias spelling
    // rather than either mis-expand or drop the still-generic branch. This
    // locks in the conservative behavior (no regression from the branch-union
    // guard above) rather than the not-yet-implemented partial expansion.
    let source = r#"
type H<T> = T extends number ? T[] : boolean;
function i<T>(p: H<T>): void { const z: string = p; }
"#;
    assert_any_contains(source, "H<T>");
    assert_none_contains(source, "boolean | T[]");
}

#[test]
fn deferred_conditional_bare_check_param_branch_keeps_alias_against_concrete_target() {
    // The branch-union guard must not fire when a branch is the bare check
    // type parameter itself (`Extract<T, U> = T extends U ? T : never`):
    // unioning would leak `never` into the display. Unlike
    // `generic_conditional_application_keeps_alias_name` (a function-argument
    // TS2345 case), this is a plain TS2322 assignment against a concrete
    // object target, exercising the same guard this test file's primary fix
    // touches.
    let source = r#"
type Extract2<T, U> = T extends U ? T : never;
type Foo = { foo: string };
type Bar = { bar: string };
function f<T>(x: Extract2<Extract2<T, Foo>, Bar>) {
  const target: { foo: string; bat: string } = x;
}
"#;
    assert_any_contains(source, "Extract2<Extract2<T, Foo>, Bar>");
    assert_none_contains(source, "type 'never'");
}

// ── Source position against a generic target (the alias is preserved) ──
//
// tsc renders the *source* of a TS2322 with its written conditional/indexed
// spelling — never its apparent branch-union / constraint — when the target is
// itself generic (a deferred conditional or type-parameter-bearing type). The
// relation still compares the apparent form; only the displayed source differs.
// This is the mirror of the target-side conditional guard and of the
// concrete-target case above (where the constraint IS shown).

#[test]
fn conditional_source_keeps_alias_against_generic_target() {
    // `T95<U>` vs `T94<U>`: both deferred conditionals. tsc shows the source as
    // `T95<U>`, not its branch union `number | boolean`. (Fixture
    // `conditionalTypes1.ts`, `f45`.)
    let source = r#"
type T94<T> = T extends string ? true : 42;
type T95<T> = T extends string ? boolean : number;
function h<U>(value: T95<U>, sink: T94<U>) { sink = value; }
"#;
    assert_any_contains(source, "Type 'T95<U>' is not assignable to type 'T94<U>'.");
    assert_none_contains(source, "number | boolean");
}

#[test]
fn conditional_source_keeps_alias_against_generic_target_renamed_binders() {
    // Same structural rule, different identifiers — proves it is not keyed on a
    // particular alias/parameter name.
    let source = r#"
type Pick94<Q> = Q extends string ? true : 42;
type Pick95<Q> = Q extends string ? boolean : number;
function relay<W>(input: Pick95<W>, out: Pick94<W>) { out = input; }
"#;
    assert_any_contains(
        source,
        "Type 'Pick95<W>' is not assignable to type 'Pick94<W>'.",
    );
    assert_none_contains(source, "number | boolean");
}

#[test]
fn indexed_access_source_keeps_spelling_against_generic_target() {
    // A deferred indexed-access source `T["x"]` keeps its written form against a
    // generic `NonNullable<T["x"]>` target, rather than collapsing to the
    // constraint `string | undefined`. (Fixture `conditionalTypes1.ts`, `f4`.)
    // `NonNullable` is defined locally (the unit harness does not load `lib.es5`)
    // with tsc's `T & {}` body so the nullish-stripping target keeps its alias.
    let source = r#"
type NonNullable<T> = T & {};
function pick<T extends { x: string | undefined }>(src: T["x"], dst: NonNullable<T["x"]>) {
    dst = src;
}
"#;
    let msgs = messages_strict(source);
    assert_msgs_any_contains(&msgs, "Type 'T[\"x\"]' is not assignable to type");
    assert_msgs_none_contains(&msgs, "Type 'string | undefined' is not assignable");
}

#[test]
fn conditional_source_still_expands_against_concrete_target() {
    // Negative boundary: against a *concrete* target tsc shows the source's
    // apparent constraint, not the alias. `IsArray<T>` is rendered `boolean`
    // for `let t: true = x`. (Fixture `distributiveConditionalTypeConstraints.ts`.)
    let source = r#"
type IsArray<T> = T extends unknown[] ? true : false;
function f1<T extends object>(x: IsArray<T>) {
    let t: true = x;
}
"#;
    assert_any_contains(source, "Type 'boolean' is not assignable to type 'true'.");
    assert_none_contains(source, "Type 'IsArray<T>'");
}
