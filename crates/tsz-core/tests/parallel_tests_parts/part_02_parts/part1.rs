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

    assert_eq!(total_functions, 2);
}

#[test]
fn test_parallel_type_interner_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    // Test that the new lock-free TypeInterner supports concurrent access
    let interner = Arc::new(TypeInterner::new());

    let mut handles = vec![];

    // Spawn multiple threads that all intern types concurrently
    for i in 0..10 {
        let interner_clone = Arc::clone(&interner);
        let handle = thread::spawn(move || {
            // Each thread interns various types
            for j in 0..100 {
                let _ = interner_clone.literal_number(j as f64);
                let _ = interner_clone.literal_string(&format!("str_{i}_{j}"));
                let _ = interner_clone.union(vec![
                    interner_clone.literal_number((j % 10) as f64),
                    interner_clone.literal_number(((j + 1) % 10) as f64),
                ]);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify the interner has the expected number of types
    // (exact count depends on deduplication, but should be reasonable)
    let len = interner.len();
    assert!(len > 100, "Expected at least 100 types, got {len}");
    assert!(len < 2000, "Expected fewer than 2000 types, got {len}");
}

#[test]
fn test_parallel_type_checking_with_shared_interner() {
    // Test that multiple files can be type-checked in parallel
    // while sharing a single TypeInterner for type deduplication
    let files = vec![
        (
            "math.ts".to_string(),
            r#"
                function add(a: number, b: number): number { return a + b; }
                function subtract(a: number, b: number): number { return a - b; }
                function multiply(a: number, b: number): number { return a * b; }
            "#
            .to_string(),
        ),
        (
            "strings.ts".to_string(),
            r#"
                function concat(a: string, b: string): string { return a + b; }
                function upper(s: string): string { return s.toUpperCase(); }
                function lower(s: string): string { return s.toLowerCase(); }
            "#
            .to_string(),
        ),
        (
            "arrays.ts".to_string(),
            r#"
                function first<T>(arr: T[]): T | undefined { return arr[0]; }
                function last<T>(arr: T[]): T | undefined { return arr[arr.length - 1]; }
                function isEmpty<T>(arr: T[]): boolean { return arr.length === 0; }
            "#
            .to_string(),
        ),
        (
            "objects.ts".to_string(),
            r#"
                function keys(obj: object): string[] { return Object.keys(obj); }
                function values(obj: object): unknown[] { return Object.values(obj); }
                function entries(obj: object): [string, unknown][] { return Object.entries(obj); }
            "#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    assert_eq!(program.files.len(), 4);

    // Check all files in parallel
    let (_result, stats) = check_functions_with_stats(&program);

    assert_eq!(stats.file_count, 4);
    // Each file has 3 functions
    assert!(
        stats.function_count >= 12,
        "Expected at least 12 functions, got {}",
        stats.function_count
    );

    // The shared TypeInterner should have deduplicated common types
    // (number, string, boolean, etc. are shared across all files)
    let interner_len = program.type_interner.len();
    assert!(
        interner_len > TypeId::FIRST_USER as usize,
        "TypeInterner should have user-defined types"
    );
}

#[test]
fn test_parallel_binding_produces_consistent_symbols() {
    // Test that parallel binding produces consistent results
    // by binding the same files multiple times
    let files = vec![
        (
            "a.ts".to_string(),
            "export const x: number = 1;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export const y: string = 'hello';".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export function add(a: number, b: number) { return a + b; }".to_string(),
        ),
    ];

    // Bind multiple times
    let results1 = parse_and_bind_parallel(files.clone());
    let results2 = parse_and_bind_parallel(files);

    // Results should be structurally identical
    assert_eq!(results1.len(), results2.len());

    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.file_name, r2.file_name);
        assert_eq!(r1.arena.len(), r2.arena.len());
        assert_eq!(r1.symbols.len(), r2.symbols.len());

        // Same symbols should be present
        for (name, _) in r1.file_locals.iter() {
            assert!(
                r2.file_locals.has(name),
                "Symbol {name} should be present in both results"
            );
        }
    }
}

#[test]
fn semantic_defs_survive_single_file_bind() {
    let result = parse_and_bind_single(
        "test.ts".to_string(),
        "class A {} interface B {} type C = number; enum D { X } namespace E {}".to_string(),
    );
    assert_eq!(
        result.semantic_defs.len(),
        5,
        "expected 5 semantic defs, got {}",
        result.semantic_defs.len()
    );
}

#[test]
fn semantic_defs_survive_merge_with_remapped_symbol_ids() {
    let files = vec![
        ("a.ts".to_string(), "export class Foo {}".to_string()),
        (
            "b.ts".to_string(),
            "export interface Bar { x: number }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Both Foo and Bar should be in the merged semantic_defs
    let names: std::collections::HashSet<_> = program
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        names.contains("Foo"),
        "Foo should be in merged semantic_defs"
    );
    assert!(
        names.contains("Bar"),
        "Bar should be in merged semantic_defs"
    );
}

#[test]
fn semantic_defs_file_id_is_correct_after_merge() {
    let files = vec![
        ("file0.ts".to_string(), "export class Alpha {}".to_string()),
        (
            "file1.ts".to_string(),
            "export type Beta = string".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    for entry in program.semantic_defs.values() {
        match entry.name.as_str() {
            "Alpha" => assert_eq!(entry.file_id, 0, "Alpha should be in file 0"),
            "Beta" => assert_eq!(entry.file_id, 1, "Beta should be in file 1"),
            _ => {}
        }
    }
}

#[test]
fn semantic_defs_stable_across_repeated_merge() {
    let files = vec![(
        "a.ts".to_string(),
        "export class C {} export interface I {} export type T = number; export enum E { X }"
            .to_string(),
    )];

    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);
    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    assert_eq!(program1.semantic_defs.len(), program2.semantic_defs.len());

    // Same names and kinds should appear
    let defs1: std::collections::HashMap<_, _> = program1
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), e.kind))
        .collect();
    let defs2: std::collections::HashMap<_, _> = program2
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), e.kind))
        .collect();
    assert_eq!(
        defs1, defs2,
        "semantic defs should be identical across rebuilds"
    );
}

#[test]
fn skeleton_index_populated_after_merge() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    assert!(
        program.skeleton_index.is_some(),
        "skeleton_index should be populated after merge"
    );
    let idx = program.skeleton_index.as_ref().unwrap();
    assert_eq!(idx.file_count, 2);
}

#[test]
fn skeleton_index_single_file() {
    let files = vec![("test.ts".to_string(), "let x = 42;".to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    assert_eq!(idx.file_count, 1);
    assert!(
        idx.merge_candidates.is_empty(),
        "single file should have no merge candidates"
    );
    assert!(
        idx.total_symbol_count > 0,
        "should have at least one symbol"
    );
}

#[test]
fn skeleton_index_captures_declared_modules() {
    let files = vec![(
        "ambient.d.ts".to_string(),
        r#"declare module "my-module" { export function hello(): void; }"#.to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    assert!(
        idx.declared_modules.contains("my-module"),
        "skeleton index should capture declared module names"
    );
}

#[test]
fn skeleton_index_captures_merge_candidates() {
    // Two script files (not modules) with the same interface name should produce
    // a merge candidate.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Shared { x: number; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Shared { y: string; }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    let shared = idx.merge_candidates.iter().find(|c| c.name == "Shared");
    assert!(
        shared.is_some(),
        "interface 'Shared' should appear as a merge candidate"
    );
    let shared = shared.unwrap();
    assert_eq!(shared.source_files.len(), 2);
    assert!(
        shared.is_valid_merge,
        "interface + interface should be a valid merge"
    );
}

#[test]
fn skeleton_index_stable_across_rebuilds() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);
    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    let idx1 = program1.skeleton_index.as_ref().unwrap();
    let idx2 = program2.skeleton_index.as_ref().unwrap();

    assert_eq!(idx1.file_count, idx2.file_count);
    assert_eq!(idx1.total_symbol_count, idx2.total_symbol_count);
    assert_eq!(idx1.merge_candidates.len(), idx2.merge_candidates.len());
    assert_eq!(idx1.total_reexport_count, idx2.total_reexport_count);
}

#[test]
fn skeleton_index_reexport_counts() {
    let files = vec![
        ("a.ts".to_string(), "export const foo = 1;".to_string()),
        ("b.ts".to_string(), "export { foo } from './a';".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    // b.ts has a named re-export
    assert!(
        idx.total_reexport_count > 0 || idx.total_wildcard_reexport_count > 0,
        "should track re-export edges in skeleton index"
    );
}

#[test]
fn skeleton_index_external_modules_excluded_from_global_merge() {
    // External modules (files with import/export) should not contribute to
    // global merge candidates. Only script files do.
    let files = vec![
        (
            "mod_a.ts".to_string(),
            "export interface Dup { x: number; }".to_string(),
        ),
        (
            "mod_b.ts".to_string(),
            "export interface Dup { y: string; }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    let dup = idx.merge_candidates.iter().find(|c| c.name == "Dup");
    assert!(
        dup.is_none(),
        "external module symbols should not appear as merge candidates"
    );
}

#[test]
fn skeleton_index_captures_module_export_specifiers() {
    // declare module "x" { ... } populates module_exports in the binder.
    // The skeleton should capture those keys in module_export_specifiers.
    let files = vec![(
        "ambient.d.ts".to_string(),
        r#"declare module "my-lib" { export function greet(): string; }"#.to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    assert!(
        idx.module_export_specifiers.contains("my-lib")
            || idx.module_export_specifiers.contains("\"my-lib\""),
        "skeleton index should capture module export specifiers, got: {:?}",
        idx.module_export_specifiers
    );
}

#[test]
fn skeleton_build_declared_modules_matches_binder() {
    // Verify that SkeletonIndex::build_declared_module_sets produces the same
    // result as the binder-scanning loop in set_all_binders for declared modules.
    let files = vec![
        (
            "ambient.d.ts".to_string(),
            r#"declare module "fs" { export function readFile(): void; }"#.to_string(),
        ),
        (
            "wildcard.d.ts".to_string(),
            r#"declare module "*.css" { const content: string; export default content; }"#
                .to_string(),
        ),
        (
            "shorthand.d.ts".to_string(),
            r#"declare module "my-shorthand";"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    let (exact, patterns) = idx.build_declared_module_sets();

    // "fs" should be in exact (from module_exports key or declared_modules)
    assert!(
        exact.contains("fs"),
        "exact set should contain 'fs', got: {exact:?}"
    );

    // "my-shorthand" from shorthand ambient module
    assert!(
        exact.contains("my-shorthand"),
        "exact set should contain 'my-shorthand', got: {exact:?}"
    );

    // "*.css" should be in patterns
    assert!(
        patterns.contains(&"*.css".to_string()),
        "patterns should contain '*.css', got: {patterns:?}"
    );
}

#[test]
fn skeleton_build_declared_modules_deduplicates_patterns() {
    // Two files both declaring the same wildcard module should produce
    // only one entry in patterns.
    let files = vec![
        (
            "a.d.ts".to_string(),
            r#"declare module "*.svg" { const url: string; export default url; }"#.to_string(),
        ),
        (
            "b.d.ts".to_string(),
            r#"declare module "*.svg" { }"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    let (_exact, patterns) = idx.build_declared_module_sets();

    let svg_count = patterns.iter().filter(|p| *p == "*.svg").count();
    assert_eq!(
        svg_count, 1,
        "duplicate wildcard patterns should be deduplicated, got {svg_count} occurrences"
    );
}

#[test]
fn skeleton_validate_against_merged_declared_modules() {
    // Ambient module declarations should match between skeleton and legacy merge.
    let files = vec![
        (
            "ambient.d.ts".to_string(),
            r#"declare module "my-lib" { export function greet(): string; }"#.to_string(),
        ),
        (
            "ambient2.d.ts".to_string(),
            r#"declare module "my-other-lib" { export const version: number; }"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // If we got here without panic, the debug validation in merge_bind_results passed.
    let idx = program.skeleton_index.as_ref().unwrap();
    assert!(
        idx.declared_modules.contains("\"my-lib\"") || idx.declared_modules.contains("my-lib"),
        "skeleton should contain declared module 'my-lib', got: {:?}",
        idx.declared_modules
    );
    assert_eq!(
        idx.declared_modules, *program.declared_modules,
        "skeleton and legacy declared_modules must match"
    );
}

#[test]
fn skeleton_validate_against_merged_shorthand_ambient() {
    // Shorthand ambient modules (declare module "x"; without body)
    // should match between skeleton and legacy merge.
    let files = vec![
        (
            "shorthands.d.ts".to_string(),
            r#"
            declare module "shorthand-a";
            declare module "shorthand-b";
            "#
            .to_string(),
        ),
        (
            "more.d.ts".to_string(),
            r#"declare module "shorthand-c";"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    assert_eq!(
        idx.shorthand_ambient_modules, *program.shorthand_ambient_modules,
        "skeleton and legacy shorthand_ambient_modules must match"
    );
    // Verify actual content
    assert!(
        program.shorthand_ambient_modules.len() >= 3,
        "should have at least 3 shorthand ambient modules, got {}",
        program.shorthand_ambient_modules.len()
    );
}

#[test]
fn skeleton_validate_against_merged_module_export_specifiers() {
    // Module export specifiers (keys of module_exports from ambient declare module blocks)
    // should match between skeleton and legacy merge (after filtering user file names).
    let files = vec![
        (
            "types.d.ts".to_string(),
            r#"
            declare module "pkg-a" {
                export function foo(): void;
            }
            declare module "pkg-b" {
                export const bar: number;
            }
            "#
            .to_string(),
        ),
        ("user.ts".to_string(), "export const x = 1;".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // The merge validation already ran. Now verify the module_export_specifiers
    // in the skeleton contain the ambient module keys.
    let idx = program.skeleton_index.as_ref().unwrap();
    let has_pkg_a = idx
        .module_export_specifiers
        .iter()
        .any(|s| s.contains("pkg-a"));
    let has_pkg_b = idx
        .module_export_specifiers
        .iter()
        .any(|s| s.contains("pkg-b"));
    assert!(
        has_pkg_a,
        "skeleton should track module_export_specifier for pkg-a"
    );
    assert!(
        has_pkg_b,
        "skeleton should track module_export_specifier for pkg-b"
    );

    // Both legacy module_exports and skeleton module_export_specifiers
    // include user file names (from the binder's own module_exports for
    // external modules). The validation filters these out when comparing
    // ambient-module topology.
    assert!(
        program.module_exports.contains_key("user.ts"),
        "legacy module_exports should include user file name"
    );
}

#[test]
fn skeleton_validate_mixed_ambient_and_user_files() {
    // A realistic mix: ambient modules, shorthand modules, user files with exports,
    // and cross-file re-exports. The debug assertion in merge_bind_results
    // validates all three skeleton sets match the legacy merge.
    let files = vec![
        (
            "globals.d.ts".to_string(),
            r#"
            declare module "my-globals" {
                export interface Config { debug: boolean; }
            }
            declare module "*.css";
            "#
            .to_string(),
        ),
        (
            "lib.ts".to_string(),
            r#"
            export function helper() { return 42; }
            export const VERSION = "1.0";
            "#
            .to_string(),
        ),
        (
            "reexporter.ts".to_string(),
            r#"export { helper } from "./lib";"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // If merge_bind_results didn't panic, all skeleton validations passed.
    let idx = program.skeleton_index.as_ref().unwrap();

    // Verify skeleton metadata is coherent.
    assert_eq!(idx.file_count, 3);
    assert!(
        idx.total_reexport_count >= 1,
        "should have at least one re-export edge"
    );

    // Shorthand ambient for *.css
    let (exact, patterns) = idx.build_declared_module_sets();
    assert!(
        patterns.iter().any(|p| p == "*.css"),
        "should have wildcard pattern for *.css"
    );
    assert!(
        exact.iter().any(|e| e == "my-globals"),
        "should have exact declared module 'my-globals'"
    );
}

#[test]
fn skeleton_fingerprint_deterministic_across_rebuilds() {
    let source = "let x = 1; export function foo(): number { return 42; }";
    let files1 = vec![("a.ts".to_string(), source.to_string())];
    let files2 = vec![("a.ts".to_string(), source.to_string())];

    let results1 = parse_and_bind_parallel(files1);
    let results2 = parse_and_bind_parallel(files2);

    let skel1 = extract_skeleton(&results1[0]);
    let skel2 = extract_skeleton(&results2[0]);

    assert_eq!(
        skel1.fingerprint, skel2.fingerprint,
        "identical source should produce identical skeleton fingerprints"
    );
    assert_ne!(
        skel1.fingerprint, 0,
        "fingerprint should not be zero for non-trivial files"
    );
}

#[test]
fn skeleton_fingerprint_changes_on_symbol_add() {
    let files_v1 = vec![("a.ts".to_string(), "let x = 1;".to_string())];
    let files_v2 = vec![("a.ts".to_string(), "let x = 1; let y = 2;".to_string())];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skel_v1 = extract_skeleton(&results_v1[0]);
    let skel_v2 = extract_skeleton(&results_v2[0]);

    assert_ne!(
        skel_v1.fingerprint, skel_v2.fingerprint,
        "adding a symbol should change the skeleton fingerprint"
    );
}

#[test]
fn skeleton_fingerprint_stable_when_body_changes() {
    // Changing a function body should NOT change the skeleton fingerprint,
    // since the skeleton only captures top-level symbol topology.
    let files_v1 = vec![(
        "a.ts".to_string(),
        "function foo(): number { return 1; }".to_string(),
    )];
    let files_v2 = vec![(
        "a.ts".to_string(),
        "function foo(): number { return 42; }".to_string(),
    )];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skel_v1 = extract_skeleton(&results_v1[0]);
    let skel_v2 = extract_skeleton(&results_v2[0]);

    assert_eq!(
        skel_v1.fingerprint, skel_v2.fingerprint,
        "changing a function body should not change the skeleton fingerprint"
    );
}

#[test]
fn skeleton_fingerprint_changes_on_export_toggle() {
    // Adding `export` to a declaration changes the skeleton
    // (is_exported flag flips).
    let files_v1 = vec![("a.ts".to_string(), "let x = 1;".to_string())];
    let files_v2 = vec![("a.ts".to_string(), "export let x = 1;".to_string())];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skel_v1 = extract_skeleton(&results_v1[0]);
    let skel_v2 = extract_skeleton(&results_v2[0]);

    assert_ne!(
        skel_v1.fingerprint, skel_v2.fingerprint,
        "toggling export should change the skeleton fingerprint"
    );
}

#[test]
fn skeleton_fingerprint_independent_of_file_name() {
    // Script files (no import/export) with the same source under different
    // file names should produce identical fingerprints.
    // Note: external modules (with export/import) include the file name in
    // `module_export_specifiers`, so their fingerprints legitimately differ.
    let source = "let x = 1;";
    let files_a = vec![("a.ts".to_string(), source.to_string())];
    let files_b = vec![("b.ts".to_string(), source.to_string())];

    let results_a = parse_and_bind_parallel(files_a);
    let results_b = parse_and_bind_parallel(files_b);

    let skel_a = extract_skeleton(&results_a[0]);
    let skel_b = extract_skeleton(&results_b[0]);

    assert_eq!(
        skel_a.fingerprint, skel_b.fingerprint,
        "fingerprint should be independent of file name for script files"
    );
    assert_ne!(skel_a.file_name, skel_b.file_name);
}

#[test]
fn skeleton_fingerprint_changes_on_declared_module() {
    let files_v1 = vec![("a.d.ts".to_string(), "declare const x: number;".to_string())];
    let files_v2 = vec![(
        "a.d.ts".to_string(),
        r#"declare const x: number; declare module "foo" { export const y: string; }"#.to_string(),
    )];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skel_v1 = extract_skeleton(&results_v1[0]);
    let skel_v2 = extract_skeleton(&results_v2[0]);

    assert_ne!(
        skel_v1.fingerprint, skel_v2.fingerprint,
        "adding a declared module should change the fingerprint"
    );
}

#[test]
fn skeleton_compute_fingerprint_matches_stored() {
    // Verify that recomputing the fingerprint yields the same value
    // as the one stored at extraction time.
    let files = vec![(
        "a.ts".to_string(),
        "export interface Foo { x: number; }".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let skel = extract_skeleton(&results[0]);

    assert_eq!(
        skel.fingerprint,
        skel.compute_fingerprint(),
        "stored fingerprint must match recomputed fingerprint"
    );
}

#[test]
fn skeleton_index_fingerprint_deterministic() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];
    let results1 = parse_and_bind_parallel(files.clone());
    let results2 = parse_and_bind_parallel(files);

    let skels1: Vec<_> = results1.iter().map(extract_skeleton).collect();
    let skels2: Vec<_> = results2.iter().map(extract_skeleton).collect();

    let idx1 = reduce_skeletons(&skels1);
    let idx2 = reduce_skeletons(&skels2);

    assert_eq!(
        idx1.fingerprint, idx2.fingerprint,
        "identical projects should produce identical aggregate fingerprints"
    );
    assert_ne!(
        idx1.fingerprint, 0,
        "aggregate fingerprint should not be zero"
    );
}

#[test]
fn skeleton_index_fingerprint_changes_on_file_add() {
    let files_v1 = vec![("a.ts".to_string(), "let x = 1;".to_string())];
    let files_v2 = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skels_v1: Vec<_> = results_v1.iter().map(extract_skeleton).collect();
    let skels_v2: Vec<_> = results_v2.iter().map(extract_skeleton).collect();

    let idx_v1 = reduce_skeletons(&skels_v1);
    let idx_v2 = reduce_skeletons(&skels_v2);

    assert_ne!(
        idx_v1.fingerprint, idx_v2.fingerprint,
        "adding a file should change the aggregate fingerprint"
    );
}

#[test]
fn skeleton_index_fingerprint_changes_on_symbol_change() {
    let files_v1 = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];
    let files_v2 = vec![
        ("a.ts".to_string(), "let x = 1; let z = 3;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skels_v1: Vec<_> = results_v1.iter().map(extract_skeleton).collect();
    let skels_v2: Vec<_> = results_v2.iter().map(extract_skeleton).collect();

    let idx_v1 = reduce_skeletons(&skels_v1);
    let idx_v2 = reduce_skeletons(&skels_v2);

    assert_ne!(
        idx_v1.fingerprint, idx_v2.fingerprint,
        "adding a symbol to one file should change the aggregate fingerprint"
    );
}

#[test]
fn skeleton_index_fingerprint_stable_on_body_change() {
    // Changing function bodies should not affect the aggregate fingerprint
    // since skeletons only capture top-level symbol topology.
    let files_v1 = vec![(
        "a.ts".to_string(),
        "function foo() { return 1; }".to_string(),
    )];
    let files_v2 = vec![(
        "a.ts".to_string(),
        "function foo() { return 999; }".to_string(),
    )];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skels_v1: Vec<_> = results_v1.iter().map(extract_skeleton).collect();
    let skels_v2: Vec<_> = results_v2.iter().map(extract_skeleton).collect();

    let idx_v1 = reduce_skeletons(&skels_v1);
    let idx_v2 = reduce_skeletons(&skels_v2);

    assert_eq!(
        idx_v1.fingerprint, idx_v2.fingerprint,
        "changing function bodies should not change the aggregate fingerprint"
    );
}

#[test]
fn skeleton_index_fingerprint_changes_on_merge_topology() {
    // Two script files declaring the same global name creates a merge candidate.
    // Changing one file to not declare that name should change the fingerprint.
    let files_v1 = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let x = 2;".to_string()),
    ];
    let files_v2 = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skels_v1: Vec<_> = results_v1.iter().map(extract_skeleton).collect();
    let skels_v2: Vec<_> = results_v2.iter().map(extract_skeleton).collect();

    let idx_v1 = reduce_skeletons(&skels_v1);
    let idx_v2 = reduce_skeletons(&skels_v2);

    // v1 has a merge candidate for `x`, v2 does not.
    assert!(
        idx_v1.merge_candidates.iter().any(|mc| mc.name == "x"),
        "v1 should have merge candidate for x"
    );
    assert!(
        !idx_v2.merge_candidates.iter().any(|mc| mc.name == "x"),
        "v2 should not have merge candidate for x"
    );
    assert_ne!(
        idx_v1.fingerprint, idx_v2.fingerprint,
        "different merge topology should produce different aggregate fingerprints"
    );
}

#[test]
fn test_merge_deterministic_symbol_order() {
    // Merging the same set of files multiple times must produce identical
    // global symbol arenas and declaration orderings.  This exercises the
    // sorted id_remap iteration introduced for deterministic merge output.
    let files = vec![
        (
            "a.ts".to_string(),
            "export interface Shared { a: number; }\nexport function helper(): void {}".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export interface Shared { b: string; }\nexport const VAL = 42;".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export namespace NS { export function inner(): void {} }\nexport type Alias = string;"
                .to_string(),
        ),
    ];

    // Run the full bind + merge pipeline several times.
    let mut prev_symbol_names: Option<Vec<String>> = None;
    let mut prev_globals_names: Option<Vec<String>> = None;
    let mut prev_decl_counts: Option<Vec<usize>> = None;

    for _run in 0..5 {
        let bind_results = parse_and_bind_parallel(files.clone());
        let merged = merge_bind_results(bind_results);

        // Collect ordered lists of global symbol names and declaration counts.
        let mut symbol_names: Vec<String> = Vec::new();
        let mut decl_counts: Vec<usize> = Vec::new();
        for i in 0..merged.symbols.len() {
            let id = SymbolId(i as u32);
            if let Some(sym) = merged.symbols.get(id) {
                symbol_names.push(sym.escaped_name.clone());
                decl_counts.push(sym.declarations.len());
            }
        }

        let mut globals_names: Vec<String> =
            merged.globals.iter().map(|(n, _)| n.clone()).collect();
        globals_names.sort();

        if let Some(ref prev) = prev_symbol_names {
            assert_eq!(
                symbol_names, *prev,
                "global symbol arena ordering must be deterministic across runs"
            );
        }
        if let Some(ref prev) = prev_globals_names {
            assert_eq!(
                globals_names, *prev,
                "globals table content must be deterministic across runs"
            );
        }
        if let Some(ref prev) = prev_decl_counts {
            assert_eq!(
                decl_counts, *prev,
                "declaration counts per symbol must be deterministic across runs"
            );
        }

        prev_symbol_names = Some(symbol_names);
        prev_globals_names = Some(globals_names);
        prev_decl_counts = Some(decl_counts);
    }
}

#[test]
fn test_merge_deterministic_global_namespace() {
    // Cross-file global namespace merging must produce deterministic export
    // tables regardless of FxHashMap iteration order.  We use `declare
    // namespace` (not `export namespace`) so symbols land in globals, not
    // per-file module_exports.
    let files = vec![
        (
            "x.d.ts".to_string(),
            "declare namespace Deep { function fa(): void; }".to_string(),
        ),
        (
            "y.d.ts".to_string(),
            "declare namespace Deep { function fb(): void; }".to_string(),
        ),
    ];

    let mut prev_deep_exports: Option<Vec<String>> = None;
    let mut prev_symbol_names: Option<Vec<String>> = None;

    for _run in 0..5 {
        let bind_results = parse_and_bind_parallel(files.clone());
        let merged = merge_bind_results(bind_results);

        // Collect ordered list of global symbol names.
        let mut symbol_names: Vec<String> = Vec::new();
        for i in 0..merged.symbols.len() {
            let id = SymbolId(i as u32);
            if let Some(sym) = merged.symbols.get(id) {
                symbol_names.push(sym.escaped_name.clone());
            }
        }

        // Find the "Deep" symbol in globals.
        let deep_id = merged
            .globals
            .get("Deep")
            .expect("Deep namespace must be in globals");

        let deep_sym = merged.symbols.get(deep_id).expect("Deep symbol must exist");

        let deep_exports: Vec<String> = deep_sym
            .exports
            .as_ref()
            .map(|e| {
                let mut names: Vec<String> = e.iter().map(|(n, _)| n.clone()).collect();
                names.sort();
                names
            })
            .unwrap_or_default();

        // Deep should have both fa and fb from cross-file merge.
        assert!(
            deep_exports.contains(&"fa".to_string()),
            "Deep exports: {deep_exports:?} — must contain fa"
        );
        assert!(
            deep_exports.contains(&"fb".to_string()),
            "Deep exports: {deep_exports:?} — must contain fb"
        );

        if let Some(ref prev) = prev_symbol_names {
            assert_eq!(
                symbol_names, *prev,
                "global symbol arena ordering must be deterministic"
            );
        }
        if let Some(ref prev) = prev_deep_exports {
            assert_eq!(
                deep_exports, *prev,
                "Deep namespace exports must be deterministic"
            );
        }
        prev_symbol_names = Some(symbol_names);
        prev_deep_exports = Some(deep_exports);
    }
}
