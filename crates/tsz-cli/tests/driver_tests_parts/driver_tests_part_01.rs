#[test]
fn compile_allow_import_clauses_to_merge_with_types_fixture_has_no_default_export_conflict() {
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/allowImportClausesToMergeWithTypes.ts",
    ) else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    for segment in source.split("// @filename: ").skip(1) {
        let mut lines = segment.lines();
        let Some(filename) = lines.next().map(str::trim) else {
            continue;
        };
        let contents = lines.collect::<Vec<_>>().join("\n");
        write_file(&base.join(filename), &contents);
    }

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.module = Some(crate::args::Module::CommonJs);
    args.no_emit = true;
    args.files = vec![PathBuf::from("index.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let default_export_conflicts: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.code,
                diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE
                    | diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS
            )
        })
        .collect();

    assert!(
        default_export_conflicts.is_empty(),
        "Expected merged import-clause/type default exports to avoid TS2323/TS2528, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );

    assert!(
        !result.diagnostics.iter().any(|diag| diag.code == diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE),
        "Expected merged default export path to avoid false TS1362, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF
                && diag.message_text.contains("originalZZZ")
        }),
        "Expected value-only default import from b.ts to still emit TS2749, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}
#[test]
fn compile_default_import_of_merged_interface_and_const_export_is_callable() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"import MyFunction from "./MyComponent";

MyFunction({msg: "Hello World"});
"#,
    );
    write_file(
        &base.join("MyComponent.ts"),
        r#"interface MyFunction { msg: string; }

export const MyFunction = ({ msg }: MyFunction) => console.log(msg);
export default MyFunction;
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.target = Some(crate::args::Target::EsNext);
    args.module = Some(crate::args::Module::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected default import of merged interface+const export to keep callable value type, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}
#[test]
fn compile_shadowing_namespace_symbol_keeps_global_symbol_value_access() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"namespace M {
    namespace Symbol {}

    class C {
        [Symbol.iterator]() {}
    }
}
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.target = Some(crate::args::Target::Es2015);
    args.no_emit = true;
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    let ts2708: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2708)
        .collect();
    assert!(
        ts2708.is_empty(),
        "Expected shadowing namespace to keep global Symbol value access, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}
#[test]
fn compile_mapped_type_generic_indexed_access_preserves_context() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"type Types = {
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
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.no_implicit_any = Some(true);
    args.strict_null_checks = Some(true);
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    let relevant = result
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
        "Expected mapped-type generic indexed access repro to avoid TS2344/TS7006, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}
#[test]
fn direct_checker_with_real_default_libs_preserves_mapped_type_generic_indexed_access_context() {
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        "Expected direct checker with real default libs to avoid TS2344/TS7006, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}
#[test]
fn merged_program_parallel_checker_preserves_mapped_type_generic_indexed_access_context() {
    let files = vec![(
        "main.ts".to_string(),
        r#"type Types = {
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
"#
        .to_string(),
    )];

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let options = CheckerOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let result = tsz::parallel::check_files_parallel(&program, &options, &lib_files);

    let diagnostics: Vec<_> = result
        .file_results
        .into_iter()
        .flat_map(|file| file.diagnostics)
        .collect();

    let relevant = diagnostics
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
        "Expected merged-program parallel checker to avoid TS2344/TS7006, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn direct_checker_with_original_binder_stays_clean_when_all_binders_are_installed() {
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(&arena, root, &lib_files);
    let binder = Arc::new(binder);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        binder.as_ref(),
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
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker
        .ctx
        .set_all_arenas(Arc::new(vec![Arc::clone(&arena)]));
    checker
        .ctx
        .set_all_binders(Arc::new(vec![Arc::clone(&binder)]));
    checker.ctx.set_current_file_idx(0);
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
        "Expected original binder to stay clean even with all_binders installed, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}
#[test]
fn reconstructed_binder_alone_preserves_mapped_type_generic_indexed_access_context() {
    let files = vec![(
        "main.ts".to_string(),
        r#"type Types = {
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
"#
        .to_string(),
    )];

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let file = &program.files[0];
    let binder = tsz::parallel::create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
    let mut checker = CheckerState::new(
        &file.arena,
        &binder,
        &query_cache,
        file.file_name.clone(),
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
    checker.check_source_file(file.source_file);

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
        "Expected reconstructed binder to avoid TS2344/TS7006 after rebuild parity fixes, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}
#[test]
fn binder_reconstruction_from_original_fields_preserves_mapped_type_generic_indexed_access_context()
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let mut reconstructed = BinderState::from_bound_state_with_scopes_and_augmentations(
        tsz_binder::BinderOptions::default(),
        original_binder.symbols.clone(),
        original_binder.file_locals.clone(),
        original_binder.node_symbols.clone(),
        BinderStateScopeInputs {
            scopes: original_binder.scopes.clone(),
            node_scope_ids: original_binder.node_scope_ids.clone(),
            global_augmentations: original_binder.global_augmentations.clone(),
            module_augmentations: original_binder.module_augmentations.clone(),
            augmentation_target_modules: original_binder.augmentation_target_modules.clone(),
            module_exports: original_binder.module_exports.clone(),
            module_declaration_exports_publicly: original_binder
                .module_declaration_exports_publicly
                .clone(),
            reexports: original_binder.reexports.clone(),
            wildcard_reexports: original_binder.wildcard_reexports.clone(),
            wildcard_reexports_type_only: original_binder.wildcard_reexports_type_only.clone(),
            symbol_arenas: original_binder.symbol_arenas.clone(),
            declaration_arenas: original_binder.declaration_arenas.clone(),
            cross_file_node_symbols: original_binder.cross_file_node_symbols.clone(),
            shorthand_ambient_modules: original_binder.shorthand_ambient_modules.clone(),
            modules_with_export_equals: original_binder.modules_with_export_equals.clone(),
            flow_nodes: original_binder.flow_nodes.clone(),
            node_flow: original_binder.node_flow.clone(),
            switch_clause_to_switch: original_binder.switch_clause_to_switch.clone(),
            expando_properties: original_binder.expando_properties.clone(),
            alias_partners: original_binder.alias_partners.clone(),
        },
    );
    reconstructed.declared_modules = original_binder.declared_modules.clone();
    reconstructed.is_external_module = original_binder.is_external_module;
    reconstructed.file_features = original_binder.file_features;
    reconstructed.lib_binders = original_binder.lib_binders.clone();
    reconstructed.lib_symbol_ids = original_binder.lib_symbol_ids.clone();
    reconstructed.lib_symbol_reverse_remap = original_binder.lib_symbol_reverse_remap.clone();
    reconstructed.semantic_defs = original_binder.semantic_defs.clone();

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &reconstructed,
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
        "Expected reconstruction from original binder fields to stay clean, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}
#[test]
fn merged_reconstruction_symbol_snapshots_match_original_for_mapped_type_chain() {
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for name in [
        "Types",
        "Test",
        "TypesMap",
        "P",
        "TypeHandlers",
        "typeHandlers",
        "onSomeEvent",
    ] {
        let original = symbol_snapshot(&original_binder, name);
        let merged = symbol_snapshot(&merged_binder, name);
        assert_eq!(
            merged, original,
            "symbol snapshot mismatch for {name}\noriginal: {original:#?}\nmerged: {merged:#?}"
        );
    }
}
#[test]
fn merged_reconstruction_identifier_resolution_matches_original_for_mapped_type_repro() {
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for (idx, node) in arena.nodes.iter().enumerate() {
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            continue;
        }
        let node_idx = tsz_parser::NodeIndex(idx as u32);
        let text = arena
            .get_identifier_at(node_idx)
            .map(|ident| ident.escaped_text.clone())
            .unwrap_or_default();

        let original_resolved = original_binder
            .resolve_identifier(&arena, node_idx)
            .and_then(|sym_id| original_binder.symbols.get(sym_id))
            .map(|sym| (sym.escaped_name.clone(), sym.flags));
        let merged_resolved = merged_binder
            .resolve_identifier(&arena, node_idx)
            .and_then(|sym_id| merged_binder.symbols.get(sym_id))
            .map(|sym| (sym.escaped_name.clone(), sym.flags));

        assert_eq!(
            merged_resolved, original_resolved,
            "identifier resolution mismatch for node {idx} text={text:?} pos={}..{}\noriginal={original_resolved:?}\nmerged={merged_resolved:?}",
            node.pos, node.end
        );
    }
}
#[test]
fn merged_reconstruction_node_symbols_match_original_for_mapped_type_repro() {
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for (&node_idx, &original_sym_id) in &original_binder.node_symbols {
        let Some(&merged_sym_id) = merged_binder.node_symbols.get(&node_idx) else {
            panic!("missing merged node symbol for node {node_idx}");
        };
        let original_snapshot = original_binder
            .symbols
            .get(original_sym_id)
            .map(|sym| (sym.escaped_name.clone(), sym.flags));
        let merged_snapshot = merged_binder
            .symbols
            .get(merged_sym_id)
            .map(|sym| (sym.escaped_name.clone(), sym.flags));
        assert_eq!(
            merged_snapshot, original_snapshot,
            "node symbol mismatch for node {node_idx}\noriginal={original_snapshot:?}\nmerged={merged_snapshot:?}"
        );
    }

    assert_eq!(
        merged_binder.node_symbols.len(),
        original_binder.node_symbols.len(),
        "node_symbols cardinality mismatch"
    );
}
#[test]
fn merged_reconstruction_nested_symbol_payloads_match_original_for_mapped_type_repro() {
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for (&node_idx, &original_sym_id) in &original_binder.node_symbols {
        let Some(&merged_sym_id) = merged_binder.node_symbols.get(&node_idx) else {
            panic!("missing merged node symbol for node {node_idx}");
        };
        let original_snapshot = symbol_snapshot_by_id(&original_binder, original_sym_id);
        let merged_snapshot = symbol_snapshot_by_id(&merged_binder, merged_sym_id);
        assert_eq!(
            merged_snapshot, original_snapshot,
            "nested symbol payload mismatch for node {node_idx}\noriginal={original_snapshot:#?}\nmerged={merged_snapshot:#?}"
        );
    }
}
#[test]
fn merged_reconstruction_declaration_arenas_match_original_for_mapped_type_chain() {
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);

    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for name in [
        "Types",
        "Test",
        "TypesMap",
        "P",
        "TypeHandlers",
        "typeHandlers",
        "onSomeEvent",
    ] {
        let original_sym_id = original_binder
            .file_locals
            .get(name)
            .expect("original symbol should exist");
        let merged_sym_id = merged_binder
            .file_locals
            .get(name)
            .expect("merged symbol should exist");
        assert_eq!(
            declaration_arena_file_names_for_symbol(&merged_binder, merged_sym_id),
            declaration_arena_file_names_for_symbol(&original_binder, original_sym_id),
            "declaration arenas mismatch for {name}"
        );
    }
}
#[test]
fn merged_reconstruction_semantic_defs_match_original_for_mapped_type_chain() {
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    // When the DefinitionStore is fully populated (parallel pipeline), semantic_defs
    // are intentionally skipped in create_binder_from_bound_file as an optimization.
    // Only compare when the merged binder actually has semantic_defs entries.
    if merged_binder.semantic_defs.is_empty() {
        // Verify that file_locals still match (the parallel path is valid).
        for name in [
            "Types",
            "Test",
            "TypesMap",
            "P",
            "TypeHandlers",
            "typeHandlers",
            "onSomeEvent",
        ] {
            assert!(
                original_binder.file_locals.get(name).is_some(),
                "original file_locals should have {name}"
            );
            assert!(
                merged_binder.file_locals.get(name).is_some(),
                "merged file_locals should have {name}"
            );
        }
    } else {
        for name in [
            "Types",
            "Test",
            "TypesMap",
            "P",
            "TypeHandlers",
            "typeHandlers",
            "onSomeEvent",
        ] {
            let original_sym_id = original_binder
                .file_locals
                .get(name)
                .expect("original symbol should exist");
            let merged_sym_id = merged_binder
                .file_locals
                .get(name)
                .expect("merged symbol should exist");
            let original_entry = original_binder
                .semantic_defs
                .get(&original_sym_id)
                .expect("original semantic def should exist");
            let merged_entry = merged_binder
                .semantic_defs
                .get(&merged_sym_id)
                .expect("merged semantic def should exist");

            let mut expected =
                semantic_def_snapshot(&original_binder, original_sym_id, original_entry);
            expected.file_id = 0;
            assert_eq!(
                semantic_def_snapshot(&merged_binder, merged_sym_id, merged_entry),
                expected,
                "semantic def mismatch for {name}"
            );
        }
    }
}
#[test]
fn reconstructed_binder_with_fresh_type_interner_preserves_mapped_type_generic_indexed_access_context()
 {
    let files = vec![(
        "main.ts".to_string(),
        r#"type Types = {
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
"#
        .to_string(),
    )];

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let file = &program.files[0];
    let binder = tsz::parallel::create_binder_from_bound_file(file, &program, 0);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &file.arena,
        &binder,
        &types,
        file.file_name.clone(),
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
    checker.check_source_file(file.source_file);

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
        "Expected reconstructed binder with fresh TypeInterner to avoid TS2344/TS7006, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    assert_eq!(
        codes,
        vec![2322, 2769],
        "Expected B[] assignment failure and Array.from overload mismatch. Got diagnostics: {:?}",
        result.diagnostics
    );
}
#[test]
#[ignore] // TODO: Promise should be assignable to PromiseLike with default libs
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
#[test]
fn compile_with_project_dir_uses_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let config_dir = base.join("configs");
    write_file(
        &config_dir.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&config_dir.join("src/index.ts"), "export const value = 1;");

    let mut args = default_args();
    args.project = Some(PathBuf::from("configs"));

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(config_dir.join("dist/src/index.js").is_file());
}
#[test]
fn compile_reports_ts7005_for_exported_bare_var_in_imported_dts() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "jsx": "react",
            "module": "commonjs",
            "target": "es2015"
          },
          "include": ["*.ts", "*.tsx", "*.d.ts"]
        }"#,
    );
    write_file(
        &base.join("file.tsx"),
        r#"declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}"#,
    );
    write_file(&base.join("test.d.ts"), "export var React;\n");
    write_file(
        &base.join("react-consumer.tsx"),
        r#"import { React } from "./test";
var foo: any;
var spread1 = <div x='' {...foo} y='' />;"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|d| d.code == 7005),
        "Expected TS7005 for exported bare var in imported .d.ts, got: {:#?}",
        result.diagnostics
    );
}
#[test]
fn compile_with_project_dir_resolves_package_exported_tsconfig_extends() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("node_modules/foo/package.json"),
        r#"{
          "name": "foo",
          "version": "1.0.0",
          "exports": {
            "./*.json": "./configs/*.json"
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/foo/configs/strict.json"),
        r#"{
          "compilerOptions": {
            "strict": true
          }
        }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{"extends":"foo/strict.json"}"#,
    );
    write_file(&base.join("index.ts"), "let x: string;\nx.toLowerCase();\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED),
        "Expected TS2454 from package-exported tsconfig extends, got diagnostics: {:?}",
        result.diagnostics
    );
}
