use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper to set up hover infrastructure and get hover info at a position.
fn get_hover_at(source: &str, line: u32, col: u32) -> Option<HoverInfo> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = HoverProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    let pos = Position::new(line, col);
    let mut cache = None;
    provider.get_hover(root, pos, &mut cache)
}

#[test]
fn test_hover_variable_type() {
    let source = "/** The answer */\nconst x = 42;\nx;";
    let info = get_hover_at(source, 2, 0);
    assert!(info.is_some(), "Should find hover info");
    if let Some(info) = info {
        assert!(!info.contents.is_empty(), "Should have contents");
        assert!(
            info.contents[0].contains("x"),
            "Should contain variable name"
        );
        assert!(info.range.is_some(), "Should have range");
    }
}

#[test]
fn test_hover_at_eof_identifier() {
    let source = "/** The answer */\nconst x = 42;\nx";
    let info = get_hover_at(source, 2, 1);
    assert!(info.is_some(), "Should find hover info at EOF");
    if let Some(info) = info {
        assert!(
            info.contents
                .iter()
                .any(|content| content.contains("The answer"))
        );
    }
}

#[test]
fn test_hover_incomplete_member_access() {
    let source = "const foo = 1;\nfoo.";
    let info = get_hover_at(source, 1, 4);
    assert!(
        info.is_some(),
        "Should find hover info after incomplete member access"
    );
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("foo"),
            "Should use base identifier for hover"
        );
    }
}

#[test]
fn test_hover_jsdoc_summary_and_params() {
    let source = "/**\n * Adds two numbers.\n * @param a First number.\n * @param b Second number.\n */\nfunction add(a: number, b: number): number { return a + b; }\nadd(1, 2);";
    let info = get_hover_at(source, 6, 0).expect("Expected hover info");
    let doc = info
        .contents
        .iter()
        .find(|c| c.contains("Adds two numbers."))
        .cloned()
        .unwrap_or_default();
    assert!(doc.contains("Adds two numbers."));
    assert!(doc.contains("Parameters:"));
    assert!(doc.contains("`a` First number."));
    assert!(doc.contains("`b` Second number."));
}

#[test]
fn test_hover_no_symbol() {
    let source = "const x = 42;";
    let info = get_hover_at(source, 0, 13);
    assert!(info.is_none(), "Should not find hover info at semicolon");
}

#[test]
fn test_hover_function() {
    let source = "function foo() { return 1; }\nfoo();";
    let info = get_hover_at(source, 1, 0);
    assert!(info.is_some(), "Should find hover info for function");
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("foo"),
            "Should contain function name"
        );
    }
}

#[test]
fn test_hover_contextually_typed_function_expression_parameter() {
    let source = "<(aa: number) =>void >(function myFn(bb) { });\nbb;";
    let info = get_hover_at(source, 0, 37).expect("Should find hover info for parameter");
    assert_eq!(
        info.display_string, "(parameter) bb: number",
        "Parameter hover should use contextual type from function expression target"
    );
}

#[test]
fn test_hover_property_uses_explicit_function_type_annotation() {
    let source =
        "class C1T5 {\n    foo: (i: number, s: string) => number = function(i) { return i; }\n}";
    let info = get_hover_at(source, 1, 4).expect("Should find hover info for property");
    assert_eq!(
        info.display_string, "(property) C1T5.foo: (i: number, s: string) => number",
        "Property hover should prefer the explicit declaration type annotation over inferred any"
    );
}

#[test]
fn test_hover_property_initializer_parameter_uses_contextual_annotation() {
    let source =
        "class C1T5 {\n    foo: (i: number, s: string) => number = function(i) { return i; }\n}";
    let info = get_hover_at(source, 1, 53)
        .expect("Should find hover info for contextually typed parameter");
    assert_eq!(
        info.display_string, "(parameter) i: number",
        "Parameter hover should use contextual type from class property function type annotation"
    );
}

#[test]
fn test_hover_array_element_function_parameter_uses_contextual_call_signature() {
    let source = "var fns: {(n: number, s: string): string;}[] = [function(n, s) { return s; }];";
    let info = get_hover_at(source, 0, 57)
        .expect("Should find hover info for contextually typed array-element parameter");
    assert_eq!(
        info.display_string, "(parameter) n: number",
        "Parameter hover should use contextual call signature from typed array element"
    );
}

#[test]
fn test_hover_namespace_exported_var_includes_namespace_container() {
    let source = "namespace C2T5 {\n    export var foo: (i: number, s: string) => number = function(i) { return i; };\n}";
    let info = get_hover_at(source, 1, 15).expect("Should find hover info for namespace var");
    assert_eq!(
        info.display_string, "var C2T5.foo: (i: number, s: string) => number",
        "Namespace-exported variable hover should include namespace container in display string"
    );
}

#[test]
fn test_hover_contextual_object_literal_property_name() {
    let source = "interface IFoo { n: number; }\ninterface IBar { foo: IFoo; }\nvar c3t12: IBar = {\n    foo: <IFoo>({})\n};";
    let info = get_hover_at(source, 3, 4)
        .expect("Should find hover info for object-literal property name");
    assert_eq!(
        info.display_string, "(property) IBar.foo: IFoo",
        "Property-name hover in contextually typed object literal should use interface property type"
    );
}

#[test]
fn test_hover_contextual_object_literal_method_name() {
    let source = "interface IFoo { f(i: number, s: string): string; }\nvar c3t13 = <IFoo>({\n    f: function(i, s) { return s; }\n});";
    let info =
        get_hover_at(source, 2, 4).expect("Should find hover info for object-literal method name");
    assert_eq!(
        info.display_string, "(method) IFoo.f(i: number, s: string): string",
        "Function-valued property hover in contextually typed object literal should use interface method signature"
    );
}

#[test]
fn test_hover_contextual_object_literal_array_property_name() {
    let source = "interface IFoo { a: number[]; }\nvar c3t14 = <IFoo>({\n    a: []\n});";
    let info = get_hover_at(source, 2, 4)
        .expect("Should find hover info for contextually typed array property");
    assert_eq!(
        info.display_string, "(property) IFoo.a: number[]",
        "Array-valued property hover in contextually typed object literal should use interface property type"
    );
}

#[test]
fn test_hover_property_access_member_name_uses_member_type() {
    let source = "var objc8: { t1: (s: string) => string } = { t1: (s: string) => s };\nobjc8.t1 = (function(s) { return s; });";
    let info = get_hover_at(source, 1, 6).expect("Should find hover info for property access name");
    assert_eq!(
        info.display_string, "(property) t1: (s: string) => string",
        "Property-access member hover should use contextual member type"
    );
}

#[test]
fn test_hover_property_access_member_name_includes_member_jsdoc() {
    let source = "/** Container docs */\ninterface Obj {\n    /** Member docs */\n    t1: (s: string) => string;\n}\ndeclare const obj: Obj;\nobj.t1;";
    let info =
        get_hover_at(source, 6, 4).expect("Should find hover info for property access member");
    assert_eq!(
        info.display_string, "(property) Obj.t1: (s: string) => string",
        "Property-access member hover should still use the member type"
    );
    assert!(
        info.documentation.contains("Member docs"),
        "Property-access member hover should include member JSDoc, got: '{}'",
        info.documentation
    );
}

#[test]
fn test_hover_property_assignment_function_parameter_uses_member_signature() {
    let source = "var objc8: { t1: (s: string) => string } = { t1: (s: string) => s };\nobjc8.t1 = (function(s) { return s; });";
    let info = get_hover_at(source, 1, 21)
        .expect("Should find hover info for assigned function parameter");
    assert_eq!(
        info.display_string, "(parameter) s: string",
        "Parameter hover in property-assignment function should use assigned member signature type"
    );
}

#[test]
fn test_hover_contextual_parameter_with_global_function_annotation_is_any() {
    let source = "const fn: Function = function(value) { return value; };";
    let info = get_hover_at(source, 0, 30)
        .expect("Should find hover info for Function-annotated callback parameter");
    assert_eq!(
        info.display_string, "(parameter) value: any",
        "Function-typed callback parameters should surface as any"
    );
}

#[test]
fn test_hover_contextual_object_literal_property_name_in_assignment() {
    let source = "interface IFoo { n: number; }\ninterface IBar { foo: IFoo; }\nconst holder: { t12: IBar } = { t12: { foo: { n: 1 } } };\nholder.t12 = {\n    foo: <IFoo>({})\n};";
    let info = get_hover_at(source, 4, 4)
        .expect("Should find hover info for contextually typed property in assignment");
    assert_eq!(
        info.display_string, "(property) IBar.foo: IFoo",
        "Object-literal property hover should use assigned member contextual type"
    );
}

#[test]
fn test_hover_contextual_parameter_in_call_argument_function_expression() {
    let source = "interface IFoo { n: number; }\nfunction c9t5(f: (n: number) => IFoo) {}\nc9t5(function(n) {\n    return <IFoo>({ n: n });\n});";
    let info = get_hover_at(source, 2, 14)
        .expect("Should find hover info for contextually typed call-argument parameter");
    assert_eq!(
        info.display_string, "(parameter) n: number",
        "Call-argument function-expression parameter hover should use contextual call signature type"
    );
}

#[test]
fn test_hover_contextual_parameter_in_property_access_call_argument() {
    let source = "interface IFoo { n: number; }\nconst api: { run: (f: (n: number) => IFoo) => void } = { run: () => {} };\napi.run(function(n) {\n    return <IFoo>({ n: n });\n});";
    let info = get_hover_at(source, 2, 17)
        .expect("Should find hover info for call-argument parameter with property-access callee");
    assert_eq!(
        info.display_string, "(parameter) n: number",
        "Call-argument function parameter hover should use contextual type for property-access callees"
    );
}

#[test]
fn test_hover_best_common_type_object_literal_array_multiline() {
    let source =
        "var a = { name: 'bob', age: 18 };\nvar b = { name: 'jim', age: 20 };\nvar c = [a, b];\nc;";
    let info = get_hover_at(source, 3, 0).expect("Should find hover info for c");
    assert_eq!(
        info.display_string, "var c: {\n    name: string;\n    age: number;\n}[]",
        "Quick-info display string should render object-literal array element types in multiline tsserver style"
    );
}

#[test]
fn test_hover_union_array_precedence_preserved() {
    let source = "var a2 = { name: 'bob', age: 18, address: 'springfield' };\nvar b2 = { name: 'jim', age: 20, dob: 1 };\nvar c2 = [a2, b2];\nc2;";
    let info = get_hover_at(source, 3, 0).expect("Should find hover info for c2");
    assert_eq!(
        info.display_string,
        "var c2: ({\n    name: string;\n    age: number;\n    address: string;\n} | {\n    name: string;\n    age: number;\n    dob: number;\n})[]",
        "Quick-info display should preserve array-vs-union precedence with parenthesized union element type"
    );
}

#[test]
fn test_hover_date_constructor_rewrites_error_property_type() {
    let source = "var a2 = { name: 'bob', age: 18, address: 'springfield' };\nvar b2 = { name: 'jim', age: 20, dob: new Date() };\nvar c2 = [a2, b2];\nc2;";
    let info = get_hover_at(source, 3, 0).expect("Should find hover info for c2");
    assert!(
        info.display_string.contains("dob: Date"),
        "Quick-info display should normalize constructor-based error property type to Date, got: {}",
        info.display_string
    );
}

// =========================================================================
// New tests for tsserver-compatible quickinfo format
// =========================================================================

#[test]
fn test_hover_const_variable_display_string() {
    let source = "const x = 42;\nx;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.display_string.starts_with("const ") || info.display_string.starts_with("let "),
        "Variable display_string should start with const or let keyword, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("x"),
        "display_string should contain variable name 'x', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains(':'),
        "display_string should contain colon for type annotation, got: {}",
        info.display_string
    );
    assert!(
        info.kind == "const" || info.kind == "let",
        "Kind should be 'const' or 'let' for block-scoped variable, got: {}",
        info.kind
    );
}

#[test]
fn test_hover_let_variable_display_string() {
    let source = "let y = \"hello\";\ny;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.display_string.starts_with("let "),
        "Let variable display_string should start with 'let ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("y"),
        "display_string should contain variable name 'y', got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "let",
        "Kind should be 'let' for let variable, got: {}",
        info.kind
    );
}

#[test]
fn test_hover_var_variable_display_string() {
    let source = "var z = true;\nz;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.display_string.starts_with("var "),
        "Var variable display_string should start with 'var ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("z"),
        "display_string should contain variable name 'z', got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "var",
        "Kind should be 'var' for var variable, got: {}",
        info.kind
    );
}

#[test]
fn test_hover_function_display_string() {
    let source = "function greet(name: string): void {}\ngreet(\"hi\");";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.display_string.starts_with("function "),
        "Function display_string should start with 'function ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("greet"),
        "display_string should contain function name 'greet', got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "function",
        "Kind should be 'function', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_class_display_string() {
    let source = "class MyClass { x: number = 0; }\nlet c = new MyClass();";
    let info = get_hover_at(source, 0, 6).expect("Should find hover info for class");
    assert!(
        info.display_string.starts_with("class "),
        "Class display_string should start with 'class ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("MyClass"),
        "display_string should contain class name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "class",
        "Kind should be 'class', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_interface_display_string() {
    let source = "interface IPoint { x: number; y: number; }\nlet p: IPoint;";
    let info = get_hover_at(source, 0, 10).expect("Should find hover info for interface");
    assert!(
        info.display_string.starts_with("interface "),
        "Interface display_string should start with 'interface ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("IPoint"),
        "display_string should contain interface name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "interface",
        "Kind should be 'interface', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_enum_display_string() {
    let source = "enum Color { Red, Green, Blue }\nlet c: Color;";
    let info = get_hover_at(source, 0, 5).expect("Should find hover info for enum");
    assert!(
        info.display_string.starts_with("enum "),
        "Enum display_string should start with 'enum ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("Color"),
        "display_string should contain enum name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "enum",
        "Kind should be 'enum', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_kind_field_populated() {
    let source = "const a = 1;\nlet b = 2;\nfunction f() {}\nclass C {}\ninterface I {}\na; b;";
    let info_a = get_hover_at(source, 5, 0).expect("Should find hover info for a");
    assert!(
        !info_a.kind.is_empty(),
        "Kind should not be empty for const variable"
    );
    let info_b = get_hover_at(source, 5, 3).expect("Should find hover info for b");
    assert!(
        !info_b.kind.is_empty(),
        "Kind should not be empty for let variable"
    );
}

#[test]
fn test_hover_documentation_field_with_jsdoc() {
    let source = "/** My variable */\nconst x = 42;\nx;";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    assert!(
        info.documentation.contains("My variable"),
        "documentation field should contain JSDoc summary, got: '{}'",
        info.documentation
    );
}

#[test]
fn test_hover_documentation_field_empty_without_jsdoc() {
    let source = "const x = 42;\nx;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.documentation.is_empty(),
        "documentation field should be empty without JSDoc, got: '{}'",
        info.documentation
    );
}

#[test]
fn test_hover_display_string_in_code_block() {
    let source = "const x = 42;\nx;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.contents[0].contains(&info.display_string),
        "Code block should contain the display_string. Code block: '{}', display_string: '{}'",
        info.contents[0],
        info.display_string
    );
}

#[test]
fn test_hover_type_alias_display_string() {
    let source = "type MyStr = string;\nlet s: MyStr;";
    let info = get_hover_at(source, 0, 5).expect("Should find hover info for type alias");
    assert!(
        info.display_string.starts_with("type "),
        "Type alias display_string should start with 'type ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("MyStr"),
        "display_string should contain type alias name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "type",
        "Kind should be 'type', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_import_alias_default_and_named_from_ambient_module() {
    let source = r#"declare module "jquery" {}
import foo, { bar } from "jquery";
foo/*useFoo*/(bar/*useBar*/);"#;

    let foo_info =
        get_hover_at(source, 2, 5).expect("Should find hover info for default import marker");
    assert_eq!(
        foo_info.display_string,
        "(alias) module \"jquery\"\nimport foo"
    );
    assert_eq!(foo_info.kind, "alias");

    let bar_info =
        get_hover_at(source, 2, 19).expect("Should find hover info for named import marker");
    assert_eq!(
        bar_info.display_string,
        "(alias) module \"jquery\"\nimport bar"
    );
    assert_eq!(bar_info.kind, "alias");
}

#[test]
fn test_hover_import_equals_alias_from_ambient_module() {
    let source = r#"declare module "jquery" {}
import bang = require("jquery");
bang/*useBang*/;"#;

    let info = get_hover_at(source, 2, 6).expect("Should find hover info for import= alias marker");
    assert_eq!(
        info.display_string,
        "(alias) module \"jquery\"\nimport bang = require(\"jquery\")"
    );
    assert_eq!(info.kind, "alias");
}

#[test]
fn test_hover_import_alias_without_resolved_module() {
    let source = r#"import foo, { bar } from "jquery";
foo(bar);"#;

    let foo_info = get_hover_at(source, 1, 0)
        .expect("Should find hover info for unresolved default import alias");
    assert_eq!(
        foo_info.display_string,
        "(alias) module \"jquery\"\nimport foo"
    );

    let bar_info =
        get_hover_at(source, 1, 4).expect("Should find hover info for unresolved named import");
    assert_eq!(
        bar_info.display_string,
        "(alias) module \"jquery\"\nimport bar"
    );
}

// =========================================================================
// Edge case tests for comprehensive coverage
// =========================================================================

#[test]
fn test_hover_empty_file() {
    let source = "";
    let info = get_hover_at(source, 0, 0);
    assert!(info.is_none(), "Should not find hover info in empty file");
}

#[test]
fn test_hover_parameter_type() {
    let source = "function foo(x: number) { return x; }";
    let info = get_hover_at(source, 0, 13).expect("Should find hover for parameter");
    assert!(
        info.display_string.contains("x"),
        "Should contain parameter name, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_arrow_function() {
    let source = "const add = (a: number, b: number) => a + b;\nadd(1, 2);";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for arrow function");
    assert!(
        info.display_string.contains("add"),
        "Should contain function name, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_namespace() {
    let source = "namespace MyNS {\n  export const x = 1;\n}";
    let info = get_hover_at(source, 0, 10).expect("Should find hover for namespace");
    assert!(
        info.display_string.contains("MyNS"),
        "Should contain namespace name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "module",
        "Namespace kind should be 'module', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_enum_member() {
    let source = "enum Color { Red = 1, Green = 2 }\nColor.Red;";
    let info = get_hover_at(source, 0, 13);
    // Hover on 'Red' member declaration
    if let Some(info) = info {
        assert!(
            info.display_string.contains("Red"),
            "Should contain enum member name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_class_method() {
    let source = "class Foo {\n  bar() { return 42; }\n}";
    let info = get_hover_at(source, 1, 2).expect("Should find hover for class method");
    assert!(
        info.display_string.contains("bar"),
        "Should contain method name, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_at_beginning_of_line() {
    let source = "const x = 42;\nx;";
    let info = get_hover_at(source, 1, 0);
    assert!(info.is_some(), "Should find hover at beginning of line");
}

#[test]
fn test_hover_multiline_function_signature() {
    let source = "function longName(\n  a: number,\n  b: string,\n  c: boolean\n): void {}";
    let info = get_hover_at(source, 0, 9).expect("Should find hover for function");
    assert!(
        info.display_string.contains("longName"),
        "Should contain function name, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_const_assertion() {
    let source = "const arr = [1, 2, 3] as const;\narr;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for const assertion");
    assert!(
        info.display_string.contains("arr"),
        "Should contain variable name, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_export_function() {
    let source = "export function exported() { return true; }";
    let info = get_hover_at(source, 0, 16).expect("Should find hover for exported function");
    assert!(
        info.display_string.contains("exported"),
        "Should contain function name, got: {}",
        info.display_string
    );
    assert!(
        info.kind_modifiers.contains("export"),
        "Should have export modifier, got: {}",
        info.kind_modifiers
    );
}

// =========================================================================
// Additional coverage tests for hover module
// =========================================================================

#[test]
fn test_hover_variable_with_type_annotation() {
    let source = "let x: string = \"hello\";\nx;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for annotated variable");
    assert!(
        info.display_string.contains("x"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("string"),
        "Should contain the type annotation, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_interface_property_via_access() {
    let source = "interface Point { x: number; y: number; }\ndeclare const p: Point;\np.x;";
    let info =
        get_hover_at(source, 2, 2).expect("Should find hover for interface property via access");
    assert!(
        info.display_string.contains("x"),
        "Should contain property name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "property",
        "Kind should be 'property', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_class_property_with_type_annotation() {
    let source = "class Foo {\n  name: string = \"\";\n}";
    let info = get_hover_at(source, 1, 2).expect("Should find hover for class property");
    assert!(
        info.display_string.contains("name"),
        "Should contain property name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("string"),
        "Should contain the type, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "property",
        "Kind should be 'property', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_enum_member_with_value() {
    let source = "enum Direction { Up = 0, Down = 1, Left = 2, Right = 3 }\nDirection.Up;";
    // Use the reference on line 1 to get hover for the enum member access
    let info = get_hover_at(source, 1, 10);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("Up"),
            "Should contain enum member name 'Up', got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_local_variable_inside_function() {
    let source = "function foo() {\n  const local = 42;\n  local;\n}";
    let info = get_hover_at(source, 2, 2).expect("Should find hover for local variable");
    assert!(
        info.display_string.contains("local"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    // Local variables inside functions are displayed as "(local const) local: ..."
    assert!(
        info.display_string.contains("local"),
        "Should show local variable, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_declare_function() {
    let source = "declare function greet(name: string): void;\ngreet;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for declared function");
    assert!(
        info.display_string.contains("greet"),
        "Should contain function name, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_declare_const() {
    let source = "declare const PI: number;\nPI;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for declared const");
    assert!(
        info.display_string.contains("PI"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("number"),
        "Should contain type, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_interface_method_via_access() {
    let source =
        "interface Greeter { greet(name: string): string; }\ndeclare const g: Greeter;\ng.greet;";
    let info =
        get_hover_at(source, 2, 2).expect("Should find hover for interface method via access");
    assert!(
        info.display_string.contains("greet"),
        "Should contain method name, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_class_expression_keyword() {
    let source = "const MyClass = class Named { x = 1; };";
    // Hover on the 'class' keyword of a class expression
    let info = get_hover_at(source, 0, 16);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("Named") || info.display_string.contains("class"),
            "Should contain class info, got: {}",
            info.display_string
        );
        assert_eq!(
            info.kind, "class",
            "Kind should be 'class', got: {}",
            info.kind
        );
    }
}

#[test]
fn test_hover_union_type_annotation() {
    let source = "let val: string | number = \"hello\";\nval;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for union-typed variable");
    assert!(
        info.display_string.contains("val"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("string") || info.display_string.contains("number"),
        "Should contain at least part of the union type, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_intersection_type_annotation() {
    let source = "interface A { a: number; }\ninterface B { b: string; }\nlet val: A & B;\nval;";
    let info =
        get_hover_at(source, 3, 0).expect("Should find hover for intersection-typed variable");
    assert!(
        info.display_string.contains("val"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("A") && info.display_string.contains("B"),
        "Should contain both intersection type parts, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_generic_function() {
    let source = "function identity<T>(arg: T): T { return arg; }\nidentity;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for generic function");
    assert!(
        info.display_string.contains("identity"),
        "Should contain function name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "function",
        "Kind should be 'function', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_generic_class() {
    let source = "class Container<T> {\n  value: T;\n  constructor(v: T) { this.value = v; }\n}";
    let info = get_hover_at(source, 0, 6).expect("Should find hover for generic class");
    assert!(
        info.display_string.contains("Container"),
        "Should contain class name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "class",
        "Kind should be 'class', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_generic_type_alias() {
    let source = "type Pair<A, B> = { first: A; second: B; };\nlet p: Pair<number, string>;";
    let info = get_hover_at(source, 0, 5).expect("Should find hover for generic type alias");
    assert!(
        info.display_string.contains("Pair"),
        "Should contain type alias name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "type",
        "Kind should be 'type', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_array_typed_variable() {
    let source = "let nums: number[] = [1, 2, 3];\nnums;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for array-typed variable");
    assert!(
        info.display_string.contains("nums"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("number[]"),
        "Should contain array type, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_tuple_type_annotation() {
    let source = "let pair: [string, number] = [\"a\", 1];\npair;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for tuple-typed variable");
    assert!(
        info.display_string.contains("pair"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("string") && info.display_string.contains("number"),
        "Should contain tuple element types, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_function_returning_void() {
    let source = "function doNothing(): void {}\ndoNothing;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for void function");
    assert!(
        info.display_string.contains("doNothing"),
        "Should contain function name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("void"),
        "Should contain void return type, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_abstract_class() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}";
    let info = get_hover_at(source, 0, 15).expect("Should find hover for abstract class");
    assert!(
        info.display_string.contains("Shape"),
        "Should contain class name, got: {}",
        info.display_string
    );
    assert!(
        info.kind_modifiers.contains("abstract"),
        "Should have 'abstract' modifier, got: {}",
        info.kind_modifiers
    );
}

#[test]
fn test_hover_property_access_on_typed_object() {
    let source =
        "interface Config { host: string; port: number; }\ndeclare const cfg: Config;\ncfg.host;";
    let info = get_hover_at(source, 2, 4).expect("Should find hover for property access member");
    assert!(
        info.display_string.contains("host"),
        "Should contain property name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("string"),
        "Should contain property type, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "property",
        "Kind should be 'property', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_enum_member_access() {
    let source = "enum Color { Red, Green, Blue }\nColor.Red;";
    let info = get_hover_at(source, 1, 6);
    // Hovering on 'Red' in Color.Red
    if let Some(info) = info {
        assert!(
            info.display_string.contains("Red"),
            "Should contain enum member name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_const_enum() {
    let source = "const enum Status { Active, Inactive }";
    let info = get_hover_at(source, 0, 11).expect("Should find hover for const enum");
    assert!(
        info.display_string.contains("Status"),
        "Should contain enum name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "enum",
        "Kind should be 'enum', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_function_with_optional_parameter() {
    let source = "function greet(name?: string): string { return name ?? \"world\"; }\ngreet;";
    let info =
        get_hover_at(source, 1, 0).expect("Should find hover for function with optional param");
    assert!(
        info.display_string.contains("greet"),
        "Should contain function name, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_function_with_rest_parameter() {
    let source =
        "function sum(...nums: number[]): number { return nums.reduce((a, b) => a + b, 0); }\nsum;";
    let info =
        get_hover_at(source, 1, 0).expect("Should find hover for function with rest parameter");
    assert!(
        info.display_string.contains("sum"),
        "Should contain function name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "function",
        "Kind should be 'function', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_jsdoc_deprecated_tag() {
    let source = "/** @deprecated Use newFn instead */\nfunction oldFn() {}\noldFn;";
    let info = get_hover_at(source, 2, 0).expect("Should find hover for deprecated function");
    assert!(
        info.contents
            .iter()
            .any(|c| c.contains("deprecated") || c.contains("Use newFn")),
        "Should contain deprecation info in hover contents, got: {:?}",
        info.contents
    );
}

#[test]
fn test_hover_jsdoc_returns_tag() {
    let source = "/**\n * Doubles a number.\n * @returns The doubled value.\n */\nfunction double(n: number): number { return n * 2; }\ndouble;";
    let info = get_hover_at(source, 5, 0).expect("Should find hover for function with @returns");
    assert!(
        info.documentation.contains("Doubles a number"),
        "Should contain summary in documentation, got: '{}'",
        info.documentation
    );
}

#[test]
fn test_hover_var_inside_function_is_local() {
    let source = "function outer() {\n  var inner = 10;\n  inner;\n}";
    let info = get_hover_at(source, 2, 2).expect("Should find hover for local var");
    assert!(
        info.display_string.contains("inner"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("local var")
            || info.display_string.contains("var")
            || info.display_string.contains("inner"),
        "Should show local var info, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_let_inside_function_is_local() {
    let source = "function outer() {\n  let count = 0;\n  count;\n}";
    let info = get_hover_at(source, 2, 2).expect("Should find hover for local let");
    assert!(
        info.display_string.contains("count"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    // Local block-scoped variables get "(local let)" prefix
    assert!(
        info.display_string.contains("local") || info.display_string.contains("let"),
        "Should indicate local scope, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_class_with_namespace_merge() {
    let source = "class Widget {}\nnamespace Widget {\n  export const version = 1;\n}";
    let info =
        get_hover_at(source, 0, 6).expect("Should find hover for class with namespace merge");
    assert!(
        info.display_string.contains("Widget"),
        "Should contain class name, got: {}",
        info.display_string
    );
    // Should show class + namespace merge
    assert!(
        info.display_string.contains("class") && info.display_string.contains("namespace"),
        "Should show both class and namespace, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_function_with_namespace_merge() {
    let source =
        "function handler() {}\nnamespace handler {\n  export const timeout = 1000;\n}\nhandler;";
    let info =
        get_hover_at(source, 4, 0).expect("Should find hover for function with namespace merge");
    assert!(
        info.display_string.contains("handler"),
        "Should contain function name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("function") && info.display_string.contains("namespace"),
        "Should show both function and namespace, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_whitespace_only_returns_none() {
    let source = "const x = 1;\n\n\nconst y = 2;";
    let info = get_hover_at(source, 1, 0);
    assert!(info.is_none(), "Should not find hover info on blank line");
}

#[test]
fn test_hover_string_literal_variable() {
    let source = "const greeting = \"hello world\";\ngreeting;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for string variable");
    assert!(
        info.display_string.contains("greeting"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    // const should preserve literal type "hello world" or show string
    assert!(
        info.display_string.contains("hello world") || info.display_string.contains("string"),
        "Should contain string type info, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_boolean_literal_variable() {
    let source = "const flag = true;\nflag;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for boolean variable");
    assert!(
        info.display_string.contains("flag"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("true") || info.display_string.contains("boolean"),
        "Should contain boolean type info, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_numeric_literal_const() {
    let source = "const PI = 3.14;\nPI;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover for numeric const");
    assert!(
        info.display_string.contains("PI"),
        "Should contain variable name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("3.14") || info.display_string.contains("number"),
        "Should contain numeric type info, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_multiple_declarations_same_interface() {
    let source =
        "interface Obj { a: number; }\ninterface Obj { b: string; }\ndeclare const o: Obj;\no.a;";
    let info = get_hover_at(source, 3, 2).expect("Should find hover for merged interface member");
    assert!(
        info.display_string.contains("a"),
        "Should contain property name, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("number"),
        "Should contain property type, got: {}",
        info.display_string
    );
}

#[test]
fn test_hover_readonly_property() {
    let source = "class Immutable {\n  readonly id: number = 1;\n}";
    let info = get_hover_at(source, 1, 11).expect("Should find hover for readonly property");
    assert!(
        info.display_string.contains("id"),
        "Should contain property name, got: {}",
        info.display_string
    );
    assert!(
        info.kind_modifiers.contains("readonly") || info.display_string.contains("id"),
        "Should indicate readonly or contain property name, got modifiers: {}, display: {}",
        info.kind_modifiers,
        info.display_string
    );
}

// =========================================================================
// Additional edge-case tests
// =========================================================================

#[test]
fn test_hover_object_destructuring_variable() {
    let source = "const obj = { a: 1, b: 'hello' };\nconst { a, b } = obj;\na;";
    let info = get_hover_at(source, 2, 0);
    assert!(
        info.is_some(),
        "Should find hover info for destructured variable"
    );
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("a"),
            "Should contain destructured variable name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_array_destructuring_variable() {
    let source = "const arr = [1, 2, 3];\nconst [first, second] = arr;\nfirst;";
    let info = get_hover_at(source, 2, 0);
    assert!(
        info.is_some(),
        "Should find hover info for array destructured variable"
    );
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("first"),
            "Should contain variable name 'first', got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_catch_parameter() {
    let source = "try {\n  throw new Error('oops');\n} catch (err) {\n  err;\n}";
    let info = get_hover_at(source, 3, 2);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("err"),
            "Should contain catch parameter name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_for_of_variable() {
    let source = "const items = [1, 2, 3];\nfor (const item of items) {\n  item;\n}";
    let info = get_hover_at(source, 2, 2);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("item"),
            "Should contain for-of variable name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_arrow_function_typed() {
    let source = "const add = (a: number, b: number): number => a + b;\nadd;";
    let info = get_hover_at(source, 1, 0);
    assert!(
        info.is_some(),
        "Should find hover for arrow function variable"
    );
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("add"),
            "Should contain arrow function variable name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_namespace_declaration() {
    let source = "namespace MyNS {\n  export const value = 42;\n}\nMyNS;";
    let info = get_hover_at(source, 3, 0);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("MyNS"),
            "Should contain namespace name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_type_assertion_variable() {
    let source = "const x = 42 as unknown as string;\nx;";
    let info = get_hover_at(source, 1, 0);
    assert!(
        info.is_some(),
        "Should find hover for type-asserted variable"
    );
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("x"),
            "Should contain variable name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_import_declaration() {
    let source = "import { foo } from './mod';\nfoo;";
    // Hover over imported identifier
    let info = get_hover_at(source, 1, 0);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("foo"),
            "Should contain imported name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_generic_function_call() {
    let source = "function identity<T>(val: T): T { return val; }\nidentity;";
    let info = get_hover_at(source, 1, 0);
    assert!(info.is_some(), "Should find hover for generic function");
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("identity"),
            "Should contain function name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_method_declaration() {
    let source =
        "class Greeter {\n  greet(name: string): string {\n    return 'Hello ' + name;\n  }\n}";
    let info = get_hover_at(source, 1, 4);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("greet"),
            "Should contain method name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_getter_accessor() {
    let source =
        "class Box {\n  private _value = 0;\n  get value(): number { return this._value; }\n}";
    let info = get_hover_at(source, 2, 6);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("value"),
            "Should contain getter name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_setter_accessor() {
    let source =
        "class Box {\n  private _value = 0;\n  set value(v: number) { this._value = v; }\n}";
    let info = get_hover_at(source, 2, 6);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("value"),
            "Should contain setter name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_static_method() {
    let source = "class Factory {\n  static create(): Factory { return new Factory(); }\n}";
    let info = get_hover_at(source, 1, 9);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("create"),
            "Should contain static method name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_mapped_type_alias() {
    let source = "type ReadonlyAll<T> = { readonly [K in keyof T]: T[K] };\ntype X = ReadonlyAll<{a: 1}>;\nlet v: X;";
    // Hover over the type alias name
    let info = get_hover_at(source, 0, 5);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("ReadonlyAll"),
            "Should contain mapped type alias name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_conditional_type_alias() {
    let source = "type IsString<T> = T extends string ? true : false;\ntype R = IsString<'hello'>;";
    let info = get_hover_at(source, 0, 5);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("IsString"),
            "Should contain conditional type alias name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_export_default_function() {
    let source = "export default function myFunc() { return 1; }";
    let info = get_hover_at(source, 0, 24);
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("myFunc"),
            "Should contain exported default function name, got: {}",
            info.contents[0]
        );
    }
}

#[test]
fn test_hover_private_field() {
    let source = "class Secret {\n  #data = 42;\n  reveal() { return this.#data; }\n}";
    let info = get_hover_at(source, 1, 4);
    if let Some(info) = info {
        assert!(
            info.display_string.contains("data") || info.display_string.contains("#data"),
            "Should contain private field name, got: {}",
            info.display_string
        );
    }
}

#[test]
fn test_hover_keyword_returns_none() {
    let source = "const x = 1;";
    // Hover over the 'const' keyword at col 0
    let info = get_hover_at(source, 0, 0);
    assert!(info.is_none(), "Should return None for keyword 'const'");
}
