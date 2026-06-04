/// TS Unsoundness #40: Distributivity Disabling via [T] extends [U]
/// Tests the `is_distributive` flag parsing and lowering through conditional types.
/// Verifies that naked type parameters are marked distributive while tuple-wrapped are not.
/// Note: This test verifies the lowering behavior via the solver's `lower_tests.rs`,
/// and checks that the thin checker properly handles conditional type declarations.
#[test]
fn test_distributivity_conditional_type_declarations() {
    // Test that conditional type declarations parse and bind correctly
    let source = r#"
type Distributive<T> = T extends any ? true : false;
type NonDistributive<T> = [T] extends [any] ? true : false;

// Verify these type aliases are usable (no errors in declaration)
declare const x: Distributive<string>;
declare const y: NonDistributive<string>;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // No diagnostics expected for type declarations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #40: Conditional type parsing with concrete extends checks
/// Tests that conditional types with concrete types parse correctly.
/// Note: Conditional type evaluation during type alias assignment is tested in `solver/evaluate_tests.rs`.
#[test]
fn test_conditional_type_concrete_extends() {
    // Test that conditional types parse and bind correctly with concrete extends checks
    let source = r#"
// Direct conditional type definitions
type StringCheck = string extends string ? "yes" : "no";
type NumberCheck = number extends string ? "yes" : "no";
type TupleCheck = [string] extends [string] ? "yes" : "no";

// These declarations should parse and bind without errors
declare const s: StringCheck;
declare const n: NumberCheck;
declare const t: TupleCheck;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // No diagnostics expected for well-formed declarations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #40: Tuple-wrapped conditional types for non-distribution
/// Tests the [T] extends [U] pattern used to disable distributivity.
/// The `is_distributive` flag detection is verified in `solver/lower_tests.rs`.
#[test]
fn test_tuple_wrapped_conditional_pattern() {
    // Test the [T] extends [U] pattern used to disable distributivity
    let source = r#"
// Generic distributive conditional
type Dist<T> = T extends string ? true : false;

// Generic non-distributive conditional (tuple-wrapped)
type NonDist<T> = [T] extends [string] ? true : false;

// Complex conditional with infer
type ExtractElement<T> = T extends (infer U)[] ? U : never;

// Complex non-distributive with infer
type ExtractElementNonDist<T> = [T] extends [(infer U)[]] ? U : never;

// Declarations to verify parsing
declare const d: Dist<string>;
declare const nd: NonDist<string>;
declare const e: ExtractElement<string[]>;
declare const end: ExtractElementNonDist<string[]>;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // No diagnostics expected for well-formed declarations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Minimal repro: Conditional type with infer for extracting state type
/// Pattern: `R extends Reducer<infer S, any> ? S : never`
#[test]
fn test_redux_pattern_extract_state_with_infer() {
    let source = r#"
type Reducer<S, A> = (state: S | undefined, action: A) => S;

type ExtractState<R> = R extends Reducer<infer S, any> ? S : never;

// Test extraction: should infer S = number
type NumberReducer = Reducer<number, { type: string }>;
type ExtractedState = ExtractState<NumberReducer>;

// Verify the extracted state type
declare const s: ExtractedState;
const n: number = s;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Print diagnostics for debugging
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Redux Pattern: ExtractState Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "ExtractState pattern should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// Minimal repro: Mapped type over keyof with conditional extraction
/// Pattern: `{ [K in keyof R]: ExtractState<R[K]> }`
// TODO: Fix TS2304 for mapped type parameter K -- binder scope gap.
#[test]
fn test_redux_pattern_state_from_reducers_mapped() {
    let source = r#"
type Reducer<S, A> = (state: S | undefined, action: A) => S;
type AnyAction = { type: string };

type ExtractState<R> = R extends Reducer<infer S, AnyAction> ? S : never;

type StateFromReducers<R> = { [K in keyof R]: ExtractState<R[K]> };

interface Reducers {
    count: Reducer<number, AnyAction>;
    message: Reducer<string, AnyAction>;
}

type AppState = StateFromReducers<Reducers>;

// Verify the mapped type evaluates correctly
declare const state: AppState;
const c: number = state.count;
const m: string = state.message;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Redux Pattern: StateFromReducers Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "StateFromReducers mapped type should work: {:?}",
        checker.ctx.diagnostics
    );
}
