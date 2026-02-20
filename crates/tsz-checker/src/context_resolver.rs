//! `TypeResolver` trait implementation for `CheckerContext`.
//!
//! Split from `context.rs` to keep file sizes manageable.
//! This implements `tsz_solver::TypeResolver` which enables the solver to resolve
//! `TypeData::Lazy(DefId)` references back to cached types during evaluation.

use crate::context::CheckerContext;

/// Implement `TypeResolver` for `CheckerContext` to support Lazy type resolution.
///
/// This enables `ApplicationEvaluator` to resolve `TypeData::Lazy(DefId)` references
/// by looking up the cached type for a symbol. The cache is populated during
/// type checking when `get_type_of_symbol()` is called.
///
/// **Architecture Note:**
/// - `resolve_lazy()` is read-only (looks up from cache)
/// - Cache is populated by `CheckerState::get_type_of_symbol()` before Application evaluation
/// - This separation keeps the solver layer (`ApplicationEvaluator`) independent of checker logic
impl<'a> tsz_solver::TypeResolver for CheckerContext<'a> {
    /// Resolve a symbol reference to its cached type (deprecated).
    ///
    /// `TypeData::Ref` is removed, but we keep this for compatibility.
    /// Converts `SymbolRef` to `SymbolId` and looks up in cache.
    fn resolve_ref(
        &self,
        symbol: tsz_solver::SymbolRef,
        _interner: &dyn tsz_solver::TypeDatabase,
    ) -> Option<tsz_solver::TypeId> {
        let sym_id = tsz_binder::SymbolId(symbol.0);
        self.symbol_types.get(&sym_id).copied()
    }

    /// Resolve a `DefId` to its cached type.
    ///
    /// This looks up the type from the `symbol_types` cache, which is populated
    /// during type checking. Returns None if the symbol hasn't been resolved yet.
    ///
    /// **Callers should ensure `get_type_of_symbol()` is called first** to populate
    /// the cache before calling `resolve_lazy()`.
    fn resolve_lazy(
        &self,
        def_id: tsz_solver::DefId,
        _interner: &dyn tsz_solver::TypeDatabase,
    ) -> Option<tsz_solver::TypeId> {
        use tsz_binder::symbol_flags;

        // Convert DefId to SymbolId using the reverse mapping.
        // Fallback: if the DefId was created by `interner.reference(SymbolRef(N))`,
        // the raw DefId value equals the SymbolId. In that case, use the SymbolId
        // directly and redirect through the proper DefId mapping.
        let sym_id = self.def_to_symbol_id(def_id).or_else(|| {
            // Fallback: `interner.reference(SymbolRef(N))` creates `Lazy(DefId(N))`
            // where N is the raw SymbolId value. The DefId(N) doesn't exist in the
            // definition store. Try using N as a SymbolId and redirect.
            let candidate = tsz_binder::SymbolId(def_id.0);
            let found = self.binder.symbols.get(candidate).is_some()
                || self
                    .lib_contexts
                    .iter()
                    .any(|lib| lib.binder.symbols.get(candidate).is_some())
                || self.all_binders.as_ref().is_some_and(|binders| {
                    binders
                        .iter()
                        .any(|binder| binder.symbols.get(candidate).is_some())
                });
            found.then_some(candidate)
        });
        if let Some(sym_id) = sym_id {
            // If this is a fallback from a raw SymbolId-based DefId, check if there's
            // a proper DefId registered for this symbol and redirect through it.
            if self.def_to_symbol.borrow().get(&def_id).is_none()
                && let Some(&real_def_id) = self.symbol_to_def.borrow().get(&sym_id)
                && real_def_id != def_id
            {
                return self.resolve_lazy(real_def_id, _interner);
            }

            // For classes, check if we should return instance type instead of constructor type.
            // Check both main binder and lib binders for the symbol.
            let symbol = self.binder.symbols.get(sym_id).or_else(|| {
                self.lib_contexts
                    .iter()
                    .find_map(|lib| lib.binder.symbols.get(sym_id))
            });
            if let Some(symbol) = symbol
                && (symbol.flags & symbol_flags::CLASS) != 0
                && let Some(instance_type) = self.symbol_instance_types.get(&sym_id)
            {
                return Some(*instance_type);
            }

            // Look up the cached type for this symbol (constructor type for classes)
            if let Some(&ty) = self.symbol_types.get(&sym_id) {
                tracing::trace!(
                    def_id = def_id.0,
                    sym_id = sym_id.0,
                    type_id = ty.0,
                    name = self
                        .binder
                        .symbols
                        .get(sym_id)
                        .map_or("?", |s| s.escaped_name.as_str()),
                    "resolve_lazy: found in symbol_types cache"
                );
                return Some(ty);
            }
        }

        // Fall back to type_env for types registered via insert_def_with_params
        // (generic lib interfaces like PromiseLike<T>, Map<K,V>, Set<T>, etc.)
        if let Ok(env) = self.type_env.try_borrow()
            && let Some(body) = env.get_def(def_id)
        {
            tracing::trace!(
                def_id = def_id.0,
                type_id = body.0,
                "resolve_lazy: found in type_env"
            );
            return Some(body);
        }

        tracing::trace!(def_id = def_id.0, "resolve_lazy: NOT FOUND");
        None
    }

    /// Get type parameters for a symbol reference (deprecated).
    ///
    /// Type parameters are embedded in the type itself rather than stored separately.
    fn get_type_params(
        &self,
        _symbol: tsz_solver::SymbolRef,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        None
    }

    /// Get type parameters for a Lazy type.
    ///
    /// For type aliases, type parameters are stored in `def_type_params`
    /// and used by the Solver to expand Application(Lazy(DefId), Args).
    ///
    /// For classes/interfaces, type parameters are embedded in the resolved type's shape
    /// (`Callable.type_params`, `Interface.type_params`, etc.) rather than stored separately.
    fn get_lazy_type_params(
        &self,
        def_id: tsz_solver::DefId,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        // Look up type parameters for type aliases
        self.get_def_type_params(def_id)
    }

    fn is_boxed_def_id(&self, def_id: tsz_solver::DefId, kind: tsz_solver::IntrinsicKind) -> bool {
        if let Ok(env) = self.type_env.try_borrow() {
            env.is_boxed_def_id(def_id, kind)
        } else {
            false
        }
    }

    fn is_boxed_type_id(
        &self,
        type_id: tsz_solver::TypeId,
        kind: tsz_solver::IntrinsicKind,
    ) -> bool {
        if let Ok(env) = self.type_env.try_borrow() {
            env.is_boxed_type_id(type_id, kind)
        } else {
            false
        }
    }

    /// Get the boxed interface type for a primitive intrinsic.
    /// Delegates to the type environment which stores boxed types registered from lib.d.ts.
    fn get_boxed_type(&self, kind: tsz_solver::IntrinsicKind) -> Option<tsz_solver::TypeId> {
        if let Ok(env) = self.type_env.try_borrow() {
            env.get_boxed_type(kind)
        } else {
            None
        }
    }

    /// Get the Array<T> interface type from lib.d.ts.
    /// Delegates to the type environment.
    fn get_array_base_type(&self) -> Option<tsz_solver::TypeId> {
        if let Ok(env) = self.type_env.try_borrow() {
            env.get_array_base_type()
        } else {
            None
        }
    }

    /// Get the type parameters for the Array<T> interface.
    /// Delegates to the type environment.
    fn get_array_base_type_params(&self) -> &[tsz_solver::TypeParamInfo] {
        // We can't borrow type_env and return a reference from it (lifetime issue),
        // so we fall back to the interner which stores the same data.
        self.types.get_array_base_type_params()
    }

    /// Get the base class type for a class/interface type.
    ///
    /// This implements the `TypeResolver` trait method for Best Common Type (BCT) algorithm.
    /// For example, given Dog that extends Animal, this returns the type for Animal.
    ///
    /// **Architecture**: Bridges Solver (BCT computation) to Binder (extends clauses) via:
    /// 1. `TypeId` -> `DefId` (from Lazy type)
    /// 2. `DefId` -> `SymbolId` (via `def_to_symbol` mapping)
    /// 3. `SymbolId` -> Parent `SymbolId` (via `InheritanceGraph`)
    /// 4. Parent `SymbolId` -> `TypeId` (via `symbol_types` cache)
    ///
    /// Returns None if:
    /// - The type is not a Lazy type (not a class/interface)
    /// - The `DefId` has no corresponding `SymbolId`
    /// - The class has no base class (no parents in `InheritanceGraph`)
    fn get_base_type(
        &self,
        type_id: tsz_solver::TypeId,
        interner: &dyn tsz_solver::TypeDatabase,
    ) -> Option<tsz_solver::TypeId> {
        use tsz_solver::type_queries;
        use tsz_solver::visitor::{callable_shape_id, object_shape_id, object_with_index_shape_id};

        // 1. First try Lazy types (type aliases, class/interface references)
        if let Some(def_id) = type_queries::get_lazy_def_id(self.types, type_id) {
            // 2. Convert DefId to SymbolId
            let sym_id = self.def_to_symbol_id(def_id)?;

            // 3. Get parents from InheritanceGraph (populated during class/interface binding)
            // Works for both classes (single inheritance) and interfaces (multiple extends)
            let parents = self.inheritance_graph.get_parents(sym_id);

            // 4. Return the first parent's type (the immediate base class/interface)
            // Note: For interfaces with multiple parents, we only return the first one.
            // This is sufficient for BCT which checks all candidates in the set.
            if let Some(parent_sym_id) = parents.first() {
                // Look up the cached type for the parent symbol
                // For classes, we need the instance type, not constructor type
                if let Some(instance_type) = self.symbol_instance_types.get(parent_sym_id) {
                    return Some(*instance_type);
                }
                // Fallback to symbol_types (constructor type) if instance type not available
                return self.symbol_types.get(parent_sym_id).copied();
            }
            return None;
        }

        // 2. For class instance types (ObjectWithIndex types), check the ObjectShape symbol
        if let Some(shape_id) = object_shape_id(interner, type_id)
            .or_else(|| object_with_index_shape_id(interner, type_id))
        {
            let shape = interner.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol {
                // Use InheritanceGraph to get parent
                let parents = self.inheritance_graph.get_parents(sym_id);
                if let Some(&parent_sym_id) = parents.first() {
                    // For classes, try instance_types first; for interfaces, use symbol_types
                    if let Some(instance_type) = self.symbol_instance_types.get(&parent_sym_id) {
                        return Some(*instance_type);
                    }
                    // Fallback to symbol_types (for interfaces)
                    return self.symbol_types.get(&parent_sym_id).copied();
                }
            }
        }

        // 3. For class instance types (Callable types), get the class declaration and check InheritanceGraph
        if let Some(_shape_id) = callable_shape_id(interner, type_id) {
            // Step 1: TypeId -> NodeIndex (Class Declaration)
            if let Some(&decl_idx) = self.class_instance_type_to_decl.get(&type_id) {
                // Step 2: NodeIndex -> SymbolId (Class Symbol)
                // This is the correct way to get the symbol without scope/name lookup issues
                if let Some(sym_id) = self.binder.get_node_symbol(decl_idx) {
                    // Step 3: SymbolId -> Parent SymbolId (via InheritanceGraph)
                    let parents = self.inheritance_graph.get_parents(sym_id);
                    if let Some(&parent_sym_id) = parents.first() {
                        // Step 4: Parent SymbolId -> Parent TypeId (Instance Type)
                        if let Some(instance_type) = self.symbol_instance_types.get(&parent_sym_id)
                        {
                            return Some(*instance_type);
                        }
                    }
                }
            }
        }

        None
    }

    /// Check if a `DefId` corresponds to a numeric enum (not a string enum).
    ///
    /// This determines whether an enum allows bidirectional number assignability (Rule #7).
    /// Numeric enums like `enum E { A = 0 }` allow `number <-> E` assignments.
    /// String enums like `enum F { A = "a" }` do NOT allow `string <-> F` assignments.
    fn is_numeric_enum(&self, def_id: tsz_solver::DefId) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        // Convert DefId to SymbolId
        let Some(sym_id) = self.def_to_symbol_id(def_id) else {
            return false;
        };

        // Get the symbol
        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return false;
        };

        // Enum members resolve via their parent ENUM symbol.
        let enum_symbol = if (symbol.flags & symbol_flags::ENUM_MEMBER) != 0 {
            // It's a member, get the parent enum symbol
            let Some(parent) = self.binder.get_symbol(symbol.parent) else {
                return false;
            };
            parent
        } else if (symbol.flags & symbol_flags::ENUM) != 0 {
            // It's the enum itself
            symbol
        } else {
            return false;
        };

        // Get the enum declaration from the arena
        let decl_idx = if !enum_symbol.value_declaration.is_none() {
            enum_symbol.value_declaration
        } else {
            *enum_symbol
                .declarations
                .first()
                .unwrap_or(&tsz_parser::parser::NodeIndex(0))
        };

        if decl_idx == tsz_parser::parser::NodeIndex(0) {
            return false;
        }

        let Some(enum_decl) = self.arena.get_enum_at(decl_idx) else {
            return false;
        };

        let mut has_string_member = false;

        for &member_idx in &enum_decl.members.nodes {
            let Some(member) = self.arena.get_enum_member_at(member_idx) else {
                continue;
            };

            if !member.initializer.is_none() {
                let Some(init_node) = self.arena.get(member.initializer) else {
                    continue;
                };
                if init_node.kind == SyntaxKind::StringLiteral as u16 {
                    has_string_member = true;
                    break;
                }
            }
        }

        // It's a numeric enum if no string members were found
        !has_string_member
    }

    /// Get the `DefKind` for a `DefId` (Task #32: Graph Isomorphism).
    ///
    /// This enables the Canonicalizer to distinguish between structural types
    /// (`TypeAlias`) and nominal types (Interface/Class/Enum).
    ///
    /// ## Structural vs Nominal
    ///
    /// - **`TypeAlias`**: Structural - `type A = { x: A }` and `type B = { x: B }`
    ///   should canonicalize to the same type with Recursive(0)
    /// - **Interface/Class**: Nominal - Different interfaces are incompatible even
    ///   if structurally identical, so they must keep their Lazy(DefId) reference
    fn get_def_kind(&self, def_id: tsz_solver::DefId) -> Option<tsz_solver::def::DefKind> {
        self.definition_store.get_kind(def_id)
    }

    /// Get the `SymbolId` for a `DefId`.
    ///
    /// Uses the `DefinitionStore` to look up the `symbol_id` stored in `DefinitionInfo`.
    /// This works across checker contexts because `DefinitionStore` is shared.
    fn def_to_symbol_id(&self, def_id: tsz_solver::DefId) -> Option<tsz_binder::SymbolId> {
        self.definition_store
            .get_symbol_id(def_id)
            .map(tsz_binder::SymbolId)
    }

    /// Get the `DefId` for a `SymbolRef` (Phase 4.2: Ref -> Lazy migration).
    ///
    /// This enables converting `SymbolRef` to `DefId` by looking up the `symbol_to_def` mapping.
    /// This is the reverse of `def_to_symbol_id`.
    ///
    /// Returns None if the `SymbolRef` doesn't have a corresponding `DefId`.
    fn symbol_to_def_id(&self, symbol: tsz_solver::SymbolRef) -> Option<tsz_solver::DefId> {
        use tsz_binder::SymbolId;

        // Convert SymbolRef to SymbolId
        let sym_id = SymbolId(symbol.0);

        // Look up in the symbol_to_def mapping (populated by get_or_create_def_id)
        self.symbol_to_def.borrow().get(&sym_id).copied()
    }

    /// Check if a `TypeId` represents a full Enum type (not a specific member).
    ///
    /// Used to distinguish between:
    /// - `enum E` (the enum TYPE - allows `let x: E = 1`)
    /// - `enum E.A` (an enum MEMBER - rejects `let x: E.A = 1`)
    ///
    /// Returns true if:
    /// - `TypeId` is `TypeData::Enum` where Symbol has ENUM flag but not `ENUM_MEMBER` flag
    /// - `TypeId` is a Union of `TypeData::Enum` members from the same parent enum
    ///
    /// Returns false for:
    /// - Enum members (symbols with `ENUM_MEMBER` flag)
    /// - Non-enum types
    fn is_enum_type(
        &self,
        type_id: tsz_solver::TypeId,
        _interner: &dyn tsz_solver::TypeDatabase,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_solver::visitor;

        // Case 1: Direct Enum type key
        if let Some((def_id, _inner)) = visitor::enum_components(self.types, type_id) {
            // Convert DefId to SymbolId
            let Some(sym_id) = self.def_to_symbol_id(def_id) else {
                return false;
            };

            // Get the symbol
            let Some(symbol) = self.binder.get_symbol(sym_id) else {
                return false;
            };

            // It's an enum type if it has ENUM flag but not ENUM_MEMBER flag
            return (symbol.flags & symbol_flags::ENUM) != 0
                && (symbol.flags & symbol_flags::ENUM_MEMBER) == 0;
        }

        // Case 2: Union of Enum members (e.g., the full enum type E = E.A | E.B | ...)
        if let Some(members) = visitor::union_list_id(self.types, type_id) {
            let member_list = self.types.type_list(members);

            // Check if all members are enum members from the same parent enum
            let mut common_parent_sym_id: Option<tsz_binder::SymbolId> = None;
            let mut has_enum_members = false;

            for &member in member_list.iter() {
                if let Some((def_id, _inner)) = visitor::enum_components(self.types, member) {
                    has_enum_members = true;

                    // Check if this is an enum member (not the enum type itself)
                    let Some(sym_id) = self.def_to_symbol_id(def_id) else {
                        return false;
                    };

                    let Some(symbol) = self.binder.get_symbol(sym_id) else {
                        return false;
                    };

                    // If this is an enum member, track the PARENT enum symbol
                    if (symbol.flags & symbol_flags::ENUM_MEMBER) != 0 {
                        // Get the parent symbol (the enum itself)
                        let parent_sym_id = symbol.parent;

                        if let Some(existing_parent) = common_parent_sym_id {
                            if existing_parent != parent_sym_id {
                                // Mixed enums in the union (different parents)
                                return false;
                            }
                        } else {
                            // Track the common parent symbol
                            common_parent_sym_id = Some(parent_sym_id);
                        }
                    } else {
                        // Found an enum type (not a member) in the union
                        // This is unusual but treat it as an enum type
                        return true;
                    }
                }
            }

            // If the union consists entirely of enum members from the same enum,
            // treat it as the enum type
            has_enum_members && common_parent_sym_id.is_some()
        } else {
            false
        }
    }

    /// Get the parent Enum's `DefId` for an Enum Member's `DefId`.
    ///
    /// This enables the Solver to check nominal relationships between enum members
    /// and their parent types (e.g., E.A -> E) without directly accessing Binder symbols.
    fn get_enum_parent_def_id(
        &self,
        member_def_id: tsz_solver::DefId,
    ) -> Option<tsz_solver::DefId> {
        use tsz_binder::symbol_flags;

        // Convert member DefId to SymbolId
        let sym_id = self.def_to_symbol_id(member_def_id)?;

        // Get the symbol
        let symbol = self.binder.get_symbol(sym_id)?;

        // Check if this is an enum member
        if (symbol.flags & symbol_flags::ENUM_MEMBER) == 0 {
            return None;
        }

        // Get the parent symbol (the enum itself)
        let parent_sym_id = symbol.parent;

        // Convert parent SymbolId back to DefId
        // The parent should have a DefId from when it was bound
        let parent_ref = tsz_solver::SymbolRef(parent_sym_id.0);
        if let Some(parent_def_id) = self.symbol_to_def_id(parent_ref) {
            return Some(parent_def_id);
        }

        // Fallback: If the parent doesn't have a DefId mapping yet,
        // we can't provide one. This shouldn't happen in well-formed code.
        None
    }

    fn is_user_enum_def(&self, def_id: tsz_solver::DefId) -> bool {
        use tsz_binder::symbol_flags;

        // Convert DefId to SymbolId
        let sym_id = match self.def_to_symbol_id(def_id) {
            Some(id) => id,
            None => return false,
        };

        // Get the symbol
        let symbol = match self.binder.get_symbol(sym_id) {
            Some(s) => s,
            None => return false,
        };

        // Check if this is a user-defined enum or enum member
        if (symbol.flags & symbol_flags::ENUM) != 0 {
            // This is an enum type - check it's not an intrinsic
            return (symbol.flags & symbol_flags::ENUM_MEMBER) == 0;
        }

        if (symbol.flags & symbol_flags::ENUM_MEMBER) != 0 {
            // This is an enum member - check if the parent is a user-defined enum
            let parent_sym_id = symbol.parent;
            if let Some(parent_symbol) = self.binder.get_symbol(parent_sym_id) {
                // Parent is a user enum if it has ENUM flag but not ENUM_MEMBER
                return (parent_symbol.flags & symbol_flags::ENUM) != 0
                    && (parent_symbol.flags & symbol_flags::ENUM_MEMBER) == 0;
            }
        }

        false
    }
}
