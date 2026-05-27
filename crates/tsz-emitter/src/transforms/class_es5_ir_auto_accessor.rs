//! Auto-accessor, property, and parameter helpers for `ES5ClassTransformer`.
//!
//! Extracted from `class_es5_ir.rs` to keep file sizes manageable.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{IRMethodName, IRNode, IRParam, IRProperty, IRPropertyDescriptor};
use crate::transforms::ir_printer::IRPrinter;
use rustc_hash::FxHashSet;
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::{contains_new_target_reference, contains_this_reference};
use tsz_scanner::SyntaxKind;

use super::{ES5ClassTransformer, PropertyNameIR, get_identifier_text};

impl<'a> ES5ClassTransformer<'a> {
    pub(super) fn auto_accessor_has_computed_name(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let Some(prop) = self.arena.get_property_decl(member_node) else {
            return false;
        };
        self.arena
            .get(prop.name)
            .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
    }

    pub(super) fn auto_accessor_instance_storage_inits_for_computed_key(
        &self,
        member_idx: NodeIndex,
    ) -> Vec<String> {
        if self.first_computed_instance_auto_accessor().is_none()
            || self
                .first_computed_instance_auto_accessor()
                .is_some_and(|accessor| accessor.member_idx != member_idx)
        {
            return Vec::new();
        }

        self.auto_accessors
            .iter()
            .filter(|accessor| !accessor.is_static)
            .map(|accessor| format!("{} = new WeakMap()", accessor.weakmap_name))
            .collect()
    }

    pub(super) fn emit_auto_accessor_storage_decls_and_static_inits(&self, body: &mut Vec<IRNode>) {
        let mut names = Vec::new();
        if let Some(alias) = self.current_static_class_alias.as_ref() {
            names.push(alias.clone());
        }
        names.extend(
            self.auto_accessors
                .iter()
                .map(|accessor| accessor.weakmap_name.clone()),
        );
        if !names.is_empty() {
            body.push(IRNode::VarDeclList(
                names
                    .into_iter()
                    .map(|name| IRNode::VarDecl {
                        name: name.into(),
                        initializer: None,
                    })
                    .collect(),
            ));
        }

        if let Some(alias) = self.current_static_class_alias.as_ref() {
            body.push(IRNode::expr_stmt(IRNode::assign(
                IRNode::id(alias.clone()),
                IRNode::id(self.class_name.clone()),
            )));
        }

        if self.first_computed_instance_auto_accessor().is_none() {
            for accessor in &self.auto_accessors {
                if accessor.is_static {
                    continue;
                }
                body.push(IRNode::expr_stmt(IRNode::assign(
                    IRNode::id(accessor.weakmap_name.clone()),
                    IRNode::NewExpr {
                        callee: Box::new(IRNode::id("WeakMap")),
                        arguments: Vec::new(),
                        explicit_arguments: true,
                    },
                )));
            }
        }

        for accessor in &self.auto_accessors {
            if !accessor.is_static {
                continue;
            }
            let value = accessor
                .initializer
                .map(|initializer| self.convert_expression_static(initializer))
                .unwrap_or(IRNode::Undefined);
            body.push(IRNode::expr_stmt(IRNode::assign(
                IRNode::id(accessor.weakmap_name.clone()),
                IRNode::object(vec![IRProperty::init("value", value)]),
            )));
        }
    }

    pub(super) fn build_auto_accessor_getter_function(&self, weakmap_name: &str) -> IRNode {
        IRNode::FunctionExpr {
            name: None,
            parameters: vec![],
            body: vec![IRNode::ret(Some(IRNode::PrivateFieldGet {
                receiver: Box::new(IRNode::this()),
                weakmap_name: weakmap_name.to_string().into(),
            }))],
            is_expression_body: true,
            body_source_range: None,
        }
    }

    pub(super) fn build_static_auto_accessor_getter_function(&self, weakmap_name: &str) -> IRNode {
        let class_alias = self
            .current_static_class_alias
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.class_name.clone());
        IRNode::FunctionExpr {
            name: None,
            parameters: vec![],
            body: vec![IRNode::ret(Some(IRNode::PrivateStaticFieldGet {
                receiver: Box::new(IRNode::id(class_alias.clone())),
                state: Box::new(IRNode::id(class_alias)),
                storage_name: weakmap_name.to_string().into(),
            }))],
            is_expression_body: true,
            body_source_range: None,
        }
    }

    pub(super) fn build_auto_accessor_setter_function(&self, weakmap_name: &str) -> IRNode {
        IRNode::FunctionExpr {
            name: None,
            parameters: vec![IRParam::new("value")],
            body: vec![IRNode::expr_stmt(IRNode::PrivateFieldSet {
                receiver: Box::new(IRNode::this()),
                weakmap_name: weakmap_name.to_string().into(),
                value: Box::new(IRNode::id("value")),
            })],
            is_expression_body: true,
            body_source_range: None,
        }
    }

    pub(super) fn build_static_auto_accessor_setter_function(&self, weakmap_name: &str) -> IRNode {
        let class_alias = self
            .current_static_class_alias
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.class_name.clone());
        IRNode::FunctionExpr {
            name: None,
            parameters: vec![IRParam::new("value")],
            body: vec![IRNode::expr_stmt(IRNode::PrivateStaticFieldSet {
                receiver: Box::new(IRNode::id(class_alias.clone())),
                state: Box::new(IRNode::id(class_alias)),
                storage_name: weakmap_name.to_string().into(),
                value: Box::new(IRNode::id("value")),
            })],
            is_expression_body: true,
            body_source_range: None,
        }
    }

    pub(super) fn auto_accessor_getter_property_name(
        &self,
        name_idx: NodeIndex,
        storage_inits: &[String],
    ) -> IRMethodName {
        if storage_inits.is_empty() {
            return self.auto_accessor_setter_property_name(name_idx);
        }
        let Some(name_node) = self.arena.get(name_idx) else {
            return self.get_method_name_ir(name_idx);
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return self.get_method_name_ir(name_idx);
        }
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return self.get_method_name_ir(name_idx);
        };

        let expr = self.convert_computed_property_expression(computed.expression, true);
        let expr_text = self.render_ir_expression(&expr);
        let mut parts = storage_inits.to_vec();
        if let Some(temp) = self.computed_prop_temp_map.get(&computed.expression) {
            parts.push(format!("{temp} = {expr_text}"));
        } else {
            parts.push(expr_text);
        }
        IRMethodName::Computed(Box::new(IRNode::Raw(
            format!("({})", parts.join(", ")).into(),
        )))
    }

    pub(super) fn auto_accessor_setter_property_name(&self, name_idx: NodeIndex) -> IRMethodName {
        let Some(name_node) = self.arena.get(name_idx) else {
            return self.get_method_name_ir(name_idx);
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return self.get_method_name_ir(name_idx);
        }
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return self.get_method_name_ir(name_idx);
        };
        if let Some(temp) = self.computed_prop_temp_map.get(&computed.expression) {
            return IRMethodName::Computed(Box::new(IRNode::id(temp.clone())));
        }
        self.get_method_name_ir(name_idx)
    }

    pub(super) fn get_field_define_property_name_ir(&self, name_idx: NodeIndex) -> IRMethodName {
        let Some(name_node) = self.arena.get(name_idx) else {
            return self.get_method_name_ir(name_idx);
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return self.get_method_name_ir(name_idx);
        }
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return self.get_method_name_ir(name_idx);
        };
        if let Some(temp) = self.computed_prop_temp_map.get(&computed.expression) {
            return IRMethodName::Computed(Box::new(IRNode::id(temp.clone())));
        }
        self.get_method_name_ir(name_idx)
    }

    fn render_ir_expression(&self, expr: &IRNode) -> String {
        let mut printer = IRPrinter::with_arena(self.arena);
        printer.set_target_es5(true);
        if let Some(source_text) = self.source_text {
            printer.set_source_text(source_text);
        }
        if let Some(transforms) = self.transforms.as_ref() {
            printer.set_transforms(transforms.clone());
        }
        printer.emit(expr).to_string()
    }

    /// Emit a property initializer as an assignment or defineProperty.
    pub(super) fn emit_property_initializer_ir(
        &self,
        prop_idx: NodeIndex,
        use_this: bool,
    ) -> Option<IRNode> {
        let prop_node = self.arena.get(prop_idx)?;
        let prop_data = self.arena.get_property_decl(prop_node)?;

        let has_initializer_equals = self.property_initializer_has_equals(prop_node, prop_data);
        if !self.use_define_for_class_fields && !has_initializer_equals {
            return None;
        }

        let receiver = if use_this {
            IRNode::id("_this")
        } else {
            IRNode::this()
        };

        let prop_name = self.get_property_name_ir(prop_data.name)?;

        let value = if has_initializer_equals {
            self.convert_async_arrow_property_initializer(prop_data.initializer)
                .unwrap_or_else(|| {
                    if use_this {
                        self.convert_expression_this_captured(prop_data.initializer)
                    } else {
                        self.convert_expression(prop_data.initializer)
                    }
                })
        } else {
            IRNode::void_0()
        };

        if self.use_define_for_class_fields {
            Some(IRNode::DefineProperty {
                target: Box::new(receiver),
                property_name: self.get_field_define_property_name_ir(prop_data.name),
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
            })
        } else {
            Some(IRNode::expr_stmt(IRNode::assign(
                self.build_property_access(receiver, prop_name),
                value,
            )))
        }
    }

    /// Build property access node based on property name type
    fn build_property_access(&self, receiver: IRNode, name: PropertyNameIR) -> IRNode {
        match name {
            PropertyNameIR::Identifier(n) => IRNode::prop(receiver, n),
            PropertyNameIR::StringLiteral(s) => IRNode::elem(receiver, IRNode::string(s)),
            PropertyNameIR::NumericLiteral(n) => IRNode::elem(receiver, IRNode::number(n)),
            PropertyNameIR::Computed(expr_idx) => {
                // If this expression has a hoisted temp variable, use it
                if let Some(temp) = self.computed_prop_temp_map.get(&expr_idx) {
                    IRNode::elem(receiver, IRNode::id(temp.clone()))
                } else {
                    IRNode::elem(
                        receiver,
                        self.convert_computed_property_expression(expr_idx, false),
                    )
                }
            }
        }
    }

    fn convert_async_arrow_property_initializer(&self, initializer: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(initializer)?;
        if node.kind != syntax_kind_ext::ARROW_FUNCTION {
            return None;
        }
        let arrow = self.arena.get_function(node)?;
        if !arrow.is_async {
            return None;
        }

        let mut async_transformer = AsyncES5Transformer::new(self.arena);
        if let Some(source_text) = self.source_text {
            async_transformer.set_source_text(source_text);
        }
        async_transformer.set_module_kind(self.module_kind);
        self.configure_async_disposable_context(&mut async_transformer);
        let has_await = async_transformer.body_contains_await(arrow.body);
        let mut generator_body = async_transformer.transform_generator_body(arrow.body, has_await);
        self.sync_async_disposable_context(&mut async_transformer);
        let hoisted_var_groups =
            AsyncES5Transformer::extract_and_remove_var_decl_groups(&mut generator_body);

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: self.extract_parameters(&arrow.parameters),
            body: vec![IRNode::AwaiterCall {
                this_arg: Box::new(IRNode::id("_this")),
                needs_lexical_this_capture: generator_body.contains_captured_this_reference(),
                generator_body: Box::new(generator_body),
                hoisted_var_groups,
                promise_constructor: self.async_method_promise_constructor(arrow.type_annotation),
                multiline_callback: false,
                directives: Vec::new(),
            }],
            is_expression_body: true,
            body_source_range: None,
        })
    }

    /// Get property name as IR-friendly representation
    pub(super) fn get_property_name_ir(&self, name_idx: NodeIndex) -> Option<PropertyNameIR> {
        let name_node = self.arena.get(name_idx)?;

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                return Some(PropertyNameIR::Computed(computed.expression));
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(PropertyNameIR::Identifier(ident.escaped_text.clone()));
            }
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(PropertyNameIR::StringLiteral(lit.text.clone()));
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            return Some(PropertyNameIR::NumericLiteral(lit.text.clone()));
        }

        None
    }

    /// Extract parameters from a parameter list
    pub(super) fn extract_parameters(&self, params: &NodeList) -> Vec<IRParam> {
        let mut result = Vec::new();
        let mut temp_counter: u8 = b'a';

        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            // Skip `this` parameter — it's TypeScript-only and erased in JS emit.
            // The parser may store it as an Identifier with text "this" or as a ThisKeyword token.
            if let Some(name_node) = self.arena.get(param.name)
                && name_node.kind == SyntaxKind::ThisKeyword as u16
            {
                continue;
            }

            let mut name = get_identifier_text(self.arena, param.name).unwrap_or_default();
            if name == "this" {
                continue;
            }
            // For destructured parameters (binding patterns), generate a temp name
            if name.is_empty() {
                let name_node = self.arena.get(param.name);
                let is_binding_pattern = name_node.is_some_and(|n| {
                    n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                });
                if is_binding_pattern {
                    name = format!("_{}", temp_counter as char);
                    temp_counter = temp_counter.wrapping_add(1);
                } else {
                    continue;
                }
            }

            let is_rest = param.dot_dot_dot_token;
            let mut ir_param = if is_rest {
                IRParam::rest(name)
            } else {
                IRParam::new(name)
            };

            // Convert default value if present
            if param.initializer.is_some() {
                ir_param.default_value = Some(Box::new(self.convert_expression(param.initializer)));
            }
            if let Some(name_node) = self.arena.get(param.name)
                && let Some(comment) = self.extract_leading_comment(name_node)
            {
                ir_param.leading_comment = Some(comment.into());
            }

            result.push(ir_param);
        }

        result
    }

    /// Generate destructuring prologue IR nodes for binding-pattern parameters.
    /// For `({ a, b })` with temp name `_a`, generates: `var a = _a.a, b = _a.b;`
    pub(super) fn generate_destructuring_prologue(
        &self,
        ast_params: &tsz_parser::parser::NodeList,
        ir_params: &[IRParam],
    ) -> Vec<IRNode> {
        let mut prologue = Vec::new();
        let mut ir_idx = 0;
        let mut reserved_temp_names: FxHashSet<String> = ir_params
            .iter()
            .map(|param| param.name.to_string())
            .collect();

        for &param_idx in &ast_params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                ir_idx += 1;
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                ir_idx += 1;
                continue;
            };

            let name_node = self.arena.get(param.name);

            // Skip `this` parameter — it was also skipped in extract_parameters,
            // so don't increment ir_idx.
            let is_this = name_node.is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16)
                || get_identifier_text(self.arena, param.name).as_deref() == Some("this");
            if is_this {
                continue;
            }

            let is_binding_pattern = name_node.is_some_and(|n| {
                n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            });

            if !is_binding_pattern {
                ir_idx += 1;
                continue;
            }

            // Get the temp name from the corresponding IR param
            let temp_name = if ir_idx < ir_params.len() {
                ir_params[ir_idx].name.to_string()
            } else {
                ir_idx += 1;
                continue;
            };

            if let Some(name_n) = name_node
                && name_n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                && let Some(pattern) = self.arena.get_binding_pattern(name_n)
            {
                let mut declarations = Vec::new();
                let mut rest_excluded = Vec::new();
                for &elem_idx in &pattern.elements.nodes {
                    if let Some(elem_node) = self.arena.get(elem_idx)
                        && let Some(elem) = self.arena.get_binding_element(elem_node)
                    {
                        let elem_name =
                            get_identifier_text(self.arena, elem.name).unwrap_or_default();
                        if !elem_name.is_empty() {
                            if elem.dot_dot_dot_token {
                                let excluded =
                                    rest_excluded.iter().cloned().map(IRNode::string).collect();
                                declarations.push(IRNode::var_decl(
                                    elem_name,
                                    Some(IRNode::call(
                                        IRNode::RuntimeHelper("__rest".into()),
                                        vec![
                                            IRNode::id(temp_name.clone()),
                                            IRNode::ArrayLiteral(excluded),
                                        ],
                                    )),
                                ));
                                continue;
                            }

                            let prop_name = if elem.property_name.is_some() {
                                get_identifier_text(self.arena, elem.property_name)
                                    .unwrap_or_else(|| elem_name.clone())
                            } else {
                                elem_name.clone()
                            };
                            rest_excluded.push(prop_name.clone());
                            declarations.push(IRNode::var_decl(
                                elem_name,
                                Some(IRNode::prop(IRNode::id(temp_name.clone()), prop_name)),
                            ));
                        }
                    }
                }
                if !declarations.is_empty() {
                    prologue.push(IRNode::VarDeclList(declarations));
                }
            } else if let Some(name_n) = name_node
                && name_n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                && let Some(pattern) = self.arena.get_binding_pattern(name_n)
            {
                let mut declarations = Vec::new();
                let source_name = if self.downlevel_iteration && !pattern.elements.nodes.is_empty()
                {
                    let read_name = Self::fresh_destructuring_temp(&mut reserved_temp_names);
                    let mut read_args = vec![IRNode::id(temp_name.clone())];
                    if let Some(limit) = self.array_binding_read_limit(pattern) {
                        read_args.push(IRNode::number(limit.to_string()));
                    }
                    declarations.push(IRNode::var_decl(
                        read_name.clone(),
                        Some(IRNode::call(
                            IRNode::RuntimeHelper("__read".into()),
                            read_args,
                        )),
                    ));
                    read_name
                } else {
                    temp_name.clone()
                };

                for (element_index, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(elem) = self.arena.get_binding_element(elem_node) else {
                        continue;
                    };
                    let elem_name = get_identifier_text(self.arena, elem.name).unwrap_or_default();
                    if elem_name.is_empty() {
                        continue;
                    }

                    let initializer = if elem.dot_dot_dot_token {
                        IRNode::call(
                            IRNode::prop(IRNode::id(source_name.clone()), "slice"),
                            vec![IRNode::number(element_index.to_string())],
                        )
                    } else {
                        IRNode::elem(
                            IRNode::id(source_name.clone()),
                            IRNode::number(element_index.to_string()),
                        )
                    };
                    declarations.push(IRNode::var_decl(elem_name, Some(initializer)));
                }

                if !declarations.is_empty() {
                    prologue.push(IRNode::VarDeclList(declarations));
                }
            }
            ir_idx += 1;
        }
        prologue
    }

    fn fresh_destructuring_temp(reserved: &mut FxHashSet<String>) -> String {
        let mut idx = 0usize;
        loop {
            let candidate = if idx < 26 {
                format!("_{}", (b'a' + idx as u8) as char)
            } else {
                format!("_{idx}")
            };
            if reserved.insert(candidate.clone()) {
                return candidate;
            }
            idx += 1;
        }
    }

    fn array_binding_read_limit(
        &self,
        pattern: &tsz_parser::parser::node::BindingPatternData,
    ) -> Option<usize> {
        for &elem_idx in &pattern.elements.nodes {
            if self
                .arena
                .get(elem_idx)
                .and_then(|node| self.arena.get_binding_element(node))
                .is_some_and(|elem| elem.dot_dot_dot_token)
            {
                return None;
            }
        }
        Some(pattern.elements.nodes.len())
    }

    /// Check if any parameters are destructured binding patterns.
    pub(super) fn has_destructured_parameters(
        &self,
        params: &tsz_parser::parser::NodeList,
    ) -> bool {
        params.nodes.iter().any(|&param_idx| {
            self.arena
                .get(param_idx)
                .and_then(|n| self.arena.get_parameter(n))
                .and_then(|p| self.arena.get(p.name))
                .is_some_and(|n| {
                    n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                })
        })
    }

    /// Get the extends clause base class
    pub(super) fn get_extends_class(&self, heritage_clauses: &Option<NodeList>) -> Option<IRNode> {
        let expr_idx = crate::transforms::emit_utils::get_extends_expression_index(
            self.arena,
            heritage_clauses,
        )?;
        Some(self.convert_expression(expr_idx))
    }

    /// Collect all arrow function node indices in a block
    fn collect_arrow_functions_in_block(&self, block_idx: NodeIndex) -> Vec<NodeIndex> {
        let mut arrows = Vec::new();
        if let Some(block_node) = self.arena.get(block_idx)
            && let Some(block) = self.arena.get_block(block_node)
        {
            for &stmt_idx in &block.statements.nodes {
                self.collect_arrow_functions_in_node(stmt_idx, &mut arrows);
            }
        }
        arrows
    }

    /// Check if constructor body needs `var _this = this;` capture
    /// Returns true if the body contains arrow functions that capture `this`
    pub(super) fn constructor_needs_this_capture(&self, body_idx: NodeIndex) -> bool {
        let arrow_indices = self.collect_arrow_functions_in_block(body_idx);

        // Check if any arrow function captures `this`
        for &arrow_idx in &arrow_indices {
            if let Some(ref transforms) = self.transforms {
                if let Some(crate::context::transform::TransformDirective::ES5ArrowFunction {
                    captures_this,
                    ..
                }) = transforms.get(arrow_idx)
                    && *captures_this
                {
                    return true;
                }
            } else {
                // Fallback: directly check if arrow contains `this` reference
                if contains_this_reference(self.arena, arrow_idx) {
                    return true;
                }
            }
        }

        false
    }

    pub(super) fn constructor_body_or_params_contain_new_target(
        &self,
        body_idx: NodeIndex,
        params: &NodeList,
    ) -> bool {
        (body_idx.is_some() && contains_new_target_reference(self.arena, body_idx))
            || params.nodes.iter().any(|&param_idx| {
                self.arena
                    .get(param_idx)
                    .and_then(|param_node| self.arena.get_parameter(param_node))
                    .is_some_and(|param| {
                        param.initializer.is_some()
                            && contains_new_target_reference(self.arena, param.initializer)
                    })
            })
    }

    pub(super) fn class_constructor_new_target_capture_ir() -> IRNode {
        IRNode::var_decl("_newTarget", Some(IRNode::Raw("this.constructor".into())))
    }

    pub(super) fn insert_class_new_target_capture(&self, body: &mut Vec<IRNode>) {
        let capture = Self::class_constructor_new_target_capture_ir();
        if self.has_extends
            && !self.extends_null
            && let Some(super_capture_idx) = body
                .iter()
                .position(|node| self.is_generated_derived_super_capture(node))
        {
            body.insert(super_capture_idx + 1, capture);
            return;
        }

        body.insert(0, capture);
    }

    fn is_generated_derived_super_capture(&self, node: &IRNode) -> bool {
        let IRNode::VarDecl {
            name,
            initializer: Some(initializer),
        } = node
        else {
            return false;
        };
        if name.as_ref() != "_this" {
            return false;
        }

        matches!(
            initializer.as_ref(),
            IRNode::LogicalOr { left, right }
                if matches!(right.as_ref(), IRNode::This { captured: false })
                    && matches!(
                        left.as_ref(),
                        IRNode::CallExpr { callee, arguments }
                            if arguments
                                .first()
                                .is_some_and(|arg| matches!(arg, IRNode::This { captured: false }))
                                && matches!(
                                    callee.as_ref(),
                                    IRNode::PropertyAccess { object, property }
                                        if property.as_ref() == "call"
                                            && matches!(
                                                object.as_ref(),
                                                IRNode::Identifier(super_name)
                                                    if super_name.as_ref() == self.super_name
                                            )
                                )
                    )
        )
    }

    fn instance_props_contain_new_target(&self, instance_props: &[NodeIndex]) -> bool {
        instance_props.iter().any(|&prop_idx| {
            self.arena
                .get(prop_idx)
                .and_then(|prop_node| self.arena.get_property_decl(prop_node))
                .is_some_and(|prop| {
                    prop.initializer.is_some()
                        && contains_new_target_reference(self.arena, prop.initializer)
                })
        })
    }

    pub(super) fn moved_instance_initializers_contain_new_target(
        &self,
        instance_props: &[NodeIndex],
    ) -> bool {
        self.instance_props_contain_new_target(instance_props)
            || self.private_fields.iter().any(|field| {
                !field.is_static
                    && field.has_initializer
                    && field.initializer.is_some()
                    && contains_new_target_reference(self.arena, field.initializer)
            })
            || self.auto_accessors.iter().any(|accessor| {
                !accessor.is_static
                    && accessor.initializer.is_some_and(|initializer| {
                        contains_new_target_reference(self.arena, initializer)
                    })
            })
    }

    /// Check if instance property initializers contain arrow functions that capture `this`.
    /// Property initializers are moved into the constructor body by the ES5 transform.
    pub(super) fn instance_props_need_this_capture(&self, instance_props: &[NodeIndex]) -> bool {
        for &prop_idx in instance_props {
            let Some(prop_node) = self.arena.get(prop_idx) else {
                continue;
            };
            let Some(prop_data) = self.arena.get_property_decl(prop_node) else {
                continue;
            };
            if prop_data.initializer.is_none() {
                continue;
            }
            // Check if the initializer contains arrow functions that capture `this`
            let mut arrows = Vec::new();
            self.collect_arrow_functions_in_node(prop_data.initializer, &mut arrows);
            for &arrow_idx in &arrows {
                if self
                    .arena
                    .get(arrow_idx)
                    .and_then(|arrow_node| self.arena.get_function(arrow_node))
                    .is_some_and(|arrow| arrow.is_async)
                {
                    return true;
                }
                if let Some(ref transforms) = self.transforms {
                    if let Some(crate::context::transform::TransformDirective::ES5ArrowFunction {
                        captures_this,
                        ..
                    }) = transforms.get(arrow_idx)
                        && *captures_this
                    {
                        return true;
                    }
                } else if contains_this_reference(self.arena, arrow_idx) {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn property_initializer_has_equals(
        &self,
        member_node: &Node,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> bool {
        let Some(text) = self.source_text else {
            return prop.initializer.is_some();
        };
        let Some(init_node) = self.arena.get(prop.initializer) else {
            return false;
        };
        if prop.type_annotation.is_none() {
            return true;
        }

        let start = member_node.pos as usize;
        let end = (init_node.pos as usize).min(text.len());
        if start >= end {
            return false;
        }
        let segment = &text.as_bytes()[start..end];
        let search_from = segment
            .iter()
            .rposition(|&byte| byte == b':')
            .map_or(0, |idx| idx + 1);
        segment[search_from..].contains(&b'=')
    }

    /// Recursively collect arrow function indices starting from a node
    fn collect_arrow_functions_in_node(&self, idx: NodeIndex, arrows: &mut Vec<NodeIndex>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::ARROW_FUNCTION {
            arrows.push(idx);
        }

        for child_idx in self.arena.get_children(idx) {
            self.collect_arrow_functions_in_node(child_idx, arrows);
        }
    }
}
