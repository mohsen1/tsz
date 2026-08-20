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
                // computed-key property assignment.
                if let Some(member) =
                    self.computed_index_member_source_display(child_idx, target_shape.as_deref())
                {
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
                // A method/accessor under a written (non-computed) name
                // renders from the literal's own checked type via the shared
                // printer — method shorthand `f(): number`, `readonly` for a
                // get-only accessor — so a method member no longer forces the
                // sibling property assignments onto the widened structural
                // fallback (`kind: "a"` stayed preserved in tsc's head while
                // tsz widened it to `kind: string`). A computed or otherwise
                // unresolvable name keeps the structural fallback for the
                // whole literal, as before.
                let (member_name, rendered) =
                    self.named_method_member_source_display(child_idx, expr_idx)?;
                all_contextual_index_properties = false;
                push_object_literal_display_member(
                    &mut parts,
                    &mut member_slots,
                    Some(member_name),
                    rendered,
                );
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
            let target_accepts_literal = self.target_property_accepts_same_base_literal(
                property_name,
                source_literal_base,
                target,
                target_shape.as_deref(),
            );
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
            // An array-literal value has neither a `literal_expression_display`
            // nor the object-literal recursion above, so it always reached the
            // widening fallback below and lost its element literals. tsc
            // renders the property with its checked (contextually typed) value
            // — `[1, 2]` against a tuple arm, `1[]` against an array arm, in
            // the head and the missing-property elaboration alike — whenever
            // the target's own per-property type is what typed it; only a
            // value no target property accepted stays widened (a shape tsc
            // anchors at the inner expression as `number[]` anyway). The
            // acceptance test is an exact element-for-element literal match
            // against the target property type, so a mismatched source is
            // never repainted as its target.
            if computed_index_kind.is_none()
                && self
                    .ctx
                    .arena
                    .get(self.ctx.arena.skip_parenthesized(value_idx))
                    .is_some_and(|node| node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
                && let Some(name) = property_name
            {
                let candidate_prop_types: Vec<TypeId> = if let Some(shape) = target_shape.as_ref() {
                    shape
                        .properties
                        .iter()
                        .filter(|p| p.name == name)
                        .map(|p| p.type_id)
                        .collect()
                } else if let Some(members) = target
                    .and_then(|target| diagnostic_query::union_members(self.ctx.types, target))
                {
                    members
                        .iter()
                        .filter_map(|member| {
                            diagnostic_query::object_shape_for_type(self.ctx.types, *member)
                        })
                        .filter_map(|member_shape| {
                            member_shape
                                .properties
                                .iter()
                                .find(|p| p.name == name)
                                .map(|p| p.type_id)
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                if let Some(accepted) = candidate_prop_types.into_iter().find(|&prop_type| {
                    self.array_literal_value_display_matches_target(value_idx, prop_type)
                }) {
                    // The accepted type is the target's own declared property
                    // type: render it directly — `widen_function_like_display_type`
                    // below would still widen its tuple element literals.
                    let rendered = self.format_type_for_assignability_message(accepted);
                    push_object_literal_display_member(
                        &mut parts,
                        &mut member_slots,
                        property_name,
                        format!("{display_name}: {rendered}"),
                    );
                    continue;
                }
            }
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

    /// Whether the contextual target carries a property `property_name` whose
    /// type contains a literal of the source value's primitive base — the
    /// `getWidenedLiteralLikeTypeForContextualType` acceptance test deciding
    /// when a fresh literal property keeps its literal in the display.
    /// A union target accepts through any of its object members; the
    /// per-member gate already enforces the base match.
    fn target_property_accepts_same_base_literal(
        &mut self,
        property_name: Option<tsz_common::Atom>,
        source_literal_base: TypeId,
        target: Option<TypeId>,
        target_shape: Option<&tsz_solver::ObjectShape>,
    ) -> bool {
        let Some(name) = property_name else {
            return false;
        };
        if let Some(shape) = target_shape {
            return shape
                .properties
                .iter()
                .find(|p| p.name == name)
                .is_some_and(|p| {
                    self.type_contains_literal_of_primitive_base(p.type_id, source_literal_base)
                });
        }
        let Some(target) = target else {
            return false;
        };
        let Some(members) = diagnostic_query::union_members(self.ctx.types, target) else {
            return false;
        };
        members.iter().any(|member| {
            diagnostic_query::object_shape_for_type(self.ctx.types, *member)
                .and_then(|member_shape| {
                    member_shape
                        .properties
                        .iter()
                        .find(|p| p.name == name)
                        .map(|p| p.type_id)
                })
                .is_some_and(|prop_type| {
                    self.type_contains_literal_of_primitive_base(prop_type, source_literal_base)
                })
        })
    }

    /// Source display for a method, getter, or setter member declared under a
    /// written (non-computed) property name: the member's rendering in the
    /// object literal's own checked type, produced by the shared printer so
    /// method shorthand (`f(): number`), `readonly` on a get-only accessor,
    /// and name quoting all match the structural formatter exactly. An
    /// accessor's value type follows the same contextual literal-preservation
    /// rule as a property assignment's: it keeps a literal only when the
    /// target's own property accepts a literal of that base, and widens for
    /// display otherwise.
    ///
    /// `None` — sending the caller to the whole-literal structural fallback,
    /// the pre-existing behavior — for a computed or unresolvable name, an
    /// errored literal, or a member the checked type does not carry.
    fn named_method_member_source_display(
        &mut self,
        member_idx: NodeIndex,
        literal_idx: NodeIndex,
    ) -> Option<(tsz_common::Atom, String)> {
        let member_node = self.ctx.arena.get(member_idx)?;
        let name_idx = if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
            method.name
        } else {
            self.ctx.arena.get_accessor(member_node)?.name
        };
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let name_text = self.get_property_name(name_idx)?;
        let member_name = self.ctx.types.intern_string(&name_text);
        let literal_type = self.get_type_of_node(literal_idx);
        if literal_type == TypeId::ERROR {
            return None;
        }
        let literal_type = self.evaluate_type_for_assignability(literal_type);
        let (prop_type, prop_is_method) =
            diagnostic_query::object_shape_for_type(self.ctx.types, literal_type).and_then(
                |shape| {
                    shape
                        .properties
                        .iter()
                        .find(|p| p.name == member_name)
                        .map(|p| (p.type_id, p.is_method))
                },
            )?;
        let display_override = if prop_is_method {
            // A method's function type has no literal to widen.
            None
        } else {
            // tsc infers an accessor member's type by widening the getter's
            // return literal (a mutable location with no freshness): oracled
            // against 7.0.2, `get b() { return true }` beside
            // `set b(v: boolean)` renders `b: boolean`, and a get-only
            // `get n() { return 5; }` against an `n: number` target renders
            // `readonly n: number`. tsz's checked accessor type keeps the raw
            // return literal, so reconstruct the inference-widened type here —
            // the inference widening (boolean literals included), not the
            // boolean-preserving display widening. (The one spelling tsc keeps
            // literal — a setter whose own parameter pins the same-base
            // literal, `set b(v: true)` — diverges upstream in accessor
            // checking before this display path is reached.)
            let widened =
                diagnostic_query::widen_type_preserving_unique_symbols(self.ctx.types, prop_type);
            (widened != prop_type).then_some(widened)
        };
        let _budget_scope = crate::error_reporter::display_budget::DisplayBudgetScope::enter();
        let mut formatter = self.ctx.create_assignability_type_formatter();
        let rendered =
            formatter.format_object_type_property(literal_type, member_name, display_override)?;
        Some((member_name, rendered))
    }

    /// Whether `array_idx` is an array literal whose every element is a
    /// literal expression (nested array literals included) exactly matching
    /// the corresponding element type of the tuple — or the uniform element
    /// type of the array — `target_prop` declares (`readonly` unwrapped).
    ///
    /// This proves the value was typed by that target property (`tsc`'s
    /// contextual typing is what keeps the element literals in its checked
    /// type), so the display may render the target's own property type; any
    /// weaker test would repaint a mismatched source as its target.
    fn array_literal_value_display_matches_target(
        &mut self,
        array_idx: NodeIndex,
        target_prop: TypeId,
    ) -> bool {
        let unwrapped = diagnostic_query::readonly_inner_type(self.ctx.types, target_prop)
            .unwrap_or(target_prop);
        let array_idx = self.ctx.arena.skip_parenthesized(array_idx);
        let elements: Vec<NodeIndex> = match self
            .ctx
            .arena
            .get(array_idx)
            .filter(|node| node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
            .and_then(|node| self.ctx.arena.get_literal_expr(node))
        {
            Some(arr) => arr.elements.nodes.to_vec(),
            None => return false,
        };
        if let Some(tuple_elems) = diagnostic_query::tuple_elements(self.ctx.types, unwrapped) {
            tuple_elems.len() == elements.len()
                && tuple_elems.iter().all(|e| e.is_required())
                && elements
                    .iter()
                    .zip(tuple_elems.iter())
                    .all(|(&el, elem)| self.array_element_display_matches_target(el, elem.type_id))
        } else if let Some(elem_type) =
            diagnostic_query::array_element_type(self.ctx.types, unwrapped)
        {
            // An empty source against an array (non-tuple) target stays on
            // the widening fallback: there is no element evidence that this
            // target typed it.
            !elements.is_empty()
                && elements
                    .iter()
                    .all(|&el| self.array_element_display_matches_target(el, elem_type))
        } else {
            false
        }
    }

    /// One element of [`Self::array_literal_value_display_matches_target`]:
    /// a literal expression whose literal type — derived from its own syntax,
    /// because the cached node type of an element is already widened by the
    /// time diagnostics render — is exactly `target_elem`, or a nested array
    /// literal matching `target_elem` element-for-element.
    fn array_element_display_matches_target(
        &mut self,
        element_idx: NodeIndex,
        target_elem: TypeId,
    ) -> bool {
        let element_idx = self.ctx.arena.skip_parenthesized(element_idx);
        let Some(node) = self.ctx.arena.get(element_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return self.array_literal_value_display_matches_target(element_idx, target_elem);
        }
        self.array_element_literal_type_from_syntax(element_idx) == Some(target_elem)
    }

    /// The literal type a literal element expression spells: string, number
    /// (a `-`/`+`-prefixed numeric literal included), or boolean. Any other
    /// expression returns `None`, keeping the acceptance test conservative.
    fn array_element_literal_type_from_syntax(&mut self, idx: NodeIndex) -> Option<TypeId> {
        use tsz_scanner::SyntaxKind;
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(diagnostic_query::display_string_literal_type(
                    self.ctx.types,
                    &lit.text,
                ))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                let value = lit
                    .value
                    .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))?;
                Some(diagnostic_query::display_number_literal_type(
                    self.ctx.types,
                    value,
                ))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(TypeId::BOOLEAN_TRUE),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(TypeId::BOOLEAN_FALSE),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let negative = unary.operator == SyntaxKind::MinusToken as u16;
                if !negative && unary.operator != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let operand = self.ctx.arena.skip_parenthesized(unary.operand);
                let operand_node = self.ctx.arena.get(operand)?;
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.ctx.arena.get_literal(operand_node)?;
                let value = lit
                    .value
                    .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))?;
                Some(diagnostic_query::display_number_literal_type(
                    self.ctx.types,
                    if negative { -value } else { value },
                ))
            }
            _ => None,
        }
    }
}
