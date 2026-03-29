//! ES5 class member emission — prototype methods, static members, and accessors.
//!
//! Extracted from `class_es5_ir.rs` to keep file sizes manageable.
//! Contains `emit_methods_ir`, `emit_static_members_ir`, and related helpers.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{IRMethodName, IRNode, IRParam, IRPropertyDescriptor};
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::{contains_this_reference, is_private_identifier};
use tsz_scanner::SyntaxKind;

use super::{ES5ClassTransformer, PropertyNameIR, collect_accessor_pairs, get_identifier_text};

impl<'a> ES5ClassTransformer<'a> {
    /// Emit prototype methods as IR (superseded by `emit_all_members_ir`)
    #[allow(dead_code)]
    pub(super) fn emit_methods_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // First pass: collect instance accessors by name to combine getter/setter pairs
        let accessor_map = collect_accessor_pairs(self.arena, &class_data.members, false);

        // Track which accessor names we've emitted
        let mut emitted_accessors: FxHashSet<String> = FxHashSet::default();

        // Second pass: emit methods and accessors in source order
        for (member_i, &member_idx) in class_data.members.nodes.iter().enumerate() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                // Skip static methods
                if self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }

                // Skip if no body
                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name_ir(method_data.name);
                let params = self.extract_parameters(&method_data.parameters);

                // Generate destructuring prologue for binding-pattern parameters
                let destructuring_prologue =
                    self.generate_destructuring_prologue(&method_data.parameters, &params);

                // Check if async method (not generator)
                let is_async = self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword)
                    && !method_data.asterisk_token;

                // Capture body source range for single-line detection
                // If we have destructuring prologue, force multi-line
                let body_source_range = if destructuring_prologue.is_empty() {
                    self.arena
                        .get(method_data.body)
                        .map(|body_node| (body_node.pos, body_node.end))
                } else {
                    None // Force multi-line when destructuring prologue exists
                };

                let method_body = if is_async {
                    // Async method: use async transformer to build proper generator body
                    let mut async_transformer = AsyncES5Transformer::new(self.arena);
                    let has_await = async_transformer.body_contains_await(method_data.body);
                    let generator_body =
                        async_transformer.transform_generator_body(method_data.body, has_await);
                    vec![IRNode::AwaiterCall {
                        this_arg: Box::new(IRNode::this()),
                        generator_body: Box::new(generator_body),
                        hoisted_vars: Vec::new(),
                        promise_constructor: None,
                    }]
                } else {
                    let mut method_body = self.convert_block_body(method_data.body);
                    // Prepend destructuring prologue
                    if !destructuring_prologue.is_empty() {
                        let mut full_body = destructuring_prologue;
                        full_body.append(&mut method_body);
                        method_body = full_body;
                    }

                    // Check if method needs `var _this = this;` capture
                    let needs_this_capture = self.constructor_needs_this_capture(method_data.body);
                    if needs_this_capture {
                        // Insert `var _this = this;` at the start of method body
                        method_body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
                    }

                    method_body
                };

                // Extract leading JSDoc comment
                let leading_comment = self.extract_leading_comment(member_node);
                let trailing_comment = self.extract_trailing_comment_for_method(method_data.body);

                let function = IRNode::FunctionExpr {
                    name: None,
                    parameters: params,
                    body: method_body,
                    is_expression_body: false,
                    body_source_range,
                };

                if self.use_define_for_class_fields {
                    body.push(IRNode::DefineProperty {
                        target: Box::new(IRNode::prop(
                            IRNode::id(self.class_name.clone()),
                            "prototype",
                        )),
                        property_name: method_name,
                        descriptor: IRPropertyDescriptor {
                            get: None,
                            set: None,
                            value: Some(Box::new(function)),
                            enumerable: false,
                            configurable: true,
                            writable: true,
                            trailing_comment,
                        },
                        leading_comment,
                    });
                } else {
                    body.push(IRNode::PrototypeMethod {
                        class_name: self.class_name.clone().into(),
                        method_name,
                        function: Box::new(function),
                        leading_comment,
                        trailing_comment,
                    });
                }
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                // Handle accessor (getter/setter) - combine pairs
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    // Skip static/abstract/private (already filtered in first pass)
                    if self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::StaticKeyword)
                        || self
                            .arena
                            .has_modifier(&accessor_data.modifiers, SyntaxKind::AbstractKeyword)
                        || is_private_identifier(self.arena, accessor_data.name)
                    {
                        continue;
                    }

                    let accessor_name = match get_identifier_text(self.arena, accessor_data.name) {
                        Some(name) => name,
                        None => format!("__computed_{}", member_idx.0),
                    };

                    // Skip if already emitted
                    if emitted_accessors.contains(&accessor_name) {
                        continue;
                    }

                    // Emit combined getter/setter
                    if let Some(&(getter_idx, setter_idx)) = accessor_map.get(&accessor_name) {
                        let get_fn = if let Some(getter_idx) = getter_idx {
                            self.build_getter_function_ir(getter_idx)
                        } else {
                            None
                        };

                        let set_fn = if let Some(setter_idx) = setter_idx {
                            self.build_setter_function_ir(setter_idx)
                        } else {
                            None
                        };

                        // Extract leading comment from the first accessor (getter or setter)
                        let leading_comment = self.extract_leading_comment(member_node);

                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::prop(
                                IRNode::id(self.class_name.clone()),
                                "prototype",
                            )),
                            property_name: self.get_method_name_ir(accessor_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: get_fn.map(Box::new),
                                set: set_fn.map(Box::new),
                                value: None,
                                enumerable: false,
                                configurable: true,
                                writable: false,
                                trailing_comment: None,
                            },
                            leading_comment,
                        });

                        let has_explicit_semicolon_member = class_data
                            .members
                            .nodes
                            .get(member_i + 1)
                            .and_then(|&idx| self.arena.get(idx))
                            .is_some_and(|n| n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);
                        if !has_explicit_semicolon_member {
                            let accessor_end = [getter_idx, setter_idx]
                                .into_iter()
                                .flatten()
                                .filter_map(|idx| self.arena.get(idx))
                                .map(|n| n.end)
                                .max()
                                .unwrap_or(member_node.end);
                            let next_pos = class_data
                                .members
                                .nodes
                                .get(member_i + 1)
                                .and_then(|&idx| self.arena.get(idx))
                                .map_or(member_node.end, |n| n.pos);
                            if self.source_has_semicolon_between(accessor_end, next_pos) {
                                body.push(IRNode::EmptyStatement);
                            }
                        }
                        if self.source_text.is_some_and(|text| {
                            let start = std::cmp::min(member_node.pos as usize, text.len());
                            let end = std::cmp::min(member_node.end as usize, text.len());
                            start < end && text[start..end].trim_end().ends_with(';')
                        }) {
                            body.push(IRNode::EmptyStatement);
                        }

                        emitted_accessors.insert(accessor_name);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(accessor) = self.find_auto_accessor(member_idx) else {
                    continue;
                };
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                    || self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                    || is_private_identifier(self.arena, prop_data.name)
                {
                    continue;
                }

                let property_name = self.get_method_name_ir(prop_data.name);
                let leading_comment = self.extract_leading_comment(member_node);
                body.push(IRNode::DefineProperty {
                    target: Box::new(IRNode::prop(
                        IRNode::id(self.class_name.clone()),
                        "prototype",
                    )),
                    property_name,
                    descriptor: IRPropertyDescriptor {
                        get: Some(Box::new(
                            self.build_auto_accessor_getter_function(&accessor.weakmap_name),
                        )),
                        set: Some(Box::new(
                            self.build_auto_accessor_setter_function(&accessor.weakmap_name),
                        )),
                        value: None,
                        enumerable: false,
                        configurable: true,
                        writable: false,
                        trailing_comment: self.extract_trailing_comment_for_node(member_node),
                    },
                    leading_comment,
                });
            } else if member_node.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT {
                body.push(IRNode::EmptyStatement);
            }
        }
    }

    /// Build a getter function IR from an accessor node
    fn build_getter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        self.build_getter_function_ir_impl(accessor_idx, false)
    }

    fn build_getter_function_ir_static(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        self.build_getter_function_ir_impl(accessor_idx, true)
    }

    fn build_getter_function_ir_impl(
        &self,
        accessor_idx: NodeIndex,
        is_static: bool,
    ) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let params = self.extract_parameters(&accessor_data.parameters);

        let body_source_range = self.arena.get(accessor_data.body).map(|n| (n.pos, n.end));

        let body = if accessor_data.body.is_none() {
            vec![]
        } else {
            let mut body = if is_static {
                self.convert_block_body_static(accessor_data.body)
            } else {
                self.convert_block_body(accessor_data.body)
            };

            // Check if getter needs `var _this = this;` capture
            let needs_this_capture = self.constructor_needs_this_capture(accessor_data.body);
            if needs_this_capture {
                // Insert `var _this = this;` at the start of getter body
                body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
            }

            body
        };

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range,
        })
    }

    /// Build a setter function IR from an accessor node
    fn build_setter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        self.build_setter_function_ir_impl(accessor_idx, false)
    }

    fn build_setter_function_ir_static(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        self.build_setter_function_ir_impl(accessor_idx, true)
    }

    fn build_setter_function_ir_impl(
        &self,
        accessor_idx: NodeIndex,
        is_static: bool,
    ) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let mut params = self.extract_parameters(&accessor_data.parameters);

        // Generate destructuring prologue for binding-pattern parameters
        let accessor_destructuring =
            self.generate_destructuring_prologue(&accessor_data.parameters, &params);

        let body_source_range = if accessor_destructuring.is_empty() {
            self.arena.get(accessor_data.body).map(|n| (n.pos, n.end))
        } else {
            None // Force multi-line when destructuring prologue exists
        };

        let mut body = if accessor_data.body.is_none() {
            vec![]
        } else {
            let mut body = if is_static {
                self.convert_block_body_static(accessor_data.body)
            } else {
                self.convert_block_body(accessor_data.body)
            };

            // Check if setter needs `var _this = this;` capture
            let needs_this_capture = self.constructor_needs_this_capture(accessor_data.body);
            if needs_this_capture {
                body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
            }

            // Prepend destructuring prologue
            if !accessor_destructuring.is_empty() {
                let mut full = accessor_destructuring;
                full.append(&mut body);
                body = full;
            }

            body
        };

        self.lower_rest_parameter_for_es5(&mut params, &mut body);

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range,
        })
    }

    /// Lower a rest parameter into ES5 `arguments` collection statements.
    /// Example: `(...v)` -> `() { var v = []; for (var _i = 0; _i < arguments.length; _i++) { ... } }`
    fn lower_rest_parameter_for_es5(&self, params: &mut Vec<IRParam>, body: &mut Vec<IRNode>) {
        let Some(rest_index) = params.iter().position(|param| param.rest) else {
            return;
        };

        let rest_name = params[rest_index].name.clone();
        params.truncate(rest_index);

        let loop_var = "_i";
        let start_index = rest_index.to_string();

        let target_index = if rest_index == 0 {
            IRNode::id(loop_var)
        } else {
            IRNode::binary(
                IRNode::id(loop_var),
                "-",
                IRNode::number(start_index.clone()),
            )
        };

        let assignment = IRNode::expr_stmt(IRNode::assign(
            IRNode::elem(IRNode::id(rest_name.clone()), target_index),
            IRNode::elem(IRNode::id("arguments"), IRNode::id(loop_var)),
        ));

        let collect_rest = IRNode::ForStatement {
            initializer: Some(Box::new(IRNode::Raw(
                format!("var {loop_var} = {start_index}").into(),
            ))),
            condition: Some(Box::new(IRNode::binary(
                IRNode::id(loop_var),
                "<",
                IRNode::prop(IRNode::id("arguments"), "length"),
            ))),
            incrementor: Some(Box::new(IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(loop_var)),
                operator: "++".to_string().into(),
            })),
            body: Box::new(IRNode::block(vec![assignment])),
        };

        body.insert(0, collect_rest);
        body.insert(0, IRNode::var_decl(rest_name, Some(IRNode::empty_array())));
    }

    /// Emit static members as IR (superseded by `emit_all_members_ir`).
    /// Returns deferred static block IIFEs (for classes with no non-block static members).
    #[allow(dead_code)]
    pub(super) fn emit_static_members_ir(
        &self,
        body: &mut Vec<IRNode>,
        class_idx: NodeIndex,
    ) -> Vec<IRNode> {
        let Some(class_node) = self.arena.get(class_idx) else {
            return Vec::new();
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return Vec::new();
        };

        // Check if class has non-block static members (properties, accessors, methods with bodies)
        // This determines whether static blocks go inline or deferred
        let has_static_props = class_data.members.nodes.iter().any(|&m_idx| {
            let Some(m_node) = self.arena.get(m_idx) else {
                return false;
            };
            if m_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                if let Some(prop_data) = self.arena.get_property_decl(m_node) {
                    return self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                        && !is_private_identifier(self.arena, prop_data.name)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                        && prop_data.initializer.is_some();
                }
            } else if (m_node.kind == syntax_kind_ext::GET_ACCESSOR
                || m_node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(acc_data) = self.arena.get_accessor(m_node)
            {
                return self
                    .arena
                    .has_modifier(&acc_data.modifiers, SyntaxKind::StaticKeyword)
                    && !self
                        .arena
                        .has_modifier(&acc_data.modifiers, SyntaxKind::AbstractKeyword)
                    && !is_private_identifier(self.arena, acc_data.name);
            }
            false
        });

        // Check if any static property initializer or static block uses `this`.
        // If so, we need to emit `var _a; _a = ClassName;` and replace `this` with `_a`.
        // Note: `this` in static methods/getters/setters stays as `this` (they have their own
        // `this` binding at call time). Only property initializers and static blocks need aliasing.
        let class_alias = self.current_static_class_alias.clone();

        // Emit `var _a; _a = ClassName;` if needed
        if let Some(ref alias) = class_alias {
            body.push(IRNode::VarDecl {
                name: alias.clone().into(),
                initializer: None,
            });
            body.push(IRNode::expr_stmt(IRNode::assign(
                IRNode::id(alias.clone()),
                IRNode::id(self.class_name.clone()),
            )));
        }

        let mut deferred_static_blocks = Vec::new();

        // First pass: collect static accessors by name to combine getter/setter pairs
        let static_accessor_map = collect_accessor_pairs(self.arena, &class_data.members, true);

        // Track which static accessor names we've emitted
        let mut emitted_static_accessors: FxHashSet<String> = FxHashSet::default();

        // Two-pass emission: methods/accessors first, then properties/static blocks.
        // TSC emits static methods and accessors before property initializers, so that
        // `ClassName.method = function() {}` precedes `ClassName.prop = value;`.
        // Static blocks are interleaved with properties in source order (second pass).
        // We collect property/static-block nodes into a deferred list, then append after
        // methods and accessors are emitted.
        let mut deferred_props_and_blocks: Vec<IRNode> = Vec::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                // Only static methods
                if !self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }

                // Skip if no body
                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name_ir(method_data.name);
                let params = self.extract_parameters(&method_data.parameters);

                // Check if async method (not generator)
                let is_async = self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword)
                    && !method_data.asterisk_token;

                // Generate destructuring prologue for binding-pattern parameters
                let static_destructuring =
                    self.generate_destructuring_prologue(&method_data.parameters, &params);

                let method_body = if is_async {
                    let mut async_transformer = AsyncES5Transformer::new(self.arena);
                    let has_await = async_transformer.body_contains_await(method_data.body);
                    let generator_body =
                        async_transformer.transform_generator_body(method_data.body, has_await);
                    vec![IRNode::AwaiterCall {
                        this_arg: Box::new(IRNode::this()),
                        generator_body: Box::new(generator_body),
                        hoisted_vars: Vec::new(),
                        promise_constructor: None,
                    }]
                } else {
                    let class_alias = self.get_class_alias_for_static_method(method_data.body);
                    let mut mbody =
                        self.convert_block_body_with_alias_static(method_data.body, class_alias);
                    if !static_destructuring.is_empty() {
                        let mut full = static_destructuring;
                        full.append(&mut mbody);
                        mbody = full;
                    }
                    mbody
                };

                // Force multi-line when destructuring prologue exists
                let body_source_range = if self.has_destructured_parameters(&method_data.parameters)
                {
                    None
                } else {
                    self.arena
                        .get(method_data.body)
                        .map(|body_node| (body_node.pos, body_node.end))
                };

                // Extract leading JSDoc comment
                let leading_comment = self.extract_leading_comment(member_node);
                let trailing_comment = self.extract_trailing_comment_for_method(method_data.body);

                // ClassName.methodName = function () { body };
                body.push(IRNode::StaticMethod {
                    class_name: self.class_name.clone().into(),
                    method_name,
                    function: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: params,
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    }),
                    leading_comment,
                    trailing_comment,
                });
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };

                // Only static properties
                if !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }

                // Skip abstract properties (they don't exist at runtime)
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                {
                    continue;
                }

                // Skip `declare` properties — ambient/type-only declarations have no runtime representation
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                {
                    continue;
                }

                // Skip private
                if is_private_identifier(self.arena, prop_data.name) {
                    continue;
                }
                // Skip accessor fields - currently emitted via auto-accessor lowering
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                {
                    continue;
                }

                // Skip if no initializer
                if prop_data.initializer.is_none() {
                    continue;
                }

                if let Some(prop_name) = self.get_property_name_ir(prop_data.name) {
                    let target = match &prop_name {
                        PropertyNameIR::Identifier(n) => {
                            IRNode::prop(IRNode::id(self.class_name.clone()), n.clone())
                        }
                        PropertyNameIR::StringLiteral(s) => IRNode::elem(
                            IRNode::id(self.class_name.clone()),
                            IRNode::string(s.clone()),
                        ),
                        PropertyNameIR::NumericLiteral(n) => IRNode::elem(
                            IRNode::id(self.class_name.clone()),
                            IRNode::number(n.clone()),
                        ),
                        PropertyNameIR::Computed(expr_idx) => {
                            // Use hoisted temp if available
                            if let Some(temp) = self.computed_prop_temp_map.get(expr_idx) {
                                IRNode::elem(
                                    IRNode::id(self.class_name.clone()),
                                    IRNode::id(temp.clone()),
                                )
                            } else {
                                IRNode::elem(
                                    IRNode::id(self.class_name.clone()),
                                    self.convert_expression_static(*expr_idx),
                                )
                            }
                        }
                    };

                    // Use class alias for the initializer value if needed
                    let value = if !self.class_decorators.is_empty() {
                        self.convert_expression_static_with_raw_this_substitution(
                            prop_data.initializer,
                            "(void 0)",
                        )
                    } else if let Some(ref alias) = class_alias {
                        self.convert_expression_static_with_class_alias(
                            prop_data.initializer,
                            alias,
                        )
                    } else {
                        self.convert_expression_static(prop_data.initializer)
                    };

                    // ClassName.prop = value;
                    // Deferred to after methods/accessors (TSC ordering)
                    deferred_props_and_blocks
                        .push(IRNode::expr_stmt(IRNode::assign(target, value)));
                }
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // Static block: wrap in IIFE to preserve block scoping
                if let Some(block_data) = self.arena.get_block(member_node) {
                    let statements: Vec<IRNode> = block_data
                        .statements
                        .nodes
                        .iter()
                        .map(|&stmt_idx| {
                            if let Some(ref alias) = class_alias {
                                self.convert_statement_static_with_class_alias(stmt_idx, alias)
                            } else {
                                self.convert_statement_static(stmt_idx)
                            }
                        })
                        .collect();

                    let iife = IRNode::StaticBlockIIFE { statements };
                    if has_static_props {
                        // Inline: maintain initialization order with other static members
                        // Deferred to after methods/accessors (TSC ordering)
                        deferred_props_and_blocks.push(iife);
                    } else {
                        // Deferred: emit after the class IIFE
                        deferred_static_blocks.push(iife);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                // Handle static accessor - combine pairs
                if let Some(accessor_data) = self.arena.get_accessor(member_node)
                    && self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    // Skip abstract/private
                    if self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::AbstractKeyword)
                        || is_private_identifier(self.arena, accessor_data.name)
                    {
                        continue;
                    }

                    let accessor_name = match get_identifier_text(self.arena, accessor_data.name) {
                        Some(name) => name,
                        None => format!("__computed_{}", member_idx.0),
                    };

                    // Skip if already emitted
                    if emitted_static_accessors.contains(&accessor_name) {
                        continue;
                    }

                    // Emit combined getter/setter
                    if let Some(&(getter_idx, setter_idx)) = static_accessor_map.get(&accessor_name)
                    {
                        let get_fn = if let Some(getter_idx) = getter_idx {
                            self.build_getter_function_ir_static(getter_idx)
                        } else {
                            None
                        };

                        let set_fn = if let Some(setter_idx) = setter_idx {
                            self.build_setter_function_ir_static(setter_idx)
                        } else {
                            None
                        };

                        let leading_comment = self.extract_leading_comment(member_node);

                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::id(self.class_name.clone())),
                            property_name: self.get_method_name_ir(accessor_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: get_fn.map(Box::new),
                                set: set_fn.map(Box::new),
                                value: None,
                                enumerable: false,
                                configurable: true,
                                writable: false,
                                trailing_comment: None,
                            },
                            leading_comment,
                        });

                        emitted_static_accessors.insert(accessor_name);
                    }
                }
            }
        }

        // Append deferred properties and static blocks after methods/accessors
        body.append(&mut deferred_props_and_blocks);

        deferred_static_blocks
    }

    /// Get method name as IR representation.
    /// Computed property names use static-like super access (`_super.X` not `_super.prototype.X`)
    /// because they are evaluated at class definition time in the IIFE body, not inside methods.
    pub(super) fn get_method_name_ir(&self, name_idx: NodeIndex) -> IRMethodName {
        let Some(name_node) = self.arena.get(name_idx) else {
            return IRMethodName::Identifier(String::new().into());
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                return IRMethodName::Computed(Box::new(
                    self.convert_expression_static(computed.expression),
                ));
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return IRMethodName::Identifier(ident.escaped_text.clone().into());
            }
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return IRMethodName::StringLiteral(lit.text.clone().into());
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            return IRMethodName::NumericLiteral(lit.text.clone().into());
        }

        IRMethodName::Identifier(String::new().into())
    }

    /// Emit all class members (prototype and static) in source order.
    /// This matches tsc's behavior of interleaving prototype and static members
    /// based on their order in the source code.
    /// Returns deferred static block IIFEs (for classes with no non-block static members).
    pub(super) fn emit_all_members_ir(
        &self,
        body: &mut Vec<IRNode>,
        class_idx: NodeIndex,
    ) -> Vec<IRNode> {
        let Some(class_node) = self.arena.get(class_idx) else {
            return Vec::new();
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return Vec::new();
        };

        // --- Static member preamble (from emit_static_members_ir) ---

        // Check if class has non-block static members (properties, accessors, methods with bodies)
        // This determines whether static blocks go inline or deferred
        let has_static_props = class_data.members.nodes.iter().any(|&m_idx| {
            let Some(m_node) = self.arena.get(m_idx) else {
                return false;
            };
            if m_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                if let Some(prop_data) = self.arena.get_property_decl(m_node) {
                    return self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                        && !is_private_identifier(self.arena, prop_data.name)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                        && prop_data.initializer.is_some();
                }
            } else if (m_node.kind == syntax_kind_ext::GET_ACCESSOR
                || m_node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(acc_data) = self.arena.get_accessor(m_node)
            {
                return self
                    .arena
                    .has_modifier(&acc_data.modifiers, SyntaxKind::StaticKeyword)
                    && !self
                        .arena
                        .has_modifier(&acc_data.modifiers, SyntaxKind::AbstractKeyword)
                    && !is_private_identifier(self.arena, acc_data.name);
            }
            false
        });

        let class_alias = self.current_static_class_alias.clone();

        let mut deferred_static_blocks = Vec::new();

        // Collect accessor pairs for both instance and static
        let instance_accessor_map = collect_accessor_pairs(self.arena, &class_data.members, false);
        let static_accessor_map = collect_accessor_pairs(self.arena, &class_data.members, true);

        let mut emitted_instance_accessors: FxHashSet<String> = FxHashSet::default();
        let mut emitted_static_accessors: FxHashSet<String> = FxHashSet::default();

        // Collect deferred static property initializers.
        // tsc emits methods/accessors (both instance and static) in source order,
        // but defers static property initializer assignments to after all methods/accessors.
        let mut deferred_static_prop_inits: Vec<IRNode> = Vec::new();

        // Single pass: emit all members in source order
        for (member_i, &member_idx) in class_data.members.nodes.iter().enumerate() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                let is_static = self
                    .arena
                    .has_modifier(&method_data.modifiers, SyntaxKind::StaticKeyword);

                // Skip if no body
                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name_ir(method_data.name);
                let params = self.extract_parameters(&method_data.parameters);

                if is_static {
                    // --- Static method (from emit_static_members_ir) ---
                    let is_async = self
                        .arena
                        .has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword)
                        && !method_data.asterisk_token;

                    let static_destructuring =
                        self.generate_destructuring_prologue(&method_data.parameters, &params);

                    let method_body = if is_async {
                        let mut async_transformer = AsyncES5Transformer::new(self.arena);
                        let has_await = async_transformer.body_contains_await(method_data.body);
                        let generator_body =
                            async_transformer.transform_generator_body(method_data.body, has_await);
                        vec![IRNode::AwaiterCall {
                            this_arg: Box::new(IRNode::this()),
                            generator_body: Box::new(generator_body),
                            hoisted_vars: Vec::new(),
                            promise_constructor: None,
                        }]
                    } else {
                        let local_class_alias =
                            self.get_class_alias_for_static_method(method_data.body);
                        let mut mbody = self.convert_block_body_with_alias_static(
                            method_data.body,
                            local_class_alias,
                        );
                        if !static_destructuring.is_empty() {
                            let mut full = static_destructuring;
                            full.append(&mut mbody);
                            mbody = full;
                        }
                        mbody
                    };

                    let body_source_range =
                        if self.has_destructured_parameters(&method_data.parameters) {
                            None
                        } else {
                            self.arena
                                .get(method_data.body)
                                .map(|body_node| (body_node.pos, body_node.end))
                        };

                    let leading_comment = self.extract_leading_comment(member_node);
                    let trailing_comment =
                        self.extract_trailing_comment_for_method(method_data.body);

                    let function = IRNode::FunctionExpr {
                        name: None,
                        parameters: params,
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    };

                    if self.use_define_for_class_fields {
                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::id(self.class_name.clone())),
                            property_name: method_name,
                            descriptor: IRPropertyDescriptor {
                                get: None,
                                set: None,
                                value: Some(Box::new(function)),
                                enumerable: false,
                                configurable: true,
                                writable: true,
                                trailing_comment,
                            },
                            leading_comment,
                        });
                    } else {
                        body.push(IRNode::StaticMethod {
                            class_name: self.class_name.clone().into(),
                            method_name,
                            function: Box::new(function),
                            leading_comment,
                            trailing_comment,
                        });
                    }
                } else {
                    // --- Instance method (from emit_methods_ir) ---
                    let destructuring_prologue =
                        self.generate_destructuring_prologue(&method_data.parameters, &params);

                    let is_async = self
                        .arena
                        .has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword)
                        && !method_data.asterisk_token;

                    let body_source_range = if destructuring_prologue.is_empty() {
                        self.arena
                            .get(method_data.body)
                            .map(|body_node| (body_node.pos, body_node.end))
                    } else {
                        None
                    };

                    let method_body = if is_async {
                        let mut async_transformer = AsyncES5Transformer::new(self.arena);
                        let has_await = async_transformer.body_contains_await(method_data.body);
                        let generator_body =
                            async_transformer.transform_generator_body(method_data.body, has_await);
                        vec![IRNode::AwaiterCall {
                            this_arg: Box::new(IRNode::this()),
                            generator_body: Box::new(generator_body),
                            hoisted_vars: Vec::new(),
                            promise_constructor: None,
                        }]
                    } else {
                        let mut method_body = self.convert_block_body(method_data.body);
                        if !destructuring_prologue.is_empty() {
                            let mut full_body = destructuring_prologue;
                            full_body.append(&mut method_body);
                            method_body = full_body;
                        }
                        let needs_this_capture =
                            self.constructor_needs_this_capture(method_data.body);
                        if needs_this_capture {
                            method_body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
                        }
                        method_body
                    };

                    let leading_comment = self.extract_leading_comment(member_node);
                    let trailing_comment =
                        self.extract_trailing_comment_for_method(method_data.body);

                    let function = IRNode::FunctionExpr {
                        name: None,
                        parameters: params,
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    };

                    if self.use_define_for_class_fields {
                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::prop(
                                IRNode::id(self.class_name.clone()),
                                "prototype",
                            )),
                            property_name: method_name,
                            descriptor: IRPropertyDescriptor {
                                get: None,
                                set: None,
                                value: Some(Box::new(function)),
                                enumerable: false,
                                configurable: true,
                                writable: true,
                                trailing_comment,
                            },
                            leading_comment,
                        });
                    } else {
                        body.push(IRNode::PrototypeMethod {
                            class_name: self.class_name.clone().into(),
                            method_name,
                            function: Box::new(function),
                            leading_comment,
                            trailing_comment,
                        });
                    }
                }
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    let is_static = self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::StaticKeyword);
                    let is_abstract = self
                        .arena
                        .has_modifier(&accessor_data.modifiers, SyntaxKind::AbstractKeyword);
                    let is_private = is_private_identifier(self.arena, accessor_data.name);

                    if is_abstract || is_private {
                        continue;
                    }

                    let accessor_name = match get_identifier_text(self.arena, accessor_data.name) {
                        Some(name) => name,
                        None => format!("__computed_{}", member_idx.0),
                    };

                    if is_static {
                        // --- Static accessor ---
                        if emitted_static_accessors.contains(&accessor_name) {
                            continue;
                        }

                        if let Some(&(getter_idx, setter_idx)) =
                            static_accessor_map.get(&accessor_name)
                        {
                            let get_fn = if let Some(getter_idx) = getter_idx {
                                self.build_getter_function_ir_static(getter_idx)
                            } else {
                                None
                            };
                            let set_fn = if let Some(setter_idx) = setter_idx {
                                self.build_setter_function_ir_static(setter_idx)
                            } else {
                                None
                            };
                            let leading_comment = self.extract_leading_comment(member_node);
                            body.push(IRNode::DefineProperty {
                                target: Box::new(IRNode::id(self.class_name.clone())),
                                property_name: self.get_method_name_ir(accessor_data.name),
                                descriptor: IRPropertyDescriptor {
                                    get: get_fn.map(Box::new),
                                    set: set_fn.map(Box::new),
                                    value: None,
                                    enumerable: false,
                                    configurable: true,
                                    writable: false,
                                    trailing_comment: None,
                                },
                                leading_comment,
                            });
                            emitted_static_accessors.insert(accessor_name);
                        }
                    } else {
                        // --- Instance accessor ---
                        if emitted_instance_accessors.contains(&accessor_name) {
                            continue;
                        }

                        if let Some(&(getter_idx, setter_idx)) =
                            instance_accessor_map.get(&accessor_name)
                        {
                            let get_fn = if let Some(getter_idx) = getter_idx {
                                self.build_getter_function_ir(getter_idx)
                            } else {
                                None
                            };
                            let set_fn = if let Some(setter_idx) = setter_idx {
                                self.build_setter_function_ir(setter_idx)
                            } else {
                                None
                            };
                            let leading_comment = self.extract_leading_comment(member_node);
                            body.push(IRNode::DefineProperty {
                                target: Box::new(IRNode::prop(
                                    IRNode::id(self.class_name.clone()),
                                    "prototype",
                                )),
                                property_name: self.get_method_name_ir(accessor_data.name),
                                descriptor: IRPropertyDescriptor {
                                    get: get_fn.map(Box::new),
                                    set: set_fn.map(Box::new),
                                    value: None,
                                    enumerable: false,
                                    configurable: true,
                                    writable: false,
                                    trailing_comment: None,
                                },
                                leading_comment,
                            });

                            let has_explicit_semicolon_member = class_data
                                .members
                                .nodes
                                .get(member_i + 1)
                                .and_then(|&idx| self.arena.get(idx))
                                .is_some_and(|n| {
                                    n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT
                                });
                            if !has_explicit_semicolon_member {
                                let accessor_end = [getter_idx, setter_idx]
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|idx| self.arena.get(idx))
                                    .map(|n| n.end)
                                    .max()
                                    .unwrap_or(member_node.end);
                                let next_pos = class_data
                                    .members
                                    .nodes
                                    .get(member_i + 1)
                                    .and_then(|&idx| self.arena.get(idx))
                                    .map_or(member_node.end, |n| n.pos);
                                if self.source_has_semicolon_between(accessor_end, next_pos) {
                                    body.push(IRNode::EmptyStatement);
                                }
                            }
                            if self.source_text.is_some_and(|text| {
                                let start = std::cmp::min(member_node.pos as usize, text.len());
                                let end = std::cmp::min(member_node.end as usize, text.len());
                                start < end && text[start..end].trim_end().ends_with(';')
                            }) {
                                body.push(IRNode::EmptyStatement);
                            }

                            emitted_instance_accessors.insert(accessor_name);
                        }
                    }
                }
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };

                let is_static = self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword);
                let is_abstract = self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword);
                let is_declare = self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword);
                let is_private_field = is_private_identifier(self.arena, prop_data.name);
                let is_accessor_keyword = self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword);

                if is_static {
                    // --- Static property ---
                    if is_abstract || is_declare || is_private_field || is_accessor_keyword {
                        continue;
                    }
                    if prop_data.initializer.is_none() {
                        continue;
                    }
                    // Defer static property initializers to after all methods/accessors.
                    // tsc emits methods/accessors in source order first, then static
                    // property initializer assignments.

                    if let Some(prop_name) = self.get_property_name_ir(prop_data.name) {
                        let target = match &prop_name {
                            PropertyNameIR::Identifier(n) => {
                                IRNode::prop(IRNode::id(self.class_name.clone()), n.clone())
                            }
                            PropertyNameIR::StringLiteral(s) => IRNode::elem(
                                IRNode::id(self.class_name.clone()),
                                IRNode::string(s.clone()),
                            ),
                            PropertyNameIR::NumericLiteral(n) => IRNode::elem(
                                IRNode::id(self.class_name.clone()),
                                IRNode::number(n.clone()),
                            ),
                            PropertyNameIR::Computed(expr_idx) => {
                                // Use hoisted temp if available
                                if let Some(temp) = self.computed_prop_temp_map.get(expr_idx) {
                                    IRNode::elem(
                                        IRNode::id(self.class_name.clone()),
                                        IRNode::id(temp.clone()),
                                    )
                                } else {
                                    IRNode::elem(
                                        IRNode::id(self.class_name.clone()),
                                        self.convert_expression_static(*expr_idx),
                                    )
                                }
                            }
                        };
                        let value = if !self.class_decorators.is_empty() {
                            self.convert_expression_static_with_raw_this_substitution(
                                prop_data.initializer,
                                "(void 0)",
                            )
                        } else if let Some(ref alias) = class_alias {
                            self.convert_expression_static_with_class_alias(
                                prop_data.initializer,
                                alias,
                            )
                        } else {
                            self.convert_expression_static(prop_data.initializer)
                        };
                        if self.use_define_for_class_fields {
                            deferred_static_prop_inits.push(IRNode::DefineProperty {
                                target: Box::new(IRNode::id(self.class_name.clone())),
                                property_name: self.get_method_name_ir(prop_data.name),
                                descriptor: IRPropertyDescriptor {
                                    get: None,
                                    set: None,
                                    value: Some(Box::new(value)),
                                    enumerable: true,
                                    configurable: true,
                                    writable: true,
                                    trailing_comment: None,
                                },
                                leading_comment: self.extract_leading_comment(member_node),
                            });
                        } else {
                            deferred_static_prop_inits
                                .push(IRNode::expr_stmt(IRNode::assign(target, value)));
                        }
                    }
                } else {
                    // --- Instance auto-accessor property ---
                    let Some(accessor) = self.find_auto_accessor(member_idx) else {
                        continue;
                    };
                    if is_abstract || is_private_field {
                        continue;
                    }
                    let property_name = self.get_method_name_ir(prop_data.name);
                    let leading_comment = self.extract_leading_comment(member_node);
                    body.push(IRNode::DefineProperty {
                        target: Box::new(IRNode::prop(
                            IRNode::id(self.class_name.clone()),
                            "prototype",
                        )),
                        property_name,
                        descriptor: IRPropertyDescriptor {
                            get: Some(Box::new(
                                self.build_auto_accessor_getter_function(&accessor.weakmap_name),
                            )),
                            set: Some(Box::new(
                                self.build_auto_accessor_setter_function(&accessor.weakmap_name),
                            )),
                            value: None,
                            enumerable: false,
                            configurable: true,
                            writable: false,
                            trailing_comment: self.extract_trailing_comment_for_node(member_node),
                        },
                        leading_comment,
                    });
                }
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // --- Static block (from emit_static_members_ir) ---
                if let Some(block_data) = self.arena.get_block(member_node) {
                    let statements: Vec<IRNode> = block_data
                        .statements
                        .nodes
                        .iter()
                        .map(|&stmt_idx| {
                            if let Some(ref alias) = class_alias {
                                self.convert_statement_static_with_class_alias(stmt_idx, alias)
                            } else {
                                self.convert_statement_static(stmt_idx)
                            }
                        })
                        .collect();

                    let iife = IRNode::StaticBlockIIFE { statements };
                    if has_static_props {
                        // Defer static blocks to after methods/accessors,
                        // interleaved with static property inits in source order
                        deferred_static_prop_inits.push(iife);
                    } else {
                        deferred_static_blocks.push(iife);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT {
                body.push(IRNode::EmptyStatement);
            }
        }

        // Emit deferred static property initializers and static blocks after
        // all methods/accessors, matching tsc's ES5 class member ordering.
        if !deferred_static_prop_inits.is_empty() {
            // Emit class alias preamble before the first static property init
            if let Some(ref alias) = class_alias {
                body.push(IRNode::VarDecl {
                    name: alias.clone().into(),
                    initializer: None,
                });
                body.push(IRNode::expr_stmt(IRNode::assign(
                    IRNode::id(alias.clone()),
                    IRNode::id(self.class_name.clone()),
                )));
            }
            body.append(&mut deferred_static_prop_inits);
        }

        deferred_static_blocks
    }

    /// Check if any static property initializer or static block uses `this`.
    /// Returns true if a class alias is needed (i.e. `var _a; _a = ClassName;`).
    ///
    /// Note: `this` in static methods/getters/setters does NOT need aliasing because
    /// regular functions have their own `this` binding. Only static property initializer
    /// expressions and static block statement bodies need `this` → `_a` substitution.
    pub(super) fn static_members_need_class_alias(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> bool {
        if !self.class_decorators.is_empty() {
            return false;
        }

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };
                // Only static properties with initializers
                if !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                    || self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                {
                    continue;
                }
                if prop_data.initializer.is_none() {
                    continue;
                }
                // Check if the initializer expression contains `this`
                if contains_this_reference(self.arena, prop_data.initializer) {
                    return true;
                }
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // Check if the static block body contains `this`
                if let Some(block_data) = self.arena.get_block(member_node) {
                    for &stmt_idx in &block_data.statements.nodes {
                        if contains_this_reference(self.arena, stmt_idx) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}
