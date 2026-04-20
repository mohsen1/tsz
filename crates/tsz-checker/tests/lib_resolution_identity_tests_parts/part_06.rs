#[test]
fn test_promise_via_augmentation_stable_def_id() {
    // Promise references within augmentation contexts should use get_lib_def_id
    // (stable identity) rather than get_or_create_def_id (on-demand creation).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib_and_options(
        r#"
async function fetchData(): Promise<string> {
    return "data";
}
const result: Promise<string> = fetchData();
result.then(data => {
    const s: string = data;
});
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
    assert!(
        errors.is_empty(),
        "Promise resolution should use stable DefId path.\nDiagnostics: {errors:#?}"
    );
}

#[test]
fn test_import_type_lib_promise_stable_lowering() {
    // import("...") type expressions for Promise should resolve through the
    // stable lib lowering path without local DefId repair.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyPromise<T> = Promise<T>;
const p: MyPromise<number> = Promise.resolve(42);
"#,
    );
    // We check there are no false TS2322 errors from broken type identity.
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "import-type Promise alias should resolve without TS2322.\nDiagnostics: {ts2322:#?}"
    );
}

#[test]
fn test_library_reference_heritage_chain_via_stable_helpers() {
    // Tests that lib-type heritage chains (e.g., Array extends ReadonlyArray)
    // resolve correctly through the stable identity helpers, ensuring that
    // inherited methods like `concat`, `indexOf` etc. are available.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: number[] = [1, 2, 3];
const idx: number = arr.indexOf(2);
const sliced: number[] = arr.slice(0, 2);
const joined: string = arr.join(",");
const includes: boolean = arr.includes(1);
"#,
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339 || *c == 2322)
        .collect();
    assert!(
        errors.is_empty(),
        "Array methods from ReadonlyArray heritage should resolve via stable helpers.\n\
         Diagnostics: {errors:#?}"
    );
}

#[test]
fn test_promise_multiple_generic_instantiations_stable() {
    // Multiple Promise instantiations with different type args should each
    // resolve through the stable DefId path without identity confusion.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib_and_options(
        r#"
async function getString(): Promise<string> { return "a"; }
async function getNumber(): Promise<number> { return 1; }
async function getBool(): Promise<boolean> { return true; }
const s: Promise<string> = getString();
const n: Promise<number> = getNumber();
const b: Promise<boolean> = getBool();
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Multiple Promise<T> instantiations should all use stable DefId.\n\
         Diagnostics: {ts2322:#?}"
    );
}

// ---- Heritage resolution wiring tests ----

#[test]
fn test_resolve_heritage_wired_during_check_source_file() {
    // Verify that resolve_cross_batch_heritage runs during check_source_file,
    // so a user class extending a lib class gets its DefId-level extends set.
    if !lib_files_available() {
        return;
    }

    let lib_files = load_lib_files_for_test();
    let source = r#"
class MyError extends Error {
    constructor(message: string) {
        super(message);
    }
}
const e: MyError = new MyError("oops");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let raw_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_binder::state::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&raw_contexts);
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    // Run the full check pipeline (which includes heritage resolution wiring)
    checker.check_source_file(root);

    // After checking, look up the DefId for MyError and verify it has extends set
    let my_error_sym = binder.file_locals.get("MyError");
    if let Some(my_error_sym) = my_error_sym
        && let Some(my_error_def) = checker.ctx.get_existing_def_id(my_error_sym)
    {
        let info = checker.ctx.definition_store.get(my_error_def);
        assert!(
            info.is_some(),
            "MyError's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            // The heritage resolution should have set extends to Error's DefId
            assert!(
                info.extends.is_some(),
                "MyError should have extends set to Error's DefId after heritage resolution."
            );
        }
    }
}

#[test]
fn test_resolve_heritage_user_class_extends_user_class() {
    // Verify heritage resolution works for user-defined classes within the same file
    // (same batch, so heritage should resolve during the primary binder pass).
    let source = r#"
class Base {
    x: number = 1;
}
class Child extends Base {
    y: string = "hello";
}
const c: Child = new Child();
"#;

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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Look up Child's DefId
    let child_sym = binder.file_locals.get("Child");
    let base_sym = binder.file_locals.get("Base");
    if let (Some(child_sym), Some(base_sym)) = (child_sym, base_sym)
        && let (Some(child_def), Some(_base_def)) = (
            checker.ctx.get_existing_def_id(child_sym),
            checker.ctx.get_existing_def_id(base_sym),
        )
    {
        let info = checker.ctx.definition_store.get(child_def);
        assert!(
            info.is_some(),
            "Child's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            assert!(
                info.extends.is_some(),
                "Child should have extends set to Base's DefId after heritage resolution."
            );
        }
    }
}

#[test]
#[ignore = "heritage resolution no longer populates DefinitionInfo.implements; resolved through type pipeline instead"]
fn test_resolve_heritage_interface_implements() {
    // Verify heritage resolution wires implements for interfaces.
    let source = r#"
interface Animal {
    name: string;
}
interface Dog extends Animal {
    breed: string;
}
const d: Dog = { name: "Rex", breed: "Lab" };
"#;

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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Look up Dog's DefId
    let dog_sym = binder.file_locals.get("Dog");
    if let Some(dog_sym) = dog_sym
        && let Some(dog_def) = checker.ctx.get_existing_def_id(dog_sym)
    {
        let info = checker.ctx.definition_store.get(dog_def);
        assert!(
            info.is_some(),
            "Dog's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            // For interfaces, heritage goes into implements
            assert!(
                !info.implements.is_empty(),
                "Dog should have implements set to Animal's DefId after heritage resolution."
            );
        }
    }
}

// =========================================================================
// Focused tests: prime_lib_type_params via get_lib_def_id
// =========================================================================
// These tests validate that prime_lib_type_params uses the stable
// get_lib_def_id helper (instead of get_existing_def_id with early return),
// ensuring type params are primed even when pre-population has gaps.

#[test]
fn test_prime_lib_type_params_via_get_lib_def_id() {
    // Array<T> type params should be primed even when accessed indirectly
    // through a nested generic context, exercising the get_lib_def_id path
    // in prime_lib_type_params.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function wrap<T>(value: T): Array<T> {
    return [value];
}
const nums: Array<number> = wrap(42);
const strs: Array<string> = wrap("hello");
// Should error: string not assignable to number
const bad: Array<number> = wrap("oops");
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2322),
        "Array<number> = wrap('oops') should produce TS2322 when type params are primed.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_type_params_primed_for_nested_generics() {
    // Promise type params must be primed via get_lib_def_id for nested
    // generic usage to infer correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function wrapInPromise<T>(value: T): Promise<T> {
    return Promise.resolve(value);
}
const p: Promise<number> = wrapInPromise(42);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise<number> = wrapInPromise(42) should resolve with primed type params.\nDiagnostics: {real_errors:#?}"
    );
}

// =========================================================================
// Focused tests: import-type lowering through lib resolution
// =========================================================================

#[test]
fn test_import_type_indirect_lib_generic() {
    // Type alias chains that eventually reference lib generics should
    // resolve through the stable identity path without DefId repair.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MaybeArray<T> = Array<T> | T;
type Numbers = MaybeArray<number>;
const a: Numbers = [1, 2, 3];
const b: Numbers = 42;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "MaybeArray<number> union with lib Array should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

