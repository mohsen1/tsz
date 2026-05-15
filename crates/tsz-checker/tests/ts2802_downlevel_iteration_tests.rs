//! Tests for TS2802 emission on `for-of` over a custom Iterable when
//! targeting ES5 without `downlevelIteration`.
//!
//! Closes #5893. The TS2802 producer machinery already exists in
//! `crates/tsz-checker/src/checkers/iterable_checker.rs` (gated on
//! `compiler_options.target.is_es5()`); this test file pins the
//! behavior end-to-end and serves as the regression surface.

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn check_with_target(source: &str, target: ScriptTarget) -> Vec<u32> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    let libs = LIBS.get_or_init(load_default_lib_files);
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target,
            ..CheckerOptions::default()
        },
        libs,
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

fn check_es5(source: &str) -> Vec<u32> {
    check_with_target(source, ScriptTarget::ES5)
}

fn check_es2020(source: &str) -> Vec<u32> {
    check_with_target(source, ScriptTarget::ES2020)
}

/// Direct repro from #5893. With `target: ES5` and no downlevelIteration,
/// for-of over a custom Iterable must emit TS2802.
#[test]
fn for_of_custom_iterable_es5_emits_ts2802() {
    let diags = check_es5(
        "class Range implements Iterable<number> {\n\
             constructor(private start: number, private end: number) {}\n\
             *[Symbol.iterator](): Iterator<number> {\n\
                 for (let i = this.start; i <= this.end; i++) {\n\
                     yield i;\n\
                 }\n\
             }\n\
         }\n\
         const range = new Range(1, 5);\n\
         for (const n of range) {}\n",
    );
    assert!(
        diags.contains(&2802),
        "for-of over custom Iterable at target=ES5 must emit TS2802; got: {diags:?}",
    );
}

/// Per .claude/CLAUDE.md §25 anti-hardcoding: the rule must not depend on
/// the spelling of the class.
#[test]
fn for_of_custom_iterable_es5_emits_ts2802_independent_of_class_name() {
    let diags = check_es5(
        "class Walker implements Iterable<string> {\n\
             *[Symbol.iterator](): Iterator<string> { yield 'a'; }\n\
         }\n\
         for (const x of new Walker()) {}\n",
    );
    assert!(
        diags.contains(&2802),
        "TS2802 must fire for any custom iterable class at ES5; got: {diags:?}",
    );
}

/// Regression guard: target=ES2020 does NOT need downlevelIteration, so
/// the same code is fine.
#[test]
fn for_of_custom_iterable_es2020_no_ts2802() {
    let diags = check_es2020(
        "class Range implements Iterable<number> {\n\
             *[Symbol.iterator](): Iterator<number> { yield 1; }\n\
         }\n\
         for (const n of new Range()) {}\n",
    );
    assert!(
        !diags.contains(&2802),
        "TS2802 must NOT fire at ES2020; got: {diags:?}",
    );
}

/// Regression guard: built-in array iteration at ES5 is fine (no
/// downlevelIteration required for plain arrays).
#[test]
fn for_of_array_es5_no_ts2802() {
    let diags = check_es5("for (const n of [1, 2, 3]) {}\n");
    assert!(
        !diags.contains(&2802),
        "TS2802 must NOT fire for plain array iteration at ES5; got: {diags:?}",
    );
}
