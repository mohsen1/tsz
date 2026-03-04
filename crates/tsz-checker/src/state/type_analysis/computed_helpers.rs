//! Contextual literal types, circular reference detection, and private property access.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::type_queries::{ContextualLiteralAllowKind, classify_for_contextual_literal};

impl<'a> CheckerState<'a> {
    pub(crate) fn contextual_literal_type(&mut self, literal_type: TypeId) -> Option<TypeId> {
        let ctx_type = self.ctx.contextual_type?;
        self.contextual_type_allows_literal(ctx_type, literal_type)
            .then_some(literal_type)
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
        if ctx_type == literal_type {
            return true;
        }
        // tsc rule: "If the contextual type is a literal type, we consider this
        // a literal context for ALL literals of the same base type."
        // e.g., contextual type "A" allows literal "f" because both are string literals.
        if tsz_solver::type_queries::are_same_base_literal_kind(
            self.ctx.types,
            ctx_type,
            literal_type,
        ) {
            return true;
        }
        if !visited.insert(ctx_type) {
            return false;
        }

        // Resolve Lazy(DefId) types before classification. Type aliases like
        // `type Direction = "north" | "south"` are Lazy until resolved.
        if let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, ctx_type) {
            // Try type_env first
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            // If not resolved, use centralized relation precondition setup to populate type_env.
            self.ensure_relation_input_ready(ctx_type);
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            return false;
        }

        // Evaluate KeyOf and IndexAccess types to their concrete form before
        // classification. E.g., keyof Person → "name" | "age".
        if tsz_solver::type_queries::is_keyof_type(self.ctx.types, ctx_type)
            || tsz_solver::type_queries::is_index_access_type(self.ctx.types, ctx_type)
            || tsz_solver::type_queries::is_conditional_type(self.ctx.types, ctx_type)
        {
            let evaluated = self.evaluate_type_with_env(ctx_type);
            if evaluated != ctx_type && evaluated != TypeId::ERROR {
                return self.contextual_type_allows_literal_inner(evaluated, literal_type, visited);
            }
        }

        match classify_for_contextual_literal(self.ctx.types, ctx_type) {
            ContextualLiteralAllowKind::Members(members) => members.iter().any(|&member| {
                self.contextual_type_allows_literal_inner(member, literal_type, visited)
            }),
            // Type parameters always allow literal types. In TypeScript, when the
            // expected type is a type parameter (e.g., K extends keyof T), the literal
            // is preserved and the constraint is checked later during generic inference.
            ContextualLiteralAllowKind::TypeParameter { .. }
            | ContextualLiteralAllowKind::TemplateLiteral => true,
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

    /// Check if a type node is a simple type reference without structural wrapping.
    ///
    /// Returns true for bare type references like `type A = B`, false for wrapped
    /// references like `type A = { x: B }` or `type A = B | null`.
    pub(crate) fn is_simple_type_reference(&self, type_node: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };

        // Type reference or identifier without structural wrapping
        matches!(
            node.kind,
            k if k == syntax_kind_ext::TYPE_REFERENCE || k == SyntaxKind::Identifier as u16
        )
    }

    /// Check if a type alias directly circularly references itself (or
    /// transitively through other type aliases currently being resolved).
    ///
    /// Returns true when a type alias resolves to itself or to another alias
    /// in the current resolution chain, without structural wrapping.
    /// Invalid examples: `type A = B; type B = A;`, `type T = T | string`
    ///
    /// Returns false for valid recursive types that use structural wrapping:
    /// `type List = { value: number; next: List | null };`
    ///
    /// `in_union_or_intersection` is set when we are recursing into union/intersection
    /// members. Per the TS spec, "a union type directly depends on each of the
    /// constituent types", so union/intersection members don't need the
    /// `is_simple_type_reference` check on the parent node.
    ///
    /// When a cycle is detected, all type alias symbols on the resolution stack
    /// between the target and the current alias are marked in `circular_type_aliases`
    /// so that each member of the cycle can independently emit TS2456.
    #[allow(clippy::only_used_in_recursion)]
    pub(crate) fn is_direct_circular_reference(
        &mut self,
        sym_id: SymbolId,
        resolved_type: TypeId,
        type_node: NodeIndex,
        in_union_or_intersection: bool,
    ) -> bool {
        // Check if resolved_type is Lazy(DefId) pointing to a type alias in the
        // current resolution chain.
        if let Some(def_id) =
            tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved_type)
            && let Some(&target_sym_id) = self.ctx.def_to_symbol.borrow().get(&def_id)
        {
            // Check if the target is in the resolution set (currently being computed).
            // This detects both direct self-references (A = A) and transitive cycles
            // (A = B; B = C; C = A) — in all cases, the target would be in the set.
            let is_in_resolution_chain = self.ctx.symbol_resolution_set.contains(&target_sym_id);

            // Only flag type alias symbols to avoid false positives for
            // interfaces/classes which can have valid structural recursion.
            let is_type_alias = self
                .ctx
                .binder
                .get_symbol(target_sym_id)
                .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0);

            if is_in_resolution_chain && is_type_alias {
                let is_direct =
                    in_union_or_intersection || self.is_simple_type_reference(type_node);

                if is_direct {
                    // Mark all type alias symbols on the resolution stack between
                    // the cycle target and the current position as circular.
                    // This ensures every member of the cycle emits TS2456.
                    let mut found_target = false;
                    for &stack_sym in &self.ctx.symbol_resolution_stack {
                        if stack_sym == target_sym_id {
                            found_target = true;
                        }
                        if found_target {
                            let is_alias = self.ctx.binder.get_symbol(stack_sym).is_some_and(|s| {
                                s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                            });
                            if is_alias {
                                self.ctx.circular_type_aliases.insert(stack_sym);
                            }
                        }
                    }
                }

                return is_direct;
            }
        }

        // Also check union/intersection members for circular references.
        // Per TS spec: "A union type directly depends on each of the constituent types."
        if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, resolved_type)
        {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node, true) {
                    return true;
                }
            }
        }
        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, resolved_type)
        {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node, true) {
                    return true;
                }
            }
        }

        false
    }

    pub(crate) fn report_private_identifier_outside_class(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let class_name = self
            .get_class_name_from_type(object_type)
            .unwrap_or_else(|| "the class".to_string());
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
            &[property_name, &class_name],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
        );
    }

    pub(crate) fn report_private_identifier_shadowed(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let type_string = self
            .get_class_name_from_type(object_type)
            .unwrap_or_else(|| "the type".to_string());
        let message = format_message(
            diagnostic_messages::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
            &[property_name, &type_string],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
        );
    }

    // Resolve a typeof type reference to its structural type.
    //
    // This function resolves `typeof X` type queries to the actual type of `X`.
    // This is useful for type operations where we need the structural type rather
    // than the type query itself.
    // **TypeQuery Resolution:**
    // - **TypeQuery**: `typeof X` → get the type of symbol X
    // - **Other types**: Return unchanged (not a typeof query)
    //
    // **Use Cases:**
    // - Assignability checking (need actual type, not typeof reference)
    // - Type comparison (typeof X should be compared to X's type)
    // - Generic constraint evaluation
    pub(crate) fn get_type_of_private_property_access(
        &mut self,
        idx: NodeIndex,
        access: &tsz_parser::parser::node::AccessExprData,
        name_idx: NodeIndex,
        object_type: TypeId,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        use tsz_solver::operations::property::PropertyAccessResult;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);

        // Mark the private identifier symbol as referenced for unused-variable tracking.
        // Private identifier accesses (`this.#foo`) go through this path (not
        // `check_property_accessibility`), so reference tracking must happen here.
        // Without this, ES private members accessed via `this.#foo` would be falsely
        // reported as unused (TS6133).
        for &sym_id in &symbols {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }

        // NOTE: Do NOT emit TS18016 here for property access expressions.
        // `obj.#prop` is always valid syntax — the private identifier in a property
        // access position is grammatically correct. TSC only emits TS18016 for truly
        // invalid positions (object literals, standalone expressions). For property
        // access, the error is always semantic (TS18013: can't access private member),
        // which is handled below based on the object's type.

        // Evaluate for type checking but preserve original for error messages
        // This preserves nominal identity (e.g., D<string>) in error messages
        let original_object_type = object_type;
        let object_type = self.evaluate_application_type(object_type);

        // Resolve Lazy class types to their constructor type for private name access.
        // When a class references itself during construction (e.g., `static s = C.#method()`),
        // the type of `C` is a Lazy(DefId) placeholder. The solver's resolve_lazy resolves
        // this to the INSTANCE type, but for value-position class references, we need the
        // CONSTRUCTOR type (which has the static private members).
        let object_type = self.resolve_lazy_class_to_constructor(object_type);

        // Property access on `never` returns `never` (bottom type propagation).
        // TSC does not emit TS18050 for property access on `never` — the result is
        // simply `never`, which allows exhaustive narrowing patterns to work correctly.
        if object_type == TypeId::NEVER {
            return TypeId::NEVER;
        }

        let (object_type_for_check, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_check) = object_type_for_check else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                // Type is entirely nullish - emit TS18050 "The value X cannot be used here"
                self.report_nullish_object(access.expression, cause, true);
            }
            return TypeId::ERROR;
        };

        // If `symbols.is_empty()`, the private identifier was not declared in any enclosing lexical class scope.
        // Therefore, this access is invalid, regardless of whether the object type actually has the property.
        if symbols.is_empty() {
            let resolved_type = self.resolve_type_for_property_access(object_type_for_check);
            let is_any_like = resolved_type == TypeId::ANY
                || resolved_type == TypeId::UNKNOWN
                || resolved_type == TypeId::ERROR;

            if is_any_like {
                // TSC special case: for any-like types, private names can't be looked up
                // dynamically. If we're outside any class body, emit TS18016. If inside a class
                // body (but the private name isn't declared there), emit TS2339.
                if !saw_class_scope {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        "Private identifiers are not allowed outside class bodies.",
                        diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                    );
                } else {
                    // For private identifiers on any-like types inside a class, tsc emits
                    // TS2339 directly (unlike regular properties which are suppressed on `any`).
                    // Private names are nominally scoped, so `any` doesn't satisfy them.
                    let type_str = if resolved_type == TypeId::ANY {
                        "any"
                    } else if resolved_type == TypeId::UNKNOWN {
                        "unknown"
                    } else {
                        "error"
                    };
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        &format!(
                            "Property '{property_name}' does not exist on type '{type_str}'.",
                        ),
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                }
            } else {
                // For concrete types, check if the property actually exists on the type.
                // If found: TS18013 (property exists but not accessible from outside its class).
                // If not found: TS2339 (property does not exist on type).
                let mut found = false;

                use tsz_solver::operations::property::PropertyAccessResult;
                match self
                    .ctx
                    .types
                    .property_access_type(resolved_type, &property_name)
                {
                    PropertyAccessResult::Success { .. } => {
                        found = true;
                    }
                    _ => {
                        if let Some(shape) =
                            crate::query_boundaries::state::type_analysis::callable_shape_for_type(
                                self.ctx.types,
                                resolved_type,
                            )
                        {
                            let prop_atom = self.ctx.types.intern_string(&property_name);
                            for prop in &shape.properties {
                                if prop.name == prop_atom {
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if found {
                    // Property exists, but we are not in the declaring scope (TS18013)
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                    );
                } else {
                    // TS2339: Property does not exist
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                }
            }
            return TypeId::ERROR;
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                } else {
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
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

        // Resolve Lazy class references to their constructor types for STATIC private members.
        //
        // When a class type is referenced during its own type construction (e.g., in a static
        // field initializer `static s = C.#method()`), the identifier resolves to
        // `Lazy(class_def_id)` — a placeholder inserted to break circular resolution. This
        // Lazy type would otherwise resolve to the *instance* type (via
        // `resolve_and_insert_def_type`), causing the compatibility check to fail when the
        // private member is static (whose declaring type is the constructor type).
        //
        // Only apply this resolution for static members; for instance members the Lazy
        // resolves to the instance type which is correct.
        let member_is_static = self.ctx.binder.get_symbol(symbols[0]).map_or(false, |sym| {
            sym.declarations
                .iter()
                .any(|&decl_idx| self.class_member_is_static(decl_idx))
        });
        let object_type_for_check = if member_is_static
            && let Some(def_id) =
                tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, object_type_for_check)
            && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & tsz_binder::symbol_flags::CLASS != 0
        {
            // Resolve to the constructor type. Use the class_constructor_type_cache to
            // avoid triggering further recursion if the constructor is already built.
            let class_decl = self.get_class_declaration_from_symbol(sym_id);
            if let Some(class_idx) = class_decl
                && let Some(&ctor_type) = self.ctx.class_constructor_type_cache.get(&class_idx)
            {
                ctor_type
            } else {
                self.get_type_of_symbol(sym_id)
            }
        } else {
            object_type_for_check
        };

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
                    .is_some_and(|ty| {
                        if self.types_have_same_private_brand(object_type_for_check, ty) {
                            true
                        } else {
                            self.is_assignable_to(object_type_for_check, ty)
                        }
                    })
            });
            if shadowed {
                self.report_private_identifier_shadowed(
                    name_idx,
                    &property_name,
                    original_object_type,
                );
                return TypeId::ERROR;
            }

            // Use original_object_type to preserve nominal identity (e.g., D<string>)
            self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
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
                ..
            } => {
                if from_index_signature {
                    // Private fields can't come from index signatures
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
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
                    crate::query_boundaries::state::type_analysis::callable_shape_for_type(
                        self.ctx.types,
                        declaring_type,
                    )
                {
                    let prop_atom = self.ctx.types.intern_string(&property_name);
                    for prop in &shape.properties {
                        if prop.name == prop_atom {
                            // Property found! Return its type
                            return if prop.optional {
                                factory.union(vec![prop.type_id, TypeId::UNDEFINED])
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
                // TS18046: 'x' is of type 'unknown'.
                // Report on the expression, not the property name.
                // Without strictNullChecks, unknown is treated like any.
                if self.error_is_of_type_unknown(name_idx) {
                    TypeId::ERROR
                } else {
                    TypeId::ANY
                }
            }
        };

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = factory.union(vec![result_type, TypeId::UNDEFINED]);
            } else {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }

    /// Check if a symbol represents a type-only export that should be excluded
    /// from the value namespace of a module.
    ///
    /// Returns `true` when the symbol was exported via `export type { X }` and
    /// should not appear as a value property on namespace objects.
    ///
    /// Handles a subtle binder quirk: `import type { A }` sets `is_type_only`
    /// on the alias symbol, and if the same name is later declared as a value
    /// (`const A = 0`) and re-exported (`export { A }`), the merged symbol
    /// still has `is_type_only = true` from the import. We detect this by
    /// checking if the symbol has BOTH `ALIAS` and `VALUE` flags — the `ALIAS`
    /// came from the import type, and the `VALUE` from the const/function/class.
    pub(crate) fn is_type_only_export_symbol(&self, sym_id: SymbolId) -> bool {
        // Use get_cross_file_symbol to check cross_file_symbol_targets FIRST,
        // avoiding SymbolId collisions: each binder uses its own SymbolId space
        // (starting from 0), so a SymbolId from another file may collide with a
        // different symbol in the local binder. get_symbol_globally checks the
        // local binder first and can return the wrong symbol, causing the
        // is_type_only check to silently fail.
        let symbol = self.get_cross_file_symbol(sym_id);

        let Some(symbol) = symbol else {
            return false;
        };

        if !symbol.is_type_only {
            return false;
        }

        // If the symbol has ALIAS + VALUE flags, is_type_only came from an
        // `import type` alias that merged with a value declaration. The value
        // export is not type-only.
        use tsz_binder::symbol_flags;
        if symbol.flags & symbol_flags::ALIAS != 0 && symbol.flags & symbol_flags::VALUE != 0 {
            return false;
        }

        true
    }

    /// Check if a named export from a module was reached through a `export type *` wildcard
    /// re-export chain. Returns `true` when the export should be excluded from the namespace
    /// object's value properties because any wildcard in the re-export chain was type-only.
    ///
    /// For example:
    /// ```typescript
    /// // ghost.ts
    /// export class Ghost {}          // Ghost.is_type_only = false
    /// // intermediate.ts
    /// export type * from './ghost'   // wildcard_reexports_type_only = true
    /// ```
    /// When building the namespace type for `intermediate`, `Ghost` should NOT appear
    /// as a value property because the wildcard re-export is type-only.
    pub(crate) fn is_export_from_type_only_wildcard(
        &self,
        module_name: &str,
        export_name: &str,
    ) -> bool {
        // Resolve the target file for this module specifier
        let Some(target_file_idx) = self.ctx.resolve_import_target(module_name) else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };
        // Get the canonical file name used as key in the target binder's data structures
        let target_file_name = self
            .ctx
            .get_arena_for_file(target_file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str());
        let Some(file_name) = target_file_name else {
            return false;
        };

        // Use the binder's re-export resolution which tracks type-only status
        // through wildcard chains
        matches!(
            target_binder.resolve_import_with_reexports_type_only(file_name, export_name),
            Some((_, true))
        )
    }
}
