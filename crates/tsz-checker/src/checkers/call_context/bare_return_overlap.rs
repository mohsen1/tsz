use crate::computation::complex::is_contextually_sensitive;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{FunctionShape, TypeId};

impl CheckerState<'_> {
    /// Detects the `#14792` shape: a generic call whose return type mentions a
    /// type parameter `U` — either bare (`(...): U`) or under a covariant wrapper
    /// (`(...): U[]`) — where a context-sensitive callback argument (whose
    /// declared signature mentions `U` in its return position) returns an object
    /// literal built ENTIRELY from the callback's own parameters that are already
    /// pinned by a concrete (non context-sensitive) sibling argument.
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
    pub(super) fn callback_object_return_pinned_by_concrete_arg(
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
            if self.callback_object_return_built_from_pinned_params(arg_idx, param_type, &pinned) {
                return true;
            }
        }
        false
    }

    /// Returns true when the callback at `arg_idx` is an inferred-return concise
    /// arrow whose body is an object literal whose every property value is one
    /// of the callback's own parameters bound to a type that is fully `pinned`.
    fn callback_object_return_built_from_pinned_params(
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
        // Inferred-return concise arrows only: an explicit return annotation
        // pins `U` independently, and block bodies are outside this narrow shape.
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

        // The concise-body return must be an object literal whose every property
        // value is one of those pinned callback parameters; no fresh leaf the
        // contextual type could refine.
        let body = self.ctx.arena.skip_parenthesized(body);
        let element_indices: Vec<NodeIndex> = {
            let Some(body_node) = self.ctx.arena.get(body) else {
                return false;
            };
            if body_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return false;
            }
            let Some(obj) = self.ctx.arena.get_literal_expr(body_node) else {
                return false;
            };
            obj.elements.nodes.clone()
        };
        if element_indices.is_empty() {
            return false;
        }
        element_indices.iter().all(|&el_idx| {
            let Some(el) = self.ctx.arena.get(el_idx) else {
                return false;
            };
            let value_idx = if el.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                self.ctx
                    .arena
                    .get_property_assignment(el)
                    .map(|p| p.initializer)
            } else if el.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                self.ctx.arena.get_shorthand_property(el).map(|p| p.name)
            } else {
                None
            };
            let Some(value_idx) = value_idx else {
                return false;
            };
            let value_idx = self.ctx.arena.skip_parenthesized(value_idx);
            self.ctx
                .arena
                .get(value_idx)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .is_some_and(|ident| pinned_param_names.contains(&ident.escaped_text))
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
