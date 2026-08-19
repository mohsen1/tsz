//! Type node resolution: converting type annotation AST nodes into `TypeId`
//! representations, plus expando property augmentation and globalThis resolution.

use crate::query_boundaries::state::type_environment as query;
use crate::state::CheckerState;
use crate::types_domain::queries::core::GlobalReceiver;
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Node Resolution
    // =========================================================================

    /// Get type from a type node.
    ///
    /// Uses compile-time constant `TypeIds` for intrinsic types (O(1) lookup).
    /// Get the type representation of a type annotation node.
    ///
    /// This is the main entry point for converting type annotation AST nodes into
    /// `TypeId` representations. Handles all TypeScript type syntax.
    ///
    /// ## Special Node Handling:
    /// - **`TypeReference`**: Validates existence before lowering (catches missing types)
    /// - **`TypeQuery`** (`typeof X`): Resolves via binder for proper symbol resolution
    /// - **`UnionType`**: Handles specially for nested typeof expression resolution
    /// - **`TypeLiteral`**: Uses checker resolution for type parameter support
    /// - **Other nodes**: Delegated to `TypeLowering`
    ///
    /// ## Type Parameter Bindings:
    /// - Uses current type parameter bindings from scope
    /// - Allows type parameters to resolve correctly in generic contexts
    ///
    /// ## Symbol Resolvers:
    /// - Provides type/value symbol resolvers to `TypeLowering`
    /// - Resolves type references and value references (for typeof)
    ///
    /// ## Error Reporting:
    /// - Checks for missing names before lowering
    /// - Emits appropriate errors for undefined types
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Primitive types
    /// let x: string;           // → STRING
    /// let y: number | boolean; // → Union(NUMBER, BOOLEAN)
    ///
    /// // Type references
    /// interface Foo {}
    /// let z: Foo;              // → Ref to Foo symbol
    ///
    /// // Generic types
    /// let a: Array<string>;    // → Application(Array, [STRING])
    ///
    /// // Type queries
    /// let value = 42;
    /// let b: typeof value;     // → TypeQuery(value symbol)
    ///
    /// // Type literals
    /// let c: { x: number };    // → Object type with property x: number
    /// ```
    /// If `idx` is a `PARENTHESIZED_TYPE` wrapping (through any number of
    /// nested parentheses) a `TYPE_QUERY` (`typeof ...`), return the inner
    /// `TYPE_QUERY` node index; otherwise `None`. Used to resolve a parenthesized
    /// `typeof` array element directly through the binder-aware path instead of
    /// the leaner parenthesized-type lowering.
    fn unwrap_parenthesized_type_query(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut unwrapped_paren = false;
        for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
                current = self.ctx.arena.get_wrapped_type(node)?.type_node;
                unwrapped_paren = true;
                continue;
            }
            return (unwrapped_paren && node.kind == syntax_kind_ext::TYPE_QUERY)
                .then_some(current);
        }
        None
    }

    pub fn get_type_from_type_node(&mut self, idx: NodeIndex) -> TypeId {
        #[cfg(test)]
        self.ctx.record_type_node_resolution_for_test(idx);

        // Delegate to TypeNodeChecker for type node handling.
        // TypeNodeChecker handles caching, type parameter scope, and recursion protection.
        //
        // Note: For types that need binder symbol resolution (TYPE_REFERENCE, TYPE_QUERY,
        // UNION_TYPE containing typeof, TYPE_LITERAL), we still use CheckerState's
        // specialized methods to ensure proper symbol resolution.
        //
        // See: docs/TS2304_SMART_CACHING_FIX.md

        // First check if this is a type that needs special handling with binder resolution
        if let Some(node) = self.ctx.arena.get(idx) {
            // TS1228: "A type predicate is only allowed in return type position for
            // functions and methods." The parser restricts predicate parsing to return
            // type positions, but some return types (getters, setters, construct
            // signatures, constructor types) still parse predicates for error recovery.
            // The checker flags these, matching tsc's getTypePredicateParent.
            if node.kind == syntax_kind_ext::TYPE_PREDICATE {
                let parent_node = self
                    .ctx
                    .arena
                    .get_extended(idx)
                    .and_then(|ext| self.ctx.arena.get(ext.parent));
                let parent_kind = parent_node.map(|p| p.kind);
                let is_valid = parent_kind.is_some_and(|kind| {
                    matches!(
                        kind,
                        syntax_kind_ext::FUNCTION_DECLARATION
                            | syntax_kind_ext::FUNCTION_EXPRESSION
                            | syntax_kind_ext::METHOD_DECLARATION
                            | syntax_kind_ext::METHOD_SIGNATURE
                            | syntax_kind_ext::CALL_SIGNATURE
                            | syntax_kind_ext::CONSTRUCT_SIGNATURE
                            | syntax_kind_ext::ARROW_FUNCTION
                            | syntax_kind_ext::FUNCTION_TYPE
                    )
                });
                // Skip TS1228 for constructor declarations that tsc covers
                // through grammar recovery. Construct signatures and constructor
                // type nodes (`new (...) => asserts x`) still get TS1228.
                let is_error_recovery_position =
                    parent_kind.is_some_and(|kind| matches!(kind, syntax_kind_ext::CONSTRUCTOR));
                // Skip TS1228 for getters/setters with invalid parameters — tsc
                // only emits TS1228 for valid accessor signatures (e.g. getters with
                // 0 params). When the accessor has parameter errors, those parser
                // errors take precedence and tsc does not additionally emit TS1228.
                let is_invalid_accessor = parent_node.is_some_and(|parent| {
                    (parent.kind == syntax_kind_ext::GET_ACCESSOR
                        || parent.kind == syntax_kind_ext::SET_ACCESSOR)
                        && self.ctx.arena.get_accessor(parent).is_some_and(|acc| {
                            let param_count = acc.parameters.nodes.len();
                            // Getters should have 0 params, setters should have 1
                            if parent.kind == syntax_kind_ext::GET_ACCESSOR {
                                param_count > 0
                            } else {
                                // For setters, type predicates in return type are
                                // always invalid but we check param count to match tsc
                                param_count != 1
                            }
                        })
                });
                if !is_valid && !is_error_recovery_position && !is_invalid_accessor {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        idx,
                        diagnostic_messages::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO,
                        diagnostic_codes::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO,
                    );
                }
            }

            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                // A `typeof` type query nested in a type argument
                // (`Wrap<typeof a>`, `Map<string, typeof a>`, …) is a value read
                // of its operand, exactly as tsc treats it. A type reference
                // lowers its whole subtree in one non-`import()` pass whose value
                // resolver never touches `referenced_symbols`, and the cache
                // short-circuits below can return before it runs at all. Record
                // the reads here so an operand read only by such a query is not
                // falsely reported unused (#16680). Gated to references that
                // actually carry type arguments — a bare reference cannot
                // contain a query.
                let has_type_args = self.ctx.arena.get_type_ref(node).is_some_and(|type_ref| {
                    type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|args| !args.nodes.is_empty())
                });
                if has_type_args {
                    self.mark_nested_type_query_reads(idx);
                }
                let should_refresh_cached_defaulted_reference = !has_type_args
                    && self.ctx.arena.get_type_ref(node).is_some_and(|type_ref| {
                        let sym_id = match self
                            .resolve_identifier_symbol_in_type_position(type_ref.type_name)
                        {
                            crate::symbol_resolver::TypeSymbolResolution::Type(sym_id) => {
                                Some(sym_id)
                            }
                            _ => match self
                                .resolve_qualified_symbol_in_type_position(type_ref.type_name)
                            {
                                crate::symbol_resolver::TypeSymbolResolution::Type(sym_id) => {
                                    Some(sym_id)
                                }
                                _ => None,
                            },
                        };

                        sym_id.is_some_and(|sym_id| {
                            self.get_type_params_for_symbol(sym_id)
                                .iter()
                                .any(|param| param.default.is_some())
                        })
                    });
                // Recovery path: a type reference can appear where an expression statement is expected
                // (e.g. malformed `this.x: any;` parses through a labeled statement).
                // In value position, primitive type keywords should emit TS2693.
                if let Some(ext) = self.ctx.arena.get_extended(idx) {
                    let parent = ext.parent;
                    let recovery_stmt_kind = if parent.is_some() {
                        self.ctx
                            .arena
                            .get(parent)
                            .map(|parent_node| parent_node.kind)
                    } else {
                        None
                    };
                    if matches!(
                        recovery_stmt_kind,
                        Some(k)
                            if k == syntax_kind_ext::LABELED_STATEMENT
                                || k == syntax_kind_ext::EXPRESSION_STATEMENT
                    ) && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                        && let Some(name) = self.entity_name_text(type_ref.type_name)
                        && matches!(
                            name.as_str(),
                            "number"
                                | "string"
                                | "boolean"
                                | "symbol"
                                | "void"
                                | "undefined"
                                | "null"
                                | "any"
                                | "unknown"
                                | "never"
                                | "object"
                                | "bigint"
                        )
                    {
                        // Route through wrong-meaning boundary: primitive keyword type-only
                        self.report_wrong_meaning_diagnostic(
                            &name,
                            type_ref.type_name,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        );
                        self.ctx.node_types.insert(idx.0, TypeId::ERROR);
                        return TypeId::ERROR;
                    }
                }

                // Validate the type reference exists before lowering
                // Check cache first - but allow re-resolution of ERROR when type params
                // are in scope, since the ERROR may have been cached when type params
                // weren't available yet (non-deterministic symbol processing order).
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !should_refresh_cached_defaulted_reference
                    {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_stack.contains(&idx)
                        && !should_refresh_cached_defaulted_reference
                    {
                        return cached;
                    }
                    // cached == ERROR but type_parameter_scope is non-empty: re-resolve
                    // cached != ERROR and type_parameter_scope non-empty: re-resolve (type params may differ)
                }
                // Lazy own-member lowering (`TSZ_LAZY_OWN_MEMBERS`): defer an
                // eligible non-generic lib-interface reference to a bare
                // `Lazy(DefId)` instead of materializing its full transitive
                // closure at this reference site (variable annotations, type
                // aliases, member annotations, …). It resolves on demand via the
                // #8638 single-member fast path / relation `resolve_lazy`. Only
                // argument-less references qualify; an ineligible reference
                // (generic, globally augmented, user-shadowed, user type) yields
                // `None` and falls through to the normal full-materialization
                // path, so flag-off is byte-identical.
                if crate::state_checking::lazy_lib_member::lazy_own_members_varpos_enabled() {
                    let deferral_name = self
                        .ctx
                        .arena
                        .get(idx)
                        .and_then(|n| self.ctx.arena.get_type_ref(n))
                        .filter(|type_ref| {
                            type_ref
                                .type_arguments
                                .as_ref()
                                .is_none_or(|args| args.nodes.is_empty())
                        })
                        .map(|type_ref| type_ref.type_name);
                    if let Some(type_name) = deferral_name
                        && let crate::symbol_resolver::TypeSymbolResolution::Type(sym_id) =
                            self.resolve_identifier_symbol_in_type_position(type_name)
                        && let Some(lazy) = self.try_defer_eligible_lib_type_reference(sym_id)
                    {
                        self.ctx.node_types.insert(idx.0, lazy);
                        return lazy;
                    }
                }
                let mut result = self.get_type_from_type_reference(idx);
                // Break a cross-file `const X = ...; type X = typeof X` self-loop:
                // register `X`'s value-space type when the reference resolves to a
                // deferred `typeof X`, so every later relation (constraint /
                // assignment / …) sees the value instead of the unresolvable query.
                // No-ops unless `result` is a bare `typeof` of a merged value
                // symbol (#15078).
                self.register_self_referential_merged_value_typeof(result);
                // Eagerly reduce a concrete `Awaited<…>` reference to its
                // unwrapped form, the way tsc computes `getAwaitedType` at the
                // reference site. The solver's lazy conditional/`infer`
                // evaluation of the standard-library `Awaited<T>` alias does not
                // converge once the awaited argument is a nested
                // `Promise<Promise<…>>` whose inner layers have materialized to
                // their structural `{ then }` Object shape: the outer conditional
                // bails to its `: T` branch and yields the still-wrapped
                // argument, so the relation sees `Promise<Promise<2>>` instead of
                // `2`. Folding here makes every `Awaited<…>` annotation position
                // converge to the same literal `tsc` reports. The fold returns
                // `None` for generic / non-thenable arguments (it only fires when
                // it actually unwraps a thenable), so deferred and non-`Awaited`
                // references are unchanged.
                if let Some(reduced) = self.try_evaluate_awaited_application(result) {
                    result = reduced;
                }
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::TYPE_QUERY {
                // Handle typeof X - need to resolve symbol properly via binder.
                // Return cached non-ERROR results when no type params in scope.
                // Always re-resolve ERROR because TypeNodeChecker may have cached
                // ERROR for qualified names it can't resolve without binder context.
                // Also re-resolve TypeQuery(SymbolRef) types — these are unresolved
                // deferred types cached by TypeNodeChecker that don't incorporate
                // control-flow narrowing.  The CheckerState path resolves them with
                // flow sensitivity (e.g., `typeof c` inside `if (typeof c === 'string')`
                // should yield the narrowed type `string`, not `string | number`).
                if let Some(&cached) = self.ctx.node_types.get(&idx.0)
                    && cached != TypeId::ERROR
                    && self.ctx.type_parameter_scope.is_empty()
                    && crate::query_boundaries::common::get_type_query_symbol_ref(
                        self.ctx.types,
                        cached,
                    )
                    .is_none()
                {
                    return cached;
                }
                let result = self.get_type_from_type_query(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::UNION_TYPE {
                // Handle union types specially to ensure nested typeof expressions
                // are resolved via binder (for abstract class detection)
                // Check cache first - allow re-resolution of ERROR when type params in scope
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_stack.contains(&idx)
                    {
                        return cached;
                    }
                }
                let result = self.get_type_from_union_type(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::INTERSECTION_TYPE {
                // Handle intersection types specially to ensure nested typeof expressions
                // are resolved via binder (same reason as UNION_TYPE above)
                // Check cache first - allow re-resolution of ERROR when type params in scope
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_stack.contains(&idx)
                    {
                        return cached;
                    }
                }
                let result = self.get_type_from_intersection_type(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::TYPE_LITERAL {
                // A type literal lowers monolithically, so a member that is
                // itself a type reference (`{ f: Wrap<typeof a> }`) never routes
                // its `typeof` operand back through the marking path. Record the
                // reads before the cache short-circuits below. See #16680.
                self.mark_nested_type_query_reads(idx);
                // Type literals should use checker resolution so type parameters resolve correctly.
                // Check cache first - allow re-resolution of ERROR when type params in scope
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_stack.contains(&idx)
                    {
                        return cached;
                    }
                }
                let result = self.get_type_from_type_literal(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::TYPE_OPERATOR {
                if let Some(op) = self.ctx.arena.get_type_operator(node)
                    && op.operator == tsz_scanner::SyntaxKind::KeyOfKeyword as u16
                    && let Some(inner_node) = self.ctx.arena.get(op.type_node)
                    && inner_node.kind == syntax_kind_ext::TYPE_REFERENCE
                    && let Some(type_ref) = self.ctx.arena.get_type_ref(inner_node)
                    && self.find_leftmost_import_call(type_ref.type_name).is_some()
                {
                    let imported_operand = {
                        let mut checker = crate::TypeNodeChecker::new(&mut self.ctx);
                        checker.import_call_type_reference(type_ref.type_name)
                    };
                    if let Some(imported_operand) = imported_operand {
                        let result = self.get_keyof_type(imported_operand);
                        self.ctx.node_types.insert(idx.0, result);
                        return result;
                    }
                }

                // Ensure inner type references of keyof/unique/readonly go through
                // the checker's constraint validation path (TS2344). The lowering
                // handles TYPE_OPERATOR via lower_type_operator which calls lower_type
                // on the inner type without going through get_type_from_type_reference.
                // This means `keyof Shared<X, Y>` skips constraint validation on Shared.
                //
                // Only process TYPE_REFERENCE inner types to avoid side effects:
                // processing non-reference types (e.g., plain identifiers, unions)
                // through the checker path can change how keyof types are resolved
                // and printed in diagnostics.
                if let Some(op) = self.ctx.arena.get_type_operator(node)
                    && let Some(inner_node) = self.ctx.arena.get(op.type_node)
                    && inner_node.kind == syntax_kind_ext::TYPE_REFERENCE
                    && self.ctx.arena.get_type_ref(inner_node).is_some_and(|tr| {
                        tr.type_arguments
                            .as_ref()
                            .is_some_and(|args| !args.nodes.is_empty())
                    })
                {
                    let _ = self.get_type_from_type_node(op.type_node);
                }
                // Fall through to TypeNodeChecker for the actual lowering
            }
            if node.kind == syntax_kind_ext::ARRAY_TYPE {
                // Route array types through CheckerState so the element type reference
                // goes through get_type_from_type_node (which checks TS2314 for generics).
                if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                    // Recovery path: malformed value expressions like `number[]` can parse
                    // as ARRAY_TYPE initializers. Emit TS2693 on the primitive keyword.
                    if let Some(ext) = self.ctx.arena.get_extended(idx) {
                        let parent = ext.parent;
                        if parent.is_some()
                            && let Some(parent_node) = self.ctx.arena.get(parent)
                            && matches!(
                                parent_node.kind,
                                k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                                    || k == syntax_kind_ext::LABELED_STATEMENT
                                    || k == syntax_kind_ext::VARIABLE_DECLARATION
                                    || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                                    || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                                    || k == syntax_kind_ext::BINARY_EXPRESSION
                                    || k == syntax_kind_ext::RETURN_STATEMENT
                            )
                            && let Some(elem_node) = self.ctx.arena.get(array_type.element_type)
                        {
                            use tsz_scanner::SyntaxKind;
                            let keyword_name = match elem_node.kind {
                                k if k == SyntaxKind::NumberKeyword as u16 => Some("number"),
                                k if k == SyntaxKind::StringKeyword as u16 => Some("string"),
                                k if k == SyntaxKind::BooleanKeyword as u16 => Some("boolean"),
                                k if k == SyntaxKind::SymbolKeyword as u16 => Some("symbol"),
                                k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                                k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined"),
                                k if k == SyntaxKind::NullKeyword as u16 => Some("null"),
                                k if k == SyntaxKind::AnyKeyword as u16 => Some("any"),
                                k if k == SyntaxKind::UnknownKeyword as u16 => Some("unknown"),
                                k if k == SyntaxKind::NeverKeyword as u16 => Some("never"),
                                k if k == SyntaxKind::ObjectKeyword as u16 => Some("object"),
                                k if k == SyntaxKind::BigIntKeyword as u16 => Some("bigint"),
                                _ => None,
                            };
                            if let Some(keyword_name) = keyword_name {
                                // Route through wrong-meaning boundary: keyword type-only
                                self.report_wrong_meaning_diagnostic(
                                    keyword_name,
                                    array_type.element_type,
                                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                                );
                                self.ctx.node_types.insert(idx.0, TypeId::ERROR);
                                return TypeId::ERROR;
                            }
                        }
                    }

                    // When the element is a parenthesized `typeof` (`(typeof X.y)[]`),
                    // resolve the `typeof` operand directly through the rich,
                    // binder-aware path. A parenthesized element otherwise routes
                    // through the leaner `TypeNodeChecker::check` lowering (via the
                    // delegated PARENTHESIZED_TYPE node), leaving the `typeof`
                    // operand in an under-evaluated deferred form; when its apparent
                    // type is a deeply-nested generic application the subtype
                    // relation then mis-accepts it against a structurally-equal
                    // target via the `isDeeplyNestedType` one-sided expansion
                    // bailout, dropping an expected diagnostic. Unwrapping here
                    // makes `(typeof X.y)[]` relate like `Array<typeof X.y>` and a
                    // `type E = typeof X.y; E[]` alias. Scoped to a `typeof` element
                    // of an array so every other parenthesized/element type keeps
                    // its existing lowering and diagnostics.
                    let element_node = self
                        .unwrap_parenthesized_type_query(array_type.element_type)
                        .unwrap_or(array_type.element_type);
                    let elem_type = self.get_type_from_type_node(element_node);
                    let result = self.ctx.types.factory().array(elem_type);
                    self.ctx.node_types.insert(idx.0, result);
                    return result;
                }
            }
        }

        // Check for unused type parameters (TS6133) in function/constructor type nodes
        let type_params = self
            .ctx
            .arena
            .get(idx)
            .and_then(|n| self.ctx.arena.get_function_type(n))
            .and_then(|fd| fd.type_parameters.clone());
        if let Some(tp) = type_params {
            self.check_unused_type_params(&Some(tp), idx);
        }

        // EXPLICIT WALK: For TYPE_REFERENCE nodes, route through CheckerState's method to emit TS2304.
        // TypeNodeChecker uses TypeLowering which doesn't emit errors, so we must handle TYPE_REFERENCE
        // explicitly here to ensure undefined type names emit TS2304.
        // This fixes cases like `function A(): (public B) => C {}` where C is undefined.
        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::TYPE_REFERENCE
        {
            return self.get_type_from_type_reference(idx);
        }

        // Poison an indexed access whose object is an unresolved-module import
        // to `any`. `tsc` resolves a reference to a generic interface from a
        // module it could not find (TS2307) to `any`, so `T[K]` collapses to
        // `any[K] = any` instead of a deferred `Application(UnresolvedTypeName,
        // …)[K]`. Keeping it deferred false-fails an arrow initializer with
        // TS2322 and suppresses the implicit-`any` parameter diagnostics that
        // `tsc` reports. The object root is detected structurally via
        // `is_unresolved_import_symbol_id` (#13755/#13780), not by name.
        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE
            && self.indexed_access_object_is_unresolved_import(node)
        {
            return TypeId::ANY;
        }

        // Pre-warm namespace-qualified interface types before entering the &self lowering pass.
        // When `import * as L` is used only in type positions, `symbol_types[L.Foo]` stays unset;
        // `ensure_type_alias_resolved_inner` then bails at the TYPE_ALIAS guard for interfaces,
        // leaving an orphan `Lazy(DefId)` (refs #12951).
        self.pre_warm_namespace_qualified_interface_types(idx);
        // A compound type reached here lowers monolithically through
        // `TypeNodeChecker`, so a nested type reference (`(p: Wrap<typeof a>) =>
        // void`, `[Wrap<typeof a>]`, `{ [K in keyof T]: Wrap<typeof a> }`) never
        // routes its `typeof` operand back through the marking path. Record the
        // reads before lowering, mirroring the `TYPE_REFERENCE`/`TYPE_LITERAL`
        // branches above; kinds that recurse per-child (union, intersection,
        // array, conditional) already reach their queries and are no-ops here
        // via the walk's `visited` dedup. See #16680.
        if self
            .ctx
            .arena
            .get(idx)
            .is_some_and(|node| Self::type_node_lowers_monolithically(node.kind))
        {
            self.mark_nested_type_query_reads(idx);
        }
        // For other type nodes, delegate to TypeNodeChecker
        let mut checker = crate::TypeNodeChecker::new(&mut self.ctx);
        let result = checker.check(idx);

        // Post-lowering TS2314 check: TypeNodeChecker uses TypeLowering which doesn't
        // validate that generic types have required type arguments. Walk nested
        // TYPE_REFERENCE nodes in compound types (FUNCTION_TYPE, TYPE_LITERAL, etc.)
        // and emit TS2314 where needed.
        if let Some(node) = self.ctx.arena.get(idx)
            && matches!(
                node.kind,
                k if k == syntax_kind_ext::FUNCTION_TYPE
                    || k == syntax_kind_ext::CONSTRUCTOR_TYPE
                    || k == syntax_kind_ext::TYPE_LITERAL
            )
        {
            self.check_nested_type_refs_for_ts2314(idx);
        }

        result
    }

    /// Whether a type node of `kind`, when reached by the compound fallthrough
    /// of [`Self::get_type_from_type_node`], lowers its whole subtree in one
    /// `TypeNodeChecker` pass — so a nested type reference does not route its
    /// `typeof` operands back through the reference-marking path and must be
    /// walked explicitly (see [`Self::mark_nested_type_query_reads`]). Kinds
    /// that resolve their children through `get_type_from_type_node` again
    /// (union, intersection, array, conditional, parenthesized) are omitted:
    /// their nested queries are already recorded when the child is evaluated.
    const fn type_node_lowers_monolithically(kind: u16) -> bool {
        matches!(
            kind,
            k if k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::CONSTRUCTOR_TYPE
                || k == syntax_kind_ext::TUPLE_TYPE
                || k == syntax_kind_ext::NAMED_TUPLE_MEMBER
                || k == syntax_kind_ext::MAPPED_TYPE
                || k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE
                || k == syntax_kind_ext::INDEXED_ACCESS_TYPE
                || k == syntax_kind_ext::TYPE_OPERATOR
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
        )
    }

    /// Record the value read performed by every `typeof` type query nested in
    /// the type-node subtree rooted at `root`.
    ///
    /// A `TYPE_QUERY`'s entity name resolves in the *value* namespace and is a
    /// genuine read of the binding it names — tsc routes it through
    /// `checkExpressionOrQualifiedName` in `checkTypeQuery`. Reference tracking
    /// for the unused-identifier pass (`referenced_symbols`) records that read
    /// as a side effect of identifier resolution during type-node evaluation —
    /// but only where evaluation actually resolves the operand. Type references
    /// (`Wrap<typeof a>`) and the compound types that lower monolithically
    /// (`FUNCTION_TYPE`, `CONSTRUCTOR_TYPE`, `TUPLE_TYPE`, `TYPE_LITERAL`, …)
    /// resolve their whole subtree in one `lower_with_resolvers` pass whose
    /// value resolver deliberately does not touch `referenced_symbols`, so a
    /// `typeof` nested inside them never recorded its operand read and a
    /// parameter or local read only by such a query was falsely reported unused
    /// (`TS6133`/`TS6196`). This walk resolves each nested query's root
    /// identifier in the value namespace, recording the reads the monolithic
    /// lowering skips — mirroring the direct-`typeof` path. It is called at
    /// each dispatch outcome in [`Self::get_type_from_type_node`] where a
    /// subtree is lowered monolithically (the `TYPE_REFERENCE` and
    /// `TYPE_LITERAL` branches, and the compound-type fallthrough beside
    /// [`Self::check_nested_type_refs_for_ts2314`]); kinds that recurse
    /// per-child (union, intersection, array, conditional) reach their nested
    /// queries on their own and are covered without a walk. The `visited` set
    /// makes re-entry from an enclosing walk idempotent. See #16680.
    pub(crate) fn mark_nested_type_query_reads(&self, root: NodeIndex) {
        use tsz_parser::parser::node::NodeAccess;

        // Same stack/`visited` DFS shape as the sibling `check_nested_type_refs_for_ts2314`.
        // `visited` guards against a node being reached twice (and terminates
        // even if an arena ever presented a cycle).
        let mut stack = vec![root];
        let mut visited = FxHashSet::default();
        while let Some(idx) = stack.pop() {
            if idx.is_none() || !visited.insert(idx) {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TYPE_QUERY
                && let Some(query) = self.ctx.arena.get_type_query(node)
                && let Some(root_ident) = self.leftmost_entity_name_node(query.expr_name)
            {
                // The identifier's parent is the `TYPE_QUERY`, so it is not
                // classified as a type context: this resolves `X` in the value
                // namespace and records the read, exactly as the top-level
                // `typeof X` annotation path does.
                self.resolve_identifier_symbol(root_ident);
            }
            stack.extend(self.ctx.arena.get_children(idx));
        }
    }

    /// Walk the AST subtree rooted at `idx` and emit TS2314 for any
    /// `TYPE_REFERENCE` nodes that reference a generic type without providing
    /// the required type arguments.
    pub(crate) fn check_nested_type_refs_for_ts2314(&mut self, root: NodeIndex) {
        use tsz_parser::parser::node::NodeAccess;

        // Collect type parameter names from function type / constructor type nodes
        // so that type references to them are not falsely flagged as needing type
        // arguments (e.g., `<A>(x: A) => A` where `A` shadows an outer generic).
        let mut type_param_names = FxHashSet::default();
        type_param_names.extend(self.ctx.type_parameter_scope.keys().cloned());
        self.collect_enclosing_type_param_names(root, &mut type_param_names);
        self.collect_type_param_names_from_function_type(root, &mut type_param_names);

        let mut stack = vec![root];
        let mut visited = FxHashSet::default();
        while let Some(idx) = stack.pop() {
            if idx.is_none() || !visited.insert(idx) {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            {
                if type_ref.type_arguments.is_none() {
                    // Skip TS2314 if the reference name matches a local type parameter
                    // introduced by this or a nested function/constructor type.
                    let is_local_type_param = self
                        .ctx
                        .arena
                        .get(type_ref.type_name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .is_some_and(|id| type_param_names.contains(id.escaped_text.as_str()));
                    if !is_local_type_param {
                        self.check_type_ref_requires_args(type_ref.type_name, idx);
                    }
                }
                // Don't descend into TYPE_REFERENCE children to avoid double-checking
                // type arguments (those are separately validated when the outer
                // TYPE_REFERENCE has args).
                continue;
            }
            // Push children for traversal
            stack.extend(self.ctx.arena.get_children(idx));
        }
    }

    /// Recursively collect type parameter names from `FUNCTION_TYPE` and
    /// `CONSTRUCTOR_TYPE` nodes within the given AST subtree.
    fn collect_type_param_names_from_function_type(
        &self,
        root: NodeIndex,
        names: &mut FxHashSet<String>,
    ) {
        use tsz_parser::parser::node::NodeAccess;

        let mut stack = vec![root];
        let mut visited = FxHashSet::default();
        while let Some(idx) = stack.pop() {
            if idx.is_none() || !visited.insert(idx) {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if (node.kind == syntax_kind_ext::FUNCTION_TYPE
                || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE)
                && let Some(func_data) = self.ctx.arena.get_function_type(node)
                && let Some(ref type_params) = func_data.type_parameters
            {
                for &tp_idx in &type_params.nodes {
                    if let Some(tp_node) = self.ctx.arena.get(tp_idx)
                        && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
                        && let Some(name_node) = self.ctx.arena.get(tp_data.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        names.insert(ident.escaped_text.to_string());
                    }
                }
            }
            stack.extend(self.ctx.arena.get_children(idx));
        }
    }

    fn collect_enclosing_type_param_names(&self, root: NodeIndex, names: &mut FxHashSet<String>) {
        let mut parent = self
            .ctx
            .arena
            .get_extended(root)
            .map_or(NodeIndex::NONE, |info| info.parent);

        while parent.is_some() {
            let Some(node) = self.ctx.arena.get(parent) else {
                break;
            };

            if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                && let Some(alias) = self.ctx.arena.get_type_alias(node)
            {
                self.collect_type_param_names_from_list(&alias.type_parameters, names);
            } else if node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                && let Some(interface) = self.ctx.arena.get_interface(node)
            {
                self.collect_type_param_names_from_list(&interface.type_parameters, names);
            } else if node.kind == syntax_kind_ext::CLASS_DECLARATION
                && let Some(class) = self.ctx.arena.get_class(node)
            {
                self.collect_type_param_names_from_list(&class.type_parameters, names);
            }

            parent = self
                .ctx
                .arena
                .get_extended(parent)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }
    }

    fn collect_type_param_names_from_list(
        &self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
        names: &mut FxHashSet<String>,
    ) {
        let Some(type_parameters) = type_parameters else {
            return;
        };

        for &tp_idx in &type_parameters.nodes {
            if let Some(tp_node) = self.ctx.arena.get(tp_idx)
                && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
                && let Some(name_node) = self.ctx.arena.get(tp_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                names.insert(ident.escaped_text.to_string());
            }
        }
    }

    /// Check if a `TYPE_REFERENCE` without type arguments references a generic type
    /// that requires type arguments (TS2314).
    fn check_type_ref_requires_args(&mut self, type_name_idx: NodeIndex, ref_idx: NodeIndex) {
        use crate::symbol_resolver::TypeSymbolResolution;

        let qn_sym_res = self.resolve_qualified_symbol_in_type_position(type_name_idx);
        if let TypeSymbolResolution::Type(sym_id) = qn_sym_res {
            let name = self
                .get_symbol_globally(sym_id)
                .map(|s| s.escaped_name.clone())
                .or_else(|| self.entity_name_text(type_name_idx))
                .unwrap_or_else(|| "<unknown>".to_string());
            let required_count = self
                .count_required_type_params_from_ast(sym_id)
                .unwrap_or_else(|| self.count_required_reference_type_params(sym_id, &name));
            if required_count > 0 {
                let type_params = self.get_reference_type_params_for_symbol(sym_id, &name);
                let display_name = Self::format_generic_display_name_with_interner(
                    &name,
                    &type_params,
                    self.ctx.types,
                );
                self.error_generic_type_requires_type_arguments_at(
                    &display_name,
                    required_count,
                    ref_idx,
                );
            }
        }
    }

    // Report a cannot find name error using solver diagnostics with source tracking.
    // Enhanced to provide suggestions for similar names, import suggestions, and
    // library change suggestions for ES2015+ types.

    // Note: can_merge_symbols is in type_checking.rs

    /// Check if a type name is a built-in mapped type utility.
    /// These are standard TypeScript utility types that transform other types.
    /// When used with type arguments, they should not cause "cannot find type" errors.
    fn augment_js_global_value_type_with_expandos(
        &mut self,
        root_name: &str,
        sym_id: SymbolId,
        base_type: TypeId,
    ) -> TypeId {
        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return base_type;
        }

        let expando_props = self.collect_expando_properties_for_root(root_name);

        if expando_props.is_empty() {
            return base_type;
        }

        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, base_type)
        else {
            return base_type;
        };

        let mut properties = shape.properties.clone();
        let mut changed = false;

        for prop_name in expando_props {
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if properties.iter().any(|prop| prop.name == prop_atom) {
                continue;
            }

            let prop_type =
                self.declared_expando_property_type_for_root(sym_id, root_name, &prop_name);

            properties.push(query::js_expando_property(
                prop_atom,
                prop_type,
                sym_id,
                properties.len() as u32,
            ));
            changed = true;
        }

        if !changed {
            return base_type;
        }

        query::object_with_expando_properties(self.ctx.types, &shape, properties, sym_id)
    }

    pub(crate) fn get_global_this_type(&mut self, error_node: NodeIndex) -> TypeId {
        let mut names: FxHashSet<String> = FxHashSet::default();

        for (name, _) in self.ctx.binder.file_locals.iter() {
            names.insert(name.clone());
        }

        if self.ctx.binder.lib_symbols_are_merged() {
            for &sym_id in self.ctx.binder.lib_symbol_ids.iter() {
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    names.insert(symbol.escaped_name.clone());
                }
            }
        } else {
            for lib_binder in self.get_lib_binders().iter() {
                for (name, _) in lib_binder.file_locals.iter() {
                    names.insert(name.clone());
                }
            }
        }

        names.insert("globalThis".to_string());

        let mut properties = Vec::new();
        for name in names {
            if !self.is_global_this_surface_candidate(&name) {
                continue;
            }

            let type_id = self.resolve_global_this_property_type(
                &name,
                error_node,
                true,
                GlobalReceiver::GlobalThis,
            );
            if type_id == TypeId::ERROR {
                continue;
            }

            let prop_name = self.ctx.types.intern_string(&name);
            properties.push(query::global_this_surface_property(
                prop_name,
                type_id,
                self.resolve_global_value_symbol(&name),
                name == "globalThis",
                properties.len() as u32,
            ));
        }

        query::global_this_surface_object(self.ctx.types, properties)
    }

    fn is_global_this_surface_candidate(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        if name == "globalThis" {
            return true;
        }

        let Some(sym_id) = self.resolve_global_value_symbol(name) else {
            return false;
        };

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            if !symbol.has_any_flags(symbol_flags::VALUE) {
                return false;
            }

            if symbol.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE)
                && !symbol.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE)
            {
                return self.resolve_lib_global_var_symbol(name).is_some();
            }

            return self.symbol_has_globalable_declaration(sym_id, symbol, None);
        }

        self.resolve_lib_global_var_symbol(name).is_some()
    }

    pub(crate) fn collect_expando_properties_for_root(&self, root_name: &str) -> FxHashSet<String> {
        let mut expando_props: FxHashSet<String> = FxHashSet::default();

        if let Some(props) = self.ctx.binder.expando_properties.get(root_name) {
            expando_props.extend(
                props
                    .iter()
                    .map(|prop| self.canonical_expando_property_name(prop)),
            );
        }

        // Use the pre-built global expando index (O(1) lookup) when available,
        // falling back to O(N) all_binders scan only if the index wasn't built.
        if let Some(expando_idx) = &self.ctx.global_expando_index {
            if let Some(props) = expando_idx.get(root_name) {
                expando_props.extend(
                    props
                        .iter()
                        .map(|prop| self.canonical_expando_property_name(prop)),
                );
            }
        } else if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if let Some(props) = binder.expando_properties.get(root_name) {
                    expando_props.extend(
                        props
                            .iter()
                            .map(|prop| self.canonical_expando_property_name(prop)),
                    );
                }
            }
        }

        expando_props
    }

    pub(crate) fn augment_callable_type_with_expandos(
        &mut self,
        root_name: &str,
        sym_id: SymbolId,
        base_type: TypeId,
    ) -> TypeId {
        use rustc_hash::FxHashSet;

        let expando_props = self.collect_expando_properties_for_root(root_name);
        if expando_props.is_empty() {
            return base_type;
        }

        // tsc lists expando members in source (assignment) order. Recover a
        // deterministic order from each property's first same-file assignment
        // position; cross-file properties with no recorded local position sort
        // last, by name, so the ordering stays stable regardless of set hashing.
        let positions = self.expando_property_source_positions(root_name);
        let mut ordered_props: Vec<String> = expando_props.into_iter().collect();
        ordered_props.sort_by(|a, b| match (positions.get(a), positions.get(b)) {
            (Some(pa), Some(pb)) => pa.cmp(pb).then_with(|| a.cmp(b)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
        });

        let Some((callable_shape, mut property_count)) =
            query::callable_shape_for_expando_base(self.ctx.types, base_type, sym_id)
        else {
            return base_type;
        };

        // Append expando members to the existing properties Vec so the resulting
        // shape preserves insertion order (synthesized members like a class's
        // `prototype` first, JS-side expandos last). The previous HashMap-based
        // implementation lost insertion order, which surfaced as wrong property
        // listings in TS2739/TS2740 messages.
        let existing: FxHashSet<tsz_common::interner::Atom> =
            callable_shape.properties.iter().map(|p| p.name).collect();
        let mut new_props = Vec::new();
        let mut seen: FxHashSet<tsz_common::interner::Atom> = FxHashSet::default();

        for prop_name in ordered_props {
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if existing.contains(&prop_atom) || !seen.insert(prop_atom) {
                continue;
            }

            let prop_type =
                self.declared_expando_property_type_for_root(sym_id, root_name, &prop_name);

            new_props.push(query::js_expando_property(
                prop_atom,
                prop_type,
                sym_id,
                property_count,
            ));
            property_count += 1;
        }

        if new_props.is_empty() {
            return base_type;
        }

        query::callable_with_appended_properties(self.ctx.types, callable_shape, new_props)
    }

    /// Provisional twin of [`Self::augment_callable_type_with_expandos`] for use
    /// while `sym_id` is still on the resolution stack (circular re-entry —
    /// see `provisional_circular_function_symbol_type`). Each member's type
    /// comes from [`Self::provisional_expando_property_signature_type`], which
    /// builds a function-valued member's signature structurally instead of
    /// checking its body — the checking path is exactly what re-enters
    /// `sym_id`'s own resolution for `root.member = function () { this... }`.
    /// A non-function member is simply omitted from this transient shape.
    pub(crate) fn augment_provisional_callable_type_with_expando_function_members(
        &mut self,
        root_name: &str,
        sym_id: SymbolId,
        base_type: TypeId,
    ) -> TypeId {
        use rustc_hash::FxHashSet;

        let expando_props = self.collect_expando_properties_for_root(root_name);
        if expando_props.is_empty() {
            return base_type;
        }

        let positions = self.expando_property_source_positions(root_name);
        let mut ordered_props: Vec<String> = expando_props.into_iter().collect();
        ordered_props.sort_by(|a, b| match (positions.get(a), positions.get(b)) {
            (Some(pa), Some(pb)) => pa.cmp(pb).then_with(|| a.cmp(b)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
        });

        let Some((callable_shape, mut property_count)) =
            query::callable_shape_for_expando_base(self.ctx.types, base_type, sym_id)
        else {
            return base_type;
        };

        let existing: FxHashSet<tsz_common::interner::Atom> =
            callable_shape.properties.iter().map(|p| p.name).collect();
        let mut new_props = Vec::new();
        let mut seen: FxHashSet<tsz_common::interner::Atom> = FxHashSet::default();

        for prop_name in ordered_props {
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if existing.contains(&prop_atom) || !seen.insert(prop_atom) {
                continue;
            }

            let Some(prop_type) =
                self.provisional_expando_property_signature_type(sym_id, root_name, &prop_name)
            else {
                continue;
            };

            new_props.push(query::js_expando_property(
                prop_atom,
                prop_type,
                sym_id,
                property_count,
            ));
            property_count += 1;
        }

        if new_props.is_empty() {
            return base_type;
        }

        query::callable_with_appended_properties(self.ctx.types, callable_shape, new_props)
    }

    /// Resolve `name` to a lib-declared global value (`declare var`/`function`/
    /// `class`) and return its value type, if any. Used as the recovery path
    /// for `globalThis.<name>` accesses when the in-scope `<name>` is a
    /// module-local binding that does not contribute a global value (a
    /// block-scoped `const`/`let`, a type-only `import type` alias, or a symbol
    /// without value meaning). `globalThis` exposes only the ambient global
    /// scope, so such module-local bindings must never shadow a real lib global.
    fn resolve_lib_global_var_value_type(&mut self, name: &str) -> Option<TypeId> {
        let lib_sym_id = self.resolve_lib_global_var_symbol(name)?;
        let lib_sym = self.ctx.binder.get_symbol(lib_sym_id).cloned()?;
        for &decl_idx in &lib_sym.declarations {
            if decl_idx.is_none() {
                continue;
            }
            let vt = self.type_of_value_declaration_for_symbol(lib_sym_id, decl_idx);
            if vt != TypeId::UNKNOWN && vt != TypeId::ERROR {
                return Some(self.augment_js_global_value_type_with_expandos(name, lib_sym_id, vt));
            }
        }
        let vt = self.get_type_of_symbol(lib_sym_id);
        if vt != TypeId::UNKNOWN && vt != TypeId::ERROR {
            return Some(self.augment_js_global_value_type_with_expandos(name, lib_sym_id, vt));
        }
        None
    }

    pub(crate) fn resolve_global_this_property_type(
        &mut self,
        name: &str,
        error_node: NodeIndex,
        allow_unknown_property_fallback: bool,
        receiver: GlobalReceiver,
    ) -> TypeId {
        // For "Window & typeof globalThis", first try to resolve the property
        // from the Window interface (the more specific type member).
        // This ensures properties like `name` on Window are found before
        // falling back to globalThis resolution.
        if receiver == GlobalReceiver::WindowAndGlobalThis
            && let Some(window_type) = self.resolve_lib_type_by_name("Window")
        {
            let prop_result = crate::query_boundaries::property_access::resolve_property_access(
                self.ctx.types,
                window_type,
                self.ctx.types.intern_string(name),
            );
            if let Some(type_id) = prop_result.success_type() {
                return type_id;
            }
            // The lib `Window` type does not carry `declare global { interface
            // Window { ... } }` augmentation members (e.g. a computed/string key
            // like `[GLOBAL_TSR]`). Consult the augmentation map before erroring,
            // matching tsc, which finds the member on the `Window` arm of
            // `window: Window & typeof globalThis`.
            if let Some(type_id) = self.resolve_augmentation_property_by_name("Window", name) {
                return type_id;
            }
        }

        if let Some(sym_id) = self.resolve_global_value_symbol(name) {
            if self.alias_resolves_to_type_only(sym_id) {
                // A module-local `import type X` binding never shadows `globalThis.X`:
                // `globalThis` exposes only the ambient global scope. Prefer the lib
                // global `var`/`function`/`class` of the same name if one exists.
                // E.g. `import type Symbol from "./Symbol"` then `globalThis.Symbol()`.
                if let Some(vt) = self.resolve_lib_global_var_value_type(name) {
                    return vt;
                }
                // Route through wrong-meaning boundary: alias resolves to type-only
                self.report_wrong_meaning_diagnostic(
                    name,
                    error_node,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                );
                return TypeId::ERROR;
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                if !symbol.has_any_flags(symbol_flags::VALUE) {
                    // A type-only module-local binding (e.g. an interface or a
                    // type-only import) does not contribute a global value. Prefer
                    // the lib global value of the same name before erroring.
                    if let Some(vt) = self.resolve_lib_global_var_value_type(name) {
                        return vt;
                    }
                    // Route through wrong-meaning boundary: symbol has no value meaning
                    self.report_wrong_meaning_diagnostic(
                        name,
                        error_node,
                        crate::query_boundaries::name_resolution::NameLookupKind::Type,
                    );
                    return TypeId::ERROR;
                }
                // In TypeScript, `typeof globalThis` only exposes `var`-declared
                // globals (FUNCTION_SCOPED_VARIABLE) and function/class declarations.
                // Block-scoped variables (let/const) are NOT properties of globalThis.
                if symbol.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE)
                    && !symbol.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE)
                {
                    // Before erroring, check if a lib `var` declaration exists.
                    // E.g. `const Symbol = globalThis.Symbol` — the local const shadows
                    // the lib `declare var Symbol: SymbolConstructor`, but globalThis
                    // should still resolve to the lib var.
                    if let Some(vt) = self.resolve_lib_global_var_value_type(name) {
                        return vt;
                    }
                    self.error_property_not_exist_on_global_this(name, error_node, receiver);
                    return TypeId::ERROR;
                }
            }
            let base_type = if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let has_type_side = symbol.has_any_flags(symbol_flags::TYPE);
                let has_value_side = symbol.has_any_flags(symbol_flags::VALUE);
                if has_type_side && has_value_side {
                    let value_type = self.type_of_value_symbol_by_name(name);
                    if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                        value_type
                    } else {
                        self.get_type_of_symbol(sym_id)
                    }
                } else {
                    self.get_type_of_symbol(sym_id)
                }
            } else {
                self.get_type_of_symbol(sym_id)
            };
            if name == "window" && base_type == TypeId::ANY {
                return TypeId::UNKNOWN;
            }
            return self.augment_js_global_value_type_with_expandos(name, sym_id, base_type);
        }

        // Self-reference: `globalThis.globalThis` resolves to `typeof globalThis`.
        if name == "globalThis" {
            return TypeId::UNKNOWN;
        }

        if self.is_known_global_value_name(name) {
            // Emit TS2318/TS2583 for missing global type in property access context
            // TS2583 for ES2015+ types, TS2318 for other global types
            use tsz_binder::lib_loader;
            if lib_loader::is_es2015_plus_type(name) {
                self.error_cannot_find_global_type(name, error_node);
            } else {
                // For pre-ES2015 globals, emit TS2318 (global type missing) instead of TS2304
                self.error_cannot_find_global_type(name, error_node);
            }
            return TypeId::ERROR;
        }

        if allow_unknown_property_fallback {
            // For truly unknown properties, return ANY to maintain compatibility with
            // JS expando patterns (e.g., `globalThis.alpha = 4` in checkJs mode).
            // The caller is responsible for emitting TS7017 (dot access) or TS7053
            // (bracket access) when noImplicitAny is enabled.
            TypeId::ANY
        } else {
            self.error_property_not_exist_on_global_this(name, error_node, receiver);
            TypeId::ERROR
        }
    }

    /// Collect `(sym_id, target_file_idx)` pairs for namespace-qualified
    /// interface/class symbols in the subtree rooted at `root` that are not yet
    /// in `symbol_types`.
    ///
    /// Only handles two-level qualified names (`L.Name` where `L` is a star
    /// namespace import).  Deeper nesting (`L.NS.Name`) is skipped.
    fn collect_namespace_qualified_interface_syms(
        &self,
        root: NodeIndex,
    ) -> Vec<(SymbolId, usize)> {
        use tsz_parser::parser::node::NodeAccess;

        let mut result = Vec::new();
        let mut stack = vec![root];
        let mut visited = FxHashSet::default();

        while let Some(idx) = stack.pop() {
            if idx.is_none() || !visited.insert(idx) {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::TYPE_REFERENCE {
                stack.extend(self.ctx.arena.get_children(idx));
                continue;
            }
            let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
                continue;
            };
            let Some(qn_node) = self.ctx.arena.get(type_ref.type_name) else {
                continue;
            };
            if qn_node.kind != syntax_kind_ext::QUALIFIED_NAME {
                continue;
            }
            let Some(qn) = self.ctx.arena.get_qualified_name(qn_node) else {
                continue;
            };
            // Only handle L.Name — left must be a plain identifier, not another qualified name
            let Some(left_ident) = self.ctx.arena.get_identifier_at(qn.left) else {
                continue;
            };
            let left_name = left_ident.escaped_text.as_str();
            let Some(right_ident) = self.ctx.arena.get_identifier_at(qn.right) else {
                continue;
            };
            let right_name = right_ident.escaped_text.as_str();

            let Some(ns_sym_id) = self.ctx.binder.file_locals.get(left_name) else {
                continue;
            };
            let Some(ns_symbol) = self.ctx.binder.get_symbol(ns_sym_id) else {
                continue;
            };
            // Must be a star namespace import: `import * as L from "..."`
            if !ns_symbol.has_any_flags(symbol_flags::ALIAS) || ns_symbol.import_name() != Some("*")
            {
                continue;
            }
            let Some(module_name) = ns_symbol.import_module() else {
                continue;
            };

            let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(self.ctx.current_file_idx, module_name)
            else {
                continue;
            };
            let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
                continue;
            };
            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
            let target_file_name = target_arena
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str());

            let member = target_file_name
                .and_then(|fn_| {
                    target_binder
                        .resolve_import_with_reexports_type_only(fn_, right_name)
                        .map(|(sym_id, _)| sym_id)
                })
                .or_else(|| {
                    target_binder
                        .resolve_import_with_reexports_type_only(module_name, right_name)
                        .map(|(sym_id, _)| sym_id)
                });

            let Some(member_sym_id) = member else {
                continue;
            };
            let Some(member_symbol) = target_binder.get_symbol(member_sym_id) else {
                continue;
            };
            if member_symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::CLASS)
                && !self.ctx.symbol_types.contains_key(&member_sym_id)
            {
                result.push((member_sym_id, target_idx));
            }
        }
        result
    }

    /// Pre-warm `symbol_types` for namespace-qualified interface/class types in
    /// the subtree rooted at `root`.
    ///
    /// When `import * as L from "..."` is used only in type positions, the
    /// namespace object type is never eagerly built, leaving
    /// `symbol_types[L.Validator]` unset.  Later,
    /// `ensure_type_alias_resolved_inner` bails at the `TYPE_ALIAS` guard for
    /// interface symbols, so the `DefId` body is never registered — an orphan
    /// `Lazy(DefId)` causes conditional key-filter mapped types to collapse to
    /// `never` (refs #12951).
    ///
    /// Calling this before `TypeNodeChecker::new` ensures the interface type is
    /// in `symbol_types` so the existing guard in
    /// `ensure_type_alias_resolved_inner` can register the `DefId` body.
    fn pre_warm_namespace_qualified_interface_types(&mut self, root: NodeIndex) {
        let to_warm = self.collect_namespace_qualified_interface_syms(root);
        for (sym_id, target_idx) in to_warm {
            self.ctx.register_symbol_file_target(sym_id, target_idx);
            let type_id = self.get_type_of_symbol(sym_id);
            // Cross-file interface symbols are cached in `cache_cross_file_symbol_type`,
            // not in `symbol_types`. `ensure_type_alias_resolved_inner` checks
            // `symbol_types` to register the `DefId` body, so explicitly populate it.
            if type_id != TypeId::ERROR && type_id != TypeId::UNKNOWN {
                self.ctx.symbol_types.insert(sym_id, type_id);
            }
        }
    }
}
