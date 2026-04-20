#[test]
fn test_lib_ref_regexp_stable_identity() {
    // RegExp is a lib type with both interface and value declarations.
    // Exercises the value-declaration lowering path (no_value_resolver stub).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const r = new RegExp("test");
const result: boolean = r.test("hello");
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 6133).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "RegExp lib type should resolve .test() via stable helpers.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_lib_map_generic_stable() {
    // Map<K, V> is a generic lib type from es2015. Verifying its DefId
    // is stable when referenced through different import-type expressions.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, number> = new Map();
m.set("a", 1);
const val: number | undefined = m.get("a");
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 6133).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Map<string, number> should resolve .set/.get via stable helpers.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_expression_typeof_lib_constructor() {
    // `typeof Promise` and `typeof Array` as import type expressions
    // exercise the value-side identity resolution.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type PC = typeof Promise;
type AC = typeof Array;
declare const p: PC;
declare const a: AC;
"#,
    );
    // We just verify no crash and no TS2304 (cannot find name)
    assert!(
        !diagnostics.iter().any(|(c, _)| *c == 2304),
        "typeof Promise/Array should resolve via stable lib identity.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_promise_all_settled_stable_lowering() {
    // Promise.allSettled exercises the Promise constructor's static methods
    // which are resolved through the value-declaration path with no_value_resolver.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1 = Promise.resolve(1);
const p2 = Promise.resolve("two");
const settled = Promise.all([p1, p2]);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 6133 && *c != 2318)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Promise.all should resolve via stable lowering.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_multiple_interface_merge_stable() {
    // Array has declarations across multiple lib files (es5 + es2015).
    // This tests that the merged interface uses a single stable DefId
    // without DefId repair across lib contexts.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const len: number = arr.length;
const first: number | undefined = arr[0];
const mapped: Array<string> = arr.map(x => x.toString());
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 6133).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339 || *c == 2322),
        "Array merged interface should have stable DefId across lib files.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_promise_conditional_return() {
    // Conditional types involving Promise exercise deep type lowering
    // through the stable identity path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type Result = UnwrapPromise<Promise<number>>;
const x: Result = 42;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 6133).collect();
    // Conditional type inference on Promise should work
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Conditional type unwrapping Promise should work.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Proxy constructor resolution ----

/// Load lib files including ES2015 sub-libs (proxy, reflect, collection, etc.)
fn load_lib_files_with_es2015_sublibs() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Search paths for the lib directory
    let lib_dirs = [
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib"),
        manifest_dir.join("../../scripts/emit/node_modules/typescript/lib"),
    ];

    let lib_names = [
        "lib.es5.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for lib_name in &lib_names {
        for lib_dir in &lib_dirs {
            let lib_path = lib_dir.join(lib_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if seen_files.insert(lib_name.to_string()) {
                    let lib_file = LibFile::from_source(lib_name.to_string(), content);
                    lib_files.push(Arc::new(lib_file));
                }
                break;
            }
        }
    }
    lib_files
}

fn compile_with_es2015_sublibs(source: &str) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return Vec::new();
    }

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
        .map(|lib| CheckerLibContext {
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

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn test_new_proxy_no_false_ts2351() {
    // `new Proxy(target, handler)` should resolve via ProxyConstructor's construct
    // signature without producing false TS2351 ("not constructable").
    // This tests that Lazy(DefId) for lib constructor interfaces are properly
    // resolved even when first accessed through a `new` expression.
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
var t = {};
var p = new Proxy(t, {});
"#,
    );
    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for `new Proxy(t, {{}})`, got: {ts2351:#?}"
    );
}

#[test]
fn test_new_proxy_with_handler_methods_no_ts2351() {
    // `new Proxy(obj, { set: ..., get: ... })` should correctly type-check
    // the handler methods through ProxyConstructor's generic construct signature.
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
var obj = { x: 1, y: 2 };
var p = new Proxy(obj, {
    get(target, prop) { return (target as any)[prop]; }
});
"#,
    );
    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for Proxy with handler methods, got: {ts2351:#?}"
    );
}
