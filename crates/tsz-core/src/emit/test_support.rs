use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bind::bind_source;
use crate::config::ProjectProvenance;
use crate::emit_paths::EmitPlan;
use crate::program::{
    CapabilityAnalysis, CapabilityContext, CompilerOptions, EmittedFile, ProgramFile,
};
use crate::source::{FileId, SourceText};
use crate::syntax::parse_source;

use super::emit_file_with_plan;

pub(super) fn emit_file(file: &ProgramFile, options: &CompilerOptions) -> Vec<EmittedFile> {
    let capabilities = CapabilityAnalysis::derive(
        std::slice::from_ref(file),
        options,
        CapabilityContext::default(),
    );
    let plan = EmitPlan::for_program(
        std::slice::from_ref(file),
        options,
        &ProjectProvenance::default(),
        &capabilities,
    );
    emit_file_with_plan(file, options, plan.for_file(file.source.id))
}

fn program_file(path: &str, text: &str) -> ProgramFile {
    let source = SourceText::new(FileId(0), PathBuf::from(path), Arc::<str>::from(text));
    let parsed = parse_source(&source);
    assert!(
        parsed.diagnostics.is_empty(),
        "test source must parse without diagnostics: {:?}",
        parsed.diagnostics
    );
    let bindings = bind_source(source.id, &parsed.unit);
    ProgramFile {
        source,
        syntax: parsed.unit,
        bindings,
    }
}

#[test]
fn erases_type_only_syntax_and_annotations() {
    let file = program_file(
        "input.ts",
        concat!(
            "interface Point { x: number; }\n",
            "type Scalar = number;\n",
            "const point: Point = { x: 1 };\n",
            "function add(a: number, b: number): number { return a + b; }\n",
        ),
    );
    let options = CompilerOptions {
        target: "esnext".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };

    let output = emit_file(&file, &options);
    assert_eq!(output.len(), 1);
    assert_eq!(
        output[0].text,
        concat!(
            "\"use strict\";\n",
            "const point = { x: 1 };\n",
            "function add(a, b) {\n",
            "    return a + b;\n",
            "}\n",
        )
    );
}

#[test]
fn emits_written_declaration_shapes_without_checking() {
    let file = program_file(
        "src/api.ts",
        concat!(
            "export const greeting: string = \"hello\";\n",
            "export interface Box<T> { readonly value?: T; }\n",
            "export function id<T>(value: T): T { return value; }\n",
        ),
    );
    let options = CompilerOptions {
        declaration: true,
        target: "esnext".to_string(),
        module: "esnext".to_string(),
        out_dir: Some(PathBuf::from("dist")),
        declaration_dir: Some(PathBuf::from("types")),
        ..CompilerOptions::default()
    };

    let output = emit_file(&file, &options);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0].path, Path::new("dist/api.js"));
    assert_eq!(output[1].path, Path::new("types/api.d.ts"));
    assert_eq!(
        output[0].text,
        concat!(
            "export const greeting = \"hello\";\n",
            "export function id(value) {\n",
            "    return value;\n",
            "}\n",
        )
    );
    assert_eq!(
        output[1].text,
        concat!(
            "export declare const greeting: string;\n",
            "export interface Box<T> {\n",
            "    readonly value?: T;\n",
            "}\n",
            "export declare function id<T>(value: T): T;\n",
        )
    );
}

#[test]
fn preserves_es_modules_at_the_ts7_defaults() {
    let file = program_file("value.ts", "export const value: number = 1;\n");
    let output = emit_file(&file, &CompilerOptions::default());
    assert_eq!(output[0].text, "export const value = 1;\n");
}

#[test]
fn keeps_expression_grouping_while_erasing_assertions() {
    let file = program_file(
        "math.ts",
        "const result: number = (1 + 2) * (3 as number);\n",
    );
    let options = CompilerOptions {
        target: "esnext".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        "\"use strict\";\nconst result = (1 + 2) * 3;\n"
    );
}

#[test]
fn derives_module_extension_outputs_without_losing_module_identity() {
    let file = program_file("src/value.mts", "export const value: number = 1;\n");
    let options = CompilerOptions {
        declaration: true,
        target: "esnext".to_string(),
        module: "nodenext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(output[0].path, Path::new("src/value.mjs"));
    assert_eq!(output[1].path, Path::new("src/value.d.mts"));
    assert_eq!(output[0].text, "export const value = 1;\n");
    assert_eq!(output[1].text, "export declare const value: number;\n");
}

#[test]
fn emits_modern_module_classes_from_structured_nodes() {
    let file = program_file(
        "src/service.ts",
        concat!(
            "import { token } from \"./token\";\n",
            "export class Service extends Base {\n",
            "  value: number = token;\n",
            "  constructor(value: number) { this.value = value; }\n",
            "  static create(value: number): Service { return new Service(value); }\n",
            "}\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2022".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "import { token } from \"./token\";\n",
            "export class Service extends Base {\n",
            "    value = token;\n",
            "    constructor(value) {\n",
            "        this.value = value;\n",
            "    }\n",
            "    static create(value) {\n",
            "        return new Service(value);\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn erases_class_overload_signatures_but_keeps_empty_implementations() {
    let file = program_file(
        "service.ts",
        concat!(
            "class Service {\n",
            "  constructor(value: string);\n",
            "  constructor() {}\n",
            "  method(value: string): void;\n",
            "  method() {}\n",
            "}\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2022".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "\"use strict\";\n",
            "class Service {\n",
            "    constructor() { }\n",
            "    method() { }\n",
            "}\n",
        )
    );
}

#[test]
fn rewrites_mixed_esm_clauses_and_assertions_from_structured_nodes() {
    let file = program_file(
        "module.ts",
        concat!(
            "import Default, { type Shape, live as renamed } from \"./dep\";\n",
            "export { type Shape, renamed as exposed } from \"./dep\";\n",
            "const value = [Default, renamed] as unknown;\n",
            "export default (value as unknown);\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2022".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "import Default, { live as renamed } from \"./dep\";\n",
            "export { renamed as exposed } from \"./dep\";\n",
            "const value = [Default, renamed];\n",
            "export default value;\n",
        )
    );
}

#[test]
fn emits_supported_class_fields_and_preserves_private_names() {
    let file = program_file(
        "model.ts",
        concat!(
            "class Model {\n",
            "  #secret = 1;\n",
            "  visible: number;\n",
            "}\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2022".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "\"use strict\";\n",
            "class Model {\n",
            "    #secret = 1;\n",
            "    visible;\n",
            "}\n",
        )
    );
}
