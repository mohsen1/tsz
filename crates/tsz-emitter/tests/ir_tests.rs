//! Tests for the IR module

use super::ir::*;

#[test]
fn test_ir_builder_helpers() {
    // Test identifier creation
    let id = IRNode::id("foo");
    assert!(matches!(id, IRNode::Identifier(name) if name == "foo"));

    // Test string literal
    let str_lit = IRNode::string("hello");
    assert!(matches!(str_lit, IRNode::StringLiteral(s) if s == "hello"));

    // Test number literal
    let num = IRNode::number("42");
    assert!(matches!(num, IRNode::NumericLiteral(n) if n == "42"));

    // Test this reference
    let this = IRNode::this();
    assert!(matches!(this, IRNode::This { captured: false }));

    let this_captured = IRNode::this_captured();
    assert!(matches!(this_captured, IRNode::This { captured: true }));

    // Test void 0
    let undef = IRNode::void_0();
    assert!(matches!(undef, IRNode::Undefined));
}

#[test]
fn test_ir_call_expr() {
    let callee = IRNode::id("foo");
    let args = vec![IRNode::number("1"), IRNode::string("bar")];
    let call = IRNode::call(callee, args);

    match call {
        IRNode::CallExpr { callee, arguments } => {
            assert!(matches!(*callee, IRNode::Identifier(name) if name == "foo"));
            assert_eq!(arguments.len(), 2);
        }
        _ => panic!("Expected CallExpr"),
    }
}

#[test]
fn test_ir_property_access() {
    let obj = IRNode::id("obj");
    let prop = IRNode::prop(obj, "prop");

    match prop {
        IRNode::PropertyAccess { object, property } => {
            assert!(matches!(*object, IRNode::Identifier(name) if name == "obj"));
            assert_eq!(property, "prop");
        }
        _ => panic!("Expected PropertyAccess"),
    }
}

#[test]
fn test_ir_binary_expr() {
    let left = IRNode::id("a");
    let right = IRNode::number("1");
    let bin = IRNode::binary(left, "+", right);

    match bin {
        IRNode::BinaryExpr {
            left,
            operator,
            right,
        } => {
            assert!(matches!(*left, IRNode::Identifier(name) if name == "a"));
            assert_eq!(operator, "+");
            assert!(matches!(*right, IRNode::NumericLiteral(n) if n == "1"));
        }
        _ => panic!("Expected BinaryExpr"),
    }
}

#[test]
fn test_ir_assign() {
    let target = IRNode::id("x");
    let value = IRNode::number("42");
    let assign = IRNode::assign(target, value);

    match assign {
        IRNode::BinaryExpr { operator, .. } => {
            assert_eq!(operator, "=");
        }
        _ => panic!("Expected BinaryExpr with = operator"),
    }
}

#[test]
fn test_ir_var_decl() {
    let decl = IRNode::var_decl("x", Some(IRNode::number("42")));

    match decl {
        IRNode::VarDecl { name, initializer } => {
            assert_eq!(name, "x");
            assert!(initializer.is_some());
        }
        _ => panic!("Expected VarDecl"),
    }
}

#[test]
fn test_ir_return_stmt() {
    let ret = IRNode::ret(Some(IRNode::id("result")));

    match ret {
        IRNode::ReturnStatement(Some(expr)) => {
            assert!(matches!(*expr, IRNode::Identifier(name) if name == "result"));
        }
        _ => panic!("Expected ReturnStatement with expression"),
    }

    let ret_void = IRNode::ret(None);
    assert!(matches!(ret_void, IRNode::ReturnStatement(None)));
}

#[test]
fn test_ir_function_expr() {
    let params = vec![IRParam::new("x"), IRParam::new("y")];
    let body = vec![IRNode::ret(Some(IRNode::binary(
        IRNode::id("x"),
        "+",
        IRNode::id("y"),
    )))];
    let func = IRNode::func_expr(Some("add".into()), params, body);

    match func {
        IRNode::FunctionExpr {
            name,
            parameters,
            body,
            ..
        } => {
            assert_eq!(name, Some("add".into()));
            assert_eq!(parameters.len(), 2);
            assert_eq!(body.len(), 1);
        }
        _ => panic!("Expected FunctionExpr"),
    }
}

#[test]
fn test_ir_function_decl() {
    let params = vec![IRParam::new("n")];
    let body = vec![IRNode::ret(Some(IRNode::id("n")))];
    let func = IRNode::func_decl("identity", params, body);

    match func {
        IRNode::FunctionDecl {
            name,
            parameters,
            body,
            body_source_range: _,
            leading_comment: _,
        } => {
            assert_eq!(name, "identity");
            assert_eq!(parameters.len(), 1);
            assert_eq!(body.len(), 1);
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_ir_param_with_default() {
    let param = IRParam::new("x").with_default(IRNode::number("0"));

    assert_eq!(param.name, "x");
    assert!(!param.rest);
    assert!(param.default_value.is_some());
}

#[test]
fn test_ir_param_rest() {
    let param = IRParam::rest("args");

    assert_eq!(param.name, "args");
    assert!(param.rest);
    assert!(param.default_value.is_none());
}

#[test]
fn test_ir_block() {
    let stmts = vec![
        IRNode::var_decl("x", Some(IRNode::number("1"))),
        IRNode::ret(Some(IRNode::id("x"))),
    ];
    let block = IRNode::block(stmts);

    match block {
        IRNode::Block(statements) => {
            assert_eq!(statements.len(), 2);
        }
        _ => panic!("Expected Block"),
    }
}

#[test]
fn test_ir_expr_stmt() {
    let expr = IRNode::call(IRNode::id("console"), vec![]);
    let stmt = IRNode::expr_stmt(expr);

    assert!(matches!(stmt, IRNode::ExpressionStatement(_)));
}

#[test]
fn test_ir_es5_class_iife() {
    // Test the ES5 class IIFE structure
    let class_iife = IRNode::ES5ClassIIFE {
        name: "Point".into(),
        base_class: None,
        body: vec![
            IRNode::func_decl(
                "Point",
                vec![IRParam::new("x"), IRParam::new("y")],
                vec![
                    IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::this(), "x"),
                        IRNode::id("x"),
                    )),
                    IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::this(), "y"),
                        IRNode::id("y"),
                    )),
                ],
            ),
            IRNode::ret(Some(IRNode::id("Point"))),
        ],
        weakmap_decls: vec![],
        weakmap_inits: vec![],
        leading_comment: None,
        deferred_static_blocks: vec![],
    };

    match class_iife {
        IRNode::ES5ClassIIFE {
            name,
            base_class,
            body,
            ..
        } => {
            assert_eq!(name, "Point");
            assert!(base_class.is_none());
            assert_eq!(body.len(), 2);
        }
        _ => panic!("Expected ES5ClassIIFE"),
    }
}

#[test]
fn test_ir_generator_body() {
    // Test generator body for async transforms
    let gen_body = IRNode::GeneratorBody {
        has_await: true,
        cases: vec![
            IRGeneratorCase {
                label: 0,
                statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                    opcode: 4,
                    value: Some(Box::new(IRNode::call(IRNode::id("fetch"), vec![]))),
                    comment: Some("yield".into()),
                }))],
            },
            IRGeneratorCase {
                label: 1,
                statements: vec![
                    IRNode::expr_stmt(IRNode::GeneratorSent),
                    IRNode::ret(Some(IRNode::GeneratorOp {
                        opcode: 2,
                        value: None,
                        comment: Some("return".into()),
                    })),
                ],
            },
        ],
    };

    match gen_body {
        IRNode::GeneratorBody { has_await, cases } => {
            assert!(has_await);
            assert_eq!(cases.len(), 2);
            assert_eq!(cases[0].label, 0);
            assert_eq!(cases[1].label, 1);
        }
        _ => panic!("Expected GeneratorBody"),
    }
}

#[test]
fn test_ir_awaiter_call() {
    let awaiter = IRNode::AwaiterCall {
        this_arg: Box::new(IRNode::this()),
        hoisted_var_groups: vec![],
        promise_constructor: None,
        generator_body: Box::new(IRNode::GeneratorBody {
            has_await: false,
            cases: vec![IRGeneratorCase {
                label: 0,
                statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                    opcode: 2,
                    value: None,
                    comment: Some("return".into()),
                }))],
            }],
        }),
    };

    assert!(matches!(awaiter, IRNode::AwaiterCall { .. }));
}

#[test]
fn test_ir_private_field_helpers() {
    // Test private field get
    let get = IRNode::PrivateFieldGet {
        receiver: Box::new(IRNode::this()),
        weakmap_name: "_Foo_bar".into(),
    };
    assert!(matches!(get, IRNode::PrivateFieldGet { .. }));

    // Test private field set
    let set = IRNode::PrivateFieldSet {
        receiver: Box::new(IRNode::this()),
        weakmap_name: "_Foo_bar".into(),
        value: Box::new(IRNode::number("42")),
    };
    assert!(matches!(set, IRNode::PrivateFieldSet { .. }));

    // Test WeakMap set
    let wm_set = IRNode::WeakMapSet {
        weakmap_name: "_Foo_bar".into(),
        key: Box::new(IRNode::this()),
        value: Box::new(IRNode::void_0()),
    };
    assert!(matches!(wm_set, IRNode::WeakMapSet { .. }));
}

#[test]
fn test_ir_object_literal() {
    let obj = IRNode::object(vec![
        IRProperty {
            key: IRPropertyKey::Identifier("x".into()),
            value: IRNode::number("1"),
            kind: IRPropertyKind::Init,
        },
        IRProperty {
            key: IRPropertyKey::StringLiteral("y".into()),
            value: IRNode::number("2"),
            kind: IRPropertyKind::Init,
        },
    ]);

    match obj {
        IRNode::ObjectLiteral {
            properties: props, ..
        } => {
            assert_eq!(props.len(), 2);
            assert!(matches!(&props[0].key, IRPropertyKey::Identifier(k) if k == "x"));
            assert!(matches!(&props[1].key, IRPropertyKey::StringLiteral(k) if k == "y"));
        }
        _ => panic!("Expected ObjectLiteral"),
    }
}

#[test]
fn test_ir_array_literal() {
    let arr = IRNode::ArrayLiteral(vec![
        IRNode::number("1"),
        IRNode::number("2"),
        IRNode::number("3"),
    ]);

    match arr {
        IRNode::ArrayLiteral(elements) => {
            assert_eq!(elements.len(), 3);
        }
        _ => panic!("Expected ArrayLiteral"),
    }
}

#[test]
fn test_ir_chained_property_access() {
    // Build: console.log
    let console_log = IRNode::prop(IRNode::id("console"), "log");

    // Build: console.log("hello")
    let call = IRNode::call(console_log, vec![IRNode::string("hello")]);

    match call {
        IRNode::CallExpr { callee, arguments } => {
            match *callee {
                IRNode::PropertyAccess { object, property } => {
                    assert!(matches!(*object, IRNode::Identifier(name) if name == "console"));
                    assert_eq!(property, "log");
                }
                _ => panic!("Expected PropertyAccess as callee"),
            }
            assert_eq!(arguments.len(), 1);
        }
        _ => panic!("Expected CallExpr"),
    }
}

// =============================================================================
// Additional builder coverage: helpers not exercised by the suite above.
// All assertions check the variant + field shape produced by each builder so a
// future refactor cannot silently change the IR contract.
// =============================================================================

#[test]
fn test_ir_elem_access() {
    // arr[0]
    let access = IRNode::elem(IRNode::id("arr"), IRNode::number("0"));

    match access {
        IRNode::ElementAccess { object, index } => {
            assert!(matches!(*object, IRNode::Identifier(name) if name == "arr"));
            assert!(matches!(*index, IRNode::NumericLiteral(n) if n == "0"));
        }
        _ => panic!("Expected ElementAccess"),
    }
}

#[test]
fn test_ir_elem_access_with_string_key() {
    // obj["computed"]
    let access = IRNode::elem(IRNode::id("obj"), IRNode::string("computed"));

    match access {
        IRNode::ElementAccess { object, index } => {
            assert!(matches!(*object, IRNode::Identifier(name) if name == "obj"));
            assert!(matches!(*index, IRNode::StringLiteral(s) if s == "computed"));
        }
        _ => panic!("Expected ElementAccess with string index"),
    }
}

#[test]
fn test_ir_paren_wraps_expression() {
    let inner = IRNode::binary(IRNode::id("a"), "+", IRNode::id("b"));
    let wrapped = inner.paren();

    match wrapped {
        IRNode::Parenthesized(boxed) => {
            assert!(matches!(*boxed, IRNode::BinaryExpr { .. }));
        }
        _ => panic!("Expected Parenthesized"),
    }
}

#[test]
fn test_ir_paren_double_wraps() {
    // Double-wrapping should produce two distinct Parenthesized layers — the
    // builder is intentionally dumb so transforms can rely on a 1:1 mapping.
    let inner = IRNode::id("x");
    let wrapped = inner.paren().paren();

    match wrapped {
        IRNode::Parenthesized(outer) => {
            assert!(matches!(*outer, IRNode::Parenthesized(_)));
        }
        _ => panic!("Expected Parenthesized"),
    }
}

#[test]
fn test_ir_var_decl_uninitialized() {
    // `var x;` (no initializer)
    let decl = IRNode::var_decl("x", None);

    match decl {
        IRNode::VarDecl { name, initializer } => {
            assert_eq!(name, "x");
            assert!(initializer.is_none());
        }
        _ => panic!("Expected VarDecl with no initializer"),
    }
}

#[test]
fn test_ir_object_literal_builder_default_source_range() {
    // The `object` builder always produces `source_range: None`. Anything that
    // wants to track the original literal span has to set it explicitly.
    let obj = IRNode::object(vec![IRProperty::init("x", IRNode::number("1"))]);

    match obj {
        IRNode::ObjectLiteral {
            properties,
            source_range,
        } => {
            assert_eq!(properties.len(), 1);
            assert!(source_range.is_none());
        }
        _ => panic!("Expected ObjectLiteral"),
    }
}

#[test]
fn test_ir_empty_object_is_empty() {
    let obj = IRNode::empty_object();

    match obj {
        IRNode::ObjectLiteral {
            properties,
            source_range,
        } => {
            assert!(properties.is_empty());
            assert!(source_range.is_none());
        }
        _ => panic!("Expected empty ObjectLiteral"),
    }
}

#[test]
fn test_ir_array_builder_preserves_order() {
    let arr = IRNode::array(vec![
        IRNode::number("1"),
        IRNode::string("two"),
        IRNode::id("three"),
    ]);

    match arr {
        IRNode::ArrayLiteral(elements) => {
            assert_eq!(elements.len(), 3);
            assert!(matches!(&elements[0], IRNode::NumericLiteral(n) if n == "1"));
            assert!(matches!(&elements[1], IRNode::StringLiteral(s) if s == "two"));
            assert!(matches!(&elements[2], IRNode::Identifier(name) if name == "three"));
        }
        _ => panic!("Expected ArrayLiteral"),
    }
}

#[test]
fn test_ir_empty_array_is_empty() {
    let arr = IRNode::empty_array();

    match arr {
        IRNode::ArrayLiteral(elements) => assert!(elements.is_empty()),
        _ => panic!("Expected empty ArrayLiteral"),
    }
}

#[test]
fn test_ir_logical_or_builder() {
    // `a || b`
    let expr = IRNode::logical_or(IRNode::id("a"), IRNode::id("b"));

    match expr {
        IRNode::LogicalOr { left, right } => {
            assert!(matches!(*left, IRNode::Identifier(name) if name == "a"));
            assert!(matches!(*right, IRNode::Identifier(name) if name == "b"));
        }
        _ => panic!("Expected LogicalOr"),
    }
}

#[test]
fn test_ir_logical_and_builder() {
    // `a && b`
    let expr = IRNode::logical_and(IRNode::id("a"), IRNode::id("b"));

    match expr {
        IRNode::LogicalAnd { left, right } => {
            assert!(matches!(*left, IRNode::Identifier(name) if name == "a"));
            assert!(matches!(*right, IRNode::Identifier(name) if name == "b"));
        }
        _ => panic!("Expected LogicalAnd"),
    }
}

#[test]
fn test_ir_sequence_builder() {
    let seq = IRNode::sequence(vec![
        IRNode::expr_stmt(IRNode::call(IRNode::id("a"), vec![])),
        IRNode::expr_stmt(IRNode::call(IRNode::id("b"), vec![])),
        IRNode::ret(None),
    ]);

    match seq {
        IRNode::Sequence(nodes) => {
            assert_eq!(nodes.len(), 3);
            assert!(matches!(nodes[0], IRNode::ExpressionStatement(_)));
            assert!(matches!(nodes[1], IRNode::ExpressionStatement(_)));
            assert!(matches!(nodes[2], IRNode::ReturnStatement(None)));
        }
        _ => panic!("Expected Sequence"),
    }
}

#[test]
fn test_ir_property_init_helper() {
    let prop = IRProperty::init("count", IRNode::number("42"));

    assert!(matches!(&prop.key, IRPropertyKey::Identifier(k) if k == "count"));
    assert!(matches!(prop.value, IRNode::NumericLiteral(n) if n == "42"));
    assert_eq!(prop.kind, IRPropertyKind::Init);
}

#[test]
fn test_ir_expr_stmt_wraps_inner_expr() {
    // The `expr_stmt` helper boxes the inner expression — make sure the
    // resulting variant is `ExpressionStatement` and the inner shape is
    // preserved verbatim.
    let inner = IRNode::call(IRNode::id("doSomething"), vec![IRNode::number("1")]);
    let stmt = IRNode::expr_stmt(inner);

    match stmt {
        IRNode::ExpressionStatement(boxed) => match *boxed {
            IRNode::CallExpr { callee, arguments } => {
                assert!(matches!(*callee, IRNode::Identifier(name) if name == "doSomething"));
                assert_eq!(arguments.len(), 1);
            }
            _ => panic!("Expected inner CallExpr"),
        },
        _ => panic!("Expected ExpressionStatement"),
    }
}

#[test]
fn test_ir_block_empty() {
    let block = IRNode::block(vec![]);

    match block {
        IRNode::Block(statements) => assert!(statements.is_empty()),
        _ => panic!("Expected empty Block"),
    }
}

#[test]
fn test_ir_param_new_defaults() {
    let param = IRParam::new("x");

    assert_eq!(param.name, "x");
    assert!(!param.rest);
    assert!(param.default_value.is_none());
}

#[test]
fn test_ir_param_rest_with_default_combo() {
    // `with_default` should override the default field even on a rest param —
    // the builder does not currently reject this combination, and the test
    // pins the existing behaviour so future refactors are explicit if they
    // want to change it.
    let param = IRParam::rest("args").with_default(IRNode::empty_array());

    assert_eq!(param.name, "args");
    assert!(param.rest);
    assert!(param.default_value.is_some());
}
