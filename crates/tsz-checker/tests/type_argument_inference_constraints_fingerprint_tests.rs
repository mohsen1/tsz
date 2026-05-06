use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
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
    checker.ctx.set_lib_contexts(Vec::new());

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .collect()
}

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = [
        "es5.d.ts",
        "dom.d.ts",
        "dom.iterable.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert(file_name.to_string()) {
                    break;
                }
                lib_files.push(Arc::new(LibFile::from_source(
                    file_name.to_string(),
                    content,
                )));
                break;
            }
        }
    }

    lib_files
}

fn diagnostics_with_libs(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn invalid_explicit_type_arg_constraints_suppress_call_argument_cascades() {
    let source = r#"
function someGenerics1<T, U extends T>(n: T, m: number) { }
someGenerics1<string, number>(3, 4);

function someGenerics5<U extends number, T>(n: T, f: (x: U) => void) { }
someGenerics5<string, number>(null, null);
"#;

    let diagnostics = diagnostics(source);
    let ts2344 = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .count();
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2345)
        .collect();

    assert_eq!(ts2344, 2, "expected one TS2344 for each bad type argument");
    assert!(
        ts2345.is_empty(),
        "invalid explicit type arguments should suppress same-call TS2345 cascades, got: {ts2345:#?}"
    );
}

#[test]
fn unresolved_sensitive_callback_context_uses_type_parameter_constraint() {
    let source = r#"
interface WindowLike {
    closed: boolean;
}

function someGenerics3<T extends WindowLike>(producer: () => T) { }
someGenerics3(() => '');
"#;

    let diagnostics = diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected callback return to be checked against the generic constraint, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0].message_text.contains("WindowLike"),
        "expected diagnostic to mention the constraint type, got: {:?}",
        ts2322[0]
    );
}

#[test]
fn lib_backed_window_constraint_contextualizes_sensitive_callback_return() {
    let source = r#"
function someGenerics3<T extends Window>(producer: () => T) { }
someGenerics3(() => '');
someGenerics3<number>(() => 3);
"#;

    let diagnostics = diagnostics_with_libs(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect();

    assert!(
        ts2322
            .iter()
            .any(|diagnostic| diagnostic.message_text.contains("Window")),
        "expected callback return to be checked against Window, got: {diagnostics:#?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|diagnostic| diagnostic.message_text.contains("Window")),
        "expected explicit type argument to be checked against Window, got: {diagnostics:#?}"
    );
}
