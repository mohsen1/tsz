//! End-to-end guards for the base instance type of a class extending a *value*
//! whose constructor type carries both a non-generic and a still-generic
//! construct signature (the `MapConstructor` shape: `new (): T` plus
//! `new <K, V>(): U`). The base instance type must come from the overload
//! applicable with zero explicit type arguments, exactly as `tsc`'s
//! `getConstructorsForTypeArguments` does — never a spurious `T | U` union that
//! leaks the generic overload's free type parameters and misfires the
//! override-variance check with a false TS2416 (the `class DraftMap extends Map`
//! family from #15248, immer compat row).
//!
//! The underlying root cause is verified directly, red/green, at its owner
//! layer by the solver unit tests in
//! `crates/tsz-solver/src/type_queries/data/tests.rs`
//! (`construct_return_union_*`), where `get_construct_return_type_union` is
//! exercised in isolation. These checker cases are forward-looking regression
//! guards for the class-heritage path: they pin the correct diagnostics for the
//! extends-value shape (matching overload accepted, incompatible override still
//! rejected, defaulted generic overload applicable) with renamed binders per the
//! anti-hardcoding gate.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn ts2416_count(source: &str) -> usize {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .filter(|d| d.code == 2416)
        .count()
}

fn assert_no_ts2416(source: &str) {
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(ts2416.is_empty(), "Expected no TS2416, got: {diags:#?}");
}

/// Override matching the applicable (non-generic) construct overload must be
/// accepted — no false TS2416 from the dropped generic overload's free param.
/// The base instance type is the zero-type-arg overload
/// `{ store(entry: string): void }`, not a `string | Key` union.
#[test]
fn extends_value_with_generic_construct_overload_no_false_ts2416() {
    assert_no_ts2416(
        r#"
declare const Base: {
  new (): { store(entry: string): void };
  new <Key, Value>(): { store(entry: Key): void };
};

class Draft extends Base {
  store(entry: string): void {}
}
"#,
    );
}

/// Binder-name-varied restatement of the same shape (anti-hardcoding gate): the
/// decision must be driven by the construct signatures' type-parameter arity,
/// never by the identifiers chosen.
#[test]
fn extends_value_generic_overload_renamed_binders_no_false_ts2416() {
    assert_no_ts2416(
        r#"
declare const Factory: {
  new (): { put(slot: number): void };
  new <Alpha, Beta, Gamma>(): { put(slot: Alpha): void };
};

class Layer extends Factory {
  put(slot: number): void {}
}
"#,
    );
}

/// A fully-defaulted generic construct overload (`new <Elem = string>(): U`) has
/// a `minTypeArgumentCount` of 0, so it stays applicable with zero type arguments
/// and resolves to `{ read(): string }`.
#[test]
fn extends_value_fully_defaulted_generic_overload_is_applicable() {
    assert_no_ts2416(
        r#"
declare const Base: {
  new <Elem = string>(): { read(): Elem };
};

class View extends Base {
  read(): string { return ""; }
}
"#,
    );
}

/// Inverse control: when the override is genuinely incompatible with the
/// applicable (non-generic) base overload, TS2416 must still fire. The filter
/// only removes the non-applicable generic overload; it does not silence real
/// override mismatches.
#[test]
fn extends_value_incompatible_override_still_emits_ts2416() {
    let count = ts2416_count(
        r#"
declare const Base: {
  new (): { store(entry: string): void };
  new <Key, Value>(): { store(entry: Key): void };
};

class Draft extends Base {
  store(entry: number): void {}
}
"#,
    );
    assert_eq!(
        count, 1,
        "Expected TS2416 when the override is incompatible with the applicable base overload"
    );
}
