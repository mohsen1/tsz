use std::sync::Arc;

use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn options() -> CompilerOptions {
    CompilerOptions {
        allow_js: true,
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    }
}

fn compile(path: &str, source: &str) -> tsz::CompileOutput {
    compile_files(&[(path, source)])
}

fn compile_files(files: &[(&str, &str)]) -> tsz::CompileOutput {
    compile_files_with_options(files, &options())
}

fn compile_files_with_options(
    files: &[(&str, &str)],
    compiler_options: &CompilerOptions,
) -> tsz::CompileOutput {
    Compiler::new().compile(
        files
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        compiler_options,
    )
}

fn assert_completion(output: &tsz::CompileOutput, expected: SemanticCompletion) {
    assert_eq!(
        output.semantic_completion, expected,
        "unexpected completion for diagnostics {:?}",
        output.diagnostics
    );
    assert_eq!(output.stats.semantic_completion, expected);
    if !expected.is_complete() {
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn cross_kind_value_peers_fail_closed_by_program_identity() {
    for source in [
        "function Joined():void; class Joined {}",
        "class Renamed {} function Renamed() {}",
        "function Colliding() {} const Colliding = 1;",
        "{ function Wrapped() {} class Wrapped {} }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    let left = ("left.ts", "function AcrossFiles() {}");
    let right = ("right.ts", "class AcrossFiles {}");
    for files in [[left, right], [right, left]] {
        assert_completion(&compile_files(&files), SemanticCompletion::Deferred);
    }

    let isolated = compile_files(&[
        ("left.ts", "export function LocalName() {}"),
        ("right.ts", "export class LocalName {}"),
    ]);
    assert_completion(&isolated, SemanticCompletion::Complete);
}

#[test]
fn repeated_class_value_hosts_defer_by_program_identity() {
    for source in [
        "class Duplicate {} class Duplicate {}",
        "{ class Nested {} class Nested {} }",
        "function wrapper() { class Renamed {} class Renamed {} }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    let first = ("a.ts", "class GlobalPair {}");
    let second = ("b.ts", "class GlobalPair {}");
    for files in [[first, second], [second, first]] {
        assert_completion(&compile_files(&files), SemanticCompletion::Deferred);
    }

    assert_completion(
        &compile("distinct.ts", "class First {} class Second {}"),
        SemanticCompletion::Complete,
    );
}

#[test]
fn multiple_function_implementations_defer_in_each_owned_scope() {
    for source in [
        "function Duplicate() {} function Duplicate() {}",
        "{ function Nested() {} function Nested() {} }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    let first = ("a.ts", "function GlobalPair() {}");
    let second = ("b.ts", "function GlobalPair() {}");
    for files in [[first, second], [second, first]] {
        assert_completion(&compile_files(&files), SemanticCompletion::Deferred);
    }

    let ordinary = compile(
        "ordinary.ts",
        "function Choose(value:string):string; function Choose(value:any):any {}",
    );
    assert!(
        ordinary.diagnostics.is_empty(),
        "{:?}",
        ordinary.diagnostics
    );
    assert_completion(&ordinary, SemanticCompletion::Complete);
}

#[test]
fn javascript_redeclaration_groups_are_modeled_only_for_functions() {
    for extension in ["js", "jsx", "mjs", "cjs"] {
        let path = format!("case.{extension}");
        for source in [
            "function Repeated() {} function Repeated() {}",
            "function wrapper() { function Nested() {} function Nested() {} }",
        ] {
            let output = compile(&path, source);
            assert!(
                output.diagnostics.is_empty(),
                "{path}: {:?}",
                output.diagnostics
            );
            assert_completion(&output, SemanticCompletion::Complete);
        }

        for source in [
            "function Joined() {} var Joined;",
            "var Reordered; function Reordered() {}",
            "function CalledAfter() {} var CalledAfter; CalledAfter();",
            "var CalledBefore; function CalledBefore() {} CalledBefore();",
            "function wrapper() { function NestedVar() {} var NestedVar; }",
            "function External() {} var External; export {};",
        ] {
            assert_completion(&compile(&path, source), SemanticCompletion::Deferred);
        }
    }

    for source in [
        "function Repeated() {} function Repeated() {}",
        "function Joined() {} var Joined;",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    for source in [
        "function Collision() {} let Collision;",
        "function Collision() {} const Collision = 1;",
        "function Collision() {} class Collision {}",
        "function InitializedAfter() {} var InitializedAfter = 1; InitializedAfter();",
        "var InitializedBefore = 1; function InitializedBefore() {} InitializedBefore();",
        "function TypedVar() {} var TypedVar:number;",
        "function TypedFunction(value:number) {} function TypedFunction(value:number) {}",
    ] {
        assert_completion(&compile("case.js", source), SemanticCompletion::Deferred);
    }

    let first_function = ("function.js", "function AcrossFiles() {}");
    let second_function = ("peer.js", "function AcrossFiles() {}");
    for files in [
        [first_function, second_function],
        [second_function, first_function],
    ] {
        let output = compile_files(&files);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_completion(&output, SemanticCompletion::Complete);
    }

    let function = ("function.js", "function AcrossVar() {}");
    let variable = ("peer.js", "var AcrossVar;");
    for files in [[function, variable], [variable, function]] {
        assert_completion(&compile_files(&files), SemanticCompletion::Deferred);
    }
}

#[test]
fn exported_javascript_redeclaration_groups_stay_unmodeled() {
    for extension in ["js", "mjs", "cjs"] {
        let path = format!("exported.{extension}");
        for source in [
            "export function Repeated() {} export function Repeated() {}",
            "export function Joined() {} export var Joined;",
            "export var Reordered; export function Reordered() {}",
            "export function Mixed() {} var Mixed;",
        ] {
            assert_completion(&compile(&path, source), SemanticCompletion::Deferred);
        }

        assert_completion(
            &compile(
                &format!("plain.{extension}"),
                "function Plain() {} function Plain() {}",
            ),
            SemanticCompletion::Complete,
        );
    }
}

#[test]
fn javascript_named_exports_do_not_promote_var_bearing_redeclarations() {
    for extension in ["js", "mjs", "cjs"] {
        let path = format!("named-export.{extension}");
        for source in [
            "function Direct() {} var Direct; export { Direct };",
            "function Aliased() {} var Aliased; export { Aliased as outward };",
            "function DeclarationType() {} var DeclarationType; export type { DeclarationType };",
            "function SpecifierType() {} var SpecifierType; export { type SpecifierType };",
            "function Remote() {} var Remote; export { Remote } from './dependency';",
            "function Local() {} var Local; const Other = 1; export { Other };",
            "function wrapper() { function Nested() {} var Nested; } const Nested = 1; export { Nested };",
            "function wrapper() { function Renamed() {} var Renamed; } const Renamed = 1; export { Renamed as outward };",
        ] {
            assert_completion(&compile(&path, source), SemanticCompletion::Deferred);
        }

        let functions = compile(
            &path,
            "function Functions() {} function Functions() {} export { Functions };",
        );
        assert!(
            functions.diagnostics.is_empty(),
            "{path}: {:?}",
            functions.diagnostics
        );
        assert_completion(&functions, SemanticCompletion::Complete);
    }

    for source in [
        "class DeclarationType {} export type { DeclarationType };",
        "class SpecifierType {} export { type SpecifierType };",
    ] {
        let output = compile("type-only.ts", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_completion(&output, SemanticCompletion::Complete);
    }
}

#[test]
fn multiple_bodyful_constructors_defer_without_a_bodyless_signature() {
    for source in [
        "class Vessel { constructor() {} constructor(value:number) {} }",
        "function wrapper() { class Nested { constructor() {} constructor(value:string) {} } }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    let single = compile("single.ts", "class Single { constructor(value:number) {} }");
    assert_completion(&single, SemanticCompletion::Complete);
}

#[test]
fn class_member_declaration_groups_fail_closed_by_semantic_slot() {
    for source in [
        "class Service { run() {} run(value:number) {} }",
        "class Renamed { deliver() {} deliver(value:string) {} }",
        "class Factory { static create() {} static create(value:number) {} }",
        "function wrapper() { class Nested { execute() {} execute(value:string) {} } }",
        "class DuplicateGet { get value() { return 1; } get value() { return 2; } }",
        "class DuplicateSet { set value(next:number) {} set value(next:number) {} }",
        "class DuplicateField { value:number; value:number; }",
        "class MethodAccessor { value() {} get value() { return 1; } }",
        "class MethodField { value() {} value:number; }",
        "class NumericMethods { 1() {} 1.0() {} }",
        "class RenamedNumericMethods { 2() {} 2.0() {} }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    // Distinct numeric spellings are conservatively unmodeled until class
    // member groups own canonical property-key identity.
    assert_completion(
        &compile(
            "distinct-numeric.ts",
            "class DistinctNumericMethods { 1() {} 2() {} }",
        ),
        SemanticCompletion::Deferred,
    );

    for source in [
        "class Separate { read() {} write() {} }",
        "class IdentifierFields { first:number; second:string; }",
        "class IdentifierFieldAndMethod { value:number; read() {} }",
        "class NumericAndIdentifier { 1() {} read() {} }",
        "class StaticAndInstance { connect() {} static connect() {} }",
        "function wrapper() { class Nested { first() {} second() {} } }",
        "class AccessorPair { get value() { return 1; } set value(next:number) {} }",
        "class StaticAndInstanceAccessor { get value() { return 1; } static get value() { return 2; } }",
        "class Ordinary { select(value:string):string; select(value:any):any {} }",
    ] {
        assert_completion(&compile("owned.ts", source), SemanticCompletion::Complete);
    }
}

#[test]
fn javascript_bodyless_overload_syntax_deferred_by_source_kind() {
    for extension in ["js", "jsx", "mjs", "cjs"] {
        let path = format!("overload.{extension}");
        let output = compile(&path, "function Select(value); function Select(value) {}");
        assert_completion(&output, SemanticCompletion::Deferred);
    }

    for source in [
        "class JavaScriptClass { method(value); method(value) {} }",
        "class JavaScriptConstructor { constructor(value); constructor(value) {} }",
    ] {
        assert_completion(&compile("class.js", source), SemanticCompletion::Deferred);
    }

    let ordinary = compile(
        "ordinary.js",
        "function implemented(value) { return value; } class Holder { constructor(value) {} }",
    );
    assert_completion(&ordinary, SemanticCompletion::Complete);
}

#[test]
fn javascript_type_declaration_hosts_defer_by_source_kind() {
    for extension in ["js", "jsx", "mjs", "cjs"] {
        let path = format!("type-host.{extension}");
        for source in [
            "type LocalAlias = string;",
            "export type PublicAlias = string;",
            "interface LocalShape { value:string; }",
            "export interface PublicShape { value:string; }",
        ] {
            assert_completion(&compile(&path, source), SemanticCompletion::Deferred);
        }
    }

    for extension in ["ts", "tsx"] {
        let path = format!("type-host.{extension}");
        for source in [
            "type LocalAlias = string;",
            "export type PublicAlias = string;",
            "interface LocalShape { value:string; }",
            "export interface PublicShape { value:string; }",
        ] {
            let output = compile(&path, source);
            assert!(
                output.diagnostics.is_empty(),
                "{path}: {:?}",
                output.diagnostics
            );
            assert_completion(&output, SemanticCompletion::Complete);
        }
    }

    assert_completion(
        &compile(
            "contextual.js",
            "let type = 1; let interface = 2; const sum = type + interface; sum;",
        ),
        SemanticCompletion::Complete,
    );
}

#[test]
fn javascript_type_only_import_hosts_defer_by_source_kind() {
    let dependency = ("dependency.ts", "export class Imported {}");
    for extension in ["js", "jsx", "mjs", "cjs"] {
        let path = format!("type-import.{extension}");
        for source in [
            "import type { Imported } from './dependency';",
            "import { type Imported } from './dependency';",
        ] {
            assert_completion(
                &compile_files(&[(path.as_str(), source), dependency]),
                SemanticCompletion::Deferred,
            );
        }

        let ordinary = compile_files(&[
            (path.as_str(), "import { Imported } from './dependency';"),
            dependency,
        ]);
        assert!(
            ordinary.diagnostics.is_empty(),
            "{path}: {:?}",
            ordinary.diagnostics
        );
        assert_completion(&ordinary, SemanticCompletion::Complete);
    }

    for extension in ["ts", "tsx"] {
        let path = format!("type-import.{extension}");
        for source in [
            "import type { Imported } from './dependency';",
            "import { type Imported } from './dependency';",
        ] {
            let output = compile_files(&[(path.as_str(), source), dependency]);
            assert!(
                output.diagnostics.is_empty(),
                "{path}: {:?}",
                output.diagnostics
            );
            assert_completion(&output, SemanticCompletion::Complete);
        }
    }
}

#[test]
fn opaque_namespace_module_and_ambient_global_hosts_defer_structurally() {
    for source in [
        "namespace Container { export function member():void; }",
        "module RenamedContainer { export function member():void; }",
        "declare module 'package-name' { function member():void; }",
        "declare global { function member():void; }",
        "global { function member():void; }",
        "global Renamed { function member():void; }",
        "global export { function member():void; }",
        "global\n{ function member():void; }",
        "global\nRenamed;",
        "global\nexport {};",
        "export namespace WrappedContainer { export const value = 1; }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    let contextual_identifiers = compile(
        "identifiers.ts",
        "module.value; namespace.value; global.value;",
    );
    assert_completion(&contextual_identifiers, SemanticCompletion::Complete);

    for source in [
        "module\nlet module = 1; namespace\nlet namespace = 2;",
        "module\n'package-name'; namespace\nRenamed;",
        "global\nlet value = 2;",
    ] {
        assert_completion(&compile("asi.ts", source), SemanticCompletion::Complete);
    }
}

#[test]
fn declaration_hosts_respect_program_owned_standard_library_identity() {
    let library_options = CompilerOptions {
        lib: Some(vec!["es2022".to_string()]),
        no_emit: true,
        strict: true,
        ..CompilerOptions::default()
    };
    for path in ["global.ts", "global.js"] {
        let global = compile_files_with_options(&[(path, "class Array {}")], &library_options);
        assert_completion(&global, SemanticCompletion::Deferred);
    }

    let module =
        compile_files_with_options(&[("module.ts", "export class Array {}")], &library_options);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    assert_completion(&module, SemanticCompletion::Complete);

    for path in ["global-function.ts", "global-function.js"] {
        let global = compile_files_with_options(&[(path, "function Array() {}")], &library_options);
        assert_completion(&global, SemanticCompletion::Deferred);
    }

    for path in ["module-function.ts", "module-function.js"] {
        let module =
            compile_files_with_options(&[(path, "export function Array() {}")], &library_options);
        assert!(
            module.diagnostics.is_empty(),
            "{path}: {:?}",
            module.diagnostics
        );
        assert_completion(&module, SemanticCompletion::Complete);
    }

    for path in ["bare.mjs", "bare.cjs"] {
        for source in ["class Array {}", "function Array() {}"] {
            let module = compile_files_with_options(&[(path, source)], &library_options);
            assert!(
                module.diagnostics.is_empty(),
                "{path}: {:?}",
                module.diagnostics
            );
            assert_completion(&module, SemanticCompletion::Complete);
        }
    }

    let mjs = ("left.mjs", "class PathScoped {}");
    let cjs = ("right.cjs", "class PathScoped {}");
    for files in [[mjs, cjs], [cjs, mjs]] {
        let modules = compile_files(&files);
        assert!(modules.diagnostics.is_empty(), "{:?}", modules.diagnostics);
        assert_completion(&modules, SemanticCompletion::Complete);
    }
}

#[test]
fn unmodeled_declaration_hosts_block_every_emit_product() {
    for (host, path, source) in [
        (
            "namespace",
            "host.ts",
            "namespace Hidden { export const value = 1; } export const runtime = 1;",
        ),
        (
            "module",
            "host.ts",
            "module Hidden { export const value = 1; } export const runtime = 1;",
        ),
        (
            "global",
            "host.ts",
            "global { const value = 1; } export const runtime = 1;",
        ),
        (
            "javascript-export-type-clause",
            "host.js",
            "class Clause {} export type { Clause };",
        ),
        (
            "javascript-type-only-specifier",
            "host.js",
            "class Specifier {} export { type Specifier };",
        ),
        (
            "javascript-import-type-clause",
            "host.js",
            "import type { Imported } from './dependency';",
        ),
        (
            "javascript-type-only-import-specifier",
            "host.js",
            "import { type Imported } from './dependency';",
        ),
        (
            "javascript-local-type-alias",
            "host.js",
            "type LocalRecord = string; const runtime = 1;",
        ),
        (
            "javascript-exported-type-alias",
            "host.js",
            "export type PublicRecord = string; export const runtime = 1;",
        ),
        (
            "javascript-local-interface",
            "host.js",
            "interface LocalShape { value:string; } const runtime = 1;",
        ),
        (
            "javascript-exported-interface",
            "host.js",
            "export interface PublicShape { value:string; } export const runtime = 1;",
        ),
    ] {
        for module in ["commonjs", "esnext"] {
            for no_check in [false, true] {
                let output = Compiler::new().compile(
                    vec![SourceInput::new(path, Arc::<str>::from(source))],
                    &CompilerOptions {
                        allow_js: true,
                        declaration: true,
                        no_check,
                        module: module.to_string(),
                        target: "esnext".to_string(),
                        ..CompilerOptions::default()
                    },
                );
                assert_completion(&output, SemanticCompletion::Deferred);
                assert!(
                    output.emitted_files.is_empty(),
                    "{host}/{module}/{no_check}: {:?}",
                    output.emitted_files
                );
            }
        }
    }
}

#[test]
fn javascript_module_roots_block_every_emit_product() {
    for extension in ["mjs", "cjs"] {
        let path = format!("implicit.{extension}");
        for (shape, source) in [
            ("bare", "const runtime = 1;"),
            ("import", "import { value } from './dependency'; value;"),
            ("export", "export const runtime = 1;"),
        ] {
            for module in ["preserve", "esnext", "commonjs"] {
                for no_check in [false, true] {
                    let output = Compiler::new().compile(
                        vec![SourceInput::new(&path, Arc::<str>::from(source))],
                        &CompilerOptions {
                            allow_js: true,
                            declaration: true,
                            no_check,
                            module: module.to_string(),
                            target: "esnext".to_string(),
                            ..CompilerOptions::default()
                        },
                    );
                    assert_completion(&output, SemanticCompletion::Deferred);
                    assert!(
                        output.emitted_files.is_empty(),
                        "{extension}/{shape}/{module}/{no_check}: {:?}",
                        output.emitted_files
                    );
                }
            }
        }
    }
}

#[test]
fn ambient_implementation_gates_preserve_owned_declarations() {
    for source in [
        "declare function implemented():void {}",
        "declare class Implemented { method():void {} }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    for (path, source) in [
        ("bare.d.ts", "function missingDeclare():void;"),
        (
            "bare-class.d.ts",
            "class MissingDeclare { constructor(); method():void; }",
        ),
        ("body.d.ts", "class AmbientBody { method():void {} }"),
    ] {
        assert_completion(&compile(path, source), SemanticCompletion::Deferred);
    }

    for (path, source) in [
        ("declared.ts", "declare function owned():void;"),
        ("exported.d.ts", "export function owned():void;"),
        (
            "ambient.d.ts",
            "declare class Ambient { constructor(); method():void; }",
        ),
        (
            "exported-class.d.ts",
            "export class Exported { constructor(); method():void; }",
        ),
    ] {
        let output = compile(path, source);
        assert!(
            output.diagnostics.is_empty(),
            "{path}: {:?}",
            output.diagnostics
        );
        assert_completion(&output, SemanticCompletion::Complete);
    }
}
