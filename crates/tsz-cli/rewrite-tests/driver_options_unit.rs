use std::ffi::OsString;
use std::path::PathBuf;

use super::{CompilerOptionPatch, parse_arguments};

#[test]
fn shared_schema_mutates_every_supported_option_outside_debug_assertions() {
    let arguments = [
        "--strict=false",
        "--strictNullChecks=true",
        "--strictPropertyInitialization=false",
        "--noImplicitAny=true",
        "--noUnusedLocals=false",
        "--noUnusedParameters=true",
        "--noLib=false",
        "--lib=es5, dom,,",
        "--noCheck=true",
        "--noEmit=false",
        "--noEmitOnError=true",
        "--declaration=false",
        "--declarationMap=true",
        "--sourceMap=false",
        "--inlineSourceMap=true",
        "--removeComments=false",
        "--allowJs=true",
        "--checkJs=false",
        "--target=es2022",
        "--module=preserve",
        "--rootDir=source",
        "--outDir=output",
        "--declarationDir=types",
    ]
    .map(OsString::from);

    let invocation = parse_arguments(&arguments).expect("all schema options parse");
    assert!(invocation.command_line_diagnostics.is_empty());
    assert_eq!(
        invocation.options,
        CompilerOptionPatch {
            strict: Some(false),
            strict_null_checks: Some(true),
            strict_property_initialization: Some(false),
            no_implicit_any: Some(true),
            no_unused_locals: Some(false),
            no_unused_parameters: Some(true),
            no_lib: Some(false),
            lib: Some(vec!["es5".into(), "dom".into()]),
            no_check: Some(true),
            no_emit: Some(false),
            no_emit_on_error: Some(true),
            declaration: Some(false),
            declaration_map: Some(true),
            source_map: Some(false),
            inline_source_map: Some(true),
            remove_comments: Some(false),
            use_define_for_class_fields: None,
            allow_js: Some(true),
            check_js: Some(false),
            target: Some("es2022".into()),
            module: Some("preserve".into()),
            root_dir: Some(PathBuf::from("source")),
            out_dir: Some(PathBuf::from("output")),
            declaration_dir: Some(PathBuf::from("types")),
            ..CompilerOptionPatch::default()
        }
    );
}
