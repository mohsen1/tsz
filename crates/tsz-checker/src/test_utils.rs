//! Shared test utilities for checker unit tests.
//!
//! Provides common parse→bind→check pipeline helpers to eliminate
//! duplicated test setup boilerplate across checker test modules.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

/// Parse, bind, and type-check a TypeScript source string, returning all diagnostics.
///
/// Uses the given `CheckerOptions` and file name. Calls `set_lib_contexts(Vec::new())`
/// so tests run without lib definitions (preventing spurious TS2318 errors).
pub fn check_source(source: &str, file_name: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

/// Parse, bind, and type-check a TypeScript source string with default options.
///
/// Convenience wrapper around [`check_source`] using `"test.ts"` and default options.
pub fn check_source_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

/// Parse, bind, and type-check a JavaScript source string.
///
/// Uses `"test.js"` filename and enables `check_js`.
pub fn check_js_source_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    )
}

/// Parse, bind, and type-check source, returning only diagnostic codes.
///
/// Convenience wrapper for tests that only inspect error codes.
pub fn check_source_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// Parse, bind, and type-check source, returning `(code, message_text)` pairs.
///
/// Convenience wrapper for tests that inspect both error codes and message text.
pub fn check_source_code_messages(source: &str) -> Vec<(u32, String)> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Parse, bind, and type-check source with `experimental_decorators` enabled, returning codes.
pub fn check_source_codes_experimental_decorators(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

/// Parse, bind, and type-check source with `no_unused_parameters` enabled.
pub fn check_source_no_unused_params(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_parameters: true,
            ..Default::default()
        },
    )
}

/// Parse, bind, and type-check source with `no_unused_locals` enabled.
pub fn check_source_no_unused_locals(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_locals: true,
            ..Default::default()
        },
    )
}

/// Parse, bind, and type-check a TypeScript source string with the given options.
///
/// Uses `"test.ts"` as the file name. Convenience wrapper for tests that need
/// custom options but not a custom file name.
pub fn check_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    check_source(source, "test.ts", options)
}

#[cfg(test)]
mod tests {
    //! Self-tests for the test_utils helpers themselves.
    //!
    //! These pin the contracts that 100s of checker tests rely on:
    //! - `check_source_diagnostics` ≡ `check_source(source, "test.ts", default)`.
    //! - `check_source_codes` is a code-only projection of `check_source_diagnostics`.
    //! - `check_source_code_messages` projects to (code, message) pairs.
    //! - `check_js_source_diagnostics` uses `test.js` + `check_js: true`.
    //! - `check_source_codes_experimental_decorators` enables the decorator flag.
    //! - `check_source_no_unused_params` / `_no_unused_locals` enable the
    //!   matching unused-detection flag.
    //! - `check_with_options` ≡ `check_source(source, "test.ts", options)`.
    use super::*;

    #[test]
    fn check_source_diagnostics_matches_explicit_default_options() {
        // The convenience wrapper must produce the same diagnostics as the
        // 3-arg `check_source` with `"test.ts"` + default options.
        let source = "interface I {} const x = new I();";
        let lhs = check_source_diagnostics(source);
        let rhs = check_source(source, "test.ts", CheckerOptions::default());
        assert_eq!(lhs.len(), rhs.len());
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn check_source_codes_is_code_projection_of_diagnostics() {
        let source = "interface I {} const x = new I();";
        let diags = check_source_diagnostics(source);
        let codes = check_source_codes(source);
        let projected: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert_eq!(codes, projected);
    }

    #[test]
    fn check_source_code_messages_projects_pairs() {
        let source = "interface I {} const x = new I();";
        let pairs = check_source_code_messages(source);
        let diags = check_source_diagnostics(source);
        assert_eq!(pairs.len(), diags.len());
        for (i, (code, msg)) in pairs.iter().enumerate() {
            assert_eq!(*code, diags[i].code);
            assert_eq!(*msg, diags[i].message_text);
        }
    }

    #[test]
    fn check_source_diagnostics_returns_empty_for_clean_source() {
        let codes = check_source_codes("const x: number = 1;");
        assert!(
            codes.is_empty(),
            "expected no diagnostics for `const x: number = 1;`, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_diagnostics_emits_ts2693_for_interface_as_value() {
        let codes = check_source_codes("interface I {} const x = new I();");
        assert!(
            codes.contains(&2693),
            "expected TS2693 for interface used as value, got: {codes:?}"
        );
    }

    #[test]
    fn check_js_source_diagnostics_uses_check_js_flag() {
        // A JS-specific diagnostic that requires `check_js: true` is the
        // simplest contract test. `function Foo(){ this.x = 1 }; new Foo()`
        // is well-typed under check_js but produces TS7006/TS7041 etc. when
        // an undeclared identifier is used. Use a source with an obvious
        // type error and confirm we see SOME diagnostics under check_js.
        let source = "var x: number = 'hi';";
        let diags = check_js_source_diagnostics(source);
        // Should NOT emit TS2322 — type annotations are syntax errors in JS
        // and the parser path produces TS8010/TS8009 instead. We just want
        // to confirm `check_js: true` was applied (the diagnostics differ
        // from the default-TS path).
        let ts_diags = check_source_diagnostics(source);
        // The two helpers have different filename + check_js flag, so the
        // diagnostic SETS should not be identical for a TS-syntax-in-JS
        // source.
        let js_codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        let ts_codes: Vec<u32> = ts_diags.iter().map(|d| d.code).collect();
        assert_ne!(
            js_codes, ts_codes,
            "JS source with TS syntax should emit different diagnostics than TS path"
        );
    }

    #[test]
    fn check_source_no_unused_params_emits_ts6133() {
        let source = "function f(unused: number) {}";
        let diags = check_source_no_unused_params(source);
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&6133),
            "expected TS6133 for unused parameter, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_no_unused_locals_emits_ts6133() {
        let source = "function f() { var unused: number = 1; }";
        let diags = check_source_no_unused_locals(source);
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&6133),
            "expected TS6133 for unused local, got: {codes:?}"
        );
    }

    #[test]
    fn check_with_options_matches_check_source_with_test_ts() {
        // `check_with_options(source, opts)` is exactly
        // `check_source(source, "test.ts", opts)` — pin that.
        let opts = CheckerOptions {
            no_unused_parameters: true,
            ..Default::default()
        };
        let source = "function f(unused: number) {}";
        let lhs = check_with_options(source, opts.clone());
        let rhs = check_source(source, "test.ts", opts);
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn check_source_codes_experimental_decorators_clean_decorator_compiles() {
        // With `experimental_decorators` enabled, a well-typed decorator
        // application must not produce diagnostics. This pins that the flag
        // gets propagated through `CheckerOptions` to the checker.
        let source = r#"
function dec(target: any) { return target; }
@dec
class C {}
"#;
        let codes = check_source_codes_experimental_decorators(source);
        // No TS1219 ("Experimental decorator") gate.
        assert!(
            !codes.contains(&1219),
            "experimental_decorators flag should suppress TS1219, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_lib_contexts_are_empty_no_ts2318() {
        // The wrapper's `set_lib_contexts(Vec::new())` step prevents
        // spurious TS2318 ("Cannot find global type") errors that would
        // otherwise fire for built-in types like Promise/Array. Pin that
        // a source that uses `Promise` does NOT emit TS2318.
        let source = "let p: Promise<number>;";
        let codes = check_source_codes(source);
        assert!(
            !codes.contains(&2318),
            "set_lib_contexts(empty) must prevent TS2318 for Promise, got: {codes:?}"
        );
    }
}
