//! Integration tests for Transform/Print separation architecture
//!
//! These tests demonstrate the complete two-phase emission pipeline:
//! 1. LoweringPass analyzes AST and produces TransformDirective entries
//! 2. Printer consults TransformContext and applies transforms during emission

use crate::emit_context::EmitContext;
use crate::emitter::Printer;
use crate::lowering_pass::LoweringPass;
use crate::parser::ParserState;
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::transform_context::{TransformContext, TransformDirective};
use std::sync::Arc;

#[test]
fn test_two_phase_emission_es5_class() {
    // Parse source
    let source = "class Point { constructor(x, y) { this.x = x; this.y = y; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    // Phase 1: Lowering Pass (Transform)
    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    // Verify transforms were generated
    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ES5Class transform"
    );

    // Phase 2: Print Pass (with Transforms)
    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 IIFE pattern was emitted
    assert!(
        output.contains("var Point"),
        "ES5 output should contain 'var Point'"
    );
    assert!(
        output.contains("function ()"),
        "ES5 output should contain IIFE pattern"
    );
    assert!(
        output.contains("return Point"),
        "ES5 output should return constructor"
    );
}

#[test]
fn test_two_phase_emission_es5_class_expression() {
    let source = "const C = class { method() { return 1; } };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut class_expr_idx = None;
    for (idx, node) in arena.nodes.iter().enumerate() {
        if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            class_expr_idx = Some(NodeIndex(idx as u32));
            break;
        }
    }

    let class_expr_idx = class_expr_idx.expect("expected class expression");
    let directive = transforms
        .get(class_expr_idx)
        .expect("expected transform directive for class expression");
    assert!(
        matches!(directive, TransformDirective::ES5ClassExpression { .. }),
        "LoweringPass should generate ES5ClassExpression transform"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var C ="),
        "ES5 class expression should downlevel to var assignment: {}",
        output
    );
    assert!(
        output.contains("(function () {"),
        "ES5 class expression should emit IIFE: {}",
        output
    );
    assert!(
        !output.contains("class {"),
        "ES5 class expression should not emit ES6 class syntax: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_es5_derived_field_initializer_order_and_nested_arrow_async_this_capture() {
    let source = r#"
class Base { m() { return 1; } }
class Derived extends Base {
    field = () => async () => super["m"]();
    constructor() { prep(); super(); post(); }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    let prep_pos = output.find("prep()").expect("expected prep() call");
    let super_pos = output
        .find("_super.call(this")
        .expect("expected super call assignment");
    let init_pos = output
        .find("_this.field =")
        .expect("expected field initializer assignment");
    let post_pos = output.find("post()").expect("expected post() call");

    assert!(
        prep_pos < super_pos,
        "Expected prep() before super call: {}",
        output
    );
    assert!(
        super_pos < init_pos,
        "Expected field initializer after super call: {}",
        output
    );
    assert!(
        init_pos < post_pos,
        "Expected post() after field initializer: {}",
        output
    );
    assert!(
        output.contains("__awaiter(_this"),
        "Expected async arrow to capture this in field initializer: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this"),
        "Expected computed super call to lower with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "Expected computed super access to be downleveled: {}",
        output
    );
}

#[test]
fn test_lowering_pass_sets_es5_helpers() {
    let source = r#"
async function foo() { await bar(); }
const { x, ...rest } = obj;
for (const v of arr) { v; }
const t = tag`hi ${name}`;
class Base {}
class Derived extends Base { #count = 0; }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);
    let helpers = transforms.helpers();

    assert!(
        helpers.awaiter && helpers.generator,
        "Expected async helpers to be set"
    );
    assert!(helpers.values, "Expected __values helper to be set");
    assert!(helpers.rest, "Expected __rest helper to be set");
    assert!(
        helpers.make_template_object,
        "Expected __makeTemplateObject helper to be set"
    );
    assert!(helpers.extends, "Expected __extends helper to be set");
    assert!(
        helpers.class_private_field_get && helpers.class_private_field_set,
        "Expected class private field helpers to be set"
    );
}

#[test]
fn test_lowering_pass_es5_class_heritage_clause() {
    let source = "class Base {} class Derived extends Base {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");

    let mut derived_idx = None;
    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
            continue;
        }
        let Some(class_data) = arena.get_class(stmt_node) else {
            continue;
        };
        if let Some(clauses) = &class_data.heritage_clauses
            && !clauses.nodes.is_empty()
        {
            derived_idx = Some(stmt_idx);
            break;
        }
    }

    let derived_idx = derived_idx.expect("expected derived class declaration");
    let directive = transforms
        .get(derived_idx)
        .expect("expected transform directive for derived class");

    match directive {
        TransformDirective::ES5Class { heritage, .. } => {
            let heritage_idx = heritage.expect("expected extends heritage clause");
            let heritage_node = arena.get(heritage_idx).expect("expected heritage node");
            let heritage_data = arena
                .get_heritage(heritage_node)
                .expect("expected heritage data");
            assert_eq!(
                heritage_data.token,
                SyntaxKind::ExtendsKeyword as u16,
                "Expected extends heritage clause"
            );
        }
        _ => panic!("Expected ES5Class directive"),
    }
}

#[test]
fn test_two_phase_emission_es5_class_extends_private_fields_helpers() {
    let source = r#"
class Base {}
class Derived extends Base {
    #count = 0;
    getCount() { return this.#count; }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var __extends"),
        "ES5 output should include __extends helper: {}",
        output
    );
    assert!(
        output.contains("var __classPrivateFieldGet"),
        "ES5 output should include __classPrivateFieldGet helper: {}",
        output
    );
    assert!(
        output.contains("var __classPrivateFieldSet"),
        "ES5 output should include __classPrivateFieldSet helper: {}",
        output
    );
    assert!(
        output.contains("__extends(Derived, _super)"),
        "ES5 output should call __extends for Derived: {}",
        output
    );
    assert!(
        output.contains("__classPrivateFieldSet(")
            && output.contains("_Derived_count")
            && output.contains("0, \"f\""),
        "ES5 output should emit private field initializer: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_super_method_arguments() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    method() { return super.m(arguments[0]); }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__extends(Derived, _super)"),
        "ES5 output should call __extends for Derived: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(this, arguments[0])"),
        "ES5 output should lower super method call with arguments: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_super_computed_method_arguments() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    method() { return super["m"](arguments[0]); }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_super.prototype[\"m\"].call(this, arguments[0])"),
        "ES5 output should lower computed super method call with arguments: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_field_arrow_super_computed() {
    let source = r#"
const key = "m";
class Base { [key]() {} }
class Derived extends Base {
    field = () => super[key]();
    constructor() { super(); }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_super.prototype[key].call(_this"),
        "ES5 output should lower computed super call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super["),
        "ES5 output should not contain computed super element access: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_super_arrow_this_capture() {
    let source = r#"
class Base {}
class Derived extends Base {
    constructor() {
        super();
        const f = () => this.x;
        f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _this = _super.call(this"),
        "ES5 output should initialize _this from super: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_method_arrow_super_this_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    method() {
        const f = () => super.m(this.x);
        return f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_method_nested_arrow_super_this_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    method() {
        const f = () => () => super.m(this.x);
        return f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_method_arrow_super_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    method() {
        const f = () => super.m(arguments[0]);
        return f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_super.prototype.m.call(_this, arguments[0])"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_ctor_arrow_super_call() {
    let source = r#"
class Base { m() { return this.x; } }
class Derived extends Base {
    constructor() {
        super();
        const f = () => super.m();
        return f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _this = _super.call(this"),
        "ES5 output should initialize _this from super: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_super_nested_arrow_this_capture() {
    let source = r#"
class Base {}
class Derived extends Base {
    constructor() {
        super();
        const f = () => () => this.x;
        f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _this = _super.call(this"),
        "ES5 output should initialize _this from super: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_super_this_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => this.x;
        await f();
        return super.m(arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(this, arguments[0])"),
        "ES5 output should lower super method call: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_super_computed_method_arguments() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        return super["m"](arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(this, arguments[0])"),
        "ES5 output should lower computed super element access: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_super_computed_method_this_arguments() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        return super["m"](this.x + arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(this"),
        "ES5 output should lower computed super element access: {}",
        output
    );
    assert!(
        output.contains("this.x"),
        "ES5 output should preserve this usage in computed super call: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in computed super call: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_arrow_super_computed_arguments() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        return () => super["m"](arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this, arguments[0])"),
        "ES5 output should lower computed super element access in returned arrow: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in returned arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_arrow_super_computed_this_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        return () => super["m"](this.x + arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside returned arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in returned arrow: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this"),
        "ES5 output should lower computed super element access in returned arrow: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_arrow_super_computed_key_this_arguments_capture()
{
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const key = "m";
        return () => super[key](this.x + arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[key].call(_this"),
        "ES5 output should lower computed super element access in returned arrow: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside returned arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in returned arrow: {}",
        output
    );
    assert!(
        !output.contains("super[key]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_arrow_super_computed_key_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const key = "m";
        return () => super[key](arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[key].call(_this, arguments[0])"),
        "ES5 output should lower computed super element access in returned arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in returned arrow: {}",
        output
    );
    assert!(
        !output.contains("super[key]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_arrow_super_method_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        return () => super.m(arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this, arguments[0])"),
        "ES5 output should lower super method call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_nested_arrow_super_computed_key_this_arguments_capture()
 {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const key = "m";
        return () => () => super[key](this.x + arguments[0]);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[key].call(_this"),
        "ES5 output should lower computed super element access in returned nested arrow: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside returned nested arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in returned nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super[key]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_arrow_super_method_this_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        return () => super.m(this.x);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super method call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside returned arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_arrow_super_method_no_args() {
    let source = r#"
class Base { m() { return 1; } }
class Derived extends Base {
    async method() {
        return () => super.m();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super method call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_return_arrow_super_computed_key_no_args() {
    let source = r#"
class Base { m() { return 1; } }
class Derived extends Base {
    async method() {
        const key = "m";
        return () => super[key]();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[key].call(_this)"),
        "ES5 output should lower computed super element access in returned arrow: {}",
        output
    );
    assert!(
        !output.contains("super[key]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel returned arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_arrow_arguments_capture() {
    let source = r#"
class Base {}
class Derived extends Base {
    async method(a: number) {
        const f = () => this.x + arguments[0];
        return await f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_arrow_super_this_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => super.m(this.x);
        return await f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_arrow_super_call_no_args() {
    let source = r#"
class Base { m() { return 1; } }
class Derived extends Base {
    async method() {
        const f = () => super.m();
        return await f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_arrow_super_computed_call_no_args() {
    let source = r#"
class Base { m() { return 1; } }
class Derived extends Base {
    async method() {
        const f = () => super["m"]();
        return await f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this)"),
        "ES5 output should lower computed super element access in arrow: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_arrow_super_computed_key_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const key = "m";
        const f = () => super[key](arguments[0]);
        return await f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in arrow: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[key].call(_this, arguments[0])"),
        "ES5 output should lower computed super element access in arrow: {}",
        output
    );
    assert!(
        !output.contains("super[key]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_arrow_super_computed_key_this_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const key = "m";
        const f = () => super[key](this.x);
        return await f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[key].call(_this"),
        "ES5 output should lower computed super element access in arrow: {}",
        output
    );
    assert!(
        !output.contains("super[key]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_arrow_super_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => super.m(arguments[0]);
        return await f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this, arguments[0])"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_arrow_super_this_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => super.m(this.x + arguments[0]);
        return await f();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_arguments() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => () => super.m(arguments[0]);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this, arguments[0])"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_call_no_args() {
    let source = r#"
class Base { m() { return 1; } }
class Derived extends Base {
    async method() {
        const f = () => () => super.m();
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_this_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => () => super.m(this.x);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_this_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => () => super.m(this.x + arguments[0]);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_computed() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => () => super["m"](arguments[0]);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this, arguments[0])"),
        "ES5 output should lower computed super element access in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_computed_this_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => () => super["m"](this.x);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this"),
        "ES5 output should lower computed super element access in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_computed_this_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => () => super["m"](this.x + arguments[0]);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in nested arrow: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this"),
        "ES5 output should lower computed super element access in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_computed_key_this_arguments_capture()
{
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const key = "m";
        const f = () => () => super[key](this.x + arguments[0]);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in nested arrow: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[key].call(_this"),
        "ES5 output should lower computed super element access in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super[key]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_computed_key_arguments_capture() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const key = "m";
        const f = () => () => super[key](arguments[0]);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in nested arrow: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[key].call(_this, arguments[0])"),
        "ES5 output should lower computed super element access in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super[key]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_nested_arrow_super_computed_arguments() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => () => super["m"](arguments[0]);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this, arguments[0])"),
        "ES5 output should lower computed super element access in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_super_nested_arrow() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    async method() {
        const f = () => () => super.m(arguments[0]);
        return await f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.m.call"),
        "ES5 output should lower super method call in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_method_nested_arrow_arguments_capture() {
    let source = r#"
class Foo {
    method(a) {
        const f = () => () => this.x + arguments[0];
        return f()();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 output should preserve arguments usage in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_class_derived_prop_init_after_super() {
    let source = r#"
class Base {}
class Derived extends Base {
    foo = 1;
    constructor() {
        super();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    let super_pos = output.find("var _this = _super.call(this");
    let prop_pos = output.find("_this.foo = 1");
    assert!(
        super_pos.is_some() && prop_pos.is_some(),
        "ES5 output should include super call and property initializer: {}",
        output
    );
    assert!(
        super_pos.unwrap() < prop_pos.unwrap(),
        "ES5 output should emit property initializer after super: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_derived_prop_arrow_capture() {
    let source = r#"
class Base {}
class Derived extends Base {
    foo = () => this.x;
    constructor() {
        super();
    }
    async method() {
        await this.foo();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    let super_pos = output.find("var _this = _super.call(this");
    let prop_pos = output.find("_this.foo = function");
    assert!(
        super_pos.is_some() && prop_pos.is_some(),
        "ES5 output should include super call and arrow initializer: {}",
        output
    );
    assert!(
        super_pos.unwrap() < prop_pos.unwrap(),
        "ES5 output should emit arrow initializer after super: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        output.contains("return __awaiter"),
        "ES5 output should emit __awaiter for async method: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_async_derived_prop_async_arrow_capture() {
    let source = r#"
class Base {}
class Derived extends Base {
    field = async () => this.x;
    constructor() {
        super();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    let super_pos = output.find("var _this = _super.call(this");
    let prop_pos = output.find("_this.field = function");
    assert!(
        super_pos.is_some() && prop_pos.is_some(),
        "ES5 output should include super call and async arrow initializer: {}",
        output
    );
    assert!(
        super_pos.unwrap() < prop_pos.unwrap(),
        "ES5 output should emit async arrow initializer after super: {}",
        output
    );
    assert!(
        output.contains("__awaiter(_this, void 0, void 0, function () {"),
        "ES5 output should capture this for async arrow: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside async arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel async arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_derived_field_async_nested_arrow_this_capture() {
    let source = r#"
class Base {}
class Derived extends Base {
    field = async () => () => this.x;
    constructor() {
        super();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _this = _super.call(this"),
        "ES5 output should initialize _this from super: {}",
        output
    );
    assert!(
        output.contains("_this.field = function"),
        "ES5 output should emit async arrow field initializer: {}",
        output
    );
    assert!(
        output.contains("__awaiter(_this"),
        "ES5 output should lower async arrow with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_derived_default_arrow_field_capture() {
    let source = r#"
class Base {}
class Derived extends Base {
    field = () => this.x;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _this = _super !== null && _super.apply(this, arguments) || this;"),
        "ES5 output should capture this for derived default ctor: {}",
        output
    );
    assert!(
        output.contains("_this.field = function"),
        "ES5 output should emit arrow field initializer: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_field_nested_arrow_this_capture() {
    let source = r#"
class Foo {
    field = () => () => this.x;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _this = this"),
        "ES5 output should capture this for field initializer: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_field_multi_arrow_this_capture() {
    let source = r#"
class Foo {
    first = () => this.x;
    second = () => () => this.y;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _this = this"),
        "ES5 output should capture this for field initializers: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this in first arrow: {}",
        output
    );
    assert!(
        output.contains("_this.y"),
        "ES5 output should capture this in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrows: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_static_field_no_this_capture() {
    let source = r#"
class Foo {
    static field = () => this.x;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("var _this = this"),
        "ES5 output should not capture this for static field: {}",
        output
    );
    assert!(
        output.contains("Foo.field = function"),
        "ES5 output should emit static field initializer: {}",
        output
    );
    assert!(
        output.contains("this.x"),
        "ES5 output should preserve this usage in static arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_class_static_async_arrow_field() {
    let source = r#"
class Foo {
    static handler = async () => {
        await fetch();
        return this.value;
    };
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__awaiter"),
        "ES5 output should use __awaiter for static async arrow: {}",
        output
    );
    assert!(
        output.contains("Foo.handler"),
        "ES5 output should emit static field initializer: {}",
        output
    );
    assert!(
        output.contains("this.value"),
        "ES5 output should preserve this for static async arrow: {}",
        output
    );
    assert!(
        !output.contains("var _this = this"),
        "ES5 output should not capture this for static field: {}",
        output
    );
    assert!(
        !output.contains("async"),
        "ES5 output should downlevel async: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_class_static_async_arrow_nested_arrow() {
    let source = r#"
class Foo {
    static handler = async () => {
        const inner = () => this.value;
        await fetch();
        return inner();
    };
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__awaiter"),
        "ES5 output should use __awaiter for static async arrow: {}",
        output
    );
    // Static field should NOT capture _this at class level
    assert!(
        !output.contains("var _this = Foo"),
        "ES5 output should not capture Foo to _this: {}",
        output
    );
    assert!(
        !output.contains("async"),
        "ES5 output should downlevel async: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_class_private_field_in_async_method() {
    let source = r#"
class Foo {
    #value = 1;
    async getValue() {
        await fetch();
        return this.#value;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__awaiter"),
        "ES5 output should use __awaiter for async method: {}",
        output
    );
    assert!(
        output.contains("__classPrivateFieldGet"),
        "ES5 output should use __classPrivateFieldGet for private field: {}",
        output
    );
    assert!(
        output.contains("_Foo_value"),
        "ES5 output should emit WeakMap for private field: {}",
        output
    );
    assert!(
        !output.contains("this.#value"),
        "ES5 output should not contain private field syntax: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_derived_field_arrow_super_call() {
    let source = r#"
class Base { m() { return this.x; } }
class Derived extends Base {
    field = () => super.m();
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_derived_field_arrow_super_and_this() {
    let source = r#"
class Base { m() { return this.x; } }
class Derived extends Base {
    field = () => super.m() + this.x;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_super.prototype.m.call(_this"),
        "ES5 output should lower super call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        !output.contains("super.m"),
        "ES5 output should not contain super method access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_derived_field_arrow_super_computed_call() {
    let source = r#"
class Base { m(x) { return x; } }
class Derived extends Base {
    field = () => super["m"](this.x);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_super.prototype[\"m\"].call(_this"),
        "ES5 output should lower computed super call with lexical this: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 output should capture this inside arrow: {}",
        output
    );
    assert!(
        !output.contains("super[\"m\"]"),
        "ES5 output should not contain super element access: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should downlevel arrow: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es6_class_no_transform() {
    // Parse source
    let source = "class Point { constructor(x, y) { this.x = x; this.y = y; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    // Phase 1: Lowering Pass (Transform) - ES6 target
    let ctx = EmitContext::default(); // ES6 by default

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    // Verify no transforms were generated for ES6
    assert!(
        transforms.is_empty(),
        "LoweringPass should NOT generate transforms for ES6 target"
    );

    // Phase 2: Print Pass (with empty Transforms)
    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(false); // ES6 target
    printer.emit(root);

    let output = printer.get_output();

    // Verify native class syntax was emitted
    assert!(
        output.contains("class Point"),
        "ES6 output should contain 'class Point'"
    );
    assert!(
        !output.contains("var Point"),
        "ES6 output should NOT contain 'var Point' (no IIFE)"
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_try_throw_parenthesized() {
    let source =
        "class Foo { method() { try { throw new Error(\"x\"); } catch (e) { return (e); } } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("try {"),
        "ES5 output should contain try block: {}",
        output
    );
    assert!(
        output.contains("throw new Error(\"x\")"),
        "ES5 output should contain throw statement: {}",
        output
    );
    assert!(
        output.contains("catch (e)"),
        "ES5 output should contain catch clause: {}",
        output
    );
    assert!(
        output.contains("return (e);"),
        "ES5 output should preserve parenthesized return: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_template_literal_downlevel() {
    let source = "class Foo { method(name) { return `hi ${name}`; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_text = parser.get_source_text().to_string();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.set_source_text(&source_text);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("\"hi \""),
        "ES5 class output should emit string literal head: {}",
        output
    );
    assert!(
        output.contains("+ (name)"),
        "ES5 class output should concatenate template expression: {}",
        output
    );
    assert!(
        !output.contains('`'),
        "ES5 class output should not emit backticks: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_tagged_template_downlevel() {
    let source = "class Foo { method(name) { return tag`hi ${name}`; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_text = parser.get_source_text().to_string();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.set_source_text(&source_text);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__makeTemplateObject"),
        "ES5 class output should emit __makeTemplateObject helper: {}",
        output
    );
    assert!(
        output.contains("tag(__templateObject_"),
        "ES5 class output should call tag with template object cache: {}",
        output
    );
    assert!(
        !output.contains('`'),
        "ES5 class output should not emit backticks: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_for_destructuring() {
    let source = "class Foo { method(obj) { for (var { x, y } = obj; ; ) { return x + y; } } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("for (var _a = obj, x = _a.x, y = _a.y;"),
        "ES5 output should destructure for-loop initializer: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_object_rest_param() {
    let source = "class Foo { method({ x, ...rest }) { return rest; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var __rest"),
        "ES5 output should include __rest helper: {}",
        output
    );
    assert!(
        output.contains("rest = __rest(_a, [\"x\"])"),
        "ES5 output should downlevel object rest parameter: {}",
        output
    );
}

#[test]
fn test_lowering_pass_es5_object_literal_directive() {
    let source = "const obj = { a: 1, [key]: 2 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected variable statement");
    let stmt_node = arena.get(stmt_idx).expect("expected variable node");
    let var_stmt = arena
        .get_variable(stmt_node)
        .expect("expected variable statement data");
    let decl_list_idx = *var_stmt
        .declarations
        .nodes
        .first()
        .expect("expected declaration list");
    let decl_list_node = arena
        .get(decl_list_idx)
        .expect("expected declaration list node");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("expected declaration list data");
    let decl_idx = *decl_list
        .declarations
        .nodes
        .first()
        .expect("expected variable declaration");
    let decl_node = arena.get(decl_idx).expect("expected declaration node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("expected declaration data");

    let directive = transforms.get(decl.initializer);
    assert!(
        matches!(directive, Some(TransformDirective::ES5ObjectLiteral { .. })),
        "LoweringPass should emit ES5ObjectLiteral directive for computed property"
    );
}

#[test]
fn test_lowering_pass_es5_template_literal_directive() {
    let source = "const msg = `hi ${name}`; const plain = `bye`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut template_expr = None;
    let mut no_sub = None;
    for (idx, node) in arena.nodes.iter().enumerate() {
        let node_idx = NodeIndex(idx as u32);
        if node.kind == crate::parser::syntax_kind_ext::TEMPLATE_EXPRESSION {
            template_expr = Some(node_idx);
        } else if node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 {
            no_sub = Some(node_idx);
        }
    }

    let template_expr = template_expr.expect("expected template expression node");
    let no_sub = no_sub.expect("expected no-substitution template node");

    assert!(
        matches!(
            transforms.get(template_expr),
            Some(TransformDirective::ES5TemplateLiteral { .. })
        ),
        "LoweringPass should emit ES5TemplateLiteral directive for template expression"
    );
    assert!(
        matches!(
            transforms.get(no_sub),
            Some(TransformDirective::ES5TemplateLiteral { .. })
        ),
        "LoweringPass should emit ES5TemplateLiteral directive for no-substitution template"
    );
}

#[test]
fn test_lowering_pass_es5_variable_declaration_list_directive() {
    let source = "let { x, y } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected variable statement");
    let stmt_node = arena.get(stmt_idx).expect("expected variable node");
    let var_stmt = arena
        .get_variable(stmt_node)
        .expect("expected variable statement data");
    let decl_list_idx = *var_stmt
        .declarations
        .nodes
        .first()
        .expect("expected declaration list");

    let directive = transforms.get(decl_list_idx);
    assert!(
        matches!(
            directive,
            Some(TransformDirective::ES5VariableDeclarationList { .. })
        ),
        "LoweringPass should emit ES5VariableDeclarationList directive for destructuring"
    );
}

#[test]
fn test_two_phase_emission_es5_variable_declaration_list_uses_var() {
    let source = "let x = 1; const y = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var x = 1;"),
        "ES5 output should emit var for let: {}",
        output
    );
    assert!(
        output.contains("var y = 2;"),
        "ES5 output should emit var for const: {}",
        output
    );
}

#[test]
fn test_lowering_pass_es5_function_parameters_directive() {
    let source = "function foo(x = 1) { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let func_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected function declaration");

    let directive = transforms.get(func_idx);
    assert!(
        matches!(
            directive,
            Some(TransformDirective::ES5FunctionParameters { .. })
        ),
        "LoweringPass should emit ES5FunctionParameters directive for default params"
    );
}

#[test]
fn test_two_phase_emission_es5_object_literal_computed() {
    let source = "const obj = { a: 1, [key]: 2 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("[key] = 2"),
        "ES5 output should lower computed property assignment: {}",
        output
    );
    assert!(
        output.contains("= { a: 1 }"),
        "ES5 output should keep base object literal: {}",
        output
    );
    assert!(
        !output.contains("[key]:"),
        "ES5 output should not keep computed property syntax: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_variable_destructuring() {
    let source = "let { x, y } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _a = obj"),
        "ES5 output should introduce temp for destructuring: {}",
        output
    );
    assert!(
        output.contains("x = _a.x"),
        "ES5 output should assign destructured properties: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_function_parameters_downlevel() {
    let source = "function foo(x = 1) { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("if (x === void 0) { x = 1; }"),
        "ES5 output should downlevel default parameters: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_object_literal_shorthand_method() {
    let source = "var x = 1; var obj = { x, method(y = 1) { return y + x; } };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("x: x"),
        "ES5 output should expand shorthand properties: {}",
        output
    );
    assert!(
        output.contains("method: function"),
        "ES5 output should downlevel object literal methods: {}",
        output
    );
    assert!(
        output.contains("if (y === void 0) { y = 1; }"),
        "ES5 output should downlevel default parameters: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_class_for_in_of() {
    let source =
        "class Foo { method(obj, arr) { for (var k in obj) { k; } for (var v of arr) { v; } } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("for (var k in obj)"),
        "ES5 output should contain for-in loop: {}",
        output
    );
    assert!(
        output.contains("__values(arr)"),
        "ES5 output should downlevel for-of with __values helper: {}",
        output
    );
    assert!(
        output.contains("var v ="),
        "ES5 output should bind iterator values: {}",
        output
    );
    assert!(
        output.contains(".return"),
        "ES5 output should close iterators: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_for_of_statement() {
    let source = "for (var v of arr) { v; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__values(arr)"),
        "ES5 output should downlevel for-of with __values helper: {}",
        output
    );
    assert!(
        output.contains(".return"),
        "ES5 output should close iterators: {}",
        output
    );
    assert!(
        !output.contains("for (var v of arr)"),
        "ES5 output should not contain raw for-of: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_class_switch_break_continue_do() {
    let source = "class Foo { method(x) { while (x) { continue; } switch (x) { case 1: break; default: return; } do { x--; } while (x); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("while (x)"),
        "ES5 output should contain while loop: {}",
        output
    );
    assert!(
        output.contains("continue;"),
        "ES5 output should contain continue statement: {}",
        output
    );
    assert!(
        output.contains("switch (x)"),
        "ES5 output should contain switch statement: {}",
        output
    );
    assert!(
        output.contains("case 1:"),
        "ES5 output should contain case clause: {}",
        output
    );
    assert!(
        output.contains("break;"),
        "ES5 output should contain break statement: {}",
        output
    );
    assert!(
        output.contains("default:"),
        "ES5 output should contain default clause: {}",
        output
    );
    assert!(
        output.contains("return;"),
        "ES5 output should contain return statement: {}",
        output
    );
    assert!(
        output.contains("do {"),
        "ES5 output should contain do statement: {}",
        output
    );
    assert!(
        output.contains("} while (x);"),
        "ES5 output should contain do-while condition: {}",
        output
    );
}

#[test]
fn test_two_phase_backward_compatibility() {
    // Parse source
    let source = "class Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    // Old way: printer without transforms (backward compatibility)
    let mut printer_old = Printer::new(&arena);
    printer_old.set_target_es5(true);
    printer_old.emit(root);
    let output_old = printer_old.get_output();

    // New way: printer with empty transforms
    let transforms = crate::transform_context::TransformContext::new();
    let mut printer_new = Printer::with_transforms(&arena, transforms);
    printer_new.set_target_es5(true);
    printer_new.emit(root);
    let output_new = printer_new.get_output();

    // Both should produce the same output (backward compatibility)
    assert_eq!(
        output_old, output_new,
        "Old and new emission paths should produce identical output"
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_function() {
    let source = "const add = (a, b) => a + b;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ES5ArrowFunction transform"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function"),
        "ES5 arrow output should contain 'function'"
    );
    assert!(
        !output.contains("=>"),
        "ES5 arrow output should not contain '=>'"
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_function_this_capture() {
    let source = "const fn = () => this.x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ES5ArrowFunction transform"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow output should capture this via IIFE: {}",
        output
    );
    assert!(
        output.contains("_this"),
        "ES5 arrow output should reference _this: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_nested_arrow_this_capture() {
    let source = "const outer = () => { const inner = () => this.x; };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("})(_this)"),
        "ES5 nested arrow should pass outer _this into inner capture: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_this_in_object_literal_property() {
    let source = "const fn = () => ({ foo: this.x });";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow output should capture this in object literal property: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 arrow output should rewrite this in object literal property: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_object_literal_method_no_capture() {
    let source = "const fn = () => ({ method() { return this.x; } });";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("_this"),
        "ES5 arrow output should not capture this for object literal method bodies: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_async_arrow_function() {
    let source = "const foo = async () => { await bar(); };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ES5ArrowFunction transform"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__awaiter"),
        "ES5 async arrow output should contain '__awaiter': {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 async arrow output should not contain '=>': {}",
        output
    );
    assert!(
        !output.contains("async function"),
        "ES5 async arrow output should not contain async syntax: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_async_arrow_this_capture_await() {
    let source = "const foo = async () => await this.bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 async arrow should capture this via IIFE: {}",
        output
    );
    assert!(
        output.contains("__awaiter(_this"),
        "ES5 async arrow should pass _this to __awaiter: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_this_in_type_assertion() {
    let source = "const foo = () => (this as any).x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow should capture this in type assertion: {}",
        output
    );
    assert!(
        output.contains("_this.x") || output.contains("(_this).x"),
        "ES5 arrow should rewrite this in type assertion: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_this_in_satisfies_expression() {
    let source = "const foo = () => (this satisfies Foo).x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow should capture this in satisfies expression: {}",
        output
    );
    assert!(
        output.contains("_this.x") || output.contains("(_this).x"),
        "ES5 arrow should rewrite this in satisfies expression: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_this_in_angle_type_assertion() {
    let source = "const foo = () => (<any>this).x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow should capture this in angle type assertion: {}",
        output
    );
    assert!(
        output.contains("_this.x") || output.contains("(_this).x"),
        "ES5 arrow should rewrite this in angle type assertion: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_this_in_tagged_template_span() {
    let source = "const foo = () => tag`${this.x}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow should capture this in tagged template span: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 arrow should rewrite this in tagged template span: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_this_in_tagged_template_tag() {
    let source = "const foo = () => this.tag`hi`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow should capture this in tagged template tag: {}",
        output
    );
    assert!(
        output.contains("_this.tag"),
        "ES5 arrow should rewrite this in tagged template tag: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_tagged_template_downlevel() {
    let source = "const msg = tag`hi ${name}!`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_text = parser.get_source_text().to_string();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);
    assert!(
        transforms.helpers().make_template_object,
        "Expected __makeTemplateObject helper to be set for tagged template"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.set_source_text(&source_text);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__makeTemplateObject"),
        "ES5 tagged template should use __makeTemplateObject helper: {}",
        output
    );
    assert!(
        output.contains("tag(__templateObject_"),
        "ES5 tagged template should call tag with template object cache: {}",
        output
    );
    assert!(
        !output.contains('`'),
        "ES5 tagged template should not emit backticks: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_template_expression_downlevel() {
    let source = "const msg = `hi ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_text = parser.get_source_text().to_string();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.set_source_text(&source_text);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("\"hi \""),
        "ES5 template expression should emit string literal head: {}",
        output
    );
    assert!(
        output.contains("+ (name)"),
        "ES5 template expression should concatenate expression: {}",
        output
    );
    assert!(
        !output.contains('`'),
        "ES5 template expression should not emit backticks: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_no_substitution_template_downlevel() {
    let source = "const msg = `hello`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_text = parser.get_source_text().to_string();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.set_source_text(&source_text);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("\"hello\""),
        "ES5 no-substitution template should emit string literal: {}",
        output
    );
    assert!(
        !output.contains('`'),
        "ES5 no-substitution template should not emit backticks: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_arrow_this_in_non_null() {
    let source = "const foo = () => this!.x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow should capture this in non-null assertion: {}",
        output
    );
    assert!(
        output.contains("_this.x"),
        "ES5 arrow should rewrite this in non-null assertion: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_es5_async_function() {
    let source = "async function foo() { return 1; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.target_es5 = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ES5AsyncFunction transform"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var __awaiter"),
        "ES5 async output should contain '__awaiter' helper: {}",
        output
    );
    assert!(
        output.contains("var __generator"),
        "ES5 async output should contain '__generator' helper: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_async_function_nested_arrow_this_capture() {
    let source = "async function foo() { const bar = () => this.x; await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_this.x"),
        "ES5 async output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 async output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_async_function_multi_decl_this_capture() {
    let source = "async function foo() { const a = 1, bar = () => this.x; await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("a = 1"),
        "ES5 async output should emit first declarator: {}",
        output
    );
    assert!(
        output.contains("bar = ") && output.contains("_this.x"),
        "ES5 async output should downlevel arrow and capture this: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 async output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_async_function_let_arrow_this_capture() {
    let source = "async function foo() { let bar = () => this.x; await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("bar = ") && output.contains("_this.x"),
        "ES5 async output should downlevel arrow and capture this: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 async output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_async_function_deep_nested_arrow_this_capture() {
    let source = "async function foo() { const bar = () => () => this.x; await bar()(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_this.x"),
        "ES5 async output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 async output should downlevel nested arrow: {}",
        output
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_two_phase_emission_es5_async_function_nested_arrow_arguments_capture() {
    let source =
        "async function foo(a) { const bar = () => () => this.x + arguments[0]; await bar()(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let ctx = EmitContext::es5();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("_this.x"),
        "ES5 async output should capture this inside nested arrow: {}",
        output
    );
    assert!(
        output.contains("arguments[0]"),
        "ES5 async output should preserve arguments usage in nested arrow: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 async output should downlevel nested arrow: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_amd_module_wrapper() {
    let source = "import { foo } from \"./bar\"; export const x = foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::AMD;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ModuleWrapper transform"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("define([\"require\", \"exports\", \"./bar\"]"),
        "AMD output should include define dependency list"
    );
    assert!(
        output.contains("function (require, exports"),
        "AMD output should include factory signature"
    );
}

#[test]
fn test_two_phase_emission_amd_wrapper_reexport_star() {
    let source = "export * from \"./dep\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::AMD;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("define([\"require\", \"exports\", \"./dep\"]"),
        "AMD output should include re-export dependency: {}",
        output
    );
    assert!(
        output.contains("require(\"./dep\")"),
        "AMD output should require re-export dependency: {}",
        output
    );
    assert!(
        output.contains("__exportStar("),
        "AMD output should include __exportStar call: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_umd_module_wrapper() {
    let source = "export const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::UMD;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ModuleWrapper transform"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("(function (factory) {"),
        "UMD output should include wrapper header"
    );
    assert!(
        output.contains("factory(require, exports)"),
        "UMD output should include CommonJS factory path"
    );
}

#[test]
fn test_two_phase_emission_umd_wrapper_reexport_star() {
    let source = "export * from \"./dep\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::UMD;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("(function (factory) {"),
        "UMD output should include wrapper header: {}",
        output
    );
    assert!(
        output.contains("require(\"./dep\")"),
        "UMD output should require re-export dependency: {}",
        output
    );
    assert!(
        output.contains("__exportStar("),
        "UMD output should include __exportStar call: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_system_module_wrapper() {
    let source = "import { foo } from \"./bar\"; export const x = foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::System;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ModuleWrapper transform"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("System.register([\"./bar\"]"),
        "System output should include System.register dependency list"
    );
    assert!(
        output.contains("execute: function ()"),
        "System output should include execute block"
    );
}

#[test]
fn test_two_phase_emission_system_wrapper_reexport_named() {
    let source = "export { foo } from \"./dep\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::System;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("System.register([\"./dep\"]"),
        "System output should include dependency list: {}",
        output
    );
    assert!(
        output.contains("require(\"./dep\")"),
        "System output should require re-export dependency: {}",
        output
    );
    assert!(
        output.contains("Object.defineProperty(exports, \"foo\""),
        "System output should define exported binding: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_commonjs_multi_export_vars() {
    let source = "export const a = 1, b = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate CommonJS export transforms"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("exports.a = a;"),
        "CommonJS output should export a"
    );
    assert!(
        output.contains("exports.b = b;"),
        "CommonJS output should export b"
    );
}

#[test]
fn test_two_phase_emission_commonjs_async_function_export() {
    let source = "export async function foo() { await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate async/CommonJS transforms"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__awaiter"),
        "ES5 output should contain __awaiter: {}",
        output
    );
    assert!(
        output.contains("exports.foo = foo;"),
        "CommonJS output should export foo: {}",
        output
    );
    assert_eq!(
        output.matches("exports.foo = foo;").count(),
        1,
        "Expected a single CommonJS export assignment: {}",
        output
    );
}

#[test]
fn test_lowering_pass_commonjs_default_anonymous_function_directive() {
    let source = "export default function () { return 1; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected export declaration");
    let stmt_node = arena.get(stmt_idx).expect("expected export node");
    let export_decl = arena
        .get_export_decl(stmt_node)
        .expect("expected export declaration data");

    let directive = transforms.get(export_decl.export_clause);
    assert!(
        matches!(
            directive,
            Some(TransformDirective::CommonJSExportDefaultExpr)
        ),
        "LoweringPass should emit default export directive for anonymous function, got: {:?}",
        directive
    );
}

#[test]
fn test_lowering_pass_commonjs_default_anonymous_class_directive() {
    let source = "export default class { method() { return 1; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected export declaration");
    let stmt_node = arena.get(stmt_idx).expect("expected export node");
    let export_decl = arena
        .get_export_decl(stmt_node)
        .expect("expected export declaration data");

    let directive = transforms.get(export_decl.export_clause);
    assert!(
        matches!(
            directive,
            Some(TransformDirective::CommonJSExportDefaultExpr)
        ),
        "LoweringPass should emit default export directive for anonymous class"
    );
}

#[test]
fn test_lowering_pass_commonjs_default_anonymous_class_directive_es5() {
    let source = "export default class { method() { return 1; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected export declaration");
    let stmt_node = arena.get(stmt_idx).expect("expected export node");
    let export_decl = arena
        .get_export_decl(stmt_node)
        .expect("expected export declaration data");

    let directive = transforms.get(export_decl.export_clause);
    assert!(
        matches!(
            directive,
            Some(TransformDirective::CommonJSExportDefaultClassES5 { .. })
        ),
        "LoweringPass should emit ES5 default export directive for anonymous class"
    );
}

#[test]
fn test_two_phase_emission_commonjs_default_anonymous_async_function_export() {
    let source = "export default async function () { await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ES5 async transforms"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.set_module_kind(crate::emitter::ModuleKind::CommonJS);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("__awaiter"),
        "ES5 output should contain __awaiter: {}",
        output
    );
    assert!(
        output.contains("exports.default = function"),
        "CommonJS output should export default function: {}",
        output
    );
    assert!(
        !output.contains("export default"),
        "CommonJS output should not contain ES module syntax: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_commonjs_default_anonymous_class_export() {
    let source = "export default class { method() { return 1; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate ES5 class transforms"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.set_module_kind(crate::emitter::ModuleKind::CommonJS);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var _a_default = /** @class */"),
        "CommonJS output should downlevel default class: {}",
        output
    );
    assert!(
        output.contains("exports.default = _a_default;"),
        "CommonJS output should export default class temp: {}",
        output
    );
    assert!(
        !output.contains("export default"),
        "CommonJS output should not contain ES module syntax: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_commonjs_export_enum() {
    let source = "export enum E { A, B = 2 }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate enum/CommonJS transforms"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.set_module_kind(crate::emitter::ModuleKind::CommonJS);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var E"),
        "CommonJS output should declare enum variable: {}",
        output
    );
    assert!(
        output.contains("(function (E)"),
        "CommonJS output should emit enum IIFE: {}",
        output
    );
    assert!(
        output.contains("exports.E = E;"),
        "CommonJS output should export enum: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_commonjs_export_const_enum_is_erased() {
    let source = "export const enum E { A = 0 }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.set_module_kind(crate::emitter::ModuleKind::CommonJS);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        !output.contains("var E"),
        "CommonJS output should not emit const enum: {}",
        output
    );
    assert!(
        !output.contains("exports.E"),
        "CommonJS output should not export const enum: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_commonjs_export_namespace() {
    let source = "export namespace N { export function foo() { return 1; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate namespace/CommonJS transforms"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.set_module_kind(crate::emitter::ModuleKind::CommonJS);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var N"),
        "CommonJS output should declare namespace variable: {}",
        output
    );
    assert!(
        output.contains("(function (N)"),
        "CommonJS output should emit namespace IIFE: {}",
        output
    );
    assert!(
        output.contains("N.foo = foo;"),
        "CommonJS output should export namespace member: {}",
        output
    );
    assert!(
        output.contains("exports.N = N;"),
        "CommonJS output should export namespace: {}",
        output
    );
}

#[test]
fn test_two_phase_emission_commonjs_auto_detect_exports() {
    let source = "export const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.auto_detect_module = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "LoweringPass should generate CommonJS export transforms via auto-detect"
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.set_auto_detect_module(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("exports.x = x;"),
        "CommonJS auto-detect should export x"
    );
}

#[test]
fn test_two_phase_emission_commonjs_export_assignment_suppresses_named_exports() {
    let source = "export = foo;\nexport const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.auto_detect_module = true;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.set_auto_detect_module(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("module.exports = foo;"),
        "CommonJS export assignment should emit module.exports"
    );
    assert!(
        !output.contains("exports.x = x;"),
        "Named exports should be suppressed when export assignment is present"
    );
}

#[test]
fn test_two_phase_emission_commonjs_export_default_arrow() {
    let source = "export default () => this.x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let mut ctx = EmitContext::es5();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.set_module_kind(crate::emitter::ModuleKind::CommonJS);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("exports.default ="),
        "CommonJS default export should emit exports.default: {}",
        output
    );
    assert!(
        output.contains("function (_this)"),
        "ES5 arrow export should capture this via IIFE: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 arrow export should not contain '=>': {}",
        output
    );
}

#[test]
fn test_transform_directive_chain_es5_class_commonjs_export() {
    let source = "class Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let class_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected class declaration");
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");
    let name_idx = class_data.name;
    assert!(!name_idx.is_none(), "expected class name");
    let name_node = arena.get(name_idx).expect("expected class name node");
    let name_id = name_node.data_index;

    let mut transforms = TransformContext::new();
    transforms.insert(
        class_idx,
        TransformDirective::Chain(vec![
            TransformDirective::ES5Class {
                class_node: class_idx,
                heritage: None,
            },
            TransformDirective::CommonJSExport {
                names: Arc::from(vec![name_id]),
                is_default: false,
                inner: Box::new(TransformDirective::Identity),
            },
        ]),
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("var Foo = /** @class */"),
        "Chained ES5 class transform should emit IIFE: {}",
        output
    );
    assert!(
        output.contains("exports.Foo = Foo;"),
        "Chained CommonJS export should emit assignment: {}",
        output
    );
    assert!(
        !output.contains("class Foo"),
        "Chained transforms should downlevel class syntax: {}",
        output
    );
}

#[test]
fn test_transform_directive_es5_class_emits_members_from_ast() {
    let source = "class Foo { method() { return 1; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.arena;

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let class_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected class declaration");

    let mut transforms = TransformContext::new();
    transforms.insert(
        class_idx,
        TransformDirective::ES5Class {
            class_node: class_idx,
            heritage: None,
        },
    );

    let mut printer = Printer::with_transforms(&arena, transforms);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();
    assert!(
        output.contains("prototype.method"),
        "ES5 class emit should include prototype members: {}",
        output
    );
    assert!(
        !output.contains("class Foo"),
        "ES5 class transform should downlevel syntax: {}",
        output
    );
}

#[test]
fn test_transform_directive_composability() {
    // This test verifies that the architecture supports composable transforms
    // For now, we just verify that the TransformContext can be created and passed around
    use crate::parser::NodeIndex;

    let mut ctx = TransformContext::new();

    // Add a transform directive
    ctx.insert(
        NodeIndex(1),
        TransformDirective::ES5Class {
            class_node: NodeIndex(1),
            heritage: None,
        },
    );

    // Verify it was stored
    assert!(ctx.has_transform(NodeIndex(1)));
    assert!(!ctx.has_transform(NodeIndex(2)));
}
