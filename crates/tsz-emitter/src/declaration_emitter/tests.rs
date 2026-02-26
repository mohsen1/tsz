use super::*;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::{FunctionShape, TypeId, TypeInterner};

#[test]
fn test_function_declaration() {
    let source = "export function add(a: number, b: number): number { return a + b; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export declare function add"),
        "Expected export declare: {output}"
    );
    assert!(
        output.contains("a: number"),
        "Expected parameter type: {output}"
    );
    assert!(
        output.contains("): number;"),
        "Expected return type: {output}"
    );
}

#[test]
fn test_non_exported_function_declaration_emits_declare_function() {
    let source = "function helper(x: string): string { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function helper"),
        "Expected non-exported function to be emitted as declare function: {output}"
    );
    assert!(
        !output.contains("export declare function helper"),
        "Expected no export keyword for non-exported top-level function in global scope: {output}"
    );
}

#[test]
fn test_class_declaration() {
    let source = r#"
    export class Calculator {
        private value: number;
        add(n: number): this {
            this.value += n;
            return this;
        }
    }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("class Calculator"),
        "Expected class declaration: {output}"
    );
    assert!(output.contains("value"), "Expected property: {output}");
    assert!(
        output.contains("add") && output.contains("number"),
        "Expected method signature with add and number: {output}"
    );
}

#[test]
fn test_interface_declaration() {
    let source = "export interface Point { x: number; y: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("interface Point"),
        "Expected interface: {output}"
    );
    assert!(output.contains("number"), "Expected number type: {output}");
}

#[test]
fn test_type_alias() {
    let source = "export type ID = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export type ID = string | number"),
        "Expected type alias: {output}"
    );
}

#[test]
fn test_type_only_export_module_gets_empty_export_marker() {
    // When a module has only an import (module syntax) and private types,
    // the .d.ts needs `export {};` to preserve module semantics, since tsc
    // would not emit any explicit exports for a file like this.
    let source = r#"
import "some-dep";
type T = { x: number };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export {};"),
        "Expected empty export marker for import-only module: {output}"
    );
}

#[test]
fn test_type_export_module_does_not_need_empty_export_marker() {
    // When the .d.ts already has an explicit type export, `export {};` is not
    // needed — the export itself preserves module semantics.
    let source = r#"
type T = { x: number };
export interface I {
    f: T;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export interface I"),
        "Expected exported interface: {output}"
    );
    assert!(
        !output.contains("export {};"),
        "Did not expect empty export marker when explicit type export exists: {output}"
    );
}

#[test]
fn test_empty_named_export_has_no_extra_spacing() {
    let source = "export {};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export {};"),
        "Expected compact empty export syntax: {output}"
    );
    assert!(
        !output.contains("export {  };"),
        "Did not expect extra spacing in empty export syntax: {output}"
    );
}

#[test]
fn test_private_set_accessor_omits_type_and_uses_value_param_name() {
    let source = r#"
declare class C {
    private set x(foo: string);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare class C"),
        "Expected declared class: {output}"
    );
    assert!(
        output.contains("private set x(value);"),
        "Expected private setter value parameter canonicalization: {output}"
    );
}

#[test]
fn test_public_set_accessor_preserves_source_param_name() {
    let source = r#"
declare class C {
    set x(foo: string);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("set x(foo: string);"),
        "Expected public setter to preserve source parameter name: {output}"
    );
}

#[test]
fn test_method_declaration_emits_inferred_return_type() {
    let source = r#"
class C {
    add() {
        return 1;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };
    let Some(class_node) = parser.arena.get(source_file.statements.nodes[0]) else {
        panic!("missing class node");
    };
    let Some(class_decl) = parser.arena.get_class(class_node) else {
        panic!("missing class declaration");
    };
    let method_idx = class_decl.members.nodes[0];

    let interner = TypeInterner::new();
    let method_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, method_type);

    let binder = BinderState::new();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("add(): number;"),
        "Expected inferred method return type: {output}"
    );
}

#[test]
fn test_property_declaration_infers_type_from_numeric_initializer_when_type_cache_missing() {
    let source = r#"
abstract class C {
    abstract prop = 1;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("abstract prop: number;"),
        "Expected inferred property type from initializer: {output}"
    );
}

#[test]
fn test_variable_declaration_infers_accessor_object_type_from_initializer_when_type_cache_missing()
{
    let source = r#"
export var basePrototype = {
  get primaryPath() {
    return 1;
  },
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export declare var basePrototype: {\n    readonly primaryPath: any;\n};"),
        "Expected multi-line object literal accessor inference: {output}"
    );
}

#[test]
fn test_constructor_type_no_double_semicolon() {
    let source = "export type Ctor = new (...args: any[]) => void;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("new (...args: any[]) => void;"),
        "Expected constructor type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in constructor type alias: {output}"
    );
}

#[test]
fn test_template_literal_type_no_double_semicolon() {
    let source = r#"export type Outcome = `${string}_${string}`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("`${string}_${string}`"),
        "Expected template literal type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in template literal type alias: {output}"
    );
}

#[test]
fn test_infer_type_no_double_semicolon() {
    let source = "export type Unpack<T> = T extends (infer U)[] ? U : T;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("infer U"),
        "Expected infer type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in type alias with infer: {output}"
    );
}

#[test]
fn test_abstract_constructor_type() {
    let source = "export type AbstractCtor = abstract new () => object;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("abstract new () => object;"),
        "Expected abstract constructor type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in abstract constructor type: {output}"
    );
}

#[test]
fn test_simple_template_literal_type() {
    let source = r#"export type Greeting = `hello`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("`hello`"),
        "Expected simple template literal type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in simple template literal type: {output}"
    );
}

#[test]
fn test_public_modifier_omitted_from_dts_class_members() {
    // tsc omits `public` from .d.ts output since it's the default accessibility
    let source = r#"
    export class Foo {
        public x: number;
        public greet(): string { return "hello"; }
        protected y: number;
        private z: number;
    }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    // `public` should be stripped (it's the default)
    assert!(
        !output.contains("public "),
        "Expected `public` modifier to be omitted from .d.ts output: {output}"
    );
    // `protected` and `private` should be preserved
    assert!(
        output.contains("protected y"),
        "Expected `protected` modifier to be preserved: {output}"
    );
    assert!(
        output.contains("private z"),
        "Expected `private` modifier to be preserved: {output}"
    );
    // Members themselves should still be present
    assert!(
        output.contains("x: number"),
        "Expected public property to still be emitted (without modifier): {output}"
    );
    assert!(
        output.contains("greet("),
        "Expected public method to still be emitted (without modifier): {output}"
    );
}
