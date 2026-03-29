//! Core type analysis implementation: qualified name resolution, symbol type computation,
//! type queries, and contextual literal type analysis.

use crate::context::TypingRequest;
use crate::query_boundaries::common::lazy_def_id;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

type TypeParamPushResult = (
    Vec<tsz_solver::TypeParamInfo>,
    Vec<(String, Option<TypeId>, bool)>,
);

impl<'a> CheckerState<'a> {
    fn can_register_evaluated_alias_form(
        &self,
        alias_def_id: tsz_solver::def::DefId,
        type_id: TypeId,
    ) -> bool {
        let mut pending = tsz_solver::visitor::collect_lazy_def_ids(self.ctx.types, type_id);
        if pending.is_empty() {
            return true;
        }

        let mut visited = FxHashSet::default();
        let mut steps = 0usize;
        while let Some(def_id) = pending.pop() {
            if !visited.insert(def_id) {
                continue;
            }
            if def_id == alias_def_id {
                return false;
            }

            let Some(body) = self.ctx.definition_store.get_body(def_id) else {
                return false;
            };

            steps += 1;
            if steps > 64 {
                return false;
            }

            pending.extend(tsz_solver::visitor::collect_lazy_def_ids(
                self.ctx.types,
                body,
            ));
        }

        true
    }

    // Nested generic declarations can be re-evaluated out of context (for example during
    // application-type expansion), so recover the nearest enclosing generic scope when the
    // current type-parameter list is missing its outer captures.
    fn maybe_push_enclosing_type_parameters(
        &mut self,
        type_parameters: &tsz_parser::parser::NodeList,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        let Some(&first_param_idx) = type_parameters.nodes.first() else {
            return Vec::new();
        };

        let mut current = self
            .ctx
            .arena
            .get_extended(first_param_idx)
            .map_or(NodeIndex::NONE, |ext| ext.parent);

        let mut depth = 0;
        while current.is_some() && depth < 64 {
            depth += 1;
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if !current.is_some() {
                break;
            }

            let maybe_enclosing_type_params =
                self.ctx
                    .arena
                    .get(current)
                    .and_then(|parent| match parent.kind {
                        k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                            .ctx
                            .arena
                            .get_interface(parent)
                            .and_then(|iface| iface.type_parameters.clone()),
                        k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                            .ctx
                            .arena
                            .get_type_alias(parent)
                            .and_then(|type_alias| type_alias.type_parameters.clone()),
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::FUNCTION_EXPRESSION
                            || k == syntax_kind_ext::ARROW_FUNCTION =>
                        {
                            self.ctx
                                .arena
                                .get_function(parent)
                                .and_then(|func| func.type_parameters.clone())
                        }
                        k if k == syntax_kind_ext::METHOD_DECLARATION => self
                            .ctx
                            .arena
                            .get_method_decl(parent)
                            .and_then(|method| method.type_parameters.clone()),
                        k if k == syntax_kind_ext::METHOD_SIGNATURE
                            || k == syntax_kind_ext::CALL_SIGNATURE
                            || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                        {
                            self.ctx
                                .arena
                                .get_signature(parent)
                                .and_then(|sig| sig.type_parameters.clone())
                        }
                        k if k == syntax_kind_ext::FUNCTION_TYPE
                            || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
                        {
                            self.ctx
                                .arena
                                .get_function_type(parent)
                                .and_then(|func| func.type_parameters.clone())
                        }
                        _ => None,
                    });

            let Some(enclosing_type_params) = maybe_enclosing_type_params else {
                continue;
            };

            let mut any_missing = false;
            let mut any_present = false;
            for &param_idx in &enclosing_type_params.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(param.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };
                if self
                    .ctx
                    .type_parameter_scope
                    .contains_key(ident.escaped_text.as_str())
                {
                    any_present = true;
                } else {
                    any_missing = true;
                }
            }

            if any_missing && !any_present {
                let (_, updates) = self.push_type_parameters(&Some(enclosing_type_params));
                return updates;
            }
        }

        Vec::new()
    }

    /// Push type parameters from enclosing generic functions/methods for a given
    /// declaration node. Used when computing local type aliases that have no own
    /// type parameters but reference type parameters from an enclosing function.
    ///
    /// For example: `function foo<T>() { type X = T extends string ? T : never; }`
    /// When computing `X`, `T` must be in the type parameter scope.
    pub(crate) fn push_enclosing_type_params_for_node(
        &mut self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: tsz_parser::parser::NodeIndex,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = arena
            .get_extended(node_idx)
            .map_or(tsz_parser::parser::NodeIndex::NONE, |ext| ext.parent);

        let mut all_updates = Vec::new();
        let mut depth = 0;
        while current.is_some() && depth < 64 {
            depth += 1;
            let Some(parent) = arena.get(current) else {
                break;
            };

            let maybe_type_params = match parent.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    arena
                        .get_function(parent)
                        .and_then(|func| func.type_parameters.clone())
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => arena
                    .get_method_decl(parent)
                    .and_then(|method| method.type_parameters.clone()),
                k if k == syntax_kind_ext::METHOD_SIGNATURE
                    || k == syntax_kind_ext::CALL_SIGNATURE
                    || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                {
                    arena
                        .get_signature(parent)
                        .and_then(|sig| sig.type_parameters.clone())
                }
                _ => None,
            };

            if let Some(type_params) = maybe_type_params {
                // Only push if these type params are from the SAME arena as we're using
                // and none of them are already in scope.
                let all_missing = type_params.nodes.iter().all(|&param_idx| {
                    arena
                        .get(param_idx)
                        .and_then(|n| arena.get_type_parameter(n))
                        .and_then(|tp| arena.get(tp.name))
                        .and_then(|n| arena.get_identifier(n))
                        .is_none_or(|ident| {
                            !self
                                .ctx
                                .type_parameter_scope
                                .contains_key(ident.escaped_text.as_str())
                        })
                });
                if all_missing && std::ptr::eq(arena, self.ctx.arena) {
                    let (_, updates) = self.push_type_parameters(&Some(type_params));
                    all_updates.extend(updates);
                }
            }

            current = arena
                .get_extended(current)
                .map_or(tsz_parser::parser::NodeIndex::NONE, |ext| ext.parent);
        }

        all_updates
    }

    /// Resolve a qualified name (A.B.C) to its type.
    ///
    /// This function handles qualified type names like `Namespace.SubType`, `Module.Interface`,
    /// or deeply nested names like `A.B.C`. It resolves each segment and looks up the final member.
    ///
    /// ## Resolution Strategy:
    /// 1. **Recursively resolve left side**: For `A.B.C`, first resolve `A.B`
    /// 2. **Get member type**: Look up rightmost member in left type's exports
    /// 3. **Handle symbol merging**: Supports merged class+namespace, enum+namespace, etc.
    ///
    /// ## Qualified Name Forms:
    /// - `Module.Type` - Type from module
    /// - `Namespace.Interface` - Interface from namespace
    /// - `A.B.C` - Deeply nested qualified name
    /// - `Class.StaticMember` - Static class member
    ///
    /// ## Symbol Resolution:
    /// - Checks exports of left side's symbol
    /// - Handles merged symbols (class+namespace, function+namespace)
    /// - Falls back to property access if not found in exports
    ///
    /// ## Error Reporting:
    /// - TS2694: Namespace has no exported member
    /// - Returns ERROR type if resolution fails
    ///
    /// ## Lib Binders:
    /// - Collects lib binders for cross-arena symbol lookup
    /// - Fixes TS2694 false positives for lib.d.ts types
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Module members
    /// namespace Utils {
    ///   export interface Helper {}
    /// }
    /// let h: Utils.Helper;  // resolve_qualified_name("Utils.Helper")
    ///
    /// // Deep nesting
    /// namespace A {
    ///   export namespace B {
    ///     export interface C {}
    ///   }
    /// }
    /// let x: A.B.C;  // resolve_qualified_name("A.B.C")
    ///
    /// // Static class members
    /// class Container {
    ///   static class Inner {}
    /// }
    /// let y: Container.Inner;  // resolve_qualified_name("Container.Inner")
    ///
    /// // Merged symbols
    /// function Model() {}
    /// namespace Model {
    ///   export interface Options {}
    /// }
    /// let opts: Model.Options;  // resolve_qualified_name("Model.Options")
    /// ```
    pub(crate) fn resolve_qualified_name(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
            return TypeId::ERROR; // Missing qualified name data - propagate error
        };

        let right_name = if let Some(right_node) = self.ctx.arena.get(qn.right) {
            if let Some(id) = self.ctx.arena.get_identifier(right_node) {
                id.escaped_text.clone()
            } else {
                return TypeId::ERROR; // Missing identifier data - propagate error
            }
        } else {
            return TypeId::ERROR; // Missing right node - propagate error
        };

        // Resolve the left side (could be Identifier or another QualifiedName)
        let left_type = if let Some(left_node) = self.ctx.arena.get(qn.left) {
            let left_name = self.entity_name_text(qn.left).unwrap_or_default();

            let sym_res = if left_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                self.resolve_qualified_symbol_in_type_position(qn.left)
            } else if left_node.kind == SyntaxKind::Identifier as u16 {
                self.resolve_identifier_symbol_as_qualified_type_anchor(qn.left)
                    .map(TypeSymbolResolution::Type)
                    .unwrap_or_else(|| self.resolve_identifier_symbol_in_type_position(qn.left))
            } else {
                TypeSymbolResolution::NotFound
            };

            match sym_res {
                TypeSymbolResolution::Type(sym_id) => {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                    {
                        let valid_namespace_flags = symbol_flags::MODULE
                            | symbol_flags::NAMESPACE_MODULE
                            | symbol_flags::VALUE_MODULE
                            | symbol_flags::CLASS
                            | symbol_flags::ENUM
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM
                            | symbol_flags::ENUM_MEMBER;

                        // Skip TS2713 for ALIAS symbols (imports) - they may target
                        // namespaces in other files. Also skip when the resolved
                        // export has an alias_partner (TYPE_ALIAS+ALIAS merge), as
                        // the partner provides namespace access. Skip when parse
                        // errors exist, as the qualified name may be malformed.
                        let is_alias = (symbol.flags & symbol_flags::ALIAS) != 0;
                        let has_alias_partner =
                            self.ctx.binder.alias_partners.contains_key(&sym_id)
                                || self.ctx.binder.resolve_import_symbol(sym_id).is_some_and(
                                    |resolved| {
                                        self.ctx.binder.alias_partners.contains_key(&resolved)
                                    },
                                );
                        if (symbol.flags & valid_namespace_flags) == 0
                            && !is_alias
                            && !has_alias_partner
                            && !self.ctx.has_parse_errors
                        {
                            let right_name = if let Some(right_node) = self.ctx.arena.get(qn.right)
                                && let Some(id) = self.ctx.arena.get_identifier(right_node)
                            {
                                id.escaped_text.clone()
                            } else {
                                String::new()
                            };

                            // Get rightmost name of the left side
                            let left_rightmost_name = if left_node.kind
                                == syntax_kind_ext::QUALIFIED_NAME
                            {
                                if let Some(left_qn) = self.ctx.arena.get_qualified_name(left_node)
                                {
                                    if let Some(rn) = self.ctx.arena.get(left_qn.right)
                                        && let Some(id) = self.ctx.arena.get_identifier(rn)
                                    {
                                        id.escaped_text.clone()
                                    } else {
                                        left_name.clone()
                                    }
                                } else {
                                    left_name.clone()
                                }
                            } else {
                                left_name.clone()
                            };

                            // Determine whether to emit TS2713 or TS2702.
                            // TS2713: the property exists on the type — suggest indexed access.
                            // TS2702: the property does NOT exist — generic "used as namespace" error.
                            //
                            // For type parameters, get_type_of_symbol may not return the
                            // TypeParameter type. Check the type_parameter_scope first.
                            let left_type_id = self
                                .ctx
                                .type_parameter_scope
                                .get(&left_name)
                                .copied()
                                .unwrap_or_else(|| self.get_type_of_symbol(sym_id));
                            let prop_exists =
                                crate::query_boundaries::property_access::type_has_property(
                                    self.ctx.types,
                                    left_type_id,
                                    &right_name,
                                );

                            use crate::diagnostics::diagnostic_codes;
                            if prop_exists {
                                self.error_at_node_msg(
                                    idx, // The entire qualified name node
                                    diagnostic_codes::CANNOT_ACCESS_BECAUSE_IS_A_TYPE_BUT_NOT_A_NAMESPACE_DID_YOU_MEAN_TO_RETRIEVE_THE,
                                    &[left_rightmost_name.as_str(), right_name.as_str()],
                                );
                            } else {
                                self.error_type_used_as_namespace_at(&left_rightmost_name, qn.left);
                            }
                            return TypeId::ERROR;
                        }
                    }
                    self.type_reference_symbol_type(sym_id)
                }
                TypeSymbolResolution::ValueOnly(_) | TypeSymbolResolution::NotFound => {
                    if left_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                        self.resolve_qualified_name(qn.left)
                    } else if left_node.kind == SyntaxKind::Identifier as u16 {
                        // globalThis is a synthetic namespace in TSC (flags = ValueModule | NamespaceModule)
                        // with exports pointing to the global scope. Suppress TS2503 for it.
                        if left_name == "globalThis" {
                            return TypeId::ERROR;
                        }
                        if !self.is_unresolved_import_symbol(qn.left) && !left_name.is_empty() {
                            // Route through boundary for TS2503/TS2552 with suggestions
                            let req = crate::query_boundaries::name_resolution::NameResolutionRequest::namespace(
                                left_name.as_str(),
                                qn.left,
                            );
                            match self.resolve_name_structured(&req) {
                                Err(_failure) => {
                                    // Emit TS2503 (cannot find namespace) with suggestions
                                    self.error_cannot_find_namespace_with_suggestion(
                                        left_name.as_str(),
                                        qn.left,
                                    );
                                }
                                Ok(_) => {
                                    // Shouldn't happen since resolve_qualified_symbol_in_type_position
                                    // already failed, but avoid false diagnostic
                                }
                            }
                        }
                        TypeId::ERROR
                    } else {
                        TypeId::ERROR
                    }
                }
            }
        } else {
            TypeId::ERROR // Missing left node - propagate error
        };

        if left_type == TypeId::ANY || left_type == TypeId::ERROR {
            return TypeId::ERROR; // Propagate error from left side
        }

        // Collect lib binders for cross-arena symbol lookup (fixes TS2694 false positives)
        let lib_binders = self.get_lib_binders();

        // First, try to resolve the left side as a symbol and check its exports.
        // This handles merged class+namespace, function+namespace, and enum+namespace symbols.
        let mut left_sym_for_missing = None;
        let mut left_module_specifier: Option<String> = None;
        let member_sym_id_from_symbol = if let Some(left_node) = self.ctx.arena.get(qn.left)
            && left_node.kind == SyntaxKind::Identifier as u16
        {
            if let Some(sym_id) = self.resolve_identifier_symbol_as_qualified_type_anchor(qn.left) {
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    left_sym_for_missing = Some(sym_id);
                    left_module_specifier = symbol.import_module.clone();
                    let mut result = self.resolve_symbol_export_for(
                        Some(sym_id),
                        symbol,
                        &right_name,
                        &lib_binders,
                    );
                    // TYPE_ALIAS+ALIAS merge: look up alias_partner and
                    // resolve the member through the ALIAS symbol's namespace
                    if result.is_none() {
                        let alias_id = self
                            .ctx
                            .binder
                            .alias_partners
                            .get(&sym_id)
                            .copied()
                            .or_else(|| {
                                let resolved = self.ctx.binder.resolve_import_symbol(sym_id)?;
                                self.ctx.binder.alias_partners.get(&resolved).copied()
                            });
                        if let Some(alias_id) = alias_id
                            && let Some(alias_sym) =
                                self.ctx.binder.get_symbol_with_libs(alias_id, &lib_binders)
                        {
                            result = alias_sym
                                .exports
                                .as_ref()
                                .and_then(|exports| exports.get(&right_name));
                            if result.is_none()
                                && let Some(module) = alias_sym.import_module.as_ref()
                            {
                                // Resolve from ALIAS's source file, then
                                // fall back to current-file resolution.
                                result = self
                                    .ctx
                                    .resolve_alias_import_member(alias_id, module, &right_name)
                                    .or_else(|| {
                                        self.resolve_effective_module_exports(module)
                                            .and_then(|e| e.get(&right_name))
                                    });
                            }
                        }
                    }
                    if result.is_none() {
                        result = self.resolve_named_class_expression_namespace_member(
                            qn.left,
                            sym_id,
                            &right_name,
                        );
                    }
                    result
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            // When the left side is a QualifiedName (e.g., `ns.Root` in `ns.Root.Foo`),
            // extract the module specifier from the root identifier of the chain so that
            // module augmentation merging can be applied to nested members.
            if left_module_specifier.is_none()
                && let Some(left_node) = self.ctx.arena.get(qn.left)
                && left_node.kind == syntax_kind_ext::QUALIFIED_NAME
            {
                left_module_specifier = self.extract_root_module_specifier(qn.left, &lib_binders);
            }
            None
        };

        // If found via symbol resolution, use it
        if let Some(member_sym_id) = member_sym_id_from_symbol {
            if (self.alias_resolves_to_value_only(member_sym_id, Some(right_name.as_str()))
                || self.symbol_is_value_only(member_sym_id, Some(right_name.as_str())))
                && !self.symbol_is_type_only(member_sym_id, Some(right_name.as_str()))
            {
                let full_name = self
                    .entity_name_text(idx)
                    .unwrap_or_else(|| right_name.clone());
                self.report_wrong_meaning(
                    &full_name,
                    idx,
                    member_sym_id,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                );
                return TypeId::ERROR;
            }
            let mut member_type = self.type_reference_symbol_type(member_sym_id);
            if let Some(module_specifier) = left_module_specifier.as_deref() {
                member_type =
                    self.apply_module_augmentations(module_specifier, &right_name, member_type);
            }
            return member_type;
        }

        if let Some(left_sym_id) = left_sym_for_missing
            && let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym_id, &lib_binders)
            && symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::CLASS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM
                    | symbol_flags::INTERFACE)
                != 0
        {
            // If the left symbol is a pure interface (no namespace meaning) and a
            // local declaration shadows an outer namespace, the member might exist
            // on the outer namespace. In tsc, import-equals and qualified type names
            // prefer namespace meaning, so a local interface shouldn't cause TS2694
            // when an outer namespace with the same name has the member.
            let is_pure_interface = (symbol.flags & symbol_flags::INTERFACE) != 0
                && (symbol.flags & symbol_flags::MODULE) == 0
                && (symbol.flags & symbol_flags::CLASS) == 0
                && (symbol.flags & symbol_flags::REGULAR_ENUM) == 0
                && (symbol.flags & symbol_flags::CONST_ENUM) == 0;
            if is_pure_interface {
                // Check if an outer namespace with this name has the member
                let left_name_str = self
                    .entity_name_text(qn.left)
                    .unwrap_or_else(|| symbol.escaped_name.clone());
                if self
                    .resolve_outer_namespace_member(qn.left, &left_name_str, &right_name)
                    .is_some()
                {
                    // The member exists on an outer namespace — don't emit TS2694.
                    // Return ERROR type since we can't resolve through the local interface,
                    // but avoid the misleading diagnostic.
                    return TypeId::ERROR;
                }
            }

            let export_names: Vec<String> = symbol
                .exports
                .as_ref()
                .map(|e| e.iter().map(|(name, _)| name.clone()).collect())
                .unwrap_or_default();
            let req =
                crate::query_boundaries::name_resolution::NameResolutionRequest::exported_member(
                    &right_name,
                    qn.right,
                    left_sym_id,
                    export_names,
                );
            let failure = match self.resolve_name_structured(&req) {
                Err(f) => f,
                Ok(_) => {
                    // Shouldn't happen since we already failed above, but be safe
                    return TypeId::ERROR;
                }
            };
            self.report_name_resolution_failure(&req, &failure);
            return TypeId::ERROR;
        }

        // Otherwise, fall back to type-based lookup for pure namespace/module types
        // Look up the member in the left side's exports
        // Supports both Lazy(DefId) and Enum types
        let fallback_sym_id = self.ctx.resolve_type_to_symbol_id(left_type);

        if let Some(fallback_sym) = fallback_sym_id
            && let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(fallback_sym, &lib_binders)
        {
            // Use the helper to resolve the member from exports, members, or re-exports
            if let Some(member_sym_id) = self.resolve_symbol_export_for(
                Some(fallback_sym),
                symbol,
                &right_name,
                &lib_binders,
            ) {
                // Check value-only, but skip for namespaces since they can be used
                // to navigate to types (e.g., Outer.Inner.Type)
                if let Some(member_symbol) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(member_sym_id, &lib_binders)
                {
                    let is_namespace = member_symbol.flags & symbol_flags::MODULE != 0;
                    if !is_namespace
                        && (self
                            .alias_resolves_to_value_only(member_sym_id, Some(right_name.as_str()))
                            || self.symbol_is_value_only(member_sym_id, Some(right_name.as_str())))
                        && !self.symbol_is_type_only(member_sym_id, Some(right_name.as_str()))
                    {
                        let full_name = self
                            .entity_name_text(idx)
                            .unwrap_or_else(|| right_name.clone());
                        self.report_wrong_meaning(
                            &full_name,
                            idx,
                            member_sym_id,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        );
                        return TypeId::ERROR;
                    }
                }
                let mut member_type = self.type_reference_symbol_type(member_sym_id);
                if let Some(module_specifier) = left_module_specifier.as_deref() {
                    member_type =
                        self.apply_module_augmentations(module_specifier, &right_name, member_type);
                }
                return member_type;
            }

            // Not found - report TS2694 or TS2724 (with spelling suggestion)
            let export_names: Vec<String> = symbol
                .exports
                .as_ref()
                .map(|e| e.iter().map(|(name, _)| name.clone()).collect())
                .unwrap_or_default();
            let req =
                crate::query_boundaries::name_resolution::NameResolutionRequest::exported_member(
                    &right_name,
                    qn.right,
                    fallback_sym,
                    export_names,
                );
            let failure = match self.resolve_name_structured(&req) {
                Err(f) => f,
                Ok(_) => {
                    return TypeId::ERROR;
                }
            };
            self.report_name_resolution_failure(&req, &failure);
            return TypeId::ERROR;
        }

        // Left side wasn't a reference to a namespace/module
        // This is likely an error - the left side should resolve to a namespace
        // Emit an appropriate error for the unresolved qualified name
        // We don't emit TS2304 here because the left side might have already emitted an error
        // Returning ERROR prevents cascading errors while still indicating failure
        if let Some(left_node) = self.ctx.arena.get(qn.left)
            && left_node.kind == SyntaxKind::Identifier as u16
            && !self.is_unresolved_import_symbol(qn.left)
            && let Some(ident) = self.ctx.arena.get_identifier(left_node)
        {
            self.error_cannot_find_namespace_with_suggestion(ident.escaped_text.as_str(), qn.left);
        }
        TypeId::ERROR
    }

    /// Walk a qualified-name chain leftward to find the root identifier and return
    /// its `import_module` (module specifier), if any.  This is used to propagate
    /// module augmentation context through nested qualified names like `ns.Root.Foo`.
    pub(crate) fn extract_root_module_specifier(
        &self,
        mut idx: NodeIndex,
        lib_binders: &[std::sync::Arc<tsz_binder::BinderState>],
    ) -> Option<String> {
        loop {
            let node = self.ctx.arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qn = self.ctx.arena.get_qualified_name(node)?;
                idx = qn.left;
            } else if node.kind == SyntaxKind::Identifier as u16 {
                let sym_id = self.resolve_identifier_symbol_as_qualified_type_anchor(idx)?;
                let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders)?;
                return symbol.import_module.clone();
            } else {
                return None;
            }
        }
    }

    /// When a named class expression shadows an outer namespace of the same name,
    /// qualified type names like `C.Member` still resolve through the namespace.
    fn resolve_named_class_expression_namespace_member(
        &self,
        node: NodeIndex,
        sym_id: SymbolId,
        member_name: &str,
    ) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if (symbol.flags & symbol_flags::CLASS) == 0 {
            return None;
        }

        let decl_kind = self
            .ctx
            .arena
            .get(symbol.value_declaration)
            .map(|decl| decl.kind)?;
        if decl_kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }

        self.resolve_outer_namespace_member(node, symbol.escaped_name.as_str(), member_name)
    }

    /// Resolve a member from an outer namespace with the same name.
    /// Used to avoid false TS2694 when a local declaration shadows an outer namespace.
    fn resolve_outer_namespace_member(
        &self,
        node: NodeIndex,
        namespace_name: &str,
        member_name: &str,
    ) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        // Walk up scopes from the enclosing scope's parent
        let Some(scope_id) = self.ctx.binder.find_enclosing_scope(self.ctx.arena, node) else {
            return self.resolve_namespace_member_from_all_binders(namespace_name, member_name);
        };
        let Some(current_scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
            return self.resolve_namespace_member_from_all_binders(namespace_name, member_name);
        };
        let mut walk_id = current_scope.parent;

        while let Some(scope) = self.ctx.binder.scopes.get(walk_id.0 as usize) {
            if let Some(sym_id) = scope.table.get(namespace_name)
                && let Some(sym) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                && (sym.flags & symbol_flags::NAMESPACE) != 0
            {
                // Found a namespace - check if it has the member
                if let Some(exports) = sym.exports.as_ref()
                    && let Some(member_id) = exports.get(member_name)
                {
                    return Some(member_id);
                }
            }
            if walk_id == scope.parent {
                break;
            }
            walk_id = scope.parent;
        }

        self.resolve_namespace_member_from_all_binders(namespace_name, member_name)
    }

    /// Resolve a member from a symbol's exports, members, or re-exports.
    ///
    /// This helper implements the common pattern of looking up a member in:
    /// 1. Direct exports
    /// 2. Members (for classes with static members)
    /// 3. Re-exports (for imported namespaces)
    ///
    /// Returns `Some(member_sym_id)` if found, `None` otherwise.
    fn resolve_symbol_export_for(
        &mut self,
        sym_id: Option<tsz_binder::SymbolId>,
        symbol: &tsz_binder::Symbol,
        member_name: &str,
        lib_binders: &[std::sync::Arc<tsz_binder::BinderState>],
    ) -> Option<tsz_binder::SymbolId> {
        // Try direct exports first
        if let Some(ref exports) = symbol.exports
            && let Some(member_id) = exports.get(member_name)
        {
            return Some(member_id);
        }

        // For classes, also check members (for static members in type queries)
        // This handles `typeof C.staticMember` where C is a class
        if symbol.flags & symbol_flags::CLASS != 0
            && let Some(ref members) = symbol.members
            && let Some(member_id) = members.get(member_name)
        {
            return Some(member_id);
        }

        if symbol.flags & symbol_flags::MODULE != 0 {
            if let Some(member_id) =
                self.resolve_module_export_from_declarations(symbol, member_name)
            {
                return Some(member_id);
            }
            // Only the anonymous source-file module should see top-level file exports
            // through `file_locals`. Named namespaces/modules must resolve members
            // through their own declaration-local export tables; otherwise `X.bar`
            // can accidentally bind to an unrelated `export function bar()` from the
            // containing file and surface TS2749 instead of TS2694.
            let has_named_module_declaration = symbol.declarations.iter().any(|&decl_idx| {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    return false;
                };
                if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                    return false;
                }
                self.ctx
                    .arena
                    .get_module(node)
                    .and_then(|module| self.ctx.arena.get(module.name))
                    .is_some_and(|name_node| name_node.kind == SyntaxKind::Identifier as u16)
            });
            if !has_named_module_declaration
                && let Some(local_sym_id) = self.ctx.binder.file_locals.get(member_name)
                && let Some(sym) = self.ctx.binder.get_symbol(local_sym_id)
                && sym.is_exported
            {
                return Some(local_sym_id);
            }
        }

        // If not found in direct exports, check for re-exports
        // The member might be re-exported from another module
        if let Some(ref module_specifier) = symbol.import_module {
            if (symbol.flags & symbol_flags::ALIAS) != 0
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                return None;
            }
            if let Some(reexported_sym_id) =
                self.resolve_reexported_member(module_specifier, member_name, lib_binders)
            {
                return Some(reexported_sym_id);
            }
            // Cross-file fallback: resolve the relative module specifier from
            // the ALIAS symbol's source file perspective.
            if let Some(alias_id) = sym_id
                && let Some(resolved) =
                    self.ctx
                        .resolve_alias_import_member(alias_id, module_specifier, member_name)
            {
                return Some(resolved);
            }
        }

        None
    }

    fn resolve_module_export_from_declarations(
        &self,
        symbol: &tsz_binder::Symbol,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module) = self.ctx.arena.get_module(node) else {
                continue;
            };
            if module.body.is_none() {
                continue;
            }
            if let Some(&scope_id) = self.ctx.binder.node_scope_ids.get(&module.body.0)
                && let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize)
                && let Some(sym_id) = scope.table.get(member_name)
                && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
                && sym.is_exported
            {
                return Some(sym_id);
            }
            let Some(module_block) = self.ctx.arena.get_module_block_at(module.body) else {
                continue;
            };
            let Some(statements) = &module_block.statements else {
                continue;
            };

            for &stmt_idx in &statements.nodes {
                let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                    continue;
                };
                if (stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION)
                    && let Some(name) = self.get_declaration_name_text(stmt_idx)
                    && name == member_name
                    && let Some(&sym_id) = self.ctx.binder.node_symbols.get(&stmt_idx.0)
                {
                    return Some(sym_id);
                }
                if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    continue;
                }
                let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if export_decl.export_clause.is_none() {
                    continue;
                }
                let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
                    continue;
                };

                match clause_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::INTERFACE_DECLARATION
                    | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    | syntax_kind_ext::ENUM_DECLARATION
                    | syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(name) =
                            self.get_declaration_name_text(export_decl.export_clause)
                            && name == member_name
                            && let Some(&sym_id) = self
                                .ctx
                                .binder
                                .node_symbols
                                .get(&export_decl.export_clause.0)
                        {
                            return Some(sym_id);
                        }
                    }
                    syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.ctx.arena.get_variable(clause_node) {
                            // VariableStatement holds VariableDeclarationList nodes.
                            // Walk list -> declaration to recover exported namespace vars.
                            for &list_idx in &var_stmt.declarations.nodes {
                                let Some(list_node) = self.ctx.arena.get(list_idx) else {
                                    continue;
                                };
                                let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                                    continue;
                                };
                                for &decl_idx in &decl_list.declarations.nodes {
                                    if let Some(name) = self.get_declaration_name_text(decl_idx)
                                        && name == member_name
                                        && let Some(&sym_id) =
                                            self.ctx.binder.node_symbols.get(&decl_idx.0)
                                    {
                                        return Some(sym_id);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Get type from a union type node (A | B).
    ///
    /// Parses a union type expression and creates a Union type with all members.
    ///
    /// ## Type Normalization:
    /// - Empty union → NEVER (the empty type)
    /// - Single member → the member itself (no union wrapper)
    /// - Multiple members → Union type with all members
    ///
    /// ## Member Resolution:
    /// - Each member is resolved via `get_type_from_type_node`
    /// - This handles nested typeof expressions and type references
    /// - Type arguments are recursively resolved
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// type StringOrNumber = string | number;
    /// // Creates Union(STRING, NUMBER)
    ///
    /// type ThreeTypes = string | number | boolean;
    /// // Creates Union(STRING, NUMBER, BOOLEAN)
    ///
    /// type Nested = (string | number) | boolean;
    /// // Normalized to Union(STRING, NUMBER, BOOLEAN)
    /// ```
    #[allow(dead_code)]
    pub(super) fn is_type_query_in_non_flow_sensitive_signature_parameter(
        &self,
        idx: NodeIndex,
    ) -> bool {
        let mut current = idx;
        let mut saw_parameter = false;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }

            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };

            match parent_node.kind {
                syntax_kind_ext::PARAMETER => saw_parameter = true,
                k if k == syntax_kind_ext::CALL_SIGNATURE
                    || k == syntax_kind_ext::CONSTRUCT_SIGNATURE
                    || k == syntax_kind_ext::METHOD_SIGNATURE
                    || k == syntax_kind_ext::FUNCTION_TYPE
                    || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
                {
                    return saw_parameter;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    return false;
                }
                _ => {}
            }

            current = parent;
        }

        false
    }

    /// Get type from a type query node (typeof X).
    ///
    /// Resolves value symbols, emits TS2504 for type-only symbols, handles
    /// unknown identifiers and missing members. Supports type arguments.
    ///
    /// Resolve a qualified name chain as a value property access chain
    /// for `typeof` context. Recurses through nested `QualifiedName` nodes
    /// so that `typeof a.b.c` resolves `a` as a value, then `.b`, then `.c`.
    #[allow(dead_code)]
    pub(super) fn resolve_typeof_qualified_value_chain(
        &mut self,
        idx: NodeIndex,
        use_flow: bool,
    ) -> TypeId {
        self.resolve_typeof_qualified_value_chain_with_request(idx, &TypingRequest::NONE, use_flow)
    }

    pub(super) fn resolve_typeof_qualified_value_chain_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
        use_flow: bool,
    ) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                return TypeId::ERROR;
            };
            let left_type =
                self.resolve_typeof_qualified_value_chain_with_request(qn.left, request, use_flow);
            if left_type == TypeId::ANY || left_type == TypeId::ERROR {
                return left_type;
            }
            if let Some(rn) = self.ctx.arena.get(qn.right)
                && let Some(ident) = self.ctx.arena.get_identifier(rn)
            {
                let object_type = self.resolve_type_for_property_access(left_type);
                if object_type == TypeId::ANY || object_type == TypeId::ERROR {
                    return object_type;
                }
                let (object_type_for_access, nullish_cause) = self.split_nullish_type(object_type);
                let Some(object_type_for_access) = object_type_for_access else {
                    if let Some(cause) = nullish_cause {
                        self.report_nullish_object(qn.left, cause, true);
                    }
                    return TypeId::ERROR;
                };
                if let Some(cause) = nullish_cause {
                    self.report_nullish_object(qn.left, cause, false);
                }
                use crate::query_boundaries::common::PropertyAccessResult;
                match self
                    .resolve_property_access_with_env(object_type_for_access, &ident.escaped_text)
                {
                    PropertyAccessResult::Success { type_id, .. } => {
                        let resolved = self.resolve_type_query_type(type_id);
                        if use_flow {
                            self.apply_flow_narrowing(idx, resolved)
                        } else {
                            resolved
                        }
                    }
                    _ => TypeId::ERROR,
                }
            } else {
                TypeId::ERROR
            }
        } else {
            // Base case: identifier or other expression — resolve as value
            let expr_request = if use_flow {
                request.read().contextual_opt(None)
            } else {
                request.write().contextual_opt(None)
            };
            self.get_type_of_node_with_request(idx, &expr_request)
        }
    }

    #[allow(dead_code)]
    pub(super) fn resolve_type_query_import_type_symbol(&self, idx: NodeIndex) -> Option<u32> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let local_sym_id = self.resolve_identifier_symbol(idx)?;
        if !self.alias_resolves_to_type_only(local_sym_id) {
            return None;
        }

        match self.resolve_identifier_symbol_in_type_position_without_tracking(idx) {
            TypeSymbolResolution::Type(sym_id) | TypeSymbolResolution::ValueOnly(sym_id) => {
                Some(sym_id.0)
            }
            TypeSymbolResolution::NotFound => Some(local_sym_id.0),
        }
    }

    /// Push type parameters into scope for generic type resolution.
    ///
    /// This is a critical function for handling generic types (classes, interfaces,
    /// functions, type aliases). It makes type parameters available for use within
    /// the generic type's body and returns information for later scope restoration.
    ///
    /// ## Two-Pass Algorithm:
    /// 1. **First pass**: Adds all type parameters to scope WITHOUT constraints
    ///    - This allows self-referential constraints like `T extends Box<T>`
    ///    - Creates unconstrained `TypeParameter` entries
    /// 2. **Second pass**: Resolves constraints and defaults with all params in scope
    ///    - Now all type parameters are visible for constraint resolution
    ///    - Updates the scope with constrained `TypeParameter` entries
    ///
    /// ## Returns:
    /// - `Vec<TypeParamInfo>`: Type parameter info with constraints and defaults
    /// - `Vec<(String, Option<TypeId>)>`: Restoration data for `pop_type_parameters`
    ///
    /// ## Constraint Validation:
    /// - Emits TS2315 if constraint type is error
    /// - Emits TS2314 if default doesn't satisfy constraint
    /// - Uses UNKNOWN for invalid constraints
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Simple type parameter
    /// function identity<T>(value: T): T { return value; }
    /// // push_type_parameters adds T to scope
    ///
    /// // Type parameter with constraint
    /// interface Comparable<T> {
    ///   compare(other: T): number;
    /// }
    /// function max<T extends Comparable<T>>(a: T, b: T): T {
    ///   // T is in scope with constraint Comparable<T>
    ///   return a.compare(b) > 0 ? a : b;
    /// }
    ///
    /// // Type parameter with default
    /// interface Box<T = string> {
    ///   value: T;
    /// }
    /// // T has default type string
    ///
    /// // Self-referential constraint (requires two-pass algorithm)
    /// type Box<T extends Box<T>> = T;
    /// // First pass: T added to scope unconstrained
    /// // Second pass: Constraint Box<T> resolved (T now in scope)
    ///
    /// // Multiple type parameters
    /// interface Map<K, V> {
    ///   get(key: K): V | undefined;
    ///   set(key: K, value: V): void;
    /// }
    /// ```
    pub(crate) fn push_type_parameters(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> TypeParamPushResult {
        let Some(list) = type_parameters else {
            return (Vec::new(), Vec::new());
        };

        // Recursion depth check: prevent stack overflow from circular type parameter
        // references (e.g. interface I<T extends I<T>> {} or circular generic defaults)
        if !self.ctx.enter_recursion() {
            return (Vec::new(), Vec::new());
        }

        let mut updates = self.maybe_push_enclosing_type_parameters(list);
        let mut params = Vec::new();
        let mut param_indices = Vec::new();
        let mut seen_names = FxHashSet::default();

        // First pass: Add all type parameters to scope WITHOUT resolving constraints
        // This allows self-referential constraints like T extends Box<T>
        let factory = self.ctx.types.factory();

        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());

            // Check for duplicate type parameter names (TS2300)
            if !seen_names.insert(name.clone()) {
                self.error_at_node_msg(
                    data.name,
                    crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[&name],
                );
            }

            // Check for reserved type names (TS2368)
            self.check_type_name_is_reserved(data.name, &name);

            let atom = self.ctx.types.intern_string(&name);

            // Create unconstrained type parameter initially
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            };
            let mut shadowed_class_param = false;
            if let Some(ref mut c) = self.ctx.enclosing_class
                && let Some(pos) = c.type_param_names.iter().position(|x| *x == name)
            {
                c.type_param_names.remove(pos);
                shadowed_class_param = true;
            }

            let type_id = factory.type_param(info);
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous, shadowed_class_param));
            param_indices.push(param_idx);
        }

        // Second pass: iteratively refine constraints/defaults against the evolving scope.
        // A single forward pass leaves transitive chains like `T extends U, U extends V`
        // pointing at the original unconstrained placeholders. Re-resolving until the
        // scope stabilizes preserves the full local constraint graph.
        let max_refinement_passes = param_indices.len().max(1);
        for _ in 0..max_refinement_passes {
            let mut changed = false;
            let mut next_params = Vec::with_capacity(param_indices.len());

            for &param_idx in &param_indices {
                let Some(node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                    continue;
                };

                let name = self
                    .ctx
                    .arena
                    .get(data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
                let atom = self.ctx.types.intern_string(&name);

                let constraint = if data.constraint != NodeIndex::NONE {
                    let constraint_type = self.get_type_from_type_node(data.constraint);
                    let is_circular =
                        if let Some(&param_type_id) = self.ctx.type_parameter_scope.get(&name) {
                            self.is_same_type_parameter(constraint_type, param_type_id, &name)
                        } else {
                            false
                        };

                    if is_circular {
                        self.error_at_node_msg(
                            data.constraint,
                            crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT,
                            &[&name],
                        );
                        Some(TypeId::UNKNOWN)
                    } else {
                        Some(constraint_type)
                    }
                } else {
                    None
                };

                let default = if data.default != NodeIndex::NONE {
                    let default_type = self.get_type_from_type_node(data.default);
                    (default_type != TypeId::ERROR).then_some(default_type)
                } else {
                    None
                };

                let is_const = self
                    .ctx
                    .arena
                    .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint,
                    default,
                    is_const,
                };

                let constrained_type_id = factory.type_param(info);
                if self.ctx.type_parameter_scope.get(&name).copied() != Some(constrained_type_id) {
                    self.ctx
                        .type_parameter_scope
                        .insert(name.clone(), constrained_type_id);
                    changed = true;
                }
                next_params.push(info);
            }

            params = next_params;
            if !changed {
                break;
            }
        }

        // Third pass: Detect indirect circular constraints (e.g., T extends U, U extends T)
        // Build a constraint graph among type parameters in this list and detect cycles.
        self.check_indirect_circular_constraints(&params, &param_indices);

        // Validate defaults against the stabilized constraints.
        for (&param_idx, param) in param_indices.iter().zip(params.iter()) {
            let Some(default_type) = param.default else {
                continue;
            };
            let Some(constraint_type) = param.constraint else {
                continue;
            };
            let constraint_has_type_params =
                crate::query_boundaries::checkers::generic::contains_type_parameters(
                    self.ctx.types,
                    constraint_type,
                );
            let default_has_type_params =
                crate::query_boundaries::checkers::generic::contains_type_parameters(
                    self.ctx.types,
                    default_type,
                );
            if constraint_has_type_params
                || default_has_type_params
                || !self.is_assignable_to(default_type, constraint_type)
            {
                if constraint_has_type_params || default_has_type_params {
                    continue;
                }
                let Some(node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                    continue;
                };
                let type_str = self.format_type(default_type);
                let constraint_str = self.format_type(constraint_type);
                self.error_at_node_msg(
                    data.default,
                    crate::diagnostics::diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                    &[&type_str, &constraint_str],
                );
            }
        }

        self.ctx.leave_recursion();
        (params, updates)
    }

    /// Detect indirect circular constraints among type parameters.
    ///
    /// For each type parameter, if its constraint is another type parameter in the same
    /// list, follow the chain. If we reach the original parameter, emit TS2313.
    /// Direct self-references (T extends T) are already caught in the second pass.
    /// Get type of a symbol with caching and circular reference detection.
    ///
    /// This is the main entry point for resolving the type of symbols (variables,
    /// functions, classes, interfaces, type aliases, etc.). All type resolution
    /// ultimately flows through this function.
    ///
    /// ## Caching:
    /// - Symbol types are cached in `ctx.symbol_types` by symbol ID
    /// - Subsequent calls for the same symbol return the cached type
    /// - Cache is populated on first successful resolution
    ///
    /// ## Fuel Management:
    /// - Consumes fuel on each call to prevent infinite loops
    /// - Returns ERROR if fuel is exhausted (prevents type checker timeout)
    ///
    /// ## Circular Reference Detection:
    /// - Tracks currently resolving symbols in `ctx.symbol_resolution_set`
    /// - Returns ERROR if a circular reference is detected
    /// - Uses a stack to track resolution depth
    ///
    /// ## Type Environment Population:
    /// - After resolution, populates the type environment for generic type expansion
    /// - For classes: Handles instance type with type parameters specially
    /// - For generic types: Stores both the type and its type parameters
    /// - Skips ANY/ERROR types (don't populate environment for errors)
    ///
    /// ## Symbol Dependency Tracking:
    /// - Records symbol dependencies for incremental type checking
    /// - Pushes/pops from dependency stack during resolution
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// let x = 42;              // get_type_of_symbol(x) → number
    /// function foo(): void {}  // get_type_of_symbol(foo) → () => void
    /// class C {}               // get_type_of_symbol(C) → typeof C (constructor)
    /// interface I {}           // get_type_of_symbol(I) → I (interface type)
    /// type T = string;         // get_type_of_symbol(T) → string
    /// ```
    pub fn get_type_of_symbol(&mut self, sym_id: SymbolId) -> TypeId {
        // Hard stack guard: bail with ERROR when the stack overflow breaker
        // has been tripped by a previous deep recursion.
        if crate::checkers_domain::stack_overflow_tripped() {
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR;
        }
        // Dynamically grow the stack for deeply recursive symbol resolution
        // chains. Replaces the previous amortized-probe approach which could
        // miss rapid stack consumption in type-level libraries.
        stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.get_type_of_symbol_inner(sym_id)
        })
    }

    fn get_type_of_symbol_inner(&mut self, sym_id: SymbolId) -> TypeId {
        use tsz_solver::SymbolRef;
        let factory = self.ctx.types.factory();
        self.record_symbol_dependency(sym_id);

        // Check cache first
        if let Some(&cached) = self.ctx.symbol_types.get(&sym_id) {
            if cached == TypeId::ERROR && self.ctx.symbol_resolution_set.contains(&sym_id) {
                // Pre-cache ANY sentinel to prevent re-entrancy: provisional_circular_function_symbol_type
                // processes type annotations which may call get_type_of_symbol for the same symbol
                // (e.g., `typeof foo<T>` in foo's own return type). Without this sentinel, the re-entrant
                // call finds ERROR, detects circularity, and calls provisional again → stack overflow.
                self.ctx.symbol_types.insert(sym_id, TypeId::ANY);
                if let Some(provisional) = self.provisional_circular_function_symbol_type(sym_id) {
                    self.ctx.symbol_types.insert(sym_id, provisional);
                    trace!(
                        sym_id = sym_id.0,
                        type_id = provisional.0,
                        file = self.ctx.file_name.as_str(),
                        "(cached provisional) get_type_of_symbol"
                    );
                    return provisional;
                }
                // Restore ERROR if provisional failed
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            }
            let cached = self
                .ctx
                .symbol_types
                .get(&sym_id)
                .copied()
                .unwrap_or(TypeId::ERROR);
            trace!(
                sym_id = sym_id.0,
                type_id = cached.0,
                file = self.ctx.file_name.as_str(),
                "(cached) get_type_of_symbol"
            );
            return cached;
        }

        // Check fuel - return ERROR if exhausted to prevent timeout
        if !self.ctx.consume_fuel() {
            // Cache ERROR so we don't keep trying to resolve this symbol
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR;
        }

        // Check for circular reference
        if self.ctx.symbol_resolution_set.contains(&sym_id) {
            // CRITICAL: For named entities (Interface, Class, TypeAlias, Enum), return Lazy placeholder
            // instead of ERROR. This allows circular dependencies to work correctly.
            //
            // For example: `interface User { filtered: Filtered } type Filtered = { [K in keyof User]: ... }`
            // When Filtered evaluates `keyof User` and User is still being checked, we return Lazy(User)
            // instead of ERROR, allowing the type system to defer evaluation.
            //
            // For other symbols (variables, functions, etc.), we still return ERROR to prevent infinite loops.
            let symbol = self.ctx.binder.get_symbol(sym_id);
            if let Some(symbol) = symbol {
                let flags = symbol.flags;
                if flags
                    & (symbol_flags::INTERFACE
                        | symbol_flags::CLASS
                        | symbol_flags::TYPE_ALIAS
                        | symbol_flags::ENUM
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE)
                    != 0
                {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    let lazy_type = factory.lazy(def_id);
                    // Don't cache the Lazy type - we want to retry when the circular reference is broken
                    return lazy_type;
                }

                if flags & symbol_flags::FUNCTION != 0
                    && flags & symbol_flags::INTERFACE == 0
                    && let Some(provisional) =
                        self.provisional_circular_function_symbol_type(sym_id)
                {
                    self.ctx.symbol_types.insert(sym_id, provisional);
                    return provisional;
                }
            }

            // For non-named entities, cache ERROR to prevent repeated deep recursion
            // This is key for fixing timeout issues with circular class inheritance
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR; // Circular reference - propagate error
        }

        // Check recursion depth to prevent stack overflow
        let depth = self.ctx.symbol_resolution_depth.get();
        if depth >= self.ctx.max_symbol_resolution_depth {
            // CRITICAL: Cache ERROR immediately to prevent repeated deep recursion
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR; // Depth exceeded - prevent stack overflow
        }
        self.ctx.symbol_resolution_depth.set(depth + 1);

        // Push onto resolution stack
        self.ctx.symbol_resolution_stack.push(sym_id);
        self.ctx.symbol_resolution_set.insert(sym_id);

        // CRITICAL: Pre-cache a placeholder to break deep recursion chains
        // This prevents stack overflow in circular class inheritance by ensuring
        // that when we try to resolve this symbol again mid-resolution, we get
        // the cached value immediately instead of recursing deeper.
        // We'll overwrite this with the real result later (line 815).
        //
        // For named entities (Interface, Class, TypeAlias, Enum), use a Lazy type
        // as the placeholder instead of ERROR. This allows circular dependencies
        // like `interface User { filtered: Filtered } type Filtered = { [K in keyof User]: ... }`
        // to work correctly, since keyof Lazy(User) can defer evaluation instead of failing.
        let symbol = self.ctx.binder.get_symbol(sym_id);
        let placeholder = if let Some(symbol) = symbol {
            let flags = symbol.flags;
            if flags
                & (symbol_flags::INTERFACE
                    | symbol_flags::CLASS
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::ENUM
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE)
                != 0
            {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                factory.lazy(def_id)
            } else if flags & symbol_flags::FUNCTION != 0 && flags & symbol_flags::INTERFACE == 0 {
                // Pre-cache ANY sentinel to break re-entrancy during provisional computation.
                // Without this, processing `typeof foo<T>` in foo's return type calls
                // get_type_of_symbol(foo) which finds nothing cached → enters circular
                // detection → calls provisional again → stack overflow.
                self.ctx.symbol_types.insert(sym_id, TypeId::ANY);
                self.provisional_circular_function_symbol_type(sym_id)
                    .unwrap_or(TypeId::ERROR)
            } else {
                TypeId::ERROR
            }
        } else {
            TypeId::ERROR
        };
        trace!(
            sym_id = sym_id.0,
            placeholder = placeholder.0,
            is_lazy = lazy_def_id(self.ctx.types, placeholder).is_some(),
            file = self.ctx.file_name.as_str(),
            "get_type_of_symbol: inserted placeholder"
        );
        self.ctx.symbol_types.insert(sym_id, placeholder);

        self.push_symbol_dependency(sym_id, true);
        let (result, type_params) = self.compute_type_of_symbol(sym_id);
        self.pop_symbol_dependency();

        // Pop from resolution stack
        self.ctx.symbol_resolution_stack.pop();
        self.ctx.symbol_resolution_set.remove(&sym_id);

        // Decrement recursion depth
        self.ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get() - 1);

        // Cache result
        self.ctx.symbol_types.insert(sym_id, result);
        trace!(
            sym_id = sym_id.0,
            type_id = result.0,
            file = self.ctx.file_name.as_str(),
            "get_type_of_symbol"
        );

        // Also populate the type environment for Application expansion
        // IMPORTANT: We use the type_params returned by compute_type_of_symbol
        // because those are the same TypeIds used when lowering the type body.
        // Calling get_type_params_for_symbol would create fresh TypeIds that don't match.
        if result != TypeId::ANY && result != TypeId::ERROR {
            // For class symbols, we need to cache BOTH the constructor type (for value position)
            // and the instance type (for type position with typeof/TypeQuery resolution).
            let class_env_entry = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                if symbol.flags & symbol_flags::CLASS != 0 {
                    self.class_instance_type_with_params_from_symbol(sym_id)
                } else {
                    None
                }
            });

            // Use try_borrow_mut to avoid panic if type_env is already borrowed.
            // This can happen during recursive type resolution (e.g., class inheritance).
            // If we can't borrow, skip the cache update - the type is still computed correctly.
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                // Get the DefId if one exists (Phase 4.3 migration)
                let def_id = self.ctx.get_existing_def_id(sym_id);

                // For CLASS symbols:
                // - `result` is the constructor type (Callable with construct signatures)
                // - `instance_type` is the instance type (Object with properties)
                //
                // We cache the CONSTRUCTOR type in the type environment so that:
                // - `typeof Animal` resolves to the constructor type
                // - `Animal` used as a value resolves to the constructor type
                //
                // The instance type is still available via `class_instance_type_from_symbol`
                // for type position contexts where it's needed.
                if let Some((instance_type, _instance_params)) = &class_env_entry {
                    // This is a CLASS symbol - cache the constructor type (result)
                    // NOT the instance type. The instance type is used for class
                    // type position (e.g., `a: Animal`), not value position.
                    if type_params.is_empty() {
                        env.insert(SymbolRef(sym_id.0), result);
                        if let Some(def_id) = def_id {
                            env.insert_def(def_id, result);
                            // Also register the instance type so resolve_lazy returns it
                            // in type position (e.g., `{new(): Foo}` where Foo is a class)
                            env.insert_class_instance_type(def_id, *instance_type);
                            // Register SymbolId <-> DefId mapping so resolve_type_query
                            // can find the constructor type via DefId path.
                            env.register_def_symbol_mapping(def_id, sym_id);
                        }
                    } else {
                        env.insert_with_params(SymbolRef(sym_id.0), result, type_params.clone());
                        if let Some(def_id) = def_id {
                            env.insert_def_with_params(def_id, result, type_params.clone());
                            // Also register the instance type for class
                            env.insert_class_instance_type(def_id, *instance_type);
                            // Register SymbolId <-> DefId mapping so resolve_type_query
                            // can find the constructor type via DefId path.
                            env.register_def_symbol_mapping(def_id, sym_id);
                        }
                    }
                    // Register class extends relationship for nominal instanceof narrowing.
                    // Look up the parent class via InheritanceGraph (SymbolId-based) and
                    // convert to DefId so the solver can walk the extends chain.
                    if let Some(def_id) = def_id {
                        let parents = self.ctx.inheritance_graph.get_parents(sym_id);
                        if let Some(&parent_sym) = parents.first()
                            && let Some(parent_def_id) = self.ctx.get_existing_def_id(parent_sym)
                        {
                            env.register_class_extends(def_id, parent_def_id);
                        }
                    }
                } else if type_params.is_empty() {
                    // Check if resolve_lib_type_by_name already registered type params
                    // for this DefId. This happens for lib interfaces like Promise<T>,
                    // Array<T> where compute_type_of_symbol returns empty params but
                    // the lib resolution path registered them via ctx.insert_def_type_params.
                    let lib_params = def_id.and_then(|d| self.ctx.get_def_type_params(d));
                    if let Some(params) = lib_params {
                        env.insert_with_params(SymbolRef(sym_id.0), result, params.clone());
                        if let Some(def_id) = def_id {
                            env.insert_def_with_params(def_id, result, params);
                        }
                    } else {
                        env.insert(SymbolRef(sym_id.0), result);
                        if let Some(def_id) = def_id {
                            env.insert_def(def_id, result);
                        }
                    }
                } else {
                    env.insert_with_params(SymbolRef(sym_id.0), result, type_params.clone());
                    if let Some(def_id) = def_id {
                        env.insert_def_with_params(def_id, result, type_params.clone());
                    }
                }

                // Register numeric enums for Rule #7 (Open Numeric Enums)
                if let Some(def_id) = def_id {
                    self.maybe_register_numeric_enum(&mut env, sym_id, def_id);
                }

                // Register enum parent relationships for Task #17 (Enum Type Resolution)
                if let Some(def_id) = def_id
                    && let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
                    && (symbol.flags & symbol_flags::ENUM_MEMBER) != 0
                {
                    let parent_sym_id = symbol.parent;
                    if let Some(parent_def_id) = self.ctx.get_existing_def_id(parent_sym_id) {
                        env.register_enum_parent(def_id, parent_def_id);
                    }
                }
            } else {
                let sym_name = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .map_or("<unknown>", |s| s.escaped_name.as_str());
                tracing::warn!(
                    sym_id = sym_id.0,
                    sym_name = sym_name,
                    type_id = result.0,
                    type_params_count = type_params.len(),
                    "type_env try_borrow_mut FAILED - skipping insertion"
                );
            }

            // Mirror DefId mappings into type_environment (flow-analyzer env)
            // so both environments stay consistent. The type_env block above
            // handles SymbolRef + DefId writes to the evaluator env; this block
            // ensures the flow-analyzer env also has the DefId entries.
            if let Some(def_id) = self.ctx.get_existing_def_id(sym_id)
                && let Ok(mut env) = self.ctx.type_environment.try_borrow_mut()
            {
                if let Some((instance_type, _)) = &class_env_entry {
                    if type_params.is_empty() {
                        env.insert_def(def_id, result);
                    } else {
                        env.insert_def_with_params(def_id, result, type_params);
                    }
                    env.insert_class_instance_type(def_id, *instance_type);
                    env.register_def_symbol_mapping(def_id, sym_id);
                } else {
                    let lib_params = if type_params.is_empty() {
                        self.ctx.get_def_type_params(def_id)
                    } else {
                        None
                    };
                    if let Some(params) = lib_params {
                        env.insert_def_with_params(def_id, result, params);
                    } else if type_params.is_empty() {
                        env.insert_def(def_id, result);
                    } else {
                        env.insert_def_with_params(def_id, result, type_params);
                    }
                }
            }

            // Register TypeId -> DefId reverse mapping for TYPE ALIASES only.
            // This enables diagnostics to display type alias names (e.g., "ExoticAnimal")
            // instead of structural expansions (e.g., "CatDog | ManBearPig | Platypus").
            //
            // Only type aliases need this: interfaces already get their names resolved
            // via ObjectShape.symbol in format_symbol_name, and registering interfaces
            // would cause false positives where inline types like `A | B` display
            // as a matching alias name instead of their structural form.
            //
            // Extract def_id before calling evaluate_type_with_env to avoid borrow
            // conflicts with symbol_to_def.
            let alias_def_id = self
                .ctx
                .symbol_to_def
                .borrow()
                .get(&sym_id)
                .copied()
                .filter(|_| {
                    self.ctx
                        .binder
                        .symbols
                        .get(sym_id)
                        .is_some_and(|s| s.flags & symbol_flags::TYPE_ALIAS != 0)
                });
            if let Some(def_id) = alias_def_id {
                self.ctx
                    .definition_store
                    .register_type_to_def(result, def_id);
                self.ctx.definition_store.set_body(def_id, result);
                // Also register the evaluated form of the type.
                // Type aliases with union/intersection bodies often contain Lazy
                // members (e.g., `type Exotic = CatDog | ManBearPig`). When these
                // are evaluated, the Lazy members resolve to concrete types,
                // producing a new TypeId.  Register this evaluated TypeId too so
                // diagnostic formatting can display the alias name regardless of
                // whether the raw or evaluated form is referenced.
                if self.can_register_evaluated_alias_form(def_id, result) {
                    let evaluated = self.evaluate_type_with_env(result);
                    if evaluated != result {
                        self.ctx
                            .definition_store
                            .register_type_to_def(evaluated, def_id);
                    }
                }
            }
        }

        result
    }

    /// Resolve a `typeof X` type query with flow-sensitive narrowing.
    ///
    /// Delegates to [`get_type_from_type_query_flow_sensitive`] which resolves
    /// the expression type via `get_type_of_node` with control-flow narrowing
    /// enabled. Falls back to symbol-based resolution for edge cases.
    pub(crate) fn get_type_from_type_query(
        &mut self,
        idx: tsz_parser::parser::NodeIndex,
    ) -> tsz_solver::TypeId {
        self.get_type_from_type_query_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_from_type_query_with_request(
        &mut self,
        idx: tsz_parser::parser::NodeIndex,
        request: &TypingRequest,
    ) -> tsz_solver::TypeId {
        self.get_type_from_type_query_flow_sensitive_with_request(idx, request)
    }
}
