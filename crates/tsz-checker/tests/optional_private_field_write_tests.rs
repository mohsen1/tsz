//! Tests for optional private field write-context type (#10749).
//!
//! When `#x?: T`, assigning `undefined` must be accepted under strictNullChecks
//! (matching public optional field behavior), and rejected under
//! exactOptionalPropertyTypes.

use tsz_binder::BinderState;
use tsz_checker::{context::CheckerOptions, state::CheckerState};
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn check(source: &str, opts: CheckerOptions) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        target: ScriptTarget::ES2022,
        strict: true,
        ..Default::default()
    }
}

fn exact_optional_opts() -> CheckerOptions {
    CheckerOptions {
        target: ScriptTarget::ES2022,
        strict: true,
        exact_optional_property_types: true,
        ..Default::default()
    }
}

// Structural rule: When a private field is optional (`#x?: T`), under
// strictNullChecks and not exactOptionalPropertyTypes, its write type is
// `T | undefined`, so assigning undefined must not produce TS2322.
#[test]
fn optional_private_field_accepts_undefined_write() {
    let codes = check(
        r#"
class Mutex {
    #promise?: Promise<void>;
    #resolve?: () => void;
    unlock(): void {
        this.#promise = undefined;
        this.#resolve = undefined;
    }
}
"#,
        strict_opts(),
    );
    assert!(
        !codes.contains(&2322),
        "should not produce TS2322 for optional private field write; got codes: {codes:?}"
    );
}

// Name-independence: different type-parameter name (V) produces the same result.
#[test]
fn optional_private_field_undefined_write_name_independent() {
    let codes = check(
        r#"
class Container<V> {
    #value?: V;
    clear(): void {
        this.#value = undefined;
    }
}
"#,
        strict_opts(),
    );
    assert!(
        !codes.contains(&2322),
        "optional private field write should accept undefined regardless of type-param name; got: {codes:?}"
    );
}

// Non-optional private field must still reject undefined.
#[test]
fn non_optional_private_field_rejects_undefined_write() {
    let codes = check(
        r#"
class C {
    #x: number;
    constructor() { this.#x = 0; }
    clear(): void {
        this.#x = undefined;
    }
}
"#,
        strict_opts(),
    );
    assert!(
        codes.contains(&2322),
        "non-optional private field write must still reject undefined; got: {codes:?}"
    );
}

// Under exactOptionalPropertyTypes, assigning undefined to an optional private
// field must be rejected (matching public field behavior).
#[test]
fn optional_private_field_rejects_undefined_with_exact_optional() {
    let codes = check(
        r#"
class D {
    #data?: string;
    reset(): void {
        this.#data = undefined;
    }
}
"#,
        exact_optional_opts(),
    );
    assert!(
        codes.contains(&2322),
        "optional private field write should be rejected under exactOptionalPropertyTypes; got: {codes:?}"
    );
}

// Public optional field (already working): parity check.
#[test]
fn optional_public_field_parity_accepts_undefined() {
    let codes = check(
        r#"
class E {
    pub?: string;
    reset(): void {
        this.pub = undefined;
    }
}
"#,
        strict_opts(),
    );
    assert!(
        !codes.contains(&2322),
        "optional public field write should accept undefined (parity baseline); got: {codes:?}"
    );
}
