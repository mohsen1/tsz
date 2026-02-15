use super::*;
use crate::transforms::ir::IRGeneratorCase;

#[test]
fn test_emit_literals() {
    assert_eq!(IRPrinter::emit_to_string(&IRNode::number("42")), "42");
    assert_eq!(
        IRPrinter::emit_to_string(&IRNode::string("hello")),
        "\"hello\""
    );
    assert_eq!(
        IRPrinter::emit_to_string(&IRNode::BooleanLiteral(true)),
        "true"
    );
    assert_eq!(
        IRPrinter::emit_to_string(&IRNode::BooleanLiteral(false)),
        "false"
    );
    assert_eq!(IRPrinter::emit_to_string(&IRNode::NullLiteral), "null");
    assert_eq!(IRPrinter::emit_to_string(&IRNode::Undefined), "void 0");
}

#[test]
fn test_emit_identifiers() {
    assert_eq!(IRPrinter::emit_to_string(&IRNode::id("foo")), "foo");
    assert_eq!(IRPrinter::emit_to_string(&IRNode::this()), "this");
    assert_eq!(IRPrinter::emit_to_string(&IRNode::this_captured()), "_this");
}

#[test]
fn test_emit_binary_expr() {
    let expr = IRNode::binary(IRNode::id("a"), "+", IRNode::number("1"));
    assert_eq!(IRPrinter::emit_to_string(&expr), "a + 1");

    let assign = IRNode::assign(IRNode::id("x"), IRNode::number("42"));
    assert_eq!(IRPrinter::emit_to_string(&assign), "x = 42");
}

#[test]
fn test_emit_call_expr() {
    let call = IRNode::call(IRNode::id("foo"), vec![]);
    assert_eq!(IRPrinter::emit_to_string(&call), "foo()");

    let call_args = IRNode::call(
        IRNode::id("bar"),
        vec![IRNode::number("1"), IRNode::string("test")],
    );
    assert_eq!(IRPrinter::emit_to_string(&call_args), "bar(1, \"test\")");
}

#[test]
fn test_emit_property_access() {
    let prop = IRNode::prop(IRNode::id("obj"), "prop");
    assert_eq!(IRPrinter::emit_to_string(&prop), "obj.prop");

    let chained = IRNode::prop(IRNode::prop(IRNode::id("a"), "b"), "c");
    assert_eq!(IRPrinter::emit_to_string(&chained), "a.b.c");
}

#[test]
fn test_emit_element_access() {
    let elem = IRNode::elem(IRNode::id("arr"), IRNode::number("0"));
    assert_eq!(IRPrinter::emit_to_string(&elem), "arr[0]");
}

#[test]
fn test_emit_var_decl() {
    let decl = IRNode::var_decl("x", None);
    assert_eq!(IRPrinter::emit_to_string(&decl), "var x;");

    let decl_init = IRNode::var_decl("y", Some(IRNode::number("42")));
    assert_eq!(IRPrinter::emit_to_string(&decl_init), "var y = 42;");
}

#[test]
fn test_emit_return_statement() {
    let ret = IRNode::ret(None);
    assert_eq!(IRPrinter::emit_to_string(&ret), "return;");

    let ret_val = IRNode::ret(Some(IRNode::number("42")));
    assert_eq!(IRPrinter::emit_to_string(&ret_val), "return 42;");
}

#[test]
fn test_emit_function_decl() {
    let func = IRNode::func_decl(
        "foo",
        vec![IRParam::new("x")],
        vec![IRNode::ret(Some(IRNode::id("x")))],
    );
    let output = IRPrinter::emit_to_string(&func);
    assert!(output.contains("function foo(x)"));
    assert!(output.contains("return x;"));
}

#[test]
fn test_emit_function_expr() {
    let func = IRNode::func_expr(None, vec![], vec![IRNode::ret(Some(IRNode::number("42")))]);
    let output = IRPrinter::emit_to_string(&func);
    assert!(output.contains("function ()"));
    assert!(output.contains("return 42;"));
}

#[test]
fn test_emit_es5_class_iife() {
    let class = IRNode::ES5ClassIIFE {
        name: "Point".to_string(),
        base_class: None,
        body: vec![
            IRNode::func_decl(
                "Point",
                vec![IRParam::new("x")],
                vec![IRNode::expr_stmt(IRNode::assign(
                    IRNode::prop(IRNode::this(), "x"),
                    IRNode::id("x"),
                ))],
            ),
            IRNode::ret(Some(IRNode::id("Point"))),
        ],
        weakmap_decls: vec![],
        weakmap_inits: vec![],
    };

    let output = IRPrinter::emit_to_string(&class);
    assert!(output.contains("var Point = /** @class */ (function ()"));
    assert!(output.contains("function Point(x)"));
    assert!(output.contains("this.x = x"));
    assert!(output.contains("return Point;"));
}

#[test]
fn test_emit_es5_class_with_extends() {
    let class = IRNode::ES5ClassIIFE {
        name: "Child".to_string(),
        base_class: Some(Box::new(IRNode::id("Parent"))),
        body: vec![
            IRNode::ExtendsHelper {
                class_name: "Child".to_string(),
            },
            IRNode::func_decl("Child", vec![], vec![]),
            IRNode::ret(Some(IRNode::id("Child"))),
        ],
        weakmap_decls: vec![],
        weakmap_inits: vec![],
    };

    let output = IRPrinter::emit_to_string(&class);
    assert!(output.contains("(function (_super)"));
    assert!(output.contains("__extends(Child, _super)"));
    assert!(output.contains("}(Parent))"));
}

#[test]
fn test_emit_generator_body_simple() {
    let generator_body = IRNode::GeneratorBody {
        has_await: false,
        cases: vec![IRGeneratorCase {
            label: 0,
            statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                opcode: 2,
                value: None,
                comment: Some("return".to_string()),
            }))],
        }],
    };

    let output = IRPrinter::emit_to_string(&generator_body);
    assert!(output.contains("return __generator(this, function (_a)"));
    assert!(output.contains("[2 /*return*/]"));
}

#[test]
fn test_emit_awaiter_call() {
    let awaiter = IRNode::AwaiterCall {
        this_arg: Box::new(IRNode::this()),
        generator_body: Box::new(IRNode::GeneratorBody {
            has_await: false,
            cases: vec![IRGeneratorCase {
                label: 0,
                statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                    opcode: 2,
                    value: None,
                    comment: Some("return".to_string()),
                }))],
            }],
        }),
    };

    let output = IRPrinter::emit_to_string(&awaiter);
    assert!(output.contains("return __awaiter(this, void 0, void 0, function ()"));
}

#[test]
fn test_emit_private_field_get() {
    let get = IRNode::PrivateFieldGet {
        receiver: Box::new(IRNode::this()),
        weakmap_name: "_Foo_bar".to_string(),
    };

    let output = IRPrinter::emit_to_string(&get);
    assert_eq!(output, "__classPrivateFieldGet(this, _Foo_bar, \"f\")");
}

#[test]
fn test_emit_private_field_set() {
    let set = IRNode::PrivateFieldSet {
        receiver: Box::new(IRNode::this()),
        weakmap_name: "_Foo_bar".to_string(),
        value: Box::new(IRNode::number("42")),
    };

    let output = IRPrinter::emit_to_string(&set);
    assert_eq!(output, "__classPrivateFieldSet(this, _Foo_bar, 42, \"f\")");
}

#[test]
fn test_emit_string_escaping() {
    let str_lit = IRNode::string("hello\nworld");
    assert_eq!(IRPrinter::emit_to_string(&str_lit), "\"hello\\nworld\"");

    let str_quotes = IRNode::string("say \"hi\"");
    assert_eq!(IRPrinter::emit_to_string(&str_quotes), "\"say \\\"hi\\\"\"");
}

#[test]
fn test_nested_sequence_respects_namespace_skip_indent() {
    let seq = IRNode::Sequence(vec![
        IRNode::Raw("x;".to_string()),
        IRNode::NamespaceIIFE {
            name: "N".to_string(),
            name_parts: vec!["N".to_string()],
            body: vec![],
            is_exported: false,
            attach_to_exports: false,
            should_declare_var: false,
            parent_name: None,
            param_name: None,
            skip_sequence_indent: true,
        },
    ]);
    let mut printer = IRPrinter::new();
    printer.set_indent_level(1);
    printer.emit(&IRNode::expr_stmt(seq));
    let output = printer.get_output().to_string();

    assert!(
        output.contains("x;\n(function (N)"),
        "namespace IIFE should not be extra-indented inside nested Sequence. Got:\n{output}"
    );
}
