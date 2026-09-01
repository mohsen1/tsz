use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_source(source: &str, options: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn legacy_metadata_options() -> PrinterOptions {
    PrinterOptions {
        legacy_decorators: true,
        emit_decorator_metadata: true,
        target: ScriptTarget::ES2015,
        ..Default::default()
    }
}

#[test]
fn decorator_metadata_async_method_without_annotation_returns_promise() {
    let source = "declare const d: MethodDecorator;\nclass C {\n    @d\n    async inferred() {}\n    @d\n    async explicitAny(): any { return 1; }\n    @d\n    async explicitVoid(): void {}\n}\n";
    let output = emit_source(source, legacy_metadata_options());

    assert!(
        output.contains(
            "__metadata(\"design:returntype\", Promise)\n], C.prototype, \"inferred\", null);"
        ),
        "Inferred async method metadata should use Promise.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__metadata(\"design:returntype\", Object)\n], C.prototype, \"explicitAny\", null);"
        ),
        "Explicit async `any` annotation should serialize normally.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__metadata(\"design:returntype\", void 0)\n], C.prototype, \"explicitVoid\", null);"
        ),
        "Explicit async `void` annotation should serialize normally.\nOutput:\n{output}"
    );
}

#[test]
fn decorator_metadata_nolib_isolated_global_type_uses_typeof_guard() {
    let source = "declare var Decorate: PropertyDecorator;\nexport class B {\n    @Decorate\n    member: Map<string, number>;\n}\n";
    let output = emit_source(
        source,
        PrinterOptions {
            no_lib: true,
            isolated_modules: true,
            ..legacy_metadata_options()
        },
    );

    assert!(
        output.contains("var _a;"),
        "Metadata guard should hoist its temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:type\", typeof (_a = typeof Map !== \"undefined\" && Map) === \"function\" ? _a : Object)"),
        "No-lib isolated metadata should guard unresolved global constructors.\nOutput:\n{output}"
    );
}

#[test]
fn decorator_metadata_unresolved_qualified_type_uses_checked_entity_chain() {
    let source = "declare function decorate(...args: any[]): any;\ndeclare namespace A {\n    export namespace B {\n        export namespace C {\n            export namespace D {\n            }\n        }\n    }\n}\nclass Foo {\n    f(@decorate user: A.B.C.D.E): void {}\n}\n";
    let output = emit_source(source, legacy_metadata_options());

    assert!(
        output.contains("var _a, _b, _c, _d;"),
        "Qualified metadata fallback should hoist intermediate and final temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:paramtypes\", [typeof (_d = typeof A !== \"undefined\" && (_a = A.B) !== void 0 && (_b = _a.C) !== void 0 && (_c = _b.D) !== void 0 && _c.E) === \"function\" ? _d : Object])"),
        "Unresolved qualified metadata should emit tsc's checked entity-name fallback.\nOutput:\n{output}"
    );
}

#[test]
fn decorator_metadata_import_equals_qualified_type_keeps_runtime_root() {
    let source = "import database = require(\"./db\");\ndeclare function decorate(...args: any[]): any;\n@decorate\nclass MyClass {\n    constructor(value: database.db) {}\n}\n";
    let output = emit_source(
        source,
        PrinterOptions {
            module: ModuleKind::CommonJS,
            ..legacy_metadata_options()
        },
    );

    assert!(
        output.contains("__metadata(\"design:paramtypes\", [database.db])"),
        "Runtime import-equals qualified metadata should not be wrapped in the unresolved fallback.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("typeof (_a = typeof database"),
        "Known runtime import-equals metadata should stay direct.\nOutput:\n{output}"
    );
}

#[test]
fn decorator_metadata_named_import_qualified_type_keeps_substituted_runtime_root() {
    let source = "import { Foo } from \"./m\";\ndeclare function decorate(...args: any[]): any;\nclass MyClass {\n    method(@decorate value: Foo.Bar): void {}\n}\n";
    let output = emit_source(
        source,
        PrinterOptions {
            module: ModuleKind::CommonJS,
            ..legacy_metadata_options()
        },
    );

    assert!(
        output.contains("__metadata(\"design:paramtypes\", [m_1.Foo.Bar])"),
        "CommonJS named-import qualified metadata should use the substituted runtime chain directly.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("typeof (_a = typeof m_1"),
        "Known runtime named-import metadata should not use the unresolved fallback.\nOutput:\n{output}"
    );
}

fn metadata_options_for(target: ScriptTarget) -> PrinterOptions {
    PrinterOptions {
        target,
        ..legacy_metadata_options()
    }
}

const BIGINT_SYMBOL_SOURCE: &str = "declare function dec(...args: any[]): any;\nclass C {\n    @dec a: bigint;\n    @dec b: symbol;\n}\n";

#[test]
fn decorator_metadata_bigint_is_always_guarded_at_modern_targets() {
    // `tsc` ALWAYS guards `bigint` because the `BigInt` global is not implied by
    // any target; an unguarded reference throws `ReferenceError` where it is
    // absent. This must hold even at the highest targets.
    for target in [
        ScriptTarget::ES2017,
        ScriptTarget::ES2020,
        ScriptTarget::ESNext,
    ] {
        let output = emit_source(BIGINT_SYMBOL_SOURCE, metadata_options_for(target));
        assert!(
            output.contains(
                "__metadata(\"design:type\", typeof BigInt === \"function\" ? BigInt : Object)"
            ),
            "bigint metadata at {target:?} should be guarded.\nOutput:\n{output}"
        );
    }
}

#[test]
fn decorator_metadata_symbol_guarded_only_below_es2015() {
    // ES5: `Symbol` is not implied by the target, so `tsc` guards it.
    let es5 = emit_source(
        BIGINT_SYMBOL_SOURCE,
        metadata_options_for(ScriptTarget::ES5),
    );
    assert!(
        es5.contains(
            "__metadata(\"design:type\", typeof Symbol === \"function\" ? Symbol : Object)"
        ),
        "symbol metadata at ES5 should be guarded.\nOutput:\n{es5}"
    );
    // bigint stays guarded at ES5 too.
    assert!(
        es5.contains(
            "__metadata(\"design:type\", typeof BigInt === \"function\" ? BigInt : Object)"
        ),
        "bigint metadata at ES5 should be guarded.\nOutput:\n{es5}"
    );
}

#[test]
fn decorator_metadata_symbol_is_bare_at_es2015_and_above() {
    // ES2015 is the boundary: `Symbol` is implied, so it is emitted bare.
    for target in [
        ScriptTarget::ES2015,
        ScriptTarget::ES2017,
        ScriptTarget::ESNext,
    ] {
        let output = emit_source(BIGINT_SYMBOL_SOURCE, metadata_options_for(target));
        assert!(
            output.contains("__metadata(\"design:type\", Symbol)"),
            "symbol metadata at {target:?} should be a bare Symbol.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("typeof Symbol === \"function\""),
            "symbol metadata at {target:?} must not be guarded.\nOutput:\n{output}"
        );
    }
}

#[test]
fn decorator_metadata_bigint_in_paramtypes_and_returntype_is_guarded() {
    let source = "declare function dec(...args: any[]): any;\nclass C {\n    @dec m(x: bigint): bigint { return x; }\n}\n";
    let output = emit_source(source, metadata_options_for(ScriptTarget::ES2020));
    assert!(
        output.contains(
            "__metadata(\"design:paramtypes\", [typeof BigInt === \"function\" ? BigInt : Object])"
        ),
        "bigint in design:paramtypes should be guarded.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__metadata(\"design:returntype\", typeof BigInt === \"function\" ? BigInt : Object)"
        ),
        "bigint in design:returntype should be guarded.\nOutput:\n{output}"
    );
}

#[test]
fn decorator_metadata_union_bigint_unwraps_to_guarded_bigint() {
    // `bigint | undefined` unwraps to the single meaningful member, which must
    // still carry the guard.
    let source = "declare function dec(...args: any[]): any;\nclass C {\n    @dec a: bigint | undefined;\n}\n";
    let output = emit_source(source, metadata_options_for(ScriptTarget::ES2020));
    assert!(
        output.contains(
            "__metadata(\"design:type\", typeof BigInt === \"function\" ? BigInt : Object)"
        ),
        "union bigint | undefined should serialize to the guarded bigint form.\nOutput:\n{output}"
    );
}

#[test]
fn decorator_metadata_bigint_literal_type_is_guarded() {
    let source = "declare function dec(...args: any[]): any;\nclass C {\n    @dec a: 1n;\n}\n";
    let output = emit_source(source, metadata_options_for(ScriptTarget::ES2020));
    assert!(
        output.contains(
            "__metadata(\"design:type\", typeof BigInt === \"function\" ? BigInt : Object)"
        ),
        "bigint literal type should serialize to the guarded bigint form.\nOutput:\n{output}"
    );
}
