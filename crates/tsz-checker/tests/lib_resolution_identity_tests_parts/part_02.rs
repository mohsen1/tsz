#[test]
fn test_import_type_lib_date_methods() {
    // Date from lib should resolve its methods (getTime, toISOString, etc.)
    // correctly via the stable DefId path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const d: Date = new Date();
const time: number = d.getTime();
const iso: string = d.toISOString();
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
        "Date methods should resolve via stable lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- SymbolId-typed resolution path tests ----
//
// These tests verify that the refactored resolution helpers
// (resolve_lib_node_in_arenas returning SymbolId instead of raw u32)
// produce correct results through the full lowering pipeline.

#[test]
fn test_promise_resolve_via_sym_id_typed_path() {
    // Verify Promise resolves correctly through the SymbolId-typed
    // resolution path (resolve_lib_node_in_arenas -> SymbolId -> get_lib_def_id).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<string> {
    return "hello";
}
const result: Promise<number> = Promise.resolve(42);
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        real_errors.is_empty(),
        "Promise should resolve via SymbolId-typed resolution path without errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_array_map_filter_via_sym_id_path() {
    // Array methods like map/filter rely on the lib heritage chain
    // (Array extends ReadonlyArray) resolving through SymbolId-typed helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const nums: number[] = [1, 2, 3];
const doubled: number[] = nums.map(x => x * 2);
const evens: number[] = nums.filter(x => x % 2 === 0);
const joined: string = nums.join(", ");
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
        "Array methods should resolve through SymbolId-typed lib heritage chain.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_expression_promise_via_sym_id_path() {
    // import() type expressions for lib types must go through the
    // SymbolId-typed resolution path correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyPromise = Promise<string>;
const p: MyPromise = Promise.resolve("test");
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        real_errors.is_empty(),
        "Type alias referencing Promise should work via SymbolId-typed path.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_nested_generic_via_sym_id_path() {
    // Nested generics (e.g., Promise<Array<Map<string, number>>>) exercise
    // the SymbolId-typed resolution recursively through multiple lib types.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<Array<number>> = Promise.resolve([1, 2, 3]);
const nested: Array<Promise<string>> = [Promise.resolve("a")];
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        real_errors.is_empty(),
        "Nested lib generics should resolve via SymbolId-typed path.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_then_catch_finally_chain_via_sym_id_path() {
    // Promise method chains exercise heritage resolution (Promise members)
    // through the SymbolId-typed path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p = Promise.resolve(42);
const chained = p.then(x => x.toString()).catch(e => "error");
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
        "Promise .then/.catch chain should resolve via SymbolId-typed path.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Stable helper path tests (no DefId repair) ----
//
// These tests verify that lib type lowering uses the stable identity helpers
// (no_value_resolver, get_lib_def_id, register_lib_def_resolved) and does
// not fall back to on-demand DefId creation or local caching tricks.

#[test]
fn test_promise_generic_return_type_stable() {
    // Verify that Promise<T> as a generic return type uses stable DefId lowering.
    // The `then` callback's return type should propagate through the Promise chain.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<{ name: string; age: number }> {
    return { name: "test", age: 30 };
}
const result: Promise<{ name: string; age: number }> = fetchData();
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[6133]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2322, 2345]),
        "Promise<{{name, age}}> return type should be stable across references.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_nested_generic_stable_lowering() {
    // Nested generics like Promise<Array<T>> exercise both Promise and Array
    // DefId resolution through stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function getItems(): Promise<Array<string>> {
    return ["a", "b"];
}
const items: Promise<Array<string>> = getItems();
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Promise<Array<string>> nested generics should resolve via stable helpers.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_error_type_stable_identity() {
    // Error is a lib type that exercises the interface heritage path
    // (Error extends Object in lib.es5.d.ts).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const e = new Error("test");
const msg: string = e.message;
const name: string = e.name;
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
        "Error lib type properties should resolve via stable DefId path.\nDiagnostics: {real_errors:#?}"
    );
}

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
    let real_errors = diagnostics_without_codes(&diagnostics, &[6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
        !has_diagnostic_code(&diagnostics, 2304),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[6133, 2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[6133]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2339, 2322]),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[6133]);
    // Conditional type inference on Promise should work
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
        "lib.es2025.iterator.d.ts",
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

    let (parser, root) = parse_test_source(source);

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

fn compile_with_esnext_iterator_libs(source: &str) -> Vec<(u32, String)> {
    let lib_files = load_lib_files(&[
        "es5.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "esnext.iterator.d.ts",
    ]);
    if lib_files.is_empty() {
        return Vec::new();
    }
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            strict: true,
            strict_null_checks: true,
            ..Default::default()
        },
        &lib_files,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn compile_multi_file_with_esnext_iterator_libs(
    files: &[(&str, &str)],
    entry_file: &str,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files(&[
        "es5.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "esnext.iterator.d.ts",
    ]);
    if lib_files.is_empty() {
        return Vec::new();
    }
    tsz_checker::test_utils::check_multi_file_with_libs(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            strict: true,
            strict_null_checks: true,
            ..Default::default()
        },
        &lib_files,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
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
    let ts2351 = diagnostics_with_code(&diagnostics, 2351);
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
    let ts2351 = diagnostics_with_code(&diagnostics, 2351);
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for Proxy with handler methods, got: {ts2351:#?}"
    );
}

#[test]
fn test_proxy_handler_identity_wrapper_uses_contextual_callback_target() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
function deprecate<T extends Function>(fn: T, msg: string, code: string): T {
    return fn;
}

function soonFrozenObjectDeprecation<T extends object>(obj: T) {
    return new Proxy(obj, {
        defineProperty: deprecate(
            (target, property, descriptor) => Reflect.defineProperty(target, property, descriptor),
            "msg",
            "code"
        ),
        deleteProperty: deprecate(
            (target, property) => Reflect.deleteProperty(target, property),
            "msg",
            "code"
        ),
        setPrototypeOf: deprecate(
            (target, proto) => Reflect.setPrototypeOf(target, proto),
            "msg",
            "code"
        ),
    });
}
"#,
    );
    let ts2322 = diagnostics_with_code(&diagnostics, 2322);
    assert!(
        ts2322.is_empty(),
        "Expected identity-wrapped Proxy handler callbacks to use contextual targets, got: {ts2322:#?}"
    );
}

#[test]
fn test_builtin_iterator_constructor_uses_scoped_abstract_typeof_alias() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
new Iterator<number>();
class C extends Iterator<number> {}
"#,
    );
    assert!(
        has_diagnostic_code(&diagnostics, 2511),
        "Expected TS2511 for abstract builtin Iterator construction. Got: {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_code(&diagnostics, 2515),
        "Expected TS2515 for missing abstract Iterator.next implementation. Got: {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_code_message(&diagnostics, 2515, "Iterator<number, undefined, unknown>"),
        "Expected TS2515 to display builtin Iterator with scoped abstract defaults. Got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2351),
        "Expected no TS2351 for builtin Iterator constructor. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_builtin_iterator_protocol_uses_scoped_defaults_in_errors() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
class BadIterator1 extends Iterator<number> {
  next() {
    if (Math.random() < .5) {
      return { done: false, value: 0 } as const;
    } else {
      return { done: true, value: "a string" } as const;
    }
  }
}
class BadIterator2 extends Iterator<number> {
  next() {
    return { done: false, value: 0 };
  }
}
class BadIterator3 extends Iterator<string> {
  next() {
    return { done: false, value: 0 };
  }
}
declare const g1: Generator<string, number, boolean>;
const iter1 = Iterator.from(g1);
declare const iter2: IteratorObject<string>;
const iter3 = iter2.flatMap(() => g1);
"#,
    );

    let ts2416_count = diagnostic_count(&diagnostics, 2416);
    assert_eq!(
        ts2416_count, 2,
        "Expected two TS2416 Iterator.next diagnostics. Got: {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_code(&diagnostics, 2345),
        "Expected TS2345 for Iterator.from rejecting Generator TNext. Got: {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Expected TS2322 for flatMap callback rejecting Generator TNext. Got: {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_code_message(&diagnostics, 2416, "Iterator<number, undefined, unknown>"),
        "Expected Iterator heritage diagnostics to show scoped abstract defaults. Got: {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_code_message(&diagnostics, 2416, "Iterator<string, undefined, unknown>"),
        "Expected Iterator<string> heritage diagnostics to show scoped abstract defaults. Got: {diagnostics:#?}"
    );
    let bare_iterator_base_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2416
                && message.contains("base type 'Iterator<")
                && !message.contains("undefined, unknown")
        })
        .collect();
    assert!(
        bare_iterator_base_messages.is_empty(),
        "Expected every builtin Iterator TS2416 base type display to include scoped defaults. Got: {bare_iterator_base_messages:#?}"
    );
}

#[test]
fn test_esnext_builtin_iterator_protocol_uses_scoped_defaults_in_all_errors() {
    let diagnostics = compile_with_esnext_iterator_libs(
        r#"
class BadIterator1 extends Iterator<number> {
  next() {
    if (Math.random() < .5) {
      return { done: false, value: 0 } as const;
    } else {
      return { done: true, value: "a string" } as const;
    }
  }
}
class BadIterator2 extends Iterator<number> {
  next() {
    return { done: false, value: 0 };
  }
}
class BadIterator3 extends Iterator<number> {
  next() {
    if (Math.random() < .5) {
      return { done: false, value: 0 };
    } else {
      return { done: true, value: "a string" };
    }
  }
}
"#,
    );

    let ts2416: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2416)
        .collect();
    assert_eq!(
        ts2416.len(),
        3,
        "Expected three Iterator.next override diagnostics. Got: {diagnostics:#?}"
    );
    assert!(
        ts2416
            .iter()
            .all(|(_, message)| message.contains("Iterator<number, undefined, unknown>")),
        "Expected every ESNext Iterator TS2416 base type display to include scoped defaults. Got: {ts2416:#?}"
    );
}

#[test]
fn test_namespace_iterator_class_does_not_use_builtin_defaults_in_errors() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
namespace N {
  export class Iterator<T> {
    next(): T {
      throw new Error();
    }
  }
}
class Bad extends N.Iterator<number> {
  override next(): string {
    return "";
  }
}
"#,
    );

    assert!(
        has_diagnostic_code_message(&diagnostics, 2416, "Iterator<number>"),
        "Expected user-defined namespace Iterator diagnostic to keep its written arity. Got: {diagnostics:#?}"
    );
    assert!(
        !has_diagnostic_code_message(&diagnostics, 2416, "Iterator<number, undefined, unknown>"),
        "Expected user-defined namespace Iterator not to receive builtin Iterator defaults. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_module_local_iterator_class_does_not_use_builtin_defaults_in_errors() {
    let diagnostics = compile_with_esnext_iterator_libs(
        r#"
export {};
class Iterator<T> {
  next(): T {
    throw new Error();
  }
}
class Bad extends Iterator<number> {
  override next(): string {
    return "";
  }
}
"#,
    );

    assert!(
        has_diagnostic_code_message(&diagnostics, 2416, "Iterator<number>"),
        "Expected module-local Iterator diagnostic to keep its written arity. Got: {diagnostics:#?}"
    );
    assert!(
        !has_diagnostic_code_message(&diagnostics, 2416, "Iterator<number, undefined, unknown>"),
        "Expected module-local Iterator not to receive builtin Iterator defaults. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_imported_iterator_class_does_not_use_builtin_defaults_in_errors() {
    let diagnostics = compile_multi_file_with_esnext_iterator_libs(
        &[
            (
                "./consumer.ts",
                r#"
import { Iterator } from "./user";
class Bad extends Iterator<number> {
  override next(): string {
    return "";
  }
}
"#,
            ),
            (
                "./user.ts",
                r#"
export class Iterator<T> {
  next(): T {
    throw new Error();
  }
}
"#,
            ),
        ],
        "./consumer.ts",
    );

    assert!(
        has_diagnostic_code_message(&diagnostics, 2416, "Iterator<number>"),
        "Expected imported user Iterator diagnostic to keep its written arity. Got: {diagnostics:#?}"
    );
    assert!(
        !has_diagnostic_code_message(&diagnostics, 2416, "Iterator<number, undefined, unknown>"),
        "Expected imported user Iterator not to receive builtin Iterator defaults. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_map_iterator_next_uses_strict_builtin_iterator_return() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
declare const map: Map<string, number>;
const value: number = map.values().next().value;
interface Next<A> {
    readonly done?: boolean;
    readonly value: A;
}
const result: Next<number> = map.values().next();
"#,
    );
    let ts2322_count = diagnostic_count(&diagnostics, 2322);
    assert_eq!(
        ts2322_count, 2,
        "Expected strict built-in iterator return to report both MapIterator.next assignments. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_synthesized_array_iterator_methods_see_es2025_helpers() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
[1, 2, 3, 4].values()
    .filter((x) => x % 2 === 0)
    .map((x) => x * 10)
    .toArray();
"#,
    );
    let false_iterator_helper_diags = diagnostics_with_any_code(&diagnostics, &[2339, 7006]);
    assert!(
        false_iterator_helper_diags.is_empty(),
        "Expected synthesized ArrayIterator methods to inherit es2025 iterator helpers. Got: {diagnostics:#?}"
    );
}

// ---------------------------------------------------------------------------
// Regression tests for issue #8422: cross-arena NodeIndex collision in
// multi-lib built-in interface type lowering.
//
// Map<K,V> and Set<T> are declared across multiple lib files. Using the wrong
// arena per-declaration injects spurious [Symbol.iterator] signatures and
// produces false-positive TS2416 / TS2322.
// ---------------------------------------------------------------------------

#[test]
fn test_map_subclass_symbol_iterator_compatible_override_no_ts2416() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    // Verify with two different class names to prove name-independence.
    for (label, source) in [
        (
            "MyMap",
            r#"
class MyMap extends Map<string, number> {
    [Symbol.iterator](): MapIterator<[string, number]> {
        return super[Symbol.iterator]();
    }
}
"#,
        ),
        (
            "Bag",
            r#"
class Bag extends Map<string, number> {
    [Symbol.iterator](): MapIterator<[string, number]> {
        return super[Symbol.iterator]();
    }
}
"#,
        ),
    ] {
        let diagnostics = compile_with_es2015_sublibs(source);
        let ts2416 = diagnostics_with_code(&diagnostics, 2416);
        assert!(
            ts2416.is_empty(),
            "class {label} extending Map<string,number> with compatible [Symbol.iterator] \
             override should NOT emit TS2416 — cross-arena collision was injecting extra \
             Iterable signatures. Got: {ts2416:#?}"
        );
    }
}

#[test]
fn test_set_subclass_symbol_iterator_compatible_override_no_ts2416() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    // Set<T> also spans multiple lib arenas; verify the same fix applies.
    for (label, source) in [
        (
            "NumberSet",
            r#"
class NumberSet extends Set<number> {
    [Symbol.iterator](): SetIterator<number> {
        return super[Symbol.iterator]();
    }
}
"#,
        ),
        (
            "TypedSet<E>",
            r#"
class TypedSet<E> extends Set<E> {
    [Symbol.iterator](): SetIterator<E> {
        return super[Symbol.iterator]();
    }
}
"#,
        ),
    ] {
        let diagnostics = compile_with_es2015_sublibs(source);
        let ts2416 = diagnostics_with_code(&diagnostics, 2416);
        assert!(
            ts2416.is_empty(),
            "class {label} extending Set<T> with compatible [Symbol.iterator] override \
             should NOT emit TS2416. Got: {ts2416:#?}"
        );
    }
}

#[test]
fn test_map_subclass_symbol_iterator_wrong_return_still_gets_ts2416() {
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    // A genuinely incompatible override must still produce TS2416.
    let diagnostics = compile_with_es2015_sublibs(
        r#"
class BadMap extends Map<string, number> {
    [Symbol.iterator](): MapIterator<string> {
        return null as any;
    }
}
"#,
    );
    let ts2416_count = diagnostic_count(&diagnostics, 2416);
    assert!(
        ts2416_count >= 1,
        "class with incompatible [Symbol.iterator] return type should emit TS2416. \
         Got: {diagnostics:#?}"
    );
}

#[test]
fn test_cross_arena_computed_name_precompute_does_not_corrupt_user_symbol_names() {
    // Regression for the precompute half of issue #8422: when in cross-arena lib
    // delegation, precompute_computed_property_names and
    // precompute_symbol_named_computed_property_names must NOT run through
    // self.ctx.arena for lib declarations. A NodeIndex from a lib file arena that
    // collides with a valid-but-unrelated node in self.ctx.arena would insert a
    // wrong NodeIndex into the computed_symbol_names set, which could then
    // incorrectly tag user-file expression nodes as symbol-named (false positive)
    // or miss actual symbol-named lib members (false negative).
    //
    // This test uses a source file large enough to produce NodeIndex values that
    // overlap with typical lib declaration NodeIndexes, and combines:
    //   - a user interface with unique-symbol computed member names
    //   - a class extending Map (whose [Symbol.iterator] is in a remote lib arena)
    //   - a class overriding [Symbol.iterator] correctly (no TS2416)
    //   - a class overriding [Symbol.iterator] incorrectly (TS2416 must fire)
    // If precompute pollution occurs, the user symbol entries could receive wrong
    // NodeIndex keys, breaking either the user interface or the Map override checks.
    let lib_files = load_lib_files_with_es2015_sublibs();
    if lib_files.is_empty() {
        return;
    }
    let diagnostics = compile_with_es2015_sublibs(
        r#"
// Enough declarations to occupy low NodeIndex slots that lib files also use.
const s1 = Symbol("s1");
const s2 = Symbol("s2");
const s3 = Symbol("s3");
interface Tagged {
    [s1]: number;
    [s2]: string;
    [s3]: boolean;
}
declare const t: Tagged;
const _n: number = t[s1];
const _s: string = t[s2];
const _b: boolean = t[s3];

// Cross-arena computed name: [Symbol.iterator] lives in es2015.iterable arena.
// A compatible override must NOT trigger TS2416.
class GoodMap extends Map<string, number> {
    [Symbol.iterator](): MapIterator<[string, number]> {
        return super[Symbol.iterator]();
    }
}

// An incompatible override MUST trigger TS2416.
class WrongMap extends Map<string, number> {
    [Symbol.iterator](): MapIterator<string> {
        return null as any;
    }
}
"#,
    );

    let ts2416 = diagnostics_with_code(&diagnostics, 2416);
    let ts2322 = diagnostics_with_code(&diagnostics, 2322);
    assert!(
        ts2416
            .iter()
            .all(|d| d.1.contains("WrongMap") || d.1.contains("[Symbol.iterator]")),
        "TS2416 should only fire for WrongMap's incompatible [Symbol.iterator]. Got: {ts2416:#?}"
    );
    assert!(
        ts2416.iter().all(|d| !d.1.contains("GoodMap")),
        "GoodMap has a compatible [Symbol.iterator] and must NOT produce TS2416. Got: {ts2416:#?}"
    );
    assert!(
        ts2322.is_empty(),
        "No TS2322 expected for user symbol reads or Map usage. Got: {ts2322:#?}"
    );
    assert!(
        !ts2416.is_empty(),
        "WrongMap must produce at least one TS2416. Got: {diagnostics:#?}"
    );
}
