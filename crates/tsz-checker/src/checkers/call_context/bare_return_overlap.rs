use crate::computation::complex::is_contextually_sensitive;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{FunctionShape, TypeId};

impl CheckerState<'_> {
    /// Detects the `#14792` shape: a generic call whose return type mentions a
    /// type parameter `U` — either bare (`(...): U`) or under a covariant wrapper
    /// (`(...): U[]`) — where a context-sensitive callback argument (whose
    /// declared signature mentions `U` in its return position) returns a value
    /// derived entirely from callback parameters that are already pinned by a
    /// concrete (non context-sensitive) sibling argument.
    ///
    /// In that shape the callback's return value is fully determined by the
    /// pinned inputs, so the outer contextual type cannot refine it; `tsc`'s
    /// argument inference for `U` (`InferencePriority` above the contextual
    /// return) wins and the callback body is checked against the inferred `U`.
    /// Suppressing the contextual return reproduces that: `U` is inferred
    /// bottom-up from the literal and any annotation mismatch is reported once,
    /// at the assignment site, instead of also inside the callback body.
    ///
    /// The check is intentionally narrow. Callbacks whose return contains a
    /// fresh leaf the contextual type could refine (a free literal such as
    /// `() => 1` or `(v) => [1, 2]`) are excluded, because there the contextual
    /// return legitimately seeds inference and must not be dropped.
    pub(super) fn callback_return_pinned_by_concrete_arg(
        &mut self,
        shape: &FunctionShape,
        args: &[NodeIndex],
        return_type_params: &FxHashSet<Atom>,
    ) -> bool {
        // Type parameters fixed by a concrete (non context-sensitive) argument
        // are pinned before the callback body is typed.
        let mut pinned: FxHashSet<Atom> = FxHashSet::default();
        for (i, &arg_idx) in args.iter().enumerate() {
            let Some(param_type) = shape.params.get(i).map(|p| p.type_id).or_else(|| {
                shape
                    .params
                    .last()
                    .and_then(|p| p.rest.then_some(p.type_id))
            }) else {
                break;
            };
            if is_contextually_sensitive(self, arg_idx) {
                continue;
            }
            pinned.extend(self.collect_type_param_names_for_context_overlap(param_type));
        }
        // A return type parameter is the inference target the contextual type
        // legitimately seeds; it is not a "pinned input".
        pinned.retain(|name| !return_type_params.contains(name));
        if pinned.is_empty() {
            return false;
        }

        for (i, &arg_idx) in args.iter().enumerate() {
            let Some(param_type) = shape.params.get(i).map(|p| p.type_id).or_else(|| {
                shape
                    .params
                    .last()
                    .and_then(|p| p.rest.then_some(p.type_id))
            }) else {
                break;
            };
            if !is_contextually_sensitive(self, arg_idx) {
                continue;
            }
            // The matching parameter must be a callback whose signature mentions
            // the bare return type parameter (its return position).
            let signature_names = self.collect_type_param_names_for_context_overlap(param_type);
            if !signature_names
                .iter()
                .any(|name| return_type_params.contains(name))
            {
                continue;
            }
            if self.callback_return_built_from_pinned_params(arg_idx, param_type, &pinned) {
                return true;
            }
        }
        false
    }

    /// Returns true when the callback at `arg_idx` has an inferred return whose
    /// complete value is structurally derived from callback parameters bound to
    /// fully `pinned` types. Supported returns are property projections rooted
    /// in those parameters and object literals whose values are such projections.
    fn callback_return_built_from_pinned_params(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
        pinned: &FxHashSet<Atom>,
    ) -> bool {
        let Some(cb_idx) = self.callback_function_index(arg_idx) else {
            return false;
        };
        // Extract the scalars we need before any `&mut self` calls below, so the
        // arena borrow of the function node is released.
        let (param_nodes, has_return_annotation, is_arrow, body): (
            Vec<NodeIndex>,
            bool,
            bool,
            NodeIndex,
        ) = {
            let Some(cb_node) = self.ctx.arena.get(cb_idx) else {
                return false;
            };
            let Some(func) = self.ctx.arena.get_function(cb_node) else {
                return false;
            };
            (
                func.parameters.nodes.clone(),
                func.type_annotation.is_some(),
                func.equals_greater_than_token,
                func.body,
            )
        };
        // Inferred-return arrows only: an explicit return annotation pins `U`
        // independently. Block bodies are accepted only by the single-return
        // extractor below.
        if has_return_annotation || !is_arrow {
            return false;
        }

        // Map each callback parameter NAME to its EXPECTED type from the
        // contextual signature (the callback's own parameters are unannotated),
        // and record the names bound to a fully-pinned type.
        let Some(cb_shape) = self.contextual_signature_after_evaluation(param_type) else {
            return false;
        };
        let mut pinned_param_names: FxHashSet<String> = FxHashSet::default();
        for (j, &param_idx) in param_nodes.iter().enumerate() {
            let Some(name) = self.simple_parameter_name(param_idx) else {
                continue;
            };
            let Some(expected) = cb_shape.params.get(j).map(|p| p.type_id) else {
                continue;
            };
            let names = self.collect_type_param_names_for_context_overlap(expected);
            if !names.is_empty() && names.iter().all(|n| pinned.contains(n)) {
                pinned_param_names.insert(name);
            }
        }
        if pinned_param_names.is_empty() {
            return false;
        }

        let Some(return_expression) = self.single_callback_return_expression(body) else {
            return false;
        };
        self.return_expression_is_pinned_projection(return_expression, &pinned_param_names)
    }

    /// Extract a concise-body expression or the expression from a block that
    /// consists of exactly one return statement.
    fn single_callback_return_expression(&self, body: NodeIndex) -> Option<NodeIndex> {
        let body = self.ctx.arena.skip_parenthesized(body);
        let body_node = self.ctx.arena.get(body)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return Some(body);
        }
        let block = self.ctx.arena.get_block(body_node)?;
        let [statement] = block.statements.nodes.as_slice() else {
            return None;
        };
        let statement_node = self.ctx.arena.get(*statement)?;
        if statement_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return None;
        }
        let return_statement = self.ctx.arena.get_return_statement(statement_node)?;
        return_statement
            .expression
            .is_some()
            .then_some(return_statement.expression)
    }

    /// A pinned projection contains no fresh expression that outer contextual
    /// typing could refine. It is either a pinned parameter, a property chain
    /// rooted in one, or a non-empty object literal composed only of such values.
    fn return_expression_is_pinned_projection(
        &self,
        expression: NodeIndex,
        pinned_param_names: &FxHashSet<String>,
    ) -> bool {
        let expression = self.ctx.arena.skip_parenthesized(expression);
        let Some(node) = self.ctx.arena.get(expression) else {
            return false;
        };
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| pinned_param_names.contains(&ident.escaped_text));
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let Some(access) = self.ctx.arena.get_access_expr(node) else {
                return false;
            };
            if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && !self.element_access_key_is_fixed_literal(access.name_or_argument)
            {
                return false;
            }
            return self
                .return_expression_is_pinned_projection(access.expression, pinned_param_names);
        }
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };
        !object.elements.nodes.is_empty()
            && object.elements.nodes.iter().all(|&element_idx| {
                let Some(element) = self.ctx.arena.get(element_idx) else {
                    return false;
                };
                let value = if element.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                    self.ctx
                        .arena
                        .get_property_assignment(element)
                        .map(|property| property.initializer)
                } else if element.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                    self.ctx
                        .arena
                        .get_shorthand_property(element)
                        .map(|property| property.name)
                } else {
                    None
                };
                value.is_some_and(|value| {
                    self.return_expression_is_pinned_projection(value, pinned_param_names)
                })
            })
    }

    fn element_access_key_is_fixed_literal(&self, key: NodeIndex) -> bool {
        let key = self.ctx.arena.skip_parenthesized(key);
        self.ctx.arena.get(key).is_some_and(|node| {
            node.kind == SyntaxKind::StringLiteral as u16
                || node.kind == SyntaxKind::NumericLiteral as u16
                || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        })
    }

    /// Returns the simple identifier name of a parameter, or `None` for
    /// destructuring/rest/non-identifier binding patterns.
    fn simple_parameter_name(&self, param_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(param_idx)?;
        let param = self.ctx.arena.get_parameter(node)?;
        if param.dot_dot_dot_token {
            return None;
        }
        let name_node = self.ctx.arena.get(param.name)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        Some(ident.escaped_text.clone())
    }
}
