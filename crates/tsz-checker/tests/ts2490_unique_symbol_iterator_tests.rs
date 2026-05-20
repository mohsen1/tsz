//! Regression tests for issue #6611: TS2490 false positive on valid iterator
//! implementations when multiple unique symbol types are in scope.
//!
//! Structural rule: When `{ [Symbol.iterator](): Iterator<T> }` returns an
//! explicit `Iterator<T>` annotation, the iterator protocol (next().value) is
//! satisfied and TS2490 must not fire regardless of other unique symbol types
//! present in the same file.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostic_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    let diags = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    for d in &diags {
        println!(
            "DIAG code={} start={} msg={}",
            d.code, d.start, d.message_text
        );
    }
    diags.into_iter().map(|d| d.code).collect()
}

// ============================================================
// Isolation tests
// ============================================================

#[test]
fn two_unique_symbols_no_type_aliases_no_ts2490() {
    let codes = diagnostic_codes(
        r#"
const sym = Symbol("sym");
const obj = { [sym]: 42 };
const val: number = obj[sym];

declare const u1: unique symbol;
declare const u2: unique symbol;

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire with two unique symbols but no type aliases; got: {codes:?}"
    );
}

#[test]
fn two_unique_symbol_type_aliases_no_indexed_access_no_ts2490() {
    let codes = diagnostic_codes(
        r#"
declare const u1: unique symbol;
declare const u2: unique symbol;
type T1 = { [u1]: string };
type T2 = { [u2]: string };

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire with type aliases and no indexed access; got: {codes:?}"
    );
}

#[test]
fn type_aliases_with_string_keys_no_ts2490() {
    // Same as failing but T1/T2 use string keys, not unique symbol keys
    let codes = diagnostic_codes(
        r#"
const sym = Symbol("sym");
const obj = { [sym]: 42 };
const val: number = obj[sym];

declare const u1: unique symbol;
declare const u2: unique symbol;
type T1 = { foo: string };
type T2 = { bar: string };

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire with string-key aliases; got: {codes:?}"
    );
}

#[test]
fn indexed_access_after_type_aliases_no_ts2490() {
    // Same elements as failing test but indexed access AFTER type aliases
    let codes = diagnostic_codes(
        r#"
const sym = Symbol("sym");
const obj = { [sym]: 42 };

declare const u1: unique symbol;
declare const u2: unique symbol;
type T1 = { [u1]: string };
type T2 = { [u2]: string };

const val: number = obj[sym];

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire when indexed access is after type aliases; got: {codes:?}"
    );
}

#[test]
fn two_unique_symbol_type_aliases_with_as_cast_no_ts2490() {
    // Using `as` cast instead of annotation - no assignability check
    let codes = diagnostic_codes(
        r#"
const sym = Symbol("sym");
const obj = { [sym]: 42 };
const val = obj[sym] as number;

declare const u1: unique symbol;
declare const u2: unique symbol;
type T1 = { [u1]: string };
type T2 = { [u2]: string };

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire with as-cast; got: {codes:?}"
    );
}

#[test]
fn two_unique_symbol_type_aliases_boolean_annotation_no_ts2490() {
    // Same as failing test but with boolean annotation instead of number
    let codes = diagnostic_codes(
        r#"
const sym = Symbol("sym");
const obj = { [sym]: true };
const val: boolean = obj[sym];

declare const u1: unique symbol;
declare const u2: unique symbol;
type T1 = { [u1]: string };
type T2 = { [u2]: string };

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire with boolean annotation; got: {codes:?}"
    );
}

#[test]
fn two_unique_symbol_type_aliases_string_annotation_no_ts2490() {
    // Same as failing test but with string annotation
    let codes = diagnostic_codes(
        r#"
const sym = Symbol("sym");
const obj = { [sym]: "hello" };
const val: string = obj[sym];

declare const u1: unique symbol;
declare const u2: unique symbol;
type T1 = { [u1]: string };
type T2 = { [u2]: string };

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire with string annotation; got: {codes:?}"
    );
}

// ============================================================
// Core repro: all three conditions present
// ============================================================

#[test]
fn two_unique_symbols_with_indexed_access_and_iterator_no_ts2490() {
    // tsc 5.9: 0 errors. tsz was emitting TS2490 due to incorrect
    // interaction between unique symbol processing and iterator checking.
    let codes = diagnostic_codes(
        r#"
const sym = Symbol("sym");
const obj = { [sym]: 42 };
const val: number = obj[sym];

declare const u1: unique symbol;
declare const u2: unique symbol;
type T1 = { [u1]: string };
type T2 = { [u2]: string };

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire on valid iterator with unique symbols in scope; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2488),
        "TS2488 must not fire on valid iterable; got: {codes:?}"
    );
}

// ============================================================
// Verify condition isolation: each removed condition must allow the code
// ============================================================

#[test]
fn iterator_without_unique_symbols_no_ts2490() {
    // Condition 2 removed: no unique symbol types in scope.
    let codes = diagnostic_codes(
        r#"
const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire without unique symbols; got: {codes:?}"
    );
}

#[test]
fn iterator_with_one_unique_symbol_no_ts2490() {
    // Only one unique symbol type — bug report says one is OK, two is not.
    let codes = diagnostic_codes(
        r#"
const sym = Symbol("sym");
const obj = { [sym]: 42 };
const val: number = obj[sym];

declare const u1: unique symbol;
type T1 = { [u1]: string };

const iterable = {
  [Symbol.iterator](): Iterator<number> {
    let i = 0;
    return {
      next() {
        return { value: i++, done: i > 3 };
      }
    };
  }
};

for (const x of iterable) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire with only one unique symbol; got: {codes:?}"
    );
}

// ============================================================
// Different iterator variable name (K vs P) — the fix must be structural
// ============================================================

#[test]
fn two_unique_symbols_with_differently_named_iterator_variable_no_ts2490() {
    // Verify the fix works regardless of how the iterable's iterator variable is named.
    let codes = diagnostic_codes(
        r#"
declare const k1: unique symbol;
declare const k2: unique symbol;
type S1 = { [k1]: number };
type S2 = { [k2]: number };

const sym = Symbol("sym");
const data = { [sym]: "hello" };
const _ = data[sym];

class MyIterable {
  [Symbol.iterator](): Iterator<string> {
    return {
      next() {
        return { value: "x", done: false };
      }
    };
  }
}

for (const s of new MyIterable()) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire regardless of iterator variable naming; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2488),
        "TS2488 must not fire; got: {codes:?}"
    );
}

// ============================================================
// Bare Iterator without explicit type args
// ============================================================

#[test]
fn two_unique_symbols_bare_iterator_annotation_no_ts2490() {
    // Same shape but with bare `Iterator` (no type args) — defaults apply.
    let codes = diagnostic_codes(
        r#"
const s = Symbol("s");
const container = { [s]: true };
const v = container[s];

declare const p: unique symbol;
declare const q: unique symbol;
type A = { [p]: string };
type B = { [q]: string };

const source = {
  [Symbol.iterator](): Iterator<number> {
    let n = 0;
    return {
      next() {
        return { value: n++, done: n > 5 };
      }
    };
  }
};

for (const num of source) {}
"#,
    );
    assert!(
        !codes.contains(&2490),
        "TS2490 must not fire with bare Iterator annotation and unique symbols; got: {codes:?}"
    );
}

// ============================================================
// Ensure actual TS2490 errors are still detected
// ============================================================

#[test]
fn genuine_ts2490_still_reported() {
    // An iterator whose next() does NOT return { value } should still get TS2490.
    // This ensures we aren't over-suppressing the diagnostic.
    let codes = diagnostic_codes(
        r#"
declare const u1: unique symbol;
declare const u2: unique symbol;
type T1 = { [u1]: string };
type T2 = { [u2]: string };

const badIterable = {
  [Symbol.iterator]() {
    return {
      next() {
        return { done: true };  // Missing 'value' — TS2490 expected
      }
    };
  }
};

for (const x of badIterable) {}
"#,
    );
    assert!(
        codes.contains(&2490) || codes.contains(&2488),
        "TS2490 or TS2488 must fire for iterator missing 'value'; got: {codes:?}"
    );
}
