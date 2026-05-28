//! Regression tests for #9752: assigning between two distinct `unique symbol`
//! types must emit TS2322 (like tsc), not TS2719 ("two different types with
//! this name"). TS2719 is reserved for distinct *named nominal* types sharing a
//! name; two `unique symbol` types stringify identically but are separate
//! symbol identities, so the failure must route through the standard TS2322
//! path. The fix detects unique-symbol operands structurally, not by display.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn codes(src: &str) -> Vec<u32> {
    check_source(src, "test.ts", strict())
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn assigning_unique_symbol_to_other_unique_symbol_is_ts2322_not_ts2719() {
    let c = codes(
        r#"
declare const s1: unique symbol;
declare const s2: unique symbol;
let b: typeof s1;
b = s2;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got {c:?}");
    assert!(!c.contains(&2719), "must not emit TS2719, got {c:?}");
}

#[test]
fn declaration_form_unique_symbol_mismatch_is_ts2322() {
    let c = codes(
        r#"
declare const s1: unique symbol;
declare const s2: unique symbol;
const a: typeof s1 = s2;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got {c:?}");
    assert!(!c.contains(&2719), "must not emit TS2719, got {c:?}");
}

#[test]
fn renamed_unique_symbols_still_ts2322_not_name_based() {
    // Different identifier spellings — proves the routing is structural, not
    // keyed on a shared display name.
    let c = codes(
        r#"
declare const alpha: unique symbol;
declare const beta: unique symbol;
let target: typeof alpha;
target = beta;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got {c:?}");
    assert!(!c.contains(&2719), "must not emit TS2719, got {c:?}");
}

#[test]
fn same_unique_symbol_assignment_is_clean() {
    let c = codes(
        r#"
declare const s1: unique symbol;
let b: typeof s1;
b = s1;
"#,
    );
    assert!(
        !c.contains(&2322) && !c.contains(&2719),
        "same-symbol assignment must be clean, got {c:?}"
    );
}

#[test]
fn unique_symbol_vs_wide_symbol_stays_ts2322() {
    let c = codes(
        r#"
declare const s1: unique symbol;
declare const w: symbol;
const x: typeof s1 = w;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got {c:?}");
    assert!(!c.contains(&2719), "must not emit TS2719, got {c:?}");
}
