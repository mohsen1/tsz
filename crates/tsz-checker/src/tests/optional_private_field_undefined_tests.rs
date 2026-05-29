//! Regression tests: optional private class fields (`#x?: T`) must carry
//! `| undefined` in their declared type, exactly like public optional fields
//! (`x?: T`) do.
//!
//! Root cause: the private-field access path read the declared type straight
//! off the annotation and never applied `?`-optionality, so `#x?: T` was typed
//! as `T` instead of `T | undefined`. That produced both false positives and
//! false negatives depending on how the field was used:
//!   - `if (this.#p)` on a `#p?: Promise<T>` field fired a false TS2801
//!     ("...is always defined") — issue #10668.
//!   - `this.#x = undefined` fired a false TS2322 — issue #10749.
//!   - reads such as `return this.#s` (to a non-optional return) or
//!     `this.#s.charAt(0)` silently missed the genuine `undefined` errors.
//!
//! Structural rule: when a private class field is declared optional under
//! `strictNullChecks`, tsc gives it read type `T | undefined` (and write type
//! `T | undefined` unless `exactOptionalPropertyTypes`); tsz now matches.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{
    check_source_codes, check_source_with_libs, check_with_options, load_default_lib_files,
};

fn codes_exact_optional(src: &str) -> Vec<u32> {
    let diags: Vec<Diagnostic> = check_with_options(
        src,
        CheckerOptions {
            exact_optional_property_types: true,
            ..CheckerOptions::default()
        },
    );
    diags.into_iter().map(|d| d.code).collect()
}

/// Run with the real lib bundle so `Promise<T>` resolves to the awaitable
/// builtin that TS2801 (`...is always defined`) keys off of.
fn codes_with_libs(src: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(src, "test.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// ---------------------------------------------------------------------------
// #10668: false TS2801 on truthiness of an optional private Promise field
// (requires the real lib so `Promise` is the awaitable builtin)
// ---------------------------------------------------------------------------

#[test]
fn no_false_2801_truthiness_of_optional_private_promise() {
    // Body does not reference the field, so the only thing that can suppress
    // TS2801 is the `| undefined` arm — not the use-in-body heuristic.
    let c = codes_with_libs(
        "
class Driver {
    #initPromise?: Promise<void>;
    m(): void {
        if (this.#initPromise) {
            console.log(\"used\");
        }
    }
}
",
    );
    assert!(!c.contains(&2801), "unexpected TS2801. Got: {c:?}");
}

#[test]
fn no_false_2801_truthiness_of_optional_private_promise_renamed_field() {
    // Different field spelling proves the rule is structural, not name-based.
    let c = codes_with_libs(
        "
class Service {
    #pending?: Promise<number>;
    run(): void {
        if (this.#pending) {
            console.log(\"used\");
        }
    }
}
",
    );
    assert!(!c.contains(&2801), "unexpected TS2801. Got: {c:?}");
}

#[test]
fn no_false_2801_kysely_runtime_driver_shape() {
    // Mirrors kysely's runtime-driver.ts field usage: lazily assign a pending
    // Promise behind a guard, reset it to `undefined` elsewhere, and `await`
    // the optional field. Must produce neither TS2801 nor TS2322.
    let c = codes_with_libs(
        "
interface Inner { init(): Promise<void>; }
class RuntimeDriver {
    #driver!: Inner;
    #initPromise?: Promise<void>;
    async init(): Promise<void> {
        if (!this.#initPromise) {
            this.#initPromise = this.#driver.init();
        }
        await this.#initPromise;
    }
    reset(): void {
        this.#initPromise = undefined;
    }
}
",
    );
    assert!(!c.contains(&2801), "unexpected TS2801. Got: {c:?}");
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn non_optional_private_promise_still_fires_2801() {
    // A genuinely always-defined private Promise field must still warn.
    let c = codes_with_libs(
        "
class A {
    #p: Promise<void> = Promise.resolve();
    m(): void {
        if (this.#p) {
            console.log(\"used\");
        }
    }
}
",
    );
    assert!(
        c.contains(&2801),
        "expected TS2801 on always-defined private Promise. Got: {c:?}"
    );
}

// ---------------------------------------------------------------------------
// #10749: false TS2322 assigning `undefined` to an optional private field
// (lib-free: the `| undefined` write type is value-type-agnostic)
// ---------------------------------------------------------------------------

#[test]
fn no_false_2322_assign_undefined_to_optional_private_string() {
    let c = check_source_codes(
        "
class A {
    #s?: string;
    m(): void {
        this.#s = undefined;
    }
}
",
    );
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn no_false_2322_assign_undefined_to_optional_private_number() {
    let c = check_source_codes(
        "
class A {
    #n?: number;
    m(): void {
        this.#n = undefined;
    }
}
",
    );
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn no_false_2322_assign_undefined_to_optional_private_object() {
    let c = check_source_codes(
        "
interface Box { value: number; }
class A {
    #b?: Box;
    m(): void {
        this.#b = undefined;
    }
}
",
    );
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn optional_private_field_matches_public_field_on_undefined_assignment() {
    // The public optional field already accepted `undefined`; the private one
    // must behave identically.
    let pub_codes = check_source_codes(
        "
class A {
    p?: string;
    m(): void { this.p = undefined; }
}
",
    );
    let priv_codes = check_source_codes(
        "
class A {
    #p?: string;
    m(): void { this.#p = undefined; }
}
",
    );
    assert!(
        !pub_codes.contains(&2322),
        "public baseline regressed: {pub_codes:?}"
    );
    assert!(
        !priv_codes.contains(&2322),
        "private differs from public: {priv_codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Reads must surface genuine `undefined` (previously false negatives)
// ---------------------------------------------------------------------------

#[test]
fn read_of_optional_private_field_is_possibly_undefined() {
    // `this.#s` is `string | undefined`, so member access without a guard is an
    // error (tsc: TS2532 / TS18048).
    let c = check_source_codes(
        "
class A {
    #s?: string;
    m(): void {
        this.#s.charAt(0);
    }
}
",
    );
    assert!(
        c.contains(&2532) || c.contains(&18048),
        "expected possibly-undefined diagnostic on optional private read. Got: {c:?}"
    );
}

#[test]
fn returning_optional_private_field_to_non_optional_type_errors() {
    let c = check_source_codes(
        "
class A {
    #s?: string;
    get(): string {
        return this.#s;
    }
}
",
    );
    assert!(
        c.contains(&2322),
        "expected TS2322 on optional private read. Got: {c:?}"
    );
}

#[test]
fn narrowing_guard_removes_undefined_from_optional_private_read() {
    // After a truthiness guard the `undefined` arm is gone, so the read is OK.
    let c = check_source_codes(
        "
class A {
    #s?: string;
    get(): void {
        if (this.#s) {
            this.#s.charAt(0);
        }
    }
}
",
    );
    assert!(
        !c.contains(&2532),
        "unexpected TS2532 after guard. Got: {c:?}"
    );
    assert!(
        !c.contains(&18048),
        "unexpected TS18048 after guard. Got: {c:?}"
    );
}

// ---------------------------------------------------------------------------
// Non-optional private fields are unaffected (negative / fallback cases)
// ---------------------------------------------------------------------------

#[test]
fn non_optional_private_field_rejects_undefined_assignment() {
    let c = check_source_codes(
        "
class A {
    #s: string = \"\";
    m(): void {
        this.#s = undefined;
    }
}
",
    );
    assert!(
        c.contains(&2322),
        "expected TS2322 assigning undefined to non-optional. Got: {c:?}"
    );
}

// ---------------------------------------------------------------------------
// exactOptionalPropertyTypes still rejects `undefined` assignment
// ---------------------------------------------------------------------------

#[test]
fn exact_optional_property_types_rejects_undefined_write_to_optional_private() {
    // With the flag on, the write type does NOT include `undefined`, so the
    // assignment is an error (tsc: TS2412).
    let c = codes_exact_optional(
        "
class A {
    #s?: string;
    m(): void {
        this.#s = undefined;
    }
}
",
    );
    assert!(
        c.contains(&2322) || c.contains(&2412),
        "expected undefined-write rejection under exactOptionalPropertyTypes. Got: {c:?}"
    );
}
