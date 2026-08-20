//! Source-type display for an object literal in an assignment diagnostic.
//!
//! Renders the source side of a `TS2322`/`TS2345` from the object-literal
//! syntax rather than from the finalized literal type, so a fresh literal can
//! keep the per-property literal types and contextual shapes `tsc` shows.
//! Because it walks syntax, it must reproduce the property-table semantics the
//! literal type gets for free — see [`push_object_literal_display_member`].

use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Record one rendered object-literal member, applying the same property-table
/// semantics the object-literal type itself uses: a repeated key keeps the
/// *first* declaration's position and takes the *last* declaration's rendering.
/// `{ a: 1, b: "s", a: true }` therefore displays as `{ a: boolean; b: string; }`,
/// matching `tsc`.
///
/// A member whose key does not resolve to a static name (an unresolved computed
/// key) cannot collide with another member in the property table, so it is
/// always appended.
fn push_object_literal_display_member(
    parts: &mut Vec<String>,
    slots: &mut rustc_hash::FxHashMap<tsz_common::Atom, usize>,
    name: Option<tsz_common::Atom>,
    rendered: String,
) {
    if let Some(name) = name {
        if let Some(&slot) = slots.get(&name) {
            parts[slot] = rendered;
            return;
        }
        slots.insert(name, parts.len());
    }
    parts.push(rendered);
}

impl<'a> CheckerState<'a> {
    pub(crate) fn object_literal_source_type_display(
        &mut self,
        expr_idx: NodeIndex,
        target: Option<TypeId>,
    ) -> Option<String> {
        // Only skip parentheses, not type assertions.  When the source is
        // `<foo>({})`, the diagnostic should display the asserted type name
        // `foo`, not the inner object literal `{}`.  Returning `None` here
        // lets the caller fall through to `get_type_of_node` which yields
        // the asserted type.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == syntax_kind_ext::RETURN_STATEMENT
            && let Some(return_stmt) = self.ctx.arena.get_return_statement(node)
            && return_stmt.expression.is_some()
        {
            return self.object_literal_source_type_display(return_stmt.expression, target);
        }
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        let target = target.map(|target| self.evaluate_type_for_assignability(target));
        if let Some(display) =
            self.computed_index_signature_object_literal_source_display(expr_idx, target)
        {
            return Some(display);
        }
        let preserve_literal_source_for_normalized_union =
            target.is_some_and(|target| self.target_is_normalized_object_literal_union(target));
        let target_shape = target.and_then(|target| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
        });
        let mut parts = Vec::new();
        // Slot of the first member written under each resolved property name, so a
        // repeated key overwrites in place instead of rendering twice (see
        // `push_object_literal_display_member`).
        let mut member_slots: rustc_hash::FxHashMap<tsz_common::Atom, usize> =
            rustc_hash::FxHashMap::default();
        // Whether every property so far is a wide (non-literal) `string`/
        // `number` computed key of ONE consistent kind that folds into the
        // target's index signature — and, among those, whether at least one
        // is NOT a re-spellable entity-name reference. `tsc` merges the whole
        // homogeneous group into one synthesized `[x: kind]: V` clause
        // (`contextual_index_signature_source_display`, below) as soon as any
        // single member can't be re-spelled from its own syntax; when every
        // member CAN be (a plain identifier or dotted `a.b.c` chain), it shows
        // each individually instead, unmerged. #16721.
        let mut contextual_index_key_kind: Option<&'static str> = None;
        let mut contextual_index_value_types = Vec::new();
        let mut all_contextual_index_properties = !literal.elements.nodes.is_empty();
        let mut any_non_entity_wide_key = false;
        for child_idx in literal.elements.nodes.iter().copied() {
            let child = self.ctx.arena.get(child_idx)?;
            if matches!(
                child.kind,
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
            ) {
                // A computed-name method/accessor that folded into an
                // index-signature bucket displays property-style from the type
                // captured at computation time (`[ws]: () => number`, a getter
                // by its return type, a setter by its parameter type), like a
                // computed-key property assignment. Any other method/accessor
                // keeps the structural fallback for the whole literal.
                let member =
                    self.computed_index_member_source_display(child_idx, target_shape.as_deref())?;
                match (contextual_index_key_kind, member.computed_index_kind) {
                    (None, Some(kind)) => contextual_index_key_kind = Some(kind),
                    (Some(existing), Some(kind)) if existing == kind => {}
                    _ => all_contextual_index_properties = false,
                }
                if member.computed_index_kind.is_some() && !member.key_is_entity_name {
                    any_non_entity_wide_key = true;
                }
                if member.computed_index_kind.is_some() {
                    contextual_index_value_types.push(member.widened_value);
                }
                // A wide computed key never resolves to a static property
                // name, so it cannot collide in the property table — always
                // appended.
                parts.push(member.rendered);
                continue;
            }
            let (name_idx, value_idx) = if child.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_property_assignment(child)?;
                (prop.name, prop.initializer)
            } else if child.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_shorthand_property(child)?;
                (prop.name, prop.name)
            } else {
                return None;
            };
            let name_node = self.ctx.arena.get(name_idx)?;
            let display_name = match name_node.kind {
                k if k == tsz_scanner::SyntaxKind::Identifier as u16 => self
                    .ctx
                    .arena
                    .get_identifier(name_node)?
                    .escaped_text
                    .to_string(),
                k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                    || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    let lit = self.ctx.arena.get_literal(name_node)?;
                    format!("\"{}\"", lit.text)
                }
                k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                    self.ctx.arena.get_literal(name_node)?.text.clone()
                }
                k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                    if let Some(name) = self.get_member_name_display_text(name_idx) {
                        name
                    } else {
                        let computed = self.ctx.arena.get_computed_property(name_node)?;
                        let expr = self.node_text(computed.expression)?;
                        format!("[{expr}]", expr = expr.trim())
                    }
                }
                _ => return None,
            };
            let computed_index_kind =
                self.contextual_computed_index_key_kind(name_idx, target_shape.as_deref());
            match (contextual_index_key_kind, computed_index_kind) {
                (None, Some(kind)) => contextual_index_key_kind = Some(kind),
                (Some(existing), Some(kind)) if existing == kind => {}
                _ => all_contextual_index_properties = false,
            }
            if computed_index_kind.is_some()
                && !self.computed_key_is_entity_name_reference(name_idx)
            {
                any_non_entity_wide_key = true;
            }
            let property_name_text = self.get_property_name(name_idx);
            let property_name = property_name_text
                .as_deref()
                .map(|name| self.ctx.types.intern_string(name));
            if self
                .ctx
                .arena
                .get(value_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16)
            {
                push_object_literal_display_member(
                    &mut parts,
                    &mut member_slots,
                    property_name,
                    format!("{display_name}: this"),
                );
                continue;
            }
            let value_type = self.get_type_of_node(value_idx);
            if value_type == TypeId::ERROR {
                return None;
            }

            // tsc preserves a fresh literal property only when the contextual
            // (target) property type carries a literal of the *same* primitive
            // base (mirroring `getWidenedLiteralLikeTypeForContextualType`); the
            // base must match so a numeric source against a string-literal target
            // still widens. The former check recognized string literals only, so
            // numeric/boolean/bigint properties were wrongly widened.
            let source_literal_base =
                diagnostic_query::widen_literal_to_primitive(self.ctx.types, value_type);
            let target_accepts_literal = property_name
                .and_then(|name| {
                    // First try the direct object shape
                    if let Some(shape) = target_shape.as_ref() {
                        return shape
                            .properties
                            .iter()
                            .find(|p| p.name == name)
                            .filter(|p| {
                                self.type_contains_literal_of_primitive_base(
                                    p.type_id,
                                    source_literal_base,
                                )
                            })
                            .map(|p| p.type_id);
                    }
                    // For union targets, check each member's properties. The
                    // per-member gate already enforces the base match, so the
                    // returned type needs no re-check below.
                    let target = target?;
                    let members = diagnostic_query::union_members(self.ctx.types, target)?;
                    for member in &members {
                        if let Some(member_shape) =
                            crate::query_boundaries::common::object_shape_for_type(
                                self.ctx.types,
                                *member,
                            )
                            && let Some(prop) =
                                member_shape.properties.iter().find(|p| p.name == name)
                            && self.type_contains_literal_of_primitive_base(
                                prop.type_id,
                                source_literal_base,
                            )
                        {
                            return Some(prop.type_id);
                        }
                    }
                    None
                })
                .is_some();
            if let Some(literal_display) = self.literal_expression_display(value_idx) {
                let preserve_normalized_union_boolean = preserve_literal_source_for_normalized_union
                    && matches!(literal_display.as_str(), "true" | "false");
                if target_accepts_literal || preserve_normalized_union_boolean {
                    push_object_literal_display_member(
                        &mut parts,
                        &mut member_slots,
                        property_name,
                        format!("{display_name}: {literal_display}"),
                    );
                    continue;
                }
            }

            // For nested object literals, recurse with the target's own
            // per-property type (the `getBestMatchIndexedAccessTypeOrUndefined`
            // derivation) as the nested contextual target, so a nested literal
            // whose contextual property type carries a same-base literal is
            // preserved exactly like a top-level one (#17782). With no target
            // property the nested render widens as before.
            let nested_target = match (target, property_name_text.as_deref()) {
                (Some(target_type), Some(prop_name))
                    if self
                        .ctx
                        .arena
                        .get(self.ctx.arena.skip_parenthesized(value_idx))
                        .is_some_and(|node| {
                            node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        }) =>
                {
                    self.object_literal_target_property_type(target_type, name_idx, prop_name)
                        .map(|(_check_type, display_type)| display_type)
                }
                _ => None,
            };
            if let Some(nested_display) =
                self.object_literal_source_type_display(value_idx, nested_target)
            {
                push_object_literal_display_member(
                    &mut parts,
                    &mut member_slots,
                    property_name,
                    format!("{display_name}: {nested_display}"),
                );
                continue;
            }

            // Fall back to type system for non-literal expressions.
            // For function properties, merge parameter types from target shape.
            let value_display_type = property_name
                .and_then(|name| {
                    let shape = target_shape.as_ref()?;
                    shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == name)
                        .map(|prop| prop.type_id)
                })
                .filter(|target_prop_type| {
                    crate::query_boundaries::diagnostics::function_shape(self.ctx.types, value_type)
                        .is_some()
                        && crate::query_boundaries::diagnostics::function_shape(
                            self.ctx.types,
                            *target_prop_type,
                        )
                        .is_some()
                })
                .and_then(|target_prop_type| {
                    let value_shape = crate::query_boundaries::diagnostics::function_shape(
                        self.ctx.types,
                        value_type,
                    )?;
                    let target_shape = crate::query_boundaries::diagnostics::function_shape(
                        self.ctx.types,
                        target_prop_type,
                    )?;
                    let merged_params: Vec<_> = value_shape
                        .params
                        .iter()
                        .zip(target_shape.params.iter())
                        .map(|(value_param, target_param)| {
                            diagnostic_query::display_param_with_type(
                                value_param,
                                target_param.type_id,
                            )
                        })
                        .collect();
                    let merged = diagnostic_query::function_type_with_params_replaced(
                        self.ctx.types,
                        &value_shape,
                        merged_params,
                    );
                    Some(merged)
                })
                .unwrap_or(value_type);
            let value_display_type = if target_accepts_literal {
                value_display_type
            } else {
                let widened = self.widen_type_for_display(value_display_type);
                if crate::query_boundaries::common::is_template_literal_type(
                    self.ctx.types,
                    widened,
                ) || crate::query_boundaries::common::is_string_intrinsic_type(
                    self.ctx.types,
                    widened,
                ) {
                    TypeId::STRING
                } else {
                    widened
                }
            };
            let widened_value_display_type =
                self.widen_function_like_display_type(value_display_type);
            let value_display =
                self.format_type_for_assignability_message(widened_value_display_type);
            if computed_index_kind.is_some() {
                contextual_index_value_types.push(widened_value_display_type);
            }
            push_object_literal_display_member(
                &mut parts,
                &mut member_slots,
                property_name,
                format!("{display_name}: {value_display}"),
            );
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        if any_non_entity_wide_key
            && let Some(index_display) = self.contextual_index_signature_source_display(
                all_contextual_index_properties,
                contextual_index_key_kind,
                contextual_index_value_types,
            )
        {
            return Some(index_display);
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }
}
