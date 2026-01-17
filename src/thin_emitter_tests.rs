//! Tests for ThinEmitter

use crate::emit_context::EmitContext;
use crate::lowering_pass::LoweringPass;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::solver::TypeInterner;
use crate::thin_binder::ThinBinderState;
use crate::thin_checker::ThinCheckerState;
use crate::thin_emitter::{ModuleKind, PrinterOptions, ScriptTarget, ThinPrinter};
use crate::thin_parser::ThinParserState;
use serde_json::Value;

fn make_printer_with_transforms<'a>(
    parser: &'a ThinParserState,
    root: NodeIndex,
    options: PrinterOptions,
    auto_detect_module: bool,
) -> ThinPrinter<'a> {
    let mut ctx = EmitContext::with_options(options.clone());
    ctx.auto_detect_module = auto_detect_module;
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = ThinPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_auto_detect_module(ctx.auto_detect_module);
    printer
}

fn make_es5_printer<'a>(parser: &'a ThinParserState, root: NodeIndex) -> ThinPrinter<'a> {
    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    make_printer_with_transforms(parser, root, options, false)
}

#[test]
fn test_thin_printer_creation() {
    use crate::parser::thin_node::ThinNodeArena;
    let arena = ThinNodeArena::new();
    let printer = ThinPrinter::new(&arena);
    assert!(printer.get_output().is_empty());
}

#[test]
fn test_thin_emitter_source_map_basic() {
    let source = "let x = 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        mappings.contains(',') || mappings.contains(';'),
        "expected non-trivial mappings, got: {mappings}"
    );
}

#[test]
fn test_thin_emitter_source_map_transform_class() {
    let source = "class Foo { constructor() {} }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        !mappings.is_empty(),
        "expected mappings for transformed output, got empty"
    );
}

#[test]
fn test_thin_emitter_source_map_names() {
    let source = "const x = 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let names = map_value
        .get("names")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        names.iter().any(|name| name.as_str() == Some("x")),
        "expected identifier name in source map: {map_json}"
    );
}

// Note: write() is private, so we can't test it directly.
// The write functionality is tested indirectly through emit tests.

#[test]
fn test_thin_emit_variable_declaration() {
    let source = "let x = 42";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    // Initialize scanner by parsing source file (which calls next_token)
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    // ES5 target emits 'var' instead of 'let'
    assert!(
        output.contains("var"),
        "Expected 'var' in output: {}",
        output
    );
    assert!(output.contains("x"), "Expected 'x' in output: {}", output);
    assert!(output.contains("42"), "Expected '42' in output: {}", output);
}

#[test]
fn test_thin_emit_variable_declaration_esnext() {
    let source = "let x = 42";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("let x"),
        "Expected 'let' in output: {}",
        output
    );
    assert!(
        !output.contains("var x"),
        "Did not expect 'var' in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_function_declaration() {
    let source = "function add(a, b) { return a + b; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function"),
        "Expected 'function' in output: {}",
        output
    );
    assert!(
        output.contains("add"),
        "Expected 'add' in output: {}",
        output
    );
    assert!(
        output.contains("return"),
        "Expected 'return' in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_function_expression() {
    let source = "const fnExpr = function() { return 1; };";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function ("),
        "Expected function expression in output: {}",
        output
    );
    assert!(
        output.contains("{ return 1; }"),
        "Expected single-line return block in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_string_literal_single_quote() {
    let source = "const s = \"hi\";";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.single_quote = true;
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("'hi'"),
        "Expected single-quoted string literal: {}",
        output
    );
}

#[test]
fn test_thin_emit_call_expression() {
    let source = "const result = foo.bar(baz[0]);";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("foo.bar("),
        "Expected property access call in output: {}",
        output
    );
    assert!(
        output.contains("baz[0]"),
        "Expected element access in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_if_statement() {
    let source = "if (x > 0) { y = 1; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(output.contains("if"), "Expected 'if' in output: {}", output);
    assert!(output.contains(">"), "Expected '>' in output: {}", output);
}

#[test]
fn test_thin_emit_while_statement() {
    let source = "while (x < 10) { x++; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("while ("),
        "Expected 'while' in output: {}",
        output
    );
    assert!(
        output.contains("x++"),
        "Expected increment in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_switch_statement() {
    let source = "switch (x) { case 1: y(); break; default: z(); }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("switch (x)"),
        "Expected switch in output: {}",
        output
    );
    assert!(
        output.contains("case 1:"),
        "Expected case clause in output: {}",
        output
    );
    assert!(
        output.contains("default:"),
        "Expected default clause in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_for_of_es5() {
    let source = "for (var v of arr) { v; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__values(arr)"),
        "Expected __values helper usage in ES5 output: {}",
        output
    );
    assert!(
        output.contains("var v ="),
        "Expected loop binding in ES5 output: {}",
        output
    );
    assert!(
        output.contains(".return"),
        "Expected iterator closing in ES5 output: {}",
        output
    );
    assert!(
        !output.contains("for (var v of arr)"),
        "ES5 output should not contain raw for-of: {}",
        output
    );
}

#[test]
fn test_thin_emit_class_declaration() {
    let source = "class Foo { }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("class"),
        "Expected 'class' in output: {}",
        output
    );
    assert!(
        output.contains("Foo"),
        "Expected 'Foo' in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_class_extends_es6() {
    let source = "class Derived extends Base<T> {}";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("class Derived extends Base"),
        "Expected 'class Derived extends Base' in ES6 output: {}",
        output
    );
    assert!(
        !output.contains("<T>"),
        "ES6 output should erase type arguments: {}",
        output
    );
}

#[test]
fn test_thin_emit_class_method_destructured_param_es5() {
    let source = "class Foo { method({ x }) { return x; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_a)"),
        "Expected temp parameter in ES5 output: {}",
        output
    );
    assert!(
        output.contains("var x = _a.x"),
        "Expected destructuring assignment in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_function_destructured_param_es5() {
    let source = "function foo({ x }) { return x; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function foo(_a)"),
        "Expected temp parameter in ES5 output: {}",
        output
    );
    assert!(
        output.contains("var x = _a.x"),
        "Expected destructuring assignment in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_function_default_param_es5() {
    let source = "function foo(a = 1) { return a; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function foo(a)"),
        "Expected default parameter to be removed from signature: {}",
        output
    );
    assert!(
        output.contains("if (a === void 0) { a = 1; }"),
        "Expected default parameter assignment in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_function_destructured_default_param_es5() {
    let source = "function foo({ x } = {}, y = x) { return y; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    let default_idx = output.find("if (_a === void 0) { _a = {}; }");
    let destructure_idx = output.find("var x = _a.x");
    let y_default_idx = output.find("if (y === void 0) { y = x; }");
    assert!(
        default_idx.is_some() && destructure_idx.is_some() && y_default_idx.is_some(),
        "Expected default/destructure/default sequence in ES5 output: {}",
        output
    );
    assert!(
        default_idx.unwrap() < destructure_idx.unwrap(),
        "Expected destructuring to follow parameter default: {}",
        output
    );
    assert!(
        destructure_idx.unwrap() < y_default_idx.unwrap(),
        "Expected second default to follow destructuring: {}",
        output
    );
}

#[test]
fn test_thin_emit_function_destructured_binding_default_es5() {
    let source = "function foo({ x = 1 }) { return x; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_b = _a.x"),
        "Expected temp binding value for defaulted destructure: {}",
        output
    );
    assert!(
        output.contains("x = _b === void 0 ? 1 : _b"),
        "Expected default value assignment for binding element: {}",
        output
    );
}

#[test]
fn test_thin_emit_function_rest_param_es5() {
    let source = "function foo(a, ...rest) { return rest.length; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function foo(a)"),
        "Expected rest parameter to be removed from signature: {}",
        output
    );
    assert!(
        output.contains("var rest = []"),
        "Expected rest parameter array initialization in ES5 output: {}",
        output
    );
    assert!(
        output.contains("arguments.length"),
        "Expected rest parameter loop over arguments in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_class_method_default_param_es5() {
    let source = "class Foo { method(a = 1) { return a; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("prototype.method"),
        "Expected ES5 prototype method emit: {}",
        output
    );
    assert!(
        output.contains("if (a === void 0) { a = 1; }"),
        "Expected default parameter assignment in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_class_method_nested_destructured_param_es5() {
    let source = "class Foo { method({ a: { b } }) { return b; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_b = _a.a"),
        "Expected temp value for nested param binding: {}",
        output
    );
    assert!(
        output.contains("b = _b.b"),
        "Expected nested param binding assignment: {}",
        output
    );
}

#[test]
fn test_thin_emit_class_method_rest_param_es5() {
    let source = "class Foo { method(...rest) { return rest.length; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("prototype.method"),
        "Expected ES5 prototype method emit: {}",
        output
    );
    assert!(
        output.contains("var rest = []"),
        "Expected rest parameter array initialization in ES5 output: {}",
        output
    );
    assert!(
        output.contains("arguments.length"),
        "Expected rest parameter loop over arguments in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_object_rest_destructuring_es5() {
    let source = "let { x, ...rest } = obj;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var __rest"),
        "Expected __rest helper in ES5 output: {}",
        output
    );
    assert!(
        output.contains("rest = __rest(_a, [\"x\"])"),
        "Expected object rest destructuring in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_array_rest_destructuring_es5() {
    let source = "let [x, ...rest] = arr;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("rest = _a.slice(1)"),
        "Expected array rest destructuring in ES5 output: {}",
        output
    );
    assert!(
        !output.contains("__rest"),
        "Array rest should not require __rest helper: {}",
        output
    );
}

#[test]
fn test_thin_emit_object_literal_computed_shorthand_es5() {
    let source = "const key = 1; const a = 2; const obj = { [key]: a, a };";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("[key] = a"),
        "Expected computed property assignment in ES5 output: {}",
        output
    );
    assert!(
        output.contains(".a = a"),
        "Expected shorthand property assignment in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_object_destructuring_default_es5() {
    let source = "let { x = 1 } = obj;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_b = _a.x"),
        "Expected temp value for defaulted object binding: {}",
        output
    );
    assert!(
        output.contains("x = _b === void 0 ? 1 : _b"),
        "Expected default value assignment for object binding: {}",
        output
    );
}

#[test]
fn test_thin_emit_array_destructuring_default_es5() {
    let source = "let [x = 1] = arr;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_b = _a[0]"),
        "Expected temp value for defaulted array binding: {}",
        output
    );
    assert!(
        output.contains("x = _b === void 0 ? 1 : _b"),
        "Expected default value assignment for array binding: {}",
        output
    );
}

#[test]
fn test_thin_emit_object_nested_destructuring_es5() {
    let source = "let { a: { b } } = obj;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_b = _a.a"),
        "Expected temp value for nested object binding: {}",
        output
    );
    assert!(
        output.contains("b = _b.b"),
        "Expected nested object binding assignment: {}",
        output
    );
}

#[test]
fn test_thin_emit_object_spread_es5() {
    let source = "let o = { ...a, b: 1 };";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("Object.assign("),
        "Expected Object.assign in ES5 output for object spread: {}",
        output
    );
    assert!(
        !output.contains("...a"),
        "ES5 output should not contain object spread syntax: {}",
        output
    );
}

#[test]
fn test_thin_emit_array_nested_destructuring_es5() {
    let source = "let [[x]] = arr;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_b = _a[0]"),
        "Expected temp value for nested array binding: {}",
        output
    );
    assert!(
        output.contains("x = _b[0]"),
        "Expected nested array binding assignment: {}",
        output
    );
}

#[test]
fn test_thin_emit_arrow_function() {
    let source = "let f = (x) => x * 2";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    // ES5 emit: arrow functions become regular function expressions
    assert!(
        output.contains("function"),
        "Expected 'function' in ES5 output: {}",
        output
    );
    assert!(
        output.contains("return x * 2"),
        "Expected 'return x * 2' in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_interface_declaration() {
    // Interface declarations are TypeScript-only, so JavaScript emit should be empty
    let source = "interface Point { x: number; y: number; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    // For JavaScript emit, interface should NOT be in output
    assert!(
        !output.contains("interface"),
        "JavaScript output should NOT contain 'interface': {}",
        output
    );
}

#[test]
fn test_thin_emit_type_alias_declaration() {
    // Type alias declarations are TypeScript-only, so JavaScript emit should be empty
    let source = "type Alias = { x: number };";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("type"),
        "JavaScript output should NOT contain 'type': {}",
        output
    );
}

#[test]
fn test_thin_emit_union_type() {
    let source = "type Value = string | number;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = &parser.arena;
    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");

    let mut type_node = None;
    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            let alias = arena
                .get_type_alias(stmt_node)
                .expect("expected type alias data");
            type_node = Some(alias.type_node);
            break;
        }
    }

    let type_node = type_node.expect("expected type alias type node");
    let mut printer = ThinPrinter::new(arena);
    printer.emit(type_node);

    let output = printer.get_output();
    assert!(
        output.contains("string | number"),
        "Expected union type in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_element() {
    let source = "const x = <div className=\"foo\">{bar}</div>;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<div"),
        "Expected JSX element in output: {}",
        output
    );
    assert!(
        output.contains("className=\"foo\""),
        "Expected JSX attribute in output: {}",
        output
    );
    assert!(
        output.contains("{bar}"),
        "Expected JSX expression in output: {}",
        output
    );
    assert!(
        output.contains("</div>"),
        "Expected JSX closing tag in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_fragment_spread() {
    let source = "const x = <><Component {...props} /></>;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<>"),
        "Expected JSX fragment open in output: {}",
        output
    );
    assert!(
        output.contains("</>"),
        "Expected JSX fragment close in output: {}",
        output
    );
    assert!(
        output.contains("{...props}"),
        "Expected JSX spread attribute in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_namespaced_member_expression() {
    let source = "const a = <svg:rect width={100} />; const b = <Foo.Bar.Baz />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<svg:rect"),
        "Expected JSX namespaced tag in output: {}",
        output
    );
    assert!(
        output.contains("<Foo.Bar.Baz"),
        "Expected JSX member tag in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_namespaced_attribute() {
    let source = "const x = <svg xlink:href=\"path\" />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<svg"),
        "Expected JSX element in output: {}",
        output
    );
    assert!(
        output.contains("xlink:href=\"path\""),
        "Expected JSX namespaced attribute in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_namespaced_attribute_expression() {
    let source = "const x = <svg xlink:href={url} />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<svg"),
        "Expected JSX element in output: {}",
        output
    );
    assert!(
        output.contains("xlink:href={url}"),
        "Expected JSX namespaced attribute expression in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_spread_attribute() {
    let source = "const x = <div {...props} dataId={id} />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<div"),
        "Expected JSX element in output: {}",
        output
    );
    assert!(
        output.contains("{...props}"),
        "Expected JSX spread attribute in output: {}",
        output
    );
    assert!(
        output.contains("dataId={id}"),
        "Expected JSX attribute expression in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_hyphenated_attribute() {
    let source = "const x = <div data-id={id} />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<div"),
        "Expected JSX element in output: {}",
        output
    );
    assert!(
        output.contains("data-id={id}"),
        "Expected JSX hyphenated attribute in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_hyphenated_element_name() {
    let source = "const x = <my-widget />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<my-widget"),
        "Expected JSX hyphenated element name in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_hyphenated_element_with_attribute() {
    let source = "const x = <my-widget data-id={id} />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<my-widget"),
        "Expected JSX hyphenated element name in output: {}",
        output
    );
    assert!(
        output.contains("data-id={id}"),
        "Expected JSX hyphenated attribute in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_member_element_with_attribute() {
    let source = "const x = <Foo.Bar baz=\"ok\" />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<Foo.Bar"),
        "Expected JSX member element in output: {}",
        output
    );
    assert!(
        output.contains("baz=\"ok\""),
        "Expected JSX attribute on member element in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_member_element_hyphenated_attribute() {
    let source = "const x = <Foo.Bar data-id={id} />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<Foo.Bar"),
        "Expected JSX member element in output: {}",
        output
    );
    assert!(
        output.contains("data-id={id}"),
        "Expected JSX hyphenated attribute on member element in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_namespaced_attribute_string_literal() {
    let source = "const x = <svg xlink:href=\"#id\" />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("xlink:href=\"#id\""),
        "Expected JSX namespaced attribute string literal in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_member_element_namespaced_attribute() {
    let source = "const x = <Foo.Bar xlink:href=\"#id\" />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<Foo.Bar"),
        "Expected JSX member element in output: {}",
        output
    );
    assert!(
        output.contains("xlink:href=\"#id\""),
        "Expected JSX namespaced attribute on member element in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_member_namespaced_attribute_expression() {
    let source = "const x = <Foo.Bar xlink:href={url} />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<Foo.Bar"),
        "Expected JSX member element in output: {}",
        output
    );
    assert!(
        output.contains("xlink:href={url}"),
        "Expected JSX namespaced attribute expression on member element in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_boolean_attribute() {
    let source = "const x = <input disabled />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<input"),
        "Expected JSX element in output: {}",
        output
    );
    assert!(
        output.contains("disabled"),
        "Expected JSX boolean attribute in output: {}",
        output
    );
    assert!(
        !output.contains("disabled="),
        "Expected JSX boolean attribute without initializer: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_member_boolean_attribute() {
    let source = "const x = <Foo.Bar disabled />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<Foo.Bar"),
        "Expected JSX member element in output: {}",
        output
    );
    assert!(
        output.contains("disabled"),
        "Expected JSX boolean attribute on member element in output: {}",
        output
    );
    assert!(
        !output.contains("disabled="),
        "Expected JSX boolean attribute without initializer: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_spread_and_boolean_attribute() {
    let source = "const x = <input {...props} disabled />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("{...props}"),
        "Expected JSX spread attribute in output: {}",
        output
    );
    assert!(
        output.contains("disabled"),
        "Expected JSX boolean attribute in output: {}",
        output
    );
    assert!(
        !output.contains("disabled="),
        "Expected JSX boolean attribute without initializer: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_namespaced_and_boolean_attributes() {
    let source = "const x = <svg xlink:href=\"#id\" focusable />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("xlink:href=\"#id\""),
        "Expected JSX namespaced attribute in output: {}",
        output
    );
    assert!(
        output.contains("focusable"),
        "Expected JSX boolean attribute in output: {}",
        output
    );
    assert!(
        !output.contains("focusable="),
        "Expected JSX boolean attribute without initializer: {}",
        output
    );
}

#[test]
fn test_thin_emit_jsx_member_spread_and_boolean_attribute() {
    let source = "const x = <Foo.Bar {...props} disabled />;";
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("<Foo.Bar"),
        "Expected JSX member element in output: {}",
        output
    );
    assert!(
        output.contains("{...props}"),
        "Expected JSX spread attribute in output: {}",
        output
    );
    assert!(
        output.contains("disabled"),
        "Expected JSX boolean attribute in output: {}",
        output
    );
    assert!(
        !output.contains("disabled="),
        "Expected JSX boolean attribute without initializer: {}",
        output
    );
}

#[test]
fn test_thin_emit_enum_declaration() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // ES5 target transforms enum to IIFE
    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    // ES5 output should be IIFE pattern, not raw 'enum' keyword
    assert!(
        output.contains("var Color;"),
        "Expected 'var Color;' in ES5 output: {}",
        output
    );
    assert!(
        output.contains("(function (Color)"),
        "Expected IIFE pattern in ES5 output: {}",
        output
    );
    assert!(
        output.contains("Color[Color[\"Red\"]"),
        "Expected reverse mapping for Red in ES5 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_enum_declaration_es6() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // ES6 mode should preserve the enum keyword (TypeScript style)
    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("enum"),
        "Expected 'enum' in ES6 output: {}",
        output
    );
    assert!(
        output.contains("Color"),
        "Expected 'Color' in ES6 output: {}",
        output
    );
    assert!(
        output.contains("Red"),
        "Expected 'Red' in ES6 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_const_enum_erased_es6() {
    let source = "const enum CE { A = 0 }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.trim().is_empty(),
        "Const enums should be erased: {}",
        output
    );
}

#[test]
fn test_thin_emit_declare_enum_erased() {
    let source = "declare enum E { A }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.trim().is_empty(),
        "Declare enums should be erased: {}",
        output
    );
}

/// Full ThinNode pipeline integration test:
/// ThinParser → ThinBinder → ThinChecker → ThinEmitter
#[test]
fn test_thin_pipeline_integration() {
    let source = r#"
        function add(a: number, b: number): number {
            return a + b;
        }
        let result = add(1, 2);
    "#;

    // Step 1: Parse
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let source_file = parser.parse_source_file();
    assert!(!source_file.is_none(), "Source file should be parsed");
    let root = source_file;

    // Step 2: Bind
    let mut binder = ThinBinderState::new();
    binder.bind_source_file(&parser.arena, source_file);
    // Verify symbols were created
    let symbol_count = binder.symbols.len();
    assert!(
        symbol_count >= 2,
        "Expected at least 2 symbols (add, result), got {}",
        symbol_count
    );

    // Step 3: Check (type inference)
    let types = TypeInterner::new();
    let checker =
        ThinCheckerState::new(&parser.arena, &binder, &types, "test.ts".to_string(), false);
    // Basic check - the checker exists and can be created
    let _ = &checker.ctx.types; // Access types arena to verify it exists

    // Step 4: Emit (ES5)
    let mut printer = make_es5_printer(&parser, root);
    printer.emit(source_file);

    let output = printer.get_output();
    assert!(
        output.contains("function"),
        "Output should contain 'function': {}",
        output
    );
    assert!(
        output.contains("add"),
        "Output should contain 'add': {}",
        output
    );
    // JavaScript emit strips types, so "number" should NOT be in output
    assert!(
        !output.contains("number"),
        "JavaScript output should NOT contain 'number' (types are stripped): {}",
        output
    );
    assert!(
        output.contains("return"),
        "Output should contain 'return': {}",
        output
    );
    // ES5 target emits 'var' instead of 'let'
    assert!(
        output.contains("var"),
        "Output should contain 'var': {}",
        output
    );
    assert!(
        output.contains("result"),
        "Output should contain 'result': {}",
        output
    );
}

#[test]
fn test_thin_emit_import() {
    let source = r#"import { foo, bar } from "module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("import"),
        "Output should contain 'import': {}",
        output
    );
    assert!(
        output.contains("foo"),
        "Output should contain 'foo': {}",
        output
    );
    assert!(
        output.contains("from"),
        "Output should contain 'from': {}",
        output
    );
}

#[test]
fn test_thin_emit_namespace_import_es6() {
    let source = r#"import * as ns from "module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("import * as ns from \"module\";"),
        "Output should contain namespace import: {}",
        output
    );
}

#[test]
fn test_thin_emit_import_type_only_erased() {
    let source = r#"import type { Foo } from "module"; const x = 1;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("import"),
        "Type-only imports should be erased: {}",
        output
    );
    assert!(
        output.contains("x = 1"),
        "Output should retain value statement: {}",
        output
    );
}

#[test]
fn test_thin_emit_import_type_specifier_filtered() {
    let source = r#"import { type Foo, bar } from "module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("import { bar } from \"module\";"),
        "Output should keep value imports only: {}",
        output
    );
    assert!(
        !output.contains("Foo"),
        "Type-only specifier should be omitted: {}",
        output
    );
}

#[test]
fn test_thin_emit_import_equals_external() {
    let source = r#"import Foo = require("./bar"); Foo;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var Foo = require(\"./bar\")"),
        "Expected import equals require emission: {}",
        output
    );
}

#[test]
fn test_thin_emit_import_equals_internal() {
    let source = "import Foo = Bar.Baz; Foo;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var Foo = Bar.Baz"),
        "Expected import equals internal alias emission: {}",
        output
    );
}

#[test]
fn test_thin_emit_export() {
    let source = "export function greet() { return 'hello'; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("export"),
        "Output should contain 'export': {}",
        output
    );
    assert!(
        output.contains("function"),
        "Output should contain 'function': {}",
        output
    );
    assert!(
        output.contains("greet"),
        "Output should contain 'greet': {}",
        output
    );
}

#[test]
fn test_thin_emit_export_default() {
    let source = "export default function () { return 1; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("export default"),
        "Output should contain 'export default': {}",
        output
    );
    assert!(
        output.contains("function"),
        "Output should contain 'function': {}",
        output
    );
}

#[test]
fn test_thin_emit_export_import_equals_es6() {
    let source = r#"export import Foo = require("./bar");"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("export var Foo = require(\"./bar\");"),
        "Expected export import equals in ES6 output: {}",
        output
    );
}

#[test]
fn test_thin_emit_export_type_only_erased() {
    let source = r#"export type { Foo } from "module"; const x = 1;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("export"),
        "Type-only exports should be erased: {}",
        output
    );
    assert!(
        output.contains("x = 1"),
        "Output should retain value statement: {}",
        output
    );
}

#[test]
fn test_thin_emit_export_type_specifier_filtered() {
    let source = r#"export { type Foo, bar } from "module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new_es6(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("export { bar } from \"module\";"),
        "Output should keep value exports only: {}",
        output
    );
    assert!(
        !output.contains("Foo"),
        "Type-only specifier should be omitted: {}",
        output
    );
}

#[test]
fn test_thin_emit_get_accessor() {
    let source = "class Foo { get value() { return this._value; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("get"),
        "Output should contain 'get': {}",
        output
    );
    assert!(
        output.contains("value"),
        "Output should contain 'value': {}",
        output
    );
}

#[test]
fn test_thin_emit_set_accessor() {
    let source = "class Foo { set value(v) { this._value = v; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("set"),
        "Output should contain 'set': {}",
        output
    );
    assert!(
        output.contains("value"),
        "Output should contain 'value': {}",
        output
    );
}

#[test]
fn test_thin_emit_decorator() {
    let source = "@Component class MyComponent {}";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("@"),
        "Output should contain '@': {}",
        output
    );
    assert!(
        output.contains("Component"),
        "Output should contain 'Component': {}",
        output
    );
    assert!(
        output.contains("class"),
        "Output should contain 'class': {}",
        output
    );
}

#[test]
fn test_thin_emit_static_property() {
    let source = "class Foo { static count = 0; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    // ES5 emit: static properties become ClassName.propName = value;
    assert!(
        output.contains("Foo.count = 0"),
        "ES5 output should contain 'Foo.count = 0': {}",
        output
    );
}

#[test]
fn test_thin_emit_private_method() {
    // For JavaScript emit, 'private' modifier is stripped
    let source = "class Foo { private doSomething(): void {} }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("private"),
        "JavaScript output should NOT contain 'private': {}",
        output
    );
    assert!(
        output.contains("doSomething"),
        "Output should contain 'doSomething': {}",
        output
    );
}

#[test]
fn test_thin_emit_static_readonly() {
    // For JavaScript emit, 'readonly' modifier is stripped and class becomes IIFE
    let source = "class Foo { static readonly MAX = 100; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    // ES5 emit: static properties become ClassName.propName = value;
    assert!(
        output.contains("Foo.MAX = 100"),
        "ES5 output should contain 'Foo.MAX = 100': {}",
        output
    );
    assert!(
        !output.contains("readonly"),
        "JavaScript output should NOT contain 'readonly': {}",
        output
    );
}

#[test]
fn test_thin_emit_protected_constructor() {
    let source = "class Singleton { protected constructor() {} }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    // ES5 emit: classes become IIFEs, protected is stripped
    assert!(
        !output.contains("protected"),
        "ES5 output should NOT contain 'protected': {}",
        output
    );
    assert!(
        output.contains("function Singleton"),
        "ES5 output should contain constructor function: {}",
        output
    );
}

#[test]
fn test_thin_emit_static_get_accessor() {
    let source = "class Foo { static get instance(): Foo { return null; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = make_es5_printer(&parser, root);
    printer.emit(root);

    let output = printer.get_output();
    // ES5 emit: class becomes IIFE (static accessors may be handled differently)
    // For now, just verify the class wrapper is emitted
    assert!(
        output.contains("var Foo"),
        "ES5 output should contain 'var Foo': {}",
        output
    );
    assert!(
        output.contains("function Foo"),
        "ES5 output should contain 'function Foo': {}",
        output
    );
}

#[test]
fn test_thin_emit_call_signature() {
    // Interfaces are TypeScript-only, so JavaScript emit should be empty
    let source = "interface Callable { (): string; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    // For JavaScript emit, interface should NOT be in output
    assert!(
        !output.contains("interface"),
        "JavaScript output should NOT contain 'interface': {}",
        output
    );
}

#[test]
fn test_thin_emit_construct_signature() {
    // Interfaces are TypeScript-only, so JavaScript emit should be empty
    let source = "interface Factory { new (): MyClass; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    // For JavaScript emit, interface should NOT be in output
    assert!(
        !output.contains("interface"),
        "JavaScript output should NOT contain 'interface': {}",
        output
    );
}

#[test]
fn test_thin_emit_generic_call_construct_signatures() {
    let source = "interface Factory { <T>(value: T): T; new <T>(value: T): Factory<T>; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = &parser.arena;
    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");

    let mut call_sig = None;
    let mut construct_sig = None;

    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
            continue;
        }
        let iface = arena
            .get_interface(stmt_node)
            .expect("expected interface data");
        for &member_idx in &iface.members.nodes {
            let Some(member_node) = arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CALL_SIGNATURE {
                call_sig = Some(member_idx);
            } else if member_node.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE {
                construct_sig = Some(member_idx);
            }
        }
    }

    let call_sig = call_sig.expect("expected call signature");
    let construct_sig = construct_sig.expect("expected construct signature");

    let mut printer = ThinPrinter::new(arena);
    printer.emit(call_sig);
    let output = printer.get_output();
    assert!(
        output.contains("<T>("),
        "Expected call signature type params in output: {}",
        output
    );
    assert!(
        output.contains("value: T"),
        "Expected call signature parameter types in output: {}",
        output
    );
    assert!(
        output.contains("): T"),
        "Expected call signature return type in output: {}",
        output
    );

    let mut printer = ThinPrinter::new(arena);
    printer.emit(construct_sig);
    let output = printer.get_output();
    assert!(
        output.contains("new <T>("),
        "Expected construct signature type params in output: {}",
        output
    );
    assert!(
        output.contains("Factory<T>"),
        "Expected construct signature return type in output: {}",
        output
    );
}

#[test]
fn test_thin_emit_readonly_property_signature() {
    // Interfaces are TypeScript-only, so JavaScript emit should be empty
    let source = "interface Config { readonly name: string; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    // For JavaScript emit, interface should NOT be in output
    assert!(
        !output.contains("interface"),
        "JavaScript output should NOT contain 'interface': {}",
        output
    );
    assert!(
        !output.contains("readonly"),
        "JavaScript output should NOT contain 'readonly': {}",
        output
    );
}

#[test]
fn test_thin_emit_readonly_index_signature() {
    // Interfaces are TypeScript-only, so JavaScript emit should be empty
    let source = "interface ReadonlyMap { readonly [key: string]: number; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.emit(root);

    let output = printer.get_output();
    // For JavaScript emit, interface should NOT be in output
    assert!(
        !output.contains("interface"),
        "JavaScript output should NOT contain 'interface': {}",
        output
    );
    assert!(
        !output.contains("readonly"),
        "JavaScript output should NOT contain 'readonly': {}",
        output
    );
}

// =============================================================================
// CommonJS Module Tests
// =============================================================================

#[test]
fn test_commonjs_preamble() {
    let source = "export const x = 42;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("\"use strict\";"),
        "Expected 'use strict' in CommonJS output: {}",
        output
    );
    // TypeScript doesn't emit __esModule in its baseline format, so we don't either
    // Just verify the preamble is there with exports init
    assert!(
        output.contains("exports.x"),
        "Expected exports.x in CommonJS output: {}",
        output
    );
}

// =============================================================================
// Auto-Detect Module Tests
// =============================================================================

#[test]
fn test_auto_detect_skips_type_only_imports() {
    let source = r#"import type { Foo } from "./types"; const x = 1;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::with_options(&parser.arena, PrinterOptions::default());
    printer.set_auto_detect_module(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("\"use strict\";"),
        "Type-only imports should not trigger CommonJS preamble: {}",
        output
    );
    assert!(
        !output.contains("exports."),
        "Type-only imports should not emit exports assignments: {}",
        output
    );
    assert!(
        output.contains("const x = 1"),
        "Expected value statement in output: {}",
        output
    );
}

#[test]
fn test_auto_detect_skips_type_only_exports() {
    let source = r#"export type { Foo } from "./types"; const x = 1;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::with_options(&parser.arena, PrinterOptions::default());
    printer.set_auto_detect_module(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("\"use strict\";"),
        "Type-only exports should not trigger CommonJS preamble: {}",
        output
    );
    assert!(
        !output.contains("exports."),
        "Type-only exports should not emit exports assignments: {}",
        output
    );
    assert!(
        output.contains("const x = 1"),
        "Expected value statement in output: {}",
        output
    );
}

#[test]
fn test_auto_detect_skips_internal_import_equals() {
    let source = "import Foo = Bar.Baz; const x = 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::with_options(&parser.arena, PrinterOptions::default());
    printer.set_auto_detect_module(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("\"use strict\";"),
        "Internal import equals should not trigger CommonJS preamble: {}",
        output
    );
    assert!(
        !output.contains("exports."),
        "Internal import equals should not emit exports assignments: {}",
        output
    );
    assert!(
        output.contains("var Foo = Bar.Baz"),
        "Expected internal import equals emission: {}",
        output
    );
}

#[test]
fn test_commonjs_import_named() {
    let source = r#"import { foo, bar } from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("require(\"./module\")"),
        "Expected require() in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("var foo = module_1.foo;"),
        "Expected foo binding in output: {}",
        output
    );
    assert!(
        output.contains("var bar = module_1.bar;"),
        "Expected bar binding in output: {}",
        output
    );
}

#[test]
fn test_commonjs_import_type_only_is_erased() {
    let source = r#"import type { Foo } from "./module"; const x = 1;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("require(\"./module\")"),
        "Type-only import should not emit require: {}",
        output
    );
    assert!(
        output.contains("const x = 1"),
        "Expected value statement in output: {}",
        output
    );
}

#[test]
fn test_commonjs_import_side_effect() {
    let source = r#"import "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("require(\"./module\");"),
        "Expected side-effect require in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_assignment_skips_esmodule_marker() {
    let source = r#"import "./module"; const foo = 1; export = foo;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("module.exports = foo"),
        "Expected export assignment in CommonJS output: {}",
        output
    );
    assert!(
        !output.contains("__esModule"),
        "Export assignment should suppress __esModule marker: {}",
        output
    );
}

#[test]
fn test_commonjs_import_namespace() {
    let source = r#"import * as ns from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("require(\"./module\")"),
        "Expected require() in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("var ns = __importStar(module_1);"),
        "Expected namespace binding in output: {}",
        output
    );
}

#[test]
fn test_commonjs_type_only_namespace_import_is_erased() {
    let source = r#"import type * as ns from "./module"; const x = 1;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("require(\"./module\")"),
        "Type-only namespace import should not emit require: {}",
        output
    );
    assert!(
        !output.contains("__importStar"),
        "Type-only namespace import should not emit helpers: {}",
        output
    );
    assert!(
        output.contains("const x = 1"),
        "Expected value statement in output: {}",
        output
    );
}

#[test]
fn test_commonjs_import_namespace_emits_helpers() {
    let source = r#"import * as ns from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var __createBinding"),
        "Expected __createBinding helper in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("var __setModuleDefault"),
        "Expected __setModuleDefault helper in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("var __importStar"),
        "Expected __importStar helper in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_import_namespace_helper_ordering() {
    let source = r#"import * as ns from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    let create_binding_pos = output
        .find("var __createBinding")
        .expect("Expected __createBinding helper");
    let set_module_default_pos = output
        .find("var __setModuleDefault")
        .expect("Expected __setModuleDefault helper");
    let import_star_pos = output
        .find("var __importStar")
        .expect("Expected __importStar helper");
    assert!(
        create_binding_pos < set_module_default_pos,
        "__createBinding should precede __setModuleDefault"
    );
    assert!(
        set_module_default_pos < import_star_pos,
        "__setModuleDefault should precede __importStar"
    );
}

#[test]
fn test_commonjs_import_default() {
    let source = r#"import myDefault from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("require(\"./module\")"),
        "Expected require() in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("var myDefault = module_1.default;"),
        "Expected default binding in output: {}",
        output
    );
}

#[test]
fn test_commonjs_reexport() {
    let source = r#"export { foo } from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("require(\"./module\")"),
        "Expected require() in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("Object.defineProperty(exports, \"foo\""),
        "Expected Object.defineProperty for re-export: {}",
        output
    );
}

#[test]
fn test_commonjs_export_type_only_reexport_is_erased() {
    let source = r#"export type { Foo } from "./module"; const x = 1;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("require(\"./module\")"),
        "Type-only re-export should not emit require: {}",
        output
    );
    assert!(
        !output.contains("Object.defineProperty(exports, \"Foo\""),
        "Type-only re-export should not emit exports: {}",
        output
    );
    assert!(
        output.contains("const x = 1"),
        "Expected value statement in output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_star() {
    let source = r#"export * from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("require(\"./module\")"),
        "Expected require() in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("__exportStar("),
        "Expected __exportStar call in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_star_emits_helpers() {
    let source = r#"export * from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var __createBinding"),
        "Expected __createBinding helper in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("var __exportStar"),
        "Expected __exportStar helper in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_star_helper_ordering() {
    let source = r#"export * from "./module";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    let create_binding_pos = output
        .find("var __createBinding")
        .expect("Expected __createBinding helper");
    let export_star_pos = output
        .find("var __exportStar")
        .expect("Expected __exportStar helper");
    assert!(
        create_binding_pos < export_star_pos,
        "__createBinding should precede __exportStar"
    );
}

#[test]
fn test_commonjs_helpers_before_esmodule_marker() {
    let source = r#"import * as ns from "./module"; export const x = 1;"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    let helper_pos = output
        .find("var __createBinding")
        .expect("Expected __createBinding helper");
    let esmodule_pos = output
        .find("__esModule")
        .expect("Expected __esModule marker");
    assert!(
        helper_pos < esmodule_pos,
        "Helpers should be emitted before __esModule marker"
    );
}

#[test]
fn test_commonjs_esmodule_marker_before_exports_init() {
    let source = "export const x = 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    let esmodule_pos = output
        .find("__esModule")
        .expect("Expected __esModule marker");
    let exports_init_pos = output
        .find("exports.x = void 0")
        .expect("Expected exports initialization");
    assert!(
        esmodule_pos < exports_init_pos,
        "__esModule marker should precede exports initialization"
    );
}

#[test]
fn test_commonjs_export_const() {
    let source = "export const x = 42;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("const x = 42;"),
        "Expected 'const x = 42;' in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.x = x;"),
        "Expected 'exports.x = x;' in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_const_destructuring() {
    let source = "export const { a, b: c } = obj;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.a = exports.c = void 0;"),
        "Expected CommonJS exports init for destructured names: {}",
        output
    );
    assert!(
        output.contains("exports.a = a;"),
        "Expected 'exports.a = a;' in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.c = c;"),
        "Expected 'exports.c = c;' in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_default_function() {
    let source = "export default function foo() { return 1; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.default ="),
        "Expected default export assignment in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("function foo"),
        "Expected default function declaration in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_default_expression() {
    let source = "export default 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.default = 1;"),
        "Expected default export assignment in CommonJS output: {}",
        output
    );
    assert!(
        !output.contains("export default"),
        "CommonJS output should not contain ES module syntax: {}",
        output
    );
}

#[test]
fn test_commonjs_export_default_arrow() {
    let source = "export default () => 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.default ="),
        "Expected default export assignment in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("=>"),
        "Expected arrow function emit in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_function() {
    let source = "export function add(a, b) { return a + b; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("function add"),
        "Expected 'function add' in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.add = add;"),
        "Expected 'exports.add = add;' in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_import_equals() {
    let source = r#"export import Foo = require("./bar");"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.Foo = void 0;"),
        "Expected exports preamble for import equals: {}",
        output
    );
    assert!(
        output.contains("var Foo = require(\"./bar\")"),
        "Expected import equals require in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.Foo = Foo;"),
        "Expected 'exports.Foo = Foo;' in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_class() {
    let source = "export class Foo { }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    // ES5 emits class as IIFE, so check for var Foo
    assert!(
        output.contains("var Foo") || output.contains("class Foo"),
        "Expected class Foo definition in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.Foo = Foo;"),
        "Expected 'exports.Foo = Foo;' in CommonJS output: {}",
        output
    );
}

#[test]
fn test_commonjs_export_namespace() {
    let source = "export namespace N { export function foo() { return 1; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__esModule"),
        "Expected __esModule marker in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("(function (N)") || output.contains("namespace N"),
        "Expected namespace emit in CommonJS output: {}",
        output
    );
    assert!(
        output.contains("exports.N = N;"),
        "Expected 'exports.N = N;' in CommonJS output: {}",
        output
    );
}

// =============================================================================
// Module Wrapper Tests (Legacy API auto-lowering)
// =============================================================================

#[test]
fn test_legacy_amd_wrapper() {
    let source = "import { foo } from \"./bar\"; export const x = foo;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("define([\"require\", \"exports\", \"./bar\"]"),
        "Expected AMD wrapper with dependency list: {}",
        output
    );
    assert!(
        output.contains("function (require, exports, bar)"),
        "Expected AMD factory signature: {}",
        output
    );
}

#[test]
fn test_legacy_umd_wrapper() {
    let source = "import { foo } from \"./bar\"; export const x = foo;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::UMD,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("(function (factory) {"),
        "Expected UMD wrapper header: {}",
        output
    );
    assert!(
        output.contains("define([\"require\", \"exports\"], factory);"),
        "Expected UMD AMD path: {}",
        output
    );
}

#[test]
fn test_legacy_system_wrapper() {
    let source = "import { foo } from \"./bar\"; export const x = foo;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("System.register([\"./bar\"]"),
        "Expected System.register wrapper with dependency list: {}",
        output
    );
    assert!(
        output.contains("execute: function () {"),
        "Expected System.register execute block: {}",
        output
    );
}

#[test]
fn test_legacy_amd_wrapper_skips_pure_type_only_module() {
    let source = r#"import type { Foo } from "./types";"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.trim().is_empty(),
        "Type-only modules should not emit AMD wrappers: {}",
        output
    );
}

#[test]
fn test_legacy_amd_wrapper_skips_type_only_imports() {
    let source = "import { type Foo } from \"./types\"; export const x = 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("define([\"require\", \"exports\"]"),
        "Expected AMD wrapper without type-only dependencies: {}",
        output
    );
    assert!(
        output.contains("function (require, exports)"),
        "Expected AMD factory signature without extra params: {}",
        output
    );
    assert!(
        !output.contains("\"./types\""),
        "Type-only import should not add AMD dependency: {}",
        output
    );
}

#[test]
fn test_legacy_system_wrapper_skips_type_only_exports() {
    let source = "export { type Foo } from \"./types\"; export const x = 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        ..Default::default()
    };
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("System.register([]"),
        "Expected System.register without type-only dependencies: {}",
        output
    );
    assert!(
        !output.contains("\"./types\""),
        "Type-only export should not add System dependency: {}",
        output
    );
}
