//! M12: a generic type parameter inferred from a type-predicate argument
//! (`guard: (raw: any) => raw is V`) must not be left `unknown` by the sibling
//! context-sensitive callbacks (superjson `makeCodec`). The predicate function's
//! `any`-typed parameter makes it "contextually sensitive" at the solver level,
//! so it is routed through `constrain_sensitive_function_return_types`, which
//! previously constrained only the return type and dropped the type predicate.
//! `V` therefore stayed `unknown` during Round-1 contextual typing, so a sibling
//! callback whose parameter references `V` was typed against `unknown` (TS18046).
//! The fix mirrors the predicate branch of `infer_signatures` in that seam.
//!
//! The witnesses use a user-defined predicate target (not `bigint`) so they are
//! independent of the default-lib available to `check_source`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "repro.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

fn count_code(diags: &[u32], expected: u32) -> usize {
    diags.iter().filter(|&&c| c == expected).count()
}

/// Core witness (renamed binders): `V` is inferred `Foo` from the predicate; the
/// `dec` callback returning `... as any` must not collapse `V` to `unknown`, so
/// `enc`'s `raw` is `Foo` and `raw.kind` is clean (no TS18046).
#[test]
fn predicate_inference_not_poisoned_by_any_returning_callback() {
    let diags = codes(
        r#"
type Kind = 'num' | 'gap' | 'big';
interface Env { limit: number; }
interface Foo { kind: 'foo'; n: number; }
function wrap<V, E, M extends Kind>(
  guard: (raw: any, env: Env) => raw is V,
  mark: M,
  enc: (raw: V, env: Env) => E,
  dec: (packed: E, env: Env) => V
) { return { guard, mark, enc, dec }; }
const isFoo = (raw: any): raw is Foo => true;
const codec = wrap(
  isFoo,
  'big',
  raw => raw.kind,
  packed => { return packed as any; }
);
export { codec };
"#,
    );
    assert_eq!(
        count_code(&diags, 18046),
        0,
        "type-predicate inference must not be poisoned to unknown; got {diags:#?}"
    );
}

/// Parity witness: the inferred `V` must be exactly `Foo` (tsc parity), not
/// `any`. Assigning `dec`'s `Foo` result to a `string` therefore fails with
/// TS2322 (as in tsc). This guards against "fix by widening V to any" — which
/// would clear TS18046 but silently diverge from tsc.
#[test]
fn predicate_inference_yields_the_concrete_type_not_any() {
    let diags = codes(
        r#"
type Kind = 'num' | 'gap' | 'big';
interface Env { limit: number; }
interface Foo { kind: 'foo'; n: number; }
function wrap<V, E, M extends Kind>(
  guard: (raw: any, env: Env) => raw is V,
  mark: M,
  enc: (raw: V, env: Env) => E,
  dec: (packed: E, env: Env) => V
) { return { guard, mark, enc, dec }; }
const isFoo = (raw: any): raw is Foo => true;
const codec = wrap(isFoo, 'big', raw => raw.kind, packed => { return packed as any; });
const revealV: string = codec.dec('x', { limit: 1 });
export { codec, revealV };
"#,
    );
    assert_eq!(
        count_code(&diags, 2322),
        1,
        "V must infer to the concrete Foo (assigning to string fails), not any; got {diags:#?}"
    );
}

/// Anti-hardcoding / negative control: DIRECT two-parameter inference
/// `f<T>(a: T, b: T)` called with `(anyVal, 5)` must still infer `T = any`
/// (tsc parity) — the predicate fix must NOT introduce a blanket "drop any"
/// that would make this infer `number` and reject the `string` assignment.
#[test]
fn direct_param_any_candidate_still_infers_any() {
    let diags = codes(
        r#"
declare function f<T>(a: T, b: T): T;
declare const anyVal: any;
const r: string = f(anyVal, 5);
export { r };
"#,
    );
    assert_eq!(
        count_code(&diags, 2322),
        0,
        "direct two-param inference must keep T = any (no false TS2322); got {diags:#?}"
    );
}

/// Negative control: with NO type predicate, a generic inferred solely from a
/// context-sensitive callback's `any`-typed return stays `unknown` in BOTH
/// compilers (tsc parity). The predicate fix must not change this.
#[test]
fn non_predicate_lone_any_callback_stays_unknown() {
    let diags = codes(
        r#"
type Kind = 'num' | 'gap' | 'big';
interface Env { limit: number; }
function wrap<V, E, M extends Kind>(
  guard: (raw: any, env: Env) => boolean,
  mark: M,
  enc: (raw: V, env: Env) => E,
  dec: (packed: E, env: Env) => V
) { return { guard, mark, enc, dec }; }
const codec = wrap(
  (raw: any) => true,
  'big',
  raw => raw.k,
  packed => { return packed as any; }
);
export { codec };
"#,
    );
    assert_eq!(
        count_code(&diags, 18046),
        1,
        "without a predicate, a lone any-callback-inferred param stays unknown (tsc parity); got {diags:#?}"
    );
}
