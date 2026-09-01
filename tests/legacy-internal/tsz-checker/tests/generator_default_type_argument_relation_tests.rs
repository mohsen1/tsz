//! Regression pins for #16119: an **unparameterised** `Generator` /
//! `AsyncGenerator` annotation related against its own explicitly-written
//! default form.
//!
//! `Generator` and `AsyncGenerator` both declare
//! `[T = unknown, TReturn = any, TNext = any]`
//! (`crates/tsz-core/src/lib-assets/es2015.generator.d.ts`,
//! `es2018.asyncgenerator.d.ts`), so a bare `Generator` and a written-out
//! `Generator[ unknown, any, any ]` denote the same type and must relate in
//! both directions. #16119 reported a false `TS2345` on exactly that pair,
//! measured on #16115's merged head; it does **not** reproduce on this tree.
//! These rows pin the family so the defect cannot return silently — the
//! shapes have no conformance-corpus fixture, so an oracled unit matrix is
//! the only detector.
//!
//! Every row was oracled first-hand against `tsc@7.0.2`
//! (`--noEmit --strict --pretty false --target es2018 --lib es2018,dom`).
//!
//! The load-bearing part of this suite is [`bare_source_to_narrower_target_still_errors`]
//! and its async twin. "tsc reports nothing" is reachable by any change that
//! makes the relation *more permissive*, including one that stops comparing
//! type arguments altogether — which is the most likely shape of a wrong fix
//! for #16119, since the suspected mechanism was a positional type-argument
//! comparison firing ahead of the structural member comparison. Those two rows
//! keep a genuinely narrower target (`T = number` against a source `T =
//! unknown`) rejecting, so a permissive regression fails the suite instead of
//! passing it.
//!
//! Arity is varied independently of the defect (`Generator[ number ]` against
//! `Generator[ number, any, any ]` in both directions) because the relation
//! reaches the same comparison through a distinct arity-normalisation path
//! (`fill_application_defaults`, `relations/subtype/rules/generics.rs`), and
//! `Iterator` / `AsyncIterator` rows cover the same defaulting rule on the
//! non-generator declarations that sit above them in the heritage chain.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

// ---------------------------------------------------------------------------
// Negative controls. These must keep reporting TS2345, or every clean row
// below is satisfied vacuously by a relation that stopped checking arguments.
// ---------------------------------------------------------------------------

#[test]
fn bare_source_to_narrower_target_still_errors() {
    // Bare `Generator` is `Generator[ unknown, any, any ]`; `unknown` is not
    // assignable to `number`, and tsc reports TS2345 here.
    assert_eq!(
        strict_codes(
            r"
declare const bare: Generator;
declare function wants(g: Generator<number, any, any>): void;
wants(bare);
",
        ),
        vec![2345],
    );
}

#[test]
fn async_bare_source_to_narrower_target_still_errors() {
    assert_eq!(
        strict_codes(
            r"
declare const bare: AsyncGenerator;
declare function wants(g: AsyncGenerator<number, any, any>): void;
wants(bare);
",
        ),
        vec![2345],
    );
}

// ---------------------------------------------------------------------------
// #16119's reported shape: bare against its own explicitly-defaulted form.
// ---------------------------------------------------------------------------

#[test]
fn bare_generator_to_explicit_default_form_is_clean() {
    assert!(
        strict_codes(
            r"
declare const bare: Generator;
declare function wants(g: Generator<unknown, any, any>): void;
wants(bare);
",
        )
        .is_empty()
    );
}

#[test]
fn explicit_default_form_to_bare_generator_is_clean() {
    // The reverse direction: writing the defaults out must not make the
    // source stop relating to the bare parameter either.
    assert!(
        strict_codes(
            r"
declare const written: Generator<unknown, any, any>;
declare function wants(g: Generator): void;
wants(written);
",
        )
        .is_empty()
    );
}

#[test]
fn bare_async_generator_to_explicit_default_form_is_clean() {
    assert!(
        strict_codes(
            r"
declare const bare: AsyncGenerator;
declare function wants(g: AsyncGenerator<unknown, any, any>): void;
wants(bare);
",
        )
        .is_empty()
    );
}

#[test]
fn explicit_default_form_to_bare_async_generator_is_clean() {
    assert!(
        strict_codes(
            r"
declare const written: AsyncGenerator<unknown, any, any>;
declare function wants(g: AsyncGenerator): void;
wants(written);
",
        )
        .is_empty()
    );
}

#[test]
fn bare_to_bare_generator_is_clean() {
    assert!(
        strict_codes(
            r"
declare const bare: Generator;
declare function wants(g: Generator): void;
wants(bare);
",
        )
        .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Partial arity: the same relation reached through default-filling rather
// than through an already-equal argument list.
// ---------------------------------------------------------------------------

#[test]
fn partially_applied_generator_to_full_arity_is_clean() {
    assert!(
        strict_codes(
            r"
declare const partial: Generator<number>;
declare function wants(g: Generator<number, any, any>): void;
wants(partial);
",
        )
        .is_empty()
    );
}

#[test]
fn full_arity_generator_to_partially_applied_is_clean() {
    assert!(
        strict_codes(
            r"
declare const full: Generator<number, any, any>;
declare function wants(g: Generator<number>): void;
wants(full);
",
        )
        .is_empty()
    );
}

#[test]
fn partially_applied_async_generator_to_full_arity_is_clean() {
    assert!(
        strict_codes(
            r"
declare const partial: AsyncGenerator<number>;
declare function wants(g: AsyncGenerator<number, any, any>): void;
wants(partial);
",
        )
        .is_empty()
    );
}

#[test]
fn full_arity_async_generator_to_partially_applied_is_clean() {
    assert!(
        strict_codes(
            r"
declare const full: AsyncGenerator<number, any, any>;
declare function wants(g: AsyncGenerator<number>): void;
wants(full);
",
        )
        .is_empty()
    );
}

// ---------------------------------------------------------------------------
// The same defaulting rule one level up the heritage chain.
// ---------------------------------------------------------------------------

#[test]
fn partially_applied_iterator_to_full_arity_is_clean() {
    assert!(
        strict_codes(
            r"
declare const partial: Iterator<number>;
declare function wants(g: Iterator<number, any, any>): void;
wants(partial);
",
        )
        .is_empty()
    );
}

#[test]
fn partially_applied_async_iterator_to_full_arity_is_clean() {
    assert!(
        strict_codes(
            r"
declare const partial: AsyncIterator<number>;
declare function wants(g: AsyncIterator<number, any, any>): void;
wants(partial);
",
        )
        .is_empty()
    );
}

// ---------------------------------------------------------------------------
// An *inferred* generator reaching the same parameter. This is the shape the
// original #16119 witness was reported through (`yield*` over an array), and
// the reporter's own probe table used it to show `yield*` was a red herring.
// ---------------------------------------------------------------------------

#[test]
fn inferred_generator_to_partially_applied_target_is_clean() {
    assert!(
        strict_codes(
            r"
function* gen() { yield* [1, 2, 3]; }
declare function wants(g: Generator<number>): void;
wants(gen());
",
        )
        .is_empty()
    );
}

#[test]
fn inferred_async_generator_to_full_arity_target_is_clean() {
    assert!(
        strict_codes(
            r"
async function* gen() { yield* [1, 2, 3]; }
declare function wants(g: AsyncGenerator<number, any, any>): void;
wants(gen());
",
        )
        .is_empty()
    );
}
