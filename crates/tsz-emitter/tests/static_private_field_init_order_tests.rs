//! Tests for the source-order emission of **static** field initializers when a
//! class mixes public static fields, private static fields, and static blocks.
//!
//! Structural rule: when a class declaration lowers its private fields (target
//! `< ES2022`), `tsc` emits the static *element* initializers — public field
//! assignments (`C.x = ...`), private field value inits
//! (`_C_y = { value: ... }`), and static block IIFEs — in **source
//! declaration order**, because their initializers can have observable side
//! effects. tsz previously grouped every private static field init ahead of the
//! public ones (`transformPropertyWorker` interleaves them; tsz emitted the
//! private group first), so a program like
//!
//! ```ts
//! class C {
//!   static a = log("a");
//!   static #b = log("b");
//!   static c = log("c");
//! }
//! ```
//!
//! initialized in the order `b, a, c` instead of `a, b, c`.
//!
//! The private-field *storage declaration* (`var _C_y`) and the class alias
//! (`_a = C`) still precede all static elements; only the value initialization
//! interleaves. A class with no public static fields keeps the private inits in
//! their existing grouped position (there is nothing to interleave against).
//!
//! Names are varied across cases so the ordering is driven by declaration
//! position, not by any particular binder spelling (anti-hardcoding gate).

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;
use tsz_emitter::output::printer::PrintOptions;

/// Assert that each needle appears in `haystack` and that they appear in the
/// given order.
fn assert_in_order(haystack: &str, needles: &[&str]) {
    let mut search_from = 0usize;
    let mut last = String::new();
    for needle in needles {
        match haystack[search_from..].find(needle) {
            Some(rel) => search_from += rel + needle.len(),
            None => panic!(
                "expected `{needle}` after `{last}` in emit output, but it was missing or \
                 out of order.\n--- output ---\n{haystack}"
            ),
        }
        last = (*needle).to_string();
    }
}

#[test]
fn static_public_and_private_field_inits_interleave_in_source_order() {
    let src = r#"
class Ledger {
  static first = log("first");
  static #second = log("second");
  static third = log("third");
  static #fourth = log("fourth");
  static readFourth() { return Ledger.#fourth; }
}
"#;
    let out = parse_and_lower_print(src, PrintOptions::es6());
    assert_in_order(
        &out,
        &[
            "Ledger.first = log(\"first\")",
            "_Ledger_second = { value: log(\"second\") }",
            "Ledger.third = log(\"third\")",
            "_Ledger_fourth = { value: log(\"fourth\") }",
        ],
    );
}

#[test]
fn static_block_interleaves_with_public_and_private_static_inits() {
    let src = r#"
class Gauge {
  static alpha = 1;
  static { setup(); }
  static #beta = 2;
  static gamma = 3;
  static readBeta() { return Gauge.#beta; }
}
"#;
    let out = parse_and_lower_print(src, PrintOptions::es6());
    assert_in_order(
        &out,
        &[
            "Gauge.alpha = 1",
            "setup()",
            "_Gauge_beta = { value: 2 }",
            "Gauge.gamma = 3",
        ],
    );
}

#[test]
fn private_static_init_after_leading_public_static_field() {
    // The private field is declared second, so its value init must follow the
    // first public field assignment rather than being hoisted ahead of it.
    let src = r#"
class Meter {
  static total = seed();
  static #count = 0;
  static readCount() { return Meter.#count; }
}
"#;
    let out = parse_and_lower_print(src, PrintOptions::es6());
    assert_in_order(
        &out,
        &["Meter.total = seed()", "_Meter_count = { value: 0 }"],
    );
}

#[test]
fn private_static_init_before_trailing_public_static_field() {
    // The private field is declared first, so its value init must precede the
    // trailing public field assignment.
    let src = r#"
class Widget {
  static #tally = start();
  static label = "w";
  static readTally() { return Widget.#tally; }
}
"#;
    let out = parse_and_lower_print(src, PrintOptions::es6());
    assert_in_order(
        &out,
        &["_Widget_tally = { value: start() }", "Widget.label = \"w\""],
    );
}

#[test]
fn private_only_static_inits_keep_source_order() {
    // No public static fields: the private inits stay in their existing grouped
    // position and keep source order among themselves.
    let src = r#"
class Registry {
  static #one = a();
  static #two = b();
  static read() { return Registry.#one + Registry.#two; }
}
"#;
    let out = parse_and_lower_print(src, PrintOptions::es6());
    assert_in_order(
        &out,
        &[
            "_Registry_one = { value: a() }",
            "_Registry_two = { value: b() }",
        ],
    );
}
