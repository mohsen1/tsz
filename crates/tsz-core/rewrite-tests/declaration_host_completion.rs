use std::sync::Arc;

use tsz::bind::{Meaning, ScopeId, bind_source_with_kind};
use tsz::diagnostics::DiagnosticCategory;
use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::parse_source;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn options() -> CompilerOptions {
    CompilerOptions {
        allow_js: true,
        check_js: Some(true),
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

fn diagnostic_rows(
    diagnostics: &[tsz::diagnostics::Diagnostic],
) -> Vec<(u32, u32, u32, DiagnosticCategory, &str)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
            )
        })
        .collect()
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
        "class EscapedMethods { x() {} '\\x78'(value:number) {} }",
        "class EscapedNumericMethods { 1() {} '\\u0031'(value:number) {} }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    // Binder-owned canonical property keys distinguish different numeric
    // symbols while merging equivalent spellings above.
    assert_completion(
        &compile(
            "distinct-numeric.ts",
            "class DistinctNumericMethods { 1() {} 2() {} }",
        ),
        SemanticCompletion::Complete,
    );

    for source in [
        "class Separate { read() {} write() {} }",
        "class IdentifierFields { first:number; second:string; }",
        "class IdentifierFieldAndMethod { value:number; read() {} }",
        "class NumericAndIdentifier { 1() {} read() {} }",
        "class StaticAndInstance { connect() {} static connect() {} }",
        "function wrapper() { class Nested { first() {} second() {} } }",
        "class Ordinary { select(value:string):string; select(value:any):any {} }",
        "class AccessorPair { get value():number { return 1; } set value(next:number) {} }",
        "class StaticAndInstanceAccessor { get value():number { return 1; } static get value():number { return 2; } }",
    ] {
        let output = compile("owned.ts", source);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "unexpected Deferred control: {source}",
        );
        assert_eq!(
            output.stats.semantic_completion,
            SemanticCompletion::Complete
        );
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

    // JavaScript type declarations have no opaque-host fact: the file-scoped
    // boundary itself must retain semantic nonclaims without affecting peers.
    let mut service = LanguageService::new(options());
    service.open(
        "type-host.js",
        Arc::<str>::from("type LocalAlias = string;"),
    );
    let sibling_source = "type Kept = MissingAcross;";
    service.open("sibling.ts", Arc::<str>::from(sibling_source));
    let host_syntax = service.syntactic_diagnostics("type-host.js");
    assert_eq!(
        host_syntax.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert!(host_syntax.diagnostics.is_empty());
    let host_semantic = service.semantic_diagnostics("type-host.js");
    assert_eq!(
        host_semantic.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(host_semantic.diagnostics.is_empty());
    let sibling_semantic = service.semantic_diagnostics("sibling.ts");
    assert_eq!(
        sibling_semantic.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        sibling_semantic
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [(
            "sibling.ts",
            2304,
            sibling_source.find("MissingAcross").unwrap() as u32,
            "MissingAcross".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingAcross'.",
            &[][..],
        )],
    );

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
        "export namespace WrappedContainer { export const value = 1; }",
    ] {
        assert_completion(&compile("case.ts", source), SemanticCompletion::Deferred);
    }

    for source in [
        "global { function member():void; }",
        "global Renamed { function member():void; }",
        "global\n{ function member():void; }",
        "global\nRenamed;",
        "global\nexport {};",
    ] {
        let output = compile("case.ts", source);
        assert!(output.diagnostics.is_empty(), "{source}");
        assert_completion(&output, SemanticCompletion::Deferred);
    }

    // This malformed modifier sequence has definitive parser diagnostics. The
    // host fact fences semantic and emit ownership, but does not make an owned
    // syntax result provisional.
    let malformed = "global export { function member():void; }";
    let output = compile("case.ts", malformed);
    assert_completion(&output, SemanticCompletion::Deferred);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                "case.ts",
                1005,
                25,
                6,
                DiagnosticCategory::Error,
                "'}' expected.",
                &[][..],
            ),
            (
                "case.ts",
                1109,
                33,
                1,
                DiagnosticCategory::Error,
                "Expression expected.",
                &[][..],
            ),
            (
                "case.ts",
                1109,
                38,
                1,
                DiagnosticCategory::Error,
                "Expression expected.",
                &[][..],
            ),
            (
                "case.ts",
                1109,
                40,
                1,
                DiagnosticCategory::Error,
                "Expression expected.",
                &[][..],
            ),
        ],
    );

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
fn bare_global_recovery_is_local_but_declared_global_augmentation_is_program_owned() {
    // Pinned TypeScript 7 reports TS2669 and TS2670 for this bare global host,
    // plus TS2304 in the independent peer. Until the global-host diagnostics
    // are ported, only the recovered host is nonclaimed; it must not hide the
    // peer's definitive diagnostic.
    let host = ("a-host.ts", "global { type Hidden = MissingInside; }");
    let peer_source = "type Kept = MissingAcross;";
    let peer = ("z-peer.ts", peer_source);
    for files in [[host, peer], [peer, host]] {
        let first = compile_files(&files);
        let second = compile_files(&files);
        for output in [&first, &second] {
            assert_completion(output, SemanticCompletion::Deferred);
            assert_eq!(
                output
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (
                        diagnostic.file.as_str(),
                        diagnostic.code,
                        diagnostic.start,
                        diagnostic.length,
                        diagnostic.category,
                        diagnostic.message_text.as_str(),
                    ))
                    .collect::<Vec<_>>(),
                [(
                    "z-peer.ts",
                    2304,
                    peer_source.find("MissingAcross").unwrap() as u32,
                    "MissingAcross".len() as u32,
                    DiagnosticCategory::Error,
                    "Cannot find name 'MissingAcross'.",
                )],
            );
        }
        assert_eq!(
            first.diagnostics, second.diagnostics,
            "bare-global recovery must be repeatable for root order {files:?}",
        );

        let mut service = LanguageService::new(options());
        for (path, source) in files {
            service.open(path, Arc::<str>::from(source));
        }
        for _ in 0..2 {
            let host_result = service.semantic_diagnostics("a-host.ts");
            assert_eq!(
                host_result.semantic_completion,
                SemanticCompletion::Deferred,
            );
            assert!(host_result.diagnostics.is_empty());

            let peer_result = service.semantic_diagnostics("z-peer.ts");
            assert_eq!(
                peer_result.semantic_completion,
                SemanticCompletion::Complete,
            );
            assert_eq!(
                diagnostic_rows(&peer_result.diagnostics),
                [(
                    2304,
                    peer_source.find("MissingAcross").unwrap() as u32,
                    "MissingAcross".len() as u32,
                    DiagnosticCategory::Error,
                    "Cannot find name 'MissingAcross'.",
                )],
            );
        }
    }

    // An identifier between `global` and the block is recovery, not a parsed
    // global-augmentation body. It has the same local containment contract.
    let malformed_host = (
        "a-host.ts",
        "global Renamed { type Hidden = MissingInside; }",
    );
    for files in [[malformed_host, peer], [peer, malformed_host]] {
        for _ in 0..2 {
            let output = compile_files(&files);
            assert_completion(&output, SemanticCompletion::Deferred);
            assert_eq!(
                diagnostic_rows(&output.diagnostics),
                [(
                    2304,
                    peer_source.find("MissingAcross").unwrap() as u32,
                    "MissingAcross".len() as u32,
                    DiagnosticCategory::Error,
                    "Cannot find name 'MissingAcross'.",
                )],
            );
        }

        let mut service = LanguageService::new(options());
        for (path, source) in files {
            service.open(path, Arc::<str>::from(source));
        }
        for _ in 0..2 {
            assert_eq!(
                service
                    .semantic_diagnostics("a-host.ts")
                    .semantic_completion,
                SemanticCompletion::Deferred,
            );
            let peer_result = service.semantic_diagnostics("z-peer.ts");
            assert_eq!(
                peer_result.semantic_completion,
                SemanticCompletion::Complete,
            );
            assert_eq!(peer_result.diagnostics.len(), 1);
            assert_eq!(peer_result.diagnostics[0].code, 2304);
        }
    }

    // A well-formed declaration is an actual global augmentation. Pinned
    // TypeScript makes GlobalContract visible to the consumer, so TSZ must
    // keep that dependent cross-file query nonclaimed until augmentation
    // checking is implemented.
    let augmentation = (
        "a-augmentation.ts",
        "export {}; declare global { interface GlobalContract { value:string; } }",
    );
    let consumer = (
        "z-consumer.ts",
        "const value: GlobalContract = { value: 'ok' };",
    );
    for files in [[augmentation, consumer], [consumer, augmentation]] {
        let first = compile_files(&files);
        let second = compile_files(&files);
        for output in [&first, &second] {
            assert_completion(output, SemanticCompletion::Deferred);
            assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        }
        assert_eq!(first.diagnostics, second.diagnostics);

        let mut service = LanguageService::new(options());
        for (path, source) in files {
            service.open(path, Arc::<str>::from(source));
        }
        for _ in 0..2 {
            assert_eq!(
                service
                    .semantic_diagnostics("z-consumer.ts")
                    .semantic_completion,
                SemanticCompletion::Deferred,
            );
        }
    }
}

#[test]
fn same_line_namespace_names_commit_to_declaration_recovery() {
    for (source, code, start, length, message) in [
        ("namespace N;", 1005, 11, 1, "'{' expected."),
        ("module Renamed;", 1005, 14, 1, "'{' expected."),
        ("namespace Outer.Inner;", 1005, 21, 1, "'{' expected."),
        ("export namespace Wrapped;", 1005, 24, 1, "'{' expected."),
        ("declare namespace Ambient;", 1005, 25, 1, "'{' expected."),
        ("namespace \"pkg\";", 1003, 10, 5, "Identifier expected."),
    ] {
        let mut service = LanguageService::new(options());
        service.open("host.ts", Arc::<str>::from(source));
        let syntax = service.syntactic_diagnostics("host.ts");
        assert_eq!(syntax.syntactic_completion, SemanticCompletion::Complete);
        assert_eq!(
            diagnostic_rows(&syntax.diagnostics),
            [(code, start, length, DiagnosticCategory::Error, message)],
            "{source}: {:#?}",
            syntax.diagnostics,
        );
        let semantic = service.semantic_diagnostics("host.ts");
        assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
        assert!(
            semantic.diagnostics.is_empty(),
            "the malformed declaration must not create ordinary-expression names: {source}: {:#?}",
            semantic.diagnostics,
        );

        let output = service.compile();
        assert_completion(&output, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete,);
        assert_eq!(
            diagnostic_rows(&output.diagnostics),
            [(code, start, length, DiagnosticCategory::Error, message)],
        );
    }
}

#[test]
fn declaration_host_recovery_keeps_siblings_and_phase_products_independent() {
    let source = "namespace Container; type Kept = MissingSibling;";
    let mut service = LanguageService::new(options());
    service.open("host.ts", Arc::<str>::from(source));

    let syntax = service.syntactic_diagnostics("host.ts");
    assert_eq!(syntax.syntactic_completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostic_rows(&syntax.diagnostics),
        [(1005, 19, 1, DiagnosticCategory::Error, "'{' expected.")],
    );

    let semantic = service.semantic_diagnostics("host.ts");
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostic_rows(&semantic.diagnostics),
        [(
            2304,
            33,
            14,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingSibling'."
        )],
    );

    let output = service.compile();
    assert_completion(&output, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostic_rows(&output.diagnostics),
        [(1005, 19, 1, DiagnosticCategory::Error, "'{' expected.")],
        "the compiler product publishes the first nonempty source diagnostic phase",
    );
}

#[test]
fn declaration_host_asi_and_body_boundaries_match_the_pinned_parser() {
    for source in ["namespace;", "namespace\nRenamed;"] {
        let mut service = LanguageService::new(options());
        service.open("asi.ts", Arc::<str>::from(source));
        let syntax = service.syntactic_diagnostics("asi.ts");
        assert_eq!(syntax.syntactic_completion, SemanticCompletion::Complete);
        assert!(syntax.diagnostics.is_empty(), "{source}");
        let semantic = service.semantic_diagnostics("asi.ts");
        assert_eq!(semantic.semantic_completion, SemanticCompletion::Complete);
        assert!(
            semantic
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == 2304),
            "{source}: {:#?}",
            semantic.diagnostics,
        );
    }

    let split_modifier = "declare\nnamespace Next;";
    let mut service = LanguageService::new(options());
    service.open("split.ts", Arc::<str>::from(split_modifier));
    let syntax = service.syntactic_diagnostics("split.ts");
    assert_eq!(syntax.syntactic_completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostic_rows(&syntax.diagnostics),
        [(1005, 22, 1, DiagnosticCategory::Error, "'{' expected.")],
    );

    let missing_close = "namespace Open { const value = 1;";
    let mut service = LanguageService::new(options());
    service.open("open.ts", Arc::<str>::from(missing_close));
    let syntax = service.syntactic_diagnostics("open.ts");
    assert_eq!(syntax.syntactic_completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostic_rows(&syntax.diagnostics),
        [(
            1005,
            missing_close.len() as u32,
            0,
            DiagnosticCategory::Error,
            "'}' expected.",
        )],
    );

    let mut ambient = LanguageService::new(options());
    ambient.open(
        "ambient.ts",
        Arc::<str>::from("declare module \"package-name\";"),
    );
    let syntax = ambient.syntactic_diagnostics("ambient.ts");
    assert_eq!(syntax.syntactic_completion, SemanticCompletion::Complete);
    assert!(syntax.diagnostics.is_empty());
    let semantic = ambient.semantic_diagnostics("ambient.ts");
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
    assert!(semantic.diagnostics.is_empty());
}

#[test]
fn opaque_enum_hosts_publish_both_authored_meanings_without_missing_name_diagnostics() {
    for (shape, source, missing) in [
        (
            "renamed-const",
            concat!(
                "const enum RenamedSignal { Ready }",
                "function useSignal(value:RenamedSignal){return RenamedSignal.Ready;}",
                "type Kept=MissingTop;",
            ),
            "MissingTop",
        ),
        (
            "nested-const",
            concat!(
                "function wrapper(){const enum NestedSignal { Ready }",
                "let value:NestedSignal;NestedSignal.Ready;",
                "type Kept=MissingNested;}",
            ),
            "MissingNested",
        ),
        (
            "ambient-const",
            concat!(
                "declare const enum AmbientSignal { Ready }",
                "declare let value:AmbientSignal;AmbientSignal.Ready;",
                "type Kept=MissingAmbient;",
            ),
            "MissingAmbient",
        ),
        (
            "ordinary",
            concat!(
                "enum OrdinarySignal { Ready }",
                "let value:OrdinarySignal;OrdinarySignal.Ready;",
                "type Kept=MissingOrdinary;",
            ),
            "MissingOrdinary",
        ),
    ] {
        let mut service = LanguageService::new(options());
        service.open("enum-host.ts", Arc::<str>::from(source));
        let syntax = service.syntactic_diagnostics("enum-host.ts");
        assert_eq!(syntax.syntactic_completion, SemanticCompletion::Deferred);
        assert!(
            syntax.diagnostics.is_empty(),
            "{shape}: {:#?}",
            syntax.diagnostics
        );
        let semantic = service.semantic_diagnostics("enum-host.ts");
        assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            semantic
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.file.as_str(),
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.category,
                    diagnostic.message_text.as_str(),
                    diagnostic.related_information.as_slice(),
                ))
                .collect::<Vec<_>>(),
            [(
                "enum-host.ts",
                2304,
                source.find(missing).expect("independent sibling") as u32,
                missing.len() as u32,
                DiagnosticCategory::Error,
                format!("Cannot find name '{missing}'.").as_str(),
                &[][..],
            )],
            "{shape}: {:#?}",
            semantic.diagnostics,
        );
        let output = service.compile();
        assert_completion(&output, SemanticCompletion::Deferred);
        assert_eq!(
            diagnostic_rows(&output.diagnostics),
            [(
                2304,
                source.find(missing).expect("independent sibling") as u32,
                missing.len() as u32,
                DiagnosticCategory::Error,
                format!("Cannot find name '{missing}'.").as_str(),
            )],
            "{shape}: {:#?}",
            output.diagnostics,
        );
    }

    let ordinary_const = compile(
        "ordinary-const.ts",
        "const enumValue = 1; const renamedEnumValue = enumValue;",
    );
    assert!(
        ordinary_const.diagnostics.is_empty(),
        "{:#?}",
        ordinary_const.diagnostics,
    );
    assert_completion(&ordinary_const, SemanticCompletion::Complete);
}

#[test]
fn modeled_declaration_hosts_keep_syntax_claim_while_enum_member_grammar_defers() {
    let enum_source = "enum Signal { Ready } type Kept = MissingSibling;";
    let namespace_source = "namespace Container { export const value = 1; }";
    let external_module_source =
        "declare module 'package-name' { function member():void; } type Kept = MissingModule;";
    for (path, source) in [
        ("b-namespace.ts", namespace_source),
        ("c-external-module.ts", external_module_source),
    ] {
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));
        let syntax = service.syntactic_diagnostics(path);
        assert_eq!(
            syntax.syntactic_completion,
            SemanticCompletion::Complete,
            "{path}: {:#?}",
            syntax.diagnostics,
        );
        assert!(
            syntax.diagnostics.is_empty(),
            "{path}: {:#?}",
            syntax.diagnostics
        );
    }

    let mut enum_service = LanguageService::new(options());
    enum_service.open("a-enum.ts", Arc::<str>::from(enum_source));
    let enum_syntax = enum_service.syntactic_diagnostics("a-enum.ts");
    assert_eq!(
        enum_syntax.syntactic_completion,
        SemanticCompletion::Deferred
    );
    assert!(enum_syntax.diagnostics.is_empty());
    let semantic = enum_service.semantic_diagnostics("a-enum.ts");
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [(
            "a-enum.ts",
            2304,
            enum_source.find("MissingSibling").unwrap() as u32,
            "MissingSibling".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingSibling'.",
            &[][..],
        )],
    );

    let mut external_service = LanguageService::new(options());
    external_service.open(
        "c-external-module.ts",
        Arc::<str>::from(external_module_source),
    );
    let external_semantic = external_service.semantic_diagnostics("c-external-module.ts");
    assert_eq!(
        external_semantic.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        external_semantic
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [(
            "c-external-module.ts",
            2304,
            external_module_source.find("MissingModule").unwrap() as u32,
            "MissingModule".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingModule'.",
            &[][..],
        )],
    );
}

#[test]
fn enum_and_rejected_external_module_syntax_nonclaims_are_host_scoped() {
    // Pinned TypeScript 7 reports TS1109 for the missing enum initializer and
    // TS1035 for each nonambient quoted module. Until those owners are ported,
    // TSZ retains their structure without claiming a definitive syntax product.
    for (shape, source) in [
        ("valid-enum", "enum Signal { Ready }"),
        ("missing-enum-initializer", "enum Signal { Ready = }"),
        (
            "nonambient-external-module",
            "module 'package-name' { function member():void; }",
        ),
        (
            "exported-nonambient-external-module",
            "export module 'package-name' { function member():void; }",
        ),
    ] {
        let mut service = LanguageService::new(options());
        service.open("host.ts", Arc::<str>::from(source));
        let syntax = service.syntactic_diagnostics("host.ts");
        assert_eq!(
            syntax.syntactic_completion,
            SemanticCompletion::Deferred,
            "{shape}: {:#?}",
            syntax.diagnostics,
        );
        assert!(
            syntax.diagnostics.is_empty(),
            "{shape}: {:#?}",
            syntax.diagnostics
        );
        let output = service.compile();
        assert_completion(&output, SemanticCompletion::Deferred);
        assert!(
            output.diagnostics.is_empty(),
            "{shape}: {:#?}",
            output.diagnostics
        );
    }

    for (shape, source) in [
        (
            "ambient-external-module",
            "declare module 'package-name' { function member():void; }",
        ),
        (
            "exported-ambient-external-module",
            "export declare module 'package-name' { function member():void; }",
        ),
    ] {
        let mut service = LanguageService::new(options());
        service.open("host.ts", Arc::<str>::from(source));
        let syntax = service.syntactic_diagnostics("host.ts");
        assert_eq!(
            syntax.syntactic_completion,
            SemanticCompletion::Complete,
            "{shape}: {:#?}",
            syntax.diagnostics,
        );
        assert!(
            syntax.diagnostics.is_empty(),
            "{shape}: {:#?}",
            syntax.diagnostics
        );
    }

    let mut service = LanguageService::new(options());
    service.open("a-host.ts", Arc::<str>::from("enum Signal { Ready = }"));
    let sibling_source = "type Kept = MissingAcross;";
    service.open("z-sibling.ts", Arc::<str>::from(sibling_source));
    assert_eq!(
        service
            .syntactic_diagnostics("a-host.ts")
            .syntactic_completion,
        SemanticCompletion::Deferred,
    );
    let sibling_syntax = service.syntactic_diagnostics("z-sibling.ts");
    assert_eq!(
        sibling_syntax.syntactic_completion,
        SemanticCompletion::Complete,
    );
    assert!(sibling_syntax.diagnostics.is_empty());
    let sibling_semantic = service.semantic_diagnostics("z-sibling.ts");
    assert_eq!(
        sibling_semantic.semantic_completion,
        SemanticCompletion::Complete,
    );
    assert_eq!(
        sibling_semantic
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [(
            "z-sibling.ts",
            2304,
            sibling_source.find("MissingAcross").unwrap() as u32,
            "MissingAcross".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingAcross'.",
            &[][..],
        )],
    );
}

#[test]
fn valid_declaration_hosts_preserve_independent_cross_file_diagnostics() {
    let host = (
        "a-host.ts",
        "declare module 'package-name' { function member():void; }",
    );
    let sibling_source = "type Kept = MissingAcross;";
    let sibling = ("z-sibling.ts", sibling_source);

    for files in [[host, sibling], [sibling, host]] {
        let output = compile_files(&files);
        assert_completion(&output, SemanticCompletion::Deferred);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.file.as_str(),
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.category,
                    diagnostic.message_text.as_str(),
                    diagnostic.related_information.as_slice(),
                ))
                .collect::<Vec<_>>(),
            [(
                "z-sibling.ts",
                2304,
                sibling_source.find("MissingAcross").unwrap() as u32,
                "MissingAcross".len() as u32,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingAcross'.",
                &[][..],
            )],
        );
    }
}

#[test]
fn declaration_host_files_preserve_owned_conditional_parser_diagnostics() {
    let malformed = concat!(
        "declare module 'package-name' { function member():void; } ",
        "const value = flag ? : 1;",
    );
    let mut service = LanguageService::new(options());
    service.open("malformed.ts", Arc::<str>::from(malformed));

    let syntax = service.syntactic_diagnostics("malformed.ts");
    assert_eq!(
        syntax.syntactic_completion,
        SemanticCompletion::Complete,
        "{:#?}",
        syntax.diagnostics,
    );
    assert_eq!(
        syntax
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [(
            "malformed.ts",
            1109,
            malformed.find(": 1").unwrap() as u32,
            1,
            DiagnosticCategory::Error,
            "Expression expected.",
            &[][..],
        )],
    );
}

#[test]
fn declaration_hosts_respect_program_owned_standard_library_identity() {
    let library_options = CompilerOptions {
        allow_js: true,
        check_js: Some(true),
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
            "const-enum",
            "host.ts",
            "const enum Hidden { Value } export const runtime = 1;",
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
                        check_js: Some(true),
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
                            check_js: Some(true),
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

#[test]
fn parsed_namespace_bodies_preserve_binder_owned_descendant_identity() {
    let source = SourceText::new(
        FileId(0),
        "namespace.ts".into(),
        Arc::<str>::from(concat!(
            "namespace Vessel {",
            "  export class Cargo {}",
            "  export const count = 1;",
            "  export namespace Hold { export interface Manifest {} }",
            "}",
            "const sibling = 2;",
        )),
    );
    let parsed = parse_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let bound = bind_source_with_kind(source.id, tsz::source::SourceKind::TypeScript, &parsed.unit);
    let root = ScopeId(0);

    for meaning in [Meaning::Value, Meaning::Type] {
        assert!(
            bound.resolve(root, "Vessel", meaning).is_some(),
            "the renamed host must retain both authored meanings: {:#?}",
            bound.declarations,
        );
    }
    let cargo = bound
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Cargo" && declaration.meaning == Meaning::Type)
        .expect("namespace class identity");
    let count = bound
        .declarations
        .iter()
        .find(|declaration| declaration.name == "count")
        .expect("namespace variable identity");
    let hold = bound
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Hold" && declaration.meaning == Meaning::Type)
        .expect("nested namespace identity");
    let manifest = bound
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Manifest")
        .expect("nested namespace descendant identity");
    let sibling = bound
        .declarations
        .iter()
        .find(|declaration| declaration.name == "sibling")
        .expect("following sibling identity");

    assert_eq!(cargo.scope, count.scope);
    assert_eq!(hold.scope, cargo.scope);
    assert_ne!(cargo.scope, root);
    assert_ne!(manifest.scope, hold.scope);
    assert_eq!(
        bound.scopes[manifest.scope.0 as usize].parent,
        Some(hold.scope)
    );
    assert_eq!(sibling.scope, root);
    assert!(bound.resolve(root, "Cargo", Meaning::Type).is_none());
    assert_eq!(
        bound.resolve(cargo.scope, "Cargo", Meaning::Type),
        Some(cargo.id),
    );
    assert_eq!(
        bound.resolve(hold.scope, "Manifest", Meaning::Type),
        None,
        "nested members must stay in the nested namespace scope",
    );
    assert_eq!(
        bound.resolve(manifest.scope, "Manifest", Meaning::Type),
        Some(manifest.id),
    );
}

#[test]
fn ambient_module_and_global_bodies_keep_local_identity_without_claiming_semantics() {
    for (path, source, body_name, body_is_global) in [
        (
            "module.ts",
            "declare module 'renamed-package' { export interface Contract {} } const peer = 1;",
            "Contract",
            false,
        ),
        (
            "global.ts",
            "declare global { interface GlobalContract {} } const peer = 1;",
            "GlobalContract",
            true,
        ),
    ] {
        let source_text = SourceText::new(FileId(0), path.into(), Arc::<str>::from(source));
        let parsed = parse_source(&source_text);
        assert!(
            parsed.diagnostics.is_empty(),
            "{path}: {:?}",
            parsed.diagnostics
        );
        let bound = bind_source_with_kind(
            source_text.id,
            tsz::source::SourceKind::TypeScript,
            &parsed.unit,
        );
        let body = bound
            .declarations
            .iter()
            .find(|declaration| declaration.name == body_name)
            .unwrap_or_else(|| panic!("{path}: missing {body_name}: {:#?}", bound.declarations));
        let peer = bound
            .declarations
            .iter()
            .find(|declaration| declaration.name == "peer")
            .expect("following sibling identity");
        assert_eq!(peer.scope, ScopeId(0));
        assert_eq!(body.scope == ScopeId(0), body_is_global, "{path}");

        let output = compile(path, source);
        assert_completion(&output, SemanticCompletion::Deferred);
    }
}

#[test]
fn namespace_body_identity_is_deterministic_across_root_order() {
    let left = (
        "a-host.ts",
        "namespace Fleet { export interface Vessel {} }",
    );
    let right_source = "namespace Fleet { export interface Cargo {} } type Kept = MissingAcross;";
    let right = ("z-host.ts", right_source);
    let forward = compile_files(&[left, right]);
    let reverse = compile_files(&[right, left]);

    for output in [&forward, &reverse] {
        assert_completion(output, SemanticCompletion::Deferred);
        assert_eq!(
            diagnostic_rows(&output.diagnostics),
            [(
                2304,
                right_source.find("MissingAcross").unwrap() as u32,
                "MissingAcross".len() as u32,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingAcross'.",
            )],
        );
    }
    assert_eq!(forward.diagnostics, reverse.diagnostics);
    assert_eq!(
        forward.stats.semantic_completion,
        reverse.stats.semantic_completion
    );
}
