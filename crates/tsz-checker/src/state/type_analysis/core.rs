//! Core type analysis implementation: qualified name resolution, symbol type computation,
//! type queries, and contextual literal type analysis.

use crate::context::TypingRequest;
use crate::query_boundaries::checkers::generic as generic_query;
use crate::query_boundaries::common as common_query;
use crate::query_boundaries::common::lazy_def_id;
use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use crate::types_domain::queries::core::GlobalReceiver;
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

type TypeParamPushResult = (
    Vec<tsz_solver::TypeParamInfo>,
    Vec<(String, Option<TypeId>, bool)>,
);

/// #14345 construction-stamp flag (default-OFF, the SAME
/// `TSZ_TYPEPARAM_DECL_IDENTITY` the carrier's lowering stamp
/// (`collect_type_parameters_decl_scoped`) reads). When on, the checker's
/// def-type-param mint (`intern_type_param_for_decl`) stamps
/// `DeclScoped(file, name_node)` instead of `User`, and `push_type_parameters`
/// stores the stamped info, so the checker-built def-param list and the lowered
/// body refs converge on the SAME `DeclScoped` `TypeId`. Flag-OFF the stamp is
/// `User` and the stored list is byte-parity unchanged.
fn decl_identity_activation() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_TYPEPARAM_DECL_IDENTITY").is_ok_and(|v| v == "1"))
}

impl CheckerState<'_> {
    fn cache_resolved_symbol_type_for_owner(&self, sym_id: SymbolId, type_id: TypeId) {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        if symbol.decl_file_idx == u32::MAX {
            return;
        }
        if symbol.decl_file_idx as usize != self.ctx.current_file_idx {
            return;
        }

        self.ctx
            .cache_cross_file_symbol_type(sym_id, symbol.decl_file_idx, type_id, Vec::new());
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
    /// Decide whether a `typeof <name>` query is positioned in the signature
    /// (type-annotation) region of a function and the name resolves only
    /// because of body-local hoisting. tsc treats such references as
    /// unresolved — the signature scope is logically outside the body, even
    /// though we bind parameters, type parameters, and body `var`/function
    /// declarations into a single function scope.
    ///
    /// Returns true when:
    ///   * `idx` is inside a function/method/arrow and its enclosing chain
    ///     stays inside the function's **signature** (i.e. we reach the
    ///     function's `body` edge, or the `type`/parameter/return-type edge,
    ///     before reaching the function itself), AND
    ///   * `name` resolves to a symbol whose declaration is inside that
    ///     function's body.
    pub(super) fn is_typeof_in_function_signature_of_body_local(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        // Walk up to find the enclosing function-like node, tracking whether we
        // ever entered its body subtree. If we entered the body, this typeof is
        // inside the body — body-scope visibility is fine there.
        let mut current = idx;
        let mut enclosing_fn: Option<NodeIndex> = None;
        let mut saw_body = false;
        let mut entered_from: NodeIndex = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    if let Some(func) = self.ctx.arena.get_function(parent_node)
                        && func.body == entered_from
                    {
                        saw_body = true;
                    }
                    enclosing_fn = Some(parent);
                    break;
                }
                _ => {}
            }
            entered_from = parent;
            current = parent;
        }

        let Some(fn_idx) = enclosing_fn else {
            return false;
        };
        if saw_body {
            return false;
        }

        let Some(fn_node) = self.ctx.arena.get(fn_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(fn_node) else {
            return false;
        };
        if func.body.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(func.body) else {
            return false;
        };
        let (body_pos, body_end) = (body_node.pos, body_node.end);

        // Ask every scope whose container is lexically inside the function
        // body: does it declare `name`? If yes, the symbol is body-only and
        // tsc treats the signature-position `typeof name` as unresolved.
        // We intentionally don't call `resolve_identifier` here — that resolver
        // sees body-hoisted vars from the function scope and would always
        // succeed, hiding the signature/body boundary we're trying to recover.
        for scope in self.ctx.binder.scopes.iter() {
            let Some(cnode) = self.ctx.arena.get(scope.container_node) else {
                continue;
            };
            if cnode.pos < body_pos || cnode.end > body_end {
                continue;
            }
            if scope.table.get(name).is_some() {
                return true;
            }
        }
        false
    }

    pub(crate) fn is_type_query_in_non_flow_sensitive_signature_parameter(
        &self,
        idx: NodeIndex,
    ) -> bool {
        crate::types_domain::type_node_helpers::is_type_query_in_non_flow_sensitive_signature_parameter(
            self.ctx.arena,
            idx,
        )
    }

    /// Get type from a type query node (typeof X).
    ///
    /// Resolves value symbols, emits TS2504 for type-only symbols, handles
    /// unknown identifiers and missing members. Supports type arguments.
    ///
    /// Resolve a qualified name chain as a value property access chain
    /// for `typeof` context. Recurses through nested `QualifiedName` nodes
    /// so that `typeof a.b.c` resolves `a` as a value, then `.b`, then `.c`.
    pub(crate) fn resolve_typeof_qualified_value_chain(
        &mut self,
        idx: NodeIndex,
        use_flow: bool,
    ) -> TypeId {
        self.resolve_typeof_qualified_value_chain_with_request(idx, &TypingRequest::NONE, use_flow)
    }

    pub(crate) fn resolve_typeof_qualified_value_chain_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
        use_flow: bool,
    ) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            let (left_idx, right_idx) = if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                    return TypeId::ERROR;
                };
                (qn.left, qn.right)
            } else {
                let Some(access) = self.ctx.arena.get_access_expr(node) else {
                    return TypeId::ERROR;
                };
                if access.question_dot_token {
                    return TypeId::ERROR;
                }
                (access.expression, access.name_or_argument)
            };
            let left_type =
                self.resolve_typeof_qualified_value_chain_with_request(left_idx, request, use_flow);
            if let Some(rn) = self.ctx.arena.get(right_idx)
                && let Some(ident) = self.ctx.arena.get_identifier(rn)
            {
                if let Some(global_like_type) = self.resolve_global_like_typeof_member_access(
                    left_idx,
                    &ident.escaped_text,
                    right_idx,
                ) {
                    return if use_flow {
                        self.apply_flow_narrowing(idx, global_like_type)
                    } else {
                        global_like_type
                    };
                }
                if left_type == TypeId::ANY || left_type == TypeId::ERROR {
                    return left_type;
                }
                let object_type = self.resolve_type_for_property_access(left_type);
                if object_type == TypeId::ANY || object_type == TypeId::ERROR {
                    return object_type;
                }
                let (object_type_for_access, nullish_cause) = self.split_nullish_type(object_type);
                let Some(object_type_for_access) = object_type_for_access else {
                    if let Some(cause) = nullish_cause {
                        self.report_nullish_object(left_idx, cause, true);
                    }
                    return TypeId::ERROR;
                };
                if let Some(cause) = nullish_cause {
                    self.report_nullish_object(left_idx, cause, false);
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

    pub(super) fn resolve_global_like_typeof_member_access(
        &mut self,
        left_idx: NodeIndex,
        member_name: &str,
        member_node: NodeIndex,
    ) -> Option<TypeId> {
        let is_this_global = self.is_this_resolving_to_global(left_idx);
        if !(self.is_global_this_like_expression(left_idx) || is_this_global) {
            return None;
        }

        let targets_global_this = self.is_global_this_expression(left_idx) || is_this_global;
        let receiver = GlobalReceiver::from_targets_global_this(targets_global_this);
        let allow_unknown_property_fallback = targets_global_this;
        let property_type = self.resolve_global_this_property_type(
            member_name,
            member_node,
            allow_unknown_property_fallback,
            receiver,
        );
        if property_type == TypeId::ERROR {
            return Some(TypeId::ERROR);
        }

        let access_targets_global_this = is_this_global || self.is_global_this_expression(left_idx);
        if access_targets_global_this
            && property_type == TypeId::ANY
            && self.ctx.no_implicit_any()
            && !self.is_js_file()
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            self.error_at_node(
                member_node,
                &format_message(
                    diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
                    &["typeof globalThis"],
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
            );
        }

        Some(self.resolve_type_query_type(property_type))
    }

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
        let mut identity_scoped_param_names = smallvec::SmallVec::<[u32; 2]>::new();

        // First pass: Add all type parameters to scope WITHOUT resolving constraints
        // This allows self-referential constraints like T extends Box<T>
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
                .map_or_else(
                    || "T".to_string(),
                    |id_data| id_data.escaped_text.to_string(),
                );

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
                origin: tsz_solver::TypeParamOrigin::User,
            };
            // A syntax-local declaration that lexically shadows an active type
            // parameter needs declaration identity even while the broader
            // declaration-identity campaign is disabled. A scratch-scope
            // collision from unrelated re-entrant resolution is not lexical
            // shadowing; require an enclosing declaration in the AST before
            // opting into the exact domain. Record the name node so every
            // refinement pass reuses the same origin.
            let needs_identity_scope =
                self.type_parameter_decl_needs_identity_scope(&name, data.name);
            if needs_identity_scope {
                identity_scoped_param_names.push(data.name.0);
            }
            let mut shadowed_class_param = false;
            if let Some(ref mut c) = self.ctx.enclosing_class
                && let Some(pos) = c.type_param_names.iter().position(|x| *x == name)
            {
                c.type_param_names.remove(pos);
                shadowed_class_param = true;
            }

            let type_id = self
                .intern_type_param_for_decl_stamped_with_identity(
                    data.name,
                    info,
                    needs_identity_scope,
                )
                .0;
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
                    .map_or_else(
                        || "T".to_string(),
                        |id_data| id_data.escaped_text.to_string(),
                    );
                let atom = self.ctx.types.intern_string(&name);

                let constraint = if data.constraint != NodeIndex::NONE {
                    let mut constraint_type = self.get_type_from_type_node(data.constraint);
                    // #15256: A method type-parameter constraint that references a
                    // type alias imported from a THIRD module can transiently fail
                    // to resolve inside a cross-arena class delegation: the child
                    // checker that rebuilds the imported class attempts to resolve
                    // the alias and yields `ERROR`, even though the alias's own
                    // declaring file resolves it correctly. Committing that `ERROR`
                    // as the constraint poisons generic inference — a literal
                    // argument widens to its constraint base (e.g. kysely's
                    // `bareC('sys.tables')` widening to `string`), cascading into
                    // false TS2345/TS2322 diagnostics. Recover a stable deferred
                    // reference (`Lazy(DefId)` of the alias, preserving type
                    // arguments) so the constraint resolves through the global
                    // `DefId -> TypeId` map at use time, order-independently,
                    // instead of collapsing to `ERROR`.
                    if constraint_type == TypeId::ERROR
                        && Self::is_in_cross_arena_delegation()
                        && let Some(deferred) =
                            self.recover_cross_arena_deferred_constraint(data.constraint)
                    {
                        constraint_type = deferred;
                    }
                    let is_typeof_constraint =
                        self.ctx.arena.get(data.constraint).is_some_and(|n| {
                            n.kind == tsz_parser::parser::syntax_kind_ext::TYPE_QUERY
                        });
                    let is_direct_mapped_constraint =
                        self.ctx.arena.get(data.constraint).is_some_and(|n| {
                            n.kind == tsz_parser::parser::syntax_kind_ext::MAPPED_TYPE
                        });
                    let is_direct_resolution_path_constraint =
                        self.ctx.arena.get(data.constraint).is_some_and(|n| {
                            matches!(
                                n.kind,
                                k if k == tsz_parser::parser::syntax_kind_ext::MAPPED_TYPE
                                    || k == tsz_parser::parser::syntax_kind_ext::UNION_TYPE
                                    || k == tsz_parser::parser::syntax_kind_ext::INTERSECTION_TYPE
                                    || k == tsz_parser::parser::syntax_kind_ext::INDEXED_ACCESS_TYPE
                            )
                        });
                    // Skip circular constraint check for `typeof` expressions.
                    // `T extends typeof a` where `a: T` resolves to `T extends T` but is
                    // NOT considered circular by tsc — it's a valid pattern for type narrowing.
                    // tsc's getConstraintOfTypeParameter defers typeof resolution.
                    let is_circular = !is_typeof_constraint
                        && if let Some(&param_type_id) = self.ctx.type_parameter_scope.get(&name) {
                            self.is_same_type_parameter(
                                constraint_type,
                                param_type_id,
                                &name,
                                is_direct_mapped_constraint,
                                is_direct_resolution_path_constraint,
                            )
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
                        self.ensure_application_symbols_resolved(constraint_type);
                        Some(constraint_type)
                    }
                } else {
                    None
                };

                let default = if data.default != NodeIndex::NONE {
                    let default_type = self.get_type_from_type_node(data.default);
                    self.ensure_application_symbols_resolved(default_type);
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
                    origin: tsz_solver::TypeParamOrigin::User,
                };

                // #14345 construction stamp: take the canonical id AND the
                // stamped info the interner keyed on. Pushing the STAMPED info
                // into the stored def-param list makes the stored list carry the
                // EXACT `DeclScoped(file, name_node)` origin the lowered body
                // refs hold, so the solver's `is_identity_for` primary check
                // (`type_param(stored) == map id`) fires. Flag-OFF,
                // `stamped_info == info` (`User`) → stored list byte-parity
                // unless this declaration shadows an active parameter.
                let (constrained_type_id, stamped_info) = self
                    .intern_type_param_for_decl_stamped_with_identity(
                        data.name,
                        info,
                        identity_scoped_param_names.contains(&data.name.0),
                    );
                if self.ctx.type_parameter_scope.get(&name).copied() != Some(constrained_type_id) {
                    self.ctx
                        .type_parameter_scope
                        .insert(name.clone(), constrained_type_id);
                    changed = true;
                }
                next_params.push(stamped_info);
            }

            params = next_params;
            if !changed {
                break;
            }
        }

        // Third pass: Detect indirect circular constraints (e.g., T extends U, U extends T)
        // Build a constraint graph among type parameters in this list and detect cycles.
        self.check_indirect_circular_constraints(&params, &param_indices);

        self.validate_type_parameter_defaults_against_constraints(&param_indices, &params);

        self.ctx.leave_recursion();
        (params, updates)
    }

    /// #15256: Rebuild a type-parameter constraint that resolved to `ERROR`
    /// inside a cross-arena class delegation as a stable deferred `Lazy(DefId)`
    /// reference (preserving type arguments as an `Application`).
    ///
    /// The transient child checker that materializes an imported class's
    /// instance type can fail to eagerly resolve a constraint whose alias is
    /// declared in a different module than the class; the eager resolution
    /// yields `ERROR` even though the alias resolves correctly through the
    /// global `DefId -> TypeId` map. Deferring to `Lazy(DefId)` — the same way
    /// member annotations reached through the class boundary defer — makes the
    /// constraint resolve consistently at use time regardless of the order in
    /// which files are checked, rather than being poisoned to `ERROR`.
    ///
    /// Returns `None` (leaving the caller's `ERROR` in place) when the
    /// constraint is not a plain type reference or its symbol cannot be
    /// resolved, so this never invents a constraint that was genuinely invalid.
    fn recover_cross_arena_deferred_constraint(
        &mut self,
        constraint_node: NodeIndex,
    ) -> Option<TypeId> {
        let (type_name, type_arg_nodes) = {
            let node = self.ctx.arena.get(constraint_node)?;
            if node.kind != syntax_kind_ext::TYPE_REFERENCE {
                return None;
            }
            let type_ref = self.ctx.arena.get_type_ref(node)?;
            let type_arg_nodes = type_ref
                .type_arguments
                .as_ref()
                .map(|args| args.nodes.clone())
                .unwrap_or_default();
            (type_ref.type_name, type_arg_nodes)
        };

        // Resolve the referenced name to a stable symbol. Bare symbol
        // resolution succeeds in the delegated context even when full type
        // materialization did not, and `create_lazy_type_ref` mints a
        // `Lazy(DefId)` that the global `DefId -> TypeId` map resolves
        // (following import aliases) at use time.
        let TypeSymbolResolution::Type(target_sym) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_name)
        else {
            return None;
        };
        let lazy = self.ctx.create_lazy_type_ref(target_sym);
        if lazy == TypeId::ERROR {
            return None;
        }
        if type_arg_nodes.is_empty() {
            return Some(lazy);
        }
        let args: Vec<TypeId> = type_arg_nodes
            .iter()
            .map(|&arg| self.get_type_from_type_node(arg))
            .collect();
        Some(self.ctx.types.application(lazy, args))
    }

    /// Allocate (or reuse) the canonical `TypeId` for one type-parameter
    /// declaration's `TypeParamInfo`.
    ///
    /// Two processings of the same declaration (e.g. `function f<T>` whose
    /// signature is computed once for parameter resolution and once for an
    /// annotation context) must converge on a single `TypeId`. Without
    /// this, `fresh_type_param` mints distinct non-deduped ids each time
    /// and every downstream interner table for types closing over `T`
    /// hashes to a different entry, defeating identity-based fast paths
    /// in the relation engine and producing spurious `TS2859`s on
    /// recursive aliases (`Recur<T>` vs `Recur<T> | undefined`).
    ///
    /// The reuse is guarded on full `TypeParamInfo` equality so the
    /// refinement pass can install a constrained variant when the user
    /// wrote `T extends C`.
    ///
    /// The canonical identity lives in the shared `DefinitionStore` (one
    /// `Arc` across parent and child checkers) under a multi-entry
    /// `(file-name Atom, name_node, info)` map. The file component makes
    /// the arena-local `NodeIndex` globally unambiguous. Identity is
    /// deliberately NOT keyed by `DefId`: the only def lookup available at
    /// mint time rides the file-agnostic raw-`SymbolId` index
    /// (`find_def_by_symbol`), whose cross-binder collisions would let two
    /// unrelated same-named declarations converge on one `TypeId`
    /// (over-sharing — observed as `RawBuilder<unknown>` contextual
    /// inference regressions on the kysely row).
    ///
    /// Sharing matters twice over. Within one checker,
    /// `get_class_instance_type_inner` and the outer
    /// `check_class_declaration` each call `push_type_parameters`
    /// independently; without a per-declaration cache they would mint
    /// different `TypeIds` for the same `T`, making
    /// `MappedType.constraint = KeyOf(T_id_instance)` differ from
    /// `K.constraint = KeyOf(T_id_check)` and silently defeating
    /// `type_param_constraint_matches` in the solver's `visit_mapped`
    /// (false TS2349 on `this.map[key]()` patterns). Across checkers,
    /// cross-arena delegation spawns child `CheckerContext`s for the same
    /// files; a per-context cache lets each child mint its own ids for the
    /// same declarations, so an `implements` clause's type arguments and a
    /// member annotation disagree and the relation identity fast path goes
    /// dead (false `TS2416`/`TS2740` `ExpressionBuilder<DB, TB>` vs itself
    /// on `kysely`-style generic-alias parameter annotations, #13044).
    ///
    /// The map is multi-entry per declaration because the two-phase push
    /// pattern (pass 1 unconstrained, pass 2 constrained refinement) needs
    /// the unconstrained and constrained variants to each keep their own
    /// stable id; a single-slot cache ping-pongs and re-mints fresh ids on
    /// every push sequence.
    ///
    /// The per-context `type_param_node_cache` is kept as a thin L1 in
    /// front of the shared store; its entries are only ever written with
    /// the store's canonical id.
    pub(crate) fn intern_type_param_for_decl(
        &mut self,
        name_node: tsz_parser::parser::NodeIndex,
        info: tsz_solver::TypeParamInfo,
    ) -> tsz_solver::TypeId {
        self.intern_type_param_for_decl_stamped(name_node, info).0
    }

    /// Like [`Self::intern_type_param_for_decl`] but ALSO returns the
    /// (possibly `DeclScoped`-stamped) `TypeParamInfo` that the canonical
    /// `TypeId` was keyed on. #14345 construction stamp: callers that store the
    /// def-param list (`push_type_parameters`) must push the STAMPED info so the
    /// stored list and the lowered body refs share the SAME `DeclScoped` origin
    /// — a separately-reconstructed stamp diverges, and the stored list must
    /// re-intern to the canonical `TypeId` for `is_identity_for`'s primary
    /// `type_param(stored) == map id` check to fire. With broad activation off,
    /// the returned info remains byte-parity `User` unless the internal caller
    /// explicitly marks a binder that shadows an active type parameter.
    pub(crate) fn intern_type_param_for_decl_stamped(
        &mut self,
        name_node: tsz_parser::parser::NodeIndex,
        info: tsz_solver::TypeParamInfo,
    ) -> (tsz_solver::TypeId, tsz_solver::TypeParamInfo) {
        self.intern_type_param_for_decl_stamped_with_identity(name_node, info, false)
    }

    pub(crate) fn intern_type_param_for_decl_stamped_with_identity(
        &mut self,
        name_node: tsz_parser::parser::NodeIndex,
        mut info: tsz_solver::TypeParamInfo,
        needs_identity_scope: bool,
    ) -> (tsz_solver::TypeId, tsz_solver::TypeParamInfo) {
        // #14345 construction stamp (broad activation or a selective shadow):
        // stamp the def-type-param
        // mint with the IDENTICAL `DeclScoped(file, name_node)` the carrier's
        // lowering body refs carry, BEFORE the cache/decl-node lookups so every
        // key reflects the stamp and the stored list + lowered refs converge on
        // the SAME `DeclScoped` `TypeId`.
        //
        // When neither activation applies, the stamp AND the early
        // `intern_string(file_name)` are both skipped: that body must be
        // BYTE-IDENTICAL to the pre-#14345 sequence, INCLUDING the position at
        // which the file name is interned
        // (after the L1 cache lookup + `registered_def`, below). Interning it
        // early shifts the program's atom-allocation order — observable in
        // order-sensitive structures even though the `TypeId`s are equivalent
        // (the conformance leak). So flag-OFF reuses the original position.
        let mut file_atom = None;
        if needs_identity_scope || decl_identity_activation() {
            let atom = self.ctx.types.intern_string(&self.ctx.file_name);
            file_atom = Some(atom);
            info.origin = tsz_solver::TypeParamOrigin::DeclScoped {
                file: atom,
                node: name_node.0,
            };
        }

        // L1: per-context cache. `name_node` always belongs to `ctx.arena`,
        // so the arena-local key is unambiguous within one context.
        if let Some(&cached) = self.ctx.type_param_node_cache.get(&(name_node.0, info)) {
            return (cached, info);
        }

        let registered_def = self
            .ctx
            .binder
            .node_symbols
            .get(&name_node.0)
            .and_then(|&sym_id| self.ctx.definition_store.find_def_by_symbol(sym_id.0));

        // Flag-OFF: intern the file name HERE (its original pre-#14345
        // position) so the atom-allocation order is byte-identical to main.
        // Flag-ON: already interned above for the stamp; reuse it.
        let file_atom =
            file_atom.unwrap_or_else(|| self.ctx.types.intern_string(&self.ctx.file_name));

        let cached =
            self.ctx
                .definition_store
                .find_type_param_for_decl_node(file_atom, name_node.0, &info);

        let minted = cached.unwrap_or_else(|| self.ctx.types.fresh_type_param(info));

        // Adopt the canonical id from the shared node-keyed map (first
        // writer wins across parallel checkers and cross-arena delegation).
        let type_id = self.ctx.definition_store.register_type_param_for_decl_node(
            file_atom,
            name_node.0,
            info,
            minted,
        );

        if let Some(def_id) = registered_def {
            self.ctx
                .definition_store
                .register_type_to_def(type_id, def_id);
        }
        // Keep the L1 cache up to date so the next push_type_parameters
        // call for this same declaration returns the same TypeId.
        self.ctx
            .type_param_node_cache
            .insert((name_node.0, info), type_id);

        (type_id, info)
    }

    pub(super) fn empty_type_literal_satisfies_optional_mapped_constraint(
        &mut self,
        param_idx: NodeIndex,
        constraint_type: TypeId,
    ) -> bool {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return false;
        };
        let Some(param_data) = self.ctx.arena.get_type_parameter(param_node) else {
            return false;
        };
        let default_node = param_data.default;
        if !self.is_empty_type_literal_node(default_node) {
            return false;
        }

        if self.constraint_node_is_partial_object(param_data.constraint) {
            return true;
        }

        let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, constraint_type)
        else {
            return false;
        };
        if args.len() != 1 {
            return false;
        }
        let Some(&arg) = args.first() else {
            return false;
        };

        let base = self.resolve_lazy_type(base);
        let Some(mapped) = crate::query_boundaries::common::mapped_type_info(
            self.ctx.types.as_type_database(),
            base,
        ) else {
            return false;
        };
        if !matches!(
            mapped.optional_modifier,
            Some(tsz_solver::MappedModifier::Add)
        ) {
            return false;
        }

        self.is_object_like_for_optional_mapped_type(arg)
    }

    fn constraint_node_is_partial_object(&mut self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(paren) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.constraint_node_is_partial_object(paren.type_node);
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(identifier) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if identifier.escaped_text != "Partial" {
            return false;
        }
        let TypeSymbolResolution::Type(partial_sym) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return false;
        };
        if !self.ctx.symbol_is_from_actual_or_cloned_lib(partial_sym) {
            return false;
        }
        let Some(type_args) = &type_ref.type_arguments else {
            return false;
        };
        if type_args.nodes.len() != 1 {
            return false;
        }
        let Some(&arg_node) = type_args.nodes.first() else {
            return false;
        };
        let arg_type = self.get_type_from_type_node(arg_node);
        self.is_object_like_for_optional_mapped_type(arg_type)
    }

    fn is_empty_type_literal_node(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(paren) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.is_empty_type_literal_node(paren.type_node);
        }
        node.kind == syntax_kind_ext::TYPE_LITERAL
            && self
                .ctx
                .arena
                .get_type_literal(node)
                .is_some_and(|type_lit| type_lit.members.nodes.is_empty())
    }

    fn is_object_like_for_optional_mapped_type(&mut self, type_id: TypeId) -> bool {
        let resolved = self.resolve_lazy_type(type_id);
        if resolved == TypeId::OBJECT {
            return true;
        }

        crate::query_boundaries::common::is_object_like_type(
            self.ctx.types.as_type_database(),
            resolved,
        )
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
        // Shared cross-context stack-overflow breaker: probe → trip → grow.
        // Symbol type resolution is a node in the heritage-merge recursion
        // cycle (`merge → get_type_of_symbol(base) → … → merge`), so it must
        // carry the same breaker as the merge entry points (#14111). `None`
        // signals a bail (already tripped, or this probe tripped it); cache
        // `ERROR` for the symbol on that path, matching the prior behaviour.
        let type_id = if let Some(type_id) = crate::checkers_domain::with_stack_guard(None, || {
            Some(self.get_type_of_symbol_inner(sym_id))
        }) {
            type_id
        } else {
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            TypeId::ERROR
        };
        // tsc widens a bare `unique symbol` alias binding to `symbol` inside
        // `getTypeOfSymbol` (`widenTypeForVariableLikeDeclaration`). Apply that on
        // the read only — never written back — so the cached declared type stays
        // unique for the DTS emitter's own read-widening path.
        self.widen_read_unique_symbol_binding(sym_id, type_id)
    }

    fn get_type_of_symbol_inner(&mut self, sym_id: SymbolId) -> TypeId {
        let factory = self.ctx.types.factory();
        self.record_symbol_dependency(sym_id);
        // A plain value `const`/`let`/`var` genuinely declared in the current file
        // must be resolved locally, even if the dynamic cross-file overlay claims
        // a foreign owner. When the current file's own exports are re-exported
        // back to it through an `export *` cycle (`internal.ts` does
        // `export * from "./common"` and `common.ts` imports from `./internal`),
        // the namespace-export stamp registers the current file's `export const`
        // symbols against the re-exporting file. Delegating to that file's arena —
        // which has no concrete declaration for the const — would collapse its
        // value type to `any` (false `TS7053` on `obj[K]`, masked real `TS2322`).
        // tsc resolves the const to its declared literal everywhere; honor the
        // current-file declaration here.
        let cross_file_owner_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .filter(|&file_idx| file_idx != self.ctx.current_file_idx)
            .filter(|&foreign_idx| {
                !self.value_variable_owned_by_current_file_not_foreign(sym_id, foreign_idx)
            });
        let use_local_symbol_state = cross_file_owner_idx.is_none();
        if let Some(file_idx) = cross_file_owner_idx
            && let Some((cached, _params)) = self
                .ctx
                .cached_cross_file_symbol_type(sym_id, file_idx as u32)
        {
            // Declaration-file class symbols gate the SYMBOL-bucket shortcut
            // on the instance side being recoverable; see
            // `class_instance_recoverable` (#13185). The gate is scoped to
            // declaration files because they have no other ClassInstance
            // writer (the class delegation path skips `.d.ts`), while
            // recomputing a `.ts` class here mid-check can degrade its
            // heritage-merged shape (aliasUsage* conformance family).
            let declaration_file_class = self.file_index_is_declaration_file(file_idx)
                && self
                    .ctx
                    .get_binder_for_file(file_idx)
                    .and_then(|binder| binder.get_symbol(sym_id))
                    .or_else(|| self.ctx.binder.get_symbol(sym_id))
                    .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::CLASS));
            if !declaration_file_class
                || self.ctx.class_instance_recoverable(sym_id, file_idx as u32)
            {
                return cached;
            }
        }

        // Check cache first
        if cross_file_owner_idx.is_none()
            && let Some(cached) = self.ctx.symbol_types.get(&sym_id)
        {
            let cached_is_stale_alias_placeholder =
                !self.ctx.symbol_resolution_set.contains(&sym_id)
                    && crate::query_boundaries::common::lazy_def_id(self.ctx.types, cached)
                        == self.ctx.get_existing_def_id(sym_id)
                    && self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_ALIAS));
            let cached_is_wrong_alias_body = !self.ctx.symbol_resolution_set.contains(&sym_id)
                && self.ctx.resolve_symbol_file_index(sym_id).is_some()
                && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                    if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
                        return false;
                    }
                    let expected_def = self
                        .ctx
                        .get_or_create_def_id_for_symbol_name(sym_id, &symbol.escaped_name);
                    crate::query_boundaries::common::lazy_def_id(self.ctx.types, cached)
                        .or_else(|| self.ctx.definition_store.find_def_for_type(cached))
                        .is_some_and(|cached_def| cached_def != expected_def)
                });
            if cached_is_stale_alias_placeholder || cached_is_wrong_alias_body {
                self.ctx.symbol_types.remove(&sym_id);
            } else {
                if cached == TypeId::ERROR && self.ctx.symbol_resolution_set.contains(&sym_id) {
                    // Pre-cache ANY sentinel to prevent re-entrancy: provisional_circular_function_symbol_type
                    // processes type annotations which may call get_type_of_symbol for the same symbol
                    // (e.g., `typeof foo<T>` in foo's own return type). Without this sentinel, the re-entrant
                    // call finds ERROR, detects circularity, and calls provisional again → stack overflow.
                    self.ctx.symbol_types.insert(sym_id, TypeId::ANY);
                    let provisional = self
                        .provisional_circular_function_symbol_type(sym_id)
                        .or_else(|| {
                            self.provisional_circular_variable_function_symbol_type(sym_id)
                        });
                    if let Some(provisional) = provisional {
                        self.ctx.symbol_types.insert(sym_id, provisional);
                        trace!(
                            sym_id = sym_id.0,
                            type_id = provisional.0,
                            file = self.ctx.file_name.as_str(),
                            "(cached provisional) get_type_of_symbol"
                        );
                        tsz_common::perf_counters::record_compute_type_of_symbol_cache_hit();
                        return provisional;
                    }
                    // Restore ERROR if provisional failed
                    self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
                }
                let cached = self.ctx.symbol_types.get(&sym_id).unwrap_or(TypeId::ERROR);
                trace!(
                    sym_id = sym_id.0,
                    type_id = cached.0,
                    file = self.ctx.file_name.as_str(),
                    "(cached) get_type_of_symbol"
                );
                tsz_common::perf_counters::record_compute_type_of_symbol_cache_hit();
                return cached;
            }
        }

        // Check fuel - return ERROR if exhausted to prevent timeout
        if !self.ctx.consume_fuel() {
            // Cache ERROR so we don't keep trying to resolve this symbol
            if use_local_symbol_state {
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            }
            return TypeId::ERROR;
        }

        // Check for circular reference
        if use_local_symbol_state && self.ctx.symbol_resolution_set.contains(&sym_id) {
            // Named entities use Lazy placeholders so circular dependencies can
            // defer evaluation; other symbols still return ERROR to avoid loops.
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
                    if flags & symbol_flags::CLASS != 0
                        && let Some(partial) = self.circular_class_partial_constructor_type(sym_id)
                    {
                        return partial;
                    }
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

                if flags & symbol_flags::VARIABLE != 0
                    && let Some(provisional) =
                        self.provisional_circular_variable_function_symbol_type(sym_id)
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
            if use_local_symbol_state {
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            }
            return TypeId::ERROR; // Depth exceeded - prevent stack overflow
        }
        self.ctx.symbol_resolution_depth.set(depth + 1);

        // Push onto resolution stack
        if use_local_symbol_state {
            self.ctx.symbol_resolution_stack.push(sym_id);
            self.ctx.symbol_resolution_set.insert(sym_id);
        }

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
        if use_local_symbol_state {
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
                } else if flags & symbol_flags::FUNCTION != 0
                    && flags & symbol_flags::INTERFACE == 0
                {
                    // Pre-cache ANY sentinel to break re-entrancy during provisional computation.
                    // Without this, processing `typeof foo<T>` in foo's return type calls
                    // get_type_of_symbol(foo) which finds nothing cached → enters circular
                    // detection → calls provisional again → stack overflow.
                    self.ctx.symbol_types.insert(sym_id, TypeId::ANY);
                    self.provisional_circular_function_symbol_type(sym_id)
                        .unwrap_or(TypeId::ERROR)
                } else if flags & symbol_flags::VARIABLE != 0 {
                    // A `const`/`let`/`var` bound to a fully-annotated arrow or
                    // function expression that self-references inside its own
                    // body (e.g. `const f = (x: number): T => { ... f(x) ... }`)
                    // reaches this placeholder before its own initializer has
                    // been checked. Without a provisional signature here, the
                    // very first in-body reference to `f` freezes at `ERROR`
                    // for the rest of the check (`param.map(f)` degrades to
                    // `unknown[]`), even though the outer symbol later resolves
                    // correctly once its initializer finishes. Same
                    // re-entrancy guard as the FUNCTION branch above.
                    self.ctx.symbol_types.insert(sym_id, TypeId::ANY);
                    self.provisional_circular_variable_function_symbol_type(sym_id)
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
        }

        // Capture the cross-arena bailout epoch so a provisional `any` minted
        // because a cross-arena delegation was refused by the depth cap during
        // this resolution is not frozen as the symbol's authoritative type. A
        // later shallower pass recomputes the real type (the immer `[WRITABLE]`
        // computed-key poison, #13846). Gated on provenance (not on the value
        // being `any`) so genuine `any` results still cache; only the
        // provisional `any` is dropped, so a single shallow resolution
        // self-heals the cache without a recompute storm. `ERROR`/`UNKNOWN` are
        // deliberate cross-file cycle markers (and are already excluded from the
        // program bucket), so they are left untouched here.
        let bailout_epoch_before = Self::cross_arena_bailout_epoch();
        self.push_symbol_dependency(sym_id, true);
        let (result, type_params) = self.compute_type_of_symbol(sym_id);
        self.pop_symbol_dependency();
        let result_is_bailout_artifact =
            Self::cross_arena_bailout_epoch() != bailout_epoch_before && result == TypeId::ANY;

        // Fold cross-file `declare module` augmentations into an exported
        // interface's materialized body at this canonical resolution point, so
        // every downstream cache (symbol_types, both type environments, the
        // shared def store) observes the SAME augmented body. Doing it only on
        // the type-reference path (`type_reference_symbol_type`) left this path —
        // reached for the solver's def-store registration and cross-file
        // delegation — publishing the un-augmented body, which then shadowed the
        // augmented form at every `keyof` / Application / assignability site
        // (#13653, extends the same-module #13509 fix to the cross-file path).
        // `apply_self_module_augmentations` is a no-op unless the program has
        // augmentations and the symbol is an exported, non-imported interface.
        // The `program_has_module_augmentations` guard keeps augmentation-free
        // programs from paying for the symbol lookup on every interface.
        //
        // Only merge when `result` is ALREADY a concrete object/callable shape:
        // a `Lazy`/unresolved cross-file body (e.g. when this symbol's home file
        // is not the file currently being checked) would otherwise fall into the
        // augmentation intersection fallback and cache a degenerate type. Those
        // cases are handled by the import/type-reference path once the body
        // resolves; here we only need to keep the *materialized* body augmented
        // so it cannot shadow that path's result in the shared def store.
        // Cheap checks first: the program-wide augmentation guard, then the
        // interface-flag test, and only then the per-`TypeId`
        // `classify_for_augmentation` lookup — so non-interface symbols never pay
        // for the classification.
        let result = if self.ctx.program_has_module_augmentations()
            && self
                .ctx
                .binder
                .get_symbol(sym_id)
                .or_else(|| self.get_cross_file_symbol(sym_id))
                .is_some_and(|symbol| {
                    symbol.has_any_flags(symbol_flags::INTERFACE)
                        && !symbol.has_any_flags(symbol_flags::CLASS)
                })
            && matches!(
                crate::query_boundaries::common::classify_for_augmentation(self.ctx.types, result),
                crate::query_boundaries::common::AugmentationTargetKind::Object(_)
                    | crate::query_boundaries::common::AugmentationTargetKind::ObjectWithIndex(_)
                    | crate::query_boundaries::common::AugmentationTargetKind::Callable(_)
            ) {
            self.apply_self_module_augmentations(sym_id, result)
        } else {
            result
        };

        // Fold cross-block / cross-file `declare global { interface X }`
        // declarations into a user-declared global interface's materialized body
        // at this same canonical resolution point, so `keyof X`, `X[K]`,
        // assignability, and display all observe the merged shape regardless of
        // declaration/file order — matching the value-position member-access
        // path that already reunites the partial symbols through
        // `global_augmentations`. Cheap guards first: the program-wide
        // global-augmentation map, then the interface-flag test, and only then
        // the per-`TypeId` `classify_for_augmentation` lookup.
        let result = if self.ctx.program_has_global_augmentations()
            && self
                .ctx
                .binder
                .get_symbol(sym_id)
                .or_else(|| self.get_cross_file_symbol(sym_id))
                .is_some_and(|symbol| {
                    symbol.has_any_flags(symbol_flags::INTERFACE)
                        && !symbol.has_any_flags(symbol_flags::CLASS)
                })
            && matches!(
                crate::query_boundaries::common::classify_for_augmentation(self.ctx.types, result),
                crate::query_boundaries::common::AugmentationTargetKind::Object(_)
                    | crate::query_boundaries::common::AugmentationTargetKind::ObjectWithIndex(_)
                    | crate::query_boundaries::common::AugmentationTargetKind::Callable(_)
            ) {
            self.apply_self_global_augmentations(sym_id, result)
        } else {
            result
        };

        // Pop from resolution stack
        if use_local_symbol_state {
            self.ctx.symbol_resolution_stack.pop();
            self.ctx.symbol_resolution_set.remove(&sym_id);
        }

        // Decrement recursion depth
        self.ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get() - 1);

        // Cache result.
        //
        // Guard against constructor type cache corruption from cycle-
        // fallback values: when an outer `get_class_constructor_type(C)`
        // is in progress and a nested `get_type_of_symbol(C)` arrives,
        // `compute_class_symbol_type` can observe a Lazy(DefId)
        // cycle-fallback and propagate it as `result`. That Lazy points
        // at the class's own DefId and resolves to the INSTANCE type —
        // caching it here would poison later value-position lookups of
        // the class (e.g. `C.staticProp` inside an instance method body)
        // and produce false TS2339. Instead, drop the placeholder so the
        // next lookup re-enters and observes the fully-built constructor
        // type after the outer resolution completes.
        let result_is_lazy_to_self = {
            common_query::lazy_def_id(self.ctx.types.as_type_database(), result)
                .zip(self.ctx.get_existing_def_id(sym_id))
                .is_some_and(|(ld, od)| ld == od)
        };
        // A class symbol queried in value position while its own instance
        // type is still being computed (`class_instance_resolution_set`
        // contains it, e.g. a static self-reference like
        // `readonly X = C.Y` forcing `typeof C` mid-build) yields a
        // provisional constructor type whose construct-signature return is
        // the Phase-0 prescan instance shape — missing computed/symbol-keyed
        // members and heritage. Caching that would leak the partial instance
        // into later `new C()` results (false TS7053/TS2739/TS2741). Only
        // results that actually embed the provisional instance are dropped;
        // healthy in-window results stay cacheable (perf on large
        // self-referential classes).
        let class_instance_resolution_in_flight = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .is_some_and(|s| s.has_any_flags(symbol_flags::CLASS))
            && self.ctor_result_embeds_inflight_instance(sym_id, result);
        if result_is_bailout_artifact {
            // Registration-window artifact (a cross-arena delegation was refused
            // by the depth cap during this resolution): drop the placeholder so
            // the next lookup re-enters and a shallower pass recomputes the
            // authoritative type, and do not promote the sentinel (#13846).
            if use_local_symbol_state {
                self.ctx.symbol_types.remove(&sym_id);
            }
        } else if let Some(file_idx) = cross_file_owner_idx {
            self.ctx.cache_cross_file_symbol_type(
                sym_id,
                file_idx as u32,
                result,
                type_params.clone(),
            );
        } else {
            let result_cached_locally = if (result_is_lazy_to_self
                || class_instance_resolution_in_flight)
                && self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|s| s.has_any_flags(symbol_flags::CLASS))
            {
                self.ctx.symbol_types.remove(&sym_id);
                false
            } else {
                self.ctx.symbol_types.insert(sym_id, result);
                true
            };
            if result_cached_locally {
                self.cache_resolved_symbol_type_for_owner(sym_id, result);
            }
        }
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
        if use_local_symbol_state
            && result != TypeId::ANY
            && result != TypeId::ERROR
            // Do not register provisional in-flight class constructor types
            // (see `class_instance_resolution_in_flight` above): the env entry
            // would pin the prescan instance shape for Lazy/Application
            // resolution even after the real instance type is finished.
            && !class_instance_resolution_in_flight
        {
            let definition_body_is_progress =
                self.publish_symbol_result_to_type_envs(sym_id, result, &type_params);

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
                .filter(|_| self.symbol_is_type_alias(sym_id));
            if definition_body_is_progress && let Some(def_id) = alias_def_id {
                self.ctx
                    .definition_store
                    .register_type_to_def(result, def_id);
                self.ctx.publish_definition_body(def_id, result);

                // Record the body's display provenance (see
                // `record_alias_body_provenance`). Only a non-generic alias is a
                // candidate: a generic alias keeps its name because the operator
                // is part of the definition, not a simplification.
                let alias_is_non_generic = self
                    .ctx
                    .definition_store
                    .get(def_id)
                    .is_some_and(|d| d.type_params.is_empty());
                // Issue #10914: a non-generic alias whose body is an application
                // of a conditional-bodied generic alias —
                // `type RO = DeepReadonly<Config>` — carries no `aliasSymbol`
                // when it resolves to an anonymous object. tsc renders the
                // resolved object structurally, so mark the body "computed" and
                // let the established display path expand it. Type-shape
                // inspection is owned by the solver through the query boundary;
                // evaluation is guarded exactly like the evaluated-form
                // registration below to stay clear of free type parameters and
                // self-referential cycles, and only runs once the cheap
                // structural gates above have matched.
                //
                // The shape gate stays object/mapped-only here on purpose,
                // even though the solver formatter's
                // `reducing_application_display` reduces *every*
                // resolved shape (scalar, tuple, union, object) for direct and
                // nested `Application` display. The two paths own different
                // surfaces: the formatter renders an application node in place,
                // whereas marking a body "computed" here also drives the def
                // store's `find_type_alias_by_body` reverse lookup. A reduced
                // *scalar* never needs the mark (a primitive-bodied alias
                // already displays as its underlying type), and a reduced
                // *tuple*/*union* shares its interned `TypeId` with any
                // directly-written alias of the same shape — marking it
                // "computed" would let that shared shape repaint the structural
                // result with the colliding alias's name. So this top-level
                // name-display gate stays conservative; the formatter owns the
                // broader shape reduction where no reverse lookup intervenes.
                // Reduced form of the alias result, driving the collapse-shape
                // gates below (a generic application only takes its array/tuple
                // shape after evaluation). Guarded like the evaluated-form
                // registration below to stay clear of cycles and free params.
                let alias_result_is_evaluable =
                    !generic_query::contains_free_type_parameters(self.ctx.types, result)
                        && self.can_register_evaluated_alias_form(def_id, result);
                let evaluated_alias_result = if alias_result_is_evaluable {
                    self.evaluate_type_with_env(result)
                } else {
                    result
                };
                let reducing_object_application = alias_is_non_generic
                    && diagnostic_query::application_base_has_conditional_alias_body(
                        self.ctx.types.as_type_database(),
                        &self.ctx.definition_store,
                        result,
                    )
                    && alias_result_is_evaluable
                    && diagnostic_query::is_object_or_mapped_type(
                        self.ctx.types.as_type_database(),
                        evaluated_alias_result,
                    );
                let body_is_computed = reducing_object_application
                    || (alias_is_non_generic
                        && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                            symbol.declarations.iter().any(|&decl_idx| {
                                super::source_alias_attribution::alias_declaration_body_is_computed(
                                    self.ctx.arena,
                                    self.ctx.types,
                                    decl_idx,
                                    result,
                                    evaluated_alias_result,
                                )
                            })
                        }));
                self.record_alias_body_provenance(result, body_is_computed, alias_is_non_generic);
                self.mark_tuple_spread_flattened_alias_def(
                    sym_id,
                    def_id,
                    result,
                    alias_is_non_generic,
                );
                self.mark_bare_nominal_ref_alias_def(sym_id, def_id, result, alias_is_non_generic);
                // Also register the evaluated form of the type.
                // Type aliases with union/intersection bodies often contain Lazy
                // members (e.g., `type Exotic = CatDog | ManBearPig`). When these
                // are evaluated, the Lazy members resolve to concrete types,
                // producing a new TypeId.  Register this evaluated TypeId too so
                // diagnostic formatting can display the alias name regardless of
                // whether the raw or evaluated form is referenced.
                if alias_result_is_evaluable && evaluated_alias_result != result {
                    self.ctx
                        .definition_store
                        .register_type_to_def(evaluated_alias_result, def_id);
                    // A computed body keeps the same provenance after a second
                    // evaluation pass collapses its Lazy members: the evaluated
                    // form must also be skipped by `find_type_alias_by_body`,
                    // otherwise the reverse lookup repaints the alias name onto
                    // the shared structural result (e.g. a conditional that
                    // reduces to `{ a: 1; }`).
                    self.record_alias_body_provenance(
                        evaluated_alias_result,
                        body_is_computed,
                        alias_is_non_generic,
                    );
                }
            }
        }

        result
    }

    /// Record the display provenance of an alias body `TypeId`: a reducing
    /// result is "computed" (rendered structurally), while a constructive
    /// non-generic alias body is "directly named" so it keeps its name even if a
    /// computed alias resolves to the same interned shape ("direct wins").
    fn record_alias_body_provenance(
        &self,
        body: TypeId,
        is_computed: bool,
        alias_is_non_generic: bool,
    ) {
        if is_computed {
            self.ctx.definition_store.mark_body_as_computed(body);
        } else if alias_is_non_generic {
            self.ctx.definition_store.mark_body_as_directly_named(body);
        }
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
