//! Namespace type merging and re-export resolution for declaration merging.

use crate::query_boundaries::class_type as query;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::Visibility;

/// Maximum recursion depth for namespace export merging.
///
/// Uses the shared `symbol_resolution_depth` counter to prevent infinite
/// recursion when namespaces re-export each other circularly.
const MAX_MERGE_DEPTH: u32 = 32;

// =============================================================================
// Namespace Type Checking
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Namespace Export Merging
    // =========================================================================

    /// Merge namespace exports into `props`, emitting TS2300 for duplicates.
    ///
    /// Shared loop for class+namespace and function+namespace merging.
    /// When `check_prototype` is true, also treats `"prototype"` as a collision
    /// (classes have an implicit static `prototype` property).
    fn namespace_member_visible_on_exported_surface(
        &self,
        sym_id: SymbolId,
        member_sym_id: SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let parent_name = symbol.escaped_name.as_str();
        let Some(member_symbol) = self.ctx.binder.get_symbol(member_sym_id) else {
            return false;
        };

        let mut saw_matching_namespace_decl = false;
        for &decl_idx in &member_symbol.declarations {
            if decl_idx.is_none() {
                continue;
            }

            let mut current = decl_idx;
            while let Some(ext) = self.ctx.arena.get_extended(current) {
                let parent_idx = ext.parent;
                if parent_idx.is_none() {
                    break;
                }
                let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                    break;
                };
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && self.module_name_matches(parent_idx, parent_name)
                {
                    saw_matching_namespace_decl = true;
                    if self.namespace_declaration_exports_member_publicly(parent_idx) {
                        return true;
                    }
                    break;
                }
                current = parent_idx;
            }
        }

        !saw_matching_namespace_decl
    }

    fn module_name_matches(&self, module_idx: NodeIndex, expected_name: &str) -> bool {
        let Some(node) = self.ctx.arena.get(module_idx) else {
            return false;
        };
        let Some(module) = self.ctx.arena.get_module(node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(module.name) else {
            return false;
        };
        self.ctx
            .arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == expected_name)
    }

    fn namespace_declaration_exports_member_publicly(&self, decl_idx: NodeIndex) -> bool {
        if self
            .ctx
            .binder
            .module_declaration_exports_publicly
            .get(&decl_idx.0)
            .copied()
            .unwrap_or(false)
        {
            return true;
        }

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(module) = self.ctx.arena.get_module(node) else {
            return false;
        };

        if self
            .ctx
            .arena
            .has_modifier_ref(module.modifiers.as_ref(), SyntaxKind::DeclareKeyword)
        {
            return true;
        }

        if let Some(name_node) = self.ctx.arena.get(module.name)
            && (name_node.kind == SyntaxKind::StringLiteral as u16
                || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
        {
            return true;
        }

        // A plain `namespace X { export ... }` (no `export` or `declare` modifier on the
        // namespace itself) still makes its exported members visible at runtime as `X.member`.
        // When the namespace is merged with a class or function, those exports contribute to
        // the constructor type's surface and can conflict with static members.
        // Identifier-named namespaces (not string-literal modules) always export publicly.
        if let Some(name_node) = self.ctx.arena.get(module.name)
            && self.ctx.arena.get_identifier(name_node).is_some()
        {
            return true;
        }

        false
    }

    fn merge_exports_into_props(
        &mut self,
        sym_id: SymbolId,
        props: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
        check_prototype: bool,
    ) {
        use tsz_common::diagnostics::diagnostic_codes;
        use tsz_solver::PropertyInfo;

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        let Some(exports) = symbol.exports.as_ref().cloned() else {
            return;
        };

        for (name, member_id) in exports.iter() {
            if self.ctx.symbol_resolution_set.contains(member_id) {
                continue;
            }
            if !self.namespace_member_visible_on_exported_surface(sym_id, *member_id) {
                continue;
            }

            // Skip type-only exports (interfaces, type aliases) — they don't conflict
            // with value properties. Only value exports collide with existing value props.
            let member_symbol = self.ctx.binder.get_symbol(*member_id);
            let member_flags = member_symbol.map_or(0, |s| s.flags);
            if member_flags & tsz_binder::symbol_flags::VALUE == 0 {
                continue;
            }

            // Skip non-instantiated namespace exports — they don't produce runtime values
            // and should not conflict with class static members.
            // e.g., `declare namespace A { namespace X {} }` merged with `class A { static X = X; }`
            let non_module_value_flags = member_flags
                & (tsz_binder::symbol_flags::VALUE & !tsz_binder::symbol_flags::VALUE_MODULE);
            if non_module_value_flags == 0
                && member_flags & tsz_binder::symbol_flags::VALUE_MODULE != 0
            {
                // The only VALUE flag is VALUE_MODULE — this is a pure namespace export.
                // Check if any of its declarations are instantiated.
                if let Some(sym) = member_symbol {
                    let any_instantiated = sym
                        .declarations
                        .iter()
                        .any(|&d| d.is_some() && self.ctx.arena.is_namespace_instantiated(d));
                    if !any_instantiated {
                        continue;
                    }
                }
            }

            // For pure namespace sub-members, build a structural object type instead
            // of using Lazy(DefId). This prevents the solver from seeing two opaque
            // Lazy types that both resolve to themselves (cycle → false assignable).
            let is_pure_namespace = member_flags
                & (tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::NAMESPACE_MODULE)
                != 0
                && member_flags
                    & (tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::FUNCTION)
                    == 0;
            let type_id = if is_pure_namespace {
                self.build_namespace_object_type(*member_id)
            } else {
                self.get_type_of_symbol(*member_id)
            };
            let name_atom = self.ctx.types.intern_string(name);

            let is_duplicate =
                props.contains_key(&name_atom) || (check_prototype && name == "prototype");
            if is_duplicate {
                // Report TS2300 on the class/function static member that conflicts.
                // TSC reports "Duplicate identifier" on BOTH the class static member
                // and the namespace export when they share the same name.
                // If the conflicting member is inherited (not directly declared),
                // skip TS2300 — the class checker handles it as TS2417 instead,
                // and we must REPLACE the inherited property with the namespace export
                // so that `typeof Derived` reflects the namespace version.
                let found_direct = self.report_duplicate_on_class_static_member(sym_id, name);

                if found_direct {
                    if let Some(export_symbol) = self.ctx.binder.get_symbol(*member_id) {
                        let decl_node = export_symbol.value_declaration;
                        if decl_node != NodeIndex::NONE {
                            let error_node = self
                                .get_declaration_name_node(decl_node)
                                .unwrap_or(decl_node);
                            self.error_at_node_msg(
                                error_node,
                                diagnostic_codes::DUPLICATE_IDENTIFIER,
                                &[name],
                            );
                        }
                    }
                    continue;
                }
                // For inherited (non-direct) duplicates, fall through to replace
                // the inherited property with the namespace export. This ensures
                // `typeof D` uses the namespace's version, which is what tsc does
                // (and what triggers TS2417 when the types are incompatible).
            }

            props.insert(
                name_atom,
                PropertyInfo {
                    name: name_atom,
                    type_id,
                    write_type: type_id,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                },
            );
        }
    }

    /// Build a structural object type for a namespace symbol by collecting its value exports.
    ///
    /// This is used instead of `get_type_of_symbol` for pure namespace sub-members when
    /// merging exports into a class constructor type. Using `get_type_of_symbol` for a
    /// namespace returns `Lazy(DefId)`, and the `type_env` stores `def -> Lazy(def)` (a
    /// self-referential mapping). When the solver tries to check
    /// `Lazy(def_a) <: Lazy(def_b)` for two different namespaces with the same name,
    /// both resolve to themselves and the recursion guard fires, returning
    /// `CycleDetected = true`, falsely treating them as subtypes.
    ///
    /// By constructing a real `Object { prop: type, ... }` type we give the solver
    /// concrete structural types it can compare property-by-property.
    fn build_namespace_object_type(&mut self, sym_id: SymbolId) -> TypeId {
        use tsz_solver::PropertyInfo;

        let depth = self.ctx.symbol_resolution_depth.get();
        if depth >= MAX_MERGE_DEPTH {
            return self.get_type_of_symbol(sym_id);
        }

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return self.get_type_of_symbol(sym_id);
        };
        let Some(exports) = symbol.exports.as_ref().cloned() else {
            return self.ctx.types.factory().object(vec![]);
        };

        self.ctx.symbol_resolution_depth.set(depth + 1);
        let mut props: Vec<PropertyInfo> = Vec::new();
        for (name, &member_id) in exports.iter() {
            if self.ctx.symbol_resolution_set.contains(&member_id) {
                continue;
            }
            let member_symbol = self.ctx.binder.get_symbol(member_id);
            let member_flags = member_symbol.map_or(0, |s| s.flags);
            // Only value exports produce runtime properties
            if member_flags & tsz_binder::symbol_flags::VALUE == 0 {
                continue;
            }
            let member_type = self.get_type_of_symbol(member_id);
            let name_atom = self.ctx.types.intern_string(name);
            props.push(PropertyInfo {
                name: name_atom,
                type_id: member_type,
                write_type: member_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
            });
        }
        self.ctx.symbol_resolution_depth.set(depth);
        self.ctx.types.factory().object(props)
    }

    /// Report TS2300 on the class static member that conflicts with a namespace export.
    /// Returns `true` if a direct (non-inherited) static member was found and reported.
    fn report_duplicate_on_class_static_member(&mut self, sym_id: SymbolId, name: &str) -> bool {
        use tsz_common::diagnostics::diagnostic_codes;
        use tsz_parser::syntax_kind_ext;

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CLASS_DECLARATION
                && node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            let Some(class) = self.ctx.arena.get_class(node) else {
                continue;
            };

            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let is_static = match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(member_node)
                        .is_some_and(|prop| self.has_static_modifier(&prop.modifiers)),
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .ctx
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|method| self.has_static_modifier(&method.modifiers)),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.ctx
                            .arena
                            .get_accessor(member_node)
                            .is_some_and(|accessor| self.has_static_modifier(&accessor.modifiers))
                    }
                    _ => false,
                };
                if !is_static {
                    continue;
                }

                if self
                    .get_member_name_node(member_node)
                    .and_then(|name_idx| self.ctx.arena.get(name_idx))
                    .and_then(|name_node| self.get_identifier_text(name_node))
                    .as_deref()
                    == Some(name)
                {
                    let error_node = self
                        .get_declaration_name_node(member_idx)
                        .unwrap_or(member_idx);
                    self.error_at_node_msg(
                        error_node,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                        &[name],
                    );
                    return true;
                }
            }
        }
        false
    }

    /// Merge namespace exports into a constructor type for class+namespace merging.
    ///
    /// When a class and namespace are merged (same name), the namespace's exports
    /// become accessible as static properties on the class constructor type.
    ///
    /// ## TypeScript Example:
    /// ```typescript
    /// class Foo {
    ///   static bar = 1;
    /// }
    /// namespace Foo {
    ///   export const baz = 2;
    /// }
    /// // Foo.bar and Foo.baz are both accessible
    /// ```
    pub(crate) fn merge_namespace_exports_into_constructor(
        &mut self,
        sym_id: SymbolId,
        ctor_type: TypeId,
    ) -> TypeId {
        use rustc_hash::FxHashMap;
        use tsz_solver::CallableShape;

        // Check recursion depth to prevent stack overflow
        let depth = self.ctx.symbol_resolution_depth.get();
        if depth >= MAX_MERGE_DEPTH {
            return ctor_type;
        }

        let Some(shape) = query::callable_shape_for_type(self.ctx.types, ctor_type) else {
            return ctor_type;
        };

        let mut props: FxHashMap<Atom, _> = shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();

        self.merge_exports_into_props(sym_id, &mut props, true);

        let properties = props.into_values().collect();
        self.ctx.types.factory().callable(CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: shape.construct_signatures.clone(),
            properties,
            string_index: shape.string_index,
            number_index: shape.number_index,
            symbol: None,
            is_abstract: false,
        })
    }

    /// Merge namespace exports into a function type for function+namespace merging.
    ///
    /// When a function and namespace are merged (same name), the namespace's exports
    /// become accessible as static properties on the function type.
    ///
    /// ## TypeScript Example:
    /// ```typescript
    /// function Model() {}
    /// namespace Model {
    ///   export interface Options {}
    /// }
    /// let opts: Model.Options;  // Works because Options is merged into Model
    /// ```
    pub(crate) fn merge_namespace_exports_into_function(
        &mut self,
        sym_id: SymbolId,
        function_type: TypeId,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        use rustc_hash::FxHashMap;
        use tsz_solver::CallableShape;

        // Get a unified CallableShape for the function type.
        // Solver query handles both Callable (overloaded) and Function (single-signature)
        // types, wrapping Functions as single-call-signature callables.
        let Some(shape) = crate::query_boundaries::common::callable_shape_for_type_extended(
            self.ctx.types,
            function_type,
        ) else {
            return (function_type, Vec::new());
        };

        let mut props: FxHashMap<Atom, _> = shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();

        self.merge_exports_into_props(sym_id, &mut props, false);

        let properties = props.into_values().collect();
        let factory = self.ctx.types.factory();
        let merged_type = factory.callable(CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: shape.construct_signatures.clone(),
            properties,
            string_index: shape.string_index,
            number_index: shape.number_index,
            symbol: None,
            is_abstract: false,
        });

        (merged_type, Vec::new())
    }

    // =========================================================================
    // Re-export Resolution
    // =========================================================================

    /// Resolve a re-exported member from a module by following re-export chains.
    ///
    /// This function handles cases where a namespace member is re-exported from
    /// another module using `export { foo } from './bar'` or `export * from './bar'`.
    ///
    /// ## Re-export Chain Resolution:
    /// 1. Check if the member is directly exported from the module
    /// 2. If not, check for named re-exports: `export { foo } from 'bar'`
    /// 3. If not found, check wildcard re-exports: `export * from 'bar'`
    /// 4. Recursively follow re-export chains to find the original member
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // bar.ts
    /// export const foo = 42;
    ///
    /// // a.ts
    /// export { foo } from './bar';
    ///
    /// // b.ts
    /// export * from './a';
    ///
    /// // main.ts
    /// import * as b from './b';
    /// let x = b.foo;  // Should find foo through re-export chain
    /// ```
    pub(crate) fn resolve_reexported_member(
        &self,
        module_specifier: &str,
        member_name: &str,
        lib_binders: &[Arc<tsz_binder::BinderState>],
    ) -> Option<SymbolId> {
        let lookup_in_exports = |binder: &tsz_binder::BinderState,
                                 module_exports: &tsz_binder::SymbolTable|
         -> Option<SymbolId> {
            if let Some(sym_id) = module_exports.get(member_name) {
                return Some(sym_id);
            }

            let export_equals_sym_id = module_exports.get("export=")?;
            let export_equals_symbol = binder.get_symbol(export_equals_sym_id)?;

            if let Some(exports) = export_equals_symbol.exports.as_ref()
                && let Some(sym_id) = exports.get(member_name)
            {
                return Some(sym_id);
            }

            if let Some(members) = export_equals_symbol.members.as_ref()
                && let Some(sym_id) = members.get(member_name)
            {
                return Some(sym_id);
            }

            for &candidate_id in binder
                .get_symbols()
                .find_all_by_name(&export_equals_symbol.escaped_name)
            {
                let Some(candidate_symbol) = binder.get_symbol(candidate_id) else {
                    continue;
                };
                if (candidate_symbol.flags
                    & (tsz_binder::symbol_flags::MODULE
                        | tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE))
                    == 0
                {
                    continue;
                }
                if let Some(exports) = candidate_symbol.exports.as_ref()
                    && let Some(sym_id) = exports.get(member_name)
                {
                    return Some(sym_id);
                }
                if let Some(members) = candidate_symbol.members.as_ref()
                    && let Some(sym_id) = members.get(member_name)
                {
                    return Some(sym_id);
                }
            }

            None
        };

        // First, check if it's a direct export from this module
        if let Some(module_exports) = self.ctx.binder.module_exports.get(module_specifier)
            && let Some(sym_id) = lookup_in_exports(self.ctx.binder, module_exports)
        {
            // Found direct export - but we need to resolve if it's itself a re-export
            // Get the symbol and check if it's an alias
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.flags & tsz_binder::symbol_flags::ALIAS != 0
            {
                // Follow the alias
                if let Some(ref import_module) = symbol.import_module {
                    let export_name = symbol.import_name.as_deref().unwrap_or(member_name);
                    return self.resolve_reexported_member(import_module, export_name, lib_binders);
                }
            }
            return Some(sym_id);
        }

        // Check for named re-exports: `export { foo } from 'bar'`
        if let Some(file_reexports) = self.ctx.binder.reexports.get(module_specifier)
            && let Some((source_module, original_name)) = file_reexports.get(member_name)
        {
            let name_to_lookup = original_name.as_deref().unwrap_or(member_name);
            return self.resolve_reexported_member(source_module, name_to_lookup, lib_binders);
        }

        // Check for wildcard re-exports: `export * from 'bar'`
        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_specifier) {
            for source_module in source_modules {
                if let Some(sym_id) =
                    self.resolve_reexported_member(source_module, member_name, lib_binders)
                {
                    return Some(sym_id);
                }
            }
        }

        // Check lib binders for the module
        for lib_binder in lib_binders {
            // First check lib binder's module_exports
            if let Some(module_exports) = lib_binder.module_exports.get(module_specifier)
                && let Some(sym_id) = lookup_in_exports(lib_binder, module_exports)
            {
                return Some(sym_id);
            }
            // Then check lib binder's re-exports
            if let Some(file_reexports) = lib_binder.reexports.get(module_specifier)
                && let Some((source_module, original_name)) = file_reexports.get(member_name)
            {
                let name_to_lookup = original_name.as_deref().unwrap_or(member_name);
                return self.resolve_reexported_member(source_module, name_to_lookup, lib_binders);
            }
            // Then check lib binder's wildcard re-exports
            if let Some(source_modules) = lib_binder.wildcard_reexports.get(module_specifier) {
                for source_module in source_modules {
                    if let Some(sym_id) =
                        self.resolve_reexported_member(source_module, member_name, lib_binders)
                    {
                        return Some(sym_id);
                    }
                }
            }
        }

        None
    }

    /// Merge namespace exports into an object type for enum+namespace merging.
    ///
    /// When an enum and namespace are merged (same name), the namespace's exports
    /// become accessible as properties on the enum object.
    ///
    /// ## TypeScript Example:
    /// ```typescript
    /// enum Direction {
    ///   Up = 1,
    ///   Down = 2
    /// }
    /// namespace Direction {
    ///   export function isVertical(d: Direction): boolean {
    ///     return d === Direction.Up || d === Direction.Down;
    ///   }
    /// }
    /// // Direction.Up and Direction.isVertical() are both accessible
    /// ```
    pub(crate) fn merge_namespace_exports_into_object(
        &mut self,
        sym_id: SymbolId,
        _enum_type: TypeId,
    ) -> TypeId {
        use rustc_hash::FxHashMap;
        use tsz_solver::PropertyInfo;

        // Check recursion depth to prevent stack overflow
        let depth = self.ctx.symbol_resolution_depth.get();
        if depth >= MAX_MERGE_DEPTH {
            return _enum_type; // Prevent infinite recursion in merge
        }

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return _enum_type;
        };
        let Some(exports) = symbol.exports.as_ref() else {
            return _enum_type;
        };

        let mut props: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();

        // Merge ALL exports from the symbol (enum members + namespace exports)
        // This allows accessing both enum members and namespace methods via EnumName.Member
        for (name, member_id) in exports.iter() {
            // Skip if this member is already being resolved (prevents infinite recursion)
            if self.ctx.symbol_resolution_set.contains(member_id) {
                continue; // Skip circular references
            }
            if !self.namespace_member_visible_on_exported_surface(sym_id, *member_id) {
                continue;
            }

            let Some(member_symbol) = self.ctx.binder.get_symbol(*member_id) else {
                continue;
            };
            use tsz_binder::symbol_flags;
            if member_symbol.flags & symbol_flags::VALUE == 0 {
                continue;
            }

            let mut type_id = self.get_type_of_symbol(*member_id);
            if member_symbol.flags & symbol_flags::INTERFACE != 0 {
                let mut candidate = self.type_of_value_declaration_for_symbol(
                    *member_id,
                    member_symbol.value_declaration,
                );
                if candidate == TypeId::UNKNOWN || candidate == TypeId::ERROR {
                    for &decl_idx in &member_symbol.declarations {
                        let cand = self.type_of_value_declaration_for_symbol(*member_id, decl_idx);
                        if cand != TypeId::UNKNOWN && cand != TypeId::ERROR {
                            candidate = cand;
                            break;
                        }
                    }
                }
                if candidate != TypeId::UNKNOWN && candidate != TypeId::ERROR {
                    type_id = candidate;
                }
            }
            let name_atom = self.ctx.types.intern_string(name);
            props.entry(name_atom).or_insert(PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
            });
        }

        let properties: Vec<PropertyInfo> = props.into_values().collect();
        self.ctx.types.object_with_flags_and_symbol(
            properties,
            tsz_solver::ObjectFlags::ENUM_NAMESPACE,
            Some(tsz_binder::SymbolId(sym_id.0)),
        )
    }
}
