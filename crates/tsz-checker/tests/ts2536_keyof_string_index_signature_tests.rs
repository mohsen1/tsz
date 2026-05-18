//! Tests for TS2536 suppression when indexing an object with a plain string index
//! signature using an index type that is provably within `string | number`.
//!
//! Structural rule: a plain string index signature `{ [s: string]: V }` accepts both
//! string and number keys per JS coercion semantics. tsc suppresses TS2536 when the
//! index type is assignable to `string | number`; it emits TS2536 when the index type
//! could include `symbol` (e.g., an unconstrained type parameter or `keyof T` for
//! generic `T`).

use tsz_checker::test_utils::check_source_diagnostics;

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

fn diag_summary(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
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
