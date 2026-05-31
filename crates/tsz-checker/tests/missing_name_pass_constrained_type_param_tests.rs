//! Regression coverage for the early `check_type_for_missing_names` pre-pass.
//!
//! When a method or callable signature declares
//! `<K extends keyof T[U]>` and returns a self-referential generic that
//! threads `K` through a mapped type (`Q<U, { [P in K]: T[U][P] }>` or any
//! other position that triggers mapped-type-constraint validation), the
//! pre-pass used to push `K` into `type_parameter_scope` without its
//! declared constraint. Subsequent eager validation of the type-argument
//! subtree would then see `K`'s constraint as `None` and incorrectly report:
//!
//! - TS2322 "Type 'K' is not assignable to type 'string | number | symbol'"
//! - TS2536 "Type 'P' cannot be used to index type 'T[U]'"
//!
//! Genuinely unconstrained type parameters (`<T>` with no `extends` clause)
//! must still report TS2322 — the fix is about preserving the **declared**
//! constraint, not silencing the diagnostic for unconstrained params.
//!
//! Structural rule: any `push_*_type_parameters` helper that runs validation
//! against the AST subtree must resolve declared constraints before the
//! validation walk so the scope reflects the user-written extends clauses.

use tsz_checker::context::CheckerOptions;

fn check(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn count_with_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

#[test]
fn recursive_self_returning_generic_with_indexed_keyof_constraint_passes() {
    // The kysely-style minimal repro. The mapped key `K` is constrained by
    // `keyof T[TB]`, where `TB` is the surrounding interface's type
    // parameter. Both diagnostics must be silent — `K`'s constraint is a
    // keyof, which is always a subtype of `string | number | symbol`.
    let source = r#"
interface T { user: { id: number; name: string } }
interface Q<TB extends keyof T, R> {
  select<K extends keyof T[TB]>(): Q<TB, { [P in K]: T[TB][P] }>;
}
"#;
    let diags = check(source);
    assert_eq!(
        count_with_code(&diags, 2322),
        0,
        "TS2322 must not fire for K constrained by keyof T[TB]: {diags:?}"
    );
    assert_eq!(
        count_with_code(&diags, 2536),
        0,
        "TS2536 must not fire for P indexing T[TB]: {diags:?}"
    );
}

#[test]
fn intersection_in_return_type_does_not_trip_mapped_key_validation() {
    // Variant from kysely's `select(['id'])` shape — `R & { [P in K]: ... }`.
    let source = r#"
interface DB { user: { id: number; name: string } }

interface Q<DB, TB extends keyof DB, R> {
  select<K extends keyof DB[TB]>(cols: K[]): Q<DB, TB, R & { [P in K]: DB[TB][P] }>;
}

declare const q: Q<DB, 'user', {}>;
const r = q.select(['id']);
"#;
    let diags = check(source);
    assert_eq!(
        count_with_code(&diags, 2322),
        0,
        "TS2322 must not fire on the return-type mapped key: {diags:?}"
    );
    assert_eq!(
        count_with_code(&diags, 2536),
        0,
        "TS2536 must not fire on the mapped property type: {diags:?}"
    );
}

#[test]
fn renamed_type_parameters_still_pass() {
    // Anti-hardcoding guard: the fix must not depend on identifier
    // spellings. Rename every bound name and the result must be identical.
    let source = r#"
interface Records { account: { id: number; label: string } }
interface View<Table extends keyof Records, Acc> {
  pick<Col extends keyof Records[Table]>(): View<Table, Acc & { [Field in Col]: Records[Table][Field] }>;
}
"#;
    let diags = check(source);
    assert_eq!(
        count_with_code(&diags, 2322),
        0,
        "renamed type parameters must not regress: {diags:?}"
    );
    assert_eq!(
        count_with_code(&diags, 2536),
        0,
        "renamed mapped key must not regress: {diags:?}"
    );
}

#[test]
fn genuinely_unconstrained_type_param_used_as_mapped_key_still_reports() {
    // Negative case: an unconstrained type parameter is not a valid mapped
    // key — TS2322 must still fire. The fix preserves declared
    // constraints; it must not silence missing ones.
    let source = r#"
type X<T> = { [P in T]: any };
"#;
    let diags = check(source);
    assert!(
        count_with_code(&diags, 2322) >= 1,
        "unconstrained type parameter must still emit TS2322: {diags:?}"
    );
}

#[test]
fn function_type_with_indexed_keyof_constraint_returning_mapped_type() {
    // Pure function-type variant (no enclosing interface). Forces the same
    // pre-pass path through `push_missing_name_type_parameters` from a
    // function-type literal context.
    let source = r#"
interface DB { user: { id: number; name: string } }
type Pick2<TB extends keyof DB> = <K extends keyof DB[TB]>(cols: K[]) => { [P in K]: DB[TB][P] };
declare const f: Pick2<'user'>;
const v = f(['id']);
"#;
    let diags = check(source);
    assert_eq!(
        count_with_code(&diags, 2322),
        0,
        "function-type variant must not emit TS2322: {diags:?}"
    );
    assert_eq!(
        count_with_code(&diags, 2536),
        0,
        "function-type variant must not emit TS2536: {diags:?}"
    );
}

#[test]
fn two_dependent_method_type_parameters_resolve_each_others_constraints() {
    // Sibling type parameter chain: K1 constrains K2. Both must be resolved
    // by pass-2 of the missing-name push so mapped-type validation against
    // K2's constraint sees a real constraint type, not a provisional `None`.
    let source = r#"
interface DB { user: { id: number; name: string } }
interface Q<TB extends keyof DB> {
  combine<K1 extends keyof DB[TB], K2 extends K1>(): { [P in K2]: DB[TB][P] };
}
"#;
    let diags = check(source);
    assert_eq!(
        count_with_code(&diags, 2322),
        0,
        "sibling type-param chain must not emit TS2322: {diags:?}"
    );
    assert_eq!(
        count_with_code(&diags, 2536),
        0,
        "sibling type-param chain must not emit TS2536: {diags:?}"
    );
}
