use super::*;
use crate::context::emit::EmitContext;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::node::NodeArena;

fn parse(source: &str) -> (NodeArena, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser.arena, root)
}

fn make_lowering<'a>(arena: &'a NodeArena, ctx: &'a EmitContext) -> LoweringPass<'a> {
    LoweringPass::new(arena, ctx)
}

// =============================================================================
// has_const_modifier
// =============================================================================

#[test]
fn test_has_const_modifier_on_const_enum() {
    let (arena, root) = parse("const enum Color { Red, Green, Blue }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = arena.get(stmt_idx).expect("expected enum node");
    let enum_decl = arena.get_enum(stmt_node).expect("expected enum data");

    assert!(
        lowering.has_const_modifier(&enum_decl.modifiers),
        "Expected const modifier on const enum"
    );
}

#[test]
fn test_has_const_modifier_on_regular_enum() {
    let (arena, root) = parse("enum Color { Red, Green, Blue }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = arena.get(stmt_idx).expect("expected enum node");
    let enum_decl = arena.get_enum(stmt_node).expect("expected enum data");

    assert!(
        !lowering.has_const_modifier(&enum_decl.modifiers),
        "Expected no const modifier on regular enum"
    );
}

// =============================================================================
// is_static_member
// =============================================================================

#[test]
fn test_is_static_member_on_static_method() {
    let (arena, root) = parse("class C { static count() {} }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");
    let member_idx = class_data.members.nodes[0];

    assert!(
        lowering.is_static_member(member_idx),
        "Expected static method to be detected as static"
    );
}

#[test]
fn test_is_static_member_on_instance_method() {
    let (arena, root) = parse("class C { greet() {} }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");
    let member_idx = class_data.members.nodes[0];

    assert!(
        !lowering.is_static_member(member_idx),
        "Expected instance method to not be detected as static"
    );
}

#[test]
fn test_is_static_member_on_none_index() {
    let arena = NodeArena::new();
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    assert!(
        !lowering.is_static_member(NodeIndex::NONE),
        "NONE index should not be static"
    );
}

// =============================================================================
// has_async_modifier
// =============================================================================

#[test]
fn test_has_async_modifier_on_async_function() {
    let (arena, root) = parse("async function fetch() { await 1; }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let func_idx = source_file.statements.nodes[0];

    assert!(
        lowering.has_async_modifier(func_idx),
        "Expected async modifier on async function"
    );
}

#[test]
fn test_has_async_modifier_on_sync_function() {
    let (arena, root) = parse("function fetch() { return 1; }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let func_idx = source_file.statements.nodes[0];

    assert!(
        !lowering.has_async_modifier(func_idx),
        "Expected no async modifier on sync function"
    );
}

// =============================================================================
// get_extends_heritage
// =============================================================================

#[test]
fn test_get_extends_heritage_with_extends() {
    let (arena, root) = parse("class Dog extends Animal {}");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");

    assert!(
        lowering
            .get_extends_heritage(&class_data.heritage_clauses)
            .is_some(),
        "Expected extends heritage clause"
    );
}

#[test]
fn test_get_extends_heritage_without_extends() {
    let (arena, root) = parse("class Foo {}");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");

    assert!(
        lowering
            .get_extends_heritage(&class_data.heritage_clauses)
            .is_none(),
        "Expected no extends heritage clause for class without extends"
    );
}

#[test]
fn test_get_extends_heritage_implements_only() {
    let (arena, root) = parse("class Foo implements Bar {}");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");

    assert!(
        lowering
            .get_extends_heritage(&class_data.heritage_clauses)
            .is_none(),
        "Expected no extends heritage clause for implements-only class"
    );
}

// =============================================================================
// mark_async_helpers
// =============================================================================

#[test]
fn test_mark_async_helpers_sets_awaiter() {
    let (arena, root) = parse("async function f() { await 1; }");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        helpers.awaiter,
        "Expected __awaiter helper for async function"
    );
}

#[test]
fn test_mark_async_helpers_sets_generator_for_es5() {
    let (arena, root) = parse("async function f() { await 1; }");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        helpers.generator,
        "Expected __generator helper for ES5 async function"
    );
}

#[test]
fn test_mark_async_helpers_no_generator_for_es2015() {
    let (arena, root) = parse("async function f() { await 1; }");
    let mut ctx = EmitContext::default();
    ctx.set_target(ScriptTarget::ES2015);

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        !helpers.generator,
        "Expected no __generator helper for ES2015 async function (has native generators)"
    );
}

// =============================================================================
// mark_class_helpers - extends
// =============================================================================

#[test]
fn test_mark_class_helpers_extends() {
    let (arena, root) = parse("class Dog extends Animal { bark() {} }");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        helpers.extends,
        "Expected __extends helper for class with extends"
    );
}

#[test]
fn test_mark_class_helpers_no_extends() {
    let (arena, root) = parse("class Foo { bar() {} }");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        !helpers.extends,
        "Expected no __extends helper for class without extends"
    );
}

// =============================================================================
// class_has_private_members
// =============================================================================

#[test]
fn test_class_has_private_members_detection() {
    // Verify the lowering pass detects private members in the class
    let (arena, root) = parse("class C { #value = 42; }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");

    assert!(
        lowering.class_has_private_members(class_data),
        "Expected private member detection for class with #value"
    );
}

#[test]
fn test_class_has_no_private_members() {
    let (arena, root) = parse("class C { value = 42; }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");

    assert!(
        !lowering.class_has_private_members(class_data),
        "Expected no private member detection for class with public field"
    );
}

#[test]
fn test_class_private_field_helpers_at_es2015() {
    // At ES2015+, private fields need __classPrivateFieldGet/Set helpers
    // because classes are native but private fields aren't
    let (arena, root) = parse("class C { #x = 1; foo() { return this.#x; } }");
    let mut ctx = EmitContext::default();
    ctx.set_target(ScriptTarget::ES2015);

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        helpers.class_private_field_get,
        "Expected __classPrivateFieldGet helper for ES2015 class with private field read"
    );
}

// =============================================================================
// class_has_decorators
// =============================================================================

#[test]
fn test_class_has_decorators_class_level() {
    let (arena, root) = parse("@sealed class Foo {}");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");

    assert!(
        lowering.class_has_decorators(class_data),
        "Expected decorators on decorated class"
    );
}

#[test]
fn test_class_has_decorators_member_level() {
    let (arena, root) = parse("class Foo { @log greet() {} }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");

    assert!(
        lowering.class_has_decorators(class_data),
        "Expected decorators on class with decorated member"
    );
}

#[test]
fn test_class_has_decorators_none() {
    let (arena, root) = parse("class Foo { greet() {} }");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("expected class node");
    let class_data = arena.get_class(class_node).expect("expected class data");

    assert!(
        !lowering.class_has_decorators(class_data),
        "Expected no decorators on plain class"
    );
}

// =============================================================================
// needs_es5_array_literal_transform (via integration)
// =============================================================================

#[test]
fn test_array_spread_marks_spread_array_helper() {
    // Array spread [1, ...other] should produce a __spreadArray transform
    let (arena, root) = parse("const arr = [1, ...other];");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        helpers.spread_array,
        "Expected __spreadArray helper for array with spread"
    );
}

#[test]
fn test_array_no_spread_no_helper() {
    // Array without spread [1, 2, 3] should NOT produce __spreadArray
    let (arena, root) = parse("const arr = [1, 2, 3];");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        !helpers.spread_array,
        "Expected no __spreadArray helper for array without spread"
    );
}

// =============================================================================
// function_parameters_need_es5_transform
// =============================================================================

#[test]
fn test_function_params_need_transform_with_rest() {
    let (arena, root) = parse("function f(...args: any[]) {}");
    let ctx = EmitContext::es5();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let func_idx = source_file.statements.nodes[0];
    let func_node = arena.get(func_idx).expect("expected func node");
    let func = arena.get_function(func_node).expect("expected func data");

    assert!(
        lowering.function_parameters_need_es5_transform(&func.parameters),
        "Expected rest parameter to need ES5 transform"
    );
}

#[test]
fn test_function_params_need_transform_with_default() {
    let (arena, root) = parse("function f(x: number = 5) {}");
    let ctx = EmitContext::es5();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let func_idx = source_file.statements.nodes[0];
    let func_node = arena.get(func_idx).expect("expected func node");
    let func = arena.get_function(func_node).expect("expected func data");

    assert!(
        lowering.function_parameters_need_es5_transform(&func.parameters),
        "Expected default parameter to need ES5 transform"
    );
}

#[test]
fn test_function_params_no_transform_needed() {
    let (arena, root) = parse("function f(x: number, y: string) {}");
    let ctx = EmitContext::es5();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let func_idx = source_file.statements.nodes[0];
    let func_node = arena.get(func_idx).expect("expected func node");
    let func = arena.get_function(func_node).expect("expected func data");

    assert!(
        !lowering.function_parameters_need_es5_transform(&func.parameters),
        "Expected simple parameters to not need ES5 transform"
    );
}

#[test]
fn test_function_params_with_destructured_param() {
    let (arena, root) = parse("function f({x, y}: Point) {}");
    let ctx = EmitContext::es5();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let func_idx = source_file.statements.nodes[0];
    let func_node = arena.get(func_idx).expect("expected func node");
    let func = arena.get_function(func_node).expect("expected func data");

    assert!(
        lowering.function_parameters_need_es5_transform(&func.parameters),
        "Expected destructured parameter to need ES5 transform"
    );
}

// =============================================================================
// file_is_module
// =============================================================================

#[test]
fn test_file_is_module_with_import() {
    let (arena, root) = parse("import { Foo } from './foo';");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");

    assert!(
        lowering.file_is_module(&source_file.statements),
        "Expected file with import to be a module"
    );
}

#[test]
fn test_file_is_module_with_export() {
    let (arena, root) = parse("export class Foo {}");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");

    assert!(
        lowering.file_is_module(&source_file.statements),
        "Expected file with export to be a module"
    );
}

#[test]
fn test_file_is_module_script_mode() {
    let (arena, root) = parse("const x = 1;");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");

    assert!(
        !lowering.file_is_module(&source_file.statements),
        "Expected file without imports/exports to not be a module"
    );
}

// =============================================================================
// contains_export_assignment
// =============================================================================

#[test]
fn test_contains_export_assignment() {
    let (arena, root) = parse("export = MyClass;");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");

    assert!(
        lowering.contains_export_assignment(&source_file.statements),
        "Expected export assignment to be detected"
    );
}

#[test]
fn test_no_export_assignment() {
    let (arena, root) = parse("export class Foo {}");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");

    assert!(
        !lowering.contains_export_assignment(&source_file.statements),
        "Expected no export assignment in normal export"
    );
}

// =============================================================================
// get_identifier_text_ref
// =============================================================================

#[test]
fn test_get_identifier_text_ref_valid() {
    let (arena, root) = parse("function hello() {}");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let func_idx = source_file.statements.nodes[0];
    let func_node = arena.get(func_idx).expect("expected func node");
    let func = arena.get_function(func_node).expect("expected func data");

    let text = lowering.get_identifier_text_ref(func.name);
    assert_eq!(text, Some("hello"), "Expected identifier text 'hello'");
}

#[test]
fn test_get_identifier_text_ref_none_index() {
    let arena = NodeArena::new();
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let text = lowering.get_identifier_text_ref(NodeIndex::NONE);
    assert!(text.is_none(), "Expected None for NONE index");
}

// =============================================================================
// call_spread_needs_spread_array (via integration)
// =============================================================================

#[test]
fn test_call_spread_single_spread_uses_apply_not_spread_array() {
    // foo(...args) uses apply, does NOT need __spreadArray
    let (arena, root) = parse("foo(...args);");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        !helpers.spread_array,
        "Single spread call should use apply, not __spreadArray"
    );
}

#[test]
fn test_call_spread_mixed_needs_spread_array() {
    // foo(1, ...args, 2) needs __spreadArray
    let (arena, root) = parse("foo(1, ...args, 2);");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        helpers.spread_array,
        "Mixed spread call should need __spreadArray"
    );
}

// =============================================================================
// is_binding_pattern_idx
// =============================================================================

#[test]
fn test_is_binding_pattern_object() {
    let (arena, root) = parse("const {x, y} = obj;");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = arena.get(stmt_idx).expect("expected stmt");
    let var_stmt = arena.get_variable(stmt_node).expect("expected var");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = arena.get(decl_list_idx).expect("expected decl list");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("expected var data");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl_node = arena.get(decl_idx).expect("expected decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("expected decl");

    assert!(
        lowering.is_binding_pattern_idx(decl.name),
        "Expected object destructuring to be a binding pattern"
    );
}

#[test]
fn test_is_binding_pattern_array() {
    let (arena, root) = parse("const [a, b] = arr;");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = arena.get(stmt_idx).expect("expected stmt");
    let var_stmt = arena.get_variable(stmt_node).expect("expected var");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = arena.get(decl_list_idx).expect("expected decl list");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("expected var data");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl_node = arena.get(decl_idx).expect("expected decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("expected decl");

    assert!(
        lowering.is_binding_pattern_idx(decl.name),
        "Expected array destructuring to be a binding pattern"
    );
}

#[test]
fn test_is_binding_pattern_simple_identifier() {
    let (arena, root) = parse("const x = 5;");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = arena.get(stmt_idx).expect("expected stmt");
    let var_stmt = arena.get_variable(stmt_node).expect("expected var");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = arena.get(decl_list_idx).expect("expected decl list");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("expected var data");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl_node = arena.get(decl_idx).expect("expected decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("expected decl");

    assert!(
        !lowering.is_binding_pattern_idx(decl.name),
        "Expected simple identifier to not be a binding pattern"
    );
}

// =============================================================================
// binding_pattern_has_object_rest
// =============================================================================

#[test]
fn test_binding_pattern_has_object_rest() {
    let (arena, root) = parse("const {x, ...rest} = obj;");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = arena.get(stmt_idx).expect("expected stmt");
    let var_stmt = arena.get_variable(stmt_node).expect("expected var");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = arena.get(decl_list_idx).expect("expected decl list");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("expected var data");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl_node = arena.get(decl_idx).expect("expected decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("expected decl");

    assert!(
        lowering.binding_pattern_has_object_rest(decl.name),
        "Expected object rest to be detected"
    );
}

#[test]
fn test_binding_pattern_no_object_rest() {
    let (arena, root) = parse("const {x, y} = obj;");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = arena.get(stmt_idx).expect("expected stmt");
    let var_stmt = arena.get_variable(stmt_node).expect("expected var");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = arena.get(decl_list_idx).expect("expected decl list");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("expected var data");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl_node = arena.get(decl_idx).expect("expected decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("expected decl");

    assert!(
        !lowering.binding_pattern_has_object_rest(decl.name),
        "Expected no object rest in simple destructuring"
    );
}

// =============================================================================
// collect_module_dependencies
// =============================================================================

#[test]
fn test_collect_module_dependencies() {
    let (arena, root) = parse("import { Foo } from './foo';\nimport { Bar } from './bar';");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");

    let deps = lowering.collect_module_dependencies(&source_file.statements.nodes);
    assert!(
        deps.contains(&"./foo".to_string()),
        "Expected './foo' dependency. Found: {deps:?}"
    );
    assert!(
        deps.contains(&"./bar".to_string()),
        "Expected './bar' dependency. Found: {deps:?}"
    );
}

#[test]
fn test_collect_module_dependencies_no_duplicates() {
    let (arena, root) = parse("import { Foo } from './foo';\nimport { Bar } from './foo';");
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    let root_node = arena.get(root).expect("expected root");
    let source_file = arena.get_source_file(root_node).expect("expected sf");

    let deps = lowering.collect_module_dependencies(&source_file.statements.nodes);
    assert_eq!(
        deps.len(),
        1,
        "Expected single unique dependency, not duplicates. Found: {deps:?}"
    );
}

#[test]
fn test_is_commonjs_default_false() {
    let arena = NodeArena::new();
    let ctx = EmitContext::default();
    let lowering = make_lowering(&arena, &ctx);

    assert!(
        !lowering.is_commonjs(),
        "Expected default to not be CommonJS mode"
    );
}

// =============================================================================
// Integration: Lowering pass identifies helper needs correctly
// =============================================================================

#[test]
fn test_lowering_spread_in_call_marks_spread_array() {
    let (arena, root) = parse("foo(1, ...args, 2);");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        helpers.spread_array,
        "Expected __spreadArray helper for mixed spread call"
    );
}

#[test]
fn test_lowering_destructuring_marks_rest_helper() {
    let (arena, root) = parse("const {x, ...rest} = obj;");
    let ctx = EmitContext::es5();

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let helpers = transforms.helpers();
    assert!(
        helpers.rest,
        "Expected __rest helper for object rest destructuring"
    );
}
