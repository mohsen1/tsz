//! Field, constructor, and auto-accessor collection for TC39 decorator emission.

use super::TC39DecoratorEmitter;
#[allow(unused_imports)]
use super::helpers::*;
#[allow(unused_imports)]
use super::{
    AutoAccessorClassCtx, AutoAccessorMemberEmitCtx, ClassBodyCtx, ClassBodyFlags,
    ClassDecoratorInstancePrivateFieldInfo, ClassDecoratorVars, CtorInitFlags, CtorMembersCtx,
    CtorOutputCtx, DecoratorApplicationCtx, DecoratorReceiverState, EsDecorateMemberCtx,
    EsDecorateVars, PlainComputedInstanceFieldInfo,
};
#[allow(unused_imports)]
use crate::transforms::emit_utils::hygienic_temp_name;
#[allow(unused_imports)]
use rustc_hash::FxHashMap;
#[allow(unused_imports)]
use tsz_parser::parser::node::{NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> TC39DecoratorEmitter<'a> {
    pub(super) fn collect_decorated_field_info(
        &self,
        decorated_members: &[DecoratedMember],
        computed_key_vars: &[(usize, String)],
    ) -> Vec<DecoratedFieldInfo> {
        let mut result = Vec::new();
        for (i, member) in decorated_members.iter().enumerate() {
            if member.kind != MemberKind::Field {
                continue;
            }
            let (access_expr, is_bracket) = match &member.name {
                MemberName::Identifier(name) | MemberName::Private(name) => (name.clone(), false),
                MemberName::StringLiteral(name) => (format!("\"{name}\""), true),
                MemberName::Computed(_) => {
                    let var = computed_key_vars
                        .iter()
                        .find(|(mi, _)| *mi == i)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| "undefined".to_string());
                    (var, true)
                }
            };
            let initializer_text = self.get_field_initializer_text(member.member_idx);
            result.push(DecoratedFieldInfo {
                access_expr,
                is_bracket_access: is_bracket,
                initializer_text,
                member_var_index: i,
            });
        }
        result
    }

    pub(super) fn collect_plain_computed_instance_fields(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        decorated_members: &[DecoratedMember],
        temp_counter: &mut u32,
    ) -> Vec<PlainComputedInstanceFieldInfo> {
        let decorated_field_indices: std::collections::HashSet<NodeIndex> = decorated_members
            .iter()
            .filter(|member| member.kind == MemberKind::Field)
            .map(|member| member.member_idx)
            .collect();
        let mut result = Vec::new();

        for &member_idx in &class_data.members.nodes {
            if decorated_field_indices.contains(&member_idx) {
                continue;
            }
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if self.arena.is_static(&prop.modifiers)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }
            let Some(name_node) = self.arena.get(prop.name) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                continue;
            }
            let Some(computed) = self.arena.get_computed_property(name_node) else {
                continue;
            };
            result.push(PlainComputedInstanceFieldInfo {
                member_idx,
                key_var: next_temp_var(temp_counter),
                key_expr: computed.expression,
                initializer_text: self.get_field_initializer_text(member_idx),
            });
        }

        result
    }

    pub(super) fn collect_constructor_parameter_properties(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Vec<ParameterPropertyInfo> {
        let mut result = Vec::new();
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.arena.get_constructor(member_node) else {
                continue;
            };
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.arena.get_parameter(param_node) else {
                    continue;
                };
                if !has_parameter_property_modifier(self.arena, &param.modifiers) {
                    continue;
                }
                let name = crate::transforms::emit_utils::identifier_emit_text_or_empty(
                    self.arena, param.name,
                );
                if !name.is_empty() {
                    result.push(ParameterPropertyInfo { name });
                }
            }
        }
        result
    }

    pub(super) fn collect_decorated_auto_accessor_info(
        &self,
        decorated_members: &[DecoratedMember],
        computed_key_vars: &[(usize, String)],
        reserved_storage_bases: &[String],
        skip_member_indices: &std::collections::HashSet<NodeIndex>,
    ) -> Vec<DecoratedAutoAccessorInfo> {
        let mut result = Vec::new();
        let mut generated_name_index = 0u32;
        let mut storage_base_counts: FxHashMap<String, u32> = FxHashMap::default();
        for storage_base in reserved_storage_bases {
            *storage_base_counts.entry(storage_base.clone()).or_default() += 1;
        }

        for (i, member) in decorated_members.iter().enumerate() {
            if member.kind != MemberKind::Accessor {
                continue;
            }
            if skip_member_indices.contains(&member.member_idx) {
                continue;
            }

            let (name, storage_base) = match &member.name {
                MemberName::Identifier(name) => (name.clone(), name.clone()),
                MemberName::Private(name) => {
                    let name = name.trim_start_matches('#').to_string();
                    (format!("#{name}"), name)
                }
                MemberName::StringLiteral(name) => {
                    let storage_base = generated_auto_accessor_name(generated_name_index);
                    generated_name_index += 1;
                    (format!("\"{name}\""), storage_base)
                }
                MemberName::Computed(_) => {
                    let access_name = computed_key_vars
                        .iter()
                        .find(|(mi, _)| *mi == i)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| "undefined".to_string());
                    let storage_base = generated_auto_accessor_name(generated_name_index);
                    generated_name_index += 1;
                    (access_name, storage_base)
                }
            };
            let count = storage_base_counts.entry(storage_base.clone()).or_default();
            let storage_base = if *count == 0 {
                storage_base
            } else {
                format!("{storage_base}_{}", *count)
            };
            *count += 1;

            result.push(DecoratedAutoAccessorInfo {
                name,
                initializer_text: self.get_field_initializer_text(member.member_idx),
                storage_base,
                member_var_index: i,
            });
        }

        result
    }

    pub(super) fn collect_class_decorator_auto_accessor_info(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        decorated_members: &[DecoratedMember],
        class_name: &str,
        class_span_text: &str,
    ) -> Vec<ClassDecoratorAutoAccessorInfo> {
        let decorated_member_indices: std::collections::HashSet<NodeIndex> = decorated_members
            .iter()
            .map(|member| member.member_idx)
            .collect();
        let mut result = Vec::new();
        let mut generated_name_index = 0u32;

        for &member_idx in &class_data.members.nodes {
            let is_decorated = decorated_member_indices.contains(&member_idx);
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if !self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }
            let is_static = self.arena.is_static(&prop.modifiers);
            if !is_static {
                continue;
            }

            let (name, is_private) = self.resolve_member_name(prop.name);
            let storage_base = match &name {
                MemberName::Identifier(name) | MemberName::Private(name) => {
                    name.trim_start_matches('#').to_string()
                }
                MemberName::StringLiteral(_) | MemberName::Computed(_) => {
                    let storage_base = generated_auto_accessor_name(generated_name_index);
                    generated_name_index += 1;
                    storage_base
                }
            };
            let storage_name = self.auto_accessor_weakmap_storage_name(
                class_name,
                &DecoratedAutoAccessorInfo {
                    name: String::new(),
                    initializer_text: String::new(),
                    storage_base: storage_base.clone(),
                    member_var_index: 0,
                },
            );
            let temp_base = if class_name.is_empty() {
                "class".to_string()
            } else {
                class_name.to_string()
            };
            let (getter_temp_var, setter_temp_var) = if is_private {
                (
                    Some(hygienic_temp_name(
                        &format!("_{temp_base}_{storage_base}_get"),
                        class_span_text,
                    )),
                    Some(hygienic_temp_name(
                        &format!("_{temp_base}_{storage_base}_set"),
                        class_span_text,
                    )),
                )
            } else {
                (None, None)
            };
            result.push(ClassDecoratorAutoAccessorInfo {
                member: DecoratedMember {
                    member_idx,
                    kind: MemberKind::Accessor,
                    name,
                    is_static,
                    is_private,
                    decorator_exprs: Vec::new(),
                    captured_decorator_exprs: Vec::new(),
                },
                is_decorated,
                storage_name,
                getter_temp_var,
                setter_temp_var,
                initializer_idx: prop.initializer,
                initializer_text: self.get_field_initializer_text(member_idx),
            });
        }

        result
    }

    pub(super) fn collect_class_decorator_static_private_field_info(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        decorated_members: &[DecoratedMember],
        class_name: &str,
        class_span_text: &str,
    ) -> Vec<ClassDecoratorStaticPrivateFieldInfo> {
        let decorated_member_indices: std::collections::HashSet<NodeIndex> = decorated_members
            .iter()
            .map(|member| member.member_idx)
            .collect();
        let mut result = Vec::new();

        for &member_idx in &class_data.members.nodes {
            let is_decorated = decorated_member_indices.contains(&member_idx);
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                || !self.arena.is_static(&prop.modifiers)
            {
                continue;
            }
            let (MemberName::Private(name), true) = self.resolve_member_name(prop.name) else {
                continue;
            };
            let private_name = name.trim_start_matches('#');
            let temp_base = if class_name.is_empty() {
                "class".to_string()
            } else {
                class_name.to_string()
            };
            let storage_name =
                hygienic_temp_name(&format!("_{temp_base}_{private_name}"), class_span_text);
            result.push(ClassDecoratorStaticPrivateFieldInfo {
                member_idx,
                is_decorated,
                member_name: name,
                storage_name,
                initializer_text: self.get_field_initializer_text(member_idx),
            });
        }

        result
    }

    pub(super) fn collect_class_decorator_instance_private_field_info(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        decorated_members: &[DecoratedMember],
        class_name: &str,
    ) -> Vec<ClassDecoratorInstancePrivateFieldInfo> {
        let decorated_member_indices: std::collections::HashSet<NodeIndex> = decorated_members
            .iter()
            .map(|member| member.member_idx)
            .collect();
        let mut result = Vec::new();

        for &member_idx in &class_data.members.nodes {
            if decorated_member_indices.contains(&member_idx) {
                continue;
            }
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if self.arena.is_static(&prop.modifiers)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }
            let (MemberName::Private(name), true) = self.resolve_member_name(prop.name) else {
                continue;
            };
            let member = DecoratedMember {
                member_idx,
                kind: MemberKind::Field,
                name: MemberName::Private(name),
                is_static: false,
                is_private: true,
                decorator_exprs: Vec::new(),
                captured_decorator_exprs: Vec::new(),
            };
            result.push(ClassDecoratorInstancePrivateFieldInfo {
                member_idx,
                storage_name: self.private_field_storage_name(class_name, &member),
                initializer_text: self.get_field_initializer_text(member_idx),
            });
        }

        result
    }

    pub(super) fn static_private_field_storage_name(
        &self,
        class_name: &str,
        member: &DecoratedMember,
        class_span_text: &str,
    ) -> String {
        let MemberName::Private(name) = &member.name else {
            return hygienic_temp_name("_class_field", class_span_text);
        };
        let private_name = name.trim_start_matches('#');
        let temp_base = if class_name.is_empty() {
            "class".to_string()
        } else {
            class_name.to_string()
        };
        hygienic_temp_name(&format!("_{temp_base}_{private_name}"), class_span_text)
    }

    pub(super) const fn needs_es2015_private_descriptor(&self, member: &DecoratedMember) -> bool {
        !self.use_static_blocks
            && member.is_private
            && matches!(
                member.kind,
                MemberKind::Method | MemberKind::Getter | MemberKind::Setter | MemberKind::Accessor
            )
    }

    pub(super) const fn needs_es2015_instance_private_brand(
        &self,
        member: &DecoratedMember,
    ) -> bool {
        self.needs_es2015_private_descriptor(member) && !member.is_static
    }

    pub(super) fn instance_private_brand_name(&self, class_name: &str) -> String {
        let temp_base = if class_name.is_empty() {
            "class"
        } else {
            class_name
        };
        format!("_{temp_base}_instances")
    }

    pub(super) fn private_field_storage_name(
        &self,
        class_name: &str,
        member: &DecoratedMember,
    ) -> String {
        let MemberName::Private(name) = &member.name else {
            return "_class_field".to_string();
        };
        let private_name = name.trim_start_matches('#');
        let temp_base = if class_name.is_empty() {
            "class"
        } else {
            class_name
        };
        format!("_{temp_base}_{private_name}")
    }

    pub(super) fn private_decorated_member_access_temp_names(
        &self,
        class_name: &str,
        member: &DecoratedMember,
    ) -> Vec<String> {
        match member.kind {
            MemberKind::Accessor => vec![
                self.private_decorated_member_get_temp_name(class_name, member),
                self.private_decorated_member_set_temp_name(class_name, member),
            ],
            MemberKind::Setter => {
                vec![self.private_decorated_member_set_temp_name(class_name, member)]
            }
            MemberKind::Method | MemberKind::Getter => {
                vec![self.private_decorated_member_get_temp_name(class_name, member)]
            }
            MemberKind::Field => Vec::new(),
        }
    }

    pub(super) fn private_decorated_member_get_temp_name(
        &self,
        class_name: &str,
        member: &DecoratedMember,
    ) -> String {
        self.private_decorated_member_temp_name(class_name, member, "get")
    }

    pub(super) fn private_decorated_member_set_temp_name(
        &self,
        class_name: &str,
        member: &DecoratedMember,
    ) -> String {
        self.private_decorated_member_temp_name(class_name, member, "set")
    }

    pub(super) fn private_decorated_member_temp_name(
        &self,
        class_name: &str,
        member: &DecoratedMember,
        access_kind: &str,
    ) -> String {
        let MemberName::Private(name) = &member.name else {
            return format!("_class_member_{access_kind}");
        };
        let private_name = name.trim_start_matches('#');
        let temp_base = if class_name.is_empty() {
            "class"
        } else {
            class_name
        };
        let suffix = match access_kind {
            "set" => "set",
            _ => "get",
        };
        format!("_{temp_base}_{private_name}_{suffix}")
    }

    pub(super) fn es2015_private_access_assignments(
        &self,
        class_name: &str,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
    ) -> Vec<String> {
        let descriptor_var = var_info.descriptor_var.as_deref().unwrap_or("_descriptor");
        match member.kind {
            MemberKind::Method => {
                let temp_name = self.private_decorated_member_get_temp_name(class_name, member);
                vec![format!(
                    "{temp_name} = function {temp_name}() {{ return {descriptor_var}.value; }}"
                )]
            }
            MemberKind::Getter => {
                let temp_name = self.private_decorated_member_get_temp_name(class_name, member);
                vec![format!(
                    "{temp_name} = function {temp_name}() {{ return {descriptor_var}.get.call(this); }}"
                )]
            }
            MemberKind::Setter => {
                let temp_name = self.private_decorated_member_set_temp_name(class_name, member);
                let params = self.private_member_parameter_list(member);
                let param = params.split(',').next().map(str::trim).unwrap_or("value");
                let param = if param.is_empty() { "value" } else { param };
                vec![format!(
                    "{temp_name} = function {temp_name}({param}) {{ return {descriptor_var}.set.call(this, {param}); }}"
                )]
            }
            MemberKind::Accessor => {
                let get_temp = self.private_decorated_member_get_temp_name(class_name, member);
                let set_temp = self.private_decorated_member_set_temp_name(class_name, member);
                vec![
                    format!(
                        "{get_temp} = function {get_temp}() {{ return {descriptor_var}.get.call(this); }}"
                    ),
                    format!(
                        "{set_temp} = function {set_temp}(value) {{ return {descriptor_var}.set.call(this, value); }}"
                    ),
                ]
            }
            MemberKind::Field => Vec::new(),
        }
    }

    pub(super) fn auto_accessor_storage_temp_name(
        &self,
        class_name: &str,
        member: &DecoratedMember,
    ) -> String {
        let class_prefix = if class_name.is_empty() {
            "class"
        } else {
            class_name
        };
        let MemberName::Private(name) = &member.name else {
            return format!("_{class_prefix}_accessor_storage");
        };
        format!(
            "_{class_prefix}_{}_accessor_storage",
            name.trim_start_matches('#')
        )
    }

    pub(super) fn collect_class_decorator_static_private_methods(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        class_name: &str,
        decorated_members: &[DecoratedMember],
        class_span_text: &str,
    ) -> Vec<ClassDecoratorStaticPrivateMethodInfo> {
        let decorated_member_indices: std::collections::HashSet<NodeIndex> = decorated_members
            .iter()
            .map(|member| member.member_idx)
            .collect();
        let mut result = Vec::new();
        for &member_idx in &class_data.members.nodes {
            let is_decorated = decorated_member_indices.contains(&member_idx);
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let (kind, modifiers, name_idx, params, body_idx, body) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (
                        MemberKind::Method,
                        method.modifiers.clone(),
                        method.name,
                        self.parameter_list_text(&method.parameters),
                        method.body,
                        self.function_body_text(method.body),
                    )
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (
                        MemberKind::Getter,
                        accessor.modifiers.clone(),
                        accessor.name,
                        String::new(),
                        accessor.body,
                        self.function_body_text(accessor.body),
                    )
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (
                        MemberKind::Setter,
                        accessor.modifiers.clone(),
                        accessor.name,
                        self.parameter_list_text(&accessor.parameters),
                        accessor.body,
                        self.function_body_text(accessor.body),
                    )
                }
                _ => continue,
            };
            if !self.arena.is_static(&modifiers) {
                continue;
            }
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if name_node.kind != SyntaxKind::PrivateIdentifier as u16 {
                continue;
            }
            let Some(private_name) = self.arena.get_identifier(name_node) else {
                continue;
            };
            let member_name = private_name.escaped_text.to_string();
            let private_name = member_name.trim_start_matches('#');
            let temp_base = if class_name.is_empty() {
                "class".to_string()
            } else {
                class_name.to_string()
            };
            let temp_suffix = match &kind {
                MemberKind::Getter => format!("{private_name}_get"),
                MemberKind::Setter => format!("{private_name}_set"),
                MemberKind::Method if is_decorated => format!("{private_name}_get"),
                _ => private_name.to_string(),
            };
            let temp_var =
                hygienic_temp_name(&format!("_{temp_base}_{temp_suffix}"), class_span_text);
            let descriptor_var = is_decorated.then(|| {
                let kind_prefix = match kind {
                    MemberKind::Getter => "get_",
                    MemberKind::Setter => "set_",
                    MemberKind::Method | MemberKind::Field | MemberKind::Accessor => "",
                };
                format!("_static_private_{kind_prefix}{private_name}_descriptor")
            });
            let needs_wrapper = matches!(kind, MemberKind::Method)
                && (self.node_tree_contains_private_identifier(body_idx, &member_name)
                    || self.class_body_references_private_name(
                        class_data,
                        member_idx,
                        &member_name,
                    ));
            result.push(ClassDecoratorStaticPrivateMethodInfo {
                member_idx,
                kind,
                is_decorated,
                member_name,
                needs_wrapper,
                function_name: temp_var.clone(),
                temp_var,
                descriptor_var,
                params,
                body,
            });
        }
        result
    }

    pub(super) fn class_body_references_private_name(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        owner_member_idx: NodeIndex,
        private_name: &str,
    ) -> bool {
        class_data.members.nodes.iter().any(|&member_idx| {
            member_idx != owner_member_idx
                && self.node_tree_contains_private_identifier(member_idx, private_name)
        })
    }

    pub(super) fn node_tree_contains_private_identifier(
        &self,
        root: NodeIndex,
        private_name: &str,
    ) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == SyntaxKind::PrivateIdentifier as u16
                && let Some(ident) = self.arena.get_identifier(node)
                && ident.escaped_text == private_name
            {
                return true;
            }
            stack.extend(self.arena.get_children(idx));
        }
        false
    }

    pub(super) fn get_field_initializer_text(&self, member_idx: NodeIndex) -> String {
        if let Some(text) = self.field_initializer_texts.get(&member_idx) {
            return text.clone();
        }
        let Some(member_node) = self.arena.get(member_idx) else {
            return String::new();
        };
        let Some(prop) = self.arena.get_property_decl(member_node) else {
            return String::new();
        };
        if prop.initializer == NodeIndex::NONE {
            return String::new();
        }
        self.node_text(prop.initializer)
    }

    pub(super) fn get_constructor_info(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Option<ConstructorInfo> {
        for &member_idx in &class_data.members.nodes {
            let member_node = self.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let ctor = self.arena.get_constructor(member_node)?;
            let source = self.source_text?;

            let params = if ctor.parameters.nodes.is_empty() {
                String::new()
            } else {
                let mut param_texts = Vec::new();
                for &param_idx in &ctor.parameters.nodes {
                    let param_node = self.arena.get(param_idx)?;
                    let param_data = self.arena.get_parameter(param_node)?;
                    let name_text = self.node_text(param_data.name);
                    if param_data.initializer.is_some() {
                        let init_text = self.node_text(param_data.initializer);
                        param_texts.push(format!("{name_text} = {init_text}"));
                    } else if param_data.dot_dot_dot_token {
                        param_texts.push(format!("...{name_text}"));
                    } else {
                        param_texts.push(name_text);
                    }
                }
                param_texts.join(", ")
            };

            if ctor.body == NodeIndex::NONE {
                return Some(ConstructorInfo {
                    params,
                    body_lines: Vec::new(),
                });
            }
            let body_node = self.arena.get(ctor.body)?;
            let block = self.arena.get_block(body_node)?;
            let mut body_lines = Vec::new();
            for &stmt_idx in &block.statements.nodes {
                let stmt_node = self.arena.get(stmt_idx)?;
                let start = stmt_node.pos as usize;
                let end = stmt_node.end as usize;
                if start < source.len() && end <= source.len() && start < end {
                    body_lines.push(source[start..end].trim().to_string());
                }
            }
            return Some(ConstructorInfo { params, body_lines });
        }
        None
    }
}
