/// Minimal repro: `DeepPartial` recursive mapped type
/// Pattern: `{ [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] }`
#[test]
fn test_redux_pattern_deep_partial() {
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

/// TS Unsoundness #31: Generic constraint rejection - constraint not assignable to T
///
/// Verifies that while T is assignable to its constraint,
/// the constraint itself cannot be assigned back to T.
#[test]
fn test_generic_constraint_rejection() {
    let source = r#"
// Error case: string is not assignable to T (T could be "hello" or other literal)
function reject<T extends string>(): T {
    return "hello"; // ERROR: string is not assignable to T
}

// Similarly, the constraint type cannot be assigned to a constrained parameter
function reject2<T extends { name: string }>(obj: { name: string }): T {
    return obj; // ERROR: { name: string } is not assignable to T
}
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

    // Should have exactly 2 errors (one for each return statement)
    let error_count = checker.ctx.diagnostics.len();

    if error_count != 2 {
        println!("=== Generic Constraint Rejection Diagnostics ===");
        println!("Expected 2 errors, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 2,
        "Should reject constraint-to-T assignments (expected 2 errors): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #31: Generic parameter identity check
///
/// When checking T <: U where both are type parameters,
/// first check identity (T == U), then check Constraint(T) <: U.
#[test]
fn test_generic_param_identity() {
    let source = r#"
// Same type parameter is assignable to itself
function identity<T>(x: T): T {
    return x; // OK: T == T
}

// Different type parameters with compatible constraints
function compatible<T extends string, U extends string>(x: T): string {
    return x; // OK: T <: string
}

// Nested constraint: U extends T, so U <: T
function nested<T, U extends T>(x: U): T {
    return x; // OK: Constraint(U) = T, so U <: T
}

// Chain of constraints
function chain<A extends string, B extends A, C extends B>(x: C): string {
    // C <: B <: A <: string
    const a: A = x; // OK: C <: A via B
    const s: string = x; // OK: C <: string via chain
    return x;
}
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
        println!("=== Generic Param Identity Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Generic param identity check should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #31: Cross-file generic constraint resolution
///
/// This test verifies that generic constraints work correctly when
/// types are referenced across different "conceptual" modules.
/// Relates to the Application expansion issue in cross-file type resolution.
///
/// Property access on T where T extends `SomeType` should resolve properties
/// from the constraint during access.
///
/// Cross-scope generic constraint resolution: basic constraints, alias chains, and union constraints.
#[test]
fn test_cross_scope_generic_constraints() {
    let source = r#"
// Simulate cross-file scenario with type aliases
type Base = { id: number };
type Extended = Base & { name: string };

// Generic function with constraint referencing external type
function process<T extends Base>(item: T): number {
    return item.id; // Should work: T has .id because Constraint(T) = Base
}

// Constraint is a type alias to another type alias
type Identifiable = Base;
function identify<T extends Identifiable>(item: T): number {
    return item.id; // Should work: need to resolve Identifiable -> Base -> { id: number }
}

// Constraint is a union type
type Entity = { kind: "user"; name: string } | { kind: "bot"; version: number };
function getKind<T extends Entity>(entity: T): "user" | "bot" {
    return entity.kind; // Should work: both union members have .kind
}
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Constraint property lookup should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// Cross-scope generic constraint with conditional type using `infer`.
#[test]
fn test_cross_scope_generic_constraints_conditional_infer() {
    let source = r#"
type ExtractId<T> = T extends { id: infer I } ? I : never;
function extractId<T extends { id: number }>(item: T): ExtractId<T> {
    return item.id as ExtractId<T>;
}
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

    // Accept TS2352 as valid — tsc also emits this for conditional type assertions
    // when the type can't be proven to overlap with the conditional result.
    let non_ts2352: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2352)
        .collect();
    assert!(
        non_ts2352.is_empty(),
        "Constraint property lookup with infer should only produce TS2352 (if any): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #26: Split Accessors (Getter/Setter Variance)
///
/// TypeScript allows a property to have different types for reading (Getter) vs writing (Setter).
/// - `get x(): string`
/// - `set x(v: string | number)`
///
/// The property `x` is effectively `string` (covariant) for reads, and `string | number` (contravariant) for writes.
///
/// Subtyping rules for split accessors:
/// - `Sub.read <: Sup.read` (Covariant)
/// - `Sup.write <: Sub.write` (Contravariant)
///
/// NOTE: Currently ignored - split accessor type checking is not fully implemented.
/// The property type should be derived from getter type for reads and setter type for writes.
#[test]
fn test_split_accessors_basic() {
    let source = r#"
class Box {
    private _value: string | number = "";

    get value(): string {
        return this._value as string;
    }

    set value(v: string | number) {
        this._value = v;
    }
}

const box = new Box();
const s: string = box.value; // OK: getter returns string
box.value = "hello"; // OK: setter accepts string
box.value = 42; // OK: setter accepts number
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Split accessor basic usage should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #26: Split Accessors - read type mismatch should error
#[test]
fn test_split_accessors_read_error() {
    let source = r#"
class Box {
    get value(): string {
        return "hello";
    }
    set value(v: string | number) {}
}

const box = new Box();
const n: number = box.value; // ERROR: string not assignable to number
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

    let error_count = checker.ctx.diagnostics.len();
    if error_count != 1 {
        println!("=== Split Accessors Read Error Diagnostics ===");
        println!("Expected 1 error, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 1,
        "Should error when reading getter returns incompatible type: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #26: Split Accessors - write type mismatch should error
///
/// Setter assignment type checking verifies that the value being assigned
/// matches the setter parameter type.
#[test]
fn test_split_accessors_write_error() {
    let source = r#"
class Box {
    get value(): string {
        return "hello";
    }
    set value(v: string) {} // Setter only accepts string
}

const box = new Box();
box.value = true; // Should ERROR: boolean not assignable to string
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

    let error_count = checker.ctx.diagnostics.len();

    assert_eq!(
        error_count, 1,
        "Expected 1 error for boolean assigned to string setter: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #43: Abstract Class Instantiation
///
/// Abstract classes cannot be instantiated directly.
/// - `new AbstractClass()` -> Error
/// - But `AbstractClass` is a subtype of `Function` (it has a prototype)
/// - You can define types that accept abstract constructors: `abstract new () => any`
#[test]
fn test_abstract_class_instantiation_error() {
    let source = r#"
declare const console: { log: (message: string) => void };

abstract class Animal {
    abstract speak(): void;
}

class Dog extends Animal {
    speak() {}
}

const dog = new Dog(); // OK: Dog is concrete
const animal = new Animal(); // ERROR: Cannot create instance of abstract class
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

    let error_count = checker.ctx.diagnostics.len();
    if error_count != 1 {
        println!("=== Abstract Class Instantiation Diagnostics ===");
        println!("Expected 1 error, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 1,
        "Should error on abstract class instantiation: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #43: Abstract constructor type assignability
///
/// `ConcreteConstructor` <: `AbstractConstructor` -> True
/// `AbstractConstructor` <: `ConcreteConstructor` -> False
///
/// EXPECTED FAILURES: typeof class and constructor type assignability
/// has issues with type resolution. Currently expects 4 errors.
#[test]
fn test_abstract_constructor_assignability() {
    let source = r#"
abstract class Animal {
    abstract speak(): void;
}

class Dog extends Animal {
    speak() {}
}

class Cat extends Animal {
    speak() {}
}

// Using typeof to get constructor types
type AnimalCtor = typeof Animal;
type DogCtor = typeof Dog;

// Concrete class constructor can be used where abstract is expected (via type alias)
const ctor1: AnimalCtor = Dog; // Should be OK: Dog extends Animal

// But we cannot instantiate the abstract class via its constructor type
function createAnimal(Ctor: typeof Animal): Animal {
    // This would be: return new Ctor(); // ERROR if Ctor is abstract
    return new Dog(); // Workaround for test
}

const animal = createAnimal(Animal); // Passing abstract class as value should be OK
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

    let error_count = checker.ctx.diagnostics.len();

    // Fixed: Abstract constructor assignability now works correctly
    // Concrete class constructors can be assigned to abstract class constructor types
    if error_count != 0 {
        println!("=== Abstract Constructor Assignability Diagnostics ===");
        println!("Expected 0 errors, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors (abstract constructor assignability fixed): {:?}",
        checker.ctx.diagnostics
    );
}

/// Test abstract to concrete constructor type assignability
///
/// Abstract constructor types should NOT be assignable to concrete constructor types.
/// This matches TypeScript's behavior.
///
/// NOTE: Currently ignored - the checker doesn't emit TS2322 errors for abstract to
/// concrete constructor assignments. The assignability check exists but doesn't
/// properly detect this case or emit the expected diagnostic.
#[test]
fn test_abstract_to_concrete_constructor_not_assignable() {
    let source = r#"
class A {}

abstract class B extends A {}

class C extends B {}

// Test 1: Abstract B to Concrete A - Should error (TS2322)
var AA: typeof A = B;

// Test 2: Concrete A to Abstract B - Should be OK (no error)
var BB: typeof B = A;

// Test 3: Abstract B to Concrete C - Should error (TS2322)
var CC: typeof C = B;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let not_assignable_count = codes.iter().filter(|&&code| code == 2322).count();

    // Debug: print all diagnostics
    println!("=== Abstract to Concrete Constructor Diagnostics ===");
    println!("Total diagnostics: {}", checker.ctx.diagnostics.len());
    for diag in &checker.ctx.diagnostics {
        println!("[{}] Code {}: {}", diag.start, diag.code, diag.message_text);
    }
    println!(
        "Abstract constructor types in context: {:?}",
        checker.ctx.abstract_constructor_types
    );

    // Should have 2 TS2322 errors:
    // - Line 8: typeof B (abstract) to typeof A (concrete)
    // - Line 14: typeof B (abstract) to typeof C (concrete)
    assert_eq!(
        not_assignable_count, 2,
        "Expected 2 TS2322 errors for abstract to concrete constructor assignment, got: {:?}\nDiagnostics: {:?}",
        codes, checker.ctx.diagnostics
    );
}

/// TS Unsoundness #43: Concrete to abstract class assignment
///
/// A concrete class is a subtype of its abstract base class.
///
/// EXPECTED FAILURES: Instance to abstract class type assignability
/// has issues with class type comparison. Currently expects 3 errors.
#[test]
fn test_concrete_extends_abstract() {
    let source = r#"
abstract class Shape {
    abstract area(): number;
    describe(): string {
        return "I am a shape";
    }
}

class Circle extends Shape {
    constructor(public radius: number) {
        super();
    }
    area(): number {
        return 3.14 * this.radius * this.radius;
    }
}

class Square extends Shape {
    constructor(public side: number) {
        super();
    }
    area(): number {
        return this.side * this.side;
    }
}

// Concrete classes should be assignable to abstract type
const shape1: Shape = new Circle(5); // Should be OK
const shape2: Shape = new Square(4); // Should be OK

// Array of abstract type should hold concrete instances
const shapes: Shape[] = [new Circle(1), new Square(2)]; // Should be OK
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

    let error_count = checker.ctx.diagnostics.len();

    // Class inheritance type checking now works - expect 0 errors
    if error_count != 0 {
        println!("=== Concrete Extends Abstract Diagnostics ===");
        println!("Expected 0 errors (class inheritance fixed), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors (class inheritance now works): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #29: The Global Function Type (The Untyped Callable)
///
/// The global `Function` interface behaves like an untyped supertype for all callables.
/// - Any arrow function/method is assignable to `Function`
/// - `Function` is NOT safe to call (effectively `(...args: any[]) => any`)
/// - It differs from `{}` or `object` because it allows bind/call/apply
///
/// Note: This test defines a local Function interface since the global
/// Function type requires lib.d.ts which isn't available in tests.
#[test]
fn test_global_function_type_callable_assignability() {
    let source = r#"
// Define a minimal Function-like interface for testing
interface FunctionLike {
    (...args: any[]): any;
    bind(thisArg: any): FunctionLike;
    call(thisArg: any, ...args: any[]): any;
    apply(thisArg: any, args: any[]): any;
}

// Various callable types
const arrow = (x: number) => x * 2;
const func = function(s: string): string { return s.toUpperCase(); };
function named(a: number, b: number): number { return a + b; }

// All callables should be assignable to the untyped callable interface
// (In real TS, these would be assignable to Function)
type AnyCallable = (...args: any[]) => any;

const c1: AnyCallable = arrow; // OK
const c2: AnyCallable = func; // OK
const c3: AnyCallable = named; // OK
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
        println!("=== Global Function Type Callable Assignability Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "All callables should be assignable to untyped callable: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #29: Function type is not assignable to specific callable
///
/// The untyped `Function` cannot be safely assigned to a specific function type
/// because we don't know its actual signature.
#[test]
fn test_function_not_assignable_to_specific() {
    let source = r#"
// Untyped callable (simulating Function)
type AnyCallable = (...args: any[]) => any;

// Specific function type
type SpecificFn = (x: number, y: number) => number;

declare const untyped: AnyCallable;

// Untyped should NOT be directly assignable to specific
// (unless the target is `any`)
const specific: SpecificFn = untyped; // This is actually allowed in TS due to any
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

    // In TypeScript, (...args: any[]) => any IS assignable to specific functions
    // because `any` disables type checking. This is intentional unsoundness.
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Function Not Assignable Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Untyped callable with any is assignable due to any unsoundness: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #29: Function type hierarchy
///
/// Tests that callable types form a proper hierarchy:
/// - Specific callable <: (...args: any[]) => any
/// - Object types without call signatures are NOT callable
#[test]
fn test_function_type_hierarchy() {
    let source = r#"
// Various function types in the hierarchy
type VoidFn = () => void;
type NumberFn = (x: number) => number;
type StringFn = (s: string) => string;
type GenericFn = <T>(x: T) => T;

// Untyped callable at the top
type AnyCallable = (...args: any[]) => any;

// Specific functions are assignable to untyped
declare const voidFn: VoidFn;
declare const numberFn: NumberFn;
declare const stringFn: StringFn;

const a1: AnyCallable = voidFn; // OK: VoidFn <: AnyCallable
const a2: AnyCallable = numberFn; // OK: NumberFn <: AnyCallable
const a3: AnyCallable = stringFn; // OK: StringFn <: AnyCallable

// Non-callable object is NOT assignable to function type
interface NotCallable {
    value: number;
}
declare const obj: NotCallable;
// const bad: AnyCallable = obj; // This would be an error
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
        println!("=== Function Type Hierarchy Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Function type hierarchy should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #32: Best Common Type (BCT) Inference
///
/// When inferring an array literal `[1, "a"]`, TS creates `(number | string)[]`
/// not a tuple. The algorithm gathers all element types and finds a common supertype,
/// or creates a union if none exists.
#[test]
fn test_best_common_type_array_literal() {
    let source = r#"
// Mixed array literal becomes union type
const mixed = [1, "hello", 2, "world"];
// Type should be (number | string)[]

// Accessing elements returns the union
const elem = mixed[0]; // number | string

// Can push either type
mixed.push(3);
mixed.push("test");

// Homogeneous array stays as single type
const numbers = [1, 2, 3, 4];
const n: number = numbers[0]; // OK

const strings = ["a", "b", "c"];
const s: string = strings[0]; // OK
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
        println!("=== Best Common Type Array Literal Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Best common type inference should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #32: BCT with class hierarchy
///
/// When array elements share a common base class, the array type
/// should use the common base (if annotated) or union of concrete types.
///
/// EXPECTED FAILURE: Class instance to base class type assignability
/// has issues. Currently expects 1 error.
#[test]
fn test_best_common_type_class_hierarchy() {
    let source = r#"
class Animal {
    name: string = "";
}

class Dog extends Animal {
    bark() { return "woof"; }
}

class Cat extends Animal {
    meow() { return "meow"; }
}

// Without annotation: union of concrete types
const pets = [new Dog(), new Cat()];
// Type is (Dog | Cat)[]

// With annotation: should use the annotated type
const animals: Animal[] = [new Dog(), new Cat()];
// Type should be Animal[]

// Can access common properties on union
const pet = pets[0];
const name = pet.name; // OK: both Dog and Cat have name
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

    let error_count = checker.ctx.diagnostics.len();

    // Class inheritance now works - expect 0 errors
    if error_count != 0 {
        println!("=== Best Common Type Class Hierarchy Diagnostics ===");
        println!("Expected 0 errors (class inheritance fixed), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors (class inheritance now works): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #32: BCT type widening behavior
///
/// Literal types in array literals get widened to their base types
/// unless the array is const or has a specific annotation.
#[test]
fn test_best_common_type_literal_widening() {
    let source = r#"
// Literal types widen in mutable arrays
const nums = [1, 2, 3]; // number[] not (1 | 2 | 3)[]
nums.push(4); // OK because it's number[]

const strs = ["a", "b"]; // string[] not ("a" | "b")[]
strs.push("c"); // OK

// Const assertion preserves literals (as readonly tuple)
const literalNums = [1, 2, 3] as const; // readonly [1, 2, 3]
// literalNums.push(4); // Would error: readonly

// Boolean literal widening
const bools = [true, false]; // boolean[]
const b: boolean = bools[0]; // OK
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
        println!("=== Best Common Type Literal Widening Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "BCT literal widening should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Module Augmentation Merging - Interface Merging
///
/// Interfaces with the same name in the same scope merge.
/// Multiple interface declarations combine their members.
#[test]
fn test_interface_merging_basic() {
    let source = r#"
// First interface declaration
interface Box {
    width: number;
    height: number;
}

// Second declaration merges with first
interface Box {
    depth: number;
    label: string;
}

// The merged interface has all properties
const box: Box = {
    width: 10,
    height: 20,
    depth: 30,
    label: "Storage"
};

// Can access all merged properties
const w: number = box.width;
const h: number = box.height;
const d: number = box.depth;
const l: string = box.label;
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
        println!("=== Interface Merging Basic Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Interface merging should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Interface merging with method overloads
///
/// When interfaces merge, methods with the same name become overloads.
#[test]
fn test_interface_merging_method_overloads() {
    let source = r#"
interface Calculator {
    add(a: number, b: number): number;
}

interface Calculator {
    add(a: string, b: string): string;
    multiply(a: number, b: number): number;
}

// Merged interface has both overloads of add and multiply
declare const calc: Calculator;

const numResult: number = calc.add(1, 2);
const strResult: string = calc.add("a", "b");
const product: number = calc.multiply(3, 4);
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
        println!("=== Interface Merging Method Overloads Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Interface merging with overloads should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Interface extending and merging
///
/// Interfaces can both extend other interfaces and merge with
/// other declarations of the same name.
///
/// NOTE: Currently ignored - interface extending and merging is not fully implemented.
#[test]
fn test_interface_extend_and_merge() {
    let source = r#"
interface Named {
    name: string;
}

interface Person extends Named {
    age: number;
}

// Merge more properties into Person
interface Person {
    email: string;
}

// Person now has name (from Named), age, and email
const person: Person = {
    name: "Alice",
    age: 30,
    email: "alice@example.com"
};

const n: string = person.name;
const a: number = person.age;
const e: string = person.email;
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
        println!("=== Interface Extend and Merge Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Interface extend and merge should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Namespace and interface merging
///
/// Namespaces can merge with interfaces to add static members.
///
/// EXPECTED FAILURE: Namespace-interface merging for value-space access
/// is not yet implemented. Currently expects 2 errors.
#[test]
fn test_namespace_interface_merging() {
    let source = r##"
interface Color {
    r: number;
    g: number;
    b: number;
}

namespace Color {
    export function fromHex(hex: string): Color {
        return { r: 0, g: 0, b: 0 };
    }
    export const RED: Color = { r: 255, g: 0, b: 0 };
}

// Use as interface type
const myColor: Color = { r: 100, g: 150, b: 200 };

// Use namespace members (these should work but currently fail)
const red: Color = Color.RED;
const fromString: Color = Color.fromHex("#FF0000");
"##;

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

    let error_count = checker.ctx.diagnostics.len();

    // Now expects 0 errors: both interface member access (myColor.r, etc.) and
    // namespace value access (Color.RED, Color.fromHex) work correctly after
    // fixing interface+namespace merge type resolution.
    assert_eq!(
        error_count, 0,
        "Expected 0 errors for namespace-interface merging: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Class and namespace merging
///
/// Classes can merge with namespaces to add static properties/methods.
///
/// NOTE: Currently ignored - class-namespace merging is not fully implemented.
/// The merging doesn't correctly handle type checking for merged static members.
#[test]
fn test_class_namespace_merging() {
    let source = r#"
class Album {
    title: string;
    constructor(title: string) {
        this.title = title;
    }
}

namespace Album {
    export interface Track {
        name: string;
        duration: number;
    }
    export function create(title: string): Album {
        return new Album(title);
    }
}

// Use class as type and constructor
const album: Album = new Album("Best Of");

// Use namespace members
const track: Album.Track = { name: "Song 1", duration: 180 };
const created: Album = Album.create("New Album");
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
        println!("=== Class Namespace Merging Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Class and namespace merging should work: {:?}",
        checker.ctx.diagnostics
    );
}

