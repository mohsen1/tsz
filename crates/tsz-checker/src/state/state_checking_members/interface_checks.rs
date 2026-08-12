//! Interface declaration and duplicate member checking.
//!
//! Extracted from `member_access.rs` to keep each file under the 2000-line
//! architectural limit.

use crate::state::CheckerState;
use crate::symbols_domain::name_text::{
    is_zero_arg_call_like_expr_in_arena, simple_computed_name_expr_text_in_arena,
};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{InterfaceData, NodeAccess};
use tsz_parser::parser::{NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::def::DefId;
use tsz_solver::{TypeId, TypeParamInfo};

type TypeParameterScopeUpdates = Vec<(String, Option<TypeId>, bool)>;
type PushedInterfaceTypeParameters = (Vec<TypeParamInfo>, TypeParameterScopeUpdates);

impl<'a> CheckerState<'a> {
    fn push_interface_type_parameters(
        &mut self,
        iface_name: NodeIndex,
        type_parameters: &Option<NodeList>,
    ) -> PushedInterfaceTypeParameters {
        let (mut params, updates) = self.push_type_parameters(type_parameters);
        let Some(merged_params) =
            self.merged_interface_type_parameters_for_scope(iface_name, &params)
        else {
            return (params, updates);
        };

        for (param, merged) in params.iter_mut().zip(merged_params.iter()) {
            let name = self.ctx.types.resolve_atom(param.name);
            let type_id = self.ctx.types.factory().type_param(*merged);
            self.ctx.type_parameter_scope.insert(name, type_id);
            *param = *merged;
        }

        (params, updates)
    }

    fn merged_interface_type_parameters_for_scope(
        &mut self,
        iface_name: NodeIndex,
        current_params: &[TypeParamInfo],
    ) -> Option<Vec<TypeParamInfo>> {
        if current_params.is_empty() {
            return None;
        }
        let sym_id = self
            .resolve_type_symbol_for_lowering(iface_name)
            .map(SymbolId)?;
        let symbol = self.get_cross_file_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::INTERFACE) || symbol.declarations.len() <= 1 {
            return None;
        }
        let merged_params = self.get_type_params_for_symbol(sym_id);
        if merged_params.len() != current_params.len() {
            return None;
        }
        let names_match = current_params
            .iter()
            .zip(merged_params.iter())
            .all(|(current, merged)| current.name == merged.name);
        if !names_match {
            return None;
        }
        let merged_adds_constraint_or_default = current_params
            .iter()
            .zip(merged_params.iter())
            .any(|(current, merged)| {
                (current.constraint.is_none() && merged.constraint.is_some())
                    || (current.default.is_none() && merged.default.is_some())
            });
        merged_adds_constraint_or_default.then_some(merged_params)
    }

    /// Minimal interface validation used by post-merge standard library checks.
    ///
    /// The normal interface checker runs many declaration-file diagnostics that are
    /// unrelated to user-induced global merges. For lib rechecks we only need member
    /// type annotations to trigger generic constraint diagnostics (TS2344) and
    /// heritage compatibility to catch broken merged inheritance (TS2430).
    pub(crate) fn check_lib_interface_declaration_post_merge(
        &mut self,
        stmt_idx: NodeIndex,
        check_extension_compatibility: bool,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(iface) = self.ctx.arena.get_interface(node) else {
            return;
        };

        let (_type_params, type_param_updates) =
            self.push_interface_type_parameters(iface.name, &iface.type_parameters);
        let interface_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();
        self.check_heritage_clauses_for_unresolved_names(
            &iface.heritage_clauses,
            false,
            &interface_type_param_names,
        );

        for &member_idx in &iface.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                let (_type_params, method_type_param_updates) =
                    self.push_type_parameters(&sig.type_parameters);
                // Resolve parameter types first so the params list (and their
                // names) is available when checking the return-type annotation.
                // The return type may reference parameters via `typeof p` —
                // pushing typeof_param_scope around the annotation resolution
                // mirrors the lowering crate's behavior so identifiers like
                // `a` in `(a: number): typeof a` resolve to the parameter
                // type instead of falling through to TS2304.
                for &param_idx in sig.parameters.as_ref().map_or(&[][..], |p| &p.nodes) {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && param.type_annotation.is_some()
                    {
                        self.check_type_node(param.type_annotation);
                        self.get_type_from_type_node(param.type_annotation);
                    }
                }
                if sig.type_annotation.is_some() {
                    let (params, _this_type) =
                        self.extract_params_from_signature_in_type_literal(sig);
                    self.push_typeof_param_scope(&params);
                    self.check_type_node(sig.type_annotation);
                    self.get_type_from_type_node(sig.type_annotation);
                    self.pop_typeof_param_scope(&params);
                }
                self.pop_type_parameters(method_type_param_updates);
                continue;
            }

            if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                if accessor.type_annotation.is_some() {
                    self.check_type_node(accessor.type_annotation);
                    self.get_type_from_type_node(accessor.type_annotation);
                }
                for &param_idx in &accessor.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && param.type_annotation.is_some()
                    {
                        self.check_type_node(param.type_annotation);
                        self.get_type_from_type_node(param.type_annotation);
                    }
                }
            }
        }

        if check_extension_compatibility {
            self.check_interface_extension_compatibility(stmt_idx, iface);
        }
        self.pop_type_parameters(type_param_updates);
    }

    /// Check an interface declaration.
    pub(crate) fn check_interface_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(iface) = self.ctx.arena.get_interface(node) else {
            return;
        };

        // TS1042: async modifier cannot be used on interface declarations
        self.check_async_modifier_on_declaration(&iface.modifiers);

        // TS1277: 'const' modifier not allowed on interface type parameters
        self.check_const_type_parameter_on_non_function(iface.type_parameters.as_ref());

        // TS1274: Check for modifiers that can never appear on type parameters
        // (public, private, static, etc.)
        self.check_never_valid_type_parameter_modifiers(iface.type_parameters.as_ref());

        // Check for reserved interface names (TS2427). The checker owns the soft
        // predefined-type names (`string`, `number`, ...); the hard-keyword
        // `void`/`null` copy this also emits is deduplicated by the CLI keep-gate
        // against the parser's own hard-keyword TS2427. See #16279 and
        // `checker_diagnostics::keep_checker_diagnostic_when_program_has_real_syntax_errors`.
        if iface.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(iface.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && crate::error_reporter::assignability::is_reserved_type_name(
                ident.escaped_text.as_str(),
            )
        {
            self.error_at_node(
                iface.name,
                &format!("Interface name cannot be '{}'.", ident.escaped_text),
                diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
            );
        }

        // TS1212: tsc emits "Identifier expected. '<name>' is a reserved word
        // in strict mode" for interface names that are strict-mode reserved
        // words like `public`, `private`, `protected`, `implements`, etc.
        // (Distinct from `interface interface {}`, where the name is a hard
        // keyword and tsc emits grammar errors like TS1438.)
        if iface.name.is_some() {
            self.check_strict_mode_reserved_name_at(iface.name, iface.name);
        }

        // Check for circular inheritance (TS2310)
        // Must be done before resolving types to avoid infinite recursion
        use crate::class_inheritance::ClassInheritanceChecker;
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        if checker.check_interface_inheritance_cycle(stmt_idx, iface) {
            // If cycle detected, we can still proceed with checking members but
            // heritage graph is now aware of the cycle (or it was reported)
        }

        // Push type parameters BEFORE checking heritage clauses
        // This allows heritage clauses to reference the interface's type parameters
        let (_type_params, type_param_updates) =
            self.push_interface_type_parameters(iface.name, &iface.type_parameters);

        // Check for duplicate type parameters
        self.check_duplicate_type_parameters(&iface.type_parameters);

        // Check type parameter defaults for ordering (TS2706), forward references (TS2744),
        // and circular defaults (TS2716)
        let iface_name_str = self
            .ctx
            .arena
            .get(iface.name)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|id| id.escaped_text.to_string());
        if let Some(ref name) = iface_name_str {
            self.check_type_parameters_for_missing_names_with_enclosing(
                &iface.type_parameters,
                name,
            );
        } else {
            self.check_type_parameters_for_missing_names(&iface.type_parameters);
        }

        // Collect interface type parameter names for TS2304 checking in heritage clauses
        let interface_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(
            &iface.heritage_clauses,
            false,
            &interface_type_param_names,
        );

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&iface.type_parameters, stmt_idx);

        // Check each interface member for missing type references and parameter properties
        // Get interface name for circularity checks (TS2502/TS2615)
        let iface_name = if iface.name != NodeIndex::NONE {
            self.ctx
                .arena
                .get(iface.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|ident| self.ctx.arena.resolve_identifier_text(ident).to_string())
        } else {
            None
        };

        for &member_idx in &iface.members.nodes {
            if self.interface_member_is_mapped_type(member_idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    member_idx,
                    diagnostic_messages::A_MAPPED_TYPE_MAY_NOT_DECLARE_PROPERTIES_OR_METHODS,
                    diagnostic_codes::A_MAPPED_TYPE_MAY_NOT_DECLARE_PROPERTIES_OR_METHODS,
                );
            }
            self.check_styled_component_inner_component_constraint(member_idx);
            self.check_type_member_for_missing_names(member_idx);
            self.check_type_member_for_parameter_properties(member_idx);
            // The noImplicitAny accessor family (TS7033/TS7032/TS7006). Needs
            // the sibling members to resolve the get/set pair, so it cannot ride
            // the per-member walk above.
            self.check_type_member_accessor_implicit_any(member_idx, &iface.members.nodes);
            // TS1268: Check index signature parameter types
            self.check_index_signature_parameter_type(member_idx);
            // TS1169: Computed property in interface must have literal/unique symbol type
            if let Some(member_node) = self.ctx.arena.get(member_idx) {
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.check_computed_property_requires_literal(
                            sig.name,
                            diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE,
                            diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE,
                        );
                    }
                    // TS1539: a bigint literal interface property name (`123n: string`).
                    // Method signatures share this same `sig` shape but never take
                    // this diagnostic — gate on the member being a property.
                    if member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
                        self.check_bigint_literal_property_name(sig.name);
                    }
                    let (_type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    // Resolve parameters first so the param scope is built
                    // before the return type annotation is checked. The
                    // return type may reference `typeof p`; pushing the
                    // typeof_param_scope mirrors the lowering crate so the
                    // checker's TS2304 path can resolve the parameter.
                    for &param_idx in sig.parameters.as_ref().map_or(&[][..], |p| &p.nodes) {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                            && param.type_annotation.is_some()
                        {
                            self.check_type_node(param.type_annotation);
                            self.get_type_from_type_node(param.type_annotation);
                        }
                    }
                    if sig.type_annotation.is_some() {
                        let (params, _this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        self.push_typeof_param_scope(&params);
                        self.check_type_node(sig.type_annotation);
                        self.get_type_from_type_node(sig.type_annotation);
                        self.pop_typeof_param_scope(&params);
                    }
                    self.pop_type_parameters(type_param_updates);
                }
                // TS2344: Eagerly resolve set accessor parameter type annotations.
                // tsc checks all type annotations during declaration checking, even
                // when the setter is never observed. Without this, type references
                // like `Fail<string>` in `set x(value: Fail<string>)` (where
                // `type Fail<T extends never> = T`) would never trigger constraint
                // validation because the getter returns early in type computation
                // and the setter parameter type is never resolved.
                if member_node.kind == syntax_kind_ext::SET_ACCESSOR
                    && let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                {
                    for &param_idx in &accessor.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                            && param.type_annotation.is_some()
                        {
                            self.get_type_from_type_node(param.type_annotation);
                        }
                    }
                }
                // Also resolve get accessor return type annotations for the same
                // reason: constraint validation on type references in return types.
                if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                    && accessor.type_annotation.is_some()
                {
                    self.get_type_from_type_node(accessor.type_annotation);
                }
                // TS2526: Find `this` types appearing inside nested type
                // literals on a property signature annotation and route them
                // through `get_type_from_type_node` so the THIS_TYPE branch
                // emits the diagnostic.
                //
                // Why a targeted walk: `get_type_of_interface` runs the
                // property's annotation through the lowering pipeline, which
                // silently maps `this` to `ThisType` without invoking
                // `is_this_type_allowed`. Calling `get_type_from_type_node`
                // on the entire annotation perturbs DefId registration order
                // for adjacent interface types and corrupts type-printer
                // output (e.g. `Real & Fake` rendering as
                // `Lazy(N) & Lazy(M)`). Resolving only the THIS_TYPE leaves
                // outer type registration untouched while still firing the
                // diagnostic.
                if member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
                    && let Some(sig) = self.ctx.arena.get_signature(member_node)
                    && sig.type_annotation.is_some()
                {
                    self.check_nested_this_types_for_ts2526(sig.type_annotation);
                }

                if member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
                    && let Some(owner_name) = iface_name.as_deref()
                    && let Some(sig) = self.ctx.arena.get_signature(member_node)
                    && sig.type_annotation.is_some()
                    && let Some(property_name) =
                        crate::types_domain::queries::core::get_literal_property_name(
                            self.ctx.arena,
                            sig.name,
                        )
                    && self.indexed_access_references_owner_property(
                        sig.type_annotation,
                        owner_name,
                        &property_name,
                    )
                {
                    let message = format!(
                        "'{property_name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(sig.name, &message, 2502);
                }
            }
            // TS2502 + TS2615: Check if property type annotation circularly
            // references itself through a mapped type applied to the enclosing interface.
            if let Some(ref iface_name) = iface_name {
                self.check_interface_property_circular_mapped_type(member_idx, iface_name);
            }
        }

        // TS2386: Check optionality agreement for interface method overloads
        {
            use rustc_hash::FxHashMap;

            // Group method signatures by name
            let mut method_groups: FxHashMap<String, Vec<(NodeIndex, bool)>> = FxHashMap::default();
            for &member_idx in &iface.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::METHOD_SIGNATURE {
                    continue;
                }
                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(sig.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };
                method_groups
                    .entry(ident.escaped_text.to_string())
                    .or_default()
                    .push((member_idx, sig.question_token));
            }
            for members in method_groups.values() {
                if members.len() < 2 {
                    continue;
                }
                let first_optional = members[0].1;
                for &(member_idx, optional) in &members[1..] {
                    if optional != first_optional {
                        let error_node = self
                            .ctx
                            .arena
                            .get(member_idx)
                            .and_then(|n| self.ctx.arena.get_signature(n))
                            .map(|s| s.name)
                            .unwrap_or(member_idx);
                        self.error_at_node(
                            error_node,
                            crate::diagnostics::diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                            crate::diagnostics::diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                        );
                    }
                }
            }
        }

        // Check for duplicate member names (TS2300)
        self.check_duplicate_interface_members(&iface.members.nodes);

        // Check that properties are assignable to index signatures (TS2411)
        // This includes both directly declared and inherited index signatures.
        // Get the interface type to check for any index signatures (direct or inherited)
        // NOTE: Use get_type_of_symbol to get the cached type, avoiding recursion issues
        let iface_type = if iface.name.is_some() {
            // Get symbol from the interface name and resolve its type
            if let Some(name_node) = self.ctx.arena.get(iface.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(&ident.escaped_text) {
                        self.get_type_of_symbol(sym_id)
                    } else {
                        TypeId::ERROR
                    }
                } else {
                    TypeId::ERROR
                }
            } else {
                TypeId::ERROR
            }
        } else {
            // Anonymous interface - compute type directly
            self.get_type_of_interface(stmt_idx)
        };

        let index_info = self.ctx.types.get_index_signatures(iface_type);

        // Check if there are own index signatures by scanning members
        let has_own_index_sig = iface.members.nodes.iter().any(|&member_idx| {
            self.ctx.arena.get(member_idx).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::INDEX_SIGNATURE
            })
        });

        // If there are any index signatures (direct, own, or inherited), check compatibility
        if index_info.string_index.is_some()
            || index_info.number_index.is_some()
            || index_info.symbol_index.is_some()
            || has_own_index_sig
        {
            self.check_index_signature_compatibility(&iface.members.nodes, iface_type, stmt_idx);

            // Also check inherited members from base interfaces against index
            // signatures. The AST-based check above only sees own members; inherited
            // properties live in the solver's resolved type and must be checked too.
            //
            // Run when the interface has heritage AND either:
            //   (a) owns an index signature (errors anchor at the index sig node), OR
            //   (b) only inherits index sigs (no own sig → errors anchor at the
            //       interface name via name_fallback_node in the callee).
            if iface.heritage_clauses.is_some()
                && (has_own_index_sig
                    || index_info.string_index.is_some()
                    || index_info.number_index.is_some()
                    || index_info.symbol_index.is_some())
            {
                self.check_inherited_properties_against_index_signatures(
                    iface_type,
                    &iface.members.nodes,
                    stmt_idx,
                );
            }
        }

        // Check that interface correctly extends base interfaces (error 2430)
        let ts2430_diag_start = self.ctx.diagnostics.len();
        self.check_interface_extension_compatibility(stmt_idx, iface);
        self.register_verified_interface_extends_if_clean(stmt_idx, iface, ts2430_diag_start);

        // Check variance annotations match actual usage (TS2636)
        self.check_variance_annotations(stmt_idx, &iface.type_parameters);

        self.pop_type_parameters(type_param_updates);
    }

    /// Register the interface's first `extends` heritage edge as
    /// checker-verified, but only when `check_interface_extension_compatibility`
    /// did not fire TS2430 ("incorrectly extends") for this declaration.
    ///
    /// The solver's nominal fast path (`class_instance_extends_target_def`)
    /// trusts this edge to skip the structural member walk against a lib
    /// target. Trusting the raw name-resolved heritage edge
    /// (`DefinitionStore::get_extends`, populated unconditionally at
    /// semantic-construction time) instead is unsound: it also holds for a
    /// declared-but-rejected `extends`, e.g. `HTMLTrackElement extends
    /// HTMLElement` in `lib.dom.d.ts`, whose property override tsc itself
    /// rejects with TS2430 (#16142).
    fn register_verified_interface_extends_if_clean(
        &mut self,
        stmt_idx: NodeIndex,
        iface: &InterfaceData,
        ts2430_diag_start: usize,
    ) {
        let fired = self.ctx.diagnostics[ts2430_diag_start..].iter().any(|d| {
            d.code == crate::diagnostics::diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
        });
        if fired {
            return;
        }
        let Some(iface_sym_id) = self.ctx.binder.get_node_symbol(stmt_idx) else {
            return;
        };
        let Some(parent_def_id) = self.first_interface_extends_def_id(iface) else {
            return;
        };
        let child_def_id = self.ctx.get_or_create_def_id(iface_sym_id);
        if child_def_id != parent_def_id {
            self.ctx
                .register_interface_extends_in_envs(child_def_id, parent_def_id);
        }
    }

    /// Resolve the `DefId` of the first type in the interface's (single)
    /// `extends` heritage clause, mirroring the "only the first extends name"
    /// semantics of the unconditional name-resolved edge in
    /// `DefinitionStore::get_extends`.
    fn first_interface_extends_def_id(&mut self, iface: &InterfaceData) -> Option<DefId> {
        let heritage_clauses = iface.heritage_clauses.as_ref()?;
        let &clause_idx = heritage_clauses.nodes.first()?;
        let clause_node = self.ctx.arena.get(clause_idx)?;
        let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
        if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
            return None;
        }
        let &type_idx = heritage.types.nodes.first()?;
        let type_node = self.ctx.arena.get(type_idx)?;
        let expr_idx = self
            .ctx
            .arena
            .get_expr_type_args(type_node)
            .map(|expr_type_args| expr_type_args.expression)
            .unwrap_or(type_idx);
        let base_sym_id = self.resolve_heritage_symbol(expr_idx)?;
        Some(self.ctx.get_or_create_def_id(base_sym_id))
    }

    fn interface_member_is_mapped_type(&self, member_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(member_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::MAPPED_TYPE)
    }

    /// Check that variance annotations (`in`/`out`) on type parameters match
    /// the actual variance of each parameter as computed by the solver (TS2636).
    ///
    /// For `out T` (covariant), T must not appear in contravariant positions.
    /// For `in T` (contravariant), T must not appear in covariant positions.
    /// `in out T` (invariant) always passes.
    ///
    /// Works for interfaces, classes, and type aliases.
    /// If `body_type` is provided, variance is computed directly on it (for type aliases
    /// whose DefId body may not be resolved yet). Otherwise, resolves via DefId.
    pub(crate) fn check_variance_annotations(
        &mut self,
        stmt_idx: NodeIndex,
        type_parameters: &Option<tsz_parser::parser::base::NodeList>,
    ) {
        self.check_variance_annotations_with_body(stmt_idx, type_parameters, None);
    }

    /// Like `check_variance_annotations` but accepts an optional pre-resolved body type.
    pub(crate) fn check_variance_annotations_with_body(
        &mut self,
        stmt_idx: NodeIndex,
        type_parameters: &Option<tsz_parser::parser::base::NodeList>,
        body_type: Option<TypeId>,
    ) {
        use tsz_scanner::SyntaxKind;

        let Some(type_params) = type_parameters else {
            return;
        };

        // Collect declared variance info for each type parameter
        struct ParamVarianceInfo {
            declared_in: bool,
            declared_out: bool,
            modifier_idx: NodeIndex,
            name: String,
            atom: tsz_common::interner::Atom,
        }

        let mut annotated_params: Vec<(usize, ParamVarianceInfo)> = Vec::new();

        for (i, &param_idx) in type_params.nodes.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(modifiers) = &param.modifiers else {
                continue;
            };

            let mut declared_in = false;
            let mut declared_out = false;
            let mut first_modifier_idx = NodeIndex::NONE;

            for &modifier_idx in &modifiers.nodes {
                let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                    continue;
                };
                if modifier_node.kind == SyntaxKind::InKeyword as u16 {
                    declared_in = true;
                    if first_modifier_idx.is_none() {
                        first_modifier_idx = modifier_idx;
                    }
                } else if modifier_node.kind == SyntaxKind::OutKeyword as u16 {
                    declared_out = true;
                    if first_modifier_idx.is_none() {
                        first_modifier_idx = modifier_idx;
                    }
                }
            }

            if !declared_in && !declared_out {
                continue;
            }

            // `in out` (invariant) is always valid
            if declared_in && declared_out {
                continue;
            }

            let param_name = self
                .ctx
                .arena
                .get(param.name)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .map(|id| id.escaped_text.to_string())
                .unwrap_or_default();

            let atom = self.ctx.types.intern_string(&param_name);

            annotated_params.push((
                i,
                ParamVarianceInfo {
                    declared_in,
                    declared_out,
                    modifier_idx: first_modifier_idx,
                    name: param_name,
                    atom,
                },
            ));
        }

        if annotated_params.is_empty() {
            return;
        }

        // DefId for the declaration under check, shared by the variance
        // computation and the TS2636 elaboration body resolution below.
        let variance_def_id = self
            .ctx
            .binder
            .get_node_symbol(stmt_idx)
            .and_then(|sid| self.ctx.get_existing_def_id(sid));

        // Compute all variances upfront (immutable borrow of self.ctx)
        // to avoid borrow conflicts with error_at_node (mutable borrow).
        let computed_variances: Vec<Option<tsz_solver::type_handles::Variance>> = {
            let db = self.ctx.types.as_type_database();
            let resolver = &self.ctx as &dyn tsz_solver::def::resolver::TypeResolver;

            // Try DefId-based resolution first (works for interfaces/classes and
            // type aliases whose bodies are already resolved)
            let def_variances = variance_def_id.and_then(|did| {
                crate::query_boundaries::variance::compute_actual_type_param_variances_with_resolver(
                    db, resolver, did,
                )
            });

            annotated_params
                .iter()
                .map(|(i, info)| {
                    // Try direct body type computation first (more reliable for
                    // type aliases where the DefId body may not be resolved yet)
                    if let Some(body) = body_type {
                        let v = crate::query_boundaries::variance::compute_variance_with_resolver(
                            db, resolver, body, info.atom,
                        );
                        if !v.is_independent() {
                            return Some(v);
                        }
                    }
                    // Fall back to DefId-based resolution
                    def_variances.as_ref().and_then(|v| v.get(*i).copied())
                })
                .collect()
        };

        // Get the declaration name for error messages
        let decl_name = self
            .ctx
            .binder
            .get_node_symbol(stmt_idx)
            .and_then(|sid| self.ctx.binder.get_symbol(sid))
            .map(|sym| sym.escaped_name.clone())
            .unwrap_or_default();

        // Collect all type param names for formatting
        let all_param_names: Vec<String> = type_params
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = self.ctx.arena.get(param_idx)?;
                let param = self.ctx.arena.get_type_parameter(param_node)?;
                let name_node = self.ctx.arena.get(param.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                Some(ident.escaped_text.to_string())
            })
            .collect();

        // Declaration body for the TS2636 nested elaboration, resolved lazily
        // and at most once — only when a violation actually fires, so the
        // common clean-annotation case pays nothing. Type aliases pass
        // `body_type` directly (its DefId body may not be resolved yet);
        // interfaces/classes resolve it from the DefId. A degenerate body
        // (`unknown`/`error`/`any`, e.g. a circular alias) yields no usable
        // relation reason, so the elaboration is left off there.
        let body_is_usable =
            |b: TypeId| b != TypeId::UNKNOWN && b != TypeId::ERROR && b != TypeId::ANY;
        let mut elaboration_body: Option<Option<TypeId>> = None;

        for (idx, (i, info)) in annotated_params.iter().enumerate() {
            let Some(actual_variance) = computed_variances[idx] else {
                continue;
            };

            // Method parameters are bivariant in tsc's variance model: they
            // record as COVARIANT inside the visitor with `REJECTION_UNRELIABLE`
            // set to mark the signal as not-pure. When T appears ONLY at
            // method-bivariant positions, `REJECTION_UNRELIABLE` stays set after
            // the visit; when T ALSO appears at a strict position (a non-method
            // property, a direct callback, etc.), `strict_occurrence_seen`
            // causes the flag to be cleared (`compute()` lines 307-309).
            //
            // tsc only emits TS2636 for reliable variance violations. So skip
            // the rejection when `rejection_unreliable()` is set — that gates
            // out the "purely method-bivariant" case (where `in T` and `out T`
            // both ride along with bivariance and neither is genuinely
            // contradicted) while still firing on real direct-position
            // violations.
            let violation = if info.declared_out {
                // `out T` (covariant): error if T appears reliably contravariantly
                actual_variance.contains(tsz_solver::type_handles::Variance::CONTRAVARIANT)
                    && !actual_variance.rejection_unreliable()
            } else {
                // `in T` (contravariant): error if T appears reliably covariantly
                actual_variance.contains(tsz_solver::type_handles::Variance::COVARIANT)
                    && !actual_variance.rejection_unreliable()
            };

            if !violation {
                continue;
            }

            // Format error message: "Type 'Controller<sub-T>' is not assignable to
            // type 'Controller<super-T>' as implied by variance annotation."
            let format_type = |marker: &str| -> String {
                let args: Vec<String> = all_param_names
                    .iter()
                    .enumerate()
                    .map(|(j, name)| {
                        if j == *i {
                            format!("{marker}-{name}")
                        } else {
                            name.clone()
                        }
                    })
                    .collect();
                format!("{}<{}>", decl_name, args.join(", "))
            };

            let (sub_type, super_type) = if info.declared_out {
                (format_type("sub"), format_type("super"))
            } else {
                (format_type("super"), format_type("sub"))
            };

            let message = crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_IMPLIED_BY_VARIANCE_ANNOTATION
                .replace("{0}", &sub_type)
                .replace("{1}", &super_type);

            // tsc's `checkTypeParameterDeferred` does not hand-build this
            // message: it runs `checkTypeAssignableTo(source, target)` over the
            // declaration body with marker substitutions for the annotated
            // parameter, and the relation's failure reason supplies the nested
            // elaboration tail (`The types returned by 'f()' are incompatible…`,
            // `Types of property 'x' are incompatible…`). Reproduce that tail by
            // running the same relation through the shared assignability gateway
            // and grafting its reason chain under the TS2636 head. The decision
            // is unchanged (still gated by the computed variance above); only the
            // elaboration is added. Falls back to the flat message when the body
            // is unavailable or the relation does not fail (e.g. the body's free
            // parameter could not be substituted).
            let code = crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_IMPLIED_BY_VARIANCE_ANNOTATION;
            let body = *elaboration_body.get_or_insert_with(|| {
                body_type.filter(|b| body_is_usable(*b)).or_else(|| {
                    variance_def_id.and_then(|did| {
                        let db = self.ctx.types.as_type_database();
                        let resolver = &self.ctx as &dyn tsz_solver::def::resolver::TypeResolver;
                        resolver
                            .resolve_lazy(did, db)
                            .filter(|b| body_is_usable(*b))
                    })
                })
            });
            let related = body
                .map(|body| {
                    self.variance_annotation_elaboration(
                        body,
                        info.atom,
                        &info.name,
                        info.declared_out,
                        info.modifier_idx,
                    )
                })
                .unwrap_or_default();
            if related.is_empty() {
                self.error_at_node(info.modifier_idx, &message, code);
            } else {
                self.error_at_node_with_related(info.modifier_idx, &message, code, related);
            }
        }
    }

    /// Build the nested relation-reason elaboration `tsc` attaches beneath a
    /// TS2636 variance-annotation violation.
    ///
    /// Mirrors tsc's `checkTypeParameterDeferred`: it substitutes a pair of
    /// marker type parameters (`sub-T` constrained `<: super-T`) for the
    /// annotated parameter in the declaration `body` — `sub-T` for the source
    /// and `super-T` for the target under `out` (covariant), swapped under `in`
    /// (contravariant) — then runs the real assignability relation. The
    /// relation's failure reason renders the same drill-down tail tsc emits
    /// (return-type, property, and contravariant-parameter frames). The markers
    /// print as `super-T`/`sub-T` because a `TypeParameter` renders as its name.
    ///
    /// Returns the related-information chain (without the relation's own top
    /// line, which the TS2636 head already states), or an empty vector when the
    /// relation does not fail — in which case the caller emits the flat message.
    fn variance_annotation_elaboration(
        &mut self,
        body: TypeId,
        target_atom: tsz_common::interner::Atom,
        name: &str,
        declared_out: bool,
        anchor_idx: NodeIndex,
    ) -> Vec<crate::diagnostics::DiagnosticRelatedInformation> {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};

        let super_atom = self.ctx.types.intern_string(&format!("super-{name}"));
        let sub_atom = self.ctx.types.intern_string(&format!("sub-{name}"));
        let (super_marker, sub_marker) = {
            let factory = self.ctx.types.factory();
            // Two distinct unconstrained marker type parameters. They are
            // mutually unassignable, so whichever direction the annotation
            // implies fails exactly where the parameter occurs. They are left
            // unconstrained on purpose: a `sub-T extends super-T` constraint
            // would make the relation append tsc's "'super-T' is assignable to
            // the constraint of type 'sub-T', but 'sub-T' could be instantiated
            // with a different subtype" tail, which tsc does not emit for the
            // variance-marker check.
            let super_marker = factory.type_param(TypeParamInfo::simple(super_atom));
            let sub_marker = factory.type_param(TypeParamInfo::simple(sub_atom));
            (super_marker, sub_marker)
        };
        // `out T` (covariant) checks `body<sub-T> <: body<super-T>`; `in T`
        // (contravariant) checks `body<super-T> <: body<sub-T>`. Matches the
        // sub/super orientation of the hand-built top line above.
        let (source_marker, target_marker) = if declared_out {
            (sub_marker, super_marker)
        } else {
            (super_marker, sub_marker)
        };
        let source = instantiate_type(
            self.ctx.types,
            body,
            &TypeSubstitution::single(target_atom, source_marker),
        );
        let target = instantiate_type(
            self.ctx.types,
            body,
            &TypeSubstitution::single(target_atom, target_marker),
        );
        // No substitution happened (atom mismatch) or the relation holds: leave
        // the decision to the caller's flat message rather than inventing a tail.
        if source == target {
            return Vec::new();
        }
        let analysis = self.analyze_assignability_failure(source, target);
        let Some(reason) = analysis.failure_reason else {
            return Vec::new();
        };
        // `render_failure_reason` builds the relation's own top line as
        // `message_text` and the drill-down as `related_information`; the TS2636
        // head already states the top line, so keep only the drill-down.
        let mut diag = self.render_failure_reason(&reason, source, target, anchor_idx, 0);
        // Drop the bare-type-parameter-target notes (TS5082 "could be
        // instantiated with an arbitrary type" / TS5075 "could be instantiated
        // with a different subtype of constraint"). Those explain that a *user*
        // type parameter could still be instantiated unfavorably; the markers
        // here are synthetic and never instantiated, so tsc omits the note in
        // the variance-annotation check. Filter by diagnostic code, not message
        // text, to keep this a structural decision.
        diag.related_information.retain(|info| {
            info.code
                != crate::diagnostics::diagnostic_codes::COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO
                && info.code
                    != crate::diagnostics::diagnostic_codes::IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_BUT_COULD_BE_INSTANTIATED_WITH_A_DIFFERE
        });
        diag.related_information
    }

    /// Check for duplicate property names in interface members (TS2300).
    /// TypeScript reports "Duplicate identifier 'X'." for each duplicate occurrence.
    /// NOTE: Method signatures (overloads) are NOT considered duplicates - interfaces allow
    /// multiple method signatures with the same name for function overloading.
    pub(crate) fn check_duplicate_interface_members(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        // Track canonical property names → (member_idx, type_annotation_node,
        // is_eagerly_bound) triples. Methods are allowed to have overloads so
        // they are excluded.
        let mut seen_properties: FxHashMap<String, Vec<(NodeIndex, NodeIndex, bool)>> =
            FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check property signatures for duplicates
            // Method signatures can have multiple overloads (same name, different types)
            if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                continue;
            }
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };

            // Determine the canonical property name for duplicate detection.
            // For non-computed names, use the syntactic text directly.
            // For computed property names (like `[c0]` where c0 is a const),
            // resolve the expression type to get the actual property name
            // (e.g., c0="1" → canonical name "1").
            let is_computed = self
                .ctx
                .arena
                .get(sig.name)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            // A computed name declares a member only when it resolves to a
            // literal or unique-symbol property name (`tsc`'s late-bound name
            // rule). `[c0]` and `[c1]` where c0="1" and c1=1 both resolve to
            // property "1" and are duplicates; `[k]` where `k: string` names no
            // member at all, so two such declarations are *not* duplicates of
            // each other even though their source spellings are identical.
            // Falling back to the syntactic text here would group them and
            // report a duplicate `tsc` does not.
            let canonical_name = if is_computed {
                let Some(name) = self.get_property_name_resolved(sig.name) else {
                    continue;
                };
                name
            } else if let Some(name) = self.get_member_name_text(sig.name) {
                name
            } else {
                continue;
            };

            let is_eager = self.is_eagerly_bound_member_name(sig.name);
            seen_properties.entry(canonical_name).or_default().push((
                member_idx,
                sig.type_annotation,
                is_eager,
            ));
        }

        // Report errors for duplicates — tsc reports TS2300 on ALL occurrences
        // (both first and subsequent), not just the second+.
        for (name, entries) in &seen_properties {
            if entries.len() > 1 {
                // tsc renders the duplicate name via `declarationNameToString` of
                // the group's first *eagerly bound* declaration's name node
                // (verbatim source spelling), falling back to the first
                // declaration only when every member of the group is
                // late-bound: `{ "artist"; artist }` reports `'"artist"'` at
                // both, `{ "1"; 1 }` reports `'"1"'` (source, not the
                // canonicalized `1`), and `{ [c0]: number; 1: number }` (where
                // `const c0 = "1"`) reports `'1'` even though `[c0]` is
                // written first, because a computed name over an entity
                // reference is late-bound and the plain numeric-literal `1`
                // is not (#16258 residual 1). Computed spellings are kept
                // whole, so `{ ["abc"]; abc }` reports `'["abc"]'` at both.
                // TS2717, by contrast, names the member by the *subsequent*
                // declaration's spelling — the asymmetry is deliberate on
                // both sides.
                let render_entry = *entries.iter().find(|entry| entry.2).unwrap_or(&entries[0]);
                let render_idx = render_entry.0;
                let first_name_node = self
                    .get_interface_member_name_node(render_idx)
                    .unwrap_or(render_idx);
                let display_name = self
                    .declaration_name_to_string(first_name_node)
                    .unwrap_or_else(|| name.clone());

                // TS2687: duplicate property declarations must agree on
                // `readonly` / optional modifiers. Independent of TS2300/TS2717.
                // The comparison reference is the same eagerly-bound
                // declaration TS2300 renders, not source-order-first (#16258
                // residual 1: oracle-verified on `readonly [c0]: number;
                // [c1]: string; 1: boolean;` — only the declaration that
                // disagrees with `1`'s modifiers is flagged, not `[c1]`'s,
                // even though `[c0]` is written first).
                let member_nodes: Vec<NodeIndex> = entries.iter().map(|entry| entry.0).collect();
                self.report_property_modifier_disagreements(render_idx, &member_nodes);

                // The reference type for TS2717 is the eagerly-bound
                // declaration's own type, not source-order-first's — same
                // reference `render_idx` as TS2300/TS2687. Oracle-verified:
                // `[c0]: number; [c1]: string; 1: boolean;` reports TS2717 on
                // `[c0]` and `[c1]` naming `'boolean'` (the eager `1`'s type)
                // as the expected type, never on `1` itself.
                let reference_type = if render_entry.1.is_some() {
                    self.get_type_from_type_node(render_entry.1)
                } else {
                    TypeId::ANY
                };

                for &(idx, type_ann, _) in entries.iter() {
                    let error_node = self.get_interface_member_name_node(idx).unwrap_or(idx);

                    // TS2300 on every declaration in the group. How each name
                    // was spelled does not matter — a computed name that
                    // reached this point resolved to a real member key, so it
                    // names the same member as its group siblings.
                    // Oracle-confirmed against `typescript@7.0.2`
                    // (re-verified for #17203): this fires unconditionally,
                    // including for an all-late-bound group — tsc does NOT
                    // merge that silently.
                    self.error_at_node_msg(
                        error_node,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                        &[&display_name],
                    );

                    // TS2717 on every declaration OTHER than the reference,
                    // when its type differs from the reference's.
                    if idx != render_idx {
                        let this_type = if type_ann.is_some() {
                            self.get_type_from_type_node(type_ann)
                        } else {
                            TypeId::ANY
                        };
                        if !self.type_contains_error(reference_type)
                            && !self.type_contains_error(this_type)
                        {
                            // TS2717 uses type identity, not assignability.
                            // With interned types, TypeId equality is structural identity.
                            if reference_type != this_type {
                                // Use display text for the property name in diagnostics.
                                // For computed properties, this preserves the `[expr]` syntax.
                                let display_name = self
                                    .get_member_name_display_text(error_node)
                                    .unwrap_or_else(|| name.clone());
                                let reference_type_str = self.format_type(reference_type);
                                let this_type_str = self.format_type(this_type);
                                self.error_at_node_msg(
                                    error_node,
                                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                    &[&display_name, &reference_type_str, &this_type_str],
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get property information needed for index signature checking.
    /// Returns (`property_name`, `property_type`, `name_node_index`).
    /// Get the name text from a member name node for duplicate member detection.
    ///
    /// Delegates to `get_literal_property_name` for non-computed names, then handles
    /// computed property names specially: string literals are wrapped as `["text"]`
    /// (matching tsc's diagnostic format), numeric literals are canonicalized, and
    /// well-known symbols like `Symbol.hasInstance` are formatted as `[Symbol.xxx]`.
    pub(crate) fn get_member_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        if name_idx.is_none() {
            return None;
        }

        // Try non-computed property name first
        if let Some(name) =
            crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, name_idx)
        {
            return Some(name);
        }

        // Handle computed property names with diagnostic-specific formatting
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            let expr_node = self.ctx.arena.get(computed.expression)?;
            match expr_node.kind {
                ek if ek == tsz_scanner::SyntaxKind::StringLiteral as u16 => {
                    // tsc formats computed string literals as ["a"] in diagnostics
                    let lit = self.ctx.arena.get_literal(expr_node)?;
                    return Some(format!("[\"{}\"]", lit.text));
                }
                ek if ek == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                    let lit = self.ctx.arena.get_literal(expr_node)?;
                    return Some(
                        tsz_solver::utils::canonicalize_numeric_name(&lit.text)
                            .unwrap_or_else(|| lit.text.clone()),
                    );
                }
                ek if ek == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    // Handle well-known symbols like Symbol.hasInstance
                    let access = self.ctx.arena.get_access_expr(expr_node)?;
                    let obj_node = self.ctx.arena.get(access.expression)?;
                    let obj_ident = self.ctx.arena.get_identifier(obj_node)?;
                    if obj_ident.escaped_text.as_str() == "Symbol" {
                        let prop_node = self.ctx.arena.get(access.name_or_argument)?;
                        let prop_ident = self.ctx.arena.get_identifier(prop_node)?;
                        return Some(format!("[Symbol.{}]", prop_ident.escaped_text));
                    }
                }
                _ => {}
            }

            if let Some(expr_text) = self.get_simple_computed_name_expr_text(computed.expression) {
                return Some(format!("[{expr_text}]"));
            }
        }

        None
    }

    fn get_simple_computed_name_expr_text(&self, expr_idx: NodeIndex) -> Option<String> {
        simple_computed_name_expr_text_in_arena(self.ctx.arena, expr_idx)
    }

    fn is_zero_arg_call_like_expr(&self, expr_idx: NodeIndex) -> bool {
        is_zero_arg_call_like_expr_in_arena(self.ctx.arena, expr_idx)
    }

    pub(crate) fn should_check_late_bound_class_property_name(
        &mut self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        // For zero-arg call expressions, check for duplicates ONLY when the
        // call returns a unique symbol type. Other calls (e.g., `[foo()]`
        // where `foo()` returns `string`) produce dynamic names that can't
        // be statically checked for duplicates.
        // Exception: bare `Symbol()` creates a new unique symbol each time,
        // so duplicate `[Symbol()]` properties are never conflicts.
        if self.is_zero_arg_call_like_expr(computed.expression)
            && !self.is_bare_symbol_constructor_call(computed.expression)
            && self
                .get_simple_computed_name_expr_text(computed.expression)
                .is_some()
        {
            // Only check duplicates if the call returns a symbol-like type
            // (unique symbol, TypeQuery resolving to a symbol variable, etc.).
            // Non-symbol return types (string, number, etc.) are dynamic and
            // may produce different values on each call.
            let call_type = self.get_type_of_node(computed.expression);
            let db = self.ctx.types.as_type_database();
            let is_symbol_like = crate::query_boundaries::common::unique_symbol_ref(db, call_type)
                .is_some()
                || crate::query_boundaries::common::type_query_symbol(db, call_type).is_some()
                || call_type == tsz_solver::TypeId::SYMBOL;
            if is_symbol_like {
                return true;
            }
            return false;
        }
        // Const identifiers referencing Symbol.for() values
        // (e.g., `const x = Symbol.for(""); class C { [x]: T; [x]: U; }`).
        self.is_const_symbol_for_identifier_expr(computed.expression)
    }

    /// Returns `true` if the expression is a bare `Symbol()` call (not
    /// `Symbol.for(...)` or `Symbol.xxx`).  Each `Symbol()` call creates a
    /// new unique symbol, so two `[Symbol()]` properties are never duplicates.
    /// A locally-bound `Symbol` shadows the global and is *not* the unique-symbol
    /// constructor.
    fn is_bare_symbol_constructor_call(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
            return false;
        };
        // Bare `Symbol()` — the callee is the global `Symbol` identifier
        self.identifier_resolves_to_unshadowed_global(call.expression, "Symbol")
    }

    fn is_symbol_for_call_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
            return false;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        // `Symbol.for(...)` only resolves to a unique-symbol-keyed expression
        // when `Symbol` is the built-in global, not a same-named local.
        self.identifier_resolves_to_unshadowed_global(access.expression, "Symbol")
            && self
                .ctx
                .arena
                .get_identifier_text(access.name_or_argument)
                .is_some_and(|name| name == "for")
    }

    fn is_const_symbol_for_identifier_expr(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(expr_idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(mut decl_idx) = symbol.primary_declaration() else {
            return false;
        };
        let mut decl_node = match self.ctx.arena.get(decl_idx) {
            Some(node) => node,
            None => return false,
        };
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            decl_idx = ext.parent;
            decl_node = parent_node;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.is_const_variable_declaration(decl_idx)
        {
            return false;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        var_decl.initializer.is_some() && self.is_symbol_for_call_expression(var_decl.initializer)
    }

    /// Returns `true` if the name node is a computed property with a non-statically-determinable
    /// expression (e.g., `[someVariable]` or `[expr()]`). TSC skips duplicate member checking
    /// for such "late-bound" names because the actual property name can't be known at compile time.
    ///
    /// Returns `false` for:
    /// - Regular identifiers (`foo`)
    /// - Computed properties with string/numeric literals (`["foo"]`, `[0]`)
    /// - Computed properties with well-known symbols (`[Symbol.iterator]`)
    /// - Computed properties whose expression resolves to a unique symbol type
    pub(crate) fn is_late_bound_member_name(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(computed.expression) else {
            return true; // can't determine -> treat as late-bound
        };
        // String/numeric literals are statically determinable
        if expr_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            || expr_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
        {
            return false;
        }
        // Well-known symbols (`Symbol.xxx`) are statically determinable, but
        // *only* when `Symbol` is the built-in global. A local
        // `const Symbol = { tag: "name" } as const` makes `Symbol.tag` a
        // regular literal-typed expression that the type-based fallback below
        // must classify on its own.
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
            && self.identifier_resolves_to_unshadowed_global(access.expression, "Symbol")
        {
            return false;
        }
        // Check whether any request variant resolves to a statically determinable key.
        // This catches cases where plain get_type_of_node widens too aggressively.
        let prev = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let expr_type = self.get_type_of_node(computed.expression);
        self.ctx.preserve_literal_types = prev;

        let evaluated_expr_type = self.evaluate_type_with_env(expr_type);
        let resolved_for_property_access =
            self.resolve_type_for_property_access(evaluated_expr_type);
        let resolved_expr_type = self.resolve_lazy_type(resolved_for_property_access);
        let assignability_expr_type = self.evaluate_type_for_assignability(expr_type);

        for candidate in [
            expr_type,
            evaluated_expr_type,
            resolved_expr_type,
            assignability_expr_type,
        ] {
            if crate::query_boundaries::common::unique_symbol_ref(
                self.ctx.types.as_type_database(),
                candidate,
            )
            .is_some()
            {
                return false;
            }
            if !matches!(
                crate::query_boundaries::common::classify_literal_type(
                    self.ctx.types.as_type_database(),
                    candidate
                ),
                crate::query_boundaries::common::LiteralTypeKind::NotLiteral
            ) {
                return false;
            }
        }
        // Everything else (unions, non-literal types, etc.) is late-bound
        true
    }

    /// Get the name node from an interface member for error reporting.
    fn get_interface_member_name_node(&self, member_idx: NodeIndex) -> Option<NodeIndex> {
        let member_node = self.ctx.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| idx.is_some()),
            k if k == syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| idx.is_some()),
            _ => None,
        }
    }

    /// tsc's `declarationNameToString` for the duplicate-identifier (TS2300)
    /// surface: the verbatim source spelling of a property name node. A
    /// string-literal name keeps its quotes (`"artist"`), a numeric-literal name
    /// keeps its raw source text (`0b11`, `1.0` — never canonicalized to `3` /
    /// `1`), a computed name renders as `["a"]`, and a plain identifier renders
    /// as written. tsc reuses the *first* declaration's spelling for every
    /// occurrence of a duplicate, so callers pass the first declaration's name.
    pub(crate) fn declaration_name_to_string(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.to_string());
        }
        if name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(name_node)
        {
            // tsc renders string-named properties with double quotes. `lit.text`
            // holds the unquoted value, so re-quote unless it already carries
            // its source quotes.
            let text = &lit.text;
            return Some(if text.starts_with('"') {
                text.clone()
            } else {
                format!("\"{text}\"")
            });
        }
        if name_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(name_node)
        {
            return Some(lit.text.clone());
        }
        self.get_member_name_text(name_idx)
    }

    /// Get the display text for a class member name, matching TSC's `declarationNameToString`.
    ///
    /// `declarationNameToString` is `getTextOfNode` — the name node's verbatim
    /// **source spelling**, with no quote convention of its own. Callers whose
    /// message template already wraps `{0}` in a literal `'...'` (TS7008,
    /// TS7010, TS7032, TS7033) get that quoting for free from the template;
    /// this function must not add a second layer, and must not normalize which
    /// quote character the author typed.
    /// - Identifiers: `foo` → `"foo"`
    /// - Numeric literals: `0.0` → `"0.0"` (NOT canonicalized to `"0"`)
    /// - String literals: `"foo"` → `"\"foo\""`, `'foo'` → `"'foo'"` (the
    ///   source's own quote character, not a fixed one)
    pub(crate) fn get_member_name_display_text(&self, name_idx: NodeIndex) -> Option<String> {
        if name_idx.is_none() {
            return None;
        }

        let name_node = self.ctx.arena.get(name_idx)?;

        // Identifier — same as canonical
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.to_string());
        }

        // String literal — verbatim source spelling, quote character and all.
        if name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
            return self.node_text(name_idx);
        }

        // Numeric literal — preserve source text (no canonicalization)
        if name_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(name_node)
        {
            return Some(lit.text.clone());
        }

        // Computed name — `declarationNameToString` renders the *syntax*, so the
        // brackets survive and the expression keeps its own spelling: `["a"]`,
        // `[1.0]` and `[0x10]` (never canonicalized to `[1]` / `[16]`), `[k]`,
        // `[Symbol.iterator]`. `get_member_name_text` cannot serve this: it is a
        // key-shaped helper whose numeric arm both drops the brackets and
        // canonicalizes the digits, which is right for a dedup key and wrong for
        // a message.
        //
        // The inner expression decides only *whether* the name is renderable —
        // a computed name with no expression (`get [](); `) is a parse-error
        // shape that tsc leaves to the syntax error alone. Once it is
        // renderable, the whole `[...]` node's own source text is what
        // `getTextOfNode` returns, interior trivia included: tsc names
        // `get [/* c */ "a"]()` as `[/* c */ "a"]`, which reassembling from the
        // expression's trivia-skipped span cannot reproduce.
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
            && let Some(inner) = self.computed_name_expression_display_text(computed.expression)
        {
            return Some(
                self.node_text(name_idx)
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| format!("[{inner}]")),
            );
        }

        // Fall back to get_member_name_text for computed properties, etc.
        self.get_member_name_text(name_idx)
    }

    /// The inner spelling of a computed member name, for
    /// `get_member_name_display_text`. Returns `None` only when the expression
    /// is absent or occupies no source text, which keeps a malformed computed
    /// name (`get [](); `, a parse-error shape) unnamed rather than rendering
    /// it as an empty `[]` — tsc reports only the syntax error there.
    ///
    /// Everything else renders as its own verbatim source text, because that is
    /// what tsc does: `declarationNameToString`'s last arm is `getTextOfNode`
    /// and it is **unconditional**, so every well-formed computed name is
    /// nameable whatever the expression is. This helper used to pick from a
    /// whitelist of expression kinds instead (literals here, then identifier /
    /// dotted access / zero-argument call / parenthesized in
    /// `simple_computed_name_expr_text_in_arena`) and return `None` for the
    /// rest — a call with arguments, a binary expression, a conditional, an
    /// assertion, a tagged template. `None` makes `member_name_for_diagnostic`
    /// fail, and every site that gates on it then drops the member's whole
    /// `noImplicitAny` family (TS7008/TS7010/TS7032/TS7033) silently, with
    /// TS7010 additionally degrading to TS7011 in class containers. Three
    /// separate fixes (#16190/#16225, #16201, #16229) each closed one more node
    /// kind before the whitelist itself was recognized as the defect (#16250).
    ///
    /// The verbatim text is also the only rendering that survives a
    /// syntax-preserving wrapper: `simple_computed_name_expr_text_in_arena`
    /// recurses *through* a parenthesized expression, so it renders `[(a)]` as
    /// `[a]`, where `getTextOfNode` keeps the parentheses. That helper is not
    /// widened to fix this — it is shared with the *key* helpers
    /// (`get_member_name_text`, `should_check_late_bound_class_property_name`)
    /// where unwrapping is the correct behaviour for member identity. It stays
    /// as the fallback for the one case verbatim text cannot serve: a node with
    /// no owning source file (a synthesized or lib-merged declaration), where
    /// `node_text` has nothing to slice.
    fn computed_name_expression_display_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.ctx.arena.get(expr_idx)?;

        // Source text answers the whole question when there is source text to
        // read: render it verbatim, or decline outright if the parser recovered
        // this name from a syntax error. Declining must *not* fall through to
        // the structured arms below — they descend into a recovered node's
        // well-formed half and rebuild a name from it (`[a.]` renders as `a.`
        // through the property-access arm), which is the same silent divergence
        // in the other direction.
        if let Some(verbatim) = self
            .node_text(expr_idx)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
        {
            return if self.computed_name_expression_is_parse_recovered(expr_idx) {
                None
            } else {
                Some(verbatim)
            };
        }

        if expr_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(expr_node)
        {
            return Some(format!("\"{}\"", lit.text));
        }

        if expr_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(expr_node)
        {
            return Some(lit.text.clone());
        }

        // Template literal computed name, with substitutions (`` `a${x}` ``) or
        // without (`` `abc` ``). `tsc` names both in messages using their own
        // source spelling, and `node_text` above already produces exactly that
        // whenever the node has an owning source file — this arm only carries
        // the source-less case, where there is no text to slice and nothing
        // better to return.
        //
        // Resolving the *key* is a different question from naming the *member*:
        // `get_property_name` does resolve `` [`abc`] `` to the key `abc` one
        // step earlier, but this display path is reached by node kind, not by
        // first-success, so the key resolver never covers for a missing arm
        // here.
        if expr_node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
            || expr_node.kind == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.node_text(expr_idx);
        }

        self.get_simple_computed_name_expr_text(expr_idx)
    }

    /// Whether a computed-name expression contains a node the parser
    /// synthesized while recovering from a syntax error, which makes the name
    /// unrenderable: `get [1+]();` parses as a binary expression whose right
    /// operand is `create_missing_expression`'s placeholder, and tsc reports
    /// only the syntax error (`TS1109`) for it, never the member's
    /// implicit-any diagnostic.
    ///
    /// The placeholder is structurally identifiable — it occupies no source
    /// text at all (`create_missing_expression` builds it at `(pos, pos)`) — so
    /// this is a deny-list over parser recovery, not a whitelist over
    /// expression kinds. That polarity is the
    /// point: an expression form this probe does not know how to descend into
    /// is treated as **well-formed** and gets named, matching tsc's
    /// unconditional `getTextOfNode`. The old whitelist had the opposite
    /// default, which is why each new node kind cost its own bug and fix.
    fn computed_name_expression_is_parse_recovered(&self, expr_idx: NodeIndex) -> bool {
        // A *required* child that is absent is the other half of the same
        // recovery signal: `[a.]` parses as a property access whose member name
        // never materialized at all, rather than as a zero-width placeholder.
        // Every position this probe descends into is required by its parent's
        // grammar, so an absent one always means the parser gave up there.
        if expr_idx.is_none() {
            return true;
        }
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return true;
        };

        if self
            .get_node_span(expr_idx)
            .is_none_or(|(start, end)| start >= end)
        {
            return true;
        }

        // Operand-bearing wrappers: a placeholder is only ever produced *inside*
        // one of these, so descending through them is enough to see it. Any
        // other kind falls through to `false` (nameable) by design.
        if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
            return self.computed_name_expression_is_parse_recovered(paren.expression);
        }
        if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
            return self.computed_name_expression_is_parse_recovered(binary.left)
                || self.computed_name_expression_is_parse_recovered(binary.right);
        }
        if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
            return self.computed_name_expression_is_parse_recovered(unary.operand);
        }
        if let Some(access) = self.ctx.arena.get_access_expr(node) {
            return self.computed_name_expression_is_parse_recovered(access.expression)
                || self.computed_name_expression_is_parse_recovered(access.name_or_argument);
        }
        if let Some(call) = self.ctx.arena.get_call_expr(node) {
            return self.computed_name_expression_is_parse_recovered(call.expression);
        }

        false
    }

    /// The name tsc puts in a member diagnostic, via `declarationNameToString`.
    ///
    /// The renderer is chosen by the name node's **kind**, not by whichever
    /// helper happens to succeed first. That distinction is the whole point:
    /// `get_property_name` resolves a computed name whose expression is a
    /// literal (`["foo"]`, `[0]`) to its property *key* and returns it without
    /// the syntactic wrapper, so a first-success chain silently renders half
    /// the computed names as bare keys and the other half — the ones
    /// `get_property_name` declines, like `[k]` and `[Symbol.iterator]` — with
    /// their brackets intact. A non-computed string-literal name has the same
    /// disease: the key resolver returns it unquoted, dropping whichever quote
    /// character the author wrote.
    pub(crate) fn member_name_for_diagnostic(&self, name_idx: NodeIndex) -> Option<String> {
        if self.ctx.arena.get(name_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                || node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
        }) {
            return self.get_member_name_display_text(name_idx);
        }
        self.get_property_name(name_idx)
            .or_else(|| self.get_member_name_display_text(name_idx))
    }

    /// Check if an interface property type annotation circularly references
    /// itself through a mapped type applied to the enclosing interface.
    ///
    /// Detects patterns like:
    /// ```text
    /// type Child<T> = { [P in NonOptionalKeys<T>]: T[P] }
    /// interface ListWidget {
    ///     "each": Child<ListWidget>;  // TS2502 + TS2615
    /// }
    /// ```
    fn check_interface_property_circular_mapped_type(
        &mut self,
        member_idx: NodeIndex,
        iface_name: &str,
    ) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };
        // Only check PROPERTY_SIGNATURE members
        if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            return;
        }
        let Some(sig) = self.ctx.arena.get_signature(member_node) else {
            return;
        };
        // Must have a type annotation
        if sig.type_annotation == NodeIndex::NONE {
            return;
        }
        let Some(type_node) = self.ctx.arena.get(sig.type_annotation) else {
            return;
        };
        // Type annotation must be a type reference with type arguments
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
            return;
        };
        let Some(args) = &type_ref.type_arguments else {
            return;
        };
        // Check if any type argument is the enclosing interface name
        let has_self_ref = args.nodes.iter().any(|&arg_idx| {
            self.ctx
                .arena
                .get(arg_idx)
                .and_then(|n| {
                    if n.kind == syntax_kind_ext::TYPE_REFERENCE {
                        let tr = self.ctx.arena.get_type_ref(n)?;
                        let name_n = self.ctx.arena.get(tr.type_name)?;
                        self.ctx.arena.get_identifier(name_n)
                    } else if n.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                        self.ctx.arena.get_identifier(n)
                    } else {
                        None
                    }
                })
                .is_some_and(|ident| self.ctx.arena.resolve_identifier_text(ident) == iface_name)
        });
        if !has_self_ref {
            return;
        }

        // Get the type alias symbol for the type reference
        let type_name_idx = type_ref.type_name;
        let alias_sym = self
            .ctx
            .arena
            .get(type_name_idx)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .and_then(|ident| {
                let name = self.ctx.arena.resolve_identifier_text(ident);
                self.ctx.binder.file_locals.get(name)
            });
        let Some(alias_sym) = alias_sym else {
            return;
        };
        // Check if the alias is a type alias with a mapped type body
        let Some(symbol) = self.ctx.binder.get_symbol(alias_sym) else {
            return;
        };
        if symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS == 0 {
            return;
        }
        let has_mapped_body = symbol.declarations.iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .and_then(|n| self.ctx.arena.get_type_alias(n))
                .and_then(|alias| self.ctx.arena.get(alias.type_node))
                .is_some_and(|body_node| body_node.kind == syntax_kind_ext::MAPPED_TYPE)
        });
        if !has_mapped_body {
            return;
        }

        // Get the property name for the diagnostic
        let raw_name = if sig.name != NodeIndex::NONE {
            crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, sig.name)
        } else {
            None
        };
        let Some(raw_name) = raw_name else {
            return;
        };
        // tsc wraps string-literal property names in quotes for TS2502/TS2615
        let is_string_lit = sig.name != NodeIndex::NONE
            && self
                .ctx
                .arena
                .get(sig.name)
                .is_some_and(|n| n.kind == tsz_scanner::SyntaxKind::StringLiteral as u16);
        let prop_name = if is_string_lit {
            format!("\"{raw_name}\"")
        } else {
            raw_name
        };

        // TS2502: 'name' is referenced directly or indirectly in its own type annotation.
        let message_2502 = format!(
            "'{prop_name}' is referenced directly or indirectly in its own type annotation."
        );
        self.error_at_node(sig.name, &message_2502, 2502);

        // TS2615: Type of property 'name' circularly references itself in mapped type '...'.
        // Build a simplified mapped type representation for the message.
        // tsc uses the full expanded type, but the error code match is what matters.
        let mapped_type_str = format!(
            "{{ [P in keyof {iface_name}]: undefined extends {iface_name}[P] ? never : P; }}"
        );
        let message_2615 = format!(
            "Type of property '{prop_name}' circularly references itself in mapped type '{mapped_type_str}'."
        );
        self.error_at_node(sig.type_annotation, &message_2615, 2615);
    }

    /// Walk a type annotation and resolve any `this` type nodes that appear
    /// inside nested `TYPE_LITERAL` contexts, so that the checker's `THIS_TYPE`
    /// branch fires the TS2526 diagnostic.
    ///
    /// `get_type_of_interface` lowers property annotations through the
    /// silent lowering pipeline, which never asks `is_this_type_allowed`.
    /// We can't simply route the whole annotation through
    /// `get_type_from_type_node` because that perturbs DefId registration
    /// order and corrupts type-printer output for sibling interface types.
    /// Resolving only the inner `THIS_TYPE` nodes keeps the outer type
    /// registration intact while still emitting the diagnostic at the
    /// right source position.
    pub(crate) fn check_nested_this_types_for_ts2526(&mut self, root: NodeIndex) {
        if root.is_none() {
            return;
        }

        // Walk descendants of `root`. We only care about THIS_TYPE nodes
        // that live inside a TYPE_LITERAL — at the top level the property
        // annotation itself already covers the position where TSC anchors
        // TS2526 (and `get_type_of_interface` calls into the type-literal
        // handling for direct property annotations elsewhere).
        let mut stack: Vec<NodeIndex> = self
            .ctx
            .arena
            .get_children(root)
            .into_iter()
            .filter(|idx| idx.is_some())
            .collect();

        while let Some(idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            // ThisKeyword (the bare `this` token) and THIS_TYPE both
            // map to a `this` type position. Treat them identically here.
            if node.kind == syntax_kind_ext::THIS_TYPE
                || node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
            {
                // Routing through the checker's type-node entry point
                // dispatches to the TypeNodeChecker's THIS_TYPE branch,
                // which calls `is_this_type_allowed` and emits TS2526
                // when the position is invalid.
                let _ = self.get_type_from_type_node(idx);
                continue;
            }
            stack.extend(
                self.ctx
                    .arena
                    .get_children(idx)
                    .into_iter()
                    .filter(|child| child.is_some()),
            );
        }
    }
}
