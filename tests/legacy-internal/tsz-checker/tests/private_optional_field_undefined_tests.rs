//! Regression tests for #10668: reading an *optional private* field
//! (`this.#p` where `#p?: T`) must yield `T | undefined`, like public optional
//! property access. Previously the private read path
//! (`private_property_declared_access_type`) returned bare `T`, which both
//! dropped a soundness check (assigning `this.#p` to `T` wrongly succeeded,
//! missing TS2322) and misfired the "always defined" awaitable-truthiness
//! diagnostic (TS2801) on `if (this.#p)`.

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
fn optional_private_promise_field_truthiness_no_ts2801() {
    let c = codes(
        r#"
class C {
  #p?: Promise<void>;
  m() { if (this.#p) { return; } this.#p = Promise.resolve(); }
}
"#,
    );
    assert!(
        !c.contains(&2801),
        "optional private field is `T | undefined`; truthiness is meaningful, must not emit TS2801, got {c:?}"
    );
}

#[test]
fn reading_optional_private_field_includes_undefined_ts2322() {
    // Reading `this.#name` (= `string | undefined`) where `string` is required
    // must error TS2322 — proving the read type now carries `| undefined`, as
    // for public optional fields and as tsc reports. (Uses a primitive rather
    // than a lib type so the assignability check is lib-independent.)
    let c = codes(
        r#"
class C {
  #name?: string;
  m(): string { return this.#name; }
}
"#,
    );
    assert!(
        c.contains(&2322),
        "reading an optional private field must include `undefined` (TS2322 on narrowing assignment), got {c:?}"
    );
}

#[test]
fn renamed_optional_private_field_is_structural_not_name_based() {
    let c = codes(
        r#"
class Widget {
  #handle?: number;
  read(): number { return this.#handle; }
}
"#,
    );
    assert!(
        c.contains(&2322),
        "expected TS2322 for renamed optional private field, got {c:?}"
    );
}

#[test]
fn non_optional_private_promise_field_still_emits_ts2801() {
    // Regression guard: a genuinely always-defined private Promise field must
    // still trigger the awaitable-truthiness diagnostic (matches tsc).
    let c = codes(
        r#"
class C {
  #p: Promise<void> = Promise.resolve();
  m() { if (this.#p) {} }
}
"#,
    );
    assert!(
        c.contains(&2801),
        "non-optional private Promise field is always defined; TS2801 must still fire, got {c:?}"
    );
}

#[test]
fn public_optional_field_unaffected() {
    // Public optional fields already included `undefined`; behavior unchanged.
    let c = codes(
        r#"
class C {
  p?: Promise<void>;
  m() { if (this.p) {} }
}
"#,
    );
    assert!(
        !c.contains(&2801) && !c.contains(&2322),
        "public optional field truthiness must stay clean, got {c:?}"
    );
}
