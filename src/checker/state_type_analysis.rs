//! Type Analysis & Compatibility Module
//!
//! Extracted from state.rs: Methods for type analysis including qualified name
//! resolution, symbol type computation, type queries, and contextual literal type analysis.

use crate::binder::{SymbolId, symbol_flags};
use crate::checker::state::CheckerState;
use crate::checker::symbol_resolver::TypeSymbolResolution;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;
use rustc_hash::FxHashSet;
use tracing::{debug, trace};

impl<'a> CheckerState<'a> {
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

        // Resolve the left side (could be Identifier or another QualifiedName)
        let left_type = if let Some(left_node) = self.ctx.arena.get(qn.left) {
            if left_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                self.resolve_qualified_name(qn.left)
            } else if left_node.kind == SyntaxKind::Identifier as u16 {
                // Resolve identifier as a type reference
                self.get_type_from_type_reference_by_name(qn.left)
            } else {
                TypeId::ERROR // Unknown node kind - propagate error
            }
        } else {
            TypeId::ERROR // Missing left node - propagate error
        };

        if left_type == TypeId::ANY || left_type == TypeId::ERROR {
            return TypeId::ERROR; // Propagate error from left side
        }

        // Get the right side name (B in A.B)
        let right_name = if let Some(right_node) = self.ctx.arena.get(qn.right) {
            if let Some(id) = self.ctx.arena.get_identifier(right_node) {
                id.escaped_text.clone()
            } else {
                return TypeId::ERROR; // Missing identifier data - propagate error
            }
        } else {
            return TypeId::ERROR; // Missing right node - propagate error
        };

        // Collect lib binders for cross-arena symbol lookup (fixes TS2694 false positives)
        let lib_binders = self.get_lib_binders();

        // First, try to resolve the left side as a symbol and check its exports.
        // This handles merged class+namespace, function+namespace, and enum+namespace symbols.
        let mut member_sym_id_from_symbol = None;
        if let Some(left_node) = self.ctx.arena.get(qn.left)
            && left_node.kind == SyntaxKind::Identifier as u16
        {
            if let TypeSymbolResolution::Type(sym_id) =
                self.resolve_identifier_symbol_in_type_position(qn.left)
            {
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    // Try direct exports first
                    if let Some(ref exports) = symbol.exports
                        && let Some(member_id) = exports.get(&right_name)
                    {
                        member_sym_id_from_symbol = Some(member_id);
                    }
                    // For classes, also check members (for static members in type queries)
                    // This handles `typeof C.staticMember` where C is a class
                    else if member_sym_id_from_symbol.is_none()
                        && symbol.flags & symbol_flags::CLASS != 0
                    {
                        if let Some(ref members) = symbol.members {
                            member_sym_id_from_symbol = members.get(&right_name);
                        }
                    }
                    // If not found in direct exports, check for re-exports
                    else if let Some(ref _exports) = symbol.exports {
                        // The member might be re-exported from another module
                        // Check if this symbol has an import_module (it's an imported namespace)
                        if let Some(ref module_specifier) = symbol.import_module {
                            // Try to resolve the member through the re-export chain
                            if let Some(reexported_sym_id) = self.resolve_reexported_member(
                                module_specifier,
                                &right_name,
                                &lib_binders,
                            ) {
                                member_sym_id_from_symbol = Some(reexported_sym_id);
                            }
                        }
                    }
                }
            }
        }

        // If found via symbol resolution, use it
        if let Some(member_sym_id) = member_sym_id_from_symbol {
            if let Some(member_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(member_sym_id, &lib_binders)
            {
                let is_namespace = member_symbol.flags & symbol_flags::MODULE != 0;
                if !is_namespace
                    && (self.alias_resolves_to_value_only(member_sym_id, Some(right_name.as_str()))
                        || self.symbol_is_value_only(member_sym_id, Some(right_name.as_str())))
                    && !self.symbol_is_type_only(member_sym_id, Some(right_name.as_str()))
                {
                    self.error_value_only_type_at(&right_name, qn.right);
                    return TypeId::ERROR;
                }
            }
            return self.type_reference_symbol_type(member_sym_id);
        }

        // Otherwise, fall back to type-based lookup for pure namespace/module types
        // Look up the member in the left side's exports
        // Supports both legacy Ref(SymbolRef) and new Lazy(DefId) types
        let fallback_sym_id = if let Some(sym_ref) =
            crate::solver::type_queries::get_symbol_ref(self.ctx.types, left_type)
        {
            Some(crate::binder::SymbolId(sym_ref.0))
        } else if let Some(def_id) =
            crate::solver::type_queries_extended::get_def_id(self.ctx.types, left_type)
        {
            self.ctx.def_to_symbol_id(def_id)
        } else {
            None
        };

        if let Some(fallback_sym) = fallback_sym_id
            && let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(fallback_sym, &lib_binders)
        {
            // Check exports table for direct export
            let mut member_sym_id = None;
            if let Some(ref exports) = symbol.exports {
                member_sym_id = exports.get(&right_name);
            }

            // For classes, also check members (for static members in type queries)
            // This handles `typeof C.staticMember` where C is a class
            if member_sym_id.is_none() && symbol.flags & symbol_flags::CLASS != 0 {
                if let Some(ref members) = symbol.members {
                    member_sym_id = members.get(&right_name);
                }
            }

            // If not found in direct exports, check for re-exports
            if member_sym_id.is_none() {
                // The symbol might be an imported namespace - check if it has an import_module
                if let Some(ref module_specifier) = symbol.import_module {
                    if let Some(reexported_sym_id) =
                        self.resolve_reexported_member(module_specifier, &right_name, &lib_binders)
                    {
                        member_sym_id = Some(reexported_sym_id);
                    }
                }
            }

            if let Some(member_sym_id) = member_sym_id {
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
                        self.error_value_only_type_at(&right_name, qn.right);
                        return TypeId::ERROR;
                    }
                }
                return self.type_reference_symbol_type(member_sym_id);
            }

            // Not found - report TS2694
            let namespace_name = self
                .entity_name_text(qn.left)
                .unwrap_or_else(|| symbol.escaped_name.clone());
            self.error_namespace_no_export(&namespace_name, &right_name, qn.right);
            return TypeId::ERROR;
        }

        // Left side wasn't a reference to a namespace/module
        // This is likely an error - the left side should resolve to a namespace
        // Emit an appropriate error for the unresolved qualified name
        // We don't emit TS2304 here because the left side might have already emitted an error
        // Returning ERROR prevents cascading errors while still indicating failure
        TypeId::ERROR
    }

    /// Helper to resolve an identifier as a type reference (for qualified name left sides).
    pub(crate) fn get_type_from_type_reference_by_name(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            let name = &ident.escaped_text;

            if let TypeSymbolResolution::Type(sym_id) =
                self.resolve_identifier_symbol_in_type_position(idx)
            {
                // Reference tracking is handled by resolve_identifier_symbol_in_type_position wrapper
                return self.type_reference_symbol_type(sym_id);
            }

            // Not found - but suppress TS2304 if this is an unresolved import
            // (TS2307 was already emitted for the import statement)
            if self.is_unresolved_import_symbol(idx) {
                return TypeId::ANY;
            }
            self.error_cannot_find_name_at(name, idx);
            return TypeId::ERROR;
        }

        TypeId::ERROR // Not an identifier - propagate error
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
    /// Get type from a type query node (typeof X).
    ///
    /// Creates a TypeQuery type that captures the type of a value, enabling type-level
    /// queries and conditional type logic.
    ///
    /// ## Resolution Strategy:
    /// 1. **Value symbol resolved** (typeof value):
    ///    - Without type args: Return the actual type directly
    ///    - With type args: Create TypeQuery type for deferred resolution
    ///    - Exception: ANY/ERROR types still create TypeQuery for proper error handling
    ///
    /// 2. **Type symbol only**: Emit TS2504 error (type cannot be used as value)
    ///
    /// 3. **Unknown identifier**:
    ///    - Known global value → return ANY (allows property access)
    ///    - Unresolved import → return ANY (TS2307 already emitted)
    ///    - Otherwise → emit TS2304 error and return ERROR
    ///
    /// 4. **Missing member** (typeof obj.prop): Emit appropriate error
    ///
    /// 5. **Fallback**: Hash the name for forward compatibility
    ///
    /// ## Type Arguments:
    /// - If present, creates TypeApplication(base, args)
    /// - Used in generic type queries: `typeof Array<string>`
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// let x = 42;
    /// type T1 = typeof x;  // number
    ///
    /// function foo(): string { return "hello"; }
    /// type T2 = typeof foo;  // () => string
    ///
    /// class MyClass {
    ///   prop = 123;
    /// }
    /// type T3 = typeof MyClass;  // typeof MyClass (constructor type)
    /// type T4 = MyClass;  // MyClass (instance type)
    ///
    /// // Type query with type arguments (advanced)
    /// type T5 = typeof Array<string>;  // typeof Array with type args
    /// ```
    pub(crate) fn get_type_from_type_query(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{SymbolRef, TypeKey};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR; // Missing type query data - propagate error
        };

        let name_text = self.entity_name_text(type_query.expr_name);
        let is_identifier = self
            .ctx
            .arena
            .get(type_query.expr_name)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some();
        let has_type_args = type_query
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());

        let base =
            if let Some(sym_id) = self.resolve_value_symbol_for_lowering(type_query.expr_name) {
                trace!("=== get_type_from_type_query ===");
                trace!(name = ?name_text, sym_id, "get_type_from_type_query");

                // Always compute the symbol type to ensure it's in the type environment
                // This is important for Application resolution and TypeQuery resolution during subtype checking
                let resolved = self.get_type_of_symbol(crate::binder::SymbolId(sym_id));
                trace!(resolved = ?resolved, "resolved type");

                if !has_type_args && resolved != TypeId::ANY && resolved != TypeId::ERROR {
                    // Return resolved type directly when there are no type arguments
                    trace!("=> returning resolved type directly");
                    return resolved;
                }

                // For type arguments or when resolved is ANY/ERROR, use TypeQuery
                let typequery_type = self.ctx.types.intern(TypeKey::TypeQuery(SymbolRef(sym_id)));
                trace!(typequery_type = ?typequery_type, "=> returning TypeQuery type");
                typequery_type
            } else if self
                .resolve_type_symbol_for_lowering(type_query.expr_name)
                .is_some()
            {
                let name = name_text.as_deref().unwrap_or("<unknown>");
                self.error_type_only_value_at(name, type_query.expr_name);
                return TypeId::ERROR;
            } else if let Some(name) = name_text {
                if is_identifier {
                    // Handle global intrinsics that may not have symbols in the binder
                    // (e.g., `typeof undefined`, `typeof NaN`, `typeof Infinity`)
                    match name.as_str() {
                        "undefined" => return TypeId::UNDEFINED,
                        "NaN" | "Infinity" => return TypeId::NUMBER,
                        _ => {}
                    }
                    if self.is_known_global_value_name(&name) {
                        // Emit TS2318/TS2583 for missing global type in typeof context
                        // TS2583 for ES2015+ types, TS2304 for other globals
                        use crate::lib_loader;
                        if lib_loader::is_es2015_plus_type(&name) {
                            self.error_cannot_find_global_type(&name, type_query.expr_name);
                        } else {
                            self.error_cannot_find_name_at(&name, type_query.expr_name);
                        }
                        return TypeId::ERROR;
                    }
                    // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                    if self.is_unresolved_import_symbol(type_query.expr_name) {
                        return TypeId::ANY;
                    }
                    self.error_cannot_find_name_at(&name, type_query.expr_name);
                    return TypeId::ERROR;
                }
                if let Some(missing_idx) = self.missing_type_query_left(type_query.expr_name)
                    && let Some(missing_name) = self
                        .ctx
                        .arena
                        .get(missing_idx)
                        .and_then(|node| self.ctx.arena.get_identifier(node))
                        .map(|ident| ident.escaped_text.clone())
                {
                    // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                    if self.is_unresolved_import_symbol(missing_idx) {
                        return TypeId::ANY;
                    }
                    self.error_cannot_find_name_at(&missing_name, missing_idx);
                    return TypeId::ERROR;
                }
                if self.report_type_query_missing_member(type_query.expr_name) {
                    return TypeId::ERROR;
                }
                // Not found - fall back to hash (for forward compatibility)
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                name.hash(&mut hasher);
                let symbol_id = hasher.finish() as u32;
                self.ctx
                    .types
                    .intern(TypeKey::TypeQuery(SymbolRef(symbol_id)))
            } else {
                return TypeId::ERROR; // No name text - propagate error
            };

        if let Some(args) = &type_query.type_arguments
            && !args.nodes.is_empty()
        {
            let type_args = args
                .nodes
                .iter()
                .map(|&idx| self.get_type_from_type_node(idx))
                .collect();
            return self.ctx.types.application(base, type_args);
        }

        base
    }

    /// Get type of a JSX opening element.
    ///
    // NOTE: get_type_of_jsx_opening_element, get_jsx_namespace_type,
    // get_intrinsic_elements_type, get_jsx_element_type moved to jsx_checker.rs

    // NOTE: get_type_from_type_node_in_type_literal, get_type_from_type_reference_in_type_literal,
    // extract_params_from_signature_in_type_literal, get_type_from_type_literal
    // moved to type_literal_checker.rs

    /// Push type parameters into scope for generic type resolution.
    ///
    /// This is a critical function for handling generic types (classes, interfaces,
    /// functions, type aliases). It makes type parameters available for use within
    /// the generic type's body and returns information for later scope restoration.
    ///
    /// ## Two-Pass Algorithm:
    /// 1. **First pass**: Adds all type parameters to scope WITHOUT constraints
    ///    - This allows self-referential constraints like `T extends Box<T>`
    ///    - Creates unconstrained TypeParameter entries
    /// 2. **Second pass**: Resolves constraints and defaults with all params in scope
    ///    - Now all type parameters are visible for constraint resolution
    ///    - Updates the scope with constrained TypeParameter entries
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
        type_parameters: &Option<crate::parser::NodeList>,
    ) -> (
        Vec<crate::solver::TypeParamInfo>,
        Vec<(String, Option<TypeId>)>,
    ) {
        use crate::solver::TypeKey;

        let Some(list) = type_parameters else {
            return (Vec::new(), Vec::new());
        };

        // Recursion depth check: prevent stack overflow from circular type parameter
        // references (e.g. interface I<T extends I<T>> {} or circular generic defaults)
        if !self.ctx.enter_recursion() {
            return (Vec::new(), Vec::new());
        }

        let mut params = Vec::new();
        let mut updates = Vec::new();
        let mut param_indices = Vec::new();

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
                .map(|id_data| id_data.escaped_text.clone())
                .unwrap_or_else(|| "T".to_string());
            let atom = self.ctx.types.intern_string(&name);

            // Create unconstrained type parameter initially
            let info = crate::solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
            };
            let type_id = self.ctx.types.intern(TypeKey::TypeParameter(info));
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous));
            param_indices.push(param_idx);
        }

        // Second pass: Now resolve constraints and defaults with all type parameters in scope
        for &param_idx in param_indices.iter() {
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
                .map(|id_data| id_data.escaped_text.clone())
                .unwrap_or_else(|| "T".to_string());
            let atom = self.ctx.types.intern_string(&name);

            let constraint = if data.constraint != NodeIndex::NONE {
                // Check for circular constraint: T extends T
                // First get the constraint type ID
                let constraint_type = self.get_type_from_type_node(data.constraint);

                // Check if the constraint references the same type parameter
                let is_circular =
                    if let Some(&param_type_id) = self.ctx.type_parameter_scope.get(&name) {
                        // Check if constraint_type is the same as the type parameter
                        // or if it's a TypeReference that resolves to this type parameter
                        self.is_same_type_parameter(constraint_type, param_type_id, &name)
                    } else {
                        false
                    };

                if is_circular {
                    // TS2313: Type parameter 'T' has a circular constraint
                    self.error_at_node(
                        data.constraint,
                        &format!("Type parameter '{}' has a circular constraint.", name),
                        crate::checker::types::diagnostics::diagnostic_codes::CONSTRAINT_OF_TYPE_PARAMETER,
                    );
                    Some(TypeId::UNKNOWN)
                } else {
                    // Note: Even if constraint_type is ERROR, we don't emit an error here
                    // because the error for the unresolved type was already emitted by get_type_from_type_node.
                    // This prevents duplicate error messages.
                    Some(constraint_type)
                }
            } else {
                None
            };

            let default = if data.default != NodeIndex::NONE {
                let default_type = self.get_type_from_type_node(data.default);
                // Validate that default satisfies constraint if present
                if let Some(constraint_type) = constraint
                    && default_type != TypeId::ERROR
                    && !self.is_assignable_to(default_type, constraint_type)
                {
                    self.error_at_node(
                            data.default,
                            crate::checker::types::diagnostics::diagnostic_messages::TYPE_NOT_SATISFY_CONSTRAINT,
                            crate::checker::types::diagnostics::diagnostic_codes::TYPE_PARAMETER_CONSTRAINT_NOT_SATISFIED,
                        );
                }
                if default_type == TypeId::ERROR {
                    None
                } else {
                    Some(default_type)
                }
            } else {
                None
            };

            let info = crate::solver::TypeParamInfo {
                name: atom,
                constraint,
                default,
            };
            params.push(info.clone());

            // UPDATE: Create a new TypeParameter with constraints and update the scope
            // This ensures that when function parameters reference these type parameters,
            // they get the constrained version, not the unconstrained placeholder
            let constrained_type_id = self.ctx.types.intern(TypeKey::TypeParameter(info));
            self.ctx
                .type_parameter_scope
                .insert(name.clone(), constrained_type_id);
        }

        self.ctx.leave_recursion();
        (params, updates)
    }

    /// Check if a constraint type is the same as a type parameter (circular constraint).
    ///
    /// This detects cases like `T extends T` where the type parameter references itself
    /// in its own constraint.
    pub(crate) fn is_same_type_parameter(
        &self,
        constraint_type: TypeId,
        param_type_id: TypeId,
        param_name: &str,
    ) -> bool {
        use crate::solver::TypeKey;

        // Direct match
        if constraint_type == param_type_id {
            return true;
        }

        // Check if constraint is a TypeParameter with the same name
        if let Some(type_key) = self.ctx.types.lookup(constraint_type) {
            if let TypeKey::TypeParameter(info) = type_key {
                // Check if the type parameter name matches
                let name_str = self.ctx.types.resolve_atom(info.name);
                if name_str == param_name {
                    return true;
                }
            }
        }

        false
    }

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
        use crate::solver::SymbolRef;

        self.record_symbol_dependency(sym_id);

        // Check cache first
        if let Some(&cached) = self.ctx.symbol_types.get(&sym_id) {
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
            // CRITICAL: Cache ERROR immediately to prevent repeated deep recursion
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

        // CRITICAL: Pre-cache a placeholder (ERROR) to break deep recursion chains
        // This prevents stack overflow in circular class inheritance by ensuring
        // that when we try to resolve this symbol again mid-resolution, we get
        // the cached ERROR immediately instead of recursing deeper.
        // We'll overwrite this with the real result later (line 3098).
        self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);

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

        // Also populate the type environment for Application expansion
        // IMPORTANT: We use the type_params returned by compute_type_of_symbol
        // because those are the same TypeIds used when lowering the type body.
        // Calling get_type_params_for_symbol would create fresh TypeIds that don't match.
        if result != TypeId::ANY && result != TypeId::ERROR {
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
                let def_id = self.ctx.symbol_to_def.get(&sym_id).copied();

                if let Some((instance_type, class_params)) = class_env_entry {
                    if class_params.is_empty() {
                        env.insert(SymbolRef(sym_id.0), instance_type);
                        // Also register with DefId for Lazy type resolution
                        if let Some(def_id) = def_id {
                            env.insert_def(def_id, instance_type);
                        }
                    } else {
                        env.insert_with_params(
                            SymbolRef(sym_id.0),
                            instance_type,
                            class_params.clone(),
                        );
                        if let Some(def_id) = def_id {
                            env.insert_def_with_params(def_id, instance_type, class_params);
                        }
                    }
                } else if type_params.is_empty() {
                    env.insert(SymbolRef(sym_id.0), result);
                    if let Some(def_id) = def_id {
                        env.insert_def(def_id, result);
                    }
                } else {
                    env.insert_with_params(SymbolRef(sym_id.0), result, type_params.clone());
                    if let Some(def_id) = def_id {
                        env.insert_def_with_params(def_id, result, type_params);
                    }
                }
            }
        }

        result
    }

    /// Get a symbol from the current binder, lib binders, or other file binders.
    /// This ensures we can resolve symbols from lib.d.ts and other files.
    pub(crate) fn get_symbol_globally(&self, sym_id: SymbolId) -> Option<&crate::binder::Symbol> {
        // 1. Check current file
        if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
            return Some(sym);
        }
        // 2. Check lib files (lib.d.ts, etc.)
        for lib in &self.ctx.lib_contexts {
            if let Some(sym) = lib.binder.get_symbol(sym_id) {
                return Some(sym);
            }
        }
        // 3. Check other files in the project (multi-file mode)
        if let Some(binders) = &self.ctx.all_binders {
            for binder in binders {
                if let Some(sym) = binder.get_symbol(sym_id) {
                    return Some(sym);
                }
            }
        }
        None
    }

    /// Compute type of a symbol (internal, not cached).
    ///
    /// Uses TypeLowering to bridge symbol declarations to solver types.
    /// Returns the computed type and the type parameters used (if any).
    /// IMPORTANT: The type params returned must be the same ones used when lowering
    /// the type body, so that instantiation works correctly.
    pub(crate) fn compute_type_of_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<crate::solver::TypeParamInfo>) {
        use crate::solver::TypeLowering;

        // Handle cross-file symbol resolution: if this symbol's arena is different
        // from the current arena, delegate to a checker using the correct arena.
        if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id)
            && !std::ptr::eq(symbol_arena.as_ref(), self.ctx.arena)
        {
            let mut checker = CheckerState::new(
                symbol_arena.as_ref(),
                self.ctx.binder,
                self.ctx.types,
                self.ctx.file_name.clone(),
                self.ctx.compiler_options.clone(),
            );
            // Copy lib contexts for global symbol resolution (Array, Promise, etc.)
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            // Copy symbol resolution state to detect cross-file cycles, but exclude
            // the current symbol (which the parent added) since this checker will
            // add it again during get_type_of_symbol
            for &id in &self.ctx.symbol_resolution_set {
                if id != sym_id {
                    checker.ctx.symbol_resolution_set.insert(id);
                }
            }
            // Copy class_instance_resolution_set to detect circular class inheritance
            for &id in &self.ctx.class_instance_resolution_set {
                checker.ctx.class_instance_resolution_set.insert(id);
            }
            // Use get_type_of_symbol to ensure proper cycle detection
            let result = checker.get_type_of_symbol(sym_id);
            return (result, Vec::new());
        }

        // Use get_symbol_globally to find symbols in lib files and other files
        // Extract needed data to avoid holding borrow across mutable operations
        let (flags, value_decl, declarations, import_module, import_name, escaped_name) =
            match self.get_symbol_globally(sym_id) {
                Some(symbol) => (
                    symbol.flags,
                    symbol.value_declaration,
                    symbol.declarations.clone(),
                    symbol.import_module.clone(),
                    symbol.import_name.clone(),
                    symbol.escaped_name.clone(),
                ),
                None => return (TypeId::UNKNOWN, Vec::new()),
            };

        // Class - return class constructor type (merging namespace exports when present)
        if flags & symbol_flags::CLASS != 0 {
            let decl_idx = if !value_decl.is_none() {
                value_decl
            } else {
                declarations.first().copied().unwrap_or(NodeIndex::NONE)
            };
            if !decl_idx.is_none()
                && let Some(node) = self.ctx.arena.get(decl_idx)
                && let Some(class) = self.ctx.arena.get_class(node)
            {
                let ctor_type = self.get_class_constructor_type(decl_idx, class);
                if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                    let merged = self.merge_namespace_exports_into_constructor(sym_id, ctor_type);
                    return (merged, Vec::new());
                }
                return (ctor_type, Vec::new());
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Namespace / Module
        // Return a Ref type AND register DefId mapping for gradual migration.
        // The Ref type is needed because resolve_qualified_name and other code
        // extracts SymbolRef from the type to look up the symbol's exports map.
        // Skip this when the symbol is also a FUNCTION — the FUNCTION branch below
        // handles merging namespace exports into the function's callable type.
        if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
            && flags & symbol_flags::FUNCTION == 0
        {
            use crate::solver::{SymbolRef, TypeKey};
            // Also create DefId mapping for future migration
            let _ = self.ctx.get_or_create_def_id(sym_id);
            return (
                self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0))),
                Vec::new(),
            );
        }

        // Enum - return a Ref type AND register DefId mapping for gradual migration.
        // The Ref type is needed because enum subtype checking depends on extracting
        // SymbolRef to check symbol flags and nominal identity.
        if flags & symbol_flags::ENUM != 0 {
            use crate::solver::{SymbolRef, TypeKey};
            // Also create DefId mapping for future migration
            let _ = self.ctx.get_or_create_def_id(sym_id);
            return (
                self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0))),
                Vec::new(),
            );
        }

        // Enum member - determine type from parent enum
        if flags & symbol_flags::ENUM_MEMBER != 0 {
            // Find the parent enum by walking up to find the containing enum declaration
            let member_type = self.enum_member_type_from_decl(value_decl);
            return (member_type, Vec::new());
        }

        // Function - build function type or callable overload set
        if flags & symbol_flags::FUNCTION != 0 {
            use crate::solver::CallableShape;

            let mut overloads = Vec::new();
            let mut implementation_decl = NodeIndex::NONE;

            for &decl_idx in &declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(func) = self.ctx.arena.get_function(node) else {
                    continue;
                };

                if func.body.is_none() {
                    overloads.push(self.call_signature_from_function(func, decl_idx));
                } else {
                    implementation_decl = decl_idx;
                }
            }

            let function_type = if !overloads.is_empty() {
                let shape = CallableShape {
                    call_signatures: overloads,
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                };
                self.ctx.types.callable(shape)
            } else if !value_decl.is_none() {
                self.get_type_of_function(value_decl)
            } else if !implementation_decl.is_none() {
                self.get_type_of_function(implementation_decl)
            } else {
                TypeId::UNKNOWN
            };

            // If function is merged with namespace, merge namespace exports into function type
            // This allows accessing namespace members through the function name: Model.Options
            if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                return self.merge_namespace_exports_into_function(sym_id, function_type);
            }

            return (function_type, Vec::new());
        }

        // Interface - return interface type with call signatures
        if flags & symbol_flags::INTERFACE != 0 {
            if !declarations.is_empty() {
                // Get type parameters from the first interface declaration
                let mut params = Vec::new();
                let mut updates = Vec::new();

                // Try to get type parameters from the interface declaration
                let first_decl = declarations.first().copied().unwrap_or(NodeIndex::NONE);
                if !first_decl.is_none() {
                    if let Some(node) = self.ctx.arena.get(first_decl) {
                        if let Some(interface) = self.ctx.arena.get_interface(node) {
                            (params, updates) =
                                self.push_type_parameters(&interface.type_parameters);
                        }
                    } else if std::env::var("TSZ_DEBUG_IMPORTS").is_ok() {
                        debug!(
                            name = %escaped_name,
                            sym_id = sym_id.0,
                            first_decl = ?first_decl,
                            arena_len = self.ctx.arena.len(),
                            "[DEBUG] Interface first_decl NOT FOUND in arena"
                        );
                    }
                }

                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let lowering = TypeLowering::with_resolvers(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let interface_type = lowering.lower_interface_declarations(&declarations);

                // Restore the type parameter scope
                self.pop_type_parameters(updates);

                // Return the interface type along with the type parameters that were used
                return (
                    self.merge_interface_heritage_types(&declarations, interface_type),
                    params,
                );
            }
            if !value_decl.is_none() {
                return (self.get_type_of_interface(value_decl), Vec::new());
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Type alias - resolve using checker's get_type_from_type_node to properly resolve symbols
        if flags & symbol_flags::TYPE_ALIAS != 0 {
            // Get the type node from the type alias declaration
            let decl_idx = if !value_decl.is_none() {
                value_decl
            } else {
                declarations.first().copied().unwrap_or(NodeIndex::NONE)
            };
            if !decl_idx.is_none()
                && let Some(node) = self.ctx.arena.get(decl_idx)
                && let Some(type_alias) = self.ctx.arena.get_type_alias(node)
            {
                let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                let alias_type = self.get_type_from_type_node(type_alias.type_node);
                self.pop_type_parameters(updates);
                // Return the params that were used during lowering - this ensures
                // type_env gets the same TypeIds as the type body
                return (alias_type, params);
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Variable - get type from annotation or infer from initializer
        if flags & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            != 0
        {
            if !value_decl.is_none()
                && let Some(node) = self.ctx.arena.get(value_decl)
            {
                // Check if this is a variable declaration
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
                    // First try type annotation using type-node lowering (resolves through binder).
                    if !var_decl.type_annotation.is_none() {
                        return (
                            self.get_type_from_type_node(var_decl.type_annotation),
                            Vec::new(),
                        );
                    }
                    if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(value_decl) {
                        return (jsdoc_type, Vec::new());
                    }
                    if !var_decl.initializer.is_none()
                        && self.is_const_variable_declaration(value_decl)
                        && let Some(literal_type) =
                            self.literal_type_from_initializer(var_decl.initializer)
                    {
                        return (literal_type, Vec::new());
                    }
                    // Fall back to inferring from initializer
                    if !var_decl.initializer.is_none() {
                        return (self.get_type_of_node(var_decl.initializer), Vec::new());
                    }
                }
                // Check if this is a function parameter
                else if let Some(param) = self.ctx.arena.get_parameter(node) {
                    // Get type from annotation
                    if !param.type_annotation.is_none() {
                        return (
                            self.get_type_from_type_node(param.type_annotation),
                            Vec::new(),
                        );
                    }
                    // Check for JSDoc type
                    if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(value_decl) {
                        return (jsdoc_type, Vec::new());
                    }
                    // Fall back to inferring from initializer (default value)
                    if !param.initializer.is_none() {
                        return (self.get_type_of_node(param.initializer), Vec::new());
                    }
                }
            }
            // Variable without type annotation or initializer gets implicit 'any'
            // This prevents cascading TS2571 errors
            return (TypeId::ANY, Vec::new());
        }

        // Alias - resolve the aliased type (import x = ns.member or ES6 imports)
        if flags & symbol_flags::ALIAS != 0 {
            if !value_decl.is_none()
                && let Some(node) = self.ctx.arena.get(value_decl)
            {
                // Handle Import Equals Declaration (import x = ns.member)
                if node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    && let Some(import) = self.ctx.arena.get_import_decl(node)
                {
                    // module_specifier holds the reference (e.g., 'ns.member' or require("..."))
                    // Use resolve_qualified_symbol to get the target symbol directly,
                    // avoiding the value-only check that's inappropriate for import aliases.
                    // Import aliases can legitimately reference value-only namespaces.
                    if let Some(target_sym) = self.resolve_qualified_symbol(import.module_specifier)
                    {
                        return (self.get_type_of_symbol(target_sym), Vec::new());
                    }
                    // Check if this is a require() call - handle by creating module namespace type
                    if let Some(module_specifier) =
                        self.get_require_module_specifier(import.module_specifier)
                    {
                        // Try to resolve the module from module_exports
                        if let Some(exports_table) =
                            self.ctx.binder.module_exports.get(&module_specifier)
                        {
                            // Create an object type with all the module's exports
                            use crate::solver::PropertyInfo;
                            let mut props: Vec<PropertyInfo> = Vec::new();
                            for (name, &sym_id) in exports_table.iter() {
                                let prop_type = self.get_type_of_symbol(sym_id);
                                let name_atom = self.ctx.types.intern_string(name);
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    type_id: prop_type,
                                    write_type: prop_type,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                });
                            }
                            let module_type = self.ctx.types.object(props);
                            return (module_type, Vec::new());
                        }
                        // Module not found - emit TS2307 error and return ANY
                        // TypeScript treats unresolved imports as `any` to avoid cascading errors
                        self.emit_module_not_found_error(&module_specifier, value_decl);
                        return (TypeId::ANY, Vec::new());
                    }
                    // Fall back to get_type_of_node for simple identifiers
                    return (self.get_type_of_node(import.module_specifier), Vec::new());
                }
                // Handle ES6 named imports (import { X } from './module')
                // Use the import_module field to resolve to the actual export
                // Check if this symbol has import tracking metadata
            }

            // For ES6 imports with import_module set, resolve using module_exports
            if let Some(ref module_name) = import_module {
                // Check if this is a shorthand ambient module (declare module "foo" without body)
                // Imports from shorthand ambient modules are typed as `any`
                if self
                    .ctx
                    .binder
                    .shorthand_ambient_modules
                    .contains(module_name)
                {
                    return (TypeId::ANY, Vec::new());
                }

                // Check if this is a namespace import (import * as ns)
                // Namespace imports have import_name set to None and should return all exports as an object
                if import_name.is_none() {
                    // This is a namespace import: import * as ns from 'module'
                    // Create an object type containing all module exports

                    // First, try local binder's module_exports
                    let exports_table = self
                        .ctx
                        .binder
                        .module_exports
                        .get(module_name)
                        .cloned()
                        // Fall back to cross-file resolution if local lookup fails
                        .or_else(|| self.resolve_cross_file_namespace_exports(module_name));

                    if let Some(exports_table) = exports_table {
                        use crate::solver::PropertyInfo;
                        let mut props: Vec<PropertyInfo> = Vec::new();
                        for (name, &export_sym_id) in exports_table.iter() {
                            let mut prop_type = self.get_type_of_symbol(export_sym_id);

                            // Rule #44: Apply module augmentations to each exported type
                            prop_type =
                                self.apply_module_augmentations(module_name, name, prop_type);

                            let name_atom = self.ctx.types.intern_string(name);
                            props.push(PropertyInfo {
                                name: name_atom,
                                type_id: prop_type,
                                write_type: prop_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                            });
                        }
                        let module_type = self.ctx.types.object(props);
                        return (module_type, Vec::new());
                    }
                    // Module not found - emit TS2307 error and return ANY
                    // TypeScript treats unresolved imports as `any` to avoid cascading errors
                    self.emit_module_not_found_error(module_name, value_decl);
                    return (TypeId::ANY, Vec::new());
                }

                // This is a named import: import { X } from 'module'
                // Use import_name if set (for renamed imports), otherwise use escaped_name
                let export_name = import_name.as_ref().unwrap_or(&escaped_name);

                // First, try local binder's module_exports
                let export_sym_id = self
                    .ctx
                    .binder
                    .module_exports
                    .get(module_name)
                    .and_then(|exports_table| exports_table.get(export_name))
                    // Fall back to cross-file resolution if local lookup fails
                    .or_else(|| self.resolve_cross_file_export(module_name, export_name));

                if let Some(export_sym_id) = export_sym_id {
                    let mut result = self.get_type_of_symbol(export_sym_id);

                    // Rule #44: Apply module augmentations to the imported type
                    // If there are augmentations for this module+interface, merge them in
                    result = self.apply_module_augmentations(module_name, export_name, result);

                    if std::env::var("TSZ_DEBUG_IMPORTS").is_ok() {
                        debug!(
                            export_name = %export_name,
                            module_name = %module_name,
                            export_sym_id = export_sym_id.0,
                            result_type_id = result.0,
                            "[DEBUG] ALIAS"
                        );
                    }
                    return (result, Vec::new());
                }
                // Module not found in exports - emit TS2307 error and return ERROR to expose type errors
                // Returning ANY would suppress downstream errors (poisoning)
                // TSC emits TS2307 for missing module and allows property access, but returning ERROR
                // gives better error detection for conformance
                self.emit_module_not_found_error(module_name, value_decl);
                return (TypeId::ERROR, Vec::new());
            }

            // Unresolved alias - return ANY to prevent cascading TS2571 errors
            return (TypeId::ANY, Vec::new());
        }

        // Fallback: return ANY for unresolved symbols to prevent cascading errors
        // The actual "cannot find" error should already be emitted elsewhere
        (TypeId::ANY, Vec::new())
    }

    pub(crate) fn contextual_literal_type(&mut self, literal_type: TypeId) -> Option<TypeId> {
        let ctx_type = self.ctx.contextual_type?;
        if self.contextual_type_allows_literal(ctx_type, literal_type) {
            Some(literal_type)
        } else {
            None
        }
    }

    pub(crate) fn contextual_type_allows_literal(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.contextual_type_allows_literal_inner(ctx_type, literal_type, &mut visited)
    }

    pub(crate) fn contextual_type_allows_literal_inner(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        use crate::solver::type_queries::{
            ContextualLiteralAllowKind, classify_for_contextual_literal,
        };

        if ctx_type == literal_type {
            return true;
        }
        if !visited.insert(ctx_type) {
            return false;
        }

        match classify_for_contextual_literal(self.ctx.types, ctx_type) {
            ContextualLiteralAllowKind::Members(members) => members.iter().any(|&member| {
                self.contextual_type_allows_literal_inner(member, literal_type, visited)
            }),
            ContextualLiteralAllowKind::TypeParameter { constraint } => constraint
                .map(|constraint| {
                    self.contextual_type_allows_literal_inner(constraint, literal_type, visited)
                })
                .unwrap_or(false),
            ContextualLiteralAllowKind::Ref(symbol) => {
                let resolved = {
                    let env = self.ctx.type_env.borrow();
                    env.get(symbol)
                };
                if let Some(resolved) = resolved
                    && resolved != ctx_type
                {
                    return self.contextual_type_allows_literal_inner(
                        resolved,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::Application => {
                let expanded = self.evaluate_application_type(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::Mapped => {
                let expanded = self.evaluate_mapped_type_with_resolution(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::NotAllowed => false,
        }
    }

    /// Resolve a typeof type reference to its structural type.
    ///
    /// This function resolves `typeof X` type queries to the actual type of `X`.
    /// This is useful for type operations where we need the structural type rather
    /// than the type query itself.
    ///
    /// **TypeQuery Resolution:**
    /// - **TypeQuery**: `typeof X` → get the type of symbol X
    /// - **Other types**: Return unchanged (not a typeof query)
    ///
    /// **Use Cases:**
    /// - Assignability checking (need actual type, not typeof reference)
    /// - Type comparison (typeof X should be compared to X's type)
    /// - Generic constraint evaluation
    ///

    // NOTE: refine_mixin_call_return_type, mixin_base_param_index, instance_type_from_constructor_type,
    // instance_type_from_constructor_type_inner, merge_base_instance_into_constructor_return,
    // merge_base_constructor_properties_into_constructor_return moved to constructor_checker.rs

    pub(crate) fn get_type_of_private_property_access(
        &mut self,
        idx: NodeIndex,
        access: &crate::parser::node::AccessExprData,
        name_idx: NodeIndex,
        object_type: TypeId,
    ) -> TypeId {
        use crate::solver::PropertyAccessResult;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);

        let object_type = self.evaluate_application_type(object_type);
        let (object_type_for_check, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_check) = object_type_for_check else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                self.report_possibly_nullish_object(access.expression, cause);
            }
            return TypeId::ERROR;
        };

        // When symbols are empty but we're inside a class scope, check if the object type
        // itself has private properties matching the name. This handles cases like:
        //   let a: A2 = this;
        //   a.#prop;  // Should work if A2 has #prop
        if symbols.is_empty() {
            // Resolve type references (Ref, TypeQuery, etc.) before property access lookup
            let resolved_type = self.resolve_type_for_property_access(object_type_for_check);

            // Try to find the property directly in the resolved object type
            use crate::solver::PropertyAccessResult;
            match self
                .ctx
                .types
                .property_access_type(resolved_type, &property_name)
            {
                PropertyAccessResult::Success { .. } => {
                    // Property exists in the type, proceed with the access
                    return self.get_type_of_property_access_by_name(
                        idx,
                        access,
                        resolved_type,
                        &property_name,
                    );
                }
                _ => {
                    // FALLBACK: Manually check if the property exists in the callable type
                    // This fixes cases where property_access_type fails due to atom comparison issues
                    // The property IS in the type (as shown by error messages), but the lookup fails
                    if let Some(shape) = crate::solver::type_queries::get_callable_shape(
                        self.ctx.types,
                        resolved_type,
                    ) {
                        let prop_atom = self.ctx.types.intern_string(&property_name);
                        for prop in &shape.properties {
                            if prop.name == prop_atom {
                                // Property found in the callable's properties list!
                                // Return the property type (handle optional and write_type)
                                let prop_type = if prop.optional {
                                    self.ctx.types.union(vec![prop.type_id, TypeId::UNDEFINED])
                                } else {
                                    prop.type_id
                                };
                                return self.apply_flow_narrowing(idx, prop_type);
                            }
                        }
                    }

                    // Property not found, emit error if appropriate
                    if saw_class_scope {
                        self.error_property_not_exist_at(&property_name, object_type, name_idx);
                    }
                    return TypeId::ERROR;
                }
            }
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    self.error_property_not_exist_at(
                        &property_name,
                        object_type_for_check,
                        name_idx,
                    );
                }
                return TypeId::ERROR;
            }
        };

        if object_type_for_check == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type_for_check == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }
        if object_type_for_check == TypeId::UNKNOWN {
            return TypeId::ANY; // UNKNOWN remains ANY for now (could be stricter)
        }

        // For private member access, use nominal typing based on private brand.
        // If both types have the same private brand, they're from the same class
        // declaration and the access should be allowed.
        let types_compatible =
            if self.types_have_same_private_brand(object_type_for_check, declaring_type) {
                true
            } else {
                self.is_assignable_to(object_type_for_check, declaring_type)
            };

        if !types_compatible {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .map(|ty| {
                        if self.types_have_same_private_brand(object_type_for_check, ty) {
                            true
                        } else {
                            self.is_assignable_to(object_type_for_check, ty)
                        }
                    })
                    .unwrap_or(false)
            });
            if shadowed {
                return TypeId::ANY;
            }

            self.error_property_not_exist_at(&property_name, object_type_for_check, name_idx);
            return TypeId::ERROR;
        }

        let declaring_type = self.resolve_type_for_property_access(declaring_type);
        let mut result_type = match self
            .ctx
            .types
            .property_access_type(declaring_type, &property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
            } => {
                if from_index_signature {
                    // Private fields can't come from index signatures
                    self.error_property_not_exist_at(
                        &property_name,
                        object_type_for_check,
                        name_idx,
                    );
                    return TypeId::ERROR;
                }
                type_id
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // If we got here, we already resolved the symbol, so the private field exists.
                // The solver might not find it due to type encoding issues.
                // FALLBACK: Try to manually find the property in the callable type
                if let Some(shape) =
                    crate::solver::type_queries::get_callable_shape(self.ctx.types, declaring_type)
                {
                    let prop_atom = self.ctx.types.intern_string(&property_name);
                    for prop in &shape.properties {
                        if prop.name == prop_atom {
                            // Property found! Return its type
                            return if prop.optional {
                                self.ctx.types.union(vec![prop.type_id, TypeId::UNDEFINED])
                            } else {
                                prop.type_id
                            };
                        }
                    }
                }
                // Property not found even in fallback, return ANY for type recovery
                TypeId::ANY
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS2339: Property does not exist on type 'unknown'
                // Use the same error as TypeScript for property access on unknown
                self.error_property_not_exist_at(&property_name, object_type_for_check, name_idx);
                TypeId::ERROR
            }
        };

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = self.ctx.types.union(vec![result_type, TypeId::UNDEFINED]);
            } else {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }
}
