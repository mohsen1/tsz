//! Type Environment Module
//!
//! Extracted from state.rs: Methods for building type environments, evaluating
//! application types, resolving property access types, and type node resolution.

use crate::query_boundaries::state_type_environment as query;
use crate::state::{CheckerState, EnumKind, MAX_INSTANTIATION_DEPTH};
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::MappedTypeId;
use tsz_solver::SourceLocation;
use tsz_solver::TypeId;
use tsz_solver::Visibility;

impl<'a> CheckerState<'a> {
    // Get type of object literal.
    // =========================================================================
    // Type Relations (uses solver::CompatChecker for assignability)
    // =========================================================================

    // Note: enum_symbol_from_type and enum_symbol_from_value_type are defined in type_checking.rs

    pub(crate) fn enum_object_type(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        use rustc_hash::FxHashMap;
        use tsz_solver::{IndexSignature, ObjectFlags, ObjectShape, PropertyInfo};

        let factory = self.ctx.types.factory();
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

                // Fix: Create nominal enum member types for each member
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
                let specific_member_type = factory.enum_type(member_def_id, literal_type);

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
            return Some(factory.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
                properties,
                string_index: None,
                number_index,
                symbol: None,
            }));
        }

        Some(factory.object(properties))
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
    /// - **`IndexAccess`** (`Type["key"]`): Indexed access types
    /// - **`KeyOf`** (`keyof Type`): Keyof operator types
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
        if !query::is_generic_type(self.ctx.types, type_id) {
            return type_id;
        }

        // Memoize monomorphic application evaluation. This is a hot path for
        // repeated accesses on aliases like DeepPartial<{...}>.
        let can_cache =
            !tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, type_id);
        if can_cache
            && let Some(&cached) = self
                .ctx
                .narrowing_cache
                .resolve_cache
                .borrow()
                .get(&type_id)
        {
            return cached;
        }

        // Canonicalize application keys by evaluating type arguments first. This
        // allows structurally equivalent applications from different declaration
        // sites (e.g., repeated inline object-literal args) to share a cache hit.
        let mut canonical_key: Option<TypeId> = None;
        if can_cache
            && let Some((base, args)) = query::application_info(self.ctx.types, type_id)
            && !args.is_empty()
        {
            let canonical_args: Vec<TypeId> = args
                .into_iter()
                .map(|arg| self.resolve_lazy_type(arg))
                .collect();
            let key = self.ctx.types.application(base, canonical_args);
            if key != type_id {
                canonical_key = Some(key);
                if let Some(&cached) = self.ctx.narrowing_cache.resolve_cache.borrow().get(&key) {
                    self.ctx
                        .narrowing_cache
                        .resolve_cache
                        .borrow_mut()
                        .insert(type_id, cached);
                    return cached;
                }
            }
        }

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
        if can_cache {
            let mut cache = self.ctx.narrowing_cache.resolve_cache.borrow_mut();
            cache.insert(type_id, result);
            if let Some(key) = canonical_key {
                cache.insert(key, result);
            }
        }

        result
    }

    pub(crate) fn evaluate_application_type_inner(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::{TypeSubstitution, instantiate_type};

        let Some((base, args)) = query::application_info(self.ctx.types, type_id) else {
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
        // For conditional type bodies with Application extends containing infer
        // (e.g., `T extends Promise<infer U> ? U : T`), preserve Application args
        // so the conditional evaluator can match at the Application level.
        let body_has_conditional_app_infer =
            self.body_is_conditional_with_application_infer(body_type);
        let evaluated_args: Vec<TypeId> = args
            .iter()
            .map(|&arg| {
                if body_has_conditional_app_infer && query::is_generic_type(self.ctx.types, arg) {
                    arg // Preserve Application form
                } else {
                    self.evaluate_type_with_env(arg)
                }
            })
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

    /// Check if a type body is a Conditional type whose `extends_type` is an Application.
    /// This detects patterns like `T extends Promise<infer U> ? U : T`.
    fn body_is_conditional_with_application_infer(&self, body_type: TypeId) -> bool {
        let Some(cond) = query::get_conditional_type(self.ctx.types, body_type) else {
            return false;
        };
        query::is_generic_type(self.ctx.types, cond.extends_type)
    }

    /// Evaluate a mapped type with symbol resolution.
    /// This handles cases like `{ [K in keyof Ref(sym)]: Template }` where the Ref
    /// needs to be resolved to get concrete keys.
    pub(crate) fn evaluate_mapped_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        // Memoize mapped-type expansion for monomorphic inputs.
        // This is a hot path for repeated property access on mapped aliases
        // (e.g., DeepPartial<...>).
        let can_cache =
            !tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, type_id);
        if can_cache
            && let Some(&cached) = self
                .ctx
                .narrowing_cache
                .resolve_cache
                .borrow()
                .get(&type_id)
        {
            return cached;
        }

        // NOTE: Manual lookup preferred here - we need the mapped_id directly
        // to call mapped_type(mapped_id) below. Using get_mapped_type would
        // return the full Arc<MappedType>, which is more than needed.
        let Some(mapped_id) = query::mapped_type_id(self.ctx.types, type_id) else {
            return type_id;
        };

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
        if can_cache {
            self.ctx
                .narrowing_cache
                .resolve_cache
                .borrow_mut()
                .insert(type_id, result);
        }
        result
    }

    pub(crate) fn evaluate_mapped_type_with_resolution_inner(
        &mut self,
        type_id: TypeId,
        mapped_id: MappedTypeId,
    ) -> TypeId {
        use tsz_solver::{PropertyInfo, TypeSubstitution, instantiate_type};
        let factory = self.ctx.types.factory();

        let mapped = self.ctx.types.mapped_type(mapped_id);

        // Evaluate the constraint to get concrete keys
        let keys = self.evaluate_mapped_constraint_with_resolution(mapped.constraint);

        // Extract string literal keys
        let string_keys =
            tsz_solver::type_queries::extract_string_literal_keys(self.ctx.types, keys);
        if string_keys.is_empty() {
            // Can't evaluate - return original
            return type_id;
        }

        // Build the resulting object properties
        let mut properties = Vec::new();
        for key_name in string_keys {
            // Create the key literal type
            let key_literal = self.ctx.types.literal_string_atom(key_name);

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
            let property_type = if let Some((obj, _idx)) =
                query::index_access_types(self.ctx.types, property_type)
            {
                // For IndexAccess types, we need to resolve the object type and get the property
                // First, check if obj is a Lazy type that needs resolution
                let obj_type = if let Some(def_id) = query::lazy_def_id(self.ctx.types, obj) {
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
                if let Some(prop) = tsz_solver::type_queries::find_property_in_object(
                    self.ctx.types,
                    obj_type,
                    key_name,
                ) {
                    prop.type_id
                } else {
                    // Property not found or not an object type, fall back
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

        factory.object(properties)
    }

    /// Evaluate a mapped type constraint with symbol resolution.
    /// Handles keyof Ref(sym) by resolving the Ref and getting its keys.
    pub(crate) fn evaluate_mapped_constraint_with_resolution(
        &mut self,
        constraint: TypeId,
    ) -> TypeId {
        match query::classify_mapped_constraint(self.ctx.types, constraint) {
            query::MappedConstraintKind::KeyOf(operand) => {
                // Evaluate the operand with symbol resolution
                let evaluated = self.evaluate_type_with_resolution(operand);
                self.get_keyof_type(evaluated)
            }
            query::MappedConstraintKind::Resolved => constraint,
            query::MappedConstraintKind::Other => {
                // Resolve Lazy(DefId) and other unresolved constraint types.
                // For example, `type Keys = "a" | "b"; { [P in Keys]: T }` has a
                // Lazy(DefId) constraint that must be resolved to get `"a" | "b"`.
                let resolved = self.evaluate_type_with_resolution(constraint);
                if resolved != constraint {
                    resolved
                } else {
                    constraint
                }
            }
        }
    }

    // Lazy type resolution, property access type resolution, and type environment
    // population methods are in `state_type_environment_lazy.rs`.

    /// Create a `TypeEnvironment` populated with resolved symbol types.
    ///
    /// This can be passed to `is_assignable_to_with_env` for type checking
    /// that needs to resolve type references.
    pub fn build_type_environment(&mut self) -> tsz_solver::TypeEnvironment {
        use tsz_binder::symbol_flags;

        // Collect unique symbols from user code only (node_symbols).
        // Lib symbols from file_locals are NOT included here — they are resolved
        // lazily on demand during statement checking. This avoids the O(N) upfront
        // cost of eagerly resolving ~2000 lib symbols, saving ~30-50ms per file.
        let mut symbols: Vec<SymbolId> = Vec::with_capacity(self.ctx.binder.node_symbols.len());
        let mut seen: FxHashSet<SymbolId> = FxHashSet::default();
        for &sym_id in self.ctx.binder.node_symbols.values() {
            if seen.insert(sym_id) {
                symbols.push(sym_id);
            }
        }

        // Sort symbols so type-defining symbols (functions, classes, interfaces, type aliases)
        // are processed BEFORE variable/parameter symbols.
        symbols.sort_by_key(|&sym_id| {
            let flags = self.ctx.binder.get_symbol(sym_id).map_or(0, |s| s.flags);
            let is_type_defining = flags
                & (symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::INTERFACE
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::ENUM
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE)
                != 0;
            (u8::from(!is_type_defining), sym_id.0)
        });

        // Resolve each symbol and add to the environment.
        // Skip variable/parameter symbols — their types are computed lazily during
        // statement checking when proper enclosing_class context is available.
        for sym_id in symbols {
            // Skip variable and parameter symbols - their types will be computed
            // lazily during statement checking with proper class context
            let flags = self.ctx.binder.get_symbol(sym_id).map_or(0, |s| s.flags);
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
    /// - Creates a temporary `CheckerState` for the other arena
    /// - Delegates type parameter extraction to the temporary checker
    ///
    /// ## Type Parameter Information:
    /// - Returns Vec<TypeParamInfo> with parameter names and constraints
    /// - Includes default type arguments if present
    /// - Used by `TypeEnvironment` for generic type expansion
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

        let mut sym_id = sym_id;
        if let Some(symbol) = self.get_symbol_globally(sym_id)
            && symbol.flags & symbol_flags::ALIAS != 0
        {
            let mut visited_aliases = Vec::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                sym_id = target;
            }
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
        let (flags, value_decl, declarations, sym_escaped_name) =
            match self.get_symbol_globally(sym_id) {
                Some(symbol) => (
                    symbol.flags,
                    symbol.value_declaration,
                    symbol.declarations.clone(),
                    symbol.escaped_name.clone(),
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
            // Guard against deep cross-arena recursion (shared with all delegation points)
            if !Self::enter_cross_arena_delegation() {
                self.ctx.leave_recursion();
                return Vec::new();
            }

            let mut checker = Box::new(CheckerState::with_parent_cache(
                symbol_arena.as_ref(),
                self.ctx.binder,
                self.ctx.types,
                self.ctx.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self, // Share parent's cache to fix Cache Isolation Bug
            ));
            let result = checker.get_type_params_for_symbol(sym_id);

            // DO NOT merge child's symbol_types back. See delegate_cross_arena_symbol_resolution
            // for the full explanation: node_symbols collisions across arenas cause cache poisoning.

            Self::leave_cross_arena_delegation();

            if !result.is_empty() {
                self.ctx.insert_def_type_params(def_id, result.clone());
                self.ctx.def_no_type_params.borrow_mut().remove(&def_id);
                self.ctx.leave_recursion();
                return result;
            }
            // Cross-arena delegation returned no type params. This can happen when
            // a user-defined generic type (e.g., `interface Tag<T>`) is merged with
            // a non-generic lib symbol of the same name. The lib arena doesn't have
            // the user's declaration, so it finds no type params. Fall through to
            // check the current arena's declarations below.
        }

        // Type alias - get type parameters from declaration
        if flags & symbol_flags::TYPE_ALIAS != 0 {
            let decl_idx = if value_decl.is_some() {
                value_decl
            } else {
                declarations.first().copied().unwrap_or(NodeIndex::NONE)
            };
            if decl_idx.is_some()
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
            let decl_idx = if value_decl.is_some() {
                value_decl
            } else {
                declarations.first().copied().unwrap_or(NodeIndex::NONE)
            };
            if decl_idx.is_some()
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

        // Interface - get type parameters from merged declarations.
        // When interfaces are merged (e.g., `interface Foo {}` + `interface Foo<T> {}`),
        // we must check ALL declarations because the first one may not have type params.
        // TypeScript considers the union of type parameters across all declarations.
        if flags & symbol_flags::INTERFACE != 0 {
            // Helper: verify that a node in the given arena is an interface whose name
            // matches the symbol's escaped_name. This prevents cross-arena NodeIndex
            // collisions from returning the wrong interface's type parameters (e.g.,
            // Promise declarations that accidentally resolve to Float32Array in a
            // different lib arena).
            let is_matching_interface =
                |arena: &tsz_parser::parser::NodeArena, decl: NodeIndex| -> bool {
                    if let Some(node) = arena.get(decl)
                        && let Some(iface) = arena.get_interface(node)
                    {
                        if let Some(name_node) = arena.get(iface.name)
                            && let Some(name_ident) = arena.get_identifier(name_node)
                        {
                            return name_ident.escaped_text.as_str() == sym_escaped_name;
                        }
                        // If we can't resolve the name, accept it (backwards compat)
                        return true;
                    }
                    false
                };

            // First try value_decl, then search all declarations for one with type params
            let mut decl_candidates = Vec::new();
            if value_decl.is_some() && is_matching_interface(self.ctx.arena, value_decl) {
                decl_candidates.push(value_decl);
            }
            for &decl in &declarations {
                if decl != value_decl && is_matching_interface(self.ctx.arena, decl) {
                    decl_candidates.push(decl);
                }
            }
            // Try each interface declaration; use the first one that has type parameters
            for decl_idx in &decl_candidates {
                if let Some(node) = self.ctx.arena.get(*decl_idx)
                    && let Some(iface) = self.ctx.arena.get_interface(node)
                    && iface
                        .type_parameters
                        .as_ref()
                        .is_some_and(|tp| !tp.is_empty())
                {
                    let (params, updates) = self.push_type_parameters(&iface.type_parameters);
                    self.pop_type_parameters(updates);
                    if !params.is_empty() {
                        self.ctx.insert_def_type_params(def_id, params.clone());
                        self.ctx.def_no_type_params.borrow_mut().remove(&def_id);
                        self.ctx.leave_recursion();
                        return params;
                    }
                }
            }
            // Fallback: if no declaration has type params, use the first declaration
            if let Some(&decl_idx) = decl_candidates.first()
                && let Some(node) = self.ctx.arena.get(decl_idx)
                && let Some(iface) = self.ctx.arena.get_interface(node)
            {
                let (params, updates) = self.push_type_parameters(&iface.type_parameters);
                self.pop_type_parameters(updates);
                // params will be empty - mark as no type params
                self.ctx.def_no_type_params.borrow_mut().insert(def_id);
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
    /// Handles empty (→ NEVER), single (→ that type), and multi-member cases.
    /// Automatically normalizes: flattens nested unions, deduplicates, sorts.
    pub fn get_union_type(&self, types: Vec<TypeId>) -> TypeId {
        tsz_solver::utils::union_or_single(self.ctx.types, types)
    }

    /// Create an intersection type from multiple types.
    ///
    /// Handles empty (→ NEVER), single (→ that type), and multi-member cases.
    /// Automatically normalizes: flattens nested intersections, deduplicates, sorts.
    pub fn get_intersection_type(&self, types: Vec<TypeId>) -> TypeId {
        tsz_solver::utils::intersection_or_single(self.ctx.types, types)
    }

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
            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
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
                        self.error_type_only_value_at(&name, type_ref.type_name);
                        self.ctx.node_types.insert(idx.0, TypeId::ERROR);
                        return TypeId::ERROR;
                    }
                }

                // Validate the type reference exists before lowering
                // Check cache first - but allow re-resolution of ERROR when type params
                // are in scope, since the ERROR may have been cached when type params
                // weren't available yet (non-deterministic symbol processing order).
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
                if let Some(&cached) = self.ctx.node_types.get(&idx.0)
                    && cached != TypeId::ERROR
                    && self.ctx.type_parameter_scope.is_empty()
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
                                self.error_type_only_value_at(
                                    keyword_name,
                                    array_type.element_type,
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
        checker.check(idx)
    }

    // =========================================================================
    // Source Location Tracking & Solver Diagnostics
    // =========================================================================

    /// Get a source location for a node.
    pub fn get_source_location(&self, idx: NodeIndex) -> Option<SourceLocation> {
        let node = self.ctx.arena.get(idx)?;
        Some(SourceLocation::new(
            self.ctx.file_name.clone(),
            node.pos,
            node.end,
        ))
    }

    // Report a type not assignable error using solver diagnostics with source tracking.
    // This is the basic error that just says "Type X is not assignable to Y".
    // For detailed errors with elaboration (e.g., "property 'x' is missing"),
    // use `error_type_not_assignable_with_reason_at` instead.

    // Report a cannot find name error using solver diagnostics with source tracking.
    // Enhanced to provide suggestions for similar names, import suggestions, and
    // library change suggestions for ES2015+ types.

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
    /// This is the main entry point for converting `TypeId` representations into
    /// human-readable type strings. Used throughout the type checker for error
    /// messages, quick info, and IDE features.
    ///
    /// ## Formatting Strategy:
    /// - Delegates to the solver's `TypeFormatter`
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
    /// - Type References: Display as `RefName`<Args>
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
