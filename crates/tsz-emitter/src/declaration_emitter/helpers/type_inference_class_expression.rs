//! Class-expression and returned-local-class type text helpers.
//!
//! Extracted from `type_inference.rs` for file-size reasons; behavior is unchanged.

use super::super::DeclarationEmitter;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Rendered instance-type members, partitioned by member category so callers
/// can reassemble them in TypeScript's structural emit order.
///
/// `tsc` builds the resolved members of a synthesized anonymous instance type
/// (for example a mixin/constructor object type) and prints index signatures
/// before named members, with locally declared members preceding members
/// inherited from a base/constraint. Keeping the two buckets separate lets the
/// caller interleave its own class members with constraint members correctly:
/// index signatures (own, then base) lead, then named members (own, then base).
#[derive(Default)]
pub(in crate::declaration_emitter) struct InstanceMemberGroups {
    /// Index-signature member lines (`[x: string]: T;`), already indented.
    pub index_signatures: String,
    /// Named member lines (methods, properties, accessors), already indented.
    pub named_members: String,
}

impl InstanceMemberGroups {
    fn is_empty(&self) -> bool {
        self.index_signatures.trim().is_empty() && self.named_members.trim().is_empty()
    }
}

impl<'a> DeclarationEmitter<'a> {
    /// Renders `member_indices` into [`InstanceMemberGroups`], routing index
    /// signatures and named members into separate buffers. `emit` is invoked
    /// with the scratch emitter for the member's bucket; callers pre-filter out
    /// constructors and static members before delegating here.
    fn render_instance_member_groups<F>(
        &self,
        member_indices: impl IntoIterator<Item = NodeIndex>,
        indent_level: u32,
        recursive_reference: Option<&str>,
        mut emit: F,
    ) -> InstanceMemberGroups
    where
        F: FnMut(&mut DeclarationEmitter<'a>, NodeIndex),
    {
        let mut index_scratch = self.scratch_object_type_body_emitter(indent_level);
        let mut named_scratch = self.scratch_object_type_body_emitter(indent_level);
        if let Some(reference_text) = recursive_reference {
            index_scratch.object_type_recursive_constructor_reference =
                Some(reference_text.to_string());
            named_scratch.object_type_recursive_constructor_reference =
                Some(reference_text.to_string());
        }
        for member_idx in member_indices {
            let is_index_signature = self
                .arena
                .get(member_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::INDEX_SIGNATURE);
            if is_index_signature {
                emit(&mut index_scratch, member_idx);
            } else {
                emit(&mut named_scratch, member_idx);
            }
        }
        InstanceMemberGroups {
            index_signatures: index_scratch.writer.take_output(),
            named_members: named_scratch.writer.take_output(),
        }
    }

    /// Joins own and base member groups into a single instance-type body in
    /// `tsc`'s structural order: index signatures (own, then base) precede named
    /// members (own, then base). The result is trimmed of trailing whitespace.
    fn join_instance_member_groups(
        own: &InstanceMemberGroups,
        base: &InstanceMemberGroups,
    ) -> String {
        let mut combined = String::with_capacity(
            own.index_signatures.len()
                + base.index_signatures.len()
                + own.named_members.len()
                + base.named_members.len(),
        );
        combined.push_str(&own.index_signatures);
        combined.push_str(&base.index_signatures);
        combined.push_str(&own.named_members);
        combined.push_str(&base.named_members);
        combined.truncate(combined.trim_end().len());
        combined
    }

    /// Emits `class`'s static members into `static_scratch` and returns the
    /// node indices of its non-static, non-constructor instance members in
    /// declaration order, ready for [`Self::render_instance_member_groups`].
    fn collect_constructor_instance_members(
        &self,
        class: &tsz_parser::parser::node::ClassData,
        static_scratch: &mut DeclarationEmitter<'a>,
    ) -> Vec<NodeIndex> {
        class
            .members
            .nodes
            .iter()
            .copied()
            .filter(|&member_idx| {
                let Some(member_node) = self.arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                    return false;
                }
                if self.class_member_is_static(member_idx) {
                    static_scratch.emit_class_member(member_idx);
                    return false;
                }
                true
            })
            .collect()
    }

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
        let mut static_scratch = self.scratch_object_type_body_emitter(self.indent_level + 1);
        let instance_member_indices =
            self.collect_constructor_instance_members(class, &mut static_scratch);
        let own_groups = self.render_instance_member_groups(
            instance_member_indices,
            instance_indent,
            None,
            |s, idx| s.emit_class_member_for_constructor_instance_type(idx),
        );
        let base_groups = base_constraint_idx
            .and_then(|constraint_idx| {
                self.constructor_constraint_base_instance_members_text(
                    constraint_idx,
                    instance_indent,
                )
            })
            .unwrap_or_default();
        let members = Self::join_instance_member_groups(&own_groups, &base_groups);
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
            Self::constructor_object_type_text(
                &construct_head,
                members,
                &static_members,
                self.indent_level,
            )
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
        depth: u32,
    ) -> String {
        let member_indent = "    ".repeat((depth + 1) as usize);
        let closing_indent = "    ".repeat(depth as usize);
        let mut constructor_type = if members.is_empty() {
            format!("{{\n{member_indent}{construct_head}: {{}};\n")
        } else {
            format!("{{\n{member_indent}{construct_head}: {{\n{members}\n{member_indent}}};\n")
        };
        if !static_members.is_empty() {
            constructor_type.push_str(static_members);
            constructor_type.push('\n');
        }
        constructor_type.push_str(&closing_indent);
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
        self.class_expression_constructor_type_text_from_ast_with_recursive_reference(
            expr_idx, None,
        )
    }

    pub(in crate::declaration_emitter) fn class_expression_constructor_type_text_from_ast_with_recursive_reference(
        &self,
        expr_idx: NodeIndex,
        recursive_reference_text: Option<&str>,
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

        let instance_indent = self.indent_level + 2;
        let mut static_scratch = self.scratch_object_type_body_emitter(self.indent_level + 1);
        if let Some(reference_text) = recursive_reference_text {
            static_scratch.object_type_recursive_constructor_reference =
                Some(reference_text.to_string());
        }
        let instance_member_indices =
            self.collect_constructor_instance_members(class, &mut static_scratch);
        let own_groups = self.render_instance_member_groups(
            instance_member_indices,
            instance_indent,
            recursive_reference_text,
            |s, idx| s.emit_class_member_for_constructor_instance_type(idx),
        );
        let base_groups = self
            .class_expression_extends_parameter_instance_members(expr_idx, class)
            .unwrap_or_default();
        let mut instance_members = Self::join_instance_member_groups(&own_groups, &base_groups);
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
        let constructor_type = Self::constructor_object_type_text(
            &construct_head,
            &instance_members,
            &static_members,
            self.indent_level,
        );

        if let Some(base_type_text) = extends_parameter_type_text {
            Some(format!("{constructor_type} & {base_type_text}"))
        } else {
            Some(constructor_type)
        }
    }

    pub(in crate::declaration_emitter) fn class_expression_has_type_parameter_modifiers(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let Some(class) = self.arena.get_class(expr_node) else {
            return false;
        };
        let Some(type_params) = class.type_parameters.as_ref() else {
            return false;
        };

        type_params.nodes.iter().copied().any(|param_idx| {
            self.arena
                .get(param_idx)
                .and_then(|param_node| self.arena.get_type_parameter(param_node))
                .and_then(|param| param.modifiers.as_ref())
                .is_some_and(|modifiers| !modifiers.nodes.is_empty())
        })
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
        Self::replace_class_expression_self_name(members, self_name, crate::ELIDED_ANY)
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
    ) -> Option<InstanceMemberGroups> {
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
    ) -> Option<InstanceMemberGroups> {
        let instance_type_idx =
            self.constructor_constraint_instance_type_node_idx(constraint_idx)?;
        self.instance_type_node_members_text_at(instance_type_idx, indent_level)
    }

    fn constructor_constraint_instance_type_node_idx(
        &self,
        constraint_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let constraint_node = self.arena.get(constraint_idx)?;

        if let Some(type_ref) = self.arena.get_type_ref(constraint_node)
            && let Some(type_arguments) = type_ref.type_arguments.as_ref()
            && let Some(instance_arg_index) =
                self.constructor_type_reference_instance_arg_index(type_ref)
        {
            return type_arguments.nodes.get(instance_arg_index).copied();
        }

        if constraint_node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
            && let Some(func_type) = self.arena.get_function_type(constraint_node)
        {
            return Some(func_type.type_annotation);
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
    ) -> Option<InstanceMemberGroups> {
        let node = self.arena.get(type_idx)?;

        if self.type_node_is_any(node) {
            let indent_str = "    ".repeat(indent_level as usize);
            return Some(InstanceMemberGroups {
                index_signatures: format!("{indent_str}[x: string]: any;\n"),
                named_members: String::new(),
            });
        }

        if let Some(type_ref) = self.arena.get_type_ref(node) {
            let name = self.get_identifier_text(type_ref.type_name)?;
            let sym_id = self.resolve_identifier_symbol(type_ref.type_name, &name)?;
            return self.symbol_instance_members_text(sym_id, indent_level);
        }

        if node.kind == syntax_kind_ext::TYPE_LITERAL
            && let Some(type_literal) = self.arena.get_type_literal(node)
        {
            let groups = self.render_instance_member_groups(
                type_literal.members.nodes.iter().copied(),
                indent_level,
                None,
                |s, idx| s.emit_interface_member(idx),
            );
            if !groups.is_empty() {
                return Some(groups);
            }
        }

        None
    }

    fn symbol_instance_members_text(
        &self,
        sym_id: SymbolId,
        indent_level: u32,
    ) -> Option<InstanceMemberGroups> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if let Some(class) = self.arena.get_class(decl_node) {
                let instance_member_indices =
                    class.members.nodes.iter().copied().filter(|&member_idx| {
                        self.arena.get(member_idx).is_some_and(|member_node| {
                            member_node.kind != syntax_kind_ext::CONSTRUCTOR
                        }) && !self.class_member_is_static(member_idx)
                    });
                let groups = self.render_instance_member_groups(
                    instance_member_indices,
                    indent_level,
                    None,
                    |s, idx| s.emit_class_member(idx),
                );
                if !groups.is_empty() {
                    return Some(groups);
                }
            }
            if let Some(interface) = self.arena.get_interface(decl_node) {
                let groups = self.render_instance_member_groups(
                    interface.members.nodes.iter().copied(),
                    indent_level,
                    None,
                    |s, idx| s.emit_interface_member(idx),
                );
                if !groups.is_empty() {
                    return Some(groups);
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
