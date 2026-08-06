use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext::METHOD_SIGNATURE;
use tsz_solver::TypeId;

pub(crate) struct InterfaceOverloadCoverageCtx<'a> {
    pub(crate) iface_name: NodeIndex,
    pub(crate) derived_name: &'a str,
    pub(crate) base_name: &'a str,
    pub(crate) base_sym_id: tsz_binder::SymbolId,
    pub(crate) base_iface_indices: &'a [NodeIndex],
    pub(crate) derived_member_names: &'a rustc_hash::FxHashSet<String>,
    pub(crate) derived_members: &'a [(String, TypeId, NodeIndex, u16, bool, bool)],
    pub(crate) substitution: &'a TypeSubstitution,
    pub(crate) interface_self_type: Option<TypeId>,
}

impl<'a> CheckerState<'a> {
    pub(crate) fn check_interface_overload_coverage(
        &mut self,
        ctx: InterfaceOverloadCoverageCtx<'_>,
    ) {
        let InterfaceOverloadCoverageCtx {
            iface_name,
            derived_name,
            base_name,
            base_sym_id,
            base_iface_indices,
            derived_member_names,
            derived_members,
            substitution,
            interface_self_type,
        } = ctx;
        let base_method_groups: Vec<(String, Vec<TypeId>)>;
        {
            let mut by_name: rustc_hash::FxHashMap<String, Vec<TypeId>> =
                rustc_hash::FxHashMap::default();
            for &base_iface_idx in base_iface_indices {
                // `base_iface_idx` is only guaranteed valid against the arena
                // that owns `base_sym_id`'s declaration — which is a foreign
                // (e.g. lib) arena whenever this coverage check runs against
                // an interface being rechecked from a different file's
                // context (the `CallableFunction`/`NewableFunction` vs.
                // user-augmented `Function` family). Reading it through
                // `self.ctx.arena` used to silently miss (`NodeArena::get`
                // returns `None` for a foreign index), dropping the base's
                // whole overloaded-method set and producing a false-negative
                // TS2430.
                let decl_arena = self.ctx.binder.arena_for_declaration_or(
                    base_sym_id,
                    base_iface_idx,
                    self.ctx.arena,
                );
                let Some(base_node) = decl_arena.get(base_iface_idx) else {
                    continue;
                };
                let Some(base_iface) = decl_arena.get_interface(base_node) else {
                    continue;
                };

                let method_member_indices: Vec<NodeIndex> = base_iface
                    .members
                    .nodes
                    .iter()
                    .copied()
                    .filter(|&member_idx| {
                        decl_arena
                            .get(member_idx)
                            .is_some_and(|node| node.kind == METHOD_SIGNATURE)
                    })
                    .collect();
                if method_member_indices.is_empty() {
                    continue;
                }

                let cross_arena = !std::ptr::eq(decl_arena, self.ctx.arena);
                let delegated_types = if cross_arena {
                    self.delegate_cross_arena_interface_member_simple_types(
                        base_iface_idx,
                        &method_member_indices,
                        decl_arena,
                        None,
                        false,
                    )
                } else {
                    None
                };

                for &base_member_idx in &method_member_indices {
                    let Some(base_member_node) = decl_arena.get(base_member_idx) else {
                        continue;
                    };
                    let Some(sig) = decl_arena.get_signature(base_member_node) else {
                        continue;
                    };
                    let Some(name) =
                        crate::types_domain::queries::core::get_literal_or_well_known_property_name(
                            decl_arena, sig.name,
                        )
                    else {
                        continue;
                    };
                    if !derived_member_names.contains(&name) {
                        continue;
                    }
                    let raw_type = if cross_arena {
                        let Some(t) = delegated_types
                            .as_ref()
                            .and_then(|types| types.get(&base_member_idx).copied())
                        else {
                            continue;
                        };
                        t
                    } else {
                        self.get_type_of_interface_member(base_member_idx)
                    };
                    let base_type = crate::query_boundaries::class::maybe_substitute_this_type(
                        self.ctx.types,
                        instantiate_type(self.ctx.types, raw_type, substitution),
                        interface_self_type,
                    );
                    by_name.entry(name).or_default().push(base_type);
                }
            }
            base_method_groups = by_name.into_iter().collect();
        }

        let mut derived_method_overloads: rustc_hash::FxHashMap<String, Vec<(TypeId, NodeIndex)>> =
            rustc_hash::FxHashMap::default();
        for (name, type_id, idx, kind, _, _) in derived_members {
            if *kind == METHOD_SIGNATURE {
                derived_method_overloads
                    .entry(name.clone())
                    .or_default()
                    .push((*type_id, *idx));
            }
        }

        let signature_contains_error = |signature: TypeId| {
            crate::query_boundaries::common::contains_error_type_in_args(self.ctx.types, signature)
        };

        tracing::debug!(
            derived = derived_name,
            base = base_name,
            n_base_method_names = base_method_groups.len(),
            n_derived_method_names = derived_method_overloads.len(),
            interface_self_type = interface_self_type.map(|t| t.0),
            "overload coverage check"
        );

        // tsc checks interface heritage with `checkTypeAssignableTo(derived, base)`:
        // the derived member's *entire* overload set must be assignable to the
        // base member's entire overload set. For overloaded function/method types
        // that is `signaturesRelatedTo`'s N×M rule — every base (target)
        // signature must be matched by some derived (source) signature, with
        // method signatures compared bivariantly.
        //
        // Comparing only a single "trailing" signature per side (the previous
        // heuristic) was wrong in both directions: it missed real mismatches
        // (a derived set that drops a base overload it cannot service, e.g. a
        // derived `pipe(op1, op2)` that no longer covers the base's one-argument
        // `pipe(op1)`), and it raised false `TS2430`s (a valid generic, specialized,
        // or superset override whose trailing signature happened not to relate to
        // the base's trailing signature). Build the full overload callables for
        // both sides and route them through the standard relation, which already
        // implements the N×M rule (`check_callable_subtype`), including method
        // bivariance and method-local generic erasure for multi-signature shapes.
        'overload_check: for (method_name, base_sigs) in &base_method_groups {
            let Some(derived_sigs) = derived_method_overloads.get(method_name) else {
                tracing::debug!(method = method_name, "no derived overloads found");
                continue;
            };
            tracing::trace!(
                method = method_name,
                base_signatures = base_sigs.len(),
                derived_signatures = derived_sigs.len(),
                "checking deferred interface method overload set"
            );
            // The member loop defers a method name when either side is
            // overloaded. Keep single/single names out of this second pass,
            // while still checking a derived overload set against a single
            // tuple-union/rest signature on the base.
            if base_sigs.len() < 2 && derived_sigs.len() < 2 {
                continue;
            }
            if base_sigs.iter().copied().any(signature_contains_error)
                || derived_sigs
                    .iter()
                    .any(|(signature, _)| signature_contains_error(*signature))
            {
                continue;
            }

            let Some(base_callable) =
                crate::query_boundaries::class::build_method_overload_callable(
                    self.ctx.types,
                    base_sigs.iter().copied(),
                    method_name,
                    1,
                )
            else {
                continue;
            };
            let Some(derived_callable) =
                crate::query_boundaries::class::build_method_overload_callable(
                    self.ctx.types,
                    derived_sigs.iter().map(|(sig, _)| *sig),
                    method_name,
                    1,
                )
            else {
                continue;
            };

            // Identical interned overload sets (a derived interface re-declaring
            // the inherited signatures verbatim) are trivially assignable.
            if derived_callable == base_callable {
                continue;
            }

            // The override is valid when the derived overload set is assignable to
            // the base overload set. Allow the fresh method-local generic retry so
            // alpha-equivalent generic overloads (rxjs/kysely-style builders whose
            // method-local type parameters carry different `TypeId`s on each side)
            // are accepted, mirroring tsc's `compareSignaturesRelated`.
            let assignable = crate::query_boundaries::class::interface_overload_set_assignable(
                self,
                derived_callable,
                base_callable,
                true,
            );

            // Anchor parse-recovery suppression on the last derived signature
            // node (non-empty: `build_method_overload_callable(.., 1)` above
            // returned `Some`, which requires at least one gathered signature).
            let derived_anchor_idx = derived_sigs
                .last()
                .expect("derived overload set is non-empty")
                .1;
            if !assignable
                && !self.should_suppress_assignability_for_parse_recovery(
                    derived_anchor_idx,
                    derived_anchor_idx,
                )
            {
                self.error_at_node(
                    iface_name,
                    &format!(
                        "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                    ),
                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                );
                break 'overload_check;
            }
        }
    }
}
