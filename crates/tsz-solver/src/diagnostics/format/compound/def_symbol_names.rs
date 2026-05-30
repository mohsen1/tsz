impl<'a> TypeFormatter<'a> {
    /// Resolve a `DefId` to a human-readable name via the definition store,
    /// falling back to `"<prefix>(<raw_id>)"` if unavailable.
    pub(super) fn format_def_id(
        &mut self,
        def_id: crate::def::DefId,
        fallback_prefix: &str,
    ) -> String {
        if let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
        {
            let name = self.format_def_name(&def);
            // Class constructor defs represent the static side of a class.
            // tsc displays these as "typeof ClassName".
            if def.is_class_constructor() {
                return format!("typeof {name}");
            }
            return name;
        }
        // NOTE: We do NOT use format_raw_def_id_symbol_fallback here.
        // DefId and SymbolId are independent ID spaces. A DefId's raw value
        // should never be interpreted as a SymbolId — doing so would return
        // the name of an unrelated symbol that happens to share the same u32.
        format!("{}({})", fallback_prefix, def_id.0)
    }

    /// Format a `DefId` with type parameters appended when the definition is generic.
    ///
    /// tsc displays uninstantiated generic types with their type parameter names:
    /// e.g., `B<T>` instead of just `B`. This matches that behavior for
    /// `TypeData::Lazy(DefId)` nodes that represent generic types without
    /// an `Application` wrapper.
    pub(super) fn format_def_id_with_type_params(
        &mut self,
        def_id: crate::def::DefId,
        fallback_prefix: &str,
    ) -> String {
        if let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
        {
            let name = self.format_def_name(&def);
            // Class constructor defs (DefKind::ClassConstructor) represent the
            // static side of a class. tsc displays these as "typeof ClassName".
            let prefix = if def.is_class_constructor() {
                "typeof "
            } else {
                ""
            };
            if def.type_params.is_empty() {
                return format!("{prefix}{name}");
            }
            let params: Vec<String> = def
                .type_params
                .iter()
                .map(|tp| self.atom(tp.name).to_string())
                .collect();
            return format!("{prefix}{}<{}>", name, params.join(", "));
        }
        // NOTE: We do NOT use format_raw_def_id_symbol_fallback here.
        // DefId and SymbolId are independent ID spaces — see comment above.
        format!("{}({})", fallback_prefix, def_id.0)
    }

    // NOTE: format_raw_def_id_symbol_fallback was removed.
    // It incorrectly assumed DefId.0 == SymbolId.0, which caused wrong type
    // names in diagnostics (e.g., enum "Foo" displaying as "timeout").
    // DefId and SymbolId are independent ID spaces and must not be conflated.

    /// Try to resolve a human-readable name for an object shape via symbol or def store lookup.
    pub(super) fn resolve_object_shape_name(&mut self, shape: &ObjectShape) -> Option<String> {
        // The empty object `{}` is a universally-shared shape. `find_def_by_shape`
        // is keyed on structural hash, so any *type alias* registered with an
        // empty body (e.g., `type T52 = T50<unknown>` reducing to `{}`) would
        // repaint every user-written `{}` annotation with that alias's name.
        // Skip the def-name fallback for empty anonymous shapes when the
        // matched def is a type alias; named empty types (interfaces, classes)
        // still resolve through `shape.symbol` above this guard, and lib
        // interfaces below are unaffected because the Object-interface special
        // case handles the only realistic empty lib shape.
        let shape_is_empty_anonymous = shape.symbol.is_none()
            && shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none();
        if let Some(sym_id) = shape.symbol
            && let Some(name) = self.format_symbol_name(sym_id)
        {
            // Namespace/module/enum value types are displayed as `typeof Name` by tsc.
            if let Some(arena) = self.symbol_arena
                && let Some(sym) = arena.get(sym_id)
            {
                use tsz_binder::symbol_flags;
                let is_namespace =
                    sym.has_any_flags(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE);
                let is_enum = sym.has_any_flags(symbol_flags::ENUM);
                let is_class = sym.has_flags(symbol_flags::CLASS);
                let is_interface = sym.has_any_flags(symbol_flags::INTERFACE);
                // When a symbol is both an interface and a namespace (declaration
                // merging), the type-space name wins — tsc displays `B`, not
                // `typeof B`.  Similarly, classes take priority over namespaces.
                if (is_namespace || is_enum) && !is_class && !is_interface {
                    return Some(format!("typeof {name}"));
                }
            }
            return Some(name);
        }
        // Fall back to def-store structural lookup for type aliases and lib interfaces.
        // User-defined interfaces preserve their symbol through merge_interface_types, so they
        // are found via path 1 above. Anonymous types (symbol=None) cannot accidentally match
        // named interfaces (symbol=Some(...)) via find_def_by_shape because PartialEq includes symbol.
        // This path handles: (a) type aliases (always symbol=None), and (b) lib interfaces
        // (built without symbol stamps, e.g. String) whose unique structural content prevents
        // false matches.
        //
        // Exception: for empty anonymous shapes (`{}`), skip the fallback
        // when the matched def's name would repaint the universal empty
        // shape:
        //   - a type alias whose body reduces to `{}`
        //     (e.g., `type T52 = T50<unknown>`),
        //   - a generic interface or class whose own shape registration was
        //     created with empty properties (e.g., `Promise<T>` registered
        //     before its body was populated).
        // In all of these, every user-written `{}` annotation would otherwise
        // pick up the unrelated def name. tsc shows the literal `{}`.
        if let Some(def_store) = self.def_store
            && let Some(def_id) = def_store.find_def_by_shape(shape)
            && let Some(def) = def_store.get(def_id)
        {
            use crate::def::DefKind;
            let skip_for_empty_alias = shape_is_empty_anonymous
                && (def.kind == DefKind::TypeAlias
                    || (matches!(def.kind, DefKind::Interface | DefKind::Class)
                        && !def.type_params.is_empty()));
            if !skip_for_empty_alias {
                return Some(self.format_def_name(&def));
            }
        }
        // Special case: detect the global Object interface by its characteristic properties.
        // The Object interface has: constructor, toString, toLocaleString, valueOf,
        // hasOwnProperty, isPrototypeOf, propertyIsEnumerable.
        // When we see an object shape with exactly these properties (in any order), display as "Object".
        if shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.len() >= 6
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "constructor")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "toString")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "toLocaleString")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "valueOf")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "hasOwnProperty")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "isPrototypeOf")
        {
            return Some("Object".to_string());
        }
        // Special case: detect the global RegExp interface by characteristic
        // members so diagnostics prefer `RegExp` over expanded structural shape.
        // This mirrors tsc display behavior in contexts like import attributes.
        if shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.len() >= 10
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "exec")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "test")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "source")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "global")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "ignoreCase")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "multiline")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "lastIndex")
        {
            return Some("RegExp".to_string());
        }
        None
    }

    pub(super) fn format_symbol_name(&mut self, sym_id: SymbolId) -> Option<String> {
        let arena = self.symbol_arena?;
        let sym = arena.get(sym_id)?;
        let mut qualified_name = sym.escaped_name.to_string();
        let mut current_parent = sym.parent;

        use tsz_binder::symbol_flags;

        // Walk up the parent chain, qualifying with enum parents only.
        // tsc qualifies type names with their containing enum (e.g., `Choice.Yes`)
        // but uses SHORT names for types inside namespaces (e.g., `Line` not `A.Line`)
        // unless disambiguation is needed (same name in outer scope). Namespace
        // qualification requires scope-aware disambiguation not yet implemented.
        // Skip file-level module symbols (synthetic names like __test1__, "file.ts", etc.)
        // as those represent file modules, not declared namespaces.
        while current_parent != SymbolId::NONE {
            if let Some(parent_sym) = arena.get(current_parent) {
                let is_qualifying_parent = parent_sym.has_any_flags(symbol_flags::ENUM);
                let name = &parent_sym.escaped_name;
                let is_file_module = name.starts_with('"')
                    || name.starts_with("__")
                    || name.contains('/')
                    || name.contains('\\')
                    || name.is_empty();
                if is_qualifying_parent && !is_file_module {
                    qualified_name = format!("{}.{}", parent_sym.escaped_name, qualified_name);
                    current_parent = parent_sym.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Some(qualified_name)
    }

    /// Resolve a `SymbolRef` (from `TypeQuery` / `ModuleNamespace`) to a display name.
    /// Tries the symbol arena first, then falls back to the definition store's
    /// `find_def_by_symbol` lookup.
    /// Resolve the variable name for a unique symbol, for use in `typeof varName` display.
    /// Public so callers outside the format module can use this (e.g., TS2367 path).
    pub fn resolve_unique_symbol_name(&mut self, sym: SymbolRef) -> Option<String> {
        self.resolve_symbol_ref_name(sym)
    }

    pub(super) fn resolve_symbol_ref_name(&mut self, sym: SymbolRef) -> Option<String> {
        if let Some(name) = self.format_symbol_name(SymbolId(sym.0)) {
            return Some(name);
        }
        // Fallback: try the definition store by symbol id
        if let Some(def_store) = self.def_store
            && let Some(def_id) = def_store.find_def_by_symbol(sym.0)
            && let Some(def) = def_store.get(def_id)
        {
            return Some(self.format_def_name(&def));
        }
        None
    }

    pub(super) fn format_def_name(&mut self, def: &crate::def::DefinitionInfo) -> String {
        // Try to build a qualified name by walking the symbol parent chain.
        // tsc qualifies type names with their containing enum (e.g., `Choice.Yes`)
        // but uses SHORT names for types inside namespaces (e.g., `Line` not `A.Line`).
        let def_name = self.atom(def.name).to_string();
        if let Some(sym_raw) = def.symbol_id
            && let Some(arena) = self.symbol_arena
            && let Some(symbol) = arena.get(SymbolId(sym_raw))
        {
            use tsz_binder::symbol_flags;
            let foreign_symbol_name_collision = symbol.escaped_name != def_name
                && self
                    .current_file_id
                    .zip(def.file_id)
                    .is_some_and(|(current_file_id, def_file_id)| current_file_id != def_file_id);

            if foreign_symbol_name_collision {
                return def_name;
            }

            // For anonymous class expressions assigned to variables, the binder
            // creates a symbol named "(Anonymous class)" but tsc displays the
            // variable name instead. Prefer the definition's name in this case.
            let base_name = if symbol.escaped_name == "(Anonymous class)" {
                def_name
            } else {
                symbol.escaped_name.to_string()
            };
            let mut qualified_name = base_name;
            let mut current_parent = symbol.parent;

            while current_parent != SymbolId::NONE {
                if let Some(parent_sym) = arena.get(current_parent) {
                    // Only qualify with enum parents, not namespace/module parents.
                    let is_qualifying_parent = parent_sym.has_any_flags(symbol_flags::ENUM);
                    let name = &parent_sym.escaped_name;
                    let is_file_module = name.starts_with('"')
                        || name.starts_with("__")
                        || name.contains('/')
                        || name.contains('\\')
                        || name.is_empty();
                    if is_qualifying_parent && !is_file_module {
                        qualified_name = format!("{}.{}", parent_sym.escaped_name, qualified_name);
                        current_parent = parent_sym.parent;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            return qualified_name;
        }

        // Fallback: use the short (unqualified) definition name.
        def_name
    }

    /// Namespace-qualify a symbol name for contexts where disambiguation is needed
    /// (e.g., union display with same-name members from different namespaces).
    pub(super) fn namespace_qualify_symbol_name(
        &self,
        sym_id: SymbolId,
        current_name: String,
    ) -> String {
        let Some(arena) = self.symbol_arena else {
            return current_name;
        };
        let Some(symbol) = arena.get(sym_id) else {
            return current_name;
        };
        let mut parts = vec![current_name];
        let mut current_parent = symbol.parent;
        use tsz_binder::symbol_flags;

        while current_parent != SymbolId::NONE {
            if let Some(parent_sym) = arena.get(current_parent) {
                let is_qualifying_parent =
                    parent_sym.has_any_flags(symbol_flags::MODULE | symbol_flags::ENUM);
                let name = &parent_sym.escaped_name;
                let is_file_module = name.starts_with('"')
                    || name.starts_with("__")
                    || name.contains('/')
                    || name.contains('\\')
                    || name.is_empty();
                if is_qualifying_parent && !is_file_module {
                    parts.push(parent_sym.escaped_name.clone());
                    current_parent = parent_sym.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if parts.len() == 1 {
            return parts.pop().expect("parts has one element");
        }

        parts.reverse();
        parts.join(".")
    }

    /// Returns a sort key for intrinsic/builtin types to match tsc's display ordering.
    /// tsc orders builtins as: string(8), number(9), bigint(10), boolean(11), etc.
    const fn builtin_sort_key(id: TypeId) -> Option<u32> {
        match id {
            TypeId::NUMBER => Some(9),
            TypeId::STRING => Some(8),
            TypeId::BIGINT => Some(10),
            TypeId::BOOLEAN | TypeId::BOOLEAN_TRUE => Some(11),
            TypeId::BOOLEAN_FALSE => Some(12),
            TypeId::VOID => Some(13),
            TypeId::UNDEFINED => Some(14),
            TypeId::NULL => Some(15),
            TypeId::SYMBOL => Some(16),
            TypeId::OBJECT => Some(17),
            TypeId::FUNCTION => Some(18),
            _ if id.is_intrinsic() => Some(id.0),
            _ => None,
        }
    }

    /// Returns (tier, `file_id`, `span_start`) for a type, used for source-order sorting.
    /// - Tier 0: Builtins/intrinsics (always first)
    /// - Tier 1: User-defined types with source info (sorted by file, then position)
    /// - Tier 2: Types without source info (preserve original order by returning sentinel)
    pub(super) fn get_source_position_for_type(
        &self,
        type_id: TypeId,
        def_store: &crate::def::DefinitionStore,
    ) -> (u32, u32, u32) {
        // Tier 0: Intrinsics have fixed position
        if let Some(key) = Self::builtin_sort_key(type_id) {
            return (0, 0, key);
        }

        let data = self.interner.lookup(type_id);

        // Type parameters are modeled as `TypeData::TypeParameter` and lose direct
        // declaration span information unless the checker registers their DefId.
        // When available, use that DefId span so diagnostics can display unions in
        // declaration/source order (e.g. `Top | T | U` instead of alloc-order drift).
        if matches!(data, Some(TypeData::TypeParameter(_) | TypeData::Infer(_)))
            && let Some(def_id) = def_store.find_def_for_type(type_id)
            && let Some(def) = def_store.get(def_id)
            && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
        {
            return (1, file_id, span_start);
        }

        // If a structural type is displayed through an alias such as
        // `Iterator<T>`, use that visible alias for source-order sorting too.
        // Otherwise diagnostics can print the alias while sorting by its
        // expanded helper shape, flipping unions like `Iterator<T> | Iterable<T>`.
        if let Some(alias_origin) = self
            .interner
            .get_display_alias(type_id)
            .filter(|&alias| alias != type_id)
        {
            match self.interner.lookup(alias_origin) {
                Some(TypeData::Application(alias_app_id)) => {
                    return self.get_application_source_position(
                        alias_app_id,
                        def_store,
                        Some(type_id),
                    );
                }
                Some(TypeData::Lazy(def_id)) => {
                    if let Some(def) = def_store.get(def_id)
                        && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
                    {
                        return (1, file_id, span_start);
                    }
                }
                _ => {}
            }
        }

        // Try Lazy(DefId) - type aliases, interfaces, classes
        if let Some(TypeData::Lazy(def_id)) = &data
            && let Some(def) = def_store.get(*def_id)
            && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
        {
            return (1, file_id, span_start);
        }

        // Try Application - generic instantiation. Use the MAX position of the
        // base and the type arguments so that types like `Container<Cover>`
        // (modeled as `Application(Container, [Cover])`) sort with their
        // user-defined element type rather than with a built-in/lib base.
        if let Some(TypeData::Application(app_id)) = &data {
            return self.get_application_source_position(*app_id, def_store, Some(type_id));
        }

        // Try Array - structural shorthand for `Array<T>`. Use the element's
        // position +1 so the array form sorts with its element but always
        // immediately after it. This keeps `Cover | Cover[]` displays in
        // source declaration order (and the canonical interner ordering of
        // the union doesn't matter because the element vs. array tie-break
        // is decided by this offset).
        if let Some(TypeData::Array(elem)) = &data {
            let (tier, file, span) = self.get_source_position_for_type(*elem, def_store);
            if tier == 0 {
                return (tier, file, span.saturating_add(100));
            }
            return (tier, file, span.saturating_add(1));
        }

        // Try ReadonlyType - `readonly T[]` modifier wrapping an inner type.
        // Use the inner type's position +1 for the same reason.
        if let Some(TypeData::ReadonlyType(inner)) = &data {
            let (tier, file, span) = self.get_source_position_for_type(*inner, def_store);
            if tier == 0 {
                return (tier, file, span.saturating_add(100));
            }
            return (tier, file, span.saturating_add(1));
        }

        // Try Enum
        if let Some(TypeData::Enum(def_id, _)) = &data
            && let Some(def) = def_store.get(*def_id)
            && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
        {
            return (1, file_id, span_start);
        }

        // Try Object/ObjectWithIndex with symbol
        if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) = &data {
            let shape = self.interner.object_shape(*shape_id);
            if let Some(sym_id) = shape.symbol
                && let Some(def_id) = def_store.find_def_by_symbol(sym_id.0)
                && let Some(def) = def_store.get(def_id)
                && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
            {
                return (1, file_id, span_start);
            }
        }

        // Try Callable with symbol
        if let Some(TypeData::Callable(shape_id)) = &data {
            let shape = self.interner.callable_shape(*shape_id);
            if let Some(sym_id) = shape.symbol
                && let Some(def_id) = def_store.find_def_by_symbol(sym_id.0)
                && let Some(def) = def_store.get(def_id)
                && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
            {
                return (1, file_id, span_start);
            }
        }

        // Tier 2: Fallback for anonymous types without source info.
        // For Object types, sort by property count (fewer properties first) to match
        // tsc's display order for anonymous object unions like `{} | { a: number }`.
        if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) = &data {
            let shape = self.interner.object_shape(*shape_id);
            let prop_count = shape.properties.len() as u32;
            // Use property count as the sort key. Objects with fewer properties
            // are displayed first by tsc.
            return (2, 0, prop_count);
        }

        // Other tier 2 types: sort after objects, preserve relative order
        (2, u32::MAX, u32::MAX)
    }

    fn get_application_source_position(
        &self,
        app_id: crate::types::TypeApplicationId,
        def_store: &crate::def::DefinitionStore,
        skip_type: Option<TypeId>,
    ) -> (u32, u32, u32) {
        let app = self.interner.type_application(app_id);
        let mut best = self.get_source_position_for_type(app.base, def_store);
        for &arg in &app.args {
            if Some(arg) == skip_type {
                continue;
            }
            let candidate = self.get_source_position_for_type(arg, def_store);
            if candidate > best {
                best = candidate;
            }
        }
        best
    }
}
