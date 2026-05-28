//! Collection and analysis helpers for TC39 decorator emission.

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
    pub(super) fn collect_class_decorator_exprs(
        &self,
        modifiers: &Option<NodeList>,
        receiver_state: &mut DecoratorReceiverState<'_>,
    ) -> Vec<String> {
        let Some(mods) = modifiers else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for &idx in &mods.nodes {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::DECORATOR
                && let Some(dec) = self.arena.get_decorator(node)
            {
                result.push(self.render_decorator_expression(dec.expression, receiver_state));
            }
        }
        result
    }

    pub(super) fn source_order_decorator_assignment_members(
        &self,
        members: &NodeList,
    ) -> std::collections::HashSet<NodeIndex> {
        let mut result = std::collections::HashSet::new();
        let mut pending_decorated_members: Vec<NodeIndex> = Vec::new();

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if self.class_member_name_is_computed(member_node)
                && !pending_decorated_members.is_empty()
            {
                result.extend(pending_decorated_members.drain(..));
            }
            if self.class_member_has_runtime_decorator(member_node)
                && !self.class_member_name_is_computed(member_node)
            {
                pending_decorated_members.push(member_idx);
            }
        }

        result
    }

    pub(super) fn class_member_has_runtime_decorator(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let modifiers = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(member_node)
                .and_then(|method| method.modifiers.as_ref()),
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .arena
                .get_property_decl(member_node)
                .and_then(|prop| prop.modifiers.as_ref()),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .arena
                .get_accessor(member_node)
                .and_then(|accessor| accessor.modifiers.as_ref()),
            _ => None,
        };
        let Some(modifiers) = modifiers else {
            return false;
        };
        if self
            .arena
            .has_modifier(&Some(modifiers.clone()), SyntaxKind::AbstractKeyword)
            || self
                .arena
                .has_modifier(&Some(modifiers.clone()), SyntaxKind::DeclareKeyword)
        {
            return false;
        }
        modifiers.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
        })
    }

    pub(super) fn class_member_name_is_computed(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let name = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(member_node)
                .map(|method| method.name),
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .arena
                .get_property_decl(member_node)
                .map(|prop| prop.name),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .arena
                .get_accessor(member_node)
                .map(|accessor| accessor.name),
            _ => None,
        };
        name.and_then(|name| self.arena.get(name))
            .is_some_and(|name_node| name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
    }

    pub(super) fn collect_decorated_members(
        &self,
        members: &NodeList,
        receiver_state: &mut DecoratorReceiverState<'_>,
        source_order_decorator_members: &std::collections::HashSet<NodeIndex>,
    ) -> Vec<DecoratedMember> {
        let mut result = Vec::new();

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let (modifiers, name_idx, kind) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (method.modifiers.clone(), method.name, MemberKind::Method)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let kind = if self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                    {
                        MemberKind::Accessor
                    } else {
                        MemberKind::Field
                    };
                    (prop.modifiers.clone(), prop.name, kind)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    let Some(acc) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (acc.modifiers.clone(), acc.name, MemberKind::Getter)
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(acc) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (acc.modifiers.clone(), acc.name, MemberKind::Setter)
                }
                _ => continue,
            };

            // Abstract and declare members have no runtime representation — skip them entirely.
            // tsc strips abstract/ambient decorated members from the decorator transform output.
            if self
                .arena
                .has_modifier(&modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }

            // Collect decorator expressions
            let mut decorator_exprs = Vec::new();
            let mut captured_decorator_exprs = Vec::new();
            if let Some(ref mods) = modifiers {
                for &mod_idx in &mods.nodes {
                    let Some(mod_node) = self.arena.get(mod_idx) else {
                        continue;
                    };
                    if mod_node.kind == syntax_kind_ext::DECORATOR
                        && let Some(dec) = self.arena.get_decorator(mod_node)
                    {
                        let decorator_expr =
                            self.render_decorator_expression(dec.expression, receiver_state);
                        let captured_decorator_expr =
                            if source_order_decorator_members.contains(&member_idx) {
                                decorator_expr.clone()
                            } else if self
                                .decorator_expression_texts
                                .contains_key(&dec.expression)
                            {
                                self.render_captured_decorator_expression(
                                    dec.expression,
                                    receiver_state,
                                )
                            } else {
                                decorator_expr.clone()
                            };
                        decorator_exprs.push(decorator_expr);
                        captured_decorator_exprs.push(captured_decorator_expr);
                    }
                }
            }
            if decorator_exprs.is_empty() {
                continue;
            }

            let is_static = self.arena.is_static(&modifiers);
            let (name, is_private) = self.resolve_member_name(name_idx);

            result.push(DecoratedMember {
                member_idx,
                kind,
                name,
                is_static,
                is_private,
                decorator_exprs,
                captured_decorator_exprs,
            });
        }

        result
    }

    pub(super) fn render_decorator_expression(
        &self,
        expr_idx: NodeIndex,
        receiver_state: &mut DecoratorReceiverState<'_>,
    ) -> String {
        let (paren_depth, inner_idx) = self.strip_parenthesized_decorator_expr(expr_idx);
        let Some(inner_node) = self.arena.get(inner_idx) else {
            return normalize_decorator_expr_text(&self.node_text(expr_idx));
        };
        let access_kind = inner_node.kind;
        let Some(access) = self.arena.get_access_expr(inner_node) else {
            return normalize_decorator_expr_text(&self.node_text(expr_idx));
        };
        if access_kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && access_kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return normalize_decorator_expr_text(&self.node_text(expr_idx));
        }

        let rendered = if self.is_super_expression(access.expression) {
            *receiver_state.needs_outer_this_capture = true;
            match access_kind {
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    let property = self.property_access_name_text(access.name_or_argument);
                    format!("super.{property}.bind({})", receiver_state.outer_this_var)
                }
                _ => {
                    let argument = self.node_text(access.name_or_argument);
                    format!("super[{argument}].bind({})", receiver_state.outer_this_var)
                }
            }
        } else {
            let receiver_temp = next_temp_var(receiver_state.temp_counter);
            receiver_state
                .receiver_temp_vars
                .push(receiver_temp.clone());
            let receiver = self.node_text(access.expression);
            match access_kind {
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    let property = self.property_access_name_text(access.name_or_argument);
                    format!("({receiver_temp} = {receiver}).{property}.bind({receiver_temp})")
                }
                _ => {
                    let argument = self.node_text(access.name_or_argument);
                    format!("({receiver_temp} = {receiver})[{argument}].bind({receiver_temp})")
                }
            }
        };
        self.wrap_decorator_expression_parens(rendered, paren_depth)
    }

    pub(super) fn render_captured_decorator_expression(
        &self,
        expr_idx: NodeIndex,
        receiver_state: &mut DecoratorReceiverState<'_>,
    ) -> String {
        if let Some(text) = self.decorator_expression_texts.get(&expr_idx) {
            if text.contains(receiver_state.outer_this_var) {
                *receiver_state.needs_outer_this_capture = true;
            }
            return normalize_decorator_expr_text(text);
        }
        self.render_decorator_expression(expr_idx, receiver_state)
    }

    pub(super) fn strip_parenthesized_decorator_expr(
        &self,
        mut idx: NodeIndex,
    ) -> (usize, NodeIndex) {
        let mut depth = 0usize;
        loop {
            let Some(node) = self.arena.get(idx) else {
                return (depth, idx);
            };
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return (depth, idx);
            }
            let Some(paren) = self.arena.get_parenthesized(node) else {
                return (depth, idx);
            };
            depth += 1;
            idx = paren.expression;
        }
    }

    pub(super) fn wrap_decorator_expression_parens(
        &self,
        mut text: String,
        depth: usize,
    ) -> String {
        for _ in 0..depth {
            text = format!("({text})");
        }
        normalize_decorator_expr_text(&text)
    }

    pub(super) fn property_access_name_text(&self, idx: NodeIndex) -> String {
        self.get_identifier_text(idx)
            .unwrap_or_else(|| self.node_text(idx))
    }

    pub(super) fn is_super_expression(&self, idx: NodeIndex) -> bool {
        self.arena
            .get(idx)
            .is_some_and(|node| node.kind == SyntaxKind::SuperKeyword as u16)
    }

    pub(super) fn resolve_member_name(&self, name_idx: NodeIndex) -> (MemberName, bool) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return (MemberName::Identifier(String::new()), false);
        };

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let text = self
                    .arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone())
                    .unwrap_or_default();
                (MemberName::Identifier(text), false)
            }
            k if k == SyntaxKind::PrivateIdentifier as u16 => {
                let text = self
                    .arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone())
                    .unwrap_or_default();
                (MemberName::Private(text), true)
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                let Some(computed) = self.arena.get_computed_property(name_node) else {
                    return (MemberName::Identifier(String::new()), false);
                };
                // Check if computed expression is a string literal
                if let Some(expr_node) = self.arena.get(computed.expression)
                    && expr_node.kind == SyntaxKind::StringLiteral as u16
                    && let Some(lit) = self.arena.get_literal(expr_node)
                {
                    return (MemberName::StringLiteral(lit.text.clone()), false);
                }
                (MemberName::Computed(computed.expression), false)
            }
            _ => (MemberName::Identifier(String::new()), false),
        }
    }

    pub(super) fn has_extends_clause(&self, heritage: &Option<NodeList>) -> bool {
        let Some(clauses) = heritage else {
            return false;
        };
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            if let Some(h) = self.arena.get_heritage_clause(clause_node)
                && h.token == SyntaxKind::ExtendsKeyword as u16
            {
                return true;
            }
        }
        false
    }

    pub(super) fn get_extends_text(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Option<String> {
        if let Some(text) = self.extends_text.as_ref() {
            return Some(text.clone());
        }
        let clauses = class_data.heritage_clauses.as_ref()?;
        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage = self.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let first_type = heritage.types.nodes.first()?;
            let type_node = self.arena.get(*first_type)?;
            if let Some(expr_data) = self.arena.get_expr_type_args(type_node) {
                return Some(self.node_text(expr_data.expression));
            }
            return Some(self.node_text(*first_type));
        }
        None
    }

    pub(super) fn compute_all_member_vars(
        &self,
        members: &[DecoratedMember],
    ) -> Vec<MemberVarInfo> {
        let mut counter: u32 = 0;
        // Track the last seen computed/string member name to group getter/setter pairs.
        // tsc only increments the suffix counter between different member names.
        let mut last_computed_name: Option<String> = None;
        members
            .iter()
            .map(|m| self.compute_member_var_info(m, &mut counter, &mut last_computed_name))
            .collect()
    }

    pub(super) fn member_var_declaration_order(&self, members: &[DecoratedMember]) -> Vec<usize> {
        let mut order: Vec<usize> = (0..members.len()).collect();
        order.sort_by_key(|&idx| (!members[idx].is_static, idx));
        order
    }

    pub(super) fn decorator_application_order(&self, members: &[DecoratedMember]) -> Vec<usize> {
        let mut order: Vec<usize> = (0..members.len()).collect();
        order.sort_by_key(|&idx| {
            let member = &members[idx];
            let field_bucket = matches!(member.kind, MemberKind::Field);
            (field_bucket, !member.is_static, idx)
        });
        order
    }

    pub(super) fn compute_member_var_info(
        &self,
        member: &DecoratedMember,
        counter: &mut u32,
        last_computed_name: &mut Option<String>,
    ) -> MemberVarInfo {
        let prefix = if member.is_static { "static_" } else { "" };
        let (kind_prefix, base_name) = match &member.name {
            MemberName::Private(name) => {
                let private_name = name.trim_start_matches('#');
                let base_name = match member.kind {
                    MemberKind::Getter => format!("private_get_{private_name}"),
                    MemberKind::Setter => format!("private_set_{private_name}"),
                    _ => format!("private_{private_name}"),
                };
                ("", base_name)
            }
            MemberName::Identifier(name) => {
                let kind_prefix = match member.kind {
                    MemberKind::Getter => "get_",
                    MemberKind::Setter => "set_",
                    _ => "",
                };
                (kind_prefix, name.clone())
            }
            MemberName::StringLiteral(_) | MemberName::Computed(_) => {
                let kind_prefix = match member.kind {
                    MemberKind::Getter => "get_",
                    MemberKind::Setter => "set_",
                    _ => "",
                };
                (kind_prefix, "member".to_string())
            }
        };

        let var_base = format!("_{prefix}{kind_prefix}{base_name}");

        // For computed/string members, only increment counter on NEW member names.
        // Getter/setter pairs with the same name share the same suffix.
        let is_computed_or_string = matches!(
            member.name,
            MemberName::StringLiteral(_) | MemberName::Computed(_)
        );

        if is_computed_or_string {
            let current_name = match &member.name {
                MemberName::StringLiteral(s) => s.clone(),
                MemberName::Computed(idx) => self.node_text(*idx),
                _ => unreachable!(),
            };
            let is_new_name = last_computed_name
                .as_ref()
                .is_none_or(|prev| *prev != current_name);
            if is_new_name {
                if last_computed_name.is_some() {
                    *counter += 1;
                }
                *last_computed_name = Some(current_name);
            }
        }

        let suffix = if *counter > 0 && is_computed_or_string {
            format!("_{}", *counter)
        } else {
            String::new()
        };

        let decorators_var = format!("{var_base}_decorators{suffix}");
        let has_field_inits = matches!(member.kind, MemberKind::Field | MemberKind::Accessor);
        let has_descriptor = member.is_private
            && matches!(
                member.kind,
                MemberKind::Method | MemberKind::Getter | MemberKind::Setter | MemberKind::Accessor
            )
            && (self.use_static_blocks || self.needs_es2015_private_descriptor(member));

        MemberVarInfo {
            decorators_var,
            has_initializers: has_field_inits,
            initializers_var: if has_field_inits {
                Some(format!("{var_base}_initializers{suffix}"))
            } else {
                None
            },
            extra_initializers_var: if has_field_inits {
                Some(format!("{var_base}_extraInitializers{suffix}"))
            } else {
                None
            },
            has_descriptor,
            descriptor_var: if has_descriptor {
                Some(format!("{var_base}_descriptor{suffix}"))
            } else {
                None
            },
        }
    }
}
