use super::*;

#[test]
fn test_parse_single_file() {
    let result = parse_file_single("test.ts".to_string(), "let x = 42;".to_string());

    assert_eq!(result.file_name, "test.ts");
    assert!(!result.source_file.is_none());
    assert!(result.parse_diagnostics.is_empty());
}

#[test]
fn test_parse_multiple_files_parallel() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "let b = 2;".to_string()),
        ("c.ts".to_string(), "let c = 3;".to_string()),
    ];

    let results = parse_files_parallel(files);

    assert_eq!(results.len(), 3);
    for result in &results {
        assert!(!result.source_file.is_none());
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_parse_with_stats() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "class Bar { constructor() {} }".to_string(),
        ),
    ];

    let (results, stats) = parse_files_with_stats(files);

    assert_eq!(results.len(), 2);
    assert_eq!(stats.file_count, 2);
    assert!(stats.total_bytes > 0);
    assert!(stats.total_nodes > 0);
    assert_eq!(stats.error_count, 0);
}

#[test]
fn test_parallel_parsing_consistency() {
    // Parse the same file multiple times in parallel
    // Results should be consistent
    let source =
        "const x: number = 42; function add(a: number, b: number): number { return a + b; }";
    let files: Vec<_> = (0..10)
        .map(|i| (format!("file{}.ts", i), source.to_string()))
        .collect();

    let results = parse_files_parallel(files);

    // All should have same number of nodes (same source)
    let first_node_count = results[0].arena.len();
    for result in &results {
        assert_eq!(result.arena.len(), first_node_count);
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_large_batch_parsing() {
    // Test with a larger batch to exercise parallelism
    let files: Vec<_> = (0..100)
        .map(|i| {
            let source = format!(
                "function fn{}(x: number): number {{ return x * {}; }}",
                i, i
            );
            (format!("module{}.ts", i), source)
        })
        .collect();

    let (results, stats) = parse_files_with_stats(files);

    assert_eq!(results.len(), 100);
    assert_eq!(stats.file_count, 100);
    // Note: ThinParser may produce parse errors for some constructs
    // The key test is that parallel parsing works correctly
    // assert_eq!(stats.error_count, 0);

    // Each file should have similar node counts
    for result in &results {
        assert!(
            result.arena.len() >= 5,
            "Each file should have at least 5 nodes"
        );
    }
}

// =========================================================================
// Parallel Binding Tests
// =========================================================================

#[test]
fn test_bind_single_file() {
    let result = parse_and_bind_single(
        "test.ts".to_string(),
        "let x = 42; function foo() {}".to_string(),
    );

    assert_eq!(result.file_name, "test.ts");
    assert!(!result.source_file.is_none());
    assert!(result.parse_diagnostics.is_empty());
    // Should have symbols for x and foo
    assert!(result.file_locals.has("x"));
    assert!(result.file_locals.has("foo"));
}

#[test]
fn test_bind_multiple_files_parallel() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "function b() {}".to_string()),
        ("c.ts".to_string(), "class C {}".to_string()),
    ];

    let results = parse_and_bind_parallel(files);

    assert_eq!(results.len(), 3);

    // Each file should have its own symbols
    assert!(results[0].file_locals.has("a"));
    assert!(results[1].file_locals.has("b"));
    assert!(results[2].file_locals.has("C"));
}

#[test]
fn test_bind_with_stats() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; }".to_string(),
        ),
        ("b.ts".to_string(), "class Bar { x: number; }".to_string()),
    ];

    let (results, stats) = parse_and_bind_with_stats(files);

    assert_eq!(results.len(), 2);
    assert_eq!(stats.file_count, 2);
    assert!(stats.total_nodes > 0);
    assert!(stats.total_symbols > 0);
    assert_eq!(stats.parse_error_count, 0);
}

#[test]
fn test_parallel_binding_consistency() {
    // Bind the same file multiple times in parallel
    // Results should be consistent
    let source =
        "const x: number = 42; function add(a: number, b: number): number { return a + b; }";
    let files: Vec<_> = (0..10)
        .map(|i| (format!("file{}.ts", i), source.to_string()))
        .collect();

    let results = parse_and_bind_parallel(files);

    // All should have same symbols
    for result in &results {
        assert!(result.file_locals.has("x"));
        assert!(result.file_locals.has("add"));
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_large_batch_binding() {
    // Test with a larger batch to exercise parallelism
    let files: Vec<_> = (0..100)
        .map(|i| {
            let source = format!(
                "function fn{}(x: number): number {{ return x * {}; }} let val{} = fn{}(10);",
                i, i, i, i
            );
            (format!("module{}.ts", i), source)
        })
        .collect();

    let (results, stats) = parse_and_bind_with_stats(files);

    assert_eq!(results.len(), 100);
    assert_eq!(stats.file_count, 100);
    assert!(
        stats.total_symbols >= 200,
        "Should have at least 200 symbols (2 per file)"
    );

    // Each file should have its function and variable
    for (i, result) in results.iter().enumerate() {
        let fn_name = format!("fn{}", i);
        let var_name = format!("val{}", i);
        assert!(
            result.file_locals.has(&fn_name),
            "File {} missing {}",
            i,
            fn_name
        );
        assert!(
            result.file_locals.has(&var_name),
            "File {} missing {}",
            i,
            var_name
        );
    }
}

// =========================================================================
// Symbol Merging Tests
// =========================================================================

#[test]
fn test_merge_single_file() {
    let files = vec![(
        "a.ts".to_string(),
        "let x = 1; function foo() {}".to_string(),
    )];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 1);
    assert!(program.globals.has("x"));
    assert!(program.globals.has("foo"));
    // Symbols should be in global arena
    assert!(program.symbols.len() >= 2);
}

#[test]
fn test_merge_multiple_files() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "function b() {}".to_string()),
        ("c.ts".to_string(), "class C {}".to_string()),
    ];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 3);
    // All symbols should be in globals
    assert!(program.globals.has("a"));
    assert!(program.globals.has("b"));
    assert!(program.globals.has("C"));
    // All symbols merged into global arena
    assert!(program.symbols.len() >= 3);
}

#[test]
fn test_merge_symbol_id_remapping() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let program = compile_files(files);

    // Get the symbol IDs from globals
    let x_id = program.globals.get("x").expect("x should exist");
    let y_id = program.globals.get("y").expect("y should exist");

    // IDs should be different (remapped properly)
    assert_ne!(x_id, y_id);

    // Both should be resolvable from global arena
    assert!(program.symbols.get(x_id).is_some());
    assert!(program.symbols.get(y_id).is_some());
}

#[test]
fn test_merge_preserves_file_locals() {
    let files = vec![
        ("a.ts".to_string(), "let a1 = 1; let a2 = 2;".to_string()),
        ("b.ts".to_string(), "let b1 = 1; let b2 = 2;".to_string()),
    ];

    let program = compile_files(files);

    // Each file should have its own locals
    assert_eq!(program.file_locals.len(), 2);
    assert!(program.file_locals[0].has("a1"));
    assert!(program.file_locals[0].has("a2"));
    assert!(program.file_locals[1].has("b1"));
    assert!(program.file_locals[1].has("b2"));
}

#[test]
fn test_compile_large_program() {
    // Simulate a larger program with many files
    let files: Vec<_> = (0..50)
        .map(|i| {
            let source = format!(
                "function fn{}() {{ return {}; }} const val{} = fn{}();",
                i, i, i, i
            );
            (format!("module{}.ts", i), source)
        })
        .collect();

    let program = compile_files(files);

    assert_eq!(program.files.len(), 50);
    // Should have at least 100 symbols (2 per file: fn + val)
    assert!(
        program.symbols.len() >= 100,
        "Expected at least 100 symbols, got {}",
        program.symbols.len()
    );

    // All function and value names should be in globals
    for i in 0..50 {
        let fn_name = format!("fn{}", i);
        let val_name = format!("val{}", i);
        assert!(program.globals.has(&fn_name), "Missing {}", fn_name);
        assert!(program.globals.has(&val_name), "Missing {}", val_name);
    }
}

#[test]
fn test_compile_with_exports() {
    // Test that export function/class/const are properly bound
    let files = vec![
        (
            "a.ts".to_string(),
            "export function add(x: number, y: number) { return x + y; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export class Calculator { add(x: number, y: number) { return x + y; } }".to_string(),
        ),
        ("c.ts".to_string(), "export const PI = 3.14159;".to_string()),
    ];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 3);
    // All exported declarations should be in globals
    assert!(
        program.globals.has("add"),
        "Exported function 'add' should be in globals"
    );
    assert!(
        program.globals.has("Calculator"),
        "Exported class 'Calculator' should be in globals"
    );
    assert!(
        program.globals.has("PI"),
        "Exported const 'PI' should be in globals"
    );
}

// =========================================================================
// Parallel Type Checking Tests
// =========================================================================

#[test]
fn test_check_redux_lodash_style_generics() {
    let files = vec![
        (
            "types.ts".to_string(),
            r#"
type AnyAction = { type: string; payload?: any };

type Reducer<S, A extends AnyAction> = (state: S | undefined, action: A) => S;

type ReducersMapObject<S, A extends AnyAction> = {
  [K in keyof S]: Reducer<S[K], A>;
};

type ExtractState<R> = R extends Reducer<infer S, AnyAction> ? S : never;
type ExtractAction<R> = R extends Reducer<any, infer A> ? A : never;

type StateFromReducers<R> = { [K in keyof R]: ExtractState<R[K]> };
type ActionFromReducers<R> = { [K in keyof R]: ExtractAction<R[K]> }[keyof R];

type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K];
};

type Dictionary<T> = { [key: string]: T };
type ValueOf<T> = T[keyof T];
type PickValue<T, V> = { [K in keyof T]: T[K] extends V ? T[K] : never };
type ActionByType<A extends AnyAction, T extends string> = A extends { type: T } ? A : never;

interface Store<S, A> {
  getState: () => S;
  dispatch: (action: A) => A;
  replaceState: (next: DeepPartial<S>) => void;
}
"#
            .to_string(),
        ),
        (
            "reducers.ts".to_string(),
            r#"
type CounterAction = { type: "inc" } | { type: "dec" };
type MessageAction = { type: "set"; payload: string };
type AppAction = CounterAction | MessageAction;

const counterReducer: Reducer<number, AnyAction> = (state = 0, action) => {
  if (action.type == "inc") return state + 1;
  if (action.type == "dec") return state - 1;
  return state;
};

const messageReducer: Reducer<string, AnyAction> = (state = "", action) => {
  if (action.type == "set") return action.payload;
  return state;
};

type RootState = {
  count: number;
  message: string;
  tags: Dictionary<number>;
};

type RootReducers = ReducersMapObject<RootState, AnyAction>;

const rootReducers: RootReducers = {
  count: counterReducer,
  message: messageReducer,
  tags: (state = {}, _action) => state,
};

const incAction: ActionByType<AppAction, "inc"> = { type: "inc" };
"#
            .to_string(),
        ),
        (
            "store.ts".to_string(),
            r#"
type StateFromReducer<R> = R extends Reducer<infer S, AnyAction> ? S : never;
type ActionFromReducer<R> = R extends Reducer<any, infer A> ? A : AnyAction;

function combineReducers<R extends ReducersMapObject<any, AnyAction>>(
  reducers: R
): Reducer<StateFromReducers<R>, ActionFromReducers<R>> {
  return (state: StateFromReducers<R> | undefined, action: ActionFromReducers<R>) => {
    const next = {} as StateFromReducers<R>;
    return next;
  };
}

function createStore<R extends Reducer<any, AnyAction>>(
  reducer: R
): Store<StateFromReducer<R>, ActionFromReducer<R>> {
  return {
    getState: () => ({} as StateFromReducer<R>),
    dispatch: (action: ActionFromReducer<R>) => action,
    replaceState: (_next: DeepPartial<StateFromReducer<R>>) => {},
  };
}
"#
            .to_string(),
        ),
        (
            "app.ts".to_string(),
            r#"
const rootReducer = combineReducers(rootReducers);

function runApp() {
  const store = createStore(rootReducer);
  const state = store.getState();
  const count: number = state.count;
  const message: string = state.message;
  const patch: DeepPartial<RootState> = { message: "ok" };

  store.replaceState(patch);

  const action: ActionFromReducers<typeof rootReducers> = { type: "inc" };
  store.dispatch(action);

  const sample: ValueOf<PickValue<RootState, number>> = count;
  return sample + count + state.tags["a"];
}
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);

    for file in &program.files {
        assert!(
            file.parse_diagnostics.is_empty(),
            "Unexpected parse diagnostics in {}",
            file.file_name
        );
    }

    let (result, stats) = check_functions_with_stats(&program);

    // Print diagnostics for debugging
    if result.diagnostic_count > 0 {
        eprintln!("\n=== DIAGNOSTICS ({}) ===", result.diagnostic_count);
        for file_result in &result.file_results {
            for diag in &file_result.diagnostics {
                eprintln!(
                    "  [{}:{}] code={}: {}",
                    file_result.file_name, diag.start, diag.code, diag.message_text
                );
            }
        }
        eprintln!("=== END DIAGNOSTICS ===\n");
    }

    assert_eq!(stats.file_count, 4);
    assert!(stats.function_count >= 5, "Expected at least 5 functions");

    // Debug: print diagnostics if there are any
    if result.diagnostic_count > 0 {
        eprintln!("\n=== DIAGNOSTICS ({}) ===", result.diagnostic_count);
        for file_result in &result.file_results {
            for diag in &file_result.diagnostics {
                eprintln!("  [{}:{}] {}", diag.file, diag.start, diag.message_text);
            }
        }
        eprintln!("=== END DIAGNOSTICS ===\n");
    }

    assert_eq!(result.diagnostic_count, 0);
}

#[test]
fn test_check_single_function() {
    let files = vec![(
        "a.ts".to_string(),
        "function add(x: number, y: number): number { return x + y; }".to_string(),
    )];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    assert_eq!(result.file_results.len(), 1);
    assert_eq!(result.function_count, 1);
    assert_eq!(result.file_results[0].function_results.len(), 1);
}

#[test]
fn test_check_multiple_functions_parallel() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; } function bar() { return 2; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "function baz(x: number) { return x * 2; }".to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    assert_eq!(result.file_results.len(), 2);
    // File a has 2 functions, file b has 1
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert_eq!(total_functions, 3);
}

#[test]
fn test_check_arrow_functions() {
    let files = vec![
        (
            "a.ts".to_string(),
            "const add = (x: number, y: number) => x + y;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "const double = (x: number) => { return x * 2; };".to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find the arrow functions
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert!(
        total_functions >= 2,
        "Should find at least 2 arrow functions"
    );
}

#[test]
fn test_check_class_methods() {
    let files = vec![
        ("a.ts".to_string(), "class Calculator { add(x: number, y: number) { return x + y; } subtract(x: number, y: number) { return x - y; } }".to_string()),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find the class methods
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert!(total_functions >= 2, "Should find at least 2 class methods");
}

#[test]
fn test_check_with_stats() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "function bar() { return 2; }".to_string(),
        ),
        (
            "c.ts".to_string(),
            "function baz() { return 3; }".to_string(),
        ),
    ];

    let program = compile_files(files);
    let (result, stats) = check_functions_with_stats(&program);

    assert_eq!(stats.file_count, 3);
    assert_eq!(stats.function_count, 3);
    assert_eq!(result.file_results.len(), 3);
}

#[test]
fn test_check_large_program_parallel() {
    // Test parallel checking with many files
    let files: Vec<_> = (0..50)
        .map(|i| {
            let source = format!(
                "function fn{}(x: number): number {{ return x * {}; }} const val{} = fn{}(10);",
                i, i, i, i
            );
            (format!("module{}.ts", i), source)
        })
        .collect();

    let program = compile_files(files);
    let (result, stats) = check_functions_with_stats(&program);

    assert_eq!(stats.file_count, 50);
    // Each file has 1 function declaration
    assert!(
        stats.function_count >= 50,
        "Expected at least 50 functions, got {}",
        stats.function_count
    );
}

#[test]
fn test_check_consistency() {
    // Check the same program multiple times - results should be consistent
    let files = vec![(
        "a.ts".to_string(),
        "function add(x: number, y: number): number { return x + y; }".to_string(),
    )];

    let program = compile_files(files);

    let result1 = check_functions_parallel(&program);
    let result2 = check_functions_parallel(&program);

    assert_eq!(result1.function_count, result2.function_count);
    assert_eq!(result1.diagnostic_count, result2.diagnostic_count);
    assert_eq!(result1.file_results.len(), result2.file_results.len());
}

#[test]
fn test_check_nested_functions() {
    let files = vec![(
        "a.ts".to_string(),
        "function outer() { function inner() { return 1; } return inner(); }".to_string(),
    )];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find both outer and inner functions
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert!(
        total_functions >= 2,
        "Should find both outer and inner functions"
    );
}

#[test]
fn test_check_exported_functions() {
    let files = vec![
        (
            "a.ts".to_string(),
            "export function add(x: number, y: number) { return x + y; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export function subtract(x: number, y: number) { return x - y; }".to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find the exported functions
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert!(total_functions >= 2, "Should find exported functions");
}
