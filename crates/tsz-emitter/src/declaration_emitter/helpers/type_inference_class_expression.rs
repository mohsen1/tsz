//! Class-expression and returned-local-class type text helpers.
//!
//! Extracted from `type_inference.rs` for file-size reasons; behavior is unchanged.

use super::super::DeclarationEmitter;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_returned_local_class_constructor_text(
        &self,
        expr_idx: NodeIndex,
        arrow_form: bool,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder
            .symbol_arenas
            .get(&sym_id)
            .map(|arena| arena.as_ref())
            .unwrap_or(self.arena);
        if !std::ptr::eq(source_arena, self.arena) {
            return None;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(func) = self.callable_function_from_symbol_decl(self.arena, decl_idx) else {
                continue;
            };
            let (class_idx, base_param_index) =
                self.function_returned_local_class_extends_parameter(func)?;
            let args = call.arguments.as_ref()?;
            let base_arg = args.nodes.get(base_param_index).copied()?;
            let base_type_text =
                self.direct_value_reference_typeof_text(base_arg)
                    .or_else(|| {
                        self.nameable_constructor_expression_text(base_arg)
                            .map(|name| format!("typeof {name}"))
                    })?;
            let base_constraint_idx =
                self.function_base_parameter_constraint_node_idx(func, base_param_index);
            return self.local_class_constructor_type_text_from_ast(
                class_idx,
                Some(&base_type_text),
                arrow_form,
                base_constraint_idx,
            );
        }

        None
    }

    fn function_returned_local_class_extends_parameter(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<(NodeIndex, usize)> {
        let body_node = self.arena.get(func.body)?;
        let block = self.arena.get_block(body_node)?;

        let returned = block
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|stmt_idx| {
                let stmt_node = self.arena.get(stmt_idx)?;
                if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                    return None;
                }
                let ret = self.arena.get_return_statement(stmt_node)?;
                if !ret.expression.is_some() {
                    return None;
                }
                self.skip_parenthesized_expression(ret.expression)
            })?;

        let returned_node = self.arena.get(returned)?;
        let class_idx = if returned_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            returned
        } else if returned_node.kind == SyntaxKind::Identifier as u16 {
            let returned_name = self.get_identifier_text(returned)?;
            block.statements.nodes.iter().copied().find(|&stmt_idx| {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    return false;
                };
                if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                    return false;
                }
                self.arena
                    .get_class(stmt_node)
                    .and_then(|class| self.get_identifier_text(class.name))
                    .as_deref()
                    == Some(returned_name.as_str())
            })?
        } else {
            return None;
        };

        let class_node = self.arena.get(class_idx)?;
        let class = self.arena.get_class(class_node)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;
        for clause_idx in heritage_clauses.nodes.iter().copied() {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let base_idx = heritage.types.nodes.first().copied()?;
            let base_node = self.arena.get(base_idx)?;
            let base_expr = self
                .arena
                .get_expr_type_args(base_node)
                .map(|expr| expr.expression)
                .unwrap_or(base_idx);
            let base_name = self.get_identifier_text(base_expr)?;
            for (idx, param_idx) in func.parameters.nodes.iter().copied().enumerate() {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                if self.get_identifier_text(param.name).as_deref() == Some(base_name.as_str()) {
                    return Some((class_idx, idx));
                }
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn function_returned_local_class_constructor_type_text(
        &self,
        func_idx: NodeIndex,
    ) -> Option<String> {
        let func_node = self.arena.get(func_idx)?;
        let func = self.arena.get_function(func_node)?;
        let body_node = self.arena.get(func.body)?;
        let block = self.arena.get_block(body_node)?;

        let returned = block
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|stmt_idx| {
                let stmt_node = self.arena.get(stmt_idx)?;
                if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                    return None;
                }
                let ret = self.arena.get_return_statement(stmt_node)?;
                if !ret.expression.is_some() {
                    return None;
                }
                self.skip_parenthesized_expression(ret.expression)
            })?;

        let returned_node = self.arena.get(returned)?;
        if returned_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            return self.class_constructor_object_type_text_from_ast(returned);
        }

        if returned_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let returned_name = self.get_identifier_text(returned)?;

        block.statements.nodes.iter().copied().find_map(|stmt_idx| {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                return None;
            }
            let class = self.arena.get_class(stmt_node)?;
            (self.get_identifier_text(class.name).as_deref() == Some(returned_name.as_str()))
                .then(|| {
                    self.local_class_constructor_type_text_from_ast(stmt_idx, None, false, None)
                })
                .flatten()
        })
    }

    fn class_constructor_object_type_text_from_ast(&self, class_idx: NodeIndex) -> Option<String> {
        self.local_class_constructor_type_text_from_ast(class_idx, None, false, None)
    }

    fn local_class_constructor_type_text_from_ast(
        &self,
        class_idx: NodeIndex,
        base_type_text: Option<&str>,
        arrow_form: bool,
        base_constraint_idx: Option<NodeIndex>,
    ) -> Option<String> {
        let class_node = self.arena.get(class_idx)?;
        let class = self.arena.get_class(class_node)?;

        let mut params_text = String::new();
        if let Some(ctor_idx) = class.members.nodes.iter().copied().find(|&member_idx| {
            self.arena
                .get(member_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::CONSTRUCTOR)
        }) {
            let ctor = self
                .arena
                .get(ctor_idx)
                .and_then(|node| self.arena.get_constructor(node))?;
            let mut scratch = self.scratch_declaration_emitter();
            scratch.in_constructor_params = true;
            scratch.emit_parameters_with_body(&ctor.parameters, ctor.body);
            scratch.in_constructor_params = false;
            params_text = scratch.writer.take_output();
        }
        if params_text.is_empty() && base_type_text.is_some() {
            params_text = "...args: any[]".to_string();
        }

        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);
        let has_static_members = class
            .members
            .nodes
            .iter()
            .copied()
            .any(|member_idx| self.class_member_is_static(member_idx));
        let force_object_form = !is_abstract && (base_type_text.is_some() || has_static_members);
        let use_arrow_form = (arrow_form || is_abstract) && !force_object_form;
        let instance_indent = if use_arrow_form {
            self.indent_level + 1
        } else {
            self.indent_level + 2
        };
        let mut instance_scratch = self.scratch_object_type_body_emitter(instance_indent);
        let mut static_scratch = self.scratch_object_type_body_emitter(self.indent_level + 1);
        if let Some(constraint_idx) = base_constraint_idx
            && let Some(base_members) = self
                .constructor_constraint_base_instance_members_text(constraint_idx, instance_indent)
        {
            instance_scratch.write(&base_members);
        }
        for member_idx in class.members.nodes.iter().copied() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            if self.class_member_is_static(member_idx) {
                static_scratch.emit_class_member(member_idx);
            } else {
                instance_scratch.emit_class_member_for_constructor_instance_type(member_idx);
            }
        }
        let members = instance_scratch.writer.take_output();
        let members = Self::strip_abstract_member_modifiers(members.trim_end());
        let members = members.as_str();
        let static_members = static_scratch.writer.take_output();
        let static_members = Self::strip_static_prefix_from_class_expression_static_members(
            static_members.trim_end(),
        );

        let constructor_type = if use_arrow_form {
            let prefix = if is_abstract { "abstract new" } else { "new" };
            let construct_head = self.class_expression_construct_head(prefix, class, &params_text);
            let arrow_type = Self::constructor_arrow_type_text(&construct_head, members);
            Self::constructor_static_intersection_type_text(&arrow_type, &static_members)
        } else {
            let construct_head = self.class_expression_construct_head("new", class, &params_text);
            Self::constructor_object_type_text(&construct_head, members, &static_members)
        };

        if let Some(base_type_text) = base_type_text {
            if use_arrow_form {
                Some(format!("({constructor_type}) & {base_type_text}"))
            } else {
                Some(format!("{constructor_type} & {base_type_text}"))
            }
        } else {
            Some(constructor_type)
        }
    }

    fn constructor_arrow_type_text(construct_head: &str, members: &str) -> String {
        if members.is_empty() {
            format!("{construct_head} => {{}}")
        } else {
            format!("{construct_head} => {{\n{members}\n}}")
        }
    }

    fn constructor_object_type_text(
        construct_head: &str,
        members: &str,
        static_members: &str,
    ) -> String {
        let mut constructor_type = if members.is_empty() {
            format!("{{\n    {construct_head}: {{}};\n")
        } else {
            format!("{{\n    {construct_head}: {{\n{members}\n    }};\n")
        };
        if !static_members.is_empty() {
            constructor_type.push_str(static_members);
            constructor_type.push('\n');
        }
        constructor_type.push('}');
        constructor_type
    }

    fn constructor_static_intersection_type_text(
        constructor_type: &str,
        static_members: &str,
    ) -> String {
        if static_members.is_empty() {
            return constructor_type.to_string();
        }
        format!("({constructor_type}) & {{\n{static_members}\n}}")
    }

    fn emit_class_member_for_constructor_instance_type(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };
        let Some(prop) = self.arena.get_property_decl(member_node) else {
            self.emit_class_member(member_idx);
            return;
        };
        if !self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
        {
            self.emit_class_member(member_idx);
            return;
        }
        if self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword)
            || self.member_has_private_identifier_name(member_idx)
            || self.member_has_non_emittable_computed_name(member_idx)
        {
            self.emit_class_member(member_idx);
            return;
        }

        let type_text = self
            .constructor_instance_auto_accessor_type_text(member_idx, prop)
            .unwrap_or_else(|| "unknown".to_string());
        self.write_indent();
        self.write("get ");
        self.emit_node(prop.name);
        self.write("(): ");
        self.write(&type_text);
        self.write(";");
        self.write_line();
        self.write_indent();
        self.write("set ");
        self.emit_node(prop.name);
        self.write("(arg: ");
        self.write(&type_text);
        self.write(");");
        self.write_line();
    }

    fn constructor_instance_auto_accessor_type_text(
        &self,
        prop_idx: NodeIndex,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> Option<String> {
        if prop.type_annotation.is_some() {
            let mut scratch = self.scratch_declaration_emitter();
            scratch.emit_type(prop.type_annotation);
            return Some(scratch.writer.take_output());
        }
        if let Some(type_id) = self.get_node_type_or_names(&[prop_idx, prop.name]) {
            return Some(self.print_type_id(type_id));
        }
        if prop.initializer.is_some() {
            return self.allowlisted_initializer_type_text(prop.initializer);
        }
        None
    }

    fn strip_abstract_member_modifiers(members: &str) -> String {
        members
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix("abstract ") {
                    let indent_len = line.len() - trimmed.len();
                    format!("{}{}", &line[..indent_len], rest)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Constraint node of the type parameter named by the function parameter
    /// at `base_param_index`, or `None` if the parameter is not annotated as
    /// a bare reference to one of the enclosing function's type parameters.
    fn function_base_parameter_constraint_node_idx(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        base_param_index: usize,
    ) -> Option<NodeIndex> {
        let param_idx = func.parameters.nodes.get(base_param_index).copied()?;
        let param = self.arena.get_parameter_at(param_idx)?;
        let type_node = self.arena.get(param.type_annotation)?;
        let type_ref = self.arena.get_type_ref(type_node)?;
        let type_param_name = self.get_identifier_text(type_ref.type_name)?;
        self.type_param_constraint_idx(func, &type_param_name)
    }

    fn type_param_constraint_idx(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        type_param_name: &str,
    ) -> Option<NodeIndex> {
        let type_params = func.type_parameters.as_ref()?;
        for type_param_idx in type_params.nodes.iter().copied() {
            let type_param = self.arena.get_type_parameter_at(type_param_idx)?;
            if self.get_identifier_text(type_param.name).as_deref() != Some(type_param_name) {
                continue;
            }
            return type_param.constraint.into_option();
        }
        None
    }

    pub(in crate::declaration_emitter) fn class_expression_constructor_type_text_from_ast(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let class = self.arena.get_class(expr_node)?;
        let extends_parameter_type_text =
            self.class_expression_extends_parameter_type_text(expr_idx, class);

        let mut params_text = String::new();
        if let Some(ctor_idx) = class.members.nodes.iter().copied().find(|&member_idx| {
            self.arena
                .get(member_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::CONSTRUCTOR)
        }) {
            let ctor = self
                .arena
                .get(ctor_idx)
                .and_then(|node| self.arena.get_constructor(node))?;
            let mut scratch = self.scratch_declaration_emitter();
            scratch.in_constructor_params = true;
            scratch.emit_parameters_with_body(&ctor.parameters, ctor.body);
            scratch.in_constructor_params = false;
            params_text = scratch.writer.take_output();
        }
        if params_text.is_empty() && extends_parameter_type_text.is_some() {
            params_text = "...args: any[]".to_string();
        }

        let mut instance_scratch = self.scratch_object_type_body_emitter(self.indent_level + 2);
        let mut static_scratch = self.scratch_object_type_body_emitter(self.indent_level + 1);
        for member_idx in class.members.nodes.iter().copied() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            if self.class_member_is_static(member_idx) {
                static_scratch.emit_class_member(member_idx);
            } else {
                instance_scratch.emit_class_member_for_constructor_instance_type(member_idx);
            }
        }
        if let Some(base_instance_members) =
            self.class_expression_extends_parameter_instance_members(expr_idx, class)
        {
            instance_scratch.write(&base_instance_members);
        }
        let instance_members = instance_scratch.writer.take_output();
        let mut instance_members = instance_members.trim_end().to_string();
        let static_members = static_scratch.writer.take_output();
        let mut static_members = Self::strip_static_prefix_from_class_expression_static_members(
            static_members.trim_end(),
        );
        if let Some(self_name) = self.get_identifier_text(class.name) {
            let elided_instance_members =
                Self::elide_class_expression_self_name(&instance_members, &self_name);
            let closing_indent = "    ".repeat((self.indent_level + 1) as usize);
            let nested_instance = format!("{{\n{elided_instance_members}\n{closing_indent}}}");
            instance_members = elided_instance_members;
            static_members = Self::replace_class_expression_self_name(
                &static_members,
                &self_name,
                &nested_instance,
            );
        }

        let construct_head = self.class_expression_construct_head("new", class, &params_text);
        let constructor_type =
            Self::constructor_object_type_text(&construct_head, &instance_members, &static_members);

        if let Some(base_type_text) = extends_parameter_type_text {
            Some(format!("{constructor_type} & {base_type_text}"))
        } else {
            Some(constructor_type)
        }
    }

    fn class_member_is_static(&self, member_idx: NodeIndex) -> bool {
        if let Some(info) = self.class_member_info(member_idx) {
            return info.is_static;
        }
        self.arena
            .get(member_idx)
            .and_then(|member_node| self.arena.get_index_signature(member_node))
            .is_some_and(|index| self.arena.is_static(&index.modifiers))
    }

    fn class_expression_construct_head(
        &self,
        prefix: &str,
        class: &tsz_parser::parser::node::ClassData,
        params_text: &str,
    ) -> String {
        let type_params = class
            .type_parameters
            .as_ref()
            .map(|type_params| {
                let mut scratch = self.scratch_declaration_emitter();
                scratch.emit_type_parameters(type_params);
                scratch.writer.take_output()
            })
            .unwrap_or_default();
        if type_params.is_empty() {
            format!("{prefix} ({params_text})")
        } else {
            format!("{prefix} {type_params}({params_text})")
        }
    }

    fn strip_static_prefix_from_class_expression_static_members(members: &str) -> String {
        members
            .lines()
            .map(|line| {
                if let Some((indent, rest)) = line.split_once("static ") {
                    format!("{indent}{rest}")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn elide_class_expression_self_name(members: &str, self_name: &str) -> String {
        Self::replace_class_expression_self_name(members, self_name, "/*elided*/ any")
    }

    fn replace_class_expression_self_name(
        members: &str,
        self_name: &str,
        replacement: &str,
    ) -> String {
        let mut out = String::with_capacity(members.len());
        let bytes = members.as_bytes();
        let needle = self_name.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + needle.len() <= bytes.len()
                && &bytes[i..i + needle.len()] == needle
                && Self::class_expression_self_name_boundary(bytes, i, i + needle.len())
            {
                out.push_str(replacement);
                i += needle.len();
                if bytes.get(i).copied() == Some(b'<') {
                    if let Some(end) = Self::scan_type_argument_list(bytes, i) {
                        i = end;
                    }
                }
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        out
    }

    fn scan_type_argument_list(bytes: &[u8], start: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut i = start;
        while i < bytes.len() {
            match bytes[i] {
                b'<' => {
                    depth += 1;
                    i += 1;
                }
                b'>' => {
                    if depth == 0 {
                        return None;
                    }
                    depth -= 1;
                    i += 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                b'"' | b'\'' => {
                    let quote = bytes[i];
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == b'\\' {
                            i = (i + 2).min(bytes.len());
                        } else if bytes[i] == quote {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        None
    }

    fn class_expression_self_name_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
        let ident = |b: u8| b == b'_' || b == b'$' || b.is_ascii_alphanumeric();
        start.checked_sub(1).is_none_or(|idx| !ident(bytes[idx]))
            && bytes.get(end).is_none_or(|b| !ident(*b))
    }

    fn class_expression_extends_parameter_instance_members(
        &self,
        expr_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<String> {
        let enclosing_func = self.enclosing_function_for_node(expr_idx)?;
        let base_type_text = self.class_expression_extends_parameter_type_text(expr_idx, class)?;
        let constraint_idx = self.type_param_constraint_idx(enclosing_func, &base_type_text)?;
        self.constructor_constraint_base_instance_members_text(
            constraint_idx,
            self.indent_level + 2,
        )
    }

    /// Instance members text inherited from a generic constructor constraint,
    /// indented at `indent_level` and ready to inline into the constructor's
    /// instance type body. Extracts the instance type from either a named
    /// `Ctor<X>` reference's first type argument or an inline `(abstract)
    /// new (...) => X` constructor type, then renders members of that type.
    pub(in crate::declaration_emitter) fn constructor_constraint_base_instance_members_text(
        &self,
        constraint_idx: NodeIndex,
        indent_level: u32,
    ) -> Option<String> {
        let constraint_node = self.arena.get(constraint_idx)?;

        if let Some(type_ref) = self.arena.get_type_ref(constraint_node)
            && let Some(type_arguments) = type_ref.type_arguments.as_ref()
            && let Some(instance_arg_index) =
                self.constructor_type_reference_instance_arg_index(type_ref)
            && let Some(instance_arg_idx) = type_arguments.nodes.get(instance_arg_index).copied()
        {
            return self.instance_type_node_members_text_at(instance_arg_idx, indent_level);
        }

        if constraint_node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
            && let Some(func_type) = self.arena.get_function_type(constraint_node)
        {
            return self
                .instance_type_node_members_text_at(func_type.type_annotation, indent_level);
        }

        None
    }

    fn constructor_type_reference_instance_arg_index(
        &self,
        type_ref: &tsz_parser::parser::node::TypeRefData,
    ) -> Option<usize> {
        let name = self.get_identifier_text(type_ref.type_name)?;
        let sym_id = self.resolve_identifier_symbol(type_ref.type_name, &name)?;
        self.symbol_constructor_instance_type_arg_index(sym_id)
    }

    fn symbol_constructor_instance_type_arg_index(&self, sym_id: SymbolId) -> Option<usize> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if let Some(alias) = self.arena.get_type_alias(decl_node)
                && let Some(index) = self.constructor_type_node_instance_type_arg_index(
                    alias.type_node,
                    alias.type_parameters.as_ref(),
                )
            {
                return Some(index);
            }
            if let Some(interface) = self.arena.get_interface(decl_node) {
                for member_idx in interface.members.nodes.iter().copied() {
                    let Some(member_node) = self.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != syntax_kind_ext::CONSTRUCT_SIGNATURE {
                        continue;
                    }
                    let Some(signature) = self.arena.get_signature(member_node) else {
                        continue;
                    };
                    if let Some(index) = self.constructor_return_type_parameter_index(
                        signature.type_annotation,
                        interface.type_parameters.as_ref(),
                    ) {
                        return Some(index);
                    }
                }
            }
        }
        None
    }

    fn constructor_type_node_instance_type_arg_index(
        &self,
        type_idx: NodeIndex,
        type_parameters: Option<&tsz_parser::NodeList>,
    ) -> Option<usize> {
        let node = self.arena.get(type_idx)?;
        if node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
            && let Some(func_type) = self.arena.get_function_type(node)
        {
            return self.constructor_return_type_parameter_index(
                func_type.type_annotation,
                type_parameters,
            );
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL
            && let Some(type_literal) = self.arena.get_type_literal(node)
        {
            for member_idx in type_literal.members.nodes.iter().copied() {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::CONSTRUCT_SIGNATURE {
                    continue;
                }
                let Some(signature) = self.arena.get_signature(member_node) else {
                    continue;
                };
                if let Some(index) = self.constructor_return_type_parameter_index(
                    signature.type_annotation,
                    type_parameters,
                ) {
                    return Some(index);
                }
            }
        }
        None
    }

    fn constructor_return_type_parameter_index(
        &self,
        return_type_idx: NodeIndex,
        type_parameters: Option<&tsz_parser::NodeList>,
    ) -> Option<usize> {
        let return_node = self.arena.get(return_type_idx)?;
        let return_ref = self.arena.get_type_ref(return_node)?;
        if return_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }
        let return_name = self.get_identifier_text(return_ref.type_name)?;
        let type_parameters = type_parameters?;
        for (index, type_param_idx) in type_parameters.nodes.iter().copied().enumerate() {
            let type_param = self.arena.get_type_parameter_at(type_param_idx)?;
            if self.get_identifier_text(type_param.name).as_deref() == Some(return_name.as_str()) {
                return Some(index);
            }
        }
        None
    }

    /// True if `node` syntactically denotes `any`. The parser produces either
    /// an `AnyKeyword` node or a `TypeReference` to the `any` keyword
    /// depending on context, so both shapes are recognised.
    fn type_node_is_any(&self, node: &tsz_parser::parser::node::Node) -> bool {
        if node.kind == SyntaxKind::AnyKeyword as u16 {
            return true;
        }
        if let Some(type_ref) = self.arena.get_type_ref(node)
            && type_ref
                .type_arguments
                .as_ref()
                .is_none_or(|args| args.nodes.is_empty())
            && self.get_identifier_text(type_ref.type_name).as_deref() == Some("any")
        {
            return true;
        }
        false
    }

    /// Render the members of a constructor's return-type node at
    /// `indent_level`, returning `None` for shapes whose members are not
    /// statically representable in DTS.
    fn instance_type_node_members_text_at(
        &self,
        type_idx: NodeIndex,
        indent_level: u32,
    ) -> Option<String> {
        let node = self.arena.get(type_idx)?;

        if self.type_node_is_any(node) {
            let indent_str = "    ".repeat(indent_level as usize);
            return Some(format!("{indent_str}[x: string]: any;\n"));
        }

        if let Some(type_ref) = self.arena.get_type_ref(node) {
            let name = self.get_identifier_text(type_ref.type_name)?;
            let sym_id = self.resolve_identifier_symbol(type_ref.type_name, &name)?;
            return self.symbol_instance_members_text(sym_id, indent_level);
        }

        if node.kind == syntax_kind_ext::TYPE_LITERAL
            && let Some(type_literal) = self.arena.get_type_literal(node)
        {
            let mut scratch = self.scratch_object_type_body_emitter(indent_level);
            for member_idx in type_literal.members.nodes.iter().copied() {
                scratch.emit_interface_member(member_idx);
            }
            let output = scratch.writer.take_output();
            if !output.trim().is_empty() {
                return Some(output);
            }
        }

        None
    }

    fn symbol_instance_members_text(&self, sym_id: SymbolId, indent_level: u32) -> Option<String> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if let Some(class) = self.arena.get_class(decl_node) {
                let mut scratch = self.scratch_object_type_body_emitter(indent_level);
                for member_idx in class.members.nodes.iter().copied() {
                    let Some(member_node) = self.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind == syntax_kind_ext::CONSTRUCTOR
                        || self.class_member_is_static(member_idx)
                    {
                        continue;
                    }
                    scratch.emit_class_member(member_idx);
                }
                let output = scratch.writer.take_output();
                if !output.trim().is_empty() {
                    return Some(output);
                }
            }
            if let Some(interface) = self.arena.get_interface(decl_node) {
                let mut scratch = self.scratch_object_type_body_emitter(indent_level);
                for member_idx in interface.members.nodes.iter().copied() {
                    scratch.emit_interface_member(member_idx);
                }
                let output = scratch.writer.take_output();
                if !output.trim().is_empty() {
                    return Some(output);
                }
            }
        }
        None
    }

    fn class_expression_extends_parameter_type_text(
        &self,
        expr_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<String> {
        let enclosing_func = self.enclosing_function_for_node(expr_idx)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;
        for clause_idx in heritage_clauses.nodes.iter().copied() {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let base_idx = heritage.types.nodes.first().copied()?;
            let base_node = self.arena.get(base_idx)?;
            let base_expr = self
                .arena
                .get_expr_type_args(base_node)
                .map(|expr| expr.expression)
                .unwrap_or(base_idx);
            if let Some(type_text) = self.function_parameter_type_text(enclosing_func, base_expr) {
                return Some(type_text);
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn direct_returned_class_expression(
        &self,
        body_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let mut returned_class = None;
        for stmt_idx in block.statements.nodes.iter().copied() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.arena.get_return_statement(stmt_node)?;
            if !ret.expression.is_some() {
                return None;
            }
            let expr_idx = self.skip_parenthesized_expression(ret.expression)?;
            let expr_node = self.arena.get(expr_idx)?;
            if expr_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
                return None;
            }
            if returned_class.replace(expr_idx).is_some() {
                return None;
            }
        }
        returned_class
    }
}
