// Tests for Checker - Type checker using `NodeArena` and Solver
//
// This module contains comprehensive type checking tests organized into categories:
// - Basic type checking (creation, intrinsic types, type interning)
// - Type compatibility and assignability
// - Excess property checking
// - Function overloads and call resolution
// - Generic types and type inference
// - Control flow analysis
// - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
/// TS Unsoundness #40: Distributivity Disabling via [T] extends [U]
/// Tests the `is_distributive` flag parsing and lowering through conditional types.
/// Verifies that naked type parameters are marked distributive while tuple-wrapped are not.
/// Note: This test verifies the lowering behavior via the solver's `lower_tests.rs`,
/// and checks that the thin checker properly handles conditional type declarations.
#[test]
fn test_distributivity_conditional_type_declarations() {
    use crate::parser::ParserState;

    // Test that conditional type declarations parse and bind correctly
    let source = r#"
type Distributive<T> = T extends any ? true : false;
type NonDistributive<T> = [T] extends [any] ? true : false;

// Verify these type aliases are usable (no errors in declaration)
declare const x: Distributive<string>;
declare const y: NonDistributive<string>;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

// =========================================================================
// Redux/Lodash Pattern Minimal Repros (Support for Worker 2)
// These tests isolate specific patterns from test_check_redux_lodash_style_generics
// =========================================================================

/// Minimal repro: Conditional type with infer for extracting state type
/// Pattern: `R extends Reducer<infer S, any> ? S : never`
#[test]
fn test_redux_pattern_extract_state_with_infer() {
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

/// Minimal repro: `DeepPartial` recursive mapped type
/// Pattern: `{ [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] }`
#[test]
fn test_redux_pattern_deep_partial() {
    use crate::parser::ParserState;

    let source = r#"
type DeepPartial<T> = {
    [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K];
};

interface State {
    count: number;
    message: string;
    nested: { value: number };
}

type PartialState = DeepPartial<State>;

// Verify partial assignment works
const patch: PartialState = { message: "ok" };
const partial: PartialState = { nested: { value: 42 } };
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        println!("=== Redux Pattern: DeepPartial Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "DeepPartial mapped type should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// Minimal repro: Generic function returning conditional type
/// Pattern: `function createStore<R>(r: R): Store<StateFromReducer<R>>`
///
/// NOTE: Currently ignored - see `test_redux_pattern_reducers_map_object`.
#[test]
fn test_redux_pattern_generic_function_with_conditional_return() {
    use crate::parser::ParserState;

    let source = r#"
type Reducer<S> = (state: S | undefined) => S;
type ExtractState<R> = R extends Reducer<infer S> ? S : never;

interface Store<S> {
    getState: () => S;
}

function createStore<R extends Reducer<number>>(reducer: R): Store<ExtractState<R>> {
    return { getState: () => ({} as ExtractState<R>) };
}

const numberReducer: Reducer<number> = (state) => state ?? 0;
const store = createStore(numberReducer);

// The returned store should have getState returning number
const state = store.getState();
const n: number = state;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        println!("=== Redux Pattern: createStore Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Accept TS2352 as valid — conditional type assertion overlap check
    let non_ts2352: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2352)
        .collect();
    assert!(
        non_ts2352.is_empty(),
        "Generic function with conditional return should only produce TS2352 (if any): {:?}",
        checker.ctx.diagnostics
    );
}

/// Minimal repro: Index access on union to extract union of types
/// Pattern: `ActionFromReducers<R> = { [K in keyof R]: ExtractAction<R[K]> }[keyof R]`
///
// TODO: Fix TS2304 for mapped type parameter K -- binder scope gap.
#[test]
fn test_redux_pattern_indexed_access_on_mapped_union() {
    use crate::parser::ParserState;

    let source = r#"
type AnyAction = { type: string };
type Reducer<S, A extends AnyAction> = (state: S | undefined, action: A) => S;

type ExtractAction<R> = R extends Reducer<any, infer A> ? A : never;

type ActionFromReducers<R> = { [K in keyof R]: ExtractAction<R[K]> }[keyof R];

interface Reducers {
    count: Reducer<number, { type: "inc" } | { type: "dec" }>;
    message: Reducer<string, { type: "set"; payload: string }>;
}

type AllActions = ActionFromReducers<Reducers>;

// AllActions should be the union of all action types
declare const action: AllActions;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        println!("=== Redux Pattern: ActionFromReducers Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Indexed access on mapped type union should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// Minimal repro: `ReducersMapObject` constraint with homomorphic mapped type
/// Pattern: `type ReducersMapObject<S, A> = { [K in keyof S]: Reducer<S[K], A> }`
///
/// NOTE: Currently ignored - complex Redux pattern type inference is not fully implemented.
/// Homomorphic mapped types with conditional constraints are not correctly resolved.
// TODO: Fix TS2304 for mapped type parameter K -- binder scope gap.
#[test]
fn test_redux_pattern_reducers_map_object() {
    use crate::parser::ParserState;

    let source = r#"
type AnyAction = { type: string; payload?: any };
type Reducer<S, A extends AnyAction> = (state: S | undefined, action: A) => S;

type ReducersMapObject<S, A extends AnyAction> = {
    [K in keyof S]: Reducer<S[K], A>;
};

interface RootState {
    count: number;
    message: string;
}

type RootReducers = ReducersMapObject<RootState, AnyAction>;

// Create concrete reducers
const counterReducer: Reducer<number, AnyAction> = (state, action) => state ?? 0;
const messageReducer: Reducer<string, AnyAction> = (state, action) => state ?? "";

// This should type-check: reducers match the expected shape
const reducers: RootReducers = {
    count: counterReducer,
    message: messageReducer,
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        println!("=== Redux Pattern: ReducersMapObject Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "ReducersMapObject constraint should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #31: Base Constraint Assignability (Generic Erasure)
///
/// Inside a generic function, when checking `T <: U`:
/// - If `T` and `U` are generic parameters, we check their constraints
/// - Rule: `T <: U` if `Constraint(T) <: U`
/// - Rule: `T <: Constraint(T)` is always true
/// - A type parameter T can be assigned to its constraint
/// - But the constraint cannot be assigned back to T (T could be narrower)
///
/// This relates to cross-file generics because constraint checking requires
/// proper instantiation and resolution of type parameter bounds.
#[test]
fn test_base_constraint_assignability() {
    use crate::parser::ParserState;

    let source = r#"
// T extends string, so T can be assigned to string
function f<T extends string>(x: T): string {
    return x; // OK: T <: string because Constraint(T) = string
}

// But string cannot be assigned to T - T could be a narrower type
function g<T extends string>(x: T): T {
    // return "hello"; // This would be an error
    return x; // OK: must return x (which is of type T)
}

// Multiple constraints interact
function h<T extends string, U extends T>(x: U): T {
    return x; // OK: U <: T because Constraint(U) = T
}

// Constraint to constraint comparison
function i<T extends string, U extends number>(x: T, y: U): string | number {
    // Both T and U are assignable to their respective constraints
    const a: string = x; // OK
    const b: number = y; // OK
    return x; // OK: T <: string <: string | number
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        println!("=== Base Constraint Assignability Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Base constraint assignability should work: {:?}",
        checker.ctx.diagnostics
    );
}
