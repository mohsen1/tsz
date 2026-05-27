//! Tests for the circular return-type assignability fix.
//!
//! When a function/getter has no explicit return type annotation, the checker
//! infers the return type from the body.  Previously it then re-checked the
//! return statement against that inferred type, which could cause false TS2322
//! errors (e.g. for nested array literals with different object shapes).
//!
//! The fix pushes `TypeId::ANY` as the return type context when the return type
//! is purely inferred, so `check_return_statement` skips the circular check.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_source, check_source_with_libs, diagnostic_count, load_lib_files,
};
use tsz_common::common::ScriptTarget;

const LIB_NAMES: &[&str] = &[
    "es5.d.ts",
    "es2015.d.ts",
    "es2015.core.d.ts",
    "es2015.collection.d.ts",
    "es2015.iterable.d.ts",
    "es2015.promise.d.ts",
    "es2015.proxy.d.ts",
    "es2015.reflect.d.ts",
    "es2015.symbol.d.ts",
    "es2015.symbol.wellknown.d.ts",
];

fn check_default(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn check_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    check_source(source, "test.ts", options)
}

fn check_with_libs(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    let libs = load_lib_files(LIB_NAMES);
    check_source_with_libs(source, "test.ts", options, &libs)
}

include!("contextual_typing_tests_parts/part_00.rs");
include!("contextual_typing_tests_parts/part_01.rs");
