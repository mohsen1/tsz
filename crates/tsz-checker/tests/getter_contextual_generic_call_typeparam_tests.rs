//! Regression tests for #16025's standalone `jsonAgg`/`FunctionModule` repro:
//! inferring a type argument from a contextual type when both the callee's
//! return type and the contextual type are intersections of the same arity.
//!
//! Structural rule: `constrain_types_impl`
//! (`crates/tsz-solver/src/operations/constraints/walker.rs`) matched a naked
//! source type parameter appearing in an intersection (e.g. `TB` in
//! `TB & string`) against *every* member of the target intersection instead
//! of only its positional counterpart. tsc's `inferFromTypes` does not do
//! this: for same-arity intersections it pairs members positionally, so a
//! naked source member only picks up its counterpart's upper bound. tsz's
//! nested single-side decompose (`(_, Intersection)` then, recursively,
//! `(Intersection, _)`) instead matched `TB` against *both* target members —
//! its real counterpart (correct) and the unrelated second member (spurious)
//! — and the two upper bounds then combined into an artificial `X & string`
//! candidate instead of `X`. Fixed by adding a same-arity
//! `(Intersection, Intersection)` arm that positionally pairs a naked member
//! with its counterpart and only falls back to broad member-to-member
//! matching for non-naked (structured) members.
//!
//! The witness is kysely's `FunctionModule<DB, TB extends keyof DB>`
//! (`src/kysely.ts:232`, via `get fn(): FunctionModule<DB, keyof DB>`), where
//! a sibling generic method's own type parameter is constrained by
//! `(TB & string) | Expr<unknown>`. Reduced from #16025's comment repro.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;
use tsz_common::ModuleKind;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2020,
        module: ModuleKind::ESNext,
        no_lib: true,
        ..Default::default()
    }
}

/// `no_lib` checking cannot resolve global types, so filter the unavoidable
/// `TS2318` noise and assert on the real diagnostics.
fn real_diagnostics(diags: &[Diagnostic]) -> Vec<(u32, &str)> {
    diags
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.as_str()))
        .collect()
}

/// Core repro: a getter's declared return type contextually types a
/// zero-argument generic call whose type parameter appears, elsewhere in the
/// same interface, inside an intersection with `string`. Renaming `DB`
/// between `Holder` and `createFunctionModule` (both spell it `DB`) is
/// incidental to kysely's real source, not the trigger — the assignability
/// diagnostic disappears once the type parameter binds to the plain
/// `keyof DB` upper bound instead of the artificial `keyof DB & string`.
#[test]
fn contextual_call_infers_positional_member_not_full_intersection() {
    let diags = check_source(
        r#"
interface Expr<T> { __exprType: T }
interface FunctionModule<DB, TB extends keyof DB> {
  <O>(name: string): O;
  jsonAgg<T extends (TB & string) | Expr<unknown>>(
    table: T,
  ): T extends TB ? DB[T][] : T extends Expr<infer O> ? O[] : never;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
class Holder<DB> {
  get fn(): FunctionModule<DB, keyof DB> { return createFunctionModule(); }
}
"#,
        "test.ts",
        opts(),
    );
    assert_eq!(
        real_diagnostics(&diags),
        Vec::<(u32, &str)>::new(),
        "core repro should be clean"
    );
}

/// Name-agnostic control: every binder renamed (`DB`→`Env`, `TB`→`Key`,
/// `T`→`Elem`, `O`→`Out`, `Expr`→`Wrapped`). Proves the fix is about
/// intersection member arity/position, not any particular spelling.
#[test]
fn contextual_call_infers_positional_member_is_name_agnostic() {
    let diags = check_source(
        r#"
interface Wrapped<Elem> { __exprType: Elem }
interface FunctionModule<Env, Key extends keyof Env> {
  <Out>(name: string): Out;
  jsonAgg<Elem extends (Key & string) | Wrapped<unknown>>(
    table: Elem,
  ): Elem extends Key ? Env[Elem][] : Elem extends Wrapped<infer Out> ? Out[] : never;
}
declare function createFunctionModule<Env, Key extends keyof Env>(): FunctionModule<Env, Key>;
class Holder<Env> {
  get fn(): FunctionModule<Env, keyof Env> { return createFunctionModule(); }
}
"#,
        "test.ts",
        opts(),
    );
    assert_eq!(
        real_diagnostics(&diags),
        Vec::<(u32, &str)>::new(),
        "renamed-binder control should be clean"
    );
}

/// Fallback control: neither intersection member is a naked type parameter
/// (both `{tag: string}` and `{extra: boolean}` are structured object
/// types), so the same-arity guard must not fire and both target members
/// must still broadly match against the source's single structured member —
/// exactly the pre-existing "structured members" behavior the fix leaves
/// alone. If the guard incorrectly treated a structured member as naked, one
/// of the two upper bounds would be dropped and this would spuriously fail
/// with a missing-property diagnostic.
#[test]
fn structured_intersection_members_still_broadly_match() {
    let diags = check_source(
        r#"
interface Tagged { tag: string }
interface Extra { extra: boolean }
declare function make(): Tagged & Extra;
function use(): Tagged & Extra {
    return make();
}
"#,
        "test.ts",
        opts(),
    );
    assert_eq!(
        real_diagnostics(&diags),
        Vec::<(u32, &str)>::new(),
        "structured (non-naked) intersection members should still combine correctly"
    );
}

/// Free-function control: the same shape without a class getter — a
/// top-level generic function's declared return-type annotation supplies the
/// contextual type instead of a getter's. Confirms the fix is keyed on the
/// intersection arity/position, not on being read through a getter.
#[test]
fn contextual_call_infers_positional_member_through_plain_function_return() {
    let diags = check_source(
        r#"
interface Expr<T> { __exprType: T }
interface FunctionModule<DB, TB extends keyof DB> {
  <O>(name: string): O;
  jsonAgg<T extends (TB & string) | Expr<unknown>>(
    table: T,
  ): T extends TB ? DB[T][] : T extends Expr<infer O> ? O[] : never;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
function fn<XX>(): FunctionModule<XX, keyof XX> {
  return createFunctionModule();
}
"#,
        "test.ts",
        opts(),
    );
    assert_eq!(
        real_diagnostics(&diags),
        Vec::<(u32, &str)>::new(),
        "free-function contextual-return control should be clean"
    );
}

/// Genuine mismatch must still be reported: the fix must not silence a real
/// error by mis-binding the naked member to the wrong positional
/// counterpart. `boolean` cannot generally satisfy `TB extends keyof DB` for
/// an unconstrained `DB`, so inferring `TB` from this contextual type must
/// still surface a real diagnostic instead of going quiet.
#[test]
fn genuine_constraint_violation_still_reported() {
    let diags = check_source(
        r#"
interface Expr<T> { __exprType: T }
interface FunctionModule<DB, TB extends keyof DB> {
  <O>(name: string): O;
  jsonAgg<T extends (TB & string) | Expr<unknown>>(
    table: T,
  ): T extends TB ? DB[T][] : T extends Expr<infer O> ? O[] : never;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
class Holder<DB> {
  get fn(): FunctionModule<DB, boolean> { return createFunctionModule(); }
}
"#,
        "test.ts",
        opts(),
    );
    let real = real_diagnostics(&diags);
    assert!(
        !real.is_empty(),
        "expected a real diagnostic for the FunctionModule<DB, boolean> constraint violation, got none"
    );
}
