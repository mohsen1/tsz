//! DTS emit: a `const` (or class `readonly`) initializer wrapped in a type
//! assertion is declared with the *asserted* type annotation, not the inline
//! `= literal` form.
//!
//! Structural rule: tsc emits the inline `= literal` form in `.d.ts` only for a
//! *fresh* literal const (`isLiteralConstDeclaration` / `isFreshLiteralType`).
//! A top-level type assertion — `as T`, `as const`, or `<T>` — gives the
//! declaration an asserted / non-fresh declared type, so tsc prints the `: T`
//! annotation:
//!
//! ```ts
//! const a = "hello" as const;  // declare const a: "hello";
//! const b = 5 as number;       // declare const b: number;
//! const g = "ok" satisfies string; // declare const g = "ok"; (satisfies stays inline)
//! ```
//!
//! Before the fix the declaration emitter unwrapped the assertion and emitted
//! the bare literal inline (`= "hello"` / `= 5`), which both loses the `: T`
//! form for `as const` and, for widening casts (`as number`, `as 1 | 2`),
//! emits an outright WRONG (narrower) type in the `.d.ts`.
//!
//! These run the full checker pipeline (the unit-level declaration-emit harness
//! uses an empty type cache and cannot infer the asserted/aliased const types).
//! Each behavioural case is exercised with more than one spelling so a
//! regression keyed on a particular literal value rather than the structural
//! shape would fail.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_const_assertion_dts_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

fn emit_dts(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let output = Command::new(tsz_bin)
        .args([
            "repro.ts",
            "--declaration",
            "--emitDeclarationOnly",
            "--strict",
            "--target",
            "es2020",
            "--lib",
            "es2020",
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz declaration emit");

    let dts = std::fs::read_to_string(temp.path.join("repro.d.ts")).unwrap_or_else(|_| {
        panic!(
            "expected repro.d.ts to be emitted.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    Some(dts)
}

macro_rules! dts_or_skip {
    ($name:expr, $src:expr) => {
        match emit_dts($name, $src) {
            Some(dts) => dts,
            None => {
                println!("skipping: tsz binary not found");
                return;
            }
        }
    };
}

// =============================================================================
// `as const` on a primitive literal -> the (non-fresh) literal annotation
// =============================================================================

#[test]
fn as_const_string_uses_literal_annotation() {
    let dts = dts_or_skip!("as_const_str", "export const a = \"hello\" as const;\n");
    assert!(
        dts.contains("export declare const a: \"hello\";"),
        "`as const` string must annotate the literal type, not inline it:\n{dts}"
    );
    assert!(
        !dts.contains("const a = "),
        "the inline `= literal` form must not be used for an `as const` const:\n{dts}"
    );
}

#[test]
fn as_const_number_and_bigint_and_negative_use_literal_annotation() {
    // Number, bigint, and negative literals all go non-fresh under `as const`.
    let dts = dts_or_skip!(
        "as_const_nums",
        "export const n = 5 as const;\nexport const b = 10n as const;\nexport const m = -3 as const;\n"
    );
    assert!(dts.contains("export declare const n: 5;"), "{dts}");
    assert!(dts.contains("export declare const b: 10n;"), "{dts}");
    assert!(dts.contains("export declare const m: -3;"), "{dts}");
}

#[test]
fn as_const_boolean_uses_literal_annotation() {
    let dts = dts_or_skip!(
        "as_const_bool",
        "export const a = true as const;\nexport const b = false as const;\n"
    );
    assert!(dts.contains("export declare const a: true;"), "{dts}");
    assert!(dts.contains("export declare const b: false;"), "{dts}");
}

// =============================================================================
// Widening / conversion `as T` casts -> the asserted type (was WRONG before)
// =============================================================================

#[test]
fn widening_cast_to_primitive_uses_asserted_type() {
    // `5 as number` must declare `: number`, never the narrower `= 5`.
    let dts = dts_or_skip!(
        "as_number",
        "export const b = 5 as number;\nexport const s = \"x\" as string;\n"
    );
    assert!(dts.contains("export declare const b: number;"), "{dts}");
    assert!(!dts.contains("const b = 5"), "{dts}");
    assert!(dts.contains("export declare const s: string;"), "{dts}");
}

#[test]
fn cast_to_literal_union_uses_asserted_union() {
    let dts = dts_or_skip!(
        "as_union",
        "export const c = 1 as 1 | 2;\nexport const d = \"a\" as \"a\" | \"b\";\n"
    );
    assert!(dts.contains("export declare const c: 1 | 2;"), "{dts}");
    assert!(
        dts.contains("export declare const d: \"a\" | \"b\";"),
        "{dts}"
    );
}

#[test]
fn cast_to_keyword_type_uses_asserted_type() {
    let dts = dts_or_skip!("as_keyword", "export const d = true as boolean;\n");
    assert!(dts.contains("export declare const d: boolean;"), "{dts}");
    assert!(!dts.contains("const d = true"), "{dts}");
}

#[test]
fn double_assertion_uses_outermost_asserted_type() {
    let dts = dts_or_skip!(
        "double_as",
        "export const o = \"x\" as unknown as string;\n"
    );
    assert!(dts.contains("export declare const o: string;"), "{dts}");
    assert!(!dts.contains("const o = \"x\""), "{dts}");
}

#[test]
fn angle_bracket_const_assertion_uses_literal_annotation() {
    let dts = dts_or_skip!("angle_const", "export const n = <const>\"ang\";\n");
    assert!(dts.contains("export declare const n: \"ang\";"), "{dts}");
}

// =============================================================================
// Parenthesized and aliased assertions propagate non-freshness
// =============================================================================

#[test]
fn parenthesized_as_const_keeps_literal_annotation() {
    // Regression: the parenthesized form previously widened to `: string`.
    let dts = dts_or_skip!("paren_as_const", "export const e = (\"hi\" as const);\n");
    assert!(dts.contains("export declare const e: \"hi\";"), "{dts}");
    assert!(
        !dts.contains(": string"),
        "must not widen the literal:\n{dts}"
    );
}

#[test]
fn const_alias_of_as_const_inherits_literal_annotation() {
    // `const f = base` where `base` is `as const`: `f` inherits base's
    // non-fresh literal type, so it annotates `: "x"` (not inline `= "x"`).
    let dts = dts_or_skip!(
        "alias_as_const",
        "const base = \"x\" as const;\nexport const f = base;\n"
    );
    assert!(dts.contains("export declare const f: \"x\";"), "{dts}");
    assert!(!dts.contains("const f = "), "{dts}");
}

// =============================================================================
// Must stay inline: fresh literals and `satisfies`
// =============================================================================

#[test]
fn fresh_literal_const_stays_inline() {
    let dts = dts_or_skip!(
        "fresh",
        "export const k = 5;\nexport const l = \"plain\";\nexport const t = true;\n"
    );
    assert!(dts.contains("export declare const k = 5;"), "{dts}");
    assert!(dts.contains("export declare const l = \"plain\";"), "{dts}");
    assert!(dts.contains("export declare const t = true;"), "{dts}");
}

#[test]
fn const_alias_of_fresh_literal_stays_inline() {
    // `const m = plain` where `plain = 7` (fresh): `m` is still a fresh literal,
    // so the inline form is preserved.
    let dts = dts_or_skip!("alias_fresh", "const plain = 7;\nexport const m = plain;\n");
    assert!(dts.contains("export declare const m = 7;"), "{dts}");
}

#[test]
fn satisfies_stays_inline() {
    // `satisfies` does not change the declared type/freshness, so the inline
    // `= literal` form is kept (tsc parity).
    let dts = dts_or_skip!(
        "satisfies",
        "export const g = \"ok\" satisfies string;\nexport const h = 42 satisfies number;\n"
    );
    assert!(dts.contains("export declare const g = \"ok\";"), "{dts}");
    assert!(dts.contains("export declare const h = 42;"), "{dts}");
}

#[test]
fn as_const_object_array_keep_readonly_annotation() {
    // `as const` object/array literals already annotate `: readonly {...}` /
    // `: readonly [...]`; this must remain unchanged.
    let dts = dts_or_skip!(
        "as_const_obj",
        "export const o = { a: 1 } as const;\nexport const arr = [1, 2] as const;\n"
    );
    assert!(dts.contains("readonly a: 1;"), "object as const:\n{dts}");
    assert!(dts.contains("readonly [1, 2]"), "array as const:\n{dts}");
}

// =============================================================================
// Class `readonly` properties follow the same rule
// =============================================================================

#[test]
fn class_readonly_as_const_uses_literal_annotation() {
    let dts = dts_or_skip!(
        "class_as_const",
        "export class C {\n  readonly a = \"hello\" as const;\n  static readonly s = 1 as 1 | 2;\n  readonly t = 10n as const;\n}\n"
    );
    assert!(dts.contains("readonly a: \"hello\";"), "{dts}");
    assert!(dts.contains("static readonly s: 1 | 2;"), "{dts}");
    assert!(dts.contains("readonly t: 10n;"), "{dts}");
}

#[test]
fn class_readonly_fresh_literal_stays_inline() {
    // A plain `readonly p = "plain"` is a fresh literal and keeps the inline
    // form; a mutable property widens to the primitive.
    let dts = dts_or_skip!(
        "class_fresh",
        "export class C {\n  readonly p = \"plain\";\n  q = \"mutable\";\n}\n"
    );
    assert!(dts.contains("readonly p = \"plain\";"), "{dts}");
    assert!(dts.contains("q: string;"), "{dts}");
}
