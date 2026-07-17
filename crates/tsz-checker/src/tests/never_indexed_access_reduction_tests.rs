//! Regression tests: indexing `never` reduces to `never` (M5 valibot family).
//!
//! Structural rule: `never` has every property, so `never[K]` is `never` for
//! any key `K` — exactly as `tsc` reduces it. Before the fix the solver's
//! indexed-access evaluator had no `never` object rule, so `never[K]` fell
//! through to a deferred `IndexAccess(never, K)` that never reduced. A mapped or
//! conditional utility that bottoms out at `never` — the canonical case is
//! `T[keyof T]` with `keyof {} = never`, feeding an alias chain like
//! `NonNullable<never['meta']>['trouble']` — then stayed unreduced and failed
//! downstream constraint / assignability checks (false TS2344 / TS2322). This
//! was the valibot `TwinRecord<{}>` M5 root: an interface whose heritage
//! type-argument is a mapped-over-empty utility must relate to the same base at
//! `unknown`/top arguments.
//!
//! Binder names are arbitrary (no valibot source text); the outcome tracks the
//! `never`-reduction shape, not any identifier.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn check(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

// `never[K]` in a type-parameter-constraint position must reduce to `never`
// (which satisfies every constraint); a deferred `IndexAccess(never, K)` would
// spuriously fail with TS2344.
#[test]
fn never_indexed_access_satisfies_constraint_no_ts2344() {
    let codes = check(
        r#"
type ChkStr<T extends string> = T;
type ChkObj<T extends { readonly kind: string }> = T;

// Direct: indexing `never` by a literal / by `keyof {}` = never.
export type A = ChkStr<never['anything']>;
export type B = ChkStr<{}[keyof {}]>;
// Chained through NonNullable + a second indexed access (the valibot alias
// chain `NonNullable<T['meta']>['trouble']` with T = {}[keyof {}] = never).
export type C = ChkObj<NonNullable<never['meta']>['trouble']>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "indexing `never` must reduce to `never` and satisfy the constraint (no TS2344). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "`never[K]` must be assignable to `never` (no TS2322). Got: {codes:?}"
    );
}

// End-to-end M5 witness (reduced valibot `TwinRecord<{}>`): an interface whose
// third heritage type-argument is `Rec | FieldsTrouble<F>` where
// `FieldsTrouble<{}> = TroubleOf<{}[keyof {}]>` bottoms out at `never`. With the
// empty field object the whole thing must reduce and satisfy the `AnyCore |
// AnyCoreAsync` union constraint — tsc-clean.
#[test]
fn mapped_over_empty_heritage_arg_relates_to_union_constraint_no_ts2344() {
    let codes = check(
        r#"
interface Tune<TT> { note?: ((t: TT) => string) | string; }
interface Trouble<TC> extends Tune<Trouble<TC>> { kind: string; cause: TC; }
interface RecTrouble extends Trouble<unknown> { kind: 'core'; }

interface Core<I, O, T extends Trouble<unknown>> {
  maker: (...a: any[]) => Core<unknown, unknown, Trouble<unknown>>;
  lazy: false;
  go: (d: unknown) => O;
  meta?: { input: I; output: O; trouble: T };
}
interface CoreAsync<I, O, T extends Trouble<unknown>>
  extends Omit<Core<I, O, T>, 'maker' | 'lazy' | 'go'> {
  maker: (...a: any[]) => Core<unknown, unknown, Trouble<unknown>> | CoreAsync<unknown, unknown, Trouble<unknown>>;
  lazy: true;
  go: (d: unknown) => Promise<O>;
}
type AnyCore = Core<unknown, unknown, Trouble<unknown>>;
type AnyCoreAsync = CoreAsync<unknown, unknown, Trouble<unknown>>;

interface Fields { [k: string]: AnyCore | AnyCoreAsync; }
type TroubleOf<T extends AnyCore | AnyCoreAsync> = NonNullable<T['meta']>['trouble'];
type FieldsTrouble<F extends Fields> = TroubleOf<F[keyof F]>;
type MapK<T> = { [K in keyof T]: unknown };
type ShapeIn<F extends Fields> = { [K in keyof MapK<F>]: MapK<F>[K] } & {};

interface Twin<F extends Fields>
  extends CoreAsync<ShapeIn<F>, ShapeIn<F>, RecTrouble | FieldsTrouble<F>> {}
type Sat<T extends AnyCore | AnyCoreAsync> = T;
export type P = Sat<Twin<{}>>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "mapped-over-empty heritage arg must reduce and satisfy the union constraint (no TS2344). Got: {codes:?}"
    );
}
