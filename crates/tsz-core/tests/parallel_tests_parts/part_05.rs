#[test]
fn test_compile_large_program() {
    // Simulate a larger program with many files
    let files: Vec<_> = (0..50)
        .map(|i| {
            let source = format!("function fn{i}() {{ return {i}; }} const val{i} = fn{i}();");
            (format!("module{i}.ts"), source)
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
        let fn_name = format!("fn{i}");
        let val_name = format!("val{i}");
        assert!(program.globals.has(&fn_name), "Missing {fn_name}");
        assert!(program.globals.has(&val_name), "Missing {val_name}");
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

/// Test parallel type checking of Redux/Lodash-style generics
///
/// NOTE: Currently ignored - complex generic type inference with Redux/Lodash-style
/// patterns is not fully implemented. The checker emits various "Object is of type 'unknown'"
/// errors for cases that should work correctly.
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
        println!("\n=== DIAGNOSTICS ({}) ===", result.diagnostic_count);
        for file_result in &result.file_results {
            for diag in &file_result.diagnostics {
                println!(
                    "  [{}:{}] code={}: {}",
                    file_result.file_name, diag.start, diag.code, diag.message_text
                );
            }
        }
        println!("=== END DIAGNOSTICS ===\n");
    }

    assert_eq!(stats.file_count, 4);
    assert!(stats.function_count >= 5, "Expected at least 5 functions");

    // Debug: print diagnostics if there are any
    if result.diagnostic_count > 0 {
        println!("\n=== DIAGNOSTICS ({}) ===", result.diagnostic_count);
        for file_result in &result.file_results {
            for diag in &file_result.diagnostics {
                println!("  [{}:{}] {}", diag.file, diag.start, diag.message_text);
            }
        }
        println!("=== END DIAGNOSTICS ===\n");
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
                "function fn{i}(x: number): number {{ return x * {i}; }} const val{i} = fn{i}(10);"
            );
            (format!("module{i}.ts"), source)
        })
        .collect();

    let program = compile_files(files);
    let (_result, stats) = check_functions_with_stats(&program);

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

