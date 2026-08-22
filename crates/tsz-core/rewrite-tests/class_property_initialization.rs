use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;
use tsz::config::{ProjectRequest, ProjectSelection, resolve_project};
use tsz::host::SystemHost;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{ClassMemberKind, PropertyNameKind, StatementKind, parse_source};
use tsz::{Compiler, CompilerOptions, SourceInput};

fn compile(source: &str, options: CompilerOptions) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(
            "property-initialization.ts",
            Arc::<str>::from(source),
        )],
        &options,
    )
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn ts2564_names(output: &tsz::CompileOutput) -> Vec<&str> {
    let source = output.program.files[0].source.text.as_ref();
    output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2564)
        .map(|diagnostic| {
            let start = diagnostic.start as usize;
            &source[start..start + diagnostic.length as usize]
        })
        .collect()
}

#[test]
fn checked_modes_report_exact_identifier_spans_and_no_check_suppresses() {
    let source = concat!(
        "class Crate<T> {\n",
        "  title: string;\n",
        "  count: number;\n",
        "  payload: T;\n",
        "}\n",
    );
    for no_emit in [false, true] {
        let checked = compile(
            source,
            CompilerOptions {
                no_emit,
                ..CompilerOptions::default()
            },
        );
        assert_eq!(codes(&checked), vec![2564, 2564, 2564]);
        assert_eq!(ts2564_names(&checked), vec!["title", "count", "payload"]);
        for diagnostic in &checked.diagnostics {
            let start = diagnostic.start as usize;
            let name = &source[start..start + diagnostic.length as usize];
            assert_eq!(
                diagnostic.message_text,
                format!(
                    "Property '{name}' has no initializer and is not definitely assigned in the constructor."
                )
            );
        }

        let unchecked = compile(
            source,
            CompilerOptions {
                no_check: true,
                no_emit,
                ..CompilerOptions::default()
            },
        );
        assert!(unchecked.diagnostics.is_empty(), "{unchecked:?}");
    }
}

#[test]
fn strict_option_inheritance_matches_typescript() {
    let source = "class Crate { value: string; }";
    for (name, strict, strict_null_checks, property_initialization, expected) in [
        ("default", true, None, None, vec![2564]),
        ("strict-off", false, None, None, vec![]),
        ("property-off", true, None, Some(false), vec![]),
        ("null-off-inherited", true, Some(false), None, vec![]),
        ("individual-on", false, Some(true), Some(true), vec![2564]),
        ("invalid-pair", false, None, Some(true), vec![5052]),
    ] {
        let output = compile(
            source,
            CompilerOptions {
                strict,
                strict_null_checks,
                strict_property_initialization: property_initialization,
                ..CompilerOptions::default()
            },
        );
        assert_eq!(codes(&output), expected, "{name}: {output:?}");
    }

    let unchecked_invalid = compile(
        source,
        CompilerOptions {
            strict: false,
            strict_property_initialization: Some(true),
            no_check: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&unchecked_invalid), vec![5052]);
}

#[test]
fn property_name_kinds_are_preserved_and_only_identifiers_are_owned() {
    let source = concat!(
        "class Crate {\n",
        "  ordinary: string;\n",
        "  #private: string;\n",
        "  'quoted': string;\n",
        "  0: string;\n",
        "}\n",
    );
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("names.ts"),
        Arc::<str>::from(source),
    ));
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("expected one class");
    };
    let StatementKind::Class(declaration) = &statement.kind else {
        panic!("expected class declaration");
    };
    assert_eq!(
        declaration
            .members
            .iter()
            .map(|member| member.name_kind)
            .collect::<Vec<_>>(),
        vec![
            PropertyNameKind::Identifier,
            PropertyNameKind::PrivateIdentifier,
            PropertyNameKind::StringLiteral,
            PropertyNameKind::NumericLiteral,
        ]
    );
    assert!(
        declaration
            .members
            .iter()
            .all(|member| matches!(member.kind, ClassMemberKind::Property { .. }))
    );

    let output = compile(source, CompilerOptions::default());
    assert_eq!(codes(&output), vec![2564]);
    assert_eq!(ts2564_names(&output), vec!["ordinary"]);
}

#[test]
fn field_shape_and_type_requirements_fail_closed() {
    let source = concat!(
        "abstract class Crate {\n",
        "  required: string;\n",
        "  initialized: string = '';\n",
        "  optional?: string;\n",
        "  definite!: string;\n",
        "  static shared: string;\n",
        "  abstract abstracted: string;\n",
        "  declare declared: string;\n",
        "  anyValue: any;\n",
        "  unknownValue: unknown;\n",
        "  undefinedValue: undefined;\n",
        "  maybe: string | undefined;\n",
        "  nothing: void;\n",
        "  voidUnion: string | void;\n",
        "}\n",
    );
    let output = compile(source, CompilerOptions::default());
    assert_eq!(codes(&output), vec![2564, 2564, 2564]);
    assert_eq!(
        ts2564_names(&output),
        vec!["required", "nothing", "voidUnion"]
    );
}

#[test]
fn references_aliases_arrays_and_type_parameters_require_initialization() {
    let source = concat!(
        "interface Shape { value: string }\n",
        "type Alias = string;\n",
        "class Crate<T> {\n",
        "  shape: Shape;\n",
        "  alias: Alias;\n",
        "  items: string[];\n",
        "  generic: T;\n",
        "  callable: () => void;\n",
        "}\n",
    );
    let output = compile(source, CompilerOptions::default());
    assert_eq!(codes(&output), vec![2564; 5], "{output:?}");
    assert_eq!(
        ts2564_names(&output),
        vec!["shape", "alias", "items", "generic", "callable"]
    );
}

#[test]
fn recursive_class_reference_unions_do_not_force_object_interiors() {
    let source = concat!(
        "class Module { members: Class[]; }\n",
        "class Namespace { members: (Class | Property)[]; }\n",
        "class Class { parent: Namespace; }\n",
        "class Property { parent: Module | Class; }\n",
    );
    let output = compile(source, CompilerOptions::default());
    assert_eq!(codes(&output), vec![2564; 4], "{output:?}");
    assert_eq!(
        ts2564_names(&output),
        vec!["members", "members", "parent", "parent"]
    );
}

#[test]
fn opaque_standard_library_aliases_never_become_required_by_name() {
    let output = compile(
        "class Crate { value: Awaited<undefined>; }",
        CompilerOptions::default(),
    );
    assert!(!codes(&output).contains(&2564), "{output:?}");
}

#[test]
fn any_constructor_member_keeps_assignment_flow_outside_this_atom() {
    for source in [
        "class Crate { value: string; constructor() {} }",
        "class Crate { value: string; constructor(value: string); }",
    ] {
        let output = compile(source, CompilerOptions::default());
        assert!(!codes(&output).contains(&2564), "{source}: {output:?}");
    }
}

#[test]
fn parser_recovered_class_members_do_not_assert_initialization_facts() {
    let output = compile(
        concat!(
            "class Stable { owned: string; }\n",
            "class Recovered { speculative: string; class Nested {} }\n",
        ),
        CompilerOptions::default(),
    );
    assert_eq!(ts2564_names(&output), vec!["owned"], "{output:?}");
}

#[test]
fn strict_property_option_error_uses_config_key_coordinates() {
    let fixture = TempDir::new().expect("tempdir");
    let config = concat!(
        "{\n",
        "  \"compilerOptions\": {\n",
        "    \"strict\": false,\n",
        "    \"strictPropertyInitialization\": true,\n",
        "    \"noCheck\": true\n",
        "  },\n",
        "  \"files\": [\"entry.ts\"]\n",
        "}\n",
    );
    std::fs::write(fixture.path().join("tsconfig.json"), config).expect("write config");
    std::fs::write(
        fixture.path().join("entry.ts"),
        "class Crate { value: string; }\n",
    )
    .expect("write source");
    let host = SystemHost::new(fixture.path());
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(fixture.path().to_path_buf())),
    );
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert_eq!(codes(&output), vec![5052]);
    let diagnostic = &output.diagnostics[0];
    assert_eq!(diagnostic.file, "tsconfig.json");
    assert_eq!(
        diagnostic.start,
        config
            .find("\"strictPropertyInitialization\"")
            .expect("property option key") as u32
    );
    assert_eq!(diagnostic.length, 30);
    assert_eq!(
        diagnostic.message_text,
        "Option 'strictPropertyInitialization' cannot be specified without specifying option 'strictNullChecks'."
    );
}
