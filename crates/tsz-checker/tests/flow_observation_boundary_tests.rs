//! Regression tests for the flow observation boundary.
//!
//! These tests verify that the checker correctly routes narrowing decisions
//! through the `query_boundaries::flow` module instead of ad-hoc inline logic.
//!
//! Coverage areas:
//! - Destructuring control-flow (default values strip undefined)
//! - Optional-chain narrowing (truthy branch removes nullish)
//! - Catch variable unknown behavior (useUnknownInCatchVariables)
//! - For-of destructuring
//! - Dependent destructured variables

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: parse, bind, check with default options.
fn check_default(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, CheckerOptions::default())
}

/// Helper: parse, bind, check with strict null checks.
fn check_strict(source: &str) -> Vec<Diagnostic> {
    let mut options = CheckerOptions::default();
    options.strict = true;
    options.strict_null_checks = true;
    check_with_options(source, options)
}

/// Helper: parse, bind, check with useUnknownInCatchVariables.
fn check_unknown_catch(source: &str) -> Vec<Diagnostic> {
    let mut options = CheckerOptions::default();
    options.strict = true;
    options.strict_null_checks = true;
    options.use_unknown_in_catch_variables = true;
    check_with_options(source, options)
}

fn check_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
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
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn has_error_code(diagnostics: &[Diagnostic], code: u32) -> bool {
    diagnostics.iter().any(|d| d.code == code)
}

fn error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics.iter().map(|d| d.code).collect()
}

// ============================================================
// Destructuring control-flow: default values
// ============================================================

#[test]
fn destructuring_object_default_strips_undefined() {
    // { name = "default" }: { name?: string } → name should be `string`, not `string | undefined`
    let source = r#"
        function f(x: { name?: string }) {
            const { name = "default" } = x;
            const s: string = name;
        }
    "#;
    let diagnostics = check_strict(source);
    // Should NOT have TS2322 (string | undefined not assignable to string)
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Destructuring default should strip undefined. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn destructuring_array_default_strips_undefined() {
    // const [x = 0] = arr where arr: (number | undefined)[]
    let source = r#"
        function f(arr: (number | undefined)[]) {
            const [x = 0] = arr;
            const n: number = x;
        }
    "#;
    let diagnostics = check_strict(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Array destructuring default should strip undefined. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn destructuring_nested_default_strips_undefined() {
    // Nested: { a: { b = 1 } = {} } : { a?: { b?: number } }
    let source = r#"
        function f(x: { a?: { b?: number } }) {
            const { a: { b = 1 } = {} } = x;
            const n: number = b;
        }
    "#;
    let diagnostics = check_strict(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Nested destructuring default should strip undefined. Got: {:?}",
        error_codes(&diagnostics)
    );
}

// ============================================================
// Optional-chain narrowing
// ============================================================

#[test]
fn optional_chain_truthiness_narrows_base() {
    // if (a?.b) { a } → a is non-nullish in true branch
    let source = r#"
        function f(a: { b: string } | null | undefined) {
            if (a?.b) {
                const x: { b: string } = a;
            }
        }
    "#;
    let diagnostics = check_strict(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Optional chain truthiness should narrow base to non-nullish. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn optional_chain_equality_narrows_base() {
    // if (a?.kind === "x") { a } → a is non-nullish in true branch
    let source = r#"
        type T = { kind: "x"; value: number } | { kind: "y"; value: string } | null;
        function f(a: T) {
            if (a?.kind === "x") {
                const v: number = a.value;
            }
        }
    "#;
    let diagnostics = check_strict(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Optional chain equality should narrow base. Got: {:?}",
        error_codes(&diagnostics)
    );
}

// ============================================================
// Catch variable unknown behavior
// ============================================================

#[test]
fn catch_variable_is_unknown_when_flag_set() {
    // With useUnknownInCatchVariables, catch variable should be `unknown`
    let source = r#"
        try {
            throw new Error("oops");
        } catch (e) {
            const x: unknown = e;
        }
    "#;
    let diagnostics = check_unknown_catch(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Catch variable should be unknown with useUnknownInCatchVariables. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn catch_variable_is_any_without_flag() {
    // Without useUnknownInCatchVariables, catch variable should be `any`
    let source = r#"
        try {
            throw new Error("oops");
        } catch (e) {
            e.message;
        }
    "#;
    let diagnostics = check_default(source);
    assert!(
        !has_error_code(&diagnostics, 2571), // TS2571: Object is of type 'unknown'
        "Catch variable should be any without flag. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn catch_variable_annotation_must_be_any_or_unknown() {
    // Catch variable with non-any/unknown annotation → TS1196
    let source = r#"
        try {
        } catch (e: string) {
        }
    "#;
    let diagnostics = check_strict(source);
    assert!(
        has_error_code(&diagnostics, 1196),
        "Catch variable with string annotation should produce TS1196. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn catch_variable_unknown_typeof_narrowing() {
    // Unknown catch variable should narrow by typeof
    let source = r#"
        try {
        } catch (e) {
            if (typeof e === "string") {
                const s: string = e;
            }
        }
    "#;
    let diagnostics = check_unknown_catch(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Unknown catch variable should narrow by typeof. Got: {:?}",
        error_codes(&diagnostics)
    );
}

// ============================================================
// For-of destructuring
// ============================================================

#[test]
fn for_of_basic_iteration() {
    let source = r#"
        const arr = [1, 2, 3];
        for (const x of arr) {
            const n: number = x;
        }
    "#;
    let diagnostics = check_strict(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "For-of should correctly infer element type. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn for_of_with_destructuring() {
    let source = r#"
        const arr: { x: number; y: string }[] = [];
        for (const { x, y } of arr) {
            const n: number = x;
            const s: string = y;
        }
    "#;
    let diagnostics = check_strict(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "For-of with destructuring should correctly infer element types. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn for_of_with_default_suppresses_missing_property() {
    // for (let {x = default} of [{}]) - should not emit TS2339 for `x`
    let source = r#"
        for (const { x = 10 } of [{}]) {
            const n: number = x;
        }
    "#;
    let diagnostics = check_default(source);
    assert!(
        !has_error_code(&diagnostics, 2339),
        "For-of with default should suppress TS2339. Got: {:?}",
        error_codes(&diagnostics)
    );
}

// ============================================================
// Dependent destructured variables
// ============================================================

#[test]
fn destructuring_tuple_types_elements() {
    let source = r#"
        function f(): [string, number] {
            return ["hello", 42];
        }
        const [a, b] = f();
        const s: string = a;
        const n: number = b;
    "#;
    let diagnostics = check_strict(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Tuple destructuring should correctly type individual elements. Got: {:?}",
        error_codes(&diagnostics)
    );
}

#[test]
fn destructuring_rest_element() {
    let source = r#"
        function f(arr: [number, string, boolean]) {
            const [first, ...rest] = arr;
            const n: number = first;
        }
    "#;
    let diagnostics = check_strict(source);
    assert!(
        !has_error_code(&diagnostics, 2322),
        "Destructuring rest should correctly type first element. Got: {:?}",
        error_codes(&diagnostics)
    );
}
