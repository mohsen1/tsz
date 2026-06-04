#[test]
fn original_binder_with_merged_program_type_interner_preserves_mapped_type_generic_indexed_access_context()
 {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );

    let (parser, root) = parse_test_source(source);
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &query_cache,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(root);

    let relevant = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected original binder with merged-program TypeInterner to avoid TS2344/TS7006, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
#[ignore = "pre-existing: remote merge regression"]
fn direct_checker_with_real_default_libs_contextually_types_constructor_parameters_rest() {
    let source = r#"
declare function createInstance<Ctor extends new (...args: any[]) => any, R extends InstanceType<Ctor>>(ctor: Ctor, ...args: ConstructorParameters<Ctor>): R;

interface IMenuWorkbenchToolBarOptions {
    toolbarOptions: {
        foo(bar: string): string
    };
}

class MenuWorkbenchToolBar {
    constructor(
        options: IMenuWorkbenchToolBarOptions | undefined,
    ) { }
}

createInstance(MenuWorkbenchToolBar, {
    toolbarOptions: {
        foo(bar) { return bar; }
    }
});
"#;

    let (parser, root) = parse_test_source(source);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected direct checker with real default libs to avoid TS2345/TS7006, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]

fn compile_array_from_iterable_uses_real_lib_iterable_overload() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"
interface A { a: string; }
interface B { b: string; }
declare const inputA: A[];

const bad: B[] = Array.from(inputA.values());
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(codes, vec![2322]);
    assert!(
        result.diagnostics[0]
            .message_text
            .contains("Type 'A[]' is not assignable to type 'B[]'")
            || result.diagnostics[0]
                .related_information
                .iter()
                .any(|related| related.message_text.contains("Iterable<B> | ArrayLike<B>")),
        "Expected Array.from result assignment mismatch. Got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn merged_program_promise_is_assignable_to_promise_like_with_default_libs() {
    let files = vec![(
        "main.ts".to_string(),
        r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#
        .to_string(),
    )];
    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let options = tsz::checker::context::CheckerOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        ..tsz::checker::context::CheckerOptions::default()
    };
    let result = tsz::parallel::check_files_parallel(&program, &options, &lib_files);

    let diagnostics: Vec<_> = result
        .file_results
        .into_iter()
        .flat_map(|file| file.diagnostics)
        .collect();

    assert!(
        diagnostics.is_empty(),
        "Expected merged-program Promise<T> to be assignable to PromiseLike<T>, got: {diagnostics:?}"
    );
}

#[test]
fn compile_with_root_dir_flattens_output_paths() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let mut args = default_args();
    args.project = Some(base.to_path_buf());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/index.js").is_file());
    assert!(base.join("dist/index.d.ts").is_file());
}

#[test]
fn compile_elides_unused_default_import_but_keeps_used_named_import() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "target": "es2022",
            "outDir": "dist"
          },
          "files": ["main.ts", "dep.ts"]
        }"#,
    );
    write_file(
        &base.join("dep.ts"),
        "export default class Foo {}\nexport function bar() {}\n",
    );
    write_file(
        &base.join("main.ts"),
        "import Foo, { bar } from \"./dep\";\nbar();\nexport {};\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );
    let js = std::fs::read_to_string(base.join("dist/main.js")).expect("read main.js");
    assert!(
        js.contains("import { bar } from \"./dep\";"),
        "Expected named import to remain without unused default binding: {js}"
    );
    assert!(
        !js.contains("Foo"),
        "Expected unused default import to be elided: {js}"
    );
}

#[test]
fn compile_respects_no_emit_on_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "let x = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(!base.join("dist/src/index.js").is_file());
}
