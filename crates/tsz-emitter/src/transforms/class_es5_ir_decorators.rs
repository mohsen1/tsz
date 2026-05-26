//! Decorator emit helpers for `ES5ClassTransformer`.
//!
//! Extracted from `class_es5_ir.rs` to keep file sizes manageable.

use crate::transforms::ir::IRNode;
use rustc_hash::FxHashSet;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::is_private_identifier;
use tsz_scanner::SyntaxKind;

use super::{
    ES5ClassTransformer, Tc39Es5ComputedMemberInjection, Tc39Es5MemberDecorator, Tc39Es5MemberName,
    get_identifier_text, serialize_param_types, serialize_type_for_metadata,
    tc39_es5_propkey_temp_name,
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
        let mut computed_counter = 0;
        let mut last_computed_name = None;
        let mut propkey_counter = 0;
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
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if is_private_identifier(self.arena, prop.name)
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                    {
                        continue;
                    }
                    (&prop.modifiers, prop.name, "field")
                }
                _ => continue,
            };

            if self
                .arena
                .has_modifier(modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }

            let decorators = self.collect_decorators_from_modifiers(modifiers);
            if decorators.is_empty() {
                continue;
            }
            let Some(name) = self.tc39_es5_member_name(
                name_idx,
                &mut computed_counter,
                &mut last_computed_name,
                &mut propkey_counter,
            ) else {
                continue;
            };

            let prefix = if self.arena.is_static(modifiers) {
                "_static_"
            } else {
                "_"
            };
            let kind_prefix = match kind {
                "getter" => "get_",
                "setter" => "set_",
                _ => "",
            };
            let (base_name, suffix) = self.tc39_es5_member_var_name_parts(&name, computed_counter);
            let initializers_var = (kind == "field")
                .then(|| format!("{prefix}{kind_prefix}{base_name}_initializers{suffix}"));
            let extra_initializers_var = (kind == "field")
                .then(|| format!("{prefix}{kind_prefix}{base_name}_extraInitializers{suffix}"));
            result.push(Tc39Es5MemberDecorator {
                member_idx,
                decorators_var: format!("{prefix}{kind_prefix}{base_name}_decorators{suffix}"),
                decorator_exprs: self.render_decorator_expressions(&decorators),
                kind,
                name,
                is_static: self.arena.is_static(modifiers),
                initializers_var,
                extra_initializers_var,
            });
        }
        result
    }

    fn tc39_es5_member_name(
        &self,
        name_idx: NodeIndex,
        computed_counter: &mut u32,
        last_computed_name: &mut Option<String>,
        propkey_counter: &mut u32,
    ) -> Option<Tc39Es5MemberName> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind == SyntaxKind::Identifier as u16 {
            let name = get_identifier_text(self.arena, name_idx)?;
            return (!name.is_empty()).then_some(Tc39Es5MemberName::Identifier(name));
        }

        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = self.arena.get_computed_property(name_node)?;
        if let Some(expr_node) = self.arena.get(computed.expression)
            && expr_node.kind == SyntaxKind::StringLiteral as u16
            && let Some(lit) = self.arena.get_literal(expr_node)
        {
            self.update_tc39_es5_computed_suffix(
                lit.text.as_str(),
                computed_counter,
                last_computed_name,
            );
            return Some(Tc39Es5MemberName::StringLiteral(lit.text.clone()));
        }

        let expr = self.convert_computed_property_expression(computed.expression, false);
        let expr_text = self.render_ir_expression(&expr);
        self.update_tc39_es5_computed_suffix(
            expr_text.as_str(),
            computed_counter,
            last_computed_name,
        );
        let key_var = tc39_es5_propkey_temp_name(*propkey_counter);
        *propkey_counter += 1;
        Some(Tc39Es5MemberName::Computed { expr_text, key_var })
    }

    fn update_tc39_es5_computed_suffix(
        &self,
        current_name: &str,
        computed_counter: &mut u32,
        last_computed_name: &mut Option<String>,
    ) {
        if last_computed_name
            .as_ref()
            .is_none_or(|prev| prev != current_name)
        {
            if last_computed_name.is_some() {
                *computed_counter += 1;
            }
            *last_computed_name = Some(current_name.to_string());
        }
    }

    fn tc39_es5_member_var_name_parts(
        &self,
        name: &Tc39Es5MemberName,
        computed_counter: u32,
    ) -> (String, String) {
        match name {
            Tc39Es5MemberName::Identifier(name) => (name.clone(), String::new()),
            Tc39Es5MemberName::StringLiteral(_) | Tc39Es5MemberName::Computed { .. } => {
                let suffix = if computed_counter > 0 {
                    format!("_{computed_counter}")
                } else {
                    String::new()
                };
                ("member".to_string(), suffix)
            }
        }
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
        let member_decorators = if self.tc39_es5_member_decorators.is_empty() {
            self.collect_tc39_es5_member_decorators(class_data)
        } else {
            self.tc39_es5_member_decorators.clone()
        };
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

        let has_instance = member_decorators
            .iter()
            .any(|member| !member.is_static && !member.is_field());
        let has_static = member_decorators
            .iter()
            .any(|member| member.is_static && !member.is_field());
        let field_pre_iife_assignments =
            self.tc39_es5_field_pre_iife_assignments(&member_decorators);
        let computed_member_injections =
            self.tc39_es5_computed_member_injections(&member_decorators);
        let mut remaining_class_lines: Vec<String> =
            class_expr_lines.map(ToOwned::to_owned).collect();
        self.inject_tc39_es5_computed_member_keys(
            &mut remaining_class_lines,
            class_name.as_str(),
            &computed_member_injections,
        );

        let mut out = String::new();
        out.push_str(&format!("{base_indent}var {class_name} = function () {{\n"));
        out.push_str(&format!("{body_indent}var {alias};\n"));
        let propkey_vars: Vec<&str> = member_decorators
            .iter()
            .filter_map(|member| match &member.name {
                Tc39Es5MemberName::Computed { key_var, .. } => Some(key_var.as_str()),
                _ => None,
            })
            .collect();
        if !propkey_vars.is_empty() {
            out.push_str(&format!("{body_indent}var {};\n", propkey_vars.join(", ")));
        }
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
            if let Some(initializers_var) = member.initializers_var.as_ref() {
                out.push_str(&format!("{body_indent}var {initializers_var} = [];\n"));
            }
            if let Some(extra_initializers_var) = member.extra_initializers_var.as_ref() {
                out.push_str(&format!(
                    "{body_indent}var {extra_initializers_var} = [];\n"
                ));
            }
        }

        out.push_str(&format!(
            "{body_indent}return {alias} = {first_class_line}\n"
        ));
        for (idx, line) in remaining_class_lines.iter().enumerate() {
            out.push_str(&inner_indent);
            out.push_str(line);
            if idx + 1 == remaining_class_lines.len() {
                out.push_str(",\n");
            } else {
                out.push('\n');
            }
        }
        for assignment in &field_pre_iife_assignments {
            out.push_str(&format!("{inner_indent}{assignment},\n"));
        }
        out.push_str(&format!("{inner_indent}(function () {{\n"));
        out.push_str(&format!(
            "{decorator_indent}var _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"
        ));
        {
            let sinked_decorator_vars: FxHashSet<&str> = computed_member_injections
                .iter()
                .flat_map(|injection| injection.decorator_vars.iter().map(String::as_str))
                .collect();
            for member in member_decorators.iter().filter(|member| {
                !member.is_field()
                    && !sinked_decorator_vars.contains(member.decorators_var.as_str())
            }) {
                out.push_str(&format!(
                    "{decorator_indent}{} = [{}];\n",
                    member.decorators_var,
                    member.decorator_exprs.join(", ")
                ));
            }
        }
        for member in &member_decorators {
            if let (Some(initializers_var), Some(extra_initializers_var)) = (
                member.initializers_var.as_ref(),
                member.extra_initializers_var.as_ref(),
            ) {
                out.push_str(&format!(
                    "{decorator_indent}__esDecorate(null, null, {}, {{ kind: \"{}\", name: {}, static: {}, private: false, access: {{ {} }}, metadata: _metadata }}, {initializers_var}, {extra_initializers_var});\n",
                    member.decorators_var,
                    member.kind,
                    self.tc39_es5_context_name(member),
                    member.is_static,
                    self.tc39_es5_member_access(member),
                ));
            } else {
                let extra_var = if member.is_static {
                    "_staticExtraInitializers"
                } else {
                    "_instanceExtraInitializers"
                };
                out.push_str(&format!(
                    "{decorator_indent}__esDecorate({alias}, null, {}, {{ kind: \"{}\", name: {}, static: {}, private: false, access: {{ {} }}, metadata: _metadata }}, null, {extra_var});\n",
                    member.decorators_var,
                    member.kind,
                    self.tc39_es5_context_name(member),
                    member.is_static,
                    self.tc39_es5_member_access(member),
                ));
            }
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
        for initializer in self.tc39_es5_static_field_initializers(&member_decorators, alias) {
            out.push_str(&initializer);
        }
        out.push_str(&format!("{inner_indent}{alias};\n"));
        out.push_str(&format!("{base_indent}}}();"));
        Some(out)
    }

    fn tc39_es5_member_access(&self, member: &Tc39Es5MemberDecorator) -> String {
        let key_expr = self.tc39_es5_key_expr(member);
        let prop_access = match &member.name {
            Tc39Es5MemberName::Identifier(name) => format!("obj.{name}"),
            Tc39Es5MemberName::StringLiteral(_) | Tc39Es5MemberName::Computed { .. } => {
                format!("obj[{key_expr}]")
            }
        };
        match member.kind {
            "setter" => format!(
                "has: function (obj) {{ return {key_expr} in obj; }}, set: function (obj, value) {{ {prop_access} = value; }}"
            ),
            "field" => format!(
                "has: function (obj) {{ return {key_expr} in obj; }}, get: function (obj) {{ return {prop_access}; }}, set: function (obj, value) {{ {prop_access} = value; }}"
            ),
            _ => format!(
                "has: function (obj) {{ return {key_expr} in obj; }}, get: function (obj) {{ return {prop_access}; }}"
            ),
        }
    }

    fn tc39_es5_key_expr(&self, member: &Tc39Es5MemberDecorator) -> String {
        match &member.name {
            Tc39Es5MemberName::Identifier(name) | Tc39Es5MemberName::StringLiteral(name) => {
                format!("\"{name}\"")
            }
            Tc39Es5MemberName::Computed { key_var, .. } => key_var.clone(),
        }
    }

    fn tc39_es5_context_name(&self, member: &Tc39Es5MemberDecorator) -> String {
        self.tc39_es5_key_expr(member)
    }

    fn tc39_es5_computed_member_injections(
        &self,
        members: &[Tc39Es5MemberDecorator],
    ) -> Vec<Tc39Es5ComputedMemberInjection> {
        let mut result = Vec::new();
        let mut assignment_queue = Vec::new();
        let mut decorator_var_queue = Vec::new();
        for member in members {
            if member.is_field() {
                continue;
            }
            assignment_queue.push(format!(
                "{} = [{}]",
                member.decorators_var,
                member.decorator_exprs.join(", ")
            ));
            decorator_var_queue.push(member.decorators_var.clone());
            if let Tc39Es5MemberName::Computed { expr_text, key_var } = &member.name {
                assignment_queue.push(format!(
                    "{key_var} = {}({expr_text})",
                    self.helper_name("__propKey")
                ));
                result.push(Tc39Es5ComputedMemberInjection {
                    kind: member.kind,
                    is_static: member.is_static,
                    expr_text: expr_text.clone(),
                    assignments: std::mem::take(&mut assignment_queue),
                    decorator_vars: std::mem::take(&mut decorator_var_queue),
                });
            }
        }
        result
    }

    fn tc39_es5_field_pre_iife_assignments(
        &self,
        members: &[Tc39Es5MemberDecorator],
    ) -> Vec<String> {
        let mut result = Vec::new();
        for member in members.iter().filter(|member| member.is_field()) {
            result.push(format!(
                "{} = [{}]",
                member.decorators_var,
                member.decorator_exprs.join(", ")
            ));
            if let Tc39Es5MemberName::Computed { expr_text, key_var } = &member.name {
                result.push(format!(
                    "{key_var} = {}({expr_text})",
                    self.helper_name("__propKey")
                ));
            }
        }
        result
    }

    fn tc39_es5_static_field_initializers(
        &self,
        members: &[Tc39Es5MemberDecorator],
        alias: &str,
    ) -> Vec<String> {
        let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
        let body_indent = "    ".repeat((self.indent_base + 3) as usize);
        let mut result = Vec::new();
        let mut previous_extra_initializers: Option<String> = None;
        let mut last_extra_initializers: Option<String> = None;

        for member in members
            .iter()
            .filter(|member| member.is_static && member.is_field())
        {
            let Some(field_init) = self.tc39_es5_static_field_initializer(
                member,
                alias,
                previous_extra_initializers.as_deref(),
            ) else {
                continue;
            };
            result.push(field_init);
            previous_extra_initializers = member.extra_initializers_var.clone();
            last_extra_initializers = member.extra_initializers_var.clone();
        }

        if let Some(extra_initializers) = last_extra_initializers {
            result.push(format!(
                "{inner_indent}(function () {{\n{body_indent}__runInitializers({alias}, {extra_initializers});\n{inner_indent}}})(),\n"
            ));
        }

        result
    }

    fn tc39_es5_static_field_initializer(
        &self,
        member: &Tc39Es5MemberDecorator,
        alias: &str,
        previous_extra_initializers: Option<&str>,
    ) -> Option<String> {
        let member_node = self.arena.get(member.member_idx)?;
        let prop = self.arena.get_property_decl(member_node)?;
        let initializers_var = member.initializers_var.as_ref()?;
        let initial_value = if self.property_initializer_has_equals(member_node, prop) {
            self.render_ir_expression(
                &self.convert_expression_static_with_class_alias(prop.initializer, alias),
            )
        } else {
            "void 0".to_string()
        };
        let value = self.tc39_es5_field_initializer_value(
            alias,
            initializers_var,
            previous_extra_initializers,
            &initial_value,
        );
        let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
        let descriptor_indent = "    ".repeat((self.indent_base + 3) as usize);

        if self.use_define_for_class_fields {
            Some(format!(
                "{inner_indent}Object.defineProperty({alias}, {}, {{\n{descriptor_indent}enumerable: true,\n{descriptor_indent}configurable: true,\n{descriptor_indent}writable: true,\n{descriptor_indent}value: {value}\n{inner_indent}}}),\n",
                self.tc39_es5_define_property_name(member),
            ))
        } else {
            Some(format!(
                "{inner_indent}{} = {value},\n",
                self.tc39_es5_member_target(alias, member),
            ))
        }
    }

    pub(super) fn tc39_es5_field_initializer_value(
        &self,
        receiver: &str,
        initializers_var: &str,
        previous_extra_initializers: Option<&str>,
        initial_value: &str,
    ) -> String {
        let field_initializer =
            format!("__runInitializers({receiver}, {initializers_var}, {initial_value})");
        if let Some(previous_extra_initializers) = previous_extra_initializers {
            format!(
                "(__runInitializers({receiver}, {previous_extra_initializers}), {field_initializer})"
            )
        } else {
            field_initializer
        }
    }

    fn tc39_es5_member_target(&self, receiver: &str, member: &Tc39Es5MemberDecorator) -> String {
        match &member.name {
            Tc39Es5MemberName::Identifier(name) => format!("{receiver}.{name}"),
            Tc39Es5MemberName::StringLiteral(name) => format!("{receiver}[\"{name}\"]"),
            Tc39Es5MemberName::Computed { key_var, .. } => format!("{receiver}[{key_var}]"),
        }
    }

    fn tc39_es5_define_property_name(&self, member: &Tc39Es5MemberDecorator) -> String {
        self.tc39_es5_key_expr(member)
    }

    fn inject_tc39_es5_computed_member_keys(
        &self,
        lines: &mut [String],
        class_name: &str,
        injections: &[Tc39Es5ComputedMemberInjection],
    ) {
        let mut consumed = vec![false; lines.len()];
        for injection in injections {
            let receiver = if injection.is_static {
                class_name.to_string()
            } else {
                format!("{class_name}.prototype")
            };
            let member_prefix = if injection.kind == "method" {
                format!("{receiver}[")
            } else {
                format!("Object.defineProperty({receiver}, ")
            };
            let (needle, replacement) = if injection.kind == "method" {
                (
                    format!("[{}]", injection.expr_text),
                    format!("[({})]", injection.assignments.join(", ")),
                )
            } else {
                (
                    format!("{},", injection.expr_text),
                    format!("({}),", injection.assignments.join(", ")),
                )
            };
            if let Some((line_index, line)) =
                lines.iter_mut().enumerate().find(|(line_index, line)| {
                    !consumed[*line_index]
                        && line.contains(&member_prefix)
                        && line.contains(&needle)
                })
                && let Some(start) = line.find(&needle)
            {
                consumed[line_index] = true;
                let end = start + needle.len();
                let mut rewritten = String::with_capacity(line.len() + replacement.len());
                rewritten.push_str(&line[..start]);
                rewritten.push_str(&replacement);
                rewritten.push_str(&line[end..]);
                *line = rewritten;
            }
        }
    }

    pub(super) fn tc39_es5_decorated_field(
        &self,
        member_idx: NodeIndex,
    ) -> Option<&Tc39Es5MemberDecorator> {
        self.tc39_es5_member_decorators
            .iter()
            .find(|member| member.member_idx == member_idx && member.is_field())
    }

    pub(super) fn tc39_previous_field_extra_initializers_var(
        &self,
        member_idx: NodeIndex,
        is_static: bool,
    ) -> Option<&str> {
        let current_pos = self
            .tc39_es5_member_decorators
            .iter()
            .position(|member| member.member_idx == member_idx && member.is_field())?;
        self.tc39_es5_member_decorators[..current_pos]
            .iter()
            .rev()
            .find(|member| member.is_static == is_static && member.is_field())
            .and_then(|member| member.extra_initializers_var.as_deref())
    }

    pub(super) fn emit_tc39_instance_field_extra_initializers_ir(
        &self,
        body: &mut Vec<IRNode>,
        use_this: bool,
    ) {
        let Some(extra_initializers) = self
            .tc39_es5_member_decorators
            .iter()
            .rev()
            .find(|member| !member.is_static && member.is_field())
            .and_then(|member| member.extra_initializers_var.as_deref())
        else {
            return;
        };
        let receiver_text = if use_this { "_this" } else { "this" };
        body.push(IRNode::expr_stmt(IRNode::Raw(
            format!("__runInitializers({receiver_text}, {extra_initializers})").into(),
        )));
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
