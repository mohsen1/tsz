use std::sync::Arc;

use tsz::bind::Meaning;
use tsz::{Compiler, CompilerOptions, SourceInput};

fn compile(source: &str, options: CompilerOptions) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            ..options
        },
    )
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_missing_essential_globals(output: &tsz::CompileOutput) {
    let expected_names = [
        "Array",
        "Boolean",
        "CallableFunction",
        "Function",
        "IArguments",
        "NewableFunction",
        "Number",
        "Object",
        "RegExp",
        "String",
    ];
    assert_eq!(codes(output), vec![2318; expected_names.len()]);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.clone(),
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.clone(),
            ))
            .collect::<Vec<_>>(),
        expected_names
            .iter()
            .map(|name| (
                String::new(),
                0,
                0,
                format!("Cannot find global type '{name}'.")
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn default_library_contributes_generated_type_and_value_symbols() {
    // When a selected pinned TS7 library declares a global, the program owns
    // one ambient declaration identity and name lookup uses that identity.
    let output = compile(
        "let table: Record<string, number>; parseInt; console;",
        CompilerOptions::default(),
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);

    for (name, meaning) in [
        ("Record", Meaning::Type),
        ("parseInt", Meaning::Value),
        ("console", Meaning::Value),
    ] {
        let declaration = output.program.resolve_global(name, meaning).unwrap();
        let declaration = output
            .program
            .standard_library_declaration(declaration)
            .unwrap();
        assert_eq!(declaration.name, name);
        assert_eq!(declaration.meaning, meaning);
    }
}

#[test]
fn opaque_library_shapes_do_not_become_definitive_unknown() {
    let output = compile(
        "const table: Record<string, number> = { one: 'wrong' };",
        CompilerOptions::default(),
    );
    assert_eq!(codes(&output), vec![2322]);
}

#[test]
fn no_lib_contributes_no_ambient_symbols() {
    let output = compile(
        "const table: Record<string, number> = { one: 1 }; parseInt;",
        CompilerOptions {
            no_lib: true,
            ..CompilerOptions::default()
        },
    );
    assert_missing_essential_globals(&output);
    assert!(output.program.standard_library.declarations().is_empty());
    assert!(
        output
            .program
            .resolve_global("Record", Meaning::Type)
            .is_none()
    );
    assert!(
        output
            .program
            .resolve_global("parseInt", Meaning::Value)
            .is_none()
    );
}

#[test]
fn authored_global_types_satisfy_the_essential_environment_gate() {
    let output = compile(
        concat!(
            "interface Array<T> {}\n",
            "interface Boolean {}\n",
            "interface CallableFunction {}\n",
            "interface Function {}\n",
            "interface IArguments {}\n",
            "interface NewableFunction {}\n",
            "interface Number {}\n",
            "interface Object {}\n",
            "interface RegExp {}\n",
            "interface String {}\n",
            "const value: number = 'wrong';\n",
        ),
        CompilerOptions {
            no_lib: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&output), vec![2322]);
}

#[test]
fn explicit_lib_replaces_the_default_full_library_set() {
    let output = compile(
        "const accepted = 1;",
        CompilerOptions {
            lib: Some(vec!["ES5".to_string()]),
            ..CompilerOptions::default()
        },
    );
    assert!(
        output
            .program
            .resolve_global("Record", Meaning::Type)
            .is_some()
    );
    assert!(
        output
            .program
            .resolve_global("parseInt", Meaning::Value)
            .is_some()
    );
    assert!(
        output
            .program
            .resolve_global("console", Meaning::Value)
            .is_none()
    );
    assert_eq!(
        output.program.standard_library.selected_libraries(),
        &["decorators", "decorators.legacy", "es5"]
    );
}

#[test]
fn target_selects_the_corresponding_default_full_library() {
    let es2025 = compile("const accepted = 1;", CompilerOptions::default());
    assert!(
        es2025
            .program
            .resolve_global("WeakRef", Meaning::Value)
            .is_some()
    );
    assert_eq!(
        es2025.program.standard_library.selected_libraries().last(),
        Some(&"es2025.full")
    );

    let es2021 = compile(
        "const accepted = 1;",
        CompilerOptions {
            target: "ES2021".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(
        es2021
            .program
            .resolve_global("WeakRef", Meaning::Value)
            .is_some()
    );
    assert_eq!(
        es2021.program.standard_library.selected_libraries().last(),
        Some(&"es2021.full")
    );
}

#[test]
fn removed_es5_target_is_a_fatal_option_diagnostic() {
    let output = compile(
        "const count: number = 'wrong';",
        CompilerOptions {
            target: "ES5".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&output), vec![5108]);
    assert_eq!(
        output.diagnostics[0].message_text,
        "Option 'target=ES5' has been removed. Please remove it from your configuration."
    );
    assert!(output.emitted_files.is_empty());
}

#[test]
fn unknown_target_is_a_fatal_exact_option_diagnostic() {
    let output = compile(
        "const count: number = 'wrong';",
        CompilerOptions {
            target: "future".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&output), vec![6046]);
    assert_eq!(
        output.diagnostics[0].message_text,
        concat!(
            "Argument for '--target' option must be: 'es6', 'es2015', 'es2016', ",
            "'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
            "'es2023', 'es2024', 'es2025', 'esnext'."
        )
    );
    assert!(output.emitted_files.is_empty());
}

#[test]
fn explicit_feature_lib_uses_its_reference_closure_only() {
    let output = compile(
        "Promise; const table: Record<string, number> = { one: 1 };",
        CompilerOptions {
            lib: Some(vec!["es2015.promise".to_string()]),
            ..CompilerOptions::default()
        },
    );
    assert_missing_essential_globals(&output);
    assert!(
        output
            .program
            .resolve_global("Promise", Meaning::Value)
            .is_some()
    );
    assert!(
        output
            .program
            .resolve_global("Record", Meaning::Type)
            .is_none()
    );
    assert_eq!(
        output.program.standard_library.selected_libraries(),
        &["es2015.promise"]
    );
}

#[test]
fn declaration_identity_order_is_independent_of_explicit_root_order() {
    let declarations = |libraries: &[&str]| {
        let output = compile(
            "const accepted = 1;",
            CompilerOptions {
                lib: Some(libraries.iter().map(|name| (*name).to_string()).collect()),
                ..CompilerOptions::default()
            },
        );
        output
            .program
            .standard_library
            .declarations()
            .iter()
            .map(|declaration| {
                (
                    declaration.id.local,
                    declaration.name.clone(),
                    declaration.meaning,
                )
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(
        declarations(&["dom", "es2015"]),
        declarations(&["es2015", "dom"])
    );
}
