//! Interface heritage index-signature compatibility helpers.

use crate::state::CheckerState;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::TypeParamInfo;

impl<'a> CheckerState<'a> {
    pub(super) fn is_direct_this_type(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::type_predicates::is_this_type(self.ctx.types, type_id)
    }

    pub(super) fn function_type_returns_current_interface_family(
        &self,
        source: TypeId,
        target: TypeId,
        current_iface_def_id: Option<tsz_solver::def::DefId>,
    ) -> bool {
        let Some(current_iface_def_id) = current_iface_def_id else {
            return false;
        };
        let Some(source_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, source)
        else {
            return false;
        };
        let Some(target_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, target)
        else {
            return false;
        };

        let source_return = source_shape.return_type;
        let target_return = target_shape.return_type;
        if self.is_direct_this_type(target_return) {
            return false;
        }

        // Only suppress when the target (base) return type is itself a named
        // type from some interface/class family. Without this guard the
        // suppression also hides genuine TS2430 errors where the base ancestor
        // returns an unrelated primitive (e.g. `string`) but the derived method
        // returns the current interface; see PR #2571 review.
        if self.type_base_def_id(target_return).is_none() {
            return false;
        }

        self.type_base_def_id(source_return) == Some(current_iface_def_id)
    }

    /// Build a combined callable per overloaded derived method name so the
    /// cross-file interface heritage path can compare an overloaded override as
    /// a whole rather than signature-by-signature (the strict relation rejects a
    /// single derived overload against the base's combined overload set even for
    /// valid specializations). Names with a single signature are skipped.
    pub(super) fn collect_overloaded_derived_method_callables(
        &self,
        derived_members: &[(String, TypeId, NodeIndex, u16, bool, bool)],
        derived_method_counts: &rustc_hash::FxHashMap<String, usize>,
    ) -> rustc_hash::FxHashMap<String, TypeId> {
        let mut by_name: rustc_hash::FxHashMap<String, Vec<TypeId>> =
            rustc_hash::FxHashMap::default();
        for (name, member_type, _, kind, _, _) in derived_members {
            if *kind == syntax_kind_ext::METHOD_SIGNATURE
                && derived_method_counts.get(name).copied().unwrap_or(0) > 1
            {
                by_name.entry(name.clone()).or_default().push(*member_type);
            }
        }
        let mut result: rustc_hash::FxHashMap<String, TypeId> = rustc_hash::FxHashMap::default();
        for (name, object_types) in by_name {
            if let Some(callable) =
                crate::query_boundaries::class::combine_overloaded_method_callable(
                    self.ctx.types,
                    &object_types,
                    &name,
                )
            {
                result.insert(name, callable);
            }
        }
        result
    }

    /// Resolve the base interface's type parameters and the heritage clause's
    /// type arguments, padding with defaults/constraints to the base arity. The
    /// cross-file interface heritage path uses these to instantiate base member
    /// types that still reference the base's own parameters. Returns `None` when
    /// there are no type arguments or the base is non-generic.
    pub(super) fn base_heritage_params_and_args(
        &mut self,
        base_sym_id: SymbolId,
        type_arguments: Option<&tsz_parser::parser::base::NodeList>,
    ) -> Option<(Vec<TypeParamInfo>, Vec<TypeId>)> {
        let args = type_arguments?;
        let mut arg_ids: Vec<TypeId> = args
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();
        if arg_ids.is_empty() {
            return None;
        }
        let base_params = self.get_type_params_for_symbol(base_sym_id);
        if base_params.is_empty() {
            return None;
        }
        if arg_ids.len() < base_params.len() {
            for param in base_params.iter().skip(arg_ids.len()) {
                arg_ids.push(
                    param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN),
                );
            }
        }
        arg_ids.truncate(base_params.len());
        Some((base_params, arg_ids))
    }

    /// For interface heritage (TS2430), the strict no-erase-generics relation
    /// can reject alpha-equivalent generic method signatures whose method-local
    /// type parameters are represented with different `TypeId`s on each side —
    /// e.g. a base member resolved from a lowered interface type (cross-file
    /// `get_type_of_symbol`, which yields a `Callable` shape) vs a derived member
    /// computed directly from its AST (a `Function` shape). When both members are
    /// callables and at least one carries method-local type parameters, and the
    /// derived signature is assignable to the base signature under fresh
    /// method-local generic instantiation (the standard relation), the override
    /// is a valid specialization and tsc does not report TS2430 (matching
    /// `compareSignaturesRelated`). This is keyed on the structural shape of the
    /// signatures, not on any identifier name, so renaming the method-local type
    /// parameter does not change the decision. Non-generic members are excluded
    /// so the strict relation continues to govern ordinary property/method
    /// overrides.
    pub(super) fn generic_method_override_is_valid_specialization(
        &mut self,
        derived: TypeId,
        base: TypeId,
    ) -> bool {
        // Match tsc's `compareSignaturesRelated`: target's method-local type
        // parameters are canonicalized only when source has its own. Without
        // method-local generics on the source (derived), target stays
        // universally quantified and a concrete implementation cannot satisfy
        // `<T extends C>(x: T) => T`; the genuine overloaded-builder cases
        // that need that escape hatch are handled separately by
        // `implementation_signature_covers_interface_overloads`.
        use crate::query_boundaries::class::callable_signature_is_generic;
        if !callable_signature_is_generic(self.ctx.types, derived).unwrap_or(false)
            || callable_signature_is_generic(self.ctx.types, base).is_none()
        {
            return false;
        }
        self.interface_heritage_generic_method_relation_outcome(derived, base)
            .related
    }

    /// True when a *non-generic* derived method validly overrides a *generic*
    /// base method by dropping the base's method-local type parameter(s) to
    /// their constraints.
    ///
    /// tsc's `compareSignaturesRelated` accepts dropping a method-local generic
    /// that appears only in **input** (contravariant) positions — the override's
    /// wider concrete parameter accepts every instantiation of the parameter —
    /// and rejects one used in a **covariant** (return/output) position, where a
    /// caller relies on getting a specific instantiation back. For example,
    /// `with(k: string): Sub` is a valid override of
    /// `with<K extends string>(k: K): Base` (input-only `K`), while
    /// `m(): string` is **not** a valid override of `m<T>(): T` (covariant `T`).
    ///
    /// The strict no-erase relation used by `should_report_member_type_mismatch`
    /// rejects *both*, producing a false `TS2430` on the sound input-only case.
    /// The decision is keyed on the structural shape of the signatures (whether
    /// the base carries method-local type parameters the override does not), not
    /// on any identifier, so renaming the method or its type parameter does not
    /// change the outcome.
    ///
    /// Restricted to the regime the strict relation over-reports — the base
    /// method is generic and the override is not — via
    /// `generic_erasure_fallback_is_safe` so ordinary non-generic and
    /// matching-generic overrides keep the strict relation's decision. In that
    /// regime, dropping the base's method-local generic(s) to their
    /// constraint(s) is sound iff BOTH hold:
    /// 1. The override is assignable to the base after erasing those generics to
    ///    their constraints (method parameters bivariant). This accepts an
    ///    input-only parameter — the override's wider concrete parameter admits
    ///    every instantiation — while still rejecting a genuine parameter or
    ///    callback-position mismatch (`each(f: () => string)` is not assignable
    ///    to the erased `each(f: () => unknown)`).
    /// 2. The override's return type is assignable to the base's return type with
    ///    the generics kept OPAQUE (no-erase). A method-local generic used
    ///    covariantly in the return (`m(): string` vs `m<T>(): T`) fails this —
    ///    a concrete type is not assignable to the opaque `T` — while a generic
    ///    used only in inputs leaves a concrete (or `this`-family) return that
    ///    relates normally.
    ///
    /// Together these reproduce tsc's `compareSignaturesRelated` decision for the
    /// non-generic-override-of-generic-base shape without re-deriving variance.
    pub(super) fn nongeneric_input_only_generic_override_is_valid(
        &mut self,
        derived: TypeId,
        base: TypeId,
    ) -> bool {
        if crate::query_boundaries::class::generic_erasure_fallback_is_safe(self, derived, base) {
            return false;
        }
        if !self
            .bivariant_callbacks_relation_outcome(derived, base)
            .related
        {
            return false;
        }
        let (Some(derived_return), Some(base_return)) = (
            self.single_call_signature_return_type(derived),
            self.single_call_signature_return_type(base),
        ) else {
            return false;
        };
        self.no_erase_generics_relation_outcome(derived_return, base_return)
            .related
    }

    /// Return type of a callable member that has exactly one call signature and
    /// no construct signatures — the single-method-override shape. Returns `None`
    /// for overloaded members, constructors, or non-callable types, so the
    /// caller defers to the strict relation in those shapes.
    fn single_call_signature_return_type(&self, type_id: TypeId) -> Option<TypeId> {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            return Some(shape.return_type);
        }
        let shape =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)?;
        if shape.call_signatures.len() == 1 && shape.construct_signatures.is_empty() {
            return Some(shape.call_signatures[0].return_type);
        }
        None
    }

    /// True when a derived interface member that mentions the polymorphic `this`
    /// type is a valid override of the base member under tsc's `this`-type
    /// relation.
    ///
    /// Interface heritage checking rewrites `this` in derived members to the
    /// derived interface's concrete self type (needed for parameter-position
    /// cases). That substitution makes a method returning the polymorphic `this`
    /// (`m(): this`) indistinguishable from one returning the self type by name
    /// (`m(): B`). tsc accepts the former as a covariant override and rejects the
    /// latter. This consults the *raw* (un-substituted) derived member type so
    /// the solver's polymorphic `this` relation governs the decision: `this` is
    /// assignable to a base `this` (covariantly, and through positions such as
    /// `this[]`), while a concrete type is not assignable to a target `this`.
    ///
    /// `raw_this_members` maps a derived member node index to its un-substituted
    /// member object shape (only members that mention `this` are present);
    /// `base_member` is the (already extracted) base member type. Returns `false`
    /// when the derived member does not mention `this`, so the normal mismatch
    /// decision stands. Keyed on the structural presence of `this`, not on any
    /// identifier, so renaming members or type parameters does not change the
    /// outcome.
    pub(super) fn this_member_override_is_polymorphic(
        &mut self,
        raw_this_members: &rustc_hash::FxHashMap<NodeIndex, TypeId>,
        derived_member_idx: NodeIndex,
        member_key: &str,
        base_member: TypeId,
    ) -> bool {
        let Some(raw_derived_member) = raw_this_members.get(&derived_member_idx).copied() else {
            return false;
        };
        let derived = crate::query_boundaries::common::find_property_by_str(
            self.ctx.types,
            raw_derived_member,
            member_key,
        )
        .map(|p| p.type_id)
        .unwrap_or(raw_derived_member);
        self.no_erase_generics_relation_outcome(derived, base_member)
            .related
    }

    pub(super) fn type_base_def_id(&self, type_id: TypeId) -> Option<tsz_solver::def::DefId> {
        crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id).or_else(|| {
            let app_id = crate::query_boundaries::common::application_id(self.ctx.types, type_id)?;
            let app = self.ctx.types.type_application(app_id);
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
        })
    }

    fn index_value_base_def_id(&self, type_id: TypeId) -> Option<tsz_solver::def::DefId> {
        self.type_base_def_id(type_id)
            .or_else(|| self.ctx.definition_store.find_def_for_type(type_id))
    }

    pub(super) fn index_value_assignable_for_interface_extends(
        &mut self,
        derived_value: TypeId,
        base_value: TypeId,
    ) -> bool {
        let derived_value = self.evaluate_type_with_env(derived_value);
        let base_value = self.evaluate_type_with_env(base_value);
        if self
            .interface_heritage_index_value_relation_outcome(derived_value, base_value)
            .related
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, derived_value)
        {
            for member in members {
                let member = self.evaluate_type_with_env(member);
                if !self.index_value_member_assignable_for_interface_extends(member, base_value) {
                    return false;
                }
            }
            return true;
        }

        false
    }

    fn index_value_member_assignable_for_interface_extends(
        &mut self,
        derived_value: TypeId,
        base_value: TypeId,
    ) -> bool {
        self.interface_heritage_index_value_relation_outcome(derived_value, base_value)
            .related
            || self.type_heritage_includes_base(derived_value, base_value)
    }

    fn type_heritage_includes_base(&mut self, derived: TypeId, base: TypeId) -> bool {
        let Some(derived_def) = self.index_value_base_def_id(derived) else {
            return false;
        };
        let Some(base_def) = self.index_value_base_def_id(base) else {
            return false;
        };
        let Some(derived_sym) = self.ctx.def_to_symbol_id_with_fallback(derived_def) else {
            return false;
        };
        let Some(base_sym) = self.ctx.def_to_symbol_id_with_fallback(base_def) else {
            return false;
        };
        self.symbol_heritage_includes_base(
            derived_sym,
            base_sym,
            &mut rustc_hash::FxHashSet::default(),
        )
    }

    fn symbol_heritage_includes_base(
        &mut self,
        derived_sym: tsz_binder::SymbolId,
        base_sym: tsz_binder::SymbolId,
        visited: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        if derived_sym == base_sym {
            return true;
        }
        if !visited.insert(derived_sym) {
            return false;
        }

        let Some(symbol) = self.ctx.binder.get_symbol(derived_sym) else {
            return false;
        };
        let declarations = symbol.declarations.clone();
        for decl_idx in declarations {
            let decl_arena =
                self.ctx
                    .binder
                    .arena_for_declaration_or(derived_sym, decl_idx, self.ctx.arena);
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let heritage_clauses = decl_arena
                .get_interface(node)
                .and_then(|iface| iface.heritage_clauses.as_ref())
                .or_else(|| {
                    decl_arena
                        .get_class(node)
                        .and_then(|class| class.heritage_clauses.as_ref())
                });
            let Some(heritage_clauses) = heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = decl_arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = decl_arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = decl_arena.get(type_idx) else {
                        continue;
                    };
                    let expr_idx =
                        if let Some(expr_type_args) = decl_arena.get_expr_type_args(type_node) {
                            expr_type_args.expression
                        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                            decl_arena
                                .get_type_ref(type_node)
                                .map(|type_ref| type_ref.type_name)
                                .unwrap_or(type_idx)
                        } else {
                            type_idx
                        };
                    let heritage_binder = self
                        .ctx
                        .get_binder_for_arena(decl_arena)
                        .unwrap_or(self.ctx.binder);
                    let Some(parent_sym) = Self::resolve_heritage_symbol_in_arena(
                        decl_arena,
                        heritage_binder,
                        expr_idx,
                    ) else {
                        continue;
                    };
                    if self.symbol_heritage_includes_base(parent_sym, base_sym, visited) {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn resolve_heritage_symbol_in_arena(
        arena: &NodeArena,
        binder: &BinderState,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let node = arena.get(expr_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return binder.resolve_identifier(arena, expr_idx);
        }
        if node.kind != syntax_kind_ext::QUALIFIED_NAME
            && node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = arena.get_access_expr_at(expr_idx)?;
        let left_sym = Self::resolve_heritage_symbol_in_arena(arena, binder, access.expression)?;
        let name = arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.clone())?;
        binder
            .get_symbol(left_sym)?
            .exports
            .as_ref()
            .and_then(|exports| exports.get(&name))
    }
}
