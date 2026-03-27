//! Type node resolution: converting type annotation AST nodes into `TypeId`
//! representations, plus expando property augmentation and globalThis resolution.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
use tsz_solver::Visibility;
use tsz_solver::{CallSignature, CallableShape};

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
    pub fn get_type_from_type_node(&mut self, idx: NodeIndex) -> TypeId {
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
            // type positions, but some return types (constructors, getters, setters,
            // construct signatures, constructor types) still parse predicates for error
            // recovery. The checker must flag these, matching tsc's getTypePredicateParent.
            if node.kind == syntax_kind_ext::TYPE_PREDICATE {
                let is_valid = self
                    .ctx
                    .arena
                    .get_extended(idx)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .is_some_and(|parent| {
                        matches!(
                            parent.kind,
                            syntax_kind_ext::FUNCTION_DECLARATION
                                | syntax_kind_ext::FUNCTION_EXPRESSION
                                | syntax_kind_ext::METHOD_DECLARATION
                                | syntax_kind_ext::METHOD_SIGNATURE
                                | syntax_kind_ext::CALL_SIGNATURE
                                | syntax_kind_ext::ARROW_FUNCTION
                                | syntax_kind_ext::FUNCTION_TYPE
                        )
                    });
                if !is_valid {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        idx,
                        diagnostic_messages::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO,
                        diagnostic_codes::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO,
                    );
                }
            }

            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                let should_refresh_cached_defaulted_reference =
                    self.ctx.arena.get_type_ref(node).is_some_and(|type_ref| {
                        let has_type_args = type_ref
                            .type_arguments
                            .as_ref()
                            .is_some_and(|args| !args.nodes.is_empty());
                        if has_type_args {
                            return false;
                        }

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
                        use crate::query_boundaries::name_resolution::NameLookupKind;
                        self.report_wrong_meaning_diagnostic(
                            &name,
                            type_ref.type_name,
                            NameLookupKind::Type,
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
                        && !self.ctx.node_resolution_set.contains(&idx)
                        && !should_refresh_cached_defaulted_reference
                    {
                        return cached;
                    }
                    // cached == ERROR but type_parameter_scope is non-empty: re-resolve
                    // cached != ERROR and type_parameter_scope non-empty: re-resolve (type params may differ)
                }
                let result = self.get_type_from_type_reference(idx);
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
                    && tsz_solver::type_queries::get_type_query_symbol_ref(self.ctx.types, cached)
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
                        && !self.ctx.node_resolution_set.contains(&idx)
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
                        && !self.ctx.node_resolution_set.contains(&idx)
                    {
                        return cached;
                    }
                }
                let result = self.get_type_from_intersection_type(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::TYPE_LITERAL {
                // Type literals should use checker resolution so type parameters resolve correctly.
                // Check cache first - allow re-resolution of ERROR when type params in scope
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_set.contains(&idx)
                    {
                        return cached;
                    }
                }
                let result = self.get_type_from_type_literal(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::TYPE_OPERATOR {
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
                                use crate::query_boundaries::name_resolution::NameLookupKind;
                                self.report_wrong_meaning_diagnostic(
                                    keyword_name,
                                    array_type.element_type,
                                    NameLookupKind::Type,
                                );
                                self.ctx.node_types.insert(idx.0, TypeId::ERROR);
                                return TypeId::ERROR;
                            }
                        }
                    }

                    let elem_type = self.get_type_from_type_node(array_type.element_type);
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

    /// Walk the AST subtree rooted at `idx` and emit TS2314 for any
    /// `TYPE_REFERENCE` nodes that reference a generic type without providing
    /// the required type arguments.
    pub(crate) fn check_nested_type_refs_for_ts2314(&mut self, root: NodeIndex) {
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
            if node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            {
                if type_ref.type_arguments.is_none() {
                    // No type arguments provided - check if generic type requires them
                    self.check_type_ref_requires_args(type_ref.type_name, idx);
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

    /// Check if a `TYPE_REFERENCE` without type arguments references a generic type
    /// that requires type arguments (TS2314).
    fn check_type_ref_requires_args(&mut self, type_name_idx: NodeIndex, ref_idx: NodeIndex) {
        use crate::symbol_resolver::TypeSymbolResolution;

        let qn_sym_res = self.resolve_qualified_symbol_in_type_position(type_name_idx);
        if let TypeSymbolResolution::Type(sym_id) = qn_sym_res {
            let required_count = self.count_required_type_params(sym_id);
            if required_count > 0 {
                let name = self
                    .get_symbol_globally(sym_id)
                    .map(|s| s.escaped_name.clone())
                    .or_else(|| self.entity_name_text(type_name_idx))
                    .unwrap_or_else(|| "<unknown>".to_string());
                let type_params = self.get_display_type_params_for_symbol(sym_id);
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
        use tsz_solver::{ObjectShape, PropertyInfo};

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

            properties.push(PropertyInfo {
                name: prop_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: Some(sym_id),
                declaration_order: properties.len() as u32,
            });
            changed = true;
        }

        if !changed {
            return base_type;
        }

        self.ctx.types.factory().object_with_index(ObjectShape {
            flags: shape.flags,
            properties,
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
            symbol: shape.symbol.or(Some(sym_id)),
        })
    }

    pub(crate) fn collect_expando_properties_for_root(&self, root_name: &str) -> FxHashSet<String> {
        let mut expando_props: FxHashSet<String> = FxHashSet::default();

        if let Some(props) = self.ctx.binder.expando_properties.get(root_name) {
            expando_props.extend(props.iter().cloned());
        }

        // Use the pre-built global expando index (O(1) lookup) when available,
        // falling back to O(N) all_binders scan only if the index wasn't built.
        if let Some(expando_idx) = &self.ctx.global_expando_index {
            if let Some(props) = expando_idx.get(root_name) {
                expando_props.extend(props.iter().cloned());
            }
        } else if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if let Some(props) = binder.expando_properties.get(root_name) {
                    expando_props.extend(props.iter().cloned());
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
        use rustc_hash::FxHashMap;
        use tsz_solver::PropertyInfo;

        let expando_props = self.collect_expando_properties_for_root(root_name);
        if expando_props.is_empty() {
            return base_type;
        }

        let (mut callable_shape, mut property_count) = if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, base_type)
        {
            ((*shape).clone(), shape.properties.len())
        } else if let Some(function_shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, base_type)
        {
            let signature = CallSignature {
                type_params: function_shape.type_params.clone(),
                params: function_shape.params.clone(),
                this_type: function_shape.this_type,
                return_type: function_shape.return_type,
                type_predicate: function_shape.type_predicate.clone(),
                is_method: function_shape.is_method,
            };
            (
                CallableShape {
                    call_signatures: if function_shape.is_constructor {
                        Vec::new()
                    } else {
                        vec![signature.clone()]
                    },
                    construct_signatures: if function_shape.is_constructor {
                        vec![signature]
                    } else {
                        Vec::new()
                    },
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: Some(sym_id),
                    is_abstract: false,
                },
                0,
            )
        } else {
            return base_type;
        };

        let mut properties: FxHashMap<tsz_common::interner::Atom, PropertyInfo> = callable_shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();
        let mut changed = false;

        for prop_name in expando_props {
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if properties.contains_key(&prop_atom) {
                continue;
            }

            let prop_type =
                self.declared_expando_property_type_for_root(sym_id, root_name, &prop_name);

            properties.insert(
                prop_atom,
                PropertyInfo {
                    name: prop_atom,
                    type_id: prop_type,
                    write_type: prop_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: Some(sym_id),
                    declaration_order: property_count as u32,
                },
            );
            property_count += 1;
            changed = true;
        }

        if !changed {
            return base_type;
        }

        callable_shape.properties = properties.into_values().collect();
        self.ctx.types.factory().callable(callable_shape)
    }

    pub(crate) fn resolve_global_this_property_type(
        &mut self,
        name: &str,
        error_node: NodeIndex,
        allow_unknown_property_fallback: bool,
        base_display: &str,
    ) -> TypeId {
        if let Some(sym_id) = self.resolve_global_value_symbol(name) {
            if self.alias_resolves_to_type_only(sym_id) {
                // Route through wrong-meaning boundary: alias resolves to type-only
                use crate::query_boundaries::name_resolution::NameLookupKind;
                self.report_wrong_meaning_diagnostic(name, error_node, NameLookupKind::Type);
                return TypeId::ERROR;
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                if (symbol.flags & symbol_flags::VALUE) == 0 {
                    // Route through wrong-meaning boundary: symbol has no value meaning
                    use crate::query_boundaries::name_resolution::NameLookupKind;
                    self.report_wrong_meaning_diagnostic(name, error_node, NameLookupKind::Type);
                    return TypeId::ERROR;
                }
                // In TypeScript, `typeof globalThis` only exposes `var`-declared
                // globals (FUNCTION_SCOPED_VARIABLE) and function/class declarations.
                // Block-scoped variables (let/const) are NOT properties of globalThis.
                if symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                    && symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE == 0
                {
                    self.error_property_not_exist_on_global_this(name, error_node, base_display);
                    return TypeId::ERROR;
                }
            }
            let base_type = if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let has_type_side = (symbol.flags & symbol_flags::TYPE) != 0;
                let has_value_side = (symbol.flags & symbol_flags::VALUE) != 0;
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
            self.error_property_not_exist_on_global_this(name, error_node, base_display);
            TypeId::ERROR
        }
    }
}
