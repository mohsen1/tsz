//! Type Environment Module
//!
//! Extracted from state.rs: Methods for building type environments, evaluating
//! application types, resolving property access types, and type node resolution.

use crate::state::{CheckerState, EnumKind, MAX_INSTANTIATION_DEPTH};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::types::Visibility;
use tsz_solver::visitor::lazy_def_id;
use tsz_solver::{TypeId, TypeKey};

impl<'a> CheckerState<'a> {
    /// Get type of object literal.
    // =========================================================================
    // Type Relations (uses solver::CompatChecker for assignability)
    // =========================================================================

    // Note: enum_symbol_from_type and enum_symbol_from_value_type are defined in type_checking.rs

    pub(crate) fn enum_object_type(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        use rustc_hash::FxHashMap;
        use tsz_solver::{IndexSignature, ObjectFlags, ObjectShape, PropertyInfo};

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let _member_type = match self.enum_kind(sym_id) {
            Some(EnumKind::String) => TypeId::STRING,
            Some(EnumKind::Numeric) => TypeId::NUMBER,
            Some(EnumKind::Mixed) => {
                // Mixed enums have both string and numeric members
                // Fall back to NUMBER for type compatibility
                TypeId::NUMBER
            }
            None => {
                // Return UNKNOWN instead of ANY for enum without explicit kind
                TypeId::UNKNOWN
            }
        };

        let mut props: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(member.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                // Fix: Create TypeKey::Enum(member_def_id, literal_type) for each member
                // This preserves nominal identity so E.A is not assignable to E.B
                let member_sym_id = self
                    .ctx
                    .binder
                    .get_node_symbol(member_idx)
                    .or_else(|| self.ctx.binder.get_node_symbol(member.name))
                    .expect("Enum member must have a symbol");
                let member_def_id = self
                    .ctx
                    .symbol_to_def
                    .borrow()
                    .get(&member_sym_id)
                    .copied()
                    .expect("Enum member must have a DefId");
                let literal_type = self.enum_member_type_from_decl(member_idx);
                let specific_member_type = self
                    .ctx
                    .types
                    .intern(TypeKey::Enum(member_def_id, literal_type));

                props.entry(name_atom).or_insert(PropertyInfo {
                    name: name_atom,
                    type_id: specific_member_type,
                    write_type: specific_member_type,
                    optional: false,
                    readonly: true,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                });
            }
        }

        let properties: Vec<PropertyInfo> = props.into_values().collect();
        if self.enum_kind(sym_id) == Some(EnumKind::Numeric) {
            let number_index = Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: true,
            });
            return Some(self.ctx.types.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
                properties,
                string_index: None,
                number_index,
                symbol: None,
            }));
        }

        Some(self.ctx.types.object(properties))
    }

    // Note: enum_kind and enum_member_type_from_decl are defined in type_checking.rs
    //
    // Note: Enumeration assignability logic has been migrated to the Solver
    // (src/solver/compat.rs). The Checker's AssignabilityOverrideProvider implementation
    // now returns None for enum cases, delegating all enumeration logic to the Solver.
    // See commit 5b8c56551 and Phase 5 (Anti-Pattern 8.1 Removal) in session file.

    // NOTE: abstract_constructor_assignability_override, constructor_access_level,
    // constructor_access_level_for_type, constructor_accessibility_mismatch,
    // constructor_accessibility_override, constructor_accessibility_mismatch_for_assignment,
    // constructor_accessibility_mismatch_for_var_decl, resolve_type_env_symbol,
    // is_abstract_constructor_type moved to constructor_checker.rs

    /// Evaluate complex type constructs for assignability checking.
    ///
    /// This function pre-processes types before assignability checking to ensure
    /// that complex type constructs are properly resolved. This is necessary because
    /// some types need to be expanded or evaluated before compatibility can be determined.
    ///
    /// ## Type Constructs Evaluated:
    /// - **Application** (`Map<string, number>`): Generic type instantiation
    /// - **IndexAccess** (`Type["key"]`): Indexed access types
    /// - **KeyOf** (`keyof Type`): Keyof operator types
    /// - **Mapped** (`{ [K in Keys]: V }`): Mapped types
    /// - **Conditional** (`T extends U ? X : Y`): Conditional types
    ///
    /// ## Evaluation Strategy:
    /// - **Application types**: Full symbol resolution with type environment
    /// - **Index/KeyOf/Mapped/Conditional**: Type environment evaluation
    /// - **Other types**: No evaluation needed (already in simplest form)
    ///
    /// ## Why Evaluation is Needed:
    /// - Generic types may be unevaluated applications (e.g., `Promise<T>`)
    /// - Indexed access types need to compute the result type
    /// - Mapped types need to expand the mapping
    /// - Conditional types need to check the condition and select branch
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Application types
    /// type App = Map<string, number>;
    /// let x: App;
    /// let y: Map<string, number>;
    /// // evaluate_type_for_assignability expands App for comparison
    ///
    /// // Indexed access types
    /// type User = { name: string; age: number };
    /// type UserName = User["name"];  // string
    /// // Evaluation needed to compute that UserName = string
    ///
    /// // Keyof types
    /// type Keys = keyof { a: string; b: number };  // "a" | "b"
    /// // Evaluation needed to compute the union of keys
    ///
    /// // Mapped types
    /// type Readonly<T> = { readonly [P in keyof T]: T[P] };
    /// type RO = Readonly<{ a: string }>;
    /// // Evaluation needed to expand the mapping
    ///
    /// // Conditional types
    /// type NonNull<T> = T extends null ? never : T;
    /// Evaluate an Application type by resolving the base symbol and instantiating.
    ///
    /// This handles types like `Store<ExtractState<R>>` by:
    /// 1. Resolving the base type reference to get its body
    /// 2. Getting the type parameters
    /// 3. Instantiating the body with the provided type arguments
    /// 4. Recursively evaluating the result
    pub(crate) fn evaluate_application_type(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::type_queries;

        if !type_queries::is_generic_type(self.ctx.types, type_id) {
            return type_id;
        }

        // Check cache first (don't clear - the cache key is the TypeId which is stable)
        if let Some(&cached) = self.ctx.application_eval_cache.get(&type_id) {
            return cached;
        }

        // Safety: Don't cache types containing inference variables since their
        // evaluation can change as inference progresses
        let is_cacheable = !self.contains_infer_types_cached(type_id);

        if !self.ctx.application_eval_set.insert(type_id) {
            // Recursion guard for self-referential mapped types.
            return type_id;
        }

        if *self.ctx.instantiation_depth.borrow() >= MAX_INSTANTIATION_DEPTH {
            self.ctx.application_eval_set.remove(&type_id);
            return type_id;
        }
        *self.ctx.instantiation_depth.borrow_mut() += 1;

        let result = self.evaluate_application_type_inner(type_id);

        *self.ctx.instantiation_depth.borrow_mut() -= 1;
        self.ctx.application_eval_set.remove(&type_id);

        // Only cache if the type doesn't contain inference variables
        if is_cacheable {
            self.ctx.application_eval_cache.insert(type_id, result);
        }
        result
    }

    pub(crate) fn evaluate_application_type_inner(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::type_queries::get_application_info;
        use tsz_solver::{TypeSubstitution, instantiate_type};

        let Some((base, args)) = get_application_info(self.ctx.types, type_id) else {
            return type_id;
        };

        // Check if the base is a Lazy or Enum type
        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base) else {
            return type_id;
        };

        // CRITICAL FIX: Get BOTH the body type AND the type parameters together
        // to ensure the TypeIds in the body match the TypeIds in the substitution.
        // Previously we called type_reference_symbol_type and get_type_params_for_symbol
        // separately, which created DIFFERENT TypeIds for the same type parameters.
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        if body_type == TypeId::ANY || body_type == TypeId::ERROR {
            return type_id;
        }

        if type_params.is_empty() {
            return body_type;
        }

        // Resolve type arguments so distributive conditionals can see unions.
        let evaluated_args: Vec<TypeId> = args
            .iter()
            .map(|&arg| self.evaluate_type_with_env(arg))
            .collect();

        // Create substitution and instantiate
        let substitution =
            TypeSubstitution::from_args(self.ctx.types, &type_params, &evaluated_args);
        let instantiated = instantiate_type(self.ctx.types, body_type, &substitution);

        // Recursively evaluate in case the result contains more applications
        let result = self.evaluate_application_type(instantiated);

        // If the result is a Mapped type, try to evaluate it with symbol resolution
        let result = self.evaluate_mapped_type_with_resolution(result);

        // Evaluate meta-types (conditional, index access, keyof) with symbol resolution
        self.evaluate_type_with_env(result)
    }

    /// Evaluate a mapped type with symbol resolution.
    /// This handles cases like `{ [K in keyof Ref(sym)]: Template }` where the Ref
    /// needs to be resolved to get concrete keys.
    pub(crate) fn evaluate_mapped_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        // NOTE: Manual lookup preferred here - we need the mapped_id directly
        // to call mapped_type(mapped_id) below. Using get_mapped_type would
        // return the full Arc<MappedType>, which is more than needed.
        let Some(mapped_id) = tsz_solver::type_queries::get_mapped_type_id(self.ctx.types, type_id)
        else {
            return type_id;
        };

        if let Some(&cached) = self.ctx.mapped_eval_cache.get(&type_id) {
            return cached;
        }

        if !self.ctx.mapped_eval_set.insert(type_id) {
            return type_id;
        }

        if *self.ctx.instantiation_depth.borrow() >= MAX_INSTANTIATION_DEPTH {
            self.ctx.mapped_eval_set.remove(&type_id);
            return type_id;
        }
        *self.ctx.instantiation_depth.borrow_mut() += 1;

        let result = self.evaluate_mapped_type_with_resolution_inner(type_id, mapped_id);

        *self.ctx.instantiation_depth.borrow_mut() -= 1;
        self.ctx.mapped_eval_set.remove(&type_id);
        self.ctx.mapped_eval_cache.insert(type_id, result);
        result
    }

    pub(crate) fn evaluate_mapped_type_with_resolution_inner(
        &mut self,
        type_id: TypeId,
        mapped_id: tsz_solver::MappedTypeId,
    ) -> TypeId {
        use tsz_solver::{LiteralValue, PropertyInfo, TypeKey, TypeSubstitution, instantiate_type};

        let mapped = self.ctx.types.mapped_type(mapped_id);

        // Evaluate the constraint to get concrete keys
        let keys = self.evaluate_mapped_constraint_with_resolution(mapped.constraint);

        // Extract string literal keys
        let string_keys = self.extract_string_literal_keys(keys);
        if string_keys.is_empty() {
            // Can't evaluate - return original
            return type_id;
        }

        // Build the resulting object properties
        let mut properties = Vec::new();
        for key_name in string_keys {
            // Create the key literal type
            let key_literal = self
                .ctx
                .types
                .intern(TypeKey::Literal(LiteralValue::String(key_name)));

            // Substitute the type parameter with the key
            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);

            // Instantiate the template without recursively expanding nested applications.
            let property_type = instantiate_type(self.ctx.types, mapped.template, &subst);

            // CRITICAL: Evaluate the property type to resolve index access types.
            // For mapped types like { [K in keyof T]?: T[K] }, after instantiation
            // we get T["host"] which is an IndexAccess type that needs to be evaluated
            // to get the actual property type (e.g., "string" for T["host"]).
            //
            // We handle this specially by directly resolving Lazy(DefId) index access
            // types, because the TypeEvaluator might not have access to the type
            // environment's def_types map during evaluation.
            let property_type = if let Some(TypeKey::IndexAccess(obj, _idx)) =
                self.ctx.types.lookup(property_type)
            {
                // For IndexAccess types, we need to resolve the object type and get the property
                // First, check if obj is a Lazy type that needs resolution
                let obj_type = if let Some(TypeKey::Lazy(def_id)) = self.ctx.types.lookup(obj) {
                    // Resolve the Lazy type to get the actual object type
                    if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                        self.get_type_of_symbol(sym_id)
                    } else {
                        obj
                    }
                } else {
                    obj
                };

                // Now get the property type from the object
                if let Some(TypeKey::Object(shape_id)) = self.ctx.types.lookup(obj_type) {
                    let shape = self.ctx.types.object_shape(shape_id);
                    // Look for the property by name (key_name is already an Atom)
                    if let Some(prop) = shape.properties.iter().find(|p| p.name == key_name) {
                        prop.type_id
                    } else {
                        // Property not found, fall back to evaluate_type_with_env
                        self.evaluate_type_with_env(property_type)
                    }
                } else {
                    // Not an object type, fall back to evaluate_type_with_env
                    self.evaluate_type_with_env(property_type)
                }
            } else {
                // Not an IndexAccess, evaluate normally
                self.evaluate_type_with_env(property_type)
            };

            let optional = matches!(
                mapped.optional_modifier,
                Some(tsz_solver::MappedModifier::Add)
            );
            let readonly = matches!(
                mapped.readonly_modifier,
                Some(tsz_solver::MappedModifier::Add)
            );

            properties.push(PropertyInfo {
                name: key_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            });
        }

        self.ctx.types.object(properties)
    }

    /// Evaluate a mapped type constraint with symbol resolution.
    /// Handles keyof Ref(sym) by resolving the Ref and getting its keys.
    pub(crate) fn evaluate_mapped_constraint_with_resolution(
        &mut self,
        constraint: TypeId,
    ) -> TypeId {
        use tsz_solver::type_queries::{MappedConstraintKind, classify_mapped_constraint};

        match classify_mapped_constraint(self.ctx.types, constraint) {
            MappedConstraintKind::KeyOf(operand) => {
                // Evaluate the operand with symbol resolution
                let evaluated = self.evaluate_type_with_resolution(operand);
                self.get_keyof_type(evaluated)
            }
            MappedConstraintKind::Resolved => constraint,
            MappedConstraintKind::Other => constraint,
        }
    }

    /// Evaluate a type with symbol resolution (Lazy types resolved to their concrete types).
    pub(crate) fn evaluate_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::type_queries::{TypeResolutionKind, classify_for_type_resolution};

        match classify_for_type_resolution(self.ctx.types, type_id) {
            TypeResolutionKind::Lazy(def_id) => {
                // Resolve Lazy(DefId) types by looking up the symbol and getting its concrete type
                // Use get_type_of_symbol instead of type_reference_symbol_type because:
                // - type_reference_symbol_type returns Lazy types (for error message formatting)
                // - get_type_of_symbol returns the actual cached concrete type
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    let resolved = self.get_type_of_symbol(sym_id);
                    // FIX: Detect identity loop by comparing DefId, not TypeId.
                    // When get_type_of_symbol hits a circular reference, it returns a Lazy placeholder
                    // for the same symbol. Even though the TypeId might be different (due to fresh interning),
                    // the DefId should be the same. This detects the cycle and breaks infinite recursion.
                    // This happens in cases like: class C { static { C.#x; } static #x = 123; }
                    let resolved_def_id = self.ctx.types.lookup(resolved).and_then(|k| match k {
                        TypeKey::Lazy(d) => Some(d),
                        _ => None,
                    });
                    if resolved_def_id == Some(def_id) {
                        return type_id;
                    }
                    // Recursively resolve if still Lazy (handles Lazy chains)
                    if let Some(TypeKey::Lazy(_)) = self.ctx.types.lookup(resolved) {
                        self.evaluate_type_with_resolution(resolved)
                    } else {
                        // Further evaluate compound types (IndexAccess, KeyOf, Mapped, etc.)
                        // that need reduction. E.g., type NameType = Person["name"] resolves
                        // to IndexAccess(Person, "name") which must be evaluated to "string".
                        self.evaluate_type_for_assignability(resolved)
                    }
                } else {
                    type_id
                }
            }
            TypeResolutionKind::Application => self.evaluate_application_type(type_id),
            TypeResolutionKind::Resolved => type_id,
        }
    }

    pub(crate) fn evaluate_type_with_env(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::TypeEvaluator;
        use tsz_solver::type_queries::get_index_access_types;

        self.ensure_application_symbols_resolved(type_id);

        // Use type_env (not type_environment) because type_env is updated during
        // type checking with user-defined DefId→TypeId mappings, while
        // type_environment only has the initial lib symbols from build_type_environment().
        let result = {
            let env = self.ctx.type_env.borrow();
            let mut evaluator = TypeEvaluator::with_resolver(self.ctx.types, &*env);
            evaluator.evaluate(type_id)
        };

        // If the result still contains IndexAccess types, try again with the full
        // checker context as resolver (which can resolve type parameters etc.)
        if get_index_access_types(self.ctx.types, result).is_some() {
            let mut evaluator = TypeEvaluator::with_resolver(self.ctx.types, &self.ctx);
            evaluator.evaluate(type_id)
        } else {
            result
        }
    }

    pub(crate) fn resolve_global_interface_type(&mut self, name: &str) -> Option<TypeId> {
        // First try file_locals (includes user-defined globals and merged lib symbols)
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return Some(self.type_reference_symbol_type(sym_id));
        }
        // Then try using get_global_type to check lib binders
        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)
        {
            return Some(self.type_reference_symbol_type(sym_id));
        }
        // Fall back to resolve_lib_type_by_name for lowering types from lib contexts
        self.resolve_lib_type_by_name(name)
    }

    pub(crate) fn apply_function_interface_for_property_access(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let Some(function_type) = self.resolve_global_interface_type("Function") else {
            return type_id;
        };
        if function_type == TypeId::ANY
            || function_type == TypeId::ERROR
            || function_type == TypeId::UNKNOWN
        {
            return type_id;
        }
        self.ctx.types.intersection2(type_id, function_type)
    }

    pub(crate) fn resolve_type_for_property_access(&mut self, type_id: TypeId) -> TypeId {
        use rustc_hash::FxHashSet;

        self.ensure_application_symbols_resolved(type_id);

        let mut visited = FxHashSet::default();
        self.resolve_type_for_property_access_inner(type_id, &mut visited)
    }

    pub(crate) fn resolve_type_for_property_access_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> TypeId {
        use tsz_binder::SymbolId;
        use tsz_solver::type_queries::{
            PropertyAccessResolutionKind, classify_for_property_access_resolution,
        };

        if !visited.insert(type_id) {
            return type_id;
        }

        // Recursion depth check to prevent stack overflow
        if !self.ctx.enter_recursion() {
            return type_id;
        }

        let result = match classify_for_property_access_resolution(self.ctx.types, type_id) {
            PropertyAccessResolutionKind::Lazy(def_id) => {
                // Resolve lazy type from definition store
                if let Some(body) = self.ctx.definition_store.get_body(def_id) {
                    if body == type_id {
                        type_id
                    } else {
                        self.resolve_type_for_property_access_inner(body, visited)
                    }
                } else {
                    // Definition not found in store - try to resolve via symbol lookup
                    // This handles cases where the definition hasn't been registered yet
                    // (e.g., in test setup that doesn't go through full lowering)
                    let sym_id_opt = self.ctx.def_to_symbol.borrow().get(&def_id).copied();
                    if let Some(sym_id) = sym_id_opt {
                        // Enums in value position behave like objects (runtime enum object).
                        // For numeric enums, this includes a number index signature for reverse mapping.
                        // This is the same logic as Ref branch above - check for ENUM flags
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            if symbol.flags & symbol_flags::ENUM != 0 {
                                if let Some(enum_object) = self.enum_object_type(sym_id) {
                                    if enum_object != type_id {
                                        let r = self.resolve_type_for_property_access_inner(
                                            enum_object,
                                            visited,
                                        );
                                        self.ctx.leave_recursion();
                                        return r;
                                    }
                                    self.ctx.leave_recursion();
                                    return enum_object;
                                }
                            }
                        }

                        let resolved = self.get_type_of_symbol(sym_id);
                        if resolved == type_id {
                            type_id
                        } else {
                            self.resolve_type_for_property_access_inner(resolved, visited)
                        }
                    } else {
                        type_id
                    }
                }
            }
            PropertyAccessResolutionKind::TypeQuery(sym_ref) => {
                let resolved = self.get_type_of_symbol(SymbolId(sym_ref.0));
                if resolved == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(resolved, visited)
                }
            }
            PropertyAccessResolutionKind::Application(_) => {
                // Don't expand Application types for property access resolution
                // This preserves nominal identity (e.g., D<string>) in error messages
                // The property access resolver will handle it correctly
                type_id
            }
            PropertyAccessResolutionKind::TypeParameter { constraint } => {
                if let Some(constraint) = constraint {
                    if constraint == type_id {
                        type_id
                    } else {
                        self.resolve_type_for_property_access_inner(constraint, visited)
                    }
                } else {
                    type_id
                }
            }
            PropertyAccessResolutionKind::NeedsEvaluation => {
                let evaluated = self.evaluate_type_with_env(type_id);
                if evaluated == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(evaluated, visited)
                }
            }
            PropertyAccessResolutionKind::Union(members) => {
                let resolved_members: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.resolve_type_for_property_access_inner(member, visited))
                    .collect();
                self.ctx.types.union_preserve_members(resolved_members)
            }
            PropertyAccessResolutionKind::Intersection(members) => {
                let resolved_members: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.resolve_type_for_property_access_inner(member, visited))
                    .collect();
                self.ctx.types.intersection(resolved_members)
            }
            PropertyAccessResolutionKind::Readonly(inner) => {
                self.resolve_type_for_property_access_inner(inner, visited)
            }
            PropertyAccessResolutionKind::FunctionLike => {
                // Apply Function interface to get properties like call/apply/bind.
                // Do NOT recurse into the expanded type: intersection2(T, Function)
                // produces a NEW TypeId each time, so the visited set can't catch
                // the loop — each iteration allocates another intersection, causing OOM.
                self.apply_function_interface_for_property_access(type_id)
            }
            PropertyAccessResolutionKind::Resolved => type_id,
        };

        self.ctx.leave_recursion();
        result
    }

    /// Resolve a lazy type (type alias) to its body type.
    ///
    /// This function resolves `TypeKey::Lazy(DefId)` types by looking up the
    /// definition's body in the definition store. This is necessary for
    /// type aliases like `type Tuple = [string, number]` where the reference
    /// to `Tuple` is stored as a lazy type.
    ///
    /// The function handles recursive type aliases by checking if the body
    /// is itself a lazy type and resolving it recursively.
    pub fn resolve_lazy_type(&mut self, type_id: TypeId) -> TypeId {
        use rustc_hash::FxHashSet;

        let mut visited = FxHashSet::default();
        self.resolve_lazy_type_inner(type_id, &mut visited)
    }

    fn resolve_lazy_type_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> TypeId {
        use tsz_solver::types::TypeKey;

        // Prevent infinite loops in circular type aliases
        if !visited.insert(type_id) {
            return type_id;
        }

        // Check if this is a lazy type
        if let Some(def_id) = lazy_def_id(self.ctx.types, type_id) {
            // Try to look up the definition's body in the definition store first
            if let Some(body) = self.ctx.definition_store.get_body(def_id) {
                // Recursively resolve in case the body is also a lazy type
                return self.resolve_lazy_type_inner(body, visited);
            }

            // If not in the definition store, try to resolve via symbol lookup
            // This handles type aliases that are resolved through compute_type_of_symbol
            let sym_id_opt = self.ctx.def_to_symbol.borrow().get(&def_id).copied();
            if let Some(sym_id) = sym_id_opt {
                let resolved = self.get_type_of_symbol(sym_id);
                // Only recurse if the resolved type is different from the original
                if resolved != type_id {
                    return self.resolve_lazy_type_inner(resolved, visited);
                }
            }
        }

        // Handle unions and intersections - resolve each member
        // Only create a new union/intersection if members actually changed
        if let Some(TypeKey::Union(list_id)) = self.ctx.types.lookup(type_id) {
            let members = self.ctx.types.type_list(list_id);
            let resolved_members: Vec<TypeId> = members
                .iter()
                .map(|&member| self.resolve_lazy_type_inner(member, visited))
                .collect();
            // Only create new union if members changed
            if resolved_members.iter().ne(members.iter()) {
                return self.ctx.types.union(resolved_members);
            }
        }

        if let Some(TypeKey::Intersection(list_id)) = self.ctx.types.lookup(type_id) {
            let members = self.ctx.types.type_list(list_id);
            let resolved_members: Vec<TypeId> = members
                .iter()
                .map(|&member| self.resolve_lazy_type_inner(member, visited))
                .collect();
            // Only create new intersection if members changed
            if resolved_members.iter().ne(members.iter()) {
                return self.ctx.types.intersection(resolved_members);
            }
        }

        type_id
    }

    /// Get keyof a type - extract the keys of an object type.
    /// Ensure all symbols referenced in Application types are resolved in the type_env.
    /// This walks the type structure and calls get_type_of_symbol for any Application base symbols.
    pub(crate) fn ensure_application_symbols_resolved(&mut self, type_id: TypeId) {
        use rustc_hash::FxHashSet;

        if self.ctx.application_symbols_resolved.contains(&type_id) {
            return;
        }
        if !self.ctx.application_symbols_resolution_set.insert(type_id) {
            return;
        }

        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        let fully_resolved = self.ensure_application_symbols_resolved_inner(type_id, &mut visited);
        self.ctx.application_symbols_resolution_set.remove(&type_id);
        if fully_resolved {
            self.ctx.application_symbols_resolved.extend(visited);
        }
    }

    pub(crate) fn insert_type_env_symbol(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        resolved: TypeId,
    ) -> bool {
        use tsz_solver::SymbolRef;

        if resolved == TypeId::ANY || resolved == TypeId::ERROR {
            return true;
        }

        // CRITICAL FIX: Only skip registering Lazy types if they point to THEMSELVES.
        // Skipping all Lazy types breaks alias chains (type A = B).
        let current_def_id = self.ctx.symbol_to_def.borrow().get(&sym_id).copied();
        if let Some(target_def_id) =
            tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved)
        {
            if Some(target_def_id) == current_def_id {
                return true; // Skip self-recursive alias (A -> A)
            }
        }

        let symbol_ref = SymbolRef(sym_id.0);
        let def_id = self.ctx.symbol_to_def.borrow().get(&sym_id).copied();

        // Reuse cached params already in the environment when available.
        let mut cached_env_params: Option<Vec<tsz_solver::TypeParamInfo>> = None;
        let mut symbol_already_registered = false;
        let mut def_already_registered = def_id.is_none();
        if let Ok(env) = self.ctx.type_env.try_borrow() {
            symbol_already_registered = env.contains(symbol_ref);
            cached_env_params = env.get_params(symbol_ref).cloned();
            if let Some(def_id) = def_id {
                def_already_registered = env.contains_def(def_id);
            }
        }
        let had_env_params = cached_env_params.is_some();
        let type_params = if let Some(params) = cached_env_params {
            params
        } else if let Some(def_id) = def_id {
            self.ctx
                .get_def_type_params(def_id)
                .unwrap_or_else(|| self.get_type_params_for_symbol(sym_id))
        } else {
            self.get_type_params_for_symbol(sym_id)
        };

        if let Some(def_id) = def_id
            && !type_params.is_empty()
        {
            self.ctx.insert_def_type_params(def_id, type_params.clone());
        }

        // Already fully registered with params (or not generic), nothing to do.
        if symbol_already_registered
            && def_already_registered
            && (had_env_params || type_params.is_empty())
        {
            return true;
        }

        // Use try_borrow_mut to avoid panic if type_env is already borrowed.
        // This can happen during recursive type resolution.
        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            if type_params.is_empty() {
                env.insert(symbol_ref, resolved);
                if let Some(def_id) = def_id {
                    env.insert_def(def_id, resolved);
                }
            } else {
                env.insert_with_params(symbol_ref, resolved, type_params.clone());
                if let Some(def_id) = def_id {
                    env.insert_def_with_params(def_id, resolved, type_params);
                }
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn ensure_application_symbols_resolved_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        use tsz_solver::type_queries::{
            SymbolResolutionTraversalKind, classify_for_symbol_resolution_traversal,
        };

        if !visited.insert(type_id) {
            return true;
        }

        match classify_for_symbol_resolution_traversal(self.ctx.types, type_id) {
            SymbolResolutionTraversalKind::Application { base, args, .. } => {
                let mut fully_resolved = true;

                // If the base is a Lazy or Enum type, resolve the symbol
                if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base) {
                    let resolved = self.type_reference_symbol_type(sym_id);
                    fully_resolved &= self.insert_type_env_symbol(sym_id, resolved);
                }

                // Recursively process base and args
                fully_resolved &= self.ensure_application_symbols_resolved_inner(base, visited);
                for arg in args {
                    fully_resolved &= self.ensure_application_symbols_resolved_inner(arg, visited);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::Lazy(def_id) => {
                let mut fully_resolved = true;
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    // Use get_type_of_symbol (not type_reference_symbol_type) because
                    // type_reference_symbol_type returns Lazy(DefId) for interfaces/classes,
                    // which insert_type_env_symbol rejects as a self-recursive alias.
                    // We need the concrete structural type for TypeEnvironment resolution.
                    let resolved = self.get_type_of_symbol(sym_id);
                    fully_resolved &= self.insert_type_env_symbol(sym_id, resolved);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::TypeParameter {
                constraint,
                default,
            } => {
                let mut fully_resolved = true;
                if let Some(constraint) = constraint {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(constraint, visited);
                }
                if let Some(default) = default {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(default, visited);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::Members(members) => {
                let mut fully_resolved = true;
                for member in members {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(member, visited);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::Function(shape_id) => {
                let mut fully_resolved = true;
                let shape = self.ctx.types.function_shape(shape_id);
                for type_param in shape.type_params.iter() {
                    if let Some(constraint) = type_param.constraint {
                        fully_resolved &=
                            self.ensure_application_symbols_resolved_inner(constraint, visited);
                    }
                    if let Some(default) = type_param.default {
                        fully_resolved &=
                            self.ensure_application_symbols_resolved_inner(default, visited);
                    }
                }
                for param in shape.params.iter() {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(param.type_id, visited);
                }
                if let Some(this_type) = shape.this_type {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(this_type, visited);
                }
                fully_resolved &=
                    self.ensure_application_symbols_resolved_inner(shape.return_type, visited);
                if let Some(predicate) = &shape.type_predicate
                    && let Some(pred_type_id) = predicate.type_id
                {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(pred_type_id, visited);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::Callable(shape_id) => {
                let mut fully_resolved = true;
                let shape = self.ctx.types.callable_shape(shape_id);
                for sig in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    for type_param in sig.type_params.iter() {
                        if let Some(constraint) = type_param.constraint {
                            fully_resolved &=
                                self.ensure_application_symbols_resolved_inner(constraint, visited);
                        }
                        if let Some(default) = type_param.default {
                            fully_resolved &=
                                self.ensure_application_symbols_resolved_inner(default, visited);
                        }
                    }
                    for param in sig.params.iter() {
                        fully_resolved &=
                            self.ensure_application_symbols_resolved_inner(param.type_id, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        fully_resolved &=
                            self.ensure_application_symbols_resolved_inner(this_type, visited);
                    }
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(sig.return_type, visited);
                    if let Some(predicate) = &sig.type_predicate
                        && let Some(pred_type_id) = predicate.type_id
                    {
                        fully_resolved &=
                            self.ensure_application_symbols_resolved_inner(pred_type_id, visited);
                    }
                }
                for prop in shape.properties.iter() {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(prop.type_id, visited);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::Object(shape_id) => {
                let mut fully_resolved = true;
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(prop.type_id, visited);
                }
                if let Some(ref idx) = shape.string_index {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(idx.value_type, visited);
                }
                if let Some(ref idx) = shape.number_index {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(idx.value_type, visited);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::Array(elem) => {
                self.ensure_application_symbols_resolved_inner(elem, visited)
            }
            SymbolResolutionTraversalKind::Tuple(elems_id) => {
                let mut fully_resolved = true;
                let elems = self.ctx.types.tuple_list(elems_id);
                for elem in elems.iter() {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(elem.type_id, visited);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::Conditional(cond_id) => {
                let mut fully_resolved = true;
                let cond = self.ctx.types.conditional_type(cond_id);
                fully_resolved &=
                    self.ensure_application_symbols_resolved_inner(cond.check_type, visited);
                fully_resolved &=
                    self.ensure_application_symbols_resolved_inner(cond.extends_type, visited);
                fully_resolved &=
                    self.ensure_application_symbols_resolved_inner(cond.true_type, visited);
                fully_resolved &=
                    self.ensure_application_symbols_resolved_inner(cond.false_type, visited);
                fully_resolved
            }
            SymbolResolutionTraversalKind::Mapped(mapped_id) => {
                let mut fully_resolved = true;
                let mapped = self.ctx.types.mapped_type(mapped_id);
                fully_resolved &=
                    self.ensure_application_symbols_resolved_inner(mapped.constraint, visited);
                fully_resolved &=
                    self.ensure_application_symbols_resolved_inner(mapped.template, visited);
                if let Some(name_type) = mapped.name_type {
                    fully_resolved &=
                        self.ensure_application_symbols_resolved_inner(name_type, visited);
                }
                fully_resolved
            }
            SymbolResolutionTraversalKind::Readonly(inner) => {
                self.ensure_application_symbols_resolved_inner(inner, visited)
            }
            SymbolResolutionTraversalKind::IndexAccess { object, index } => {
                let mut fully_resolved = true;
                fully_resolved &= self.ensure_application_symbols_resolved_inner(object, visited);
                fully_resolved &= self.ensure_application_symbols_resolved_inner(index, visited);
                fully_resolved
            }
            SymbolResolutionTraversalKind::KeyOf(inner) => {
                self.ensure_application_symbols_resolved_inner(inner, visited)
            }
            SymbolResolutionTraversalKind::Terminal => true,
        }
    }

    /// Create a TypeEnvironment populated with resolved symbol types.
    ///
    /// This can be passed to `is_assignable_to_with_env` for type checking
    /// that needs to resolve type references.
    pub fn build_type_environment(&mut self) -> tsz_solver::TypeEnvironment {
        use tsz_binder::symbol_flags;

        // Collect all unique symbols from node_symbols map using BTreeSet for
        // deterministic ordering. Non-deterministic HashSet caused type parameter
        // resolution failures: parameter symbols processed before their parent
        // function would fail to resolve type params like T, causing spurious TS2304.
        let mut symbols: Vec<SymbolId> = self
            .ctx
            .binder
            .node_symbols
            .values()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();

        // FIX: Also include lib symbols for proper property resolution
        // This ensures Error, Math, JSON, Promise, Array, etc. can be resolved when accessed.
        // IMPORTANT: Use file_locals which contains merged lib symbols with remapped IDs,
        // NOT lib.binder.symbols which contains original (unremapped) IDs.
        // After merge_lib_contexts_into_binder(), lib symbols are stored in file_locals
        // with new IDs unique to the main binder's arena.
        if !self.ctx.lib_contexts.is_empty() {
            let existing: rustc_hash::FxHashSet<SymbolId> = symbols.iter().copied().collect();

            // Get lib symbols from file_locals (where they were merged with remapped IDs)
            for (_name, &sym_id) in self.ctx.binder.file_locals.iter() {
                if !existing.contains(&sym_id) {
                    symbols.push(sym_id);
                }
            }
        }

        // Sort symbols so type-defining symbols (functions, classes, interfaces, type aliases)
        // are processed BEFORE variable/parameter symbols. This ensures type parameters
        // are properly scoped when parameter types reference them.
        // Priority: 0 = type-defining (processed first), 1 = variables/parameters (processed last)
        symbols.sort_by_key(|&sym_id| {
            let flags = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .map(|s| s.flags)
                .unwrap_or(0);
            let is_type_defining = flags
                & (symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::INTERFACE
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::ENUM
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE)
                != 0;
            if is_type_defining { 0u8 } else { 1u8 }
        });

        // Resolve each symbol and add to the environment.
        // IMPORTANT: Skip variable/parameter symbols to avoid premature type computation.
        // Computing types for local variables inside class method bodies triggers property
        // access type checks (check_property_accessibility) before check_class_declaration
        // has set the enclosing_class context. This causes false positive TS2445 errors
        // (protected member access denied) because the checker doesn't know we're inside
        // a class method. Variable types are computed lazily during statement checking
        // when the proper enclosing_class context is available.
        // The type environment is only needed for Application type expansion (generic
        // type instantiation) which applies to type aliases, interfaces, classes, etc.
        for sym_id in symbols {
            // Skip variable and parameter symbols - their types will be computed
            // lazily during statement checking with proper class context
            let flags = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .map(|s| s.flags)
                .unwrap_or(0);
            if flags
                & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
                != 0
                && flags
                    & (symbol_flags::CLASS
                        | symbol_flags::FUNCTION
                        | symbol_flags::INTERFACE
                        | symbol_flags::TYPE_ALIAS
                        | symbol_flags::ENUM
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE)
                    == 0
            {
                continue;
            }

            // Get the type for this symbol
            // IMPORTANT: get_type_of_symbol internally calls compute_type_of_symbol which
            // returns both the type AND the correct type_params, then inserts them into
            // ctx.type_env. We MUST NOT separately call get_type_params_for_symbol because
            // that creates fresh type parameter IDs that won't match those used in the type body.
            // This was causing generic type instantiation to fail (e.g., Promise<string>.then()).
            let _type_id = self.get_type_of_symbol(sym_id);
        }

        // Return a clone of ctx.type_env which was correctly populated by get_type_of_symbol
        // with matching type parameter IDs
        self.ctx.type_env.borrow().clone()
    }

    /// Get type parameters for a symbol (generic types).
    ///
    /// Extracts type parameter information for generic types (classes, interfaces,
    /// type aliases). Used for populating the type environment and for generic
    /// type instantiation.
    ///
    /// ## Symbol Types Handled:
    /// - **Type Alias**: Extracts type parameters from type alias declaration
    /// - **Interface**: Extracts type parameters from interface declaration
    /// - **Class**: Extracts type parameters from class declaration
    /// - **Other**: Returns empty vector (no type parameters)
    ///
    /// ## Cross-Arena Resolution:
    /// - Handles symbols defined in other arenas (e.g., imported symbols)
    /// - Creates a temporary CheckerState for the other arena
    /// - Delegates type parameter extraction to the temporary checker
    ///
    /// ## Type Parameter Information:
    /// - Returns Vec<TypeParamInfo> with parameter names and constraints
    /// - Includes default type arguments if present
    /// - Used by TypeEnvironment for generic type expansion
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Type alias with type parameters
    /// type Pair<T, U> = [T, U];
    /// // get_type_params_for_symbol(Pair) → [T, U]
    ///
    /// // Interface with type parameters
    /// interface Box<T> {
    ///   value: T;
    /// }
    /// // get_type_params_for_symbol(Box) → [T]
    ///
    /// // Class with type parameters
    /// class Container<T> {
    ///   constructor(public item: T) {}
    /// }
    /// // get_type_params_for_symbol(Container) → [T]
    ///
    /// // Type parameters with constraints
    /// interface SortedMap<K extends Comparable, V> {}
    /// // get_type_params_for_symbol(SortedMap) → [K: Comparable, V]
    /// ```
    pub(crate) fn get_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        // Recursion depth check: prevent stack overflow from circular generic defaults
        // (e.g. type A<T = B> = T; type B<T = A> = T;)
        if !self.ctx.enter_recursion() {
            return Vec::new();
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if let Some(cached) = self.ctx.get_def_type_params(def_id) {
            self.ctx.leave_recursion();
            return cached;
        }
        if self.ctx.def_no_type_params.borrow().contains(&def_id) {
            self.ctx.leave_recursion();
            return Vec::new();
        }

        // Use get_symbol_globally to find symbols in lib files and other files.
        // Extract needed data to avoid holding a borrow during deeper operations.
        let (flags, value_decl, declarations) = match self.get_symbol_globally(sym_id) {
            Some(symbol) => (
                symbol.flags,
                symbol.value_declaration,
                symbol.declarations.clone(),
            ),
            None => {
                self.ctx.leave_recursion();
                return Vec::new();
            }
        };

        // Fast path: only class/interface/type alias symbols can declare type parameters.
        if flags & (symbol_flags::TYPE_ALIAS | symbol_flags::CLASS | symbol_flags::INTERFACE) == 0 {
            self.ctx.def_no_type_params.borrow_mut().insert(def_id);
            self.ctx.leave_recursion();
            return Vec::new();
        }

        if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id)
            && !std::ptr::eq(symbol_arena.as_ref(), self.ctx.arena)
        {
            let mut checker = CheckerState::with_parent_cache(
                symbol_arena.as_ref(),
                self.ctx.binder,
                self.ctx.types,
                self.ctx.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self, // Share parent's cache to fix Cache Isolation Bug
            );
            let result = checker.get_type_params_for_symbol(sym_id);

            // Propagate delegated symbol caches back to the parent context.
            for (&cached_sym, &cached_ty) in &checker.ctx.symbol_types {
                self.ctx.symbol_types.entry(cached_sym).or_insert(cached_ty);
            }
            for (&cached_sym, &cached_ty) in &checker.ctx.symbol_instance_types {
                self.ctx
                    .symbol_instance_types
                    .entry(cached_sym)
                    .or_insert(cached_ty);
            }

            if !result.is_empty() {
                self.ctx.insert_def_type_params(def_id, result.clone());
                self.ctx.def_no_type_params.borrow_mut().remove(&def_id);
            } else {
                self.ctx.def_no_type_params.borrow_mut().insert(def_id);
            }

            self.ctx.leave_recursion();
            return result;
        }

        // Type alias - get type parameters from declaration
        if flags & symbol_flags::TYPE_ALIAS != 0 {
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
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                    self.ctx.def_no_type_params.borrow_mut().remove(&def_id);
                } else {
                    self.ctx.def_no_type_params.borrow_mut().insert(def_id);
                }
                self.ctx.leave_recursion();
                return params;
            }
        }

        // Class - get type parameters from declaration
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
                let (params, updates) = self.push_type_parameters(&class.type_parameters);
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                    self.ctx.def_no_type_params.borrow_mut().remove(&def_id);
                } else {
                    self.ctx.def_no_type_params.borrow_mut().insert(def_id);
                }
                self.ctx.leave_recursion();
                return params;
            }
        }

        // Interface - get type parameters from first declaration
        if flags & symbol_flags::INTERFACE != 0 {
            let decl_idx = if !value_decl.is_none() {
                value_decl
            } else {
                declarations.first().copied().unwrap_or(NodeIndex::NONE)
            };
            if !decl_idx.is_none()
                && let Some(node) = self.ctx.arena.get(decl_idx)
                && let Some(iface) = self.ctx.arena.get_interface(node)
            {
                let (params, updates) = self.push_type_parameters(&iface.type_parameters);
                self.pop_type_parameters(updates);
                if !params.is_empty() {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                    self.ctx.def_no_type_params.borrow_mut().remove(&def_id);
                } else {
                    self.ctx.def_no_type_params.borrow_mut().insert(def_id);
                }
                self.ctx.leave_recursion();
                return params;
            }
        }

        self.ctx.def_no_type_params.borrow_mut().insert(def_id);
        self.ctx.leave_recursion();
        Vec::new()
    }

    /// Count the number of required type parameters for a symbol.
    ///
    /// A type parameter is "required" if it doesn't have a default value.
    /// This is important for validating generic type usage and error messages.
    ///
    /// ## Required vs Optional:
    /// - **Required**: Must be explicitly provided by the caller
    /// - **Optional**: Has a default value, can be omitted
    ///
    /// ## Use Cases:
    /// - Validating that enough type arguments are provided
    /// - Error messages: "Expected X type arguments but got Y"
    /// - Generic function/method overload resolution
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // All required
    /// interface Pair<T, U> {}
    /// // count_required_type_params(Pair) → 2
    /// const x: Pair = {};  // ❌ Error: Expected 2 type arguments
    /// const y: Pair<string, number> = {};  // ✅
    ///
    /// // One optional
    /// interface Box<T = string> {}
    /// // count_required_type_params(Box) → 0 (T has default)
    /// const a: Box = {};  // ✅ T defaults to string
    /// const b: Box<number> = {};  // ✅ Explicit number
    ///
    /// // Mixed required and optional
    /// interface Map<K, V = any> {}
    /// // count_required_type_params(Map) → 1 (K required, V optional)
    /// const m1: Map<string> = {};  // ✅ K=string, V=any
    /// const m2: Map<string, number> = {};  // ✅ Both specified
    /// const m3: Map = {};  // ❌ K is required
    /// ```
    pub(crate) fn count_required_type_params(&mut self, sym_id: SymbolId) -> usize {
        let type_params = self.get_type_params_for_symbol(sym_id);
        type_params.iter().filter(|p| p.default.is_none()).count()
    }

    /// Create a union type from multiple types.
    ///
    /// Automatically normalizes: flattens nested unions, deduplicates, sorts.
    pub fn get_union_type(&self, types: Vec<TypeId>) -> TypeId {
        self.ctx.types.union(types)
    }

    /// Create an intersection type from multiple types.
    ///
    /// Automatically normalizes: flattens nested intersections, deduplicates, sorts.
    pub fn get_intersection_type(&self, types: Vec<TypeId>) -> TypeId {
        self.ctx.types.intersection(types)
    }

    // =========================================================================
    // Type Node Resolution
    // =========================================================================

    /// Get type from a type node.
    ///
    /// Uses compile-time constant TypeIds for intrinsic types (O(1) lookup).
    /// Get the type representation of a type annotation node.
    ///
    /// This is the main entry point for converting type annotation AST nodes into
    /// TypeId representations. Handles all TypeScript type syntax.
    ///
    /// ## Special Node Handling:
    /// - **TypeReference**: Validates existence before lowering (catches missing types)
    /// - **TypeQuery** (`typeof X`): Resolves via binder for proper symbol resolution
    /// - **UnionType**: Handles specially for nested typeof expression resolution
    /// - **TypeLiteral**: Uses checker resolution for type parameter support
    /// - **Other nodes**: Delegated to TypeLowering
    ///
    /// ## Type Parameter Bindings:
    /// - Uses current type parameter bindings from scope
    /// - Allows type parameters to resolve correctly in generic contexts
    ///
    /// ## Symbol Resolvers:
    /// - Provides type/value symbol resolvers to TypeLowering
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
            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                // Validate the type reference exists before lowering
                // Check cache first - but allow re-resolution of ERROR when type params
                // are in scope, since the ERROR may have been cached when type params
                // weren't available yet (non-deterministic symbol processing order).
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
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
                // Handle typeof X - need to resolve symbol properly via binder
                // Check cache first - allow re-resolution of ERROR when type params in scope
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
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
                    if cached == TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                }
                let result = self.get_type_from_union_type(idx);
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
                    if cached == TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                }
                let result = self.get_type_from_type_literal(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
        }

        // For other type nodes, delegate to TypeNodeChecker
        let mut checker = crate::TypeNodeChecker::new(&mut self.ctx);
        checker.check(idx)
    }

    // =========================================================================
    // Source Location Tracking & Solver Diagnostics
    // =========================================================================

    /// Get a source location for a node.
    pub fn get_source_location(&self, idx: NodeIndex) -> Option<tsz_solver::SourceLocation> {
        let node = self.ctx.arena.get(idx)?;
        Some(tsz_solver::SourceLocation::new(
            self.ctx.file_name.as_str(),
            node.pos,
            node.end,
        ))
    }

    /// Report a type not assignable error using solver diagnostics with source tracking.
    ///
    /// This is the basic error that just says "Type X is not assignable to Y".
    /// For detailed errors with elaboration (e.g., "property 'x' is missing"),
    /// use `error_type_not_assignable_with_reason_at` instead.

    /// Report a cannot find name error using solver diagnostics with source tracking.
    /// Enhanced to provide suggestions for similar names, import suggestions, and
    /// library change suggestions for ES2015+ types.

    // Note: can_merge_symbols is in type_checking.rs

    /// Check if a type name is a built-in mapped type utility.
    /// These are standard TypeScript utility types that transform other types.
    /// When used with type arguments, they should not cause "cannot find type" errors.
    pub(crate) fn resolve_global_this_property_type(
        &mut self,
        name: &str,
        error_node: NodeIndex,
    ) -> TypeId {
        if let Some(sym_id) = self.resolve_global_value_symbol(name) {
            if self.alias_resolves_to_type_only(sym_id) {
                self.error_type_only_value_at(name, error_node);
                return TypeId::ERROR;
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & symbol_flags::VALUE) == 0
            {
                self.error_type_only_value_at(name, error_node);
                return TypeId::ERROR;
            }
            return self.get_type_of_symbol(sym_id);
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

        self.error_property_not_exist_at(name, TypeId::ANY, error_node);
        TypeId::ERROR
    }

    /// Format a type as a human-readable string for error messages and diagnostics.
    ///
    /// This is the main entry point for converting TypeId representations into
    /// human-readable type strings. Used throughout the type checker for error
    /// messages, quick info, and IDE features.
    ///
    /// ## Formatting Strategy:
    /// - Delegates to the solver's TypeFormatter
    /// - Provides symbol table for resolving symbol names
    /// - Handles all type constructs (primitives, generics, unions, etc.)
    ///
    /// ## Type Formatting Rules:
    /// - Primitives: Display as intrinsic names (string, number, etc.)
    /// - Literals: Display as literal values ("hello", 42, true)
    /// - Arrays: Display as T[] or Array<T>
    /// - Tuples: Display as [T, U, V]
    /// - Unions: Display as T | U | V (with parentheses when needed)
    /// - Intersections: Display as T & U & V (with parentheses when needed)
    /// - Functions: Display as (args) => return
    /// - Objects: Display as { prop: Type; ... }
    /// - Type Parameters: Display as T, U, V (short names)
    /// - Type References: Display as RefName<Args>
    ///
    /// ## Use Cases:
    /// - Error messages: "Type X is not assignable to Y"
    /// - Quick info (hover): Type information for IDE
    /// - Completion: Type hints in autocomplete
    /// - Diagnostics: All type-related error messages
    ///
    /// ## TypeScript Examples (Formatted Output):
    /// ```typescript
    /// // Primitives
    /// let x: string;           // format_type → "string"
    /// let y: number;           // format_type → "number"
    ///
    /// // Literals
    /// let a: "hello";          // format_type → "\"hello\""
    /// let b: 42;               // format_type → "42"
    ///
    /// // Composed types
    /// type Pair = [string, number];
    /// // format_type(Pair) → "[string, number]"
    ///
    /// type Union = string | number | boolean;
    /// // format_type(Union) → "string | number | boolean"
    ///
    /// // Generics
    /// type Map<K, V> = Record<K, V>;
    /// // format_type(Map<string, number>) → "Record<string, number>"
    ///
    /// // Functions
    /// type Handler = (data: string) => void;
    /// // format_type(Handler) → "(data: string) => void"
    ///
    /// // Objects
    /// type User = { name: string; age: number };
    /// // format_type(User) → "{ name: string; age: number }"
    ///
    /// // Complex
    /// type Complex = Array<{ id: number } | null>;
    /// // format_type(Complex) → "Array<{ id: number } | null>"
    /// ```
    pub fn format_type(&self, type_id: TypeId) -> String {
        // Phase 4.2.1: Use full formatter with DefId context for proper type name display
        let mut formatter = self.ctx.create_type_formatter();
        formatter.format(type_id)
    }
}
