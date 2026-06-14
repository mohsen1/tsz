//! Tests for TS2536 suppression when indexing an object with a plain string index
//! signature using an index type that is provably within `string | number`.
//!
//! Structural rule: a plain string index signature `{ [s: string]: V }` accepts both
//! string and number keys per JS coercion semantics. tsc suppresses TS2536 when the
//! index type is assignable to `string | number`; it emits TS2536 when the index type
//! could include `symbol` (e.g., an unconstrained type parameter or `keyof T` for
//! generic `T`).

use tsz_checker::test_utils::{
    DEFAULT_LIB_NAMES, check_source_diagnostics, check_source_with_libs, load_lib_files,
    strict_checker_options,
};

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

fn diag_summary(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn strict_codes_with_libs(source: &str) -> Vec<u32> {
    let libs = load_lib_files(DEFAULT_LIB_NAMES);
    check_source_with_libs(source, "test.ts", strict_checker_options(), &libs)
        .into_iter()
        .map(|d| d.code)
        .filter(|code| *code != 2318)
        .collect()
}

// ── TYPE-LEVEL indexed access: K extends string (assignable to string|number) ──

/// `type A<K extends string> = Obj[K]` — index within string|number → no TS2536.
#[test]
fn type_level_k_extends_string_no_ts2536() {
    let source = r#"
interface Obj { [s: string]: number; }
type A<K extends string> = Obj[K];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Obj[K extends string] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Anti-hardcoding: same rule with type-parameter named `Key` instead of `K`.
#[test]
fn type_level_key_extends_string_no_ts2536() {
    let source = r#"
interface Store { [s: string]: boolean; }
type B<Key extends string> = Store[Key];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Store[Key extends string] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// `K extends string | number` is also within string|number → no TS2536.
#[test]
fn type_level_k_extends_string_or_number_no_ts2536() {
    let source = r#"
interface Dict { [s: string]: unknown; }
type C<K extends string | number> = Dict[K];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Dict[K extends string | number] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Concrete `keyof Obj` on an object with a string index signature.
/// `keyof { [s: string]: V; extra: T }` = `string | number`, which is within
/// `string | number` → no TS2536.
#[test]
fn type_level_concrete_keyof_string_indexed_no_ts2536() {
    let source = r#"
interface Env { [s: string]: string; HOME: string; }
type EnvKey = keyof Env;
type V = Env[EnvKey];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Env[keyof Env] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Mapped type `{ [K in keyof U]: Obj[K] }` where `U extends string[]`.
/// `keyof string[]` ⊆ `string | number` → no TS2536.
/// This is the actual conformance regression that motivated this fix.
#[test]
fn type_level_mapped_keyof_array_constraint_no_ts2536() {
    let source = r#"
interface Obj { [s: string]: number; }
type Mapped<U extends string[]> = { [K in keyof U]: Obj[K] };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Obj[K in keyof (U extends string[])] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Anti-hardcoding: same rule with type-parameter named `Arr` instead of `U`.
#[test]
fn type_level_mapped_keyof_array_constraint_renamed_param_no_ts2536() {
    let source = r#"
interface Registry { [name: string]: object; version: string; }
type Snapshot<Arr extends unknown[]> = { [Idx in keyof Arr]: Registry[Idx] };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Registry[Idx in keyof (Arr extends unknown[])] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

#[test]
fn strict_mapped_apparent_type_assignment_keeps_only_ts2322() {
    let source = r#"
type Obj = {
    [s: string]: number;
};

type SourceFn = <T>(target: { [K in keyof T]: T[K] }) => void;
type TargetFn = <Arr extends string[]>(source: { [Idx in keyof Arr]: Obj[Idx] }) => void;

declare let sourceFn: SourceFn;
declare let targetFn: TargetFn;
targetFn = sourceFn;
"#;
    let codes = strict_codes_with_libs(source);
    assert_eq!(
        codes,
        vec![2322],
        "strict function assignment should keep TS2322 but suppress false TS2536; got {codes:?}"
    );
}

// ── negative: objects without a plain string index signature are not covered ───

/// An object with only named properties (no index signature at all) does not have
/// a plain string index sig. `K extends string` is still invalid → TS2536.
/// This proves the `has_plain_string_index` guard (`key_type == STRING`) is active:
/// without it, `is_assignable_to(K extends string, string | number)` would
/// incorrectly suppress TS2536 for ANY object with any index signature.
#[test]
fn type_level_no_index_sig_still_emits_ts2536() {
    let source = r#"
interface Exact { x: string; y: number; }
type Bad4<K extends string> = Exact[K];
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        count(&diags, 2536) > 0,
        "Exact (no index sig) with K extends string must still emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

// ── TYPE-LEVEL indexed access: TS2536 must fire (index can include symbol) ────

/// Unconstrained `K` can be any type including symbol → TS2536.
#[test]
fn type_level_unconstrained_k_emits_ts2536() {
    let source = r#"
interface Obj { [s: string]: number; }
type Bad<K> = Obj[K];
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        count(&diags, 2536) > 0,
        "Obj[K] (unconstrained K) must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// `K extends keyof T` for generic `T` can include symbol keys → TS2536.
#[test]
fn type_level_k_extends_keyof_generic_t_emits_ts2536() {
    let source = r#"
interface Obj { [s: string]: number; }
type Bad2<T, K extends keyof T> = Obj[K];
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        count(&diags, 2536) > 0,
        "Obj[K extends keyof T] (generic T) must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Anti-hardcoding: same rule with `Prop extends keyof Src` names.
#[test]
fn type_level_prop_extends_keyof_generic_src_emits_ts2536() {
    let source = r#"
interface Config { [key: string]: unknown; debug: boolean; }
type Bad3<Src, Prop extends keyof Src> = Config[Prop];
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        count(&diags, 2536) > 0,
        "Config[Prop extends keyof Src] (generic Src) must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

// ── VALUE-LEVEL: concrete keyof on string-indexed objects ─────────────────────

/// Value-level element access with `k: keyof Env` where Env has a string index
/// signature. `keyof Env` = `string | number` → no TS2536.
#[test]
fn value_level_concrete_keyof_string_indexed_no_ts2536() {
    let source = r#"
interface Env { [s: string]: string; HOME: string; }
type EnvKey = keyof Env;
declare const env: Env;
declare const k: EnvKey;
const v = env[k];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "env[keyof Env] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Inline `keyof Config` in a function parameter.
#[test]
fn value_level_inline_keyof_string_indexed_no_ts2536() {
    let source = r#"
interface Config { [key: string]: unknown; debug: boolean; }
declare const cfg: Config;
function readProp(k: keyof Config) {
    return cfg[k];
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "cfg[keyof Config] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

// ── boundary: TS2322 is not suppressed by the TS2536 fix ─────────────────────

/// The suppression is narrow: TS2536 is suppressed for valid key types, but TS2322
/// must still fire when assigning a mismatched value type.
#[test]
fn ts2322_not_suppressed_on_string_indexed_write() {
    let source = r#"
interface NumStore { [s: string]: number; }
declare const store: NumStore;
declare const k: keyof { x: string; y: string };
const bad: string = "hello";
store[k] = bad;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "NumStore[keyof T] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
    let mismatch = count(&diags, 2322) + count(&diags, 2345);
    assert!(
        mismatch > 0,
        "assigning string to NumStore[k] (value type number) must emit TS2322/TS2345; got: {:?}",
        diag_summary(&diags)
    );
}

// ── Unconstrained type-parameter object indexed by a concrete key (#13212) ──
//
// A bare/unconstrained type parameter has the implicit constraint `unknown` in
// tsc, so `keyof T` is `keyof unknown` = `never`: no concrete property key is a
// member, and `T[key]` is a TS2536. tsz's element-indexability classifier
// permissively treats every type parameter as string/number indexable (to keep
// value-space `T[k]` from a spurious TS7053), which previously suppressed TS2536
// here. The fix only honors that permissive verdict for explicitly-constrained
// parameters (which resolve to a concrete key space) — a bare parameter falls
// through to TS2536, matching tsc.

/// `type X<B> = B['out']` — string-literal key on an unconstrained param → TS2536.
#[test]
fn unconstrained_param_string_literal_index_emits_ts2536() {
    let source = r#"
type X<B> = B['out'];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        1,
        "B['out'] on unconstrained B must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Anti-hardcoding: same rule with a differently-named parameter and key.
#[test]
fn unconstrained_renamed_param_string_literal_index_emits_ts2536() {
    let source = r#"
type Lookup<Shape> = Shape['field'];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        1,
        "Shape['field'] on unconstrained Shape must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Numeric-literal key on an unconstrained param → TS2536.
#[test]
fn unconstrained_param_numeric_literal_index_emits_ts2536() {
    let source = r#"
type X<B> = B[0];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        1,
        "B[0] on unconstrained B must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// `string` / `number` primitive key on an unconstrained param → TS2536.
#[test]
fn unconstrained_param_primitive_key_index_emits_ts2536() {
    for source in ["type X<B> = B[string];", "type X<B> = B[number];"] {
        let diags = check_source_diagnostics(source);
        assert_eq!(
            count(&diags, 2536),
            1,
            "{source} must emit TS2536; got: {:?}",
            diag_summary(&diags)
        );
    }
}

/// A union of concrete literal keys on an unconstrained param → TS2536.
#[test]
fn unconstrained_param_union_literal_index_emits_ts2536() {
    let source = r#"
type X<B> = B['a' | 'b'];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        1,
        "B['a' | 'b'] on unconstrained B must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// A parameter whose constraint is itself an unconstrained parameter is also
/// opaque (`keyof` = `never`), so a concrete key still emits TS2536.
#[test]
fn param_constrained_to_unconstrained_param_emits_ts2536() {
    let source = r#"
type X<A, B extends A> = B['out'];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        1,
        "B['out'] where B extends unconstrained A must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// `B extends unknown` makes the implicit constraint explicit; tsc emits TS2536
/// identically — the fix must not change this already-correct case.
#[test]
fn param_extends_unknown_concrete_index_emits_ts2536() {
    let source = r#"
type X<B extends unknown> = B['x'];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        1,
        "B['x'] where B extends unknown must emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

// ── Negative cases: the fix must not over-fire ──

/// A concrete key present in the parameter's constraint stays valid (no TS2536).
#[test]
fn constrained_param_present_key_no_ts2536() {
    let source = r#"
type X<B extends { a: 1 }> = B['a'];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "B['a'] where B extends {{ a: 1 }} must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// A parameter constrained to a string-index signature accepts any string key.
#[test]
fn constrained_param_string_index_signature_no_ts2536() {
    let source = r#"
type X<B extends Record<string, number>> = B['foo'];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "B['foo'] where B extends Record<string, number> must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// `T[keyof T]`, `T[never]`, and `T[K extends keyof T]` are deferred-valid on a
/// bare parameter and must stay clean (the index carries the type parameter).
#[test]
fn unconstrained_param_deferred_index_no_ts2536() {
    for source in [
        "type X<B> = B[keyof B];",
        "type X<B> = B[never];",
        "type X<B, K extends keyof B> = B[K];",
    ] {
        let diags = check_source_diagnostics(source);
        assert_eq!(
            count(&diags, 2536),
            0,
            "{source} must not emit TS2536; got: {:?}",
            diag_summary(&diags)
        );
    }
}

/// Value-space `T[k]` on a bare parameter must still report TS7053 (and never a
/// spurious TS2536) — the permissive classifier behavior is preserved there.
#[test]
fn value_space_bare_param_index_still_ts7053_not_ts2536() {
    let source = r#"
function f<T>(o: T) { return o['x']; }
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "value-space o['x'] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
    assert!(
        count(&diags, 7053) > 0,
        "value-space o['x'] on bare T must emit TS7053; got: {:?}",
        diag_summary(&diags)
    );
}

// ── Conditional true-branch object constraints ──

/// In `T extends O ? T['m'] : never`, the true branch treats `T` as constrained
/// by `O`, so the present key `m` must not report TS2536.
#[test]
fn conditional_true_branch_present_property_no_ts2536() {
    let source = r#"
type M = { p: string };
type O = { m: () => M };
type X<T extends M> = T;
type MyReturnType<F> = F extends (...args: any[]) => infer R ? R : never;
type FFG<T> = T extends O ? X<MyReturnType<T['m']>> : never;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "T['m'] in the true branch of T extends O must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Anti-hardcoding: same branch-constraint rule with renamed binders and key.
#[test]
fn conditional_true_branch_renamed_property_no_ts2536() {
    let source = r#"
type Boxed = { value: number };
type Shape = { read: () => Boxed };
type Wrap<Item extends Boxed> = Item;
type ResultOf<F> = F extends (...args: any[]) => infer R ? R : never;
type Lookup<Candidate> = Candidate extends Shape ? Wrap<ResultOf<Candidate['read']>> : never;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Candidate['read'] in the true branch of Candidate extends Shape must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Array-like true-branch constraints permit numeric indexed access.
#[test]
fn conditional_true_branch_array_numeric_index_no_ts2536() {
    let source = r#"
type KnockedOut<T> = T extends any[] ? T[number] : T;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "T[number] in the true branch of T extends any[] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}
