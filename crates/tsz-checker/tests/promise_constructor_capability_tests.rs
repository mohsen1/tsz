use std::sync::Arc;

use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn load_lib_files(names: &[&str]) -> Vec<Arc<LibFile>> {
    tsz_checker::test_utils::load_compiled_lib_files(names)
}

fn check_with_libs(source: &str, lib_names: &[&str]) -> Vec<Diagnostic> {
    let lib_files = load_lib_files(lib_names);

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn es2015_target_with_es5_lib_still_reports_missing_promise_constructor() {
    let diagnostics = check_with_libs(
        r#"
const loadAsync = async () => {
    await import("./dep");
};
"#,
        &["lib.es5.d.ts"],
    );

    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2705),
        "Expected TS2705 for async function with ES5-only libs, got: {diagnostics:#?}"
    );
    assert!(
        codes.contains(&2712),
        "Expected TS2712 for dynamic import with ES5-only libs, got: {diagnostics:#?}"
    );
}

#[test]
fn reports_ts2712_for_each_dynamic_import_site_in_conformance_shape() {
    let source = r#"
declare var console: any;
class C {
    private myModule = import("./0");
    method() {
        const loadAsync = import("./0");
        this.myModule.then(Zero => {
            console.log(Zero.foo());
        }, async err => {
            console.log(err.message);
        });
        const loadAsync2 = import("./0");
        const loadAsync3 = import("./0");
    }
}
"#;

    let diagnostics = check_with_libs(source, &["lib.es5.d.ts"]);
    let ts2712_count = diagnostics.iter().filter(|d| d.code == 2712).count();

    assert_eq!(
        ts2712_count, 4,
        "Expected 4 TS2712 errors (one per import site), got: {ts2712_count}",
    );
}

// Regression for issue #4762: async METHODS were missing the same
// async-return-must-be-Promise check that function declarations get.
// Mirrors `ts1064_suggestion_wraps_declared_return_type` below but for
// class methods (concrete and abstract). Pre-fix, all three method
// arms silently accepted non-Promise return types.
#[test]
fn ts1064_fires_for_async_methods_with_non_promise_return() {
    let diagnostics = check_with_libs(
        r#"
interface Box<T> {
  value: T;
}

class C {
  async primitive(): number {
    return 1;
  }

  async generic(): Box<number> {
    return { value: 1 };
  }
}

abstract class D {
  abstract async ambient(): number;
}
"#,
        &["lib.es5.d.ts", "lib.es2015.promise.d.ts"],
    );

    let ts1064: Vec<_> = diagnostics.iter().filter(|d| d.code == 1064).collect();
    // Concrete `primitive` and `generic`, and abstract `ambient` — three
    // method-level TS1064s in total. Pre-fix, ts1064.len() was 0.
    assert_eq!(
        ts1064.len(),
        3,
        "Expected three TS1064 diagnostics for async methods, got: {diagnostics:#?}"
    );

    let messages: Vec<_> = ts1064.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Promise<number>")),
        "Expected TS1064 suggestion for Promise<number>, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Promise<Box<number>>")),
        "Expected TS1064 suggestion for Promise<Box<number>>, got: {messages:#?}"
    );
}

// Regression: a non-async method with a non-Promise return type must
// NOT trigger TS1064. Pins the `is_async` guard inside the validator
// — without it the new call site would over-fire on every annotated
// non-Promise method, including ordinary synchronous ones.
#[test]
fn ts1064_does_not_fire_for_non_async_methods() {
    let diagnostics = check_with_libs(
        r#"
class C {
  sync(): number {
    return 1;
  }
}
"#,
        &["lib.es5.d.ts", "lib.es2015.promise.d.ts"],
    );

    let ts1064: Vec<_> = diagnostics.iter().filter(|d| d.code == 1064).collect();
    assert!(
        ts1064.is_empty(),
        "Did not expect any TS1064 diagnostics for non-async methods, got: {diagnostics:#?}"
    );
}

// Regression: a generator method (`async *`) must NOT trigger TS1064 —
// generators have a different return-type protocol (AsyncGenerator)
// that the validator's `is_generator` guard short-circuits on.
#[test]
fn ts1064_does_not_fire_for_async_generator_method() {
    let diagnostics = check_with_libs(
        r#"
class C {
  async *gen(): number {
    yield 1;
  }
}
"#,
        &["lib.es5.d.ts", "lib.es2015.promise.d.ts"],
    );

    let ts1064: Vec<_> = diagnostics.iter().filter(|d| d.code == 1064).collect();
    assert!(
        ts1064.is_empty(),
        "Did not expect any TS1064 diagnostics for async generator method (different protocol), got: {diagnostics:#?}"
    );
}

#[test]
fn ts1064_suggestion_wraps_declared_return_type() {
    let diagnostics = check_with_libs(
        r#"
interface Box<T> {
  value: T;
}

async function primitive(): number {
  return 1;
}

async function generic(): Box<number> {
  return { value: 1 };
}
"#,
        &["lib.es5.d.ts", "lib.es2015.promise.d.ts"],
    );

    let ts1064: Vec<_> = diagnostics.iter().filter(|d| d.code == 1064).collect();
    assert_eq!(
        ts1064.len(),
        2,
        "Expected two TS1064 diagnostics, got: {diagnostics:#?}"
    );

    let messages: Vec<_> = ts1064.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Promise<number>")),
        "Expected TS1064 suggestion for Promise<number>, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Promise<Box<number>>")),
        "Expected TS1064 suggestion for Promise<Box<number>>, got: {messages:#?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("Promise<void>")),
        "Did not expect Promise<void> fallback suggestion, got: {messages:#?}"
    );
}

#[test]
fn ts1064_does_not_fire_for_global_promise_with_indexed_return_type() {
    let diagnostics = check_with_libs(
        r#"
interface Obj {
  stringProp: string;
  anyProp: any;
}

async function tuple(): Promise<[number, boolean]> {
  throw 0;
}

async function indexed(obj: Obj): Promise<Obj["stringProp"]> {
  return obj.stringProp;
}

async function genericIndexed<TObj extends Obj, K extends keyof TObj>(
  obj: TObj,
  key: K,
): Promise<TObj[K]> {
  return obj[key];
}
"#,
        &["lib.es5.d.ts", "lib.es2015.promise.d.ts"],
    );

    let ts1064: Vec<_> = diagnostics.iter().filter(|d| d.code == 1064).collect();
    assert!(
        ts1064.is_empty(),
        "Did not expect TS1064 for annotations resolving to the global Promise, got: {diagnostics:#?}"
    );
}

#[test]
fn preserves_type_parameter_from_custom_promise_like_type() {
    // Test that we preserve type parameters from custom Promise-like types
    // even when complex Promise unwrapping fails.
    // This tests the fix for: async example<T>(): Task<T> { return; }
    // where Task<T> extends Promise<T>
    let diagnostics = check_with_libs(
        r#"
class Task<T> extends Promise<T> { }

class Test {
    async example<T>(): Task<T> { return; }
}
"#,
        &["lib.es2015.full.d.ts"],
    );

    // We expect TS2322 for bare return statement
    // The key is that we check against the unwrapped type parameter 'T',
    // not the full Task<T> type.
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for bare return with custom Promise type, got: {diagnostics:#?}"
    );
}
