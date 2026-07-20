//! Driver-level regressions for cross-file class declaration ownership.
//!
//! `NodeIndex` is arena-relative. An imported class declaration can therefore
//! have the same raw index as an unrelated class in the importing file. These
//! tests check only the consumer file against a fully merged program so the
//! exporting class has not already published a cached type.

use super::{MergedProgram, ParallelCheckPlan, compile_files_with_libs};
use crate::checker::context::CheckerOptions;
use crate::parser::NodeIndex;

fn compile(files: &[(&str, &str)]) -> MergedProgram {
    compile_files_with_libs(
        files
            .iter()
            .map(|(name, source)| ((*name).to_owned(), (*source).to_owned()))
            .collect(),
        &[],
    )
}

fn exported_value_declaration(
    program: &MergedProgram,
    file_name: &str,
    export_name: &str,
) -> NodeIndex {
    let symbol_id = program
        .module_exports
        .get(file_name)
        .and_then(|exports| exports.get(export_name))
        .unwrap_or_else(|| panic!("missing export {export_name:?} from {file_name:?}"));
    program
        .symbols
        .get(symbol_id)
        .unwrap_or_else(|| panic!("missing merged symbol for {export_name:?}"))
        .value_declaration
}

fn check_consumer_cold(program: &MergedProgram, file_name: &str) -> Vec<u32> {
    let file_idx = program
        .files
        .iter()
        .position(|file| file.file_name == file_name)
        .unwrap_or_else(|| panic!("missing consumer file {file_name:?}"));
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    let plan = ParallelCheckPlan::build(program, &options, &[]);
    plan.check_one_file(file_idx, &program.files[file_idx])
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn imported_class_ignores_same_index_local_class_when_checked_cold() {
    // The empty statement and otherwise-unused members intentionally align
    // `ImportedCompiler` with `EnclosingDialect` at the same raw `NodeIndex`.
    let program = compile(&[
        (
            "compiler.ts",
            r#"
export abstract class BaseCompiler { compileQuery(): void {} }
;
export class ImportedCompiler extends BaseCompiler {
    first(): void {}
    second(): void {}
    third(): void {}
    fourth(): void {}
    static {}
}
"#,
        ),
        (
            "dialect.ts",
            r#"
import { ImportedCompiler } from './compiler.js'
interface Compiler { compileQuery(): void }
export class EnclosingDialect {
    constructor(config: string) { void config }
    createCompiler(): Compiler { return new ImportedCompiler() }
}
"#,
        ),
    ]);

    let imported_declaration =
        exported_value_declaration(&program, "compiler.ts", "ImportedCompiler");
    let unrelated_local_declaration =
        exported_value_declaration(&program, "dialect.ts", "EnclosingDialect");
    assert_eq!(
        imported_declaration, unrelated_local_declaration,
        "the regression requires equal raw NodeIndex values from different arenas"
    );

    let codes = check_consumer_cold(&program, "dialect.ts");
    assert!(
        !codes.contains(&2741),
        "the imported class instance must retain its inherited compileQuery member: {codes:?}"
    );
    assert!(
        !codes.contains(&2554),
        "the unrelated local constructor must not become the imported constructor: {codes:?}"
    );
}

#[test]
fn imported_class_owner_is_name_and_declaration_position_independent() {
    let program = compile(&[
        (
            "engine.ts",
            r#"
export abstract class EngineBase { compileQuery(): void {} }
export class RenamedEngine extends EngineBase {
    first(): void {}
    second(): void {}
    third(): void {}
    fourth(): void {}
    static {}
}
"#,
        ),
        (
            "factory.ts",
            r#"
import { RenamedEngine } from './engine.js'
interface EngineContract { compileQuery(): void }
export class RenamedFactory {
    constructor(config: string) { void config }
    create(): EngineContract { return new RenamedEngine() }
}
"#,
        ),
    ]);

    let imported_declaration = exported_value_declaration(&program, "engine.ts", "RenamedEngine");
    let local_declaration = exported_value_declaration(&program, "factory.ts", "RenamedFactory");
    assert_ne!(
        imported_declaration, local_declaration,
        "this adjacent case must exercise different raw declaration positions"
    );

    let codes = check_consumer_cold(&program, "factory.ts");
    assert!(
        !codes.contains(&2741) && !codes.contains(&2554),
        "renaming and shifting declarations must preserve the imported class type: {codes:?}"
    );
}

#[test]
fn imported_required_constructor_still_reports_ts2554_when_checked_cold() {
    let program = compile(&[
        (
            "required.ts",
            r#"
export class RequiredEngine {
    constructor(config: string) { void config }
    compileQuery(): void {}
}
"#,
        ),
        (
            "consumer.ts",
            r#"
import { RequiredEngine } from './required.js'
interface EngineContract { compileQuery(): void }
export function create(): EngineContract { return new RequiredEngine() }
"#,
        ),
    ]);

    let codes = check_consumer_cold(&program, "consumer.ts");
    assert_eq!(
        codes.iter().filter(|&&code| code == 2554).count(),
        1,
        "a real required constructor must still report exactly one TS2554: {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "constructor checking must preserve the imported instance members: {codes:?}"
    );
}
