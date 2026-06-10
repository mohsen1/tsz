//! Regression tests for `(typeof X)[number]` over spread-built const tuples.
//!
//! Structural rule: when a `const X = [...]` initializer contains an array
//! spread and `typeof X` is consumed by an indexed access (or `keyof`) in a
//! type-alias body, the alias may be lowered before `X`'s initializer type is
//! computed, so the `IndexAccess(TypeQuery(X), number)` evaluation defers.
//! That deferred identity result must not be committed to the project-wide
//! `closed_eval_cache`: a `TypeQuery` operand resolves through mutable checker
//! state (`symbol_types`), so it is only closed-eval cacheable once the
//! resolver can already produce the value type (mirroring the `Lazy` operand
//! arm). Before the fix, the poisoned cache entry made the alias permanently
//! opaque — it failed assignability in both directions, surfacing in kysely as
//! ~146 false TS2322/TS2345/TS2416/TS2349/TS2394 through the
//! `ComparisonOperator`/`BinaryOperator` `(typeof OPS)[number]` aliases.
//!
//! Per the anti-hardcoding gate, binder names vary across cases so the tests
//! pin the structural rule, not a spelling.

use crate::test_utils::check_source_diagnostics;

fn codes(src: &str) -> Vec<u32> {
    let mut v: Vec<u32> = check_source_diagnostics(src)
        .iter()
        .map(|d| d.code)
        .collect();
    v.sort_unstable();
    v
}

// ── Witness: spread + trailing element (kysely BINARY_OPERATORS shape) ──────

#[test]
fn typeof_spread_const_tuple_index_access_is_element_union() {
    // tsc: clean. `(typeof WIDE)[number]` must evaluate to the union of all
    // element literals, including those contributed by the spread.
    assert_eq!(
        codes(
            r#"
const NARROW = ['eq', 'ne'] as const;
const WIDE = [...NARROW, 'lt', 'gt'] as const;
type NarrowOp = (typeof NARROW)[number];
type WideOp = (typeof WIDE)[number];
declare const n: NarrowOp;
const w: WideOp = n;
const back: 'eq' | 'ne' | 'lt' | 'gt' = w;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Spread position variants ────────────────────────────────────────────────

#[test]
fn typeof_spread_only_const_tuple_index_access() {
    assert_eq!(
        codes(
            r#"
const BASE = ['a', 'b'] as const;
const COPY = [...BASE] as const;
type CopyT = (typeof COPY)[number];
declare const c: CopyT;
const k: 'a' | 'b' = c;
"#
        ),
        Vec::<u32>::new()
    );
}

#[test]
fn typeof_leading_element_then_spread_const_tuple_index_access() {
    assert_eq!(
        codes(
            r#"
const TAIL = ['y', 'z'] as const;
const FULL = ['x', ...TAIL] as const;
type FullT = (typeof FULL)[number];
declare const f: FullT;
const k: 'x' | 'y' | 'z' = f;
"#
        ),
        Vec::<u32>::new()
    );
}

#[test]
fn typeof_two_spreads_const_tuple_index_access() {
    // The kysely shape spreads two const arrays plus extra literals.
    assert_eq!(
        codes(
            r#"
const CMP = ['=', '!='] as const;
const ARITH = ['+', '-'] as const;
const ALL = [...CMP, ...ARITH, '&&'] as const;
type Cmp = (typeof CMP)[number];
type All = (typeof ALL)[number];
declare const c: Cmp;
const a: All = c;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Control: no spread (must keep working) ──────────────────────────────────

#[test]
fn typeof_plain_const_tuple_index_access_control() {
    assert_eq!(
        codes(
            r#"
const PLAIN = ['p', 'q'] as const;
type PlainT = (typeof PLAIN)[number];
declare const p: PlainT;
const k: 'p' | 'q' = p;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Negative control: incompatible literal still errors ─────────────────────

#[test]
fn typeof_spread_const_tuple_index_access_rejects_foreign_literal() {
    // tsc: TS2322 — 'other' is not a member of the element union.
    assert_eq!(
        codes(
            r#"
const INNER = ['u', 'v'] as const;
const OUTER = [...INNER, 'w'] as const;
type OuterT = (typeof OUTER)[number];
declare const bad: 'other';
const k: OuterT = bad;
"#
        ),
        vec![2322]
    );
}

#[test]
fn typeof_spread_const_tuple_index_access_rejects_widened_string() {
    // tsc: TS2322 — `string` is not assignable to the literal union.
    assert_eq!(
        codes(
            r#"
const ONE = ['m'] as const;
const TWO = [...ONE, 'n'] as const;
type TwoT = (typeof TWO)[number];
declare const s: string;
const k: TwoT = s;
"#
        ),
        vec![2322]
    );
}

// ── Union-with-interface alias (kysely OperatorExpression shape, TS2345) ────

#[test]
fn typeof_spread_operator_union_alias_call_argument() {
    // tsc: clean. The narrow operator union (plus an interface member) must be
    // assignable to the wide one — the kysely
    // `ComparisonOperatorExpression -> BinaryOperatorExpression` call path.
    assert_eq!(
        codes(
            r#"
const SMALL = ['like', 'match'] as const;
const BIG = [...SMALL, 'regexp'] as const;
interface Expr<T> {
  readonly expressionType?: T;
}
type SmallOp = (typeof SMALL)[number] | Expr<unknown>;
type BigOp = (typeof BIG)[number] | Expr<unknown>;
declare function take(op: BigOp): void;
declare const s: SmallOp;
take(s);
"#
        ),
        Vec::<u32>::new()
    );
}

// ── keyof typeof variant (same TypeQuery-operand family) ────────────────────

#[test]
fn keyof_typeof_spread_const_object_member_access() {
    // `keyof (typeof obj)` routed through a spread-built const tuple member —
    // the keyof/index family shares the closed-eval operand gate.
    assert_eq!(
        codes(
            r#"
const ROOTS = ['r1', 'r2'] as const;
const TABLE = { keys: [...ROOTS, 'r3'] } as const;
type KeyList = (typeof TABLE)['keys'][number];
declare const k: KeyList;
const ok: 'r1' | 'r2' | 'r3' = k;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Use-before-definition order variant ─────────────────────────────────────

#[test]
fn typeof_spread_const_tuple_alias_declared_before_const() {
    // The alias appears before the const declarations: the type query must
    // still resolve once the initializer type is computed (hoisting order).
    assert_eq!(
        codes(
            r#"
type LateT = (typeof LATE)[number];
const EARLY = ['e1'] as const;
const LATE = [...EARLY, 'e2'] as const;
declare const l: LateT;
const k: 'e1' | 'e2' = l;
"#
        ),
        Vec::<u32>::new()
    );
}
