#[test]
fn compile_contextually_typed_jsx_children2_include_project_has_no_ts2739() {
    let Some(mut source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/contextuallyTypedJsxChildren2.tsx",
    ) else {
        return;
    };
    let Some(react16) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    source = source.replace("\"/.lib/react16.d.ts\"", "\"./.lib/react16.d.ts\"");

    write_file(&base.join("test.tsx"), &source);
    write_file(&base.join(".lib/react16.d.ts"), &react16);
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": true,
            "jsx": "react",
            "esModuleInterop": true,
            "noEmit": true,
            "skipLibCheck": true
          },
          "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
          "exclude": ["node_modules"]
        }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts2739: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
        })
        .collect();

    assert!(
        ts2739.is_empty(),
        "Expected include-glob react16 JSX children fixture to avoid TS2739, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_react16_automatic_jsx_intrinsics_keep_children_and_img_src() {
    let Some(react16) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join(".lib/react16.d.ts"), &react16);
    write_file(
        &base.join("one.tsx"),
        r#"/// <reference path="./.lib/react16.d.ts" />
/* @jsxRuntime classic */
import * as React from "react";
export const first = <img src="./image.png" />;
"#,
    );
    write_file(
        &base.join("two.tsx"),
        r#"/// <reference path="./.lib/react16.d.ts" />
/* @jsxRuntime automatic */
const props = { answer: 42 };
const a = <div key="foo" {...props}>text</div>;
const b = <img src="./image.png" />;

export { a, b };
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"export * as one from "./one.js";
export * as two from "./two.js";
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::ReactJsx);
    args.module = Some(crate::args::Module::CommonJs);
    args.no_emit = true;
    args.files = vec![
        PathBuf::from("one.tsx"),
        PathBuf::from("two.tsx"),
        PathBuf::from("index.ts"),
    ];

    let result = compile(&args, base).expect("compile should succeed");
    let relevant: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || d.code
                    == diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU
        })
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected real react16 automatic JSX intrinsics to accept text children and img src, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_jsx_call_elaboration_check_no_crash1_react16_fixture_reports_ts2322() {
    let Some(mut source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/jsxCallElaborationCheckNoCrash1.tsx",
    ) else {
        return;
    };
    let Some(react16) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    source = source.replace("\"/.lib/react16.d.ts\"", "\"./.lib/react16.d.ts\"");

    write_file(&base.join("test.tsx"), &source);
    write_file(&base.join(".lib/react16.d.ts"), &react16);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::React);
    args.es_module_interop = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.tsx")];

    let result = compile(&args, base).expect("compile should succeed");
    let jsx_ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text
                    .contains("LibraryManagedAttributes<Tag, DetailedHTMLProps")
        })
        .collect();

    assert!(
        !jsx_ts2322.is_empty(),
        "Expected real react16 generic intrinsic JSX fixture to report TS2322, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_generic_call_at_yield_expression_in_generic_call_fixture_reports_outer_ts2345() {
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/genericCallAtYieldExpressionInGenericCall1.ts",
    ) else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.ts"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts2345: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    let ts2488: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR)
        .collect();

    assert_eq!(
        ts2345.len(),
        2,
        "Expected fixture to report the two outer TS2345 callback mismatches, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
    assert_eq!(
        ts2488.len(),
        1,
        "Expected fixture to keep the single inner TS2488, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
    assert!(
        ts2345
            .iter()
            .all(|diag| diag.message_text.contains("Generator<number, void, any>")),
        "Expected outer TS2345 diagnostics to preserve the unannotated generator surface `Generator<number, void, any>`, got diagnostics: {ts2345:?}",
    );
    assert!(
        ts2488[0].message_text.contains("Type '() => T'"),
        "Expected inner TS2488 diagnostic to preserve the non-generic function surface `() => T`, got: {:?}",
        ts2488[0]
    );
}

#[test]
fn compile_generic_call_at_yield_expression_in_generic_call2_fixture_has_no_ts2345() {
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/genericCallAtYieldExpressionInGenericCall2.ts",
    ) else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.ts"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts2345: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();

    assert!(
        ts2345.is_empty(),
        "Expected fixture to avoid stale TS2345 diagnostics, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_return_type_inference_contextual_parameter_types_in_generator_fixture_has_no_errors() {
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/returnTypeInferenceContextualParameterTypesInGenerator1.ts",
    ) else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.ts"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected generator contextual return fixture to have no diagnostics, got: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_excessive_stack_depth_flat_array_fixture_reports_normalized_jsx_key_target() {
    let Some(source) =
        load_typescript_fixture("TypeScript/tests/cases/compiler/excessiveStackDepthFlatArray.ts")
    else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.tsx"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::React);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.tsx")];

    let result = compile(&args, base).expect("compile should succeed");
    let jsx_key_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text
                    .contains("Type '{ key: string; }' is not assignable to type")
        })
        .collect();

    assert!(
        jsx_key_diags.iter().any(|diag| {
            diag.message_text.contains("HTMLAttributes<HTMLLIElement>")
                && !diag.message_text.contains("DetailedHTMLProps")
        }),
        "Expected JSX key TS2322 to target normalized HTMLAttributes<HTMLLIElement>, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}
