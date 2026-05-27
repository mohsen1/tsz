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

// =============================================================================
// Phase 1 DefId-First Stable Identity Tests (Parallel Pipeline)
// =============================================================================

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

// =============================================================================
// Skeleton integration into MergedProgram
// =============================================================================

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

// =============================================================================
// Skeleton Fingerprinting Tests
// =============================================================================

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

// =============================================================================
// SkeletonIndex aggregate fingerprint tests
// =============================================================================

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

/// Regression: when the same namespace member is declared in two sibling lib
/// files (the parent namespace already merges across files), the global merge
/// must collapse the nested declarations into one symbol carrying both
/// declarations. Without this, property lookup on the namespace-scoped type
/// reports a different shape depending on which lib's declaration won the
/// initial allocation race — mirroring the real-world regression where
/// `Intl.ResolvedDateTimeFormatOptions` was split between
/// `lib.es5.d.ts` (carrying `calendar`, `numberingSystem`, ...) and
/// `lib.es2021.intl.d.ts` (carrying `dateStyle`, `formatMatcher`, ...) and
/// only one half survived in the merged shape.
#[test]
fn lib_merge_collapses_same_named_nested_interfaces_across_lib_files() {
    fn assert_merged(namespace: &str, type_name: &str, members_a: &str, members_b: &str) {
        let src_a =
            format!("declare namespace {namespace} {{ interface {type_name} {{ {members_a} }} }}");
        let src_b =
            format!("declare namespace {namespace} {{ interface {type_name} {{ {members_b} }} }}");

        let lib_files = vec![
            std::sync::Arc::new(crate::lib_loader::LibFile::from_source(
                "lib.a.d.ts".to_string(),
                src_a,
            )),
            std::sync::Arc::new(crate::lib_loader::LibFile::from_source(
                "lib.b.d.ts".to_string(),
                src_b,
            )),
        ];

        let program = merge_bind_results(parse_and_bind_parallel_with_libs(
            vec![("main.ts".to_string(), String::new())],
            &lib_files,
        ));

        let ns_id = program
            .globals
            .get(namespace)
            .unwrap_or_else(|| panic!("namespace {namespace} should be a global lib symbol"));
        let ns_sym = program
            .symbols
            .get(ns_id)
            .unwrap_or_else(|| panic!("namespace {namespace} symbol must exist"));

        let exports = ns_sym
            .exports
            .as_ref()
            .unwrap_or_else(|| panic!("namespace {namespace} must have exports"));
        let type_id = exports
            .get(type_name)
            .unwrap_or_else(|| panic!("{namespace}.{type_name} must be exported"));

        // The merged interface symbol must carry distinct declarations from
        // BOTH lib files. `symbol.declarations` deduplicates by raw NodeIndex
        // which can coincide across arenas, so verify via the (symbol, decl)
        // → arenas map: two declarations from sibling lib files must show up
        // as either two NodeIndex entries or one NodeIndex entry with two
        // arenas.
        let type_sym = program
            .symbols
            .get(type_id)
            .unwrap_or_else(|| panic!("{namespace}.{type_name} symbol must exist"));
        let total_arena_decls: usize = type_sym
            .declarations
            .iter()
            .map(|&decl_idx| {
                program
                    .declaration_arenas
                    .get(&(type_id, decl_idx))
                    .map_or(0, |v| v.len())
            })
            .sum();
        assert_eq!(
            total_arena_decls,
            2,
            "{namespace}.{type_name} should hold both lib declarations across arenas, \
             got declarations={:?} and arena counts={:?}",
            type_sym.declarations,
            type_sym
                .declarations
                .iter()
                .map(|&d| program
                    .declaration_arenas
                    .get(&(type_id, d))
                    .map_or(0, |v| v.len()))
                .collect::<Vec<_>>(),
        );
    }

    // Original repro: `Intl.ResolvedDateTimeFormatOptions` split across two libs.
    assert_merged(
        "Intl",
        "ResolvedDateTimeFormatOptions",
        "calendar: string; numberingSystem: string;",
        "dateStyle?: string; formatMatcher?: string;",
    );

    // Vary the namespace and type names to prove the rule isn't keyed on
    // any particular spelling.
    assert_merged(
        "MyOwnNS",
        "Config",
        "host: string; port: number;",
        "useTls: boolean; retries: number;",
    );

    // Renamed sibling namespace under a different alias.
    assert_merged(
        "Reflect2",
        "Capability",
        "read: boolean;",
        "write: boolean;",
    );
}

/// Negative case for the nested-merge rule: two `Foo` interfaces nested
/// inside *different* namespaces must remain distinct, even after the merge.
/// Without keying on the parent's global id, both `Foo`s would collapse and
/// `NsA.Foo` would gain members from `NsB.Foo`.
#[test]
fn lib_merge_does_not_collapse_nested_interfaces_under_different_namespaces() {
    let lib_files = vec![
        std::sync::Arc::new(crate::lib_loader::LibFile::from_source(
            "lib.a.d.ts".to_string(),
            "declare namespace NsA { interface Foo { onlyA: string; } }".to_string(),
        )),
        std::sync::Arc::new(crate::lib_loader::LibFile::from_source(
            "lib.b.d.ts".to_string(),
            "declare namespace NsB { interface Foo { onlyB: number; } }".to_string(),
        )),
    ];

    let program = merge_bind_results(parse_and_bind_parallel_with_libs(
        vec![("main.ts".to_string(), String::new())],
        &lib_files,
    ));

    let lookup = |ns: &str, type_name: &str| -> SymbolId {
        let ns_id = program.globals.get(ns).expect("namespace");
        let ns_sym = program.symbols.get(ns_id).expect("namespace symbol");
        ns_sym
            .exports
            .as_ref()
            .expect("exports")
            .get(type_name)
            .expect("type export")
    };

    let a_id = lookup("NsA", "Foo");
    let b_id = lookup("NsB", "Foo");
    assert_ne!(
        a_id, b_id,
        "NsA.Foo and NsB.Foo must remain distinct symbols; \
         the nested-merge key (global parent id, name) must scope them by parent",
    );

    let a_sym = program.symbols.get(a_id).expect("NsA.Foo");
    let b_sym = program.symbols.get(b_id).expect("NsB.Foo");
    let a_decl_count: usize = a_sym
        .declarations
        .iter()
        .map(|&d| {
            program
                .declaration_arenas
                .get(&(a_id, d))
                .map_or(0, |v| v.len())
        })
        .sum();
    let b_decl_count: usize = b_sym
        .declarations
        .iter()
        .map(|&d| {
            program
                .declaration_arenas
                .get(&(b_id, d))
                .map_or(0, |v| v.len())
        })
        .sum();
    assert_eq!(a_decl_count, 1, "NsA.Foo should hold one declaration");
    assert_eq!(b_decl_count, 1, "NsB.Foo should hold one declaration");
}

#[test]
fn test_skeleton_index_estimated_size_bytes_is_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
        (
            "c.ts".to_string(),
            "export * from './a'; export { b } from './b';".to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    let stats = program.residency_stats();
    assert!(stats.has_skeleton_index);
    assert!(
        stats.skeleton_estimated_size_bytes > 0,
        "skeleton index should report nonzero estimated size, got 0"
    );
    // The estimate should at least cover the base struct size
    assert!(
        stats.skeleton_estimated_size_bytes >= std::mem::size_of::<SkeletonIndex>(),
        "skeleton size estimate ({}) should be >= struct size ({})",
        stats.skeleton_estimated_size_bytes,
        std::mem::size_of::<SkeletonIndex>()
    );
}

#[test]
fn test_skeleton_index_estimated_size_grows_with_content() {
    // Small project
    let small_files = vec![("a.ts".to_string(), "export const a = 1;".to_string())];
    let small_results = parse_and_bind_parallel(small_files);
    let small_program = merge_bind_results(small_results);
    let small_size = small_program
        .skeleton_index
        .as_ref()
        .unwrap()
        .estimated_size_bytes();

    // Larger project with more symbols and cross-file relationships
    let large_files = vec![
        (
            "a.ts".to_string(),
            "export const a1 = 1; export const a2 = 2; export const a3 = 3;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export const b1 = 1; export const b2 = 2; export const b3 = 3;".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export * from './a'; export * from './b';".to_string(),
        ),
        (
            "d.ts".to_string(),
            "export { a1, a2 } from './a'; export { b1 } from './b';".to_string(),
        ),
    ];
    let large_results = parse_and_bind_parallel(large_files);
    let large_program = merge_bind_results(large_results);
    let large_size = large_program
        .skeleton_index
        .as_ref()
        .unwrap()
        .estimated_size_bytes();

    assert!(
        large_size > small_size,
        "larger project skeleton ({large_size} bytes) should be bigger than small ({small_size} bytes)"
    );
}

#[test]
fn test_bind_result_estimated_size_bytes_is_nonzero() {
    let result = parse_and_bind_single("a.ts".to_string(), "export const a = 1;".to_string());
    let size = result.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero for any bind result"
    );
    // Must be at least the struct size itself
    assert!(
        size >= std::mem::size_of::<BindResult>(),
        "estimated size ({}) should be >= struct size ({})",
        size,
        std::mem::size_of::<BindResult>()
    );
}

#[test]
fn test_bind_result_estimated_size_grows_with_content() {
    let small = parse_and_bind_single("s.ts".to_string(), "const x = 1;".to_string());
    let small_size = small.estimated_size_bytes();

    let large_source = (0..50)
        .map(|i| format!("export function fn{i}(a: number, b: string): boolean {{ return true; }}"))
        .collect::<Vec<_>>()
        .join("\n");
    let large = parse_and_bind_single("l.ts".to_string(), large_source);
    let large_size = large.estimated_size_bytes();

    assert!(
        large_size > small_size,
        "larger file ({large_size} bytes) should have bigger estimate than small file ({small_size} bytes)"
    );
}

#[test]
fn test_bind_result_estimated_size_accounts_for_flow_nodes() {
    // Code with control flow creates flow nodes
    let source = r#"
        function f(x: number) {
            if (x > 0) {
                return x;
            } else if (x < 0) {
                return -x;
            } else {
                return 0;
            }
        }
    "#;
    let result = parse_and_bind_single("flow.ts".to_string(), source.to_string());
    let size = result.estimated_size_bytes();

    // Simple file without control flow
    let simple = parse_and_bind_single("simple.ts".to_string(), "const x = 1;".to_string());
    let simple_size = simple.estimated_size_bytes();

    assert!(
        size > simple_size,
        "file with control flow ({size} bytes) should be larger than simple file ({simple_size} bytes)"
    );
}

