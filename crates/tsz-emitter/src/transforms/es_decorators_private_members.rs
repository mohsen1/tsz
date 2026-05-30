//! ES-decorate call and private member emit for TC39 decorator emission.

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
    pub(super) fn emit_es_decorate_call(
        &self,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
        member_ctx: &EsDecorateMemberCtx<'_>,
        indent: &str,
        out: &mut String,
        vars: &EsDecorateVars<'_>,
    ) {
        let EsDecorateMemberCtx {
            member_index,
            class_alias,
            class_private_ref,
            class_name,
            computed_key_vars,
            class_decorator_static_private_methods: _,
            class_decorator_auto_accessor_infos,
            class_decorator_static_private_fields: _,
        } = member_ctx;
        let EsDecorateVars {
            instance_extra_initializers_var,
            static_extra_initializers_var,
            metadata_var,
        } = vars;
        let kind_str = match member.kind {
            MemberKind::Method => "method",
            MemberKind::Getter => "getter",
            MemberKind::Setter => "setter",
            MemberKind::Field => "field",
            MemberKind::Accessor => "accessor",
        };

        let name_str = self.member_name_for_context(member, computed_key_vars, *member_index);
        let access_str = self.member_access_for_context(member, member_ctx);

        let is_field_like = matches!(member.kind, MemberKind::Field | MemberKind::Accessor);

        let descriptor_arg = if member.is_private
            && (self.use_static_blocks || self.needs_es2015_private_descriptor(member))
        {
            self.private_member_descriptor_arg(
                member,
                var_info,
                &name_str,
                class_alias,
                class_private_ref,
                class_name,
                class_decorator_auto_accessor_infos,
            )
        } else {
            "null".to_string()
        };

        // For methods/getters/setters/accessors/private, first arg is the class ref.
        // For plain fields, first arg is null.
        let ctor_arg = if member.kind == MemberKind::Field {
            "null".to_string()
        } else {
            class_alias.to_string()
        };

        // For fields/accessors, pass per-field initializer and extra-initializer arrays.
        // For methods/getters/setters, pass null + instance/static extra initializers.
        let (init_arg, extra_init_arg) = if is_field_like {
            let init = var_info.initializers_var.as_deref().unwrap_or("null");
            let extra = var_info.extra_initializers_var.as_deref().unwrap_or("null");
            (init.to_string(), extra.to_string())
        } else {
            let extra = if member.is_static {
                static_extra_initializers_var.to_string()
            } else {
                instance_extra_initializers_var.to_string()
            };
            ("null".to_string(), extra)
        };

        let es_decorate = self.helper("__esDecorate");
        out.push_str(&format!(
            "{indent}{es_decorate}({ctor_arg}, {descriptor_arg}, {}, {{ kind: \"{kind_str}\", name: {name_str}, static: {}, private: {}, access: {{ {access_str} }}, metadata: {metadata_var} }}, {init_arg}, {extra_init_arg});\n",
            var_info.decorators_var,
            member.is_static,
            member.is_private,
        ));
    }

    pub(super) fn private_member_descriptor_arg(
        &self,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
        name_str: &str,
        class_alias: &str,
        class_private_ref: &str,
        class_name: &str,
        class_decorator_auto_accessor_infos: &[ClassDecoratorAutoAccessorInfo],
    ) -> String {
        let descriptor_var = var_info.descriptor_var.as_deref().unwrap_or("_descriptor");
        let set_function_name = self.helper("__setFunctionName");
        match member.kind {
            MemberKind::Method => {
                let function_expr = self.private_method_function_expr(member);
                format!(
                    "{descriptor_var} = {{ value: {set_function_name}({function_expr}, {name_str}) }}"
                )
            }
            MemberKind::Getter => {
                let function_expr = self.private_getter_function_expr(member);
                format!(
                    "{descriptor_var} = {{ get: {set_function_name}({function_expr}, {name_str}, \"get\") }}"
                )
            }
            MemberKind::Setter => {
                let function_expr = self.private_setter_function_expr(member);
                format!(
                    "{descriptor_var} = {{ set: {set_function_name}({function_expr}, {name_str}, \"set\") }}"
                )
            }
            MemberKind::Accessor => {
                if member.is_static
                    && member.is_private
                    && let Some(info) = class_decorator_auto_accessor_infos
                        .iter()
                        .find(|info| info.member.member_idx == member.member_idx)
                {
                    let get_helper = self.helper("__classPrivateFieldGet");
                    let set_helper = self.helper("__classPrivateFieldSet");
                    let receiver = if self.use_static_blocks {
                        "this"
                    } else {
                        class_private_ref
                    };
                    return format!(
                        "{descriptor_var} = {{ get: {set_function_name}(function () {{ return {get_helper}({receiver}, {class_private_ref}, \"f\", {}); }}, {name_str}, \"get\"), set: {set_function_name}(function (value) {{ {set_helper}({receiver}, {class_private_ref}, value, \"f\", {}); }}, {name_str}, \"set\") }}",
                        info.storage_name, info.storage_name
                    );
                }
                if self.needs_es2015_private_descriptor(member) {
                    let storage_name = self.auto_accessor_storage_temp_name(class_name, member);
                    let get_helper = self.helper("__classPrivateFieldGet");
                    let set_helper = self.helper("__classPrivateFieldSet");
                    if member.is_static {
                        return format!(
                            "{descriptor_var} = {{ get: {set_function_name}(function () {{ return {get_helper}({class_alias}, {class_alias}, \"f\", {storage_name}); }}, {name_str}, \"get\"), set: {set_function_name}(function (value) {{ {set_helper}({class_alias}, {class_alias}, value, \"f\", {storage_name}); }}, {name_str}, \"set\") }}"
                        );
                    }
                    return format!(
                        "{descriptor_var} = {{ get: {set_function_name}(function () {{ return {get_helper}(this, {storage_name}, \"f\"); }}, {name_str}, \"get\"), set: {set_function_name}(function (value) {{ {set_helper}(this, {storage_name}, value, \"f\"); }}, {name_str}, \"set\") }}"
                    );
                }
                let storage_name = self.private_auto_accessor_storage_name(member);
                format!(
                    "{descriptor_var} = {{ get: {set_function_name}(function () {{ return this.{storage_name}; }}, {name_str}, \"get\"), set: {set_function_name}(function (value) {{ this.{storage_name} = value; }}, {name_str}, \"set\") }}"
                )
            }
            MemberKind::Field => "null".to_string(),
        }
    }

    pub(super) fn emit_private_decorated_member_wrapper(
        &self,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
        indent: &str,
        out: &mut String,
    ) {
        let Some(member_name) = self.private_member_name(member) else {
            return;
        };
        let descriptor_var = var_info.descriptor_var.as_deref().unwrap_or("_descriptor");
        let static_prefix = if member.is_static { "static " } else { "" };
        match member.kind {
            MemberKind::Method => {
                out.push_str(&format!(
                    "{indent}{static_prefix}get {member_name}() {{ return {descriptor_var}.value; }}\n"
                ));
            }
            MemberKind::Getter => {
                out.push_str(&format!(
                    "{indent}{static_prefix}get {member_name}() {{ return {descriptor_var}.get.call(this); }}\n"
                ));
            }
            MemberKind::Setter => {
                let params = self.private_member_parameter_list(member);
                let param = params.split(',').next().map(str::trim).unwrap_or("value");
                let param = if param.is_empty() { "value" } else { param };
                out.push_str(&format!(
                    "{indent}{static_prefix}set {member_name}({param}) {{ return {descriptor_var}.set.call(this, {param}); }}\n"
                ));
            }
            MemberKind::Field | MemberKind::Accessor => {}
        }
    }

    pub(super) fn private_method_function_expr(&self, member: &DecoratedMember) -> String {
        let Some(member_node) = self.arena.get(member.member_idx) else {
            return "function () { }".to_string();
        };
        let Some(method) = self.arena.get_method_decl(member_node) else {
            return "function () { }".to_string();
        };
        let async_prefix = if self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword)
        {
            "async "
        } else {
            ""
        };
        let star = if method.asterisk_token { "*" } else { "" };
        let params = self.parameter_list_text(&method.parameters);
        let body = self.function_body_text(method.body);
        format!("{async_prefix}function{star} ({params}) {body}")
    }

    pub(super) fn private_getter_function_expr(&self, member: &DecoratedMember) -> String {
        let Some(member_node) = self.arena.get(member.member_idx) else {
            return "function () { }".to_string();
        };
        let Some(accessor) = self.arena.get_accessor(member_node) else {
            return "function () { }".to_string();
        };
        let body = self.function_body_text(accessor.body);
        format!("function () {body}")
    }

    pub(super) fn private_setter_function_expr(&self, member: &DecoratedMember) -> String {
        let params = self.private_member_parameter_list(member);
        let Some(member_node) = self.arena.get(member.member_idx) else {
            return format!("function ({params}) {{ }}");
        };
        let Some(accessor) = self.arena.get_accessor(member_node) else {
            return format!("function ({params}) {{ }}");
        };
        let body = self.function_body_text(accessor.body);
        format!("function ({params}) {body}")
    }

    pub(super) fn private_member_parameter_list(&self, member: &DecoratedMember) -> String {
        let Some(member_node) = self.arena.get(member.member_idx) else {
            return "value".to_string();
        };
        if let Some(method) = self.arena.get_method_decl(member_node) {
            return self.parameter_list_text(&method.parameters);
        }
        if let Some(accessor) = self.arena.get_accessor(member_node) {
            return self.parameter_list_text(&accessor.parameters);
        }
        "value".to_string()
    }

    pub(super) fn parameter_list_text(&self, parameters: &NodeList) -> String {
        parameters
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = self.arena.get(param_idx)?;
                let param_data = self.arena.get_parameter(param_node)?;
                let name_text = self.node_text(param_data.name);
                let param_text = if param_data.initializer != NodeIndex::NONE {
                    let init_text = self.node_text(param_data.initializer);
                    format!("{name_text} = {init_text}")
                } else if param_data.dot_dot_dot_token {
                    format!("...{name_text}")
                } else {
                    name_text
                };
                Some(param_text)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(super) fn function_body_text(&self, body_idx: NodeIndex) -> String {
        if body_idx == NodeIndex::NONE {
            return "{ }".to_string();
        }
        if let Some(body) = self.function_body_texts.get(&body_idx) {
            body.clone()
        } else if let Some(body) = self.block_text_from_source(body_idx) {
            body
        } else {
            "{ }".to_string()
        }
    }

    pub(super) fn block_text_from_source(&self, body_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(body_idx)?;
        let source = self.source_text?;
        let start = node.pos as usize;
        let rest = source.get(start..)?;
        let open_rel = rest.find('{')?;
        let open = start + open_rel;
        let mut depth = 0usize;
        let mut chars = source[open..].char_indices().peekable();
        while let Some((rel_idx, ch)) = chars.next() {
            match ch {
                '"' | '\'' => {
                    let quote = ch;
                    while let Some((_, next)) = chars.next() {
                        if next == '\\' {
                            chars.next();
                        } else if next == quote {
                            break;
                        }
                    }
                }
                '`' => {
                    while let Some((_, next)) = chars.next() {
                        if next == '\\' {
                            chars.next();
                        } else if next == '`' {
                            break;
                        }
                    }
                }
                '/' if chars.peek().is_some_and(|(_, next)| *next == '/') => {
                    chars.next();
                    for (_, next) in chars.by_ref() {
                        if next == '\n' {
                            break;
                        }
                    }
                }
                '/' if chars.peek().is_some_and(|(_, next)| *next == '*') => {
                    chars.next();
                    let mut prev = '\0';
                    for (_, next) in chars.by_ref() {
                        if prev == '*' && next == '/' {
                            break;
                        }
                        prev = next;
                    }
                }
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let end = open + rel_idx + ch.len_utf8();
                        return source.get(open..end).map(|body| {
                            if body == "{}" {
                                "{ }".to_string()
                            } else {
                                body.to_string()
                            }
                        });
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(super) fn private_member_name(&self, member: &DecoratedMember) -> Option<String> {
        match &member.name {
            MemberName::Private(name) => Some(name.clone()),
            _ => None,
        }
    }

    pub(super) fn private_auto_accessor_storage_name(&self, member: &DecoratedMember) -> String {
        match &member.name {
            MemberName::Private(name) => {
                format!("#{}_accessor_storage", name.trim_start_matches('#'))
            }
            _ => "#accessor_storage".to_string(),
        }
    }

    pub(super) fn member_name_for_context(
        &self,
        member: &DecoratedMember,
        computed_key_vars: &[(usize, String)],
        member_index: usize,
    ) -> String {
        match &member.name {
            MemberName::Identifier(name)
            | MemberName::StringLiteral(name)
            | MemberName::Private(name) => format!("\"{name}\""),
            MemberName::Computed(_) => computed_key_vars
                .iter()
                .find(|(i, _)| *i == member_index)
                .map(|(_, var)| var.clone())
                .unwrap_or_else(|| "undefined".to_string()),
        }
    }

    pub(super) fn member_access_for_context(
        &self,
        member: &DecoratedMember,
        member_ctx: &EsDecorateMemberCtx<'_>,
    ) -> String {
        let EsDecorateMemberCtx {
            member_index,
            class_alias: _,
            class_private_ref: class_alias,
            class_name,
            computed_key_vars,
            class_decorator_static_private_methods,
            class_decorator_auto_accessor_infos,
            class_decorator_static_private_fields,
        } = member_ctx;
        if member.is_static && member.is_private {
            let get_helper = self.helper("__classPrivateFieldGet");
            let set_helper = self.helper("__classPrivateFieldSet");
            let has_helper = self.helper("__classPrivateFieldIn");
            if let Some(info) = class_decorator_static_private_fields
                .iter()
                .find(|info| info.member_idx == member.member_idx)
            {
                return format!(
                    "has: obj => {has_helper}({class_alias}, obj), get: obj => {get_helper}(obj, {class_alias}, \"f\", {}), set: (obj, value) => {{ {set_helper}(obj, {class_alias}, value, \"f\", {}); }}",
                    info.storage_name, info.storage_name
                );
            }
            if let Some(info) = class_decorator_auto_accessor_infos
                .iter()
                .find(|info| info.member.member_idx == member.member_idx)
            {
                let (Some(get_temp), Some(set_temp)) = (
                    info.getter_temp_var.as_deref(),
                    info.setter_temp_var.as_deref(),
                ) else {
                    return String::new();
                };
                return format!(
                    "has: obj => {has_helper}({class_alias}, obj), get: obj => {get_helper}(obj, {class_alias}, \"a\", {get_temp}), set: (obj, value) => {{ {set_helper}(obj, {class_alias}, value, \"a\", {set_temp}); }}"
                );
            }
            for info in class_decorator_static_private_methods
                .iter()
                .filter(|info| info.member_idx == member.member_idx)
            {
                match info.kind {
                    MemberKind::Method | MemberKind::Getter => {
                        return format!(
                            "has: obj => {has_helper}({class_alias}, obj), get: obj => {get_helper}(obj, {class_alias}, \"a\", {})",
                            info.temp_var
                        );
                    }
                    MemberKind::Setter => {
                        return format!(
                            "has: obj => {has_helper}({class_alias}, obj), set: (obj, value) => {{ {set_helper}(obj, {class_alias}, value, \"a\", {}); }}",
                            info.temp_var
                        );
                    }
                    MemberKind::Field | MemberKind::Accessor => {}
                }
            }
        }
        if !self.use_static_blocks
            && member.is_static
            && member.is_private
            && member.kind == MemberKind::Field
        {
            let storage_name = self.static_private_field_storage_name(class_name, member, "");
            let get_helper = self.helper("__classPrivateFieldGet");
            let set_helper = self.helper("__classPrivateFieldSet");
            let has_helper = self.helper("__classPrivateFieldIn");
            return format!(
                "has: obj => {has_helper}({class_alias}, obj), get: obj => {get_helper}(obj, {class_alias}, \"f\", {storage_name}), set: (obj, value) => {{ {set_helper}(obj, {class_alias}, value, \"f\", {storage_name}); }}"
            );
        }
        if !self.use_static_blocks
            && !member.is_static
            && member.is_private
            && member.kind == MemberKind::Field
        {
            let storage_name = self.private_field_storage_name(class_name, member);
            let get_helper = self.helper("__classPrivateFieldGet");
            let set_helper = self.helper("__classPrivateFieldSet");
            let has_helper = self.helper("__classPrivateFieldIn");
            return format!(
                "has: obj => {has_helper}({storage_name}, obj), get: obj => {get_helper}(obj, {storage_name}, \"f\"), set: (obj, value) => {{ {set_helper}(obj, {storage_name}, value, \"f\"); }}"
            );
        }
        if self.needs_es2015_private_descriptor(member) {
            let get_helper = self.helper("__classPrivateFieldGet");
            let set_helper = self.helper("__classPrivateFieldSet");
            let has_helper = self.helper("__classPrivateFieldIn");
            let private_state = if member.is_static {
                class_alias.to_string()
            } else {
                self.instance_private_brand_name(class_name)
            };
            return match member.kind {
                MemberKind::Method | MemberKind::Getter => {
                    let access_temp =
                        self.private_decorated_member_get_temp_name(class_name, member);
                    format!(
                        "has: obj => {has_helper}({private_state}, obj), get: obj => {get_helper}(obj, {private_state}, \"a\", {access_temp})"
                    )
                }
                MemberKind::Setter => {
                    let access_temp =
                        self.private_decorated_member_set_temp_name(class_name, member);
                    format!(
                        "has: obj => {has_helper}({private_state}, obj), set: (obj, value) => {{ {set_helper}(obj, {private_state}, value, \"a\", {access_temp}); }}"
                    )
                }
                MemberKind::Accessor => {
                    let get_temp = self.private_decorated_member_get_temp_name(class_name, member);
                    let set_temp = self.private_decorated_member_set_temp_name(class_name, member);
                    format!(
                        "has: obj => {has_helper}({private_state}, obj), get: obj => {get_helper}(obj, {private_state}, \"a\", {get_temp}), set: (obj, value) => {{ {set_helper}(obj, {private_state}, value, \"a\", {set_temp}); }}"
                    )
                }
                MemberKind::Field => unreachable!(),
            };
        }

        let key_expr = match &member.name {
            MemberName::Identifier(name) | MemberName::StringLiteral(name) => {
                format!("\"{name}\"")
            }
            MemberName::Private(name) => name.clone(),
            MemberName::Computed(_) => computed_key_vars
                .iter()
                .find(|(i, _)| *i == *member_index)
                .map(|(_, var)| var.clone())
                .unwrap_or_else(|| "undefined".to_string()),
        };

        // Private fields use dot notation (obj.#field), same as regular identifiers
        let prop_access = match &member.name {
            MemberName::Identifier(name) | MemberName::Private(name) => format!("obj.{name}"),
            _ => format!("obj[{key_expr}]"),
        };

        let has_in = format!("{key_expr} in obj");

        match member.kind {
            MemberKind::Method | MemberKind::Getter => {
                format!("has: obj => {has_in}, get: obj => {prop_access}")
            }
            MemberKind::Setter => {
                format!("has: obj => {has_in}, set: (obj, value) => {{ {prop_access} = value; }}")
            }
            MemberKind::Field | MemberKind::Accessor => {
                format!(
                    "has: obj => {has_in}, get: obj => {prop_access}, set: (obj, value) => {{ {prop_access} = value; }}"
                )
            }
        }
    }
}
