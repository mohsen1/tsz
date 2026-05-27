//! Decorator emit helpers for `ES5ClassTransformer`.
//!
//! Extracted from `class_es5_ir.rs` to keep file sizes manageable.

use crate::transforms::ir::IRNode;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

use super::{
    ES5ClassTransformer, Tc39Es5MemberDecorator, get_identifier_text, serialize_param_types,
    serialize_type_for_metadata,
};

impl<'a> ES5ClassTransformer<'a> {
    /// Collect decorator `NodeIndex` list from a modifier list
    fn collect_decorators_from_modifiers(&self, modifiers: &Option<NodeList>) -> Vec<NodeIndex> {
        let Some(mods) = modifiers else {
            return Vec::new();
        };
        mods.nodes
            .iter()
            .copied()
            .filter(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
            })
            .collect()
    }

    /// Collect parameter decorators from a method's parameter list for ES5 emit.
    /// Returns `Vec` of (`runtime_param_index`, `decorator_node_indices`).
    /// Skips the `this` parameter since it's erased in JS emit.
    fn collect_param_decorators_es5(&self, parameters: &NodeList) -> Vec<(usize, Vec<NodeIndex>)> {
        let mut result = Vec::new();
        let mut runtime_index = 0usize;
        for &param_idx in &parameters.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            // Skip `this` parameter
            if let Some(name_node) = self.arena.get(param.name) {
                if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                    continue;
                }
                if name_node.kind == SyntaxKind::Identifier as u16
                    && self
                        .arena
                        .get_identifier(name_node)
                        .is_some_and(|id| id.escaped_text == "this")
                {
                    continue;
                }
            }

            let decorators = self.collect_decorators_from_modifiers(&param.modifiers);
            if !decorators.is_empty() {
                result.push((runtime_index, decorators));
            }
            runtime_index += 1;
        }
        result
    }

    fn helper_name(&self, name: &str) -> String {
        if self.tslib_prefix {
            format!("{}.{name}", self.tslib_import_binding)
        } else {
            name.to_string()
        }
    }

    /// Render a single decorator expression as a string using the IR printer.
    fn render_single_decorator_expression(&self, dec_idx: NodeIndex) -> Option<String> {
        use crate::transforms::ir_printer::IRPrinter;
        let dec_node = self.arena.get(dec_idx)?;
        let dec = self.arena.get_decorator(dec_node)?;
        let ir_expr = self.convert_expression_static(dec.expression);
        let mut printer = IRPrinter::with_arena(self.arena);
        if let Some(source_text) = self.source_text {
            printer.set_source_text(source_text);
        }
        if let Some(ref transforms) = self.transforms {
            printer.set_transforms(transforms.clone());
        }
        Some(printer.emit(&ir_expr).to_string())
    }

    /// Render decorator expressions as strings using the IR printer.
    fn render_decorator_expressions(&self, decorators: &[NodeIndex]) -> Vec<String> {
        use crate::transforms::ir_printer::IRPrinter;
        let mut result = Vec::new();
        for &dec_idx in decorators {
            if let Some(dec_node) = self.arena.get(dec_idx)
                && let Some(dec) = self.arena.get_decorator(dec_node)
            {
                let ir_expr = self.convert_expression_static(dec.expression);
                let mut printer = IRPrinter::with_arena(self.arena);
                if let Some(source_text) = self.source_text {
                    printer.set_source_text(source_text);
                }
                if let Some(ref transforms) = self.transforms {
                    printer.set_transforms(transforms.clone());
                }
                let rendered = printer.emit(&ir_expr).to_string();
                result.push(rendered);
            }
        }
        result
    }

    pub(super) fn collect_tc39_es5_member_decorators(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Vec<Tc39Es5MemberDecorator> {
        let mut result = Vec::new();
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let (modifiers, name_idx, kind) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if !method.body.is_some() {
                        continue;
                    }
                    (&method.modifiers, method.name, "method")
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (&accessor.modifiers, accessor.name, "getter")
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (&accessor.modifiers, accessor.name, "setter")
                }
                _ => continue,
            };

            let decorators = self.collect_decorators_from_modifiers(modifiers);
            if decorators.is_empty() {
                continue;
            }
            let Some(name) = get_identifier_text(self.arena, name_idx) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }

            let prefix = if self.arena.is_static(modifiers) {
                "_static_"
            } else {
                "_"
            };
            result.push(Tc39Es5MemberDecorator {
                decorators_var: format!("{prefix}{name}_decorators"),
                decorator_exprs: self.render_decorator_expressions(&decorators),
                kind,
                name,
                is_static: self.arena.is_static(modifiers),
            });
        }
        result
    }

    pub fn wrap_tc39_es5_output(
        &self,
        class_idx: NodeIndex,
        override_name: Option<&str>,
        inner_output: &str,
    ) -> Option<String> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;
        let class_name = override_name
            .map(ToOwned::to_owned)
            .or_else(|| get_identifier_text(self.arena, class_data.name))?;
        let member_decorators = self.collect_tc39_es5_member_decorators(class_data);
        if member_decorators.is_empty() {
            return None;
        }

        let alias = "_a";
        let base_indent = "    ".repeat(self.indent_base as usize);
        let body_indent = "    ".repeat((self.indent_base + 1) as usize);
        let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
        let decorator_indent = "    ".repeat((self.indent_base + 3) as usize);

        let prefix = format!("var {class_name} = ");
        let mut class_expr = inner_output.trim_end().strip_prefix(&prefix)?.to_string();
        if let Some(stripped) = class_expr.strip_suffix(';') {
            class_expr = stripped.to_string();
        }
        let mut class_expr_lines = class_expr.lines();
        let first_class_line = class_expr_lines.next().unwrap_or_default();

        let has_instance = member_decorators.iter().any(|member| !member.is_static);
        let has_static = member_decorators.iter().any(|member| member.is_static);

        let mut out = String::new();
        out.push_str(&format!("{base_indent}var {class_name} = function () {{\n"));
        out.push_str(&format!("{body_indent}var {alias};\n"));
        if has_instance {
            out.push_str(&format!(
                "{body_indent}var _instanceExtraInitializers = [];\n"
            ));
        }
        if has_static {
            out.push_str(&format!(
                "{body_indent}var _staticExtraInitializers = [];\n"
            ));
        }
        for member in &member_decorators {
            out.push_str(&format!("{body_indent}var {};\n", member.decorators_var));
        }

        out.push_str(&format!(
            "{body_indent}return {alias} = {first_class_line}\n"
        ));
        let remaining_class_lines: Vec<&str> = class_expr_lines.collect();
        for (idx, line) in remaining_class_lines.iter().enumerate() {
            out.push_str(&inner_indent);
            out.push_str(line);
            if idx + 1 == remaining_class_lines.len() {
                out.push_str(",\n");
            } else {
                out.push('\n');
            }
        }
        out.push_str(&format!("{inner_indent}(function () {{\n"));
        out.push_str(&format!(
            "{decorator_indent}var _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"
        ));
        for member in &member_decorators {
            out.push_str(&format!(
                "{decorator_indent}{} = [{}];\n",
                member.decorators_var,
                member.decorator_exprs.join(", ")
            ));
            let extra_var = if member.is_static {
                "_staticExtraInitializers"
            } else {
                "_instanceExtraInitializers"
            };
            out.push_str(&format!(
                "{decorator_indent}__esDecorate({alias}, null, {}, {{ kind: \"{}\", name: \"{}\", static: {}, private: false, access: {{ {} }}, metadata: _metadata }}, null, {extra_var});\n",
                member.decorators_var,
                member.kind,
                member.name,
                member.is_static,
                self.tc39_es5_member_access(member),
            ));
        }
        out.push_str(&format!(
            "{decorator_indent}if (_metadata) Object.defineProperty({alias}, Symbol.metadata, {{ enumerable: true, configurable: true, writable: true, value: _metadata }});\n"
        ));
        if has_static {
            out.push_str(&format!(
                "{decorator_indent}__runInitializers({alias}, _staticExtraInitializers);\n"
            ));
        }
        out.push_str(&format!("{inner_indent}}})(),\n"));
        out.push_str(&format!("{inner_indent}{alias};\n"));
        out.push_str(&format!("{base_indent}}}();"));
        Some(out)
    }

    fn tc39_es5_member_access(&self, member: &Tc39Es5MemberDecorator) -> String {
        let name = &member.name;
        match member.kind {
            "setter" => format!(
                "has: function (obj) {{ return \"{name}\" in obj; }}, set: function (obj, value) {{ obj.{name} = value; }}"
            ),
            _ => format!(
                "has: function (obj) {{ return \"{name}\" in obj; }}, get: function (obj) {{ return obj.{name}; }}"
            ),
        }
    }

    fn accessor_metadata_strings(
        &self,
        members: &[NodeIndex],
        name_idx: NodeIndex,
        is_static: bool,
    ) -> Vec<String> {
        let Some(target_name) = get_identifier_text(self.arena, name_idx) else {
            return vec![
                format!(
                    "{}(\"design:type\", Object)",
                    self.helper_name("__metadata")
                ),
                format!(
                    "{}(\"design:paramtypes\", [])",
                    self.helper_name("__metadata")
                ),
            ];
        };
        let mut setter_parameters: Option<NodeList> = None;
        let mut getter_type = NodeIndex::NONE;

        for &member_idx in members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::GET_ACCESSOR
                && member_node.kind != syntax_kind_ext::SET_ACCESSOR
            {
                continue;
            }
            let Some(accessor) = self.arena.get_accessor(member_node) else {
                continue;
            };
            if self.arena.is_static(&accessor.modifiers) != is_static {
                continue;
            }
            if get_identifier_text(self.arena, accessor.name).as_deref() != Some(&target_name) {
                continue;
            }
            if member_node.kind == syntax_kind_ext::SET_ACCESSOR {
                setter_parameters = Some(accessor.parameters.clone());
            } else if accessor.type_annotation.is_some() {
                getter_type = accessor.type_annotation;
            }
        }

        let design_type = if let Some(params) = setter_parameters.as_ref() {
            params
                .nodes
                .first()
                .and_then(|&param_idx| self.arena.get(param_idx))
                .and_then(|param_node| self.arena.get_parameter(param_node))
                .and_then(|param| {
                    param
                        .type_annotation
                        .is_some()
                        .then_some(param.type_annotation)
                })
                .map(|type_idx| serialize_type_for_metadata(self.arena, type_idx))
                .unwrap_or_else(|| "Object".to_string())
        } else if getter_type.is_some() {
            serialize_type_for_metadata(self.arena, getter_type)
        } else {
            "Object".to_string()
        };

        let param_types = setter_parameters
            .as_ref()
            .map(|params| serialize_param_types(self.arena, params))
            .unwrap_or_default();

        vec![
            format!(
                "{}(\"design:type\", {design_type})",
                self.helper_name("__metadata")
            ),
            format!(
                "{}(\"design:paramtypes\", [{param_types}])",
                self.helper_name("__metadata")
            ),
        ]
    }

    /// Emit `__decorate` calls for decorated members inside the IIFE body.
    pub(super) fn emit_member_decorator_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // Track accessor names that have already been emitted so that
        // getter/setter pairs produce only one __decorate call (the first one).
        let mut emitted_accessor_names = std::collections::HashSet::<String>::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            enum MemberMeta {
                Property {
                    type_annotation: NodeIndex,
                },
                Method {
                    parameters: NodeList,
                    return_type: NodeIndex,
                    async_returns_promise: bool,
                },
                Accessor {
                    name: NodeIndex,
                    is_static: bool,
                },
            }

            let (modifiers, name_idx, is_property, is_accessor, meta) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    // Skip overload signatures (no body) — decorators on overloads
                    // are not emitted as __decorate targets
                    if !method.body.is_some() {
                        continue;
                    }
                    let has_async_modifier = self
                        .arena
                        .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword);
                    let has_generator_asterisk = method.asterisk_token
                        || crate::transforms::emit_utils::source_header_has_async_generator_asterisk(
                            self.source_text,
                            member_node.pos,
                            self.arena
                                .get(method.body)
                                .map_or(member_node.end, |body| body.pos),
                        );
                    let meta = MemberMeta::Method {
                        parameters: method.parameters.clone(),
                        return_type: method.type_annotation,
                        async_returns_promise: has_async_modifier && !has_generator_asterisk,
                    };
                    (&method.modifiers, method.name, false, false, meta)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let is_auto_accessor = self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                    let meta = MemberMeta::Property {
                        type_annotation: prop.type_annotation,
                    };
                    (&prop.modifiers, prop.name, !is_auto_accessor, false, meta)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (
                        &accessor.modifiers,
                        accessor.name,
                        false,
                        true,
                        MemberMeta::Accessor {
                            name: accessor.name,
                            is_static: self.arena.is_static(&accessor.modifiers),
                        },
                    )
                }
                _ => continue,
            };

            let decorators = self.collect_decorators_from_modifiers(modifiers);

            // Collect parameter decorators for methods/constructors.
            // Each entry is (runtime_param_index, decorator_nodes).
            let param_decorators: Vec<(usize, Vec<NodeIndex>)> = match &meta {
                MemberMeta::Method { parameters, .. } => {
                    self.collect_param_decorators_es5(parameters)
                }
                _ => Vec::new(),
            };

            if decorators.is_empty() && param_decorators.is_empty() {
                continue;
            }

            let is_static = self.arena.is_static(modifiers);

            let member_name = get_identifier_text(self.arena, name_idx);
            let Some(member_name) = member_name else {
                continue;
            };
            if member_name.is_empty() {
                continue;
            }

            // For getter/setter pairs, tsc emits only one __decorate call
            // for the first accessor that has decorators. Skip the second.
            if is_accessor && !emitted_accessor_names.insert(member_name.clone()) {
                continue;
            }

            let mut dec_strs = self.render_decorator_expressions(&decorators);
            // Add __param entries for parameter decorators
            for (param_idx, param_decs) in &param_decorators {
                for dec_idx in param_decs {
                    let dec_str = self.render_single_decorator_expression(*dec_idx);
                    if let Some(dec_str) = dec_str {
                        dec_strs.push(format!(
                            "{}({param_idx}, {dec_str})",
                            self.helper_name("__param")
                        ));
                    }
                }
            }
            let target_str = if is_static {
                self.class_name.clone()
            } else {
                format!("{}.prototype", self.class_name)
            };
            let desc_str = if is_property { "void 0" } else { "null" };

            // Collect metadata strings if emit_decorator_metadata is enabled
            let metadata_strs: Vec<String> = if self.emit_decorator_metadata {
                match &meta {
                    MemberMeta::Property { type_annotation } => {
                        let serialized = serialize_type_for_metadata(self.arena, *type_annotation);
                        vec![format!(
                            "{}(\"design:type\", {serialized})",
                            self.helper_name("__metadata")
                        )]
                    }
                    MemberMeta::Method {
                        parameters,
                        return_type,
                        async_returns_promise,
                    } => {
                        let param_types = serialize_param_types(self.arena, parameters);
                        let ret_type = if return_type.is_some() {
                            serialize_type_for_metadata(self.arena, *return_type)
                        } else if *async_returns_promise {
                            "Promise".to_string()
                        } else {
                            "void 0".to_string()
                        };
                        vec![
                            format!(
                                "{}(\"design:type\", Function)",
                                self.helper_name("__metadata")
                            ),
                            format!(
                                "{}(\"design:paramtypes\", [{param_types}])",
                                self.helper_name("__metadata")
                            ),
                            format!(
                                "{}(\"design:returntype\", {ret_type})",
                                self.helper_name("__metadata")
                            ),
                        ]
                    }
                    MemberMeta::Accessor { name, is_static } => {
                        self.accessor_metadata_strings(&class_data.members.nodes, *name, *is_static)
                    }
                }
            } else {
                Vec::new()
            };

            // Format matching tsc:
            // __decorate([\n        dec1,\n        dec2\n    ], target, "name", desc)
            // Note: first line indent is handled by the body emitter's write_indent().
            // Continuation lines after \n need absolute indentation from column 0.
            // The indent_base accounts for nesting (e.g., namespace IIFE body).
            let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
            let outer_indent = "    ".repeat((self.indent_base + 1) as usize);
            let total_entries = dec_strs.len() + metadata_strs.len();
            let mut raw = String::new();
            raw.push_str(&self.helper_name("__decorate"));
            raw.push_str("([");
            for (i, dec_str) in dec_strs.iter().enumerate() {
                raw.push('\n');
                raw.push_str(&inner_indent);
                raw.push_str(dec_str);
                if i + 1 < total_entries {
                    raw.push(',');
                }
            }
            for (i, meta_str) in metadata_strs.iter().enumerate() {
                raw.push('\n');
                raw.push_str(&inner_indent);
                raw.push_str(meta_str);
                if dec_strs.len() + i + 1 < total_entries {
                    raw.push(',');
                }
            }
            raw.push('\n');
            raw.push_str(&outer_indent);
            raw.push_str("], ");
            raw.push_str(&target_str);
            raw.push_str(", \"");
            raw.push_str(&member_name);
            raw.push_str("\", ");
            raw.push_str(desc_str);
            raw.push(')');

            body.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
                raw.into(),
            ))));
        }
    }

    /// Emit `ClassName = __decorate([dec1, ...], ClassName)` for class-level decorators.
    /// When `emit_decorator_metadata` is enabled and the class has a constructor,
    /// also includes `__metadata("design:paramtypes", [...])` in the decorator array.
    pub(super) fn emit_class_decorator_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let dec_strs = self.render_decorator_expressions(&self.class_decorators);
        if dec_strs.is_empty() {
            return;
        }

        // Collect constructor parameter decorators (__param entries).
        // tsc includes these in the class-level __decorate call between
        // class decorators and __metadata entries.
        let mut param_strs: Vec<String> = Vec::new();
        let mut metadata_strs: Vec<String> = Vec::new();
        if let Some(class_node) = self.arena.get(class_idx)
            && let Some(class_data) = self.arena.get_class(class_node)
        {
            for &member_idx in &class_data.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.arena.get_constructor(member_node)
                {
                    // Collect __param entries for constructor parameter decorators
                    let all_param_decs = self.collect_param_decorators_es5(&ctor.parameters);
                    for (param_idx, decs) in &all_param_decs {
                        for dec_idx in decs {
                            if let Some(dec_str) = self.render_single_decorator_expression(*dec_idx)
                            {
                                param_strs.push(format!(
                                    "{}({param_idx}, {dec_str})",
                                    self.helper_name("__param")
                                ));
                            }
                        }
                    }

                    // Build constructor paramtypes metadata if emit_decorator_metadata is enabled
                    if self.emit_decorator_metadata {
                        let param_types = serialize_param_types(self.arena, &ctor.parameters);
                        metadata_strs.push(format!(
                            "{}(\"design:paramtypes\", [{param_types}])",
                            self.helper_name("__metadata")
                        ));
                    }
                    break;
                }
            }
        }

        // Format matching tsc:
        // ClassName = __decorate([\n        dec1,\n        __param(0, dec),\n        __metadata(...)\n    ], ClassName)
        let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
        let outer_indent = "    ".repeat((self.indent_base + 1) as usize);
        let total_entries = dec_strs.len() + param_strs.len() + metadata_strs.len();
        let mut raw = String::new();
        raw.push_str(&self.class_name);
        raw.push_str(" = ");
        if let Some(alias) = self.class_self_reference_alias.as_ref() {
            raw.push_str(alias);
            raw.push_str(" = ");
        }
        raw.push_str(&self.helper_name("__decorate"));
        raw.push_str("([");
        let mut written = 0;
        for dec_str in &dec_strs {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(dec_str);
            written += 1;
            if written < total_entries {
                raw.push(',');
            }
        }
        for param_str in &param_strs {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(param_str);
            written += 1;
            if written < total_entries {
                raw.push(',');
            }
        }
        for meta_str in &metadata_strs {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(meta_str);
            written += 1;
            if written < total_entries {
                raw.push(',');
            }
        }
        raw.push('\n');
        raw.push_str(&outer_indent);
        raw.push_str("], ");
        raw.push_str(&self.class_name);
        raw.push(')');

        body.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
            raw.into(),
        ))));
    }

    /// Emit `ClassName = __decorate([__param(0, dec), ...], ClassName)` for constructor
    /// parameter decorators when there are no class-level decorators. tsc emits this
    /// at the class level when a constructor parameter has a decorator.
    pub(super) fn emit_ctor_param_decorator_ir(
        &self,
        body: &mut Vec<IRNode>,
        class_idx: NodeIndex,
    ) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // Find the constructor and collect its parameter decorators
        let mut all_param_decs: Vec<(usize, Vec<NodeIndex>)> = Vec::new();
        for &member_idx in &class_data.members.nodes {
            if let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                && let Some(ctor) = self.arena.get_constructor(member_node)
            {
                all_param_decs = self.collect_param_decorators_es5(&ctor.parameters);
                break;
            }
        }

        if all_param_decs.is_empty() {
            return;
        }

        // Build __param(index, dec) strings
        let mut param_strs: Vec<String> = Vec::new();
        for (param_idx, decs) in &all_param_decs {
            for dec_idx in decs {
                if let Some(dec_str) = self.render_single_decorator_expression(*dec_idx) {
                    param_strs.push(format!(
                        "{}({param_idx}, {dec_str})",
                        self.helper_name("__param")
                    ));
                }
            }
        }

        if param_strs.is_empty() {
            return;
        }

        // Build constructor paramtypes metadata if emit_decorator_metadata is enabled
        let metadata_strs: Vec<String> = if self.emit_decorator_metadata {
            let mut meta = Vec::new();
            for &member_idx in &class_data.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.arena.get_constructor(member_node)
                {
                    let param_types = serialize_param_types(self.arena, &ctor.parameters);
                    meta.push(format!(
                        "{}(\"design:paramtypes\", [{param_types}])",
                        self.helper_name("__metadata")
                    ));
                    break;
                }
            }
            meta
        } else {
            Vec::new()
        };

        let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
        let outer_indent = "    ".repeat((self.indent_base + 1) as usize);
        let total_entries = param_strs.len() + metadata_strs.len();
        let mut raw = String::new();
        raw.push_str(&self.class_name);
        raw.push_str(" = ");
        raw.push_str(&self.helper_name("__decorate"));
        raw.push_str("([");
        for (i, param_str) in param_strs.iter().enumerate() {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(param_str);
            if i + 1 < total_entries {
                raw.push(',');
            }
        }
        for (i, meta_str) in metadata_strs.iter().enumerate() {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(meta_str);
            if param_strs.len() + i + 1 < total_entries {
                raw.push(',');
            }
        }
        raw.push('\n');
        raw.push_str(&outer_indent);
        raw.push_str("], ");
        raw.push_str(&self.class_name);
        raw.push(')');

        body.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
            raw.into(),
        ))));
    }
}
