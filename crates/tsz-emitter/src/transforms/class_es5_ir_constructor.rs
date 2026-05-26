//! Constructor IR emission for ES5 class transformer.
//!
//! Extracted from `class_es5_ir.rs` to keep file sizes manageable.
//! Contains `emit_constructor_ir` and related constructor/super-call helpers.

use super::{
    AutoAccessorFieldInfo, ES5ClassTransformer, get_identifier_text,
    has_parameter_property_modifier,
};
use crate::transforms::ir::{
    IRCatchClause, IRMethodName, IRNode, IRParam, IRProperty, IRPropertyDescriptor, IRPropertyKey,
    IRPropertyKind,
};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::{
    contains_super_reference, contains_this_reference, is_private_identifier,
};
use tsz_scanner::SyntaxKind;

impl<'a> ES5ClassTransformer<'a> {
    pub(super) fn emit_constructor_ir(&self, class_idx: NodeIndex) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        // Collect instance property initializers (non-private only)
        let instance_props: Vec<NodeIndex> = class_data
            .members
            .nodes
            .iter()
            .filter_map(|&member_idx| {
                let member_node = self.arena.get(member_idx)?;
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    return None;
                }
                let prop_data = self.arena.get_property_decl(member_node)?;
                // Skip static properties
                if self.arena.is_static(&prop_data.modifiers) {
                    return None;
                }
                // Skip abstract properties (they don't exist at runtime)
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                {
                    return None;
                }
                // Skip `declare` properties — ambient/type-only declarations have no runtime representation
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                {
                    return None;
                }
                // Skip private fields (they use WeakMap pattern)
                if is_private_identifier(self.arena, prop_data.name) {
                    return None;
                }
                // Skip accessor fields (emitted as getter/setter pair + backing storage)
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                {
                    return None;
                }
                (self.use_define_for_class_fields
                    || self.property_initializer_has_equals(member_node, prop_data)
                    || self.tc39_es5_decorated_field(member_idx).is_some())
                .then_some(member_idx)
            })
            .collect();

        // Find constructor implementation
        let mut constructor_data = None;
        let mut constructor_member_node: Option<&tsz_parser::parser::node::Node> = None;
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor_data) = self.arena.get_constructor(member_node) else {
                    continue;
                };
                // Only use constructor with body (not overload signatures)
                if ctor_data.body.is_some() {
                    constructor_member_node = Some(member_node);
                    constructor_data = Some(ctor_data);
                    break;
                }
            }
        }

        // Build constructor body
        let mut ctor_body = Vec::new();
        let mut params = Vec::new();
        let mut body_source_range = None;
        let mut trailing_comment = None;
        let mut leading_comment = None;
        let has_private_fields = self.private_fields.iter().any(|f| !f.is_static);
        let constructor_temps_before = self.extra_hoisted_temps.borrow().len();

        if let Some(ctor) = constructor_data {
            // Extract parameters
            params = self.extract_parameters(&ctor.parameters);
            trailing_comment = self.extract_trailing_comment_for_method(ctor.body);
            // Extract leading JSDoc/block comment from the constructor declaration.
            if let Some(member_node) = constructor_member_node {
                leading_comment = self.extract_leading_comment(member_node);
            }
            // ES5 class-lowered constructors should follow TypeScript's normalized
            // multi-line function body formatting, not original source single-line shape.
            body_source_range = None;

            if self.has_extends {
                // Derived class with explicit constructor
                self.emit_derived_constructor_body_ir(
                    &mut ctor_body,
                    ctor.body,
                    &ctor.parameters,
                    &instance_props,
                );
            } else {
                // Non-derived class with explicit constructor
                self.emit_base_constructor_body_ir(
                    &mut ctor_body,
                    ctor.body,
                    &ctor.parameters,
                    &instance_props,
                );
            }
            let constructor_scope_contains_new_target =
                self.constructor_body_or_params_contain_new_target(ctor.body, &ctor.parameters);
            let moved_initializers_contain_new_target =
                self.moved_instance_initializers_contain_new_target(&instance_props);
            if constructor_scope_contains_new_target {
                ctor_body.insert(0, Self::class_constructor_new_target_capture_ir());
            }
            if moved_initializers_contain_new_target
                && (!constructor_scope_contains_new_target
                    || (self.has_extends && !self.extends_null))
            {
                self.insert_class_new_target_capture(&mut ctor_body);
            }
        } else {
            // Default constructor
            let moved_initializers_contain_new_target =
                self.moved_instance_initializers_contain_new_target(&instance_props);
            if self.has_extends && !self.extends_null {
                if instance_props.is_empty() && !has_private_fields {
                    // Simple: return _super !== null && _super.apply(this, arguments) || this;
                    ctor_body.push(IRNode::ret(Some(IRNode::logical_or(
                        IRNode::logical_and(
                            IRNode::binary(
                                IRNode::id(self.super_name.clone()),
                                "!==",
                                IRNode::NullLiteral,
                            ),
                            IRNode::call(
                                IRNode::prop(IRNode::id(self.super_name.clone()), "apply"),
                                vec![IRNode::this(), IRNode::id("arguments")],
                            ),
                        ),
                        IRNode::this(),
                    ))));
                } else {
                    // var _this = _super !== null && _super.apply(this, arguments) || this;
                    ctor_body.push(IRNode::var_decl(
                        "_this",
                        Some(IRNode::logical_or(
                            IRNode::logical_and(
                                IRNode::binary(
                                    IRNode::id(self.super_name.clone()),
                                    "!==",
                                    IRNode::NullLiteral,
                                ),
                                IRNode::call(
                                    IRNode::prop(IRNode::id(self.super_name.clone()), "apply"),
                                    vec![IRNode::this(), IRNode::id("arguments")],
                                ),
                            ),
                            IRNode::this(),
                        )),
                    ));
                    if moved_initializers_contain_new_target {
                        ctor_body.push(Self::class_constructor_new_target_capture_ir());
                    }

                    // Private field initializations
                    self.emit_private_field_initializations_ir(&mut ctor_body, true);
                    self.emit_private_accessor_initializations_ir(&mut ctor_body, true);
                    self.emit_auto_accessor_initializations_ir(&mut ctor_body, true);

                    // Instance property initializations
                    for &prop_idx in &instance_props {
                        self.emit_property_leading_comment(&mut ctor_body, prop_idx);
                        if let Some(ir) = self.emit_property_initializer_ir(prop_idx, true) {
                            ctor_body.push(ir);
                        }
                    }
                    self.emit_tc39_instance_field_extra_initializers_ir(&mut ctor_body, true);

                    // return _this;
                    ctor_body.push(IRNode::ret(Some(IRNode::id("_this"))));
                }
            } else {
                // Non-derived class default constructor
                // Check if instance property initializers need _this capture
                if self.instance_props_need_this_capture(&instance_props) {
                    ctor_body.push(IRNode::var_decl("_this", Some(IRNode::this())));
                }
                if moved_initializers_contain_new_target {
                    ctor_body.push(Self::class_constructor_new_target_capture_ir());
                }

                // Emit private field initializations
                self.emit_private_field_initializations_ir(&mut ctor_body, false);
                self.emit_private_accessor_initializations_ir(&mut ctor_body, false);
                self.emit_auto_accessor_initializations_ir(&mut ctor_body, false);

                // Instance property initializations
                for &prop_idx in &instance_props {
                    self.emit_property_leading_comment(&mut ctor_body, prop_idx);
                    if let Some(ir) = self.emit_property_initializer_ir(prop_idx, false) {
                        ctor_body.push(ir);
                    }
                }
                self.emit_tc39_instance_field_extra_initializers_ir(&mut ctor_body, false);
            }
        }

        self.insert_constructor_hoisted_temps(&mut ctor_body, constructor_temps_before);

        let ctor_fn = IRNode::FunctionDecl {
            name: self.class_name.clone().into(),
            parameters: params,
            body: ctor_body,
            body_source_range,
            leading_comment,
        };

        if let Some(comment) = trailing_comment {
            Some(IRNode::Sequence(vec![
                ctor_fn,
                IRNode::TrailingComment(comment.into()),
            ]))
        } else {
            Some(ctor_fn)
        }
    }

    /// Emit derived class constructor body with `super()` transformation
    fn emit_derived_constructor_body_ir(
        &self,
        body: &mut Vec<IRNode>,
        body_idx: NodeIndex,
        params: &NodeList,
        instance_props: &[NodeIndex],
    ) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };

        // Find super() call
        let mut super_stmt_idx = None;
        let mut super_stmt_position = 0;
        for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if self.is_super_call_statement(stmt_idx) {
                super_stmt_idx = Some(stmt_idx);
                super_stmt_position = i;
                break;
            }
        }

        // Check if we can use the simple `return _super.call(this, ...) || this;` form.
        // This optimization applies when the constructor body has super() as its only statement
        // and there's no additional work to do (no parameter properties, instance props,
        // private fields, or arrow functions capturing `this`).
        let has_param_props = params.nodes.iter().any(|&p| {
            self.arena
                .get(p)
                .and_then(|n| self.arena.get_parameter(n))
                .map(|param| has_parameter_property_modifier(self.arena, &param.modifiers))
                .unwrap_or(false)
        });
        let has_destructuring_params = params.nodes.iter().any(|&p| {
            self.arena
                .get(p)
                .and_then(|n| self.arena.get_parameter(n))
                .and_then(|param| self.arena.get(param.name))
                .is_some_and(|name| {
                    name.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                })
        });
        let has_private_fields = self.private_fields.iter().any(|f| !f.is_static);
        let has_auto_accessors = self.auto_accessors.iter().any(|a| !a.is_static);
        let has_private_accessors = self.private_accessors.iter().any(|a| !a.is_static);
        let stmts_after_super = super_stmt_idx
            .map(|_| block.statements.nodes.len() - super_stmt_position - 1)
            .unwrap_or(0);
        let needs_this_capture = self.constructor_needs_this_capture(body_idx);
        let needs_pre_super_this_capture =
            self.derived_constructor_needs_pre_super_this_capture(block, super_stmt_position);
        let has_top_level_using = block.statements.nodes.iter().any(|&stmt_idx| {
            self.using_declaration_list_for_statement(stmt_idx)
                .is_some()
        });

        if has_top_level_using {
            self.emit_derived_constructor_body_with_using_ir(
                body,
                body_node,
                block,
                super_stmt_idx,
                super_stmt_position,
                params,
                instance_props,
            );
            return;
        }

        if super_stmt_idx.is_none() && contains_super_reference(self.arena, body_idx) {
            self.emit_derived_constructor_body_with_nested_super_ir(
                body,
                body_node,
                block,
                params,
                instance_props,
            );
            return;
        }

        let can_use_tail_super_return = super_stmt_idx.is_some()
            && stmts_after_super == 0
            && instance_props.is_empty()
            && !has_param_props
            && !has_destructuring_params
            && !has_private_fields
            && !has_auto_accessors
            && !has_private_accessors
            && !needs_pre_super_this_capture
            && !needs_this_capture;

        if can_use_tail_super_return {
            let mut prev_stmt_end = body_node.pos;
            for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
                if i >= super_stmt_position {
                    break;
                }
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.emit_leading_statement_comments(body, prev_stmt_end, stmt_node.pos);
                    prev_stmt_end = stmt_node.end;
                }
                body.push(self.convert_statement(stmt_idx));
            }

            if let Some(super_idx) = super_stmt_idx {
                if let Some(super_node) = self.arena.get(super_idx) {
                    self.emit_leading_statement_comments(body, prev_stmt_end, super_node.pos);
                }
                // Tail form: earlier statements remain intact, then the final
                // `super()` can return directly without materializing `_this`.
                let super_return = self.emit_super_call_return_ir(super_idx);
                body.push(super_return);
            }
            return;
        }

        // Snapshot hoisted temps before processing constructor body so we can
        // separate temps generated inside the constructor from class-level temps.
        let temps_before = self.extra_hoisted_temps.borrow().len();
        let saved_temp_counter = self.temp_var_counter.get();
        self.temp_var_counter.set(0);

        if super_stmt_idx.is_some() && needs_pre_super_this_capture {
            body.push(IRNode::var_decl("_this", Some(IRNode::this())));
        }

        // Emit statements before super(). When they reference `this` or
        // `super.property`, tsc preserves the invalid pre-super shape by routing
        // those references through a preinitialized `_this` capture.
        let mut prev_stmt_end = body_node.pos;
        for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if i >= super_stmt_position && super_stmt_idx.is_some() {
                break;
            }
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                self.emit_leading_statement_comments(body, prev_stmt_end, stmt_node.pos);
                prev_stmt_end = stmt_node.end;
            }
            let statement = if needs_pre_super_this_capture {
                self.convert_statement_pre_super_this_captured(stmt_idx)
            } else {
                self.convert_statement(stmt_idx)
            };
            body.push(statement);
        }

        // Emit super() as either `var _this = _super.call(...) || this` or, when
        // a pre-super capture already exists, `_this = _super.call(...) || this`.
        if let Some(super_idx) = super_stmt_idx {
            if needs_pre_super_this_capture {
                body.push(IRNode::expr_stmt(
                    self.emit_super_call_assignment_ir_with_arg_capture(super_idx, true),
                ));
            } else {
                let super_call = self.emit_super_call_ir(super_idx);
                body.push(super_call);
            }
        }

        // Emit destructuring prologue for binding-pattern parameters
        {
            let ir_params = self.extract_parameters(params);
            let prologue = self.generate_destructuring_prologue(params, &ir_params);
            body.extend(prologue);
        }

        // Emit parameter properties
        self.emit_parameter_properties_ir(body, params, true);

        // Emit private field initializations
        self.emit_private_field_initializations_ir(body, true);
        self.emit_private_accessor_initializations_ir(body, true);
        self.emit_auto_accessor_initializations_ir(body, true);

        // Emit instance property initializers
        for &prop_idx in instance_props {
            self.emit_property_leading_comment(body, prop_idx);
            if let Some(ir) = self.emit_property_initializer_ir(prop_idx, true) {
                body.push(ir);
            }
        }
        self.emit_tc39_instance_field_extra_initializers_ir(body, true);

        // Emit remaining statements after super()
        // In derived constructors, `this` becomes `_this` after super() call
        if super_stmt_idx.is_some() {
            for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
                if i <= super_stmt_position {
                    continue;
                }
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.emit_leading_statement_comments(body, prev_stmt_end, stmt_node.pos);
                    prev_stmt_end = stmt_node.end;
                }
                body.push(self.convert_statement_this_captured(stmt_idx));
            }
        }

        // Hoist temps generated during constructor body to the top of the
        // constructor function, not the class IIFE.
        self.insert_constructor_hoisted_temps(body, temps_before);
        self.temp_var_counter.set(saved_temp_counter);

        let remaining_can_complete_normally = if super_stmt_idx.is_some() {
            self.statements_can_complete_normally(
                &block.statements.nodes[(super_stmt_position + 1)..],
            )
        } else {
            true
        };

        // return _this;
        if super_stmt_idx.is_some() && remaining_can_complete_normally {
            body.push(IRNode::ret(Some(IRNode::id("_this"))));
        }
    }

    fn emit_derived_constructor_body_with_using_ir(
        &self,
        body: &mut Vec<IRNode>,
        body_node: &Node,
        block: &tsz_parser::parser::node::BlockData,
        super_stmt_idx: Option<NodeIndex>,
        super_stmt_position: usize,
        params: &NodeList,
        instance_props: &[NodeIndex],
    ) {
        let temps_before = self.extra_hoisted_temps.borrow().len();
        let saved_temp_counter = self.temp_var_counter.get();
        self.temp_var_counter.set(0);
        let (env_name, error_name) = self.next_constructor_disposable_env_names();

        body.push(IRNode::var_decl("_this", Some(IRNode::this())));
        body.push(IRNode::var_decl(
            env_name.clone(),
            Some(Self::disposable_env_initializer_ir()),
        ));

        let mut try_body = Vec::new();
        let mut prev_stmt_end = body_node.pos;
        for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if i >= super_stmt_position && super_stmt_idx.is_some() {
                break;
            }
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                self.emit_leading_statement_comments(&mut try_body, prev_stmt_end, stmt_node.pos);
                prev_stmt_end = stmt_node.end;
            }
            try_body.push(
                self.convert_constructor_statement_with_using_env(stmt_idx, &env_name, false),
            );
        }

        if let Some(super_idx) = super_stmt_idx {
            try_body.push(IRNode::expr_stmt(
                self.emit_super_call_assignment_ir(super_idx),
            ));
        }

        {
            let ir_params = self.extract_parameters(params);
            let prologue = self.generate_destructuring_prologue(params, &ir_params);
            try_body.extend(prologue);
        }

        self.emit_parameter_properties_ir(&mut try_body, params, true);
        self.emit_private_field_initializations_ir(&mut try_body, true);
        self.emit_private_accessor_initializations_ir(&mut try_body, true);
        self.emit_auto_accessor_initializations_ir(&mut try_body, true);

        for &prop_idx in instance_props {
            self.emit_property_leading_comment(&mut try_body, prop_idx);
            if let Some(ir) = self.emit_property_initializer_ir(prop_idx, true) {
                try_body.push(ir);
            }
        }
        self.emit_tc39_instance_field_extra_initializers_ir(&mut try_body, true);

        if super_stmt_idx.is_some() {
            for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
                if i <= super_stmt_position {
                    continue;
                }
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.emit_leading_statement_comments(
                        &mut try_body,
                        prev_stmt_end,
                        stmt_node.pos,
                    );
                    prev_stmt_end = stmt_node.end;
                }
                try_body.push(
                    self.convert_constructor_statement_with_using_env(stmt_idx, &env_name, true),
                );
            }
        }

        body.push(IRNode::TryStatement {
            try_block: Box::new(IRNode::Block(try_body)),
            catch_clause: Some(IRCatchClause {
                param: Some(error_name.clone().into()),
                body: vec![
                    IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::id(env_name.clone()), "error"),
                        IRNode::id(error_name),
                    )),
                    IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
                        IRNode::BooleanLiteral(true),
                    )),
                ],
            }),
            finally_block: Some(Box::new(IRNode::Block(vec![IRNode::expr_stmt(
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                    arguments: vec![IRNode::id(env_name)],
                },
            )]))),
        });

        self.insert_constructor_hoisted_temps(body, temps_before);
        self.temp_var_counter.set(saved_temp_counter);

        let remaining_can_complete_normally = if super_stmt_idx.is_some() {
            self.statements_can_complete_normally(
                &block.statements.nodes[(super_stmt_position + 1)..],
            )
        } else {
            true
        };

        if super_stmt_idx.is_some() && remaining_can_complete_normally {
            body.push(IRNode::ret(Some(IRNode::id("_this"))));
        }
    }

    fn emit_derived_constructor_body_with_nested_super_ir(
        &self,
        body: &mut Vec<IRNode>,
        body_node: &Node,
        block: &tsz_parser::parser::node::BlockData,
        params: &NodeList,
        instance_props: &[NodeIndex],
    ) {
        let temps_before = self.extra_hoisted_temps.borrow().len();
        let saved_temp_counter = self.temp_var_counter.get();
        self.temp_var_counter.set(0);

        body.push(IRNode::var_decl("_this", Some(IRNode::this())));

        {
            let ir_params = self.extract_parameters(params);
            let prologue = self.generate_destructuring_prologue(params, &ir_params);
            body.extend(prologue);
        }

        self.emit_parameter_properties_ir(body, params, true);
        self.emit_private_field_initializations_ir(body, true);
        self.emit_private_accessor_initializations_ir(body, true);
        self.emit_auto_accessor_initializations_ir(body, true);

        for &prop_idx in instance_props {
            self.emit_property_leading_comment(body, prop_idx);
            if let Some(ir) = self.emit_property_initializer_ir(prop_idx, true) {
                body.push(ir);
            }
        }
        self.emit_tc39_instance_field_extra_initializers_ir(body, true);

        let mut prev_stmt_end = body_node.pos;
        for &stmt_idx in &block.statements.nodes {
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                self.emit_leading_statement_comments(body, prev_stmt_end, stmt_node.pos);
                prev_stmt_end = stmt_node.end;
            }
            body.push(self.convert_statement_this_captured(stmt_idx));
        }

        self.insert_constructor_hoisted_temps(body, temps_before);
        self.temp_var_counter.set(saved_temp_counter);

        if self.statements_can_complete_normally(&block.statements.nodes) {
            body.push(IRNode::ret(Some(IRNode::id("_this"))));
        }
    }

    fn convert_constructor_statement_with_using_env(
        &self,
        stmt_idx: NodeIndex,
        env_name: &str,
        capture_this: bool,
    ) -> IRNode {
        if let Some(ir) =
            self.convert_using_variable_statement_for_env(stmt_idx, env_name, capture_this)
        {
            return ir;
        }

        if capture_this {
            self.convert_statement_this_captured(stmt_idx)
        } else {
            self.convert_statement(stmt_idx)
        }
    }

    fn convert_using_variable_statement_for_env(
        &self,
        stmt_idx: NodeIndex,
        env_name: &str,
        capture_this: bool,
    ) -> Option<IRNode> {
        let (decl_list, flags) = self.using_declaration_list_for_statement(stmt_idx)?;
        let using_async = node_flags::is_await_using(flags);
        let mut declarations = Vec::new();

        for &decl_idx in &decl_list.declarations.nodes {
            let decl_node = self.arena.get(decl_idx)?;
            let decl = self.arena.get_variable_declaration(decl_node)?;
            let name = get_identifier_text(self.arena, decl.name)?;
            let value = if decl.initializer.is_none() {
                IRNode::Undefined
            } else if capture_this {
                self.convert_expression_this_captured(decl.initializer)
            } else {
                self.convert_expression(decl.initializer)
            };
            declarations.push(IRNode::var_decl(
                name,
                Some(IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                    arguments: vec![
                        IRNode::id(env_name.to_string()),
                        value,
                        IRNode::BooleanLiteral(using_async),
                    ],
                }),
            ));
        }

        match declarations.len() {
            0 => None,
            1 => declarations.into_iter().next(),
            _ => Some(IRNode::VarDeclList(declarations)),
        }
    }

    pub(super) fn convert_using_variable_statement_for_env_with_context(
        &self,
        stmt_idx: NodeIndex,
        env_name: &str,
        is_static: bool,
        class_alias: Option<&str>,
        lexical_this_capture_alias: Option<&str>,
    ) -> Option<IRNode> {
        let (decl_list, flags) = self.using_declaration_list_for_statement(stmt_idx)?;
        let using_async = node_flags::is_await_using(flags);
        let mut declarations = Vec::new();

        for &decl_idx in &decl_list.declarations.nodes {
            let decl_node = self.arena.get(decl_idx)?;
            let decl = self.arena.get_variable_declaration(decl_node)?;
            let name = get_identifier_text(self.arena, decl.name)?;
            let value = if decl.initializer.is_none() {
                IRNode::Undefined
            } else {
                self.convert_expression_with_context(
                    decl.initializer,
                    is_static,
                    class_alias,
                    lexical_this_capture_alias,
                )
            };
            declarations.push(IRNode::var_decl(
                name,
                Some(IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                    arguments: vec![
                        IRNode::id(env_name.to_string()),
                        value,
                        IRNode::BooleanLiteral(using_async),
                    ],
                }),
            ));
        }

        match declarations.len() {
            0 => None,
            1 => declarations.into_iter().next(),
            _ => Some(IRNode::VarDeclList(declarations)),
        }
    }

    pub(super) fn block_has_using_declarations(&self, statements: &NodeList) -> bool {
        statements.nodes.iter().any(|&stmt_idx| {
            self.using_declaration_list_for_statement(stmt_idx)
                .is_some()
        })
    }

    fn using_declaration_list_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(&tsz_parser::parser::node::VariableData, u32)> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return None;
        }

        let var_stmt = self.arena.get_variable(stmt_node)?;
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let decl_list_node = self.arena.get(decl_list_idx)?;
            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                let flags = decl_list_node.flags as u32;
                if (flags & node_flags::USING) != 0 {
                    return self
                        .arena
                        .get_variable(decl_list_node)
                        .map(|decl_list| (decl_list, flags));
                }
            }
        }

        None
    }

    pub(super) fn next_constructor_disposable_env_names(&self) -> (String, String) {
        loop {
            let id = self.disposable_env_counter.get();
            self.disposable_env_counter.set(id + 1);
            let env_name = format!("env_{id}");
            let error_name = format!("e_{id}");
            if self.is_blocked_disposable_name(&env_name)
                || self.is_blocked_disposable_name(&error_name)
            {
                continue;
            }
            self.blocked_disposable_env_names
                .borrow_mut()
                .insert(env_name.clone());
            self.blocked_disposable_env_names
                .borrow_mut()
                .insert(error_name.clone());
            self.generated_disposable_env_names
                .borrow_mut()
                .extend([env_name.clone(), error_name.clone()]);
            return (env_name, error_name);
        }
    }

    fn is_blocked_disposable_name(&self, name: &str) -> bool {
        self.blocked_disposable_env_names.borrow().contains(name)
            || self
                .arena
                .identifiers
                .iter()
                .any(|identifier| identifier.escaped_text == name)
    }

    pub(super) fn disposable_env_initializer_ir() -> IRNode {
        IRNode::object(vec![
            IRProperty {
                key: IRPropertyKey::Identifier("stack".into()),
                value: IRNode::ArrayLiteral(Vec::new()),
                kind: IRPropertyKind::Init,
            },
            IRProperty {
                key: IRPropertyKey::Identifier("error".into()),
                value: IRNode::Undefined,
                kind: IRPropertyKind::Init,
            },
            IRProperty {
                key: IRPropertyKey::Identifier("hasError".into()),
                value: IRNode::BooleanLiteral(false),
                kind: IRPropertyKind::Init,
            },
        ])
    }

    pub(super) fn using_try_statement_ir(
        env_name: String,
        error_name: String,
        try_body: Vec<IRNode>,
    ) -> IRNode {
        IRNode::TryStatement {
            try_block: Box::new(IRNode::Block(try_body)),
            catch_clause: Some(IRCatchClause {
                param: Some(error_name.clone().into()),
                body: vec![
                    IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::id(env_name.clone()), "error"),
                        IRNode::id(error_name),
                    )),
                    IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
                        IRNode::BooleanLiteral(true),
                    )),
                ],
            }),
            finally_block: Some(Box::new(IRNode::Block(vec![IRNode::expr_stmt(
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                    arguments: vec![IRNode::id(env_name)],
                },
            )]))),
        }
    }

    fn statements_can_complete_normally(&self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if !self.statement_can_complete_normally(stmt_idx) {
                return false;
            }
        }
        true
    }

    fn statement_can_complete_normally(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(stmt_idx) else {
            return true;
        };

        match node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT =>
            {
                false
            }
            k if k == syntax_kind_ext::BLOCK => self
                .arena
                .get_block(node)
                .is_none_or(|block| self.statements_can_complete_normally(&block.statements.nodes)),
            k if k == syntax_kind_ext::IF_STATEMENT => {
                let Some(if_stmt) = self.arena.get_if_statement(node) else {
                    return true;
                };
                if if_stmt.else_statement.is_none() {
                    return true;
                }
                self.statement_can_complete_normally(if_stmt.then_statement)
                    || self.statement_can_complete_normally(if_stmt.else_statement)
            }
            _ => true,
        }
    }

    /// Emit base class constructor body
    fn emit_base_constructor_body_ir(
        &self,
        body: &mut Vec<IRNode>,
        body_idx: NodeIndex,
        params: &NodeList,
        instance_props: &[NodeIndex],
    ) {
        let temps_before = self.extra_hoisted_temps.borrow().len();
        let saved_temp_counter = self.temp_var_counter.get();
        self.temp_var_counter.set(0);
        let using_region_names = self
            .arena
            .get(body_idx)
            .and_then(|block_node| self.arena.get_block(block_node))
            .filter(|block| self.block_has_using_declarations(&block.statements))
            .map(|_| self.next_constructor_disposable_env_names());

        // Check if constructor body or instance property initializers contain
        // arrow functions that capture `this`.
        // TSC emits `var _this = this;` as the FIRST statement in the constructor.
        let needs_this_capture = self.constructor_needs_this_capture(body_idx)
            || self.instance_props_need_this_capture(instance_props);
        if needs_this_capture {
            // Emit: var _this = this;
            body.push(IRNode::var_decl("_this", Some(IRNode::this())));
        }

        // Emit destructuring prologue for binding-pattern parameters
        {
            let ir_params = self.extract_parameters(params);
            let prologue = self.generate_destructuring_prologue(params, &ir_params);
            body.extend(prologue);
        }

        // Emit private field initializations
        self.emit_private_field_initializations_ir(body, false);
        self.emit_private_accessor_initializations_ir(body, false);
        self.emit_auto_accessor_initializations_ir(body, false);

        // Emit parameter properties
        self.emit_parameter_properties_ir(body, params, false);

        // Emit instance property initializers
        for &prop_idx in instance_props {
            self.emit_property_leading_comment(body, prop_idx);
            if let Some(ir) = self.emit_property_initializer_ir(prop_idx, false) {
                body.push(ir);
            }
        }
        self.emit_tc39_instance_field_extra_initializers_ir(body, false);

        // Emit original constructor body
        if let Some(block_node) = self.arena.get(body_idx)
            && let Some(block) = self.arena.get_block(block_node)
        {
            let mut prev_stmt_end = block_node.pos;
            if block.statements.nodes.is_empty() {
                self.emit_empty_block_comments(body, block_node);
            } else if let Some((env_name, error_name)) = using_region_names {
                body.push(IRNode::var_decl(
                    env_name.clone(),
                    Some(Self::disposable_env_initializer_ir()),
                ));
                let mut try_body = Vec::new();
                for &stmt_idx in &block.statements.nodes {
                    if let Some(stmt_node) = self.arena.get(stmt_idx) {
                        self.emit_leading_statement_comments(
                            &mut try_body,
                            prev_stmt_end,
                            stmt_node.pos,
                        );
                        prev_stmt_end = stmt_node.end;
                    }
                    try_body.push(
                        self.convert_constructor_statement_with_using_env(
                            stmt_idx, &env_name, false,
                        ),
                    );
                }
                body.push(Self::using_try_statement_ir(env_name, error_name, try_body));
            } else {
                for &stmt_idx in &block.statements.nodes {
                    if let Some(stmt_node) = self.arena.get(stmt_idx) {
                        self.emit_leading_statement_comments(body, prev_stmt_end, stmt_node.pos);
                        prev_stmt_end = stmt_node.end;
                    }
                    body.push(self.convert_statement(stmt_idx));
                }
            }
        }

        self.insert_constructor_hoisted_temps(body, temps_before);
        self.temp_var_counter.set(saved_temp_counter);
    }

    fn insert_constructor_hoisted_temps(&self, body: &mut Vec<IRNode>, temps_before: usize) {
        let temps_after = self.extra_hoisted_temps.borrow().len();
        if temps_after <= temps_before {
            return;
        }

        let ctor_temps: Vec<String> = self
            .extra_hoisted_temps
            .borrow_mut()
            .drain(temps_before..)
            .collect();
        let var_decls: Vec<IRNode> = ctor_temps
            .into_iter()
            .map(|name| IRNode::VarDecl {
                name: name.into(),
                initializer: None,
            })
            .collect();
        body.insert(0, IRNode::VarDeclList(var_decls));
    }

    /// Check if a statement is a `super()` call
    fn is_super_call_statement(&self, stmt_idx: NodeIndex) -> bool {
        self.root_super_call_from_statement(stmt_idx).is_some()
    }

    fn root_super_call_from_statement(&self, stmt_idx: NodeIndex) -> Option<NodeIndex> {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return None;
        };

        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }

        let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
            return None;
        };
        self.root_super_call_expression(expr_stmt.expression)
    }

    fn root_super_call_expression(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = expr_idx;
        while let Some(node) = self.arena.get(current) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let Some(paren) = self.arena.get_parenthesized(node) else {
                    return None;
                };
                current = paren.expression;
                continue;
            }

            if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return None;
            }

            let call = self.arena.get_call_expr(node)?;
            let callee = self.arena.get(call.expression)?;
            return (callee.kind == SyntaxKind::SuperKeyword as u16).then_some(current);
        }
        None
    }

    fn is_parenthesized_root_super_call_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
            return false;
        };
        self.arena
            .get(expr_stmt.expression)
            .is_some_and(|node| node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION)
            && self
                .root_super_call_expression(expr_stmt.expression)
                .is_some()
    }

    /// Emit super(args) as var _this = _super.call(this, args) || this;
    fn emit_super_call_ir(&self, stmt_idx: NodeIndex) -> IRNode {
        let value = if self.is_parenthesized_root_super_call_statement(stmt_idx) {
            IRNode::Parenthesized(Box::new(IRNode::assign(
                IRNode::id("_this"),
                self.emit_super_call_assignment_value_ir_with_arg_capture(stmt_idx, true),
            )))
        } else {
            self.emit_super_call_assignment_value_ir_with_arg_capture(stmt_idx, true)
        };
        IRNode::var_decl("_this", Some(value))
    }

    fn emit_super_call_assignment_ir(&self, stmt_idx: NodeIndex) -> IRNode {
        self.emit_super_call_assignment_ir_with_arg_capture(stmt_idx, false)
    }

    fn emit_super_call_assignment_ir_with_arg_capture(
        &self,
        stmt_idx: NodeIndex,
        capture_args_this: bool,
    ) -> IRNode {
        let assignment = IRNode::assign(
            IRNode::id("_this"),
            self.emit_super_call_assignment_value_ir_with_arg_capture(stmt_idx, capture_args_this),
        );
        if self.is_parenthesized_root_super_call_statement(stmt_idx) {
            IRNode::Parenthesized(Box::new(assignment))
        } else {
            assignment
        }
    }

    fn emit_super_call_assignment_value_ir_with_arg_capture(
        &self,
        stmt_idx: NodeIndex,
        capture_args_this: bool,
    ) -> IRNode {
        let mut args = vec![IRNode::this()];

        if let Some(call_idx) = self.root_super_call_from_statement(stmt_idx)
            && let Some(call_node) = self.arena.get(call_idx)
            && let Some(call) = self.arena.get_call_expr(call_node)
            && let Some(ref call_args) = call.arguments
        {
            for &arg_idx in &call_args.nodes {
                let arg = if capture_args_this {
                    self.convert_expression_this_captured(arg_idx)
                } else {
                    self.convert_expression(arg_idx)
                };
                args.push(arg);
            }
        }

        IRNode::logical_or(
            IRNode::call(
                IRNode::prop(IRNode::id(self.super_name.clone()), "call"),
                args,
            ),
            IRNode::this(),
        )
    }

    fn derived_constructor_needs_pre_super_this_capture(
        &self,
        block: &tsz_parser::parser::node::BlockData,
        super_stmt_position: usize,
    ) -> bool {
        if super_stmt_position == 0 {
            return false;
        }
        // A pre-super this capture is only necessary when a statement that runs
        // before super() actually references `this` or `super.prop` (which
        // implicitly uses `this`). String literals, console.log() calls, and
        // other statements that don't touch `this` don't need the capture.
        for &stmt_idx in block.statements.nodes.iter().take(super_stmt_position) {
            if contains_this_reference(self.arena, stmt_idx)
                || contains_super_reference(self.arena, stmt_idx)
            {
                return true;
            }
        }
        false
    }

    /// Emit super(args) as return _super.call(this, args) || this;
    /// Used when the constructor body only contains `super()` with no other work.
    fn emit_super_call_return_ir(&self, stmt_idx: NodeIndex) -> IRNode {
        let mut args = vec![IRNode::this()];

        if let Some(call_idx) = self.root_super_call_from_statement(stmt_idx)
            && let Some(call_node) = self.arena.get(call_idx)
            && let Some(call) = self.arena.get_call_expr(call_node)
            && let Some(ref call_args) = call.arguments
        {
            for &arg_idx in &call_args.nodes {
                args.push(self.convert_expression(arg_idx));
            }
        }

        // return _super.call(this, args...) || this;
        IRNode::ret(Some(IRNode::logical_or(
            IRNode::call(
                IRNode::prop(IRNode::id(self.super_name.clone()), "call"),
                args,
            ),
            IRNode::this(),
        )))
    }

    /// Emit parameter properties (public/private/protected/readonly params)
    fn emit_parameter_properties_ir(
        &self,
        body: &mut Vec<IRNode>,
        params: &NodeList,
        use_this: bool,
    ) {
        let mut consumed_tc39_instance_initializers = false;
        let parameter_properties_consume_tc39_initializers =
            !self.tc39_has_instance_decorated_fields();
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if has_parameter_property_modifier(self.arena, &param.modifiers)
                && let Some(param_name) = get_identifier_text(self.arena, param.name)
            {
                let receiver = if use_this {
                    IRNode::id("_this")
                } else {
                    IRNode::this()
                };
                let value = if parameter_properties_consume_tc39_initializers
                    && self.tc39_instance_initializers_needed()
                    && !consumed_tc39_instance_initializers
                {
                    consumed_tc39_instance_initializers = true;
                    let receiver_text = if use_this { "_this" } else { "this" };
                    IRNode::Raw(
                        format!(
                            "(__runInitializers({receiver_text}, _instanceExtraInitializers), {param_name})"
                        )
                        .into(),
                    )
                } else {
                    IRNode::id(param_name.clone())
                };

                if self.use_define_for_class_fields {
                    body.push(IRNode::DefineProperty {
                        target: Box::new(receiver),
                        property_name: IRMethodName::Identifier(param_name.clone().into()),
                        descriptor: IRPropertyDescriptor {
                            get: None,
                            set: None,
                            value: Some(Box::new(value)),
                            get_leading_comment: None,
                            set_leading_comment: None,
                            enumerable: true,
                            configurable: true,
                            writable: true,
                            trailing_comment: None,
                        },
                        leading_comment: None,
                    });
                } else {
                    // this.param = param; or _this.param = param;
                    body.push(IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(receiver, param_name.clone()),
                        value,
                    )));
                }
            }
        }

        if parameter_properties_consume_tc39_initializers
            && self.tc39_instance_initializers_needed()
            && !consumed_tc39_instance_initializers
        {
            let receiver_text = if use_this { "_this" } else { "this" };
            body.push(IRNode::expr_stmt(IRNode::Raw(
                format!("__runInitializers({receiver_text}, _instanceExtraInitializers)").into(),
            )));
        }
    }

    pub(super) const fn tc39_instance_initializers_needed(&self) -> bool {
        self.tc39_decorators && self.tc39_has_instance_member_decorators
    }

    /// Emit private field initializations using `WeakMap.set()`
    fn emit_private_field_initializations_ir(&self, body: &mut Vec<IRNode>, use_this: bool) {
        let key = if use_this {
            IRNode::id("_this")
        } else {
            IRNode::this()
        };

        for field in &self.private_fields {
            if field.is_static {
                continue;
            }

            // _ClassName_field.set(this, void 0);
            body.push(IRNode::expr_stmt(IRNode::WeakMapSet {
                weakmap_name: field.weakmap_name.clone().into(),
                key: Box::new(key.clone()),
                value: Box::new(IRNode::Undefined),
            }));

            // If has initializer: __classPrivateFieldSet(this, _ClassName_field, value, "f");
            if field.has_initializer && field.initializer.is_some() {
                body.push(IRNode::expr_stmt(IRNode::PrivateFieldSet {
                    receiver: Box::new(key.clone()),
                    weakmap_name: field.weakmap_name.clone().into(),
                    value: Box::new(self.convert_expression(field.initializer)),
                }));
            }
        }
    }

    /// Emit private accessor initializations using `WeakMap.set()`
    fn emit_private_accessor_initializations_ir(&self, body: &mut Vec<IRNode>, use_this: bool) {
        let key = if use_this {
            IRNode::id("_this")
        } else {
            IRNode::this()
        };

        for acc in &self.private_accessors {
            if acc.is_static {
                continue;
            }

            // Emit getter: _ClassName_accessor_get.set(this, function() { ... });
            if let Some(ref get_var) = acc.get_var_name
                && let Some(getter_body) = acc.getter_body
            {
                body.push(IRNode::expr_stmt(IRNode::WeakMapSet {
                    weakmap_name: get_var.clone().into(),
                    key: Box::new(key.clone()),
                    value: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: vec![],
                        body: self.convert_block_body(getter_body),
                        is_expression_body: false,
                        body_source_range: None,
                    }),
                }));
            }

            // Emit setter: _ClassName_accessor_set.set(this, function(param) { ... });
            if let Some(ref set_var) = acc.set_var_name
                && let Some(setter_body) = acc.setter_body
            {
                let param_name = if let Some(param_idx) = acc.setter_param {
                    get_identifier_text(self.arena, param_idx)
                        .unwrap_or_else(|| "value".to_string())
                } else {
                    "value".to_string()
                };

                body.push(IRNode::expr_stmt(IRNode::WeakMapSet {
                    weakmap_name: set_var.clone().into(),
                    key: Box::new(key.clone()),
                    value: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: vec![IRParam::new(param_name)],
                        body: self.convert_block_body(setter_body),
                        is_expression_body: false,
                        body_source_range: None,
                    }),
                }));
            }
        }
    }

    /// Emit auto-accessor field initializations using `WeakMap.set()`
    fn emit_auto_accessor_initializations_ir(&self, body: &mut Vec<IRNode>, use_this: bool) {
        let key = if use_this {
            IRNode::id("_this")
        } else {
            IRNode::this()
        };

        for accessor in &self.auto_accessors {
            if accessor.is_static {
                continue;
            }

            let value = accessor
                .initializer
                .map(|initializer| self.convert_expression(initializer))
                .unwrap_or(IRNode::Undefined);

            // _Class_accessor_storage.set(this, value);
            body.push(IRNode::expr_stmt(IRNode::WeakMapSet {
                weakmap_name: accessor.weakmap_name.clone().into(),
                key: Box::new(key.clone()),
                value: Box::new(value),
            }));
        }
    }

    pub(super) fn find_auto_accessor(
        &self,
        member_idx: NodeIndex,
    ) -> Option<&AutoAccessorFieldInfo> {
        self.auto_accessors
            .iter()
            .find(|acc| acc.member_idx == member_idx)
    }

    pub(super) fn auto_accessor_storage_decls_in_iife(&self) -> bool {
        self.auto_accessors
            .iter()
            .any(|accessor| accessor.is_static)
    }

    pub(super) fn first_computed_instance_auto_accessor(&self) -> Option<&AutoAccessorFieldInfo> {
        self.auto_accessors.iter().find(|accessor| {
            if accessor.is_static {
                return false;
            }
            self.auto_accessor_has_computed_name(accessor.member_idx)
        })
    }
}
