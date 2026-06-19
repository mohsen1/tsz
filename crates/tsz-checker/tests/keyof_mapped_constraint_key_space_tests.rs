//! `keyof` of a mapped type is its *constraint*, not the key space of the
//! materialized index-signature object.
//!
//! Structural rule: `When the operand of `keyof` is a mapped type
//! `{ [P in C]: V }` (including its alias forms `Record<C, V>` and an explicit
//! `type M = { [P in C]: V }`), tsc reduces `keyof` to the constraint `C` — a
//! string-keyed constraint contributes exactly `string`, never the implicit
//! `string | number` that a genuine `[k: string]` index signature carries.`
//! tsz materializes such mapped types into an object with a string index
//! signature; `keyof` of that object previously added the implicit `number`,
//! so `const k: keyof { [P in string]: V } = 5` was wrongly accepted (tsc
//! reports TS2322). The materialized object now carries
//! `ObjectFlags::MAPPED_CONSTRAINT_KEYS` so `keyof` follows the constraint, and
//! the `evaluate_keyof` reduction covers the operand before materialization.
//!
//! Homomorphic mapped types (`{ [K in keyof T]: V }`) keep `keyof T` and must
//! still expose the source's full key space (a `string` index source key still
//! yields `string | number`); genuine `[k: string]` index signatures are
//! untouched. The decision is purely structural (constraint kind), never keyed
//! on identifier or file-name text — the negative/anti-hardcoding cases below
//! vary every binder name.
//!
//! These cases use *explicit* mapped types so they do not depend on the
//! stripped test-lib's `Record` definition; the lib-`Record<string, V>` form is
//! the same code path and is covered by the CLI repro and ready-review
//! conformance against the real lib.

use tsz_checker::test_utils::check_source_strict_codes as codes;

fn has_ts2322(source: &str) -> bool {
    codes(source).contains(&2322)
}

// ── The core divergence: a string-keyed mapped constraint contributes only
//    `string`, so a numeric literal key is rejected. ────────────────────────

#[test]
fn keyof_explicit_mapped_string_alias_rejects_number_key() {
    // `type M = { [P in string]: V }; keyof M` is `string`.
    assert!(has_ts2322(
        "type M = { [P in string]: number }; const k: keyof M = 5;"
    ));
}

#[test]
fn keyof_explicit_mapped_string_inline_rejects_number_key() {
    assert!(has_ts2322("const k: keyof { [P in string]: number } = 5;"));
}

#[test]
fn keyof_mapped_string_double_alias_rejects_number_key() {
    // Regression for the shared-eval-cache poisoning: aliasing the `keyof`
    // itself (`type Keys = keyof M`) must not reintroduce the implicit number.
    assert!(has_ts2322(
        "type M = { [P in string]: number }; type Keys = keyof M; const k: Keys = 5;"
    ));
}

#[test]
fn keyof_mapped_number_constraint_rejects_string_key() {
    assert!(has_ts2322(
        "type MNum = { [P in number]: number }; const k: keyof MNum = \"x\";"
    ));
}

#[test]
fn keyof_mapped_symbol_constraint_rejects_string_key() {
    assert!(has_ts2322(
        "type MSym = { [P in symbol]: number }; const k: keyof MSym = \"x\";"
    ));
}

// ── Positive members of the key space must still be accepted. ───────────────

#[test]
fn keyof_explicit_mapped_string_accepts_string_key() {
    assert!(!has_ts2322(
        "type M = { [P in string]: number }; const k: keyof M = \"any\";"
    ));
}

#[test]
fn keyof_mapped_number_constraint_accepts_number_key() {
    assert!(!has_ts2322(
        "type MNum = { [P in number]: number }; const k: keyof MNum = 5;"
    ));
}

// ── Negative controls: genuine index signatures and homomorphic maps keep
//    their full `string | number` (or `keyof T`) key space. ─────────────────

#[test]
fn keyof_genuine_string_index_signature_still_accepts_number_key() {
    // A real `[k: string]` index signature implies numeric keys.
    assert!(!has_ts2322(
        "type IndexSig = { [k: string]: number }; const k: keyof IndexSig = 5;"
    ));
}

#[test]
fn keyof_homomorphic_over_string_index_keeps_number_key() {
    // `{ [K in keyof T]: T[K] }` over a string-index source preserves `keyof T`
    // = `string | number`, so a numeric key remains valid.
    assert!(!has_ts2322(
        "type Source = { [k: string]: number };\n\
         type Homo = { [K in keyof Source]: Source[K] };\n\
         const k: keyof Homo = 5;"
    ));
}

#[test]
fn mapped_and_index_signature_remain_mutually_assignable() {
    // The new interner distinction (constraint-keyed vs genuine index) must not
    // break structural assignability between the two forms.
    assert!(!has_ts2322(
        "type Mapped = { [P in string]: number };\n\
         type IndexSig = { [k: string]: number };\n\
         declare let m: Mapped;\n\
         declare let i: IndexSig;\n\
         m = i;\n\
         i = m;"
    ));
}

// ── Anti-hardcoding: the decision is structural, not name-driven. ───────────

#[test]
fn keyof_mapped_string_constraint_renamed_binders_reject_number() {
    // Same structure, every binder renamed — behavior must be identical.
    assert!(has_ts2322(
        "type Payload = { [Slot in string]: boolean }; type Keys = keyof Payload; const sink: Keys = 5;"
    ));
}

#[test]
fn keyof_mapped_string_constraint_renamed_binders_accept_string() {
    assert!(!has_ts2322(
        "type Payload = { [Slot in string]: boolean }; type Keys = keyof Payload; const sink: Keys = \"ok\";"
    ));
}
