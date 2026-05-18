//! Tests for TS2536 suppression when the index is (or constrains to) a
//! `keyof T` expression and the object has a string index signature.
//!
//! Structural rule: a string index signature `{ [s: string]: V }` accepts every
//! string (and number, per JS semantics). `keyof T` always produces a subset of
//! `string | number | symbol`, so `Obj[keyof T]` is always valid when `Obj` has
//! a string index signature. tsc emits no TS2536 in this case.

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

// ── concrete keyof ────────────────────────────────────────────────────────────

/// Direct `keyof Obj` used to index an object with a string index signature.
/// tsc emits no TS2536.
#[test]
fn concrete_keyof_on_string_indexed_object_no_ts2536() {
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
        "Env[keyof Env] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Same shape with the `keyof` applied inline (no type alias) and a different
/// object variable name — proves the fix is not keyed on the identifier.
#[test]
fn inline_keyof_on_string_indexed_object_no_ts2536() {
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
        "Config[keyof Config] in function must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

// ── generic type parameter constrained to keyof T ────────────────────────────

/// Type parameter `K extends keyof T` used to index an object with a string
/// index signature. tsc suppresses TS2536 because `keyof T` ⊆ `string | number`.
#[test]
fn generic_param_k_extends_keyof_t_on_string_indexed_no_ts2536() {
    let source = r#"
interface Dictionary { [s: string]: number; }
function lookup<T, K extends keyof T>(dict: Dictionary, key: K): number {
    return dict[key];
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Dictionary[K extends keyof T] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Anti-hardcoding (§25): same rule with different bound-variable name `Key`
/// instead of `K`. The fix must not depend on the literal name `K`.
#[test]
fn generic_param_key_extends_keyof_t_on_string_indexed_no_ts2536() {
    let source = r#"
interface Store { [s: string]: boolean; }
function get<Src, Key extends keyof Src>(store: Store, key: Key): boolean {
    return store[key];
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Store[Key extends keyof Src] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

/// Anti-hardcoding: yet another name `Prop` and a different object shape.
#[test]
fn generic_param_prop_extends_keyof_obj_on_string_indexed_no_ts2536() {
    let source = r#"
interface Registry { [name: string]: object; version: string; }
function fetch<Obj, Prop extends keyof Obj>(registry: Registry, key: Prop): object {
    return registry[key];
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Registry[Prop extends keyof Obj] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

// ── Record<string, V> (equivalent to a plain string index signature) ──────────

/// `Record<string, V>` expands to a plain string index signature.
/// Indexing with `keyof T` must not produce TS2536.
#[test]
fn record_string_indexed_with_keyof_no_ts2536() {
    let source = r#"
function copy<T>(src: T, dst: Record<string, unknown>, key: keyof T): void {
    dst[key] = src[key];
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2536),
        0,
        "Record<string,unknown>[keyof T] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
}

// ── boundary: fix is narrow — TS2322 is not suppressed ───────────────────────

/// The suppression must be narrow: it only prevents TS2536 (invalid index type).
/// When the index IS a valid `keyof T` for a string-indexed object but the
/// value type is wrong, TS2322 must still fire.
/// (`Record<string, number>` only stores `number`, assigning a `string` fails.)
#[test]
fn ts2322_not_suppressed_on_string_indexed_write() {
    let source = r#"
interface NumStore { [s: string]: number; }
declare const store: NumStore;
declare const k: keyof { x: string; y: string };
const bad: string = "hello";
// Suppress TS2536 (key is valid) but NOT TS2322 (value is wrong type)
store[k] = bad;
"#;
    let diags = check_source_diagnostics(source);
    // TS2536 must be suppressed: key is keyof-derived, object has string index sig.
    assert_eq!(
        count(&diags, 2536),
        0,
        "NumStore[keyof T] must not emit TS2536; got: {:?}",
        diag_summary(&diags)
    );
    // TS2322 or TS2345 must fire: `string` is not assignable to `number`.
    let mismatch = count(&diags, 2322) + count(&diags, 2345);
    assert!(
        mismatch > 0,
        "assigning string to NumStore[k] (value type number) must emit TS2322/TS2345; got: {:?}",
        diag_summary(&diags)
    );
}
