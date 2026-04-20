#[test]
fn test_lib_binders_have_semantic_defs() {
    // Verify that lib binders actually populate semantic_defs during binding.
    // This is a prerequisite for pre_populate_def_ids_from_lib_binders to work.
    let lib_files = load_lib_files_for_test();
    if lib_files.is_empty() {
        return;
    }

    let mut total_semantic_defs = 0;
    for lib_file in &lib_files {
        let count = lib_file.binder.semantic_defs.len();
        total_semantic_defs += count;
    }

    // lib.es5.d.ts alone has hundreds of top-level declarations (Array, String,
    // Number, Boolean, Error, Promise, Map, etc.). If semantic_defs is empty,
    // it means the binder isn't recording them for lib files.
    assert!(
        total_semantic_defs > 50,
        "Lib binders should have significant semantic_defs, found {total_semantic_defs}"
    );
}

#[test]
fn test_lib_pre_population_creates_def_ids_for_lib_symbols() {
    // Verify that calling pre_populate_def_ids_from_lib_binders creates DefIds
    // in the DefinitionStore for lib symbols, eliminating O(N) scans on first access.
    if !lib_files_available() {
        return;
    }

    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), "const x: number = 1;".to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts: Vec<CheckerLibContext> = lib_files
        .iter()
        .map(|lib| {
            let binder_ctx = BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            };
            binder.merge_lib_contexts_into_binder(&[binder_ctx]);
            CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            }
        })
        .collect();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    // Pre-populate from primary binder
    let primary_count = checker.ctx.pre_populate_def_ids_from_binder();

    // Pre-populate from lib binders
    let lib_count = checker.ctx.pre_populate_def_ids_from_lib_binders();

    // The lib binders should contribute DefIds (Array, String, Number, etc.)
    assert!(
        lib_count > 0,
        "pre_populate_def_ids_from_lib_binders should create DefIds. \
         Primary: {primary_count}, Lib: {lib_count}"
    );
}

#[test]
fn test_lib_symbols_have_existing_def_ids_after_pre_population() {
    // After pre-population, get_existing_def_id should succeed for all lib
    // symbols that were merged into the main binder's file_locals. This proves
    // that lib-resolution closures can use get_existing_def_id instead of
    // get_or_create_def_id (no on-demand DefId creation needed).
    if !lib_files_available() {
        return;
    }

    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), "const x: number = 1;".to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts: Vec<CheckerLibContext> = lib_files
        .iter()
        .map(|lib| {
            let binder_ctx = BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            };
            binder.merge_lib_contexts_into_binder(&[binder_ctx]);
            CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            }
        })
        .collect();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    // Pre-populate (same as checker construction does)
    checker.ctx.pre_populate_def_ids_from_binder();
    checker.ctx.pre_populate_def_ids_from_lib_binders();

    // Key lib symbols that should have pre-existing DefIds
    let expected_symbols = [
        "Array", "String", "Number", "Boolean", "Object", "Function", "Error",
    ];
    let mut missing = Vec::new();
    for name in &expected_symbols {
        if let Some(sym_id) = binder.file_locals.get(name)
            && checker.ctx.get_existing_def_id(sym_id).is_none()
        {
            missing.push(*name);
        }
        // Symbol might not be in file_locals if lib files don't include it
    }
    assert!(
        missing.is_empty(),
        "These lib symbols should have pre-existing DefIds but don't: {missing:?}. \
         This means lib-resolution closures cannot safely use get_existing_def_id."
    );
}

// ---- Promise / generic lib reference tests ----

#[test]
fn test_promise_resolve_with_lib_no_false_errors() {
    if !lib_files_available() {
        return;
    }
    // Basic Promise usage should not produce errors when lib is loaded
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = Promise.resolve(42);
async function f(): Promise<string> { return "hello"; }
"#,
    );
    // Filter out TS2318 (missing global type) which is acceptable
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !has_error(
            &real_errors
                .iter()
                .map(|&&(c, ref m)| (c, m.clone()))
                .collect::<Vec<_>>(),
            2322
        ),
        "Promise<number> should not produce TS2322 with lib loaded.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_generic_array_with_lib_retains_type_params() {
    if !lib_files_available() {
        return;
    }
    // Array<T> should be generic and retain its type parameter
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const first: number = arr[0];
// This should error: string is not assignable to number[]
const bad: Array<number> = ["a", "b"];
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2322),
        "Expected TS2322 for string[] assigned to number[].\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_map_generic_lib_reference_with_stable_identity() {
    if !lib_files_available() {
        return;
    }
    // Map<K,V> should resolve correctly with lib loaded
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, number> = new Map();
m.set("key", 42);
// This should error: boolean is not assignable to number
m.set("key", true);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2345),
        "Expected TS2345 for boolean argument to Map.set(string, number).\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_map_constructor_rejects_heterogeneous_value_inference() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const map = new Map([["", true], ["", 0]]);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2769),
        "Map constructor should reject heterogeneous value inference instead of widening to a union.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_chaining_identity_stable() {
    if !lib_files_available() {
        return;
    }
    // Promise chaining should work with stable DefId identity
    let diagnostics = compile_with_lib(
        r#"
async function chain(): Promise<number> {
    const p = Promise.resolve(42);
    return p.then(x => x + 1);
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    // Should not have type errors in basic Promise chaining
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2345),
        "Promise chaining should not produce type errors.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Generic lib type parameter resolution ----

#[test]
fn test_readonly_array_heritage_resolves() {
    if !lib_files_available() {
        return;
    }
    // ReadonlyArray<T> is a base of Array<T> - heritage should resolve
    let diagnostics = compile_with_lib(
        r#"
const arr: ReadonlyArray<number> = [1, 2, 3];
const len: number = arr.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "ReadonlyArray.length should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_partial_type_alias_lib_resolution() {
    if !lib_files_available() {
        return;
    }
    // Partial<T> is a utility type alias in lib - should resolve correctly
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; }
const partial: Partial<User> = { name: "Alice" };
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Partial<User> should accept partial objects.\nDiagnostics: {real_errors:#?}"
    );
}

