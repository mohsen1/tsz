//! Unified JS/CommonJS export surface synthesis.
//!
//! This module provides a single authority for computing the export shape of a
//! CommonJS/JS module. Instead of each consumer re-deriving the export surface
//! from scratch, they call `resolve_js_export_surface` which synthesizes a
//! `JsExportSurface` combining:
//!
//! - `module.exports = X` (direct module export assignment)
//! - `exports.foo = Y` / `module.exports.foo = Y` (property assignments)
//! - `Object.defineProperty(exports, "foo", desc)` (defineProperty exports)
//! - Prototype property assignments (`Ctor.prototype.method = fn`)
//! - Constructor function -> callable+constructable type upgrade
//!
//! The result is cached per target file index to avoid redundant computation.

use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

/// Represents the synthesized export surface of a JS/CommonJS module.
#[derive(Debug, Clone)]
pub struct JsExportSurface {
    /// The direct `module.exports = X` type, if any.
    /// This is the "base" type that gets intersected with namespace properties.
    pub direct_export_type: Option<TypeId>,

    /// Named property exports from `exports.foo = ...`, `module.exports.foo = ...`,
    /// and `Object.defineProperty(exports, ...)`.
    pub named_exports: Vec<PropertyInfo>,

    /// Prototype method bindings collected from `Ctor.prototype.method = fn` patterns.
    /// These get merged into the constructor's instance type.
    pub prototype_members: Vec<PropertyInfo>,

    /// Whether the module has any CommonJS export patterns at all.
    pub has_commonjs_exports: bool,
}

impl JsExportSurface {
    pub const fn empty() -> Self {
        Self {
            direct_export_type: None,
            named_exports: Vec::new(),
            prototype_members: Vec::new(),
            has_commonjs_exports: false,
        }
    }

    /// Look up a named export by name within this surface.
    ///
    /// Checks `named_exports` first, then `prototype_members`.
    /// Returns the `TypeId` if found. This is the canonical way to check
    /// whether a specific named export exists in a CommonJS module's surface
    /// without re-scanning the AST.
    pub fn lookup_named_export(
        &self,
        name: &str,
        types: &dyn tsz_solver::TypeDatabase,
    ) -> Option<TypeId> {
        let name_atom = types.intern_string(name);
        if let Some(prop) = self.named_exports.iter().find(|p| p.name == name_atom) {
            return Some(prop.type_id);
        }
        if let Some(prop) = self.prototype_members.iter().find(|p| p.name == name_atom) {
            return Some(prop.type_id);
        }
        None
    }

    /// Check whether this surface has a named export with the given name.
    pub fn has_named_export(&self, name: &str, types: &dyn tsz_solver::TypeDatabase) -> bool {
        self.lookup_named_export(name, types).is_some()
    }

    /// Build the final TypeId for this export surface.
    /// Merges direct export type with named exports into a single type.
    pub fn to_type_id(&self, checker: &mut CheckerState<'_>) -> Option<TypeId> {
        if !self.has_commonjs_exports {
            return None;
        }

        let factory = checker.ctx.types.factory();

        let namespace_type = if !self.named_exports.is_empty() {
            Some(factory.object(self.named_exports.clone()))
        } else {
            None
        };

        match (self.direct_export_type, namespace_type) {
            (Some(dt), Some(ns)) => Some(factory.intersection2(dt, ns)),
            (Some(dt), None) => Some(dt),
            (None, Some(ns)) => Some(ns),
            (None, None) => None,
        }
    }

    /// Build the final TypeId, also storing the display name for diagnostics.
    ///
    /// Only applies the display name when the result includes named exports
    /// (i.e., it's a namespace-like type). A bare `module.exports = X` returns
    /// the raw type without a namespace display name, preserving the original
    /// type shape in diagnostics (e.g., `{ a: number }` instead of `typeof import("mod")`).
    pub fn to_type_id_with_display_name(
        &self,
        checker: &mut CheckerState<'_>,
        display_name: Option<String>,
    ) -> Option<TypeId> {
        let type_id = self.to_type_id(checker)?;
        // Only tag with display name if we have named exports (namespace-like).
        // A bare direct export (module.exports = X) keeps the raw type display.
        if let Some(name) = display_name
            && !self.named_exports.is_empty()
        {
            checker.ctx.namespace_module_names.insert(type_id, name);
        }
        Some(type_id)
    }
}

impl<'a> CheckerState<'a> {
    /// Main entry point: resolve the complete JS export surface for a target file.
    ///
    /// This is the ONE AUTHORITY for synthesizing JS/CommonJS export shapes.
    /// All consumers should call this instead of independently re-deriving
    /// export properties from the AST.
    ///
    /// Results are cached per target file index.
    pub(crate) fn resolve_js_export_surface(&mut self, target_file_idx: usize) -> JsExportSurface {
        // Check cache first
        if let Some(cached) = self.ctx.js_export_surface_cache.get(&target_file_idx) {
            return cached.clone();
        }

        let surface = self.compute_js_export_surface(target_file_idx);

        // Cache the result
        self.ctx
            .js_export_surface_cache
            .insert(target_file_idx, surface.clone());

        surface
    }

    /// Resolve JS export surface for a module specifier (resolves to file index first).
    pub(crate) fn resolve_js_export_surface_for_module(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<JsExportSurface> {
        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))?;

        Some(self.resolve_js_export_surface(target_file_idx))
    }

    /// Look up a single named export from a CommonJS module's export surface.
    ///
    /// This is the canonical replacement for `resolve_direct_commonjs_assignment_export_type`.
    /// Instead of re-scanning the target file's AST for `exports.foo = ...` patterns,
    /// it uses the cached `JsExportSurface` which already contains all named exports.
    pub(crate) fn resolve_js_export_named_type(
        &mut self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let surface = self.resolve_js_export_surface_for_module(module_name, source_file_idx)?;
        surface.lookup_named_export(export_name, self.ctx.types)
    }

    /// Check whether a CommonJS module has a named export (without computing its type).
    ///
    /// Uses the cached export surface. Canonical way to suppress TS2305 for
    /// names that exist as `exports.foo = ...` or `module.exports.foo = ...`.
    pub(crate) fn js_export_surface_has_export(
        &mut self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        self.resolve_js_export_surface_for_module(module_name, source_file_idx)
            .is_some_and(|surface| surface.has_named_export(export_name, self.ctx.types))
    }

    /// Build the namespace type for a CommonJS file from its export surface.
    ///
    /// This is the canonical replacement for `commonjs_namespace_type_for_file`.
    /// Instead of re-scanning the AST, it builds the namespace type from the
    /// cached `JsExportSurface`.
    pub(crate) fn js_export_surface_namespace_type(
        &mut self,
        target_file_idx: usize,
    ) -> Option<TypeId> {
        let surface = self.resolve_js_export_surface(target_file_idx);
        if !surface.has_commonjs_exports {
            return None;
        }
        let type_id = surface.to_type_id(self)?;
        if let Some(specifier) = self.ctx.module_specifiers.get(&(target_file_idx as u32)) {
            self.ctx
                .namespace_module_names
                .insert(type_id, specifier.clone());
        }
        Some(type_id)
    }

    /// Compute the JS export surface from scratch (uncached).
    fn compute_js_export_surface(&mut self, target_file_idx: usize) -> JsExportSurface {
        let mut surface = JsExportSurface::empty();

        // 1. Collect direct `module.exports = X` assignment
        surface.direct_export_type = self.compute_direct_module_export_type(target_file_idx);

        // 2. Collect named property exports (`exports.foo = ...`, `module.exports.foo = ...`)
        let mut props = Vec::new();
        self.augment_namespace_props_with_commonjs_exports_for_file(target_file_idx, &mut props);
        surface.named_exports = props;

        // 3. Collect prototype property assignments for constructor functions
        surface.prototype_members = self.collect_prototype_exports_for_file(target_file_idx);

        surface.has_commonjs_exports = surface.direct_export_type.is_some()
            || !surface.named_exports.is_empty()
            || !surface.prototype_members.is_empty();

        surface
    }

    /// Compute the direct `module.exports = X` type for a target file.
    fn compute_direct_module_export_type(&mut self, target_file_idx: usize) -> Option<TypeId> {
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let source_file = target_arena.source_files.first()?;
        let mut rhs_expr = None;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            if let Some(found_rhs) =
                self.direct_commonjs_module_export_assignment_rhs(target_arena, stmt.expression)
            {
                rhs_expr = Some(found_rhs);
            }
        }

        let rhs_expr = rhs_expr?;
        let rhs_type = self.infer_commonjs_export_rhs_type(target_file_idx, rhs_expr, None);
        (rhs_type != TypeId::UNDEFINED).then_some(rhs_type)
    }

    /// Collect prototype property assignments for constructor functions exported from a file.
    ///
    /// Scans for patterns like:
    /// - `Ctor.prototype.method = function() { ... }`
    /// - `Ctor.prototype = { method: function() { ... } }`
    fn collect_prototype_exports_for_file(&mut self, target_file_idx: usize) -> Vec<PropertyInfo> {
        use tsz_parser::parser::NodeIndex;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // Phase 1: Collect prototype member assignments (ctor_name, member_name, rhs_idx)
        // from the arena. This borrows the arena immutably.
        let pending: Vec<(String, String, NodeIndex)> = {
            let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
            let Some(source_file) = target_arena.source_files.first() else {
                return Vec::new();
            };

            let mut pending = Vec::new();
            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = target_arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                    continue;
                }
                let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                    continue;
                };
                let Some(expr_node) = target_arena.get(stmt.expression) else {
                    continue;
                };
                if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                    continue;
                }
                let Some(binary) = target_arena.get_binary_expr(expr_node) else {
                    continue;
                };
                if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                    continue;
                }

                if let Some((ctor_name, member_name)) =
                    Self::parse_prototype_member_assignment(target_arena, binary.left)
                {
                    pending.push((ctor_name, member_name, binary.right));
                }
            }
            pending
        };

        if pending.is_empty() {
            return Vec::new();
        }

        // Phase 2: Infer types for each RHS (borrows self mutably).
        let mut prototype_props: FxHashMap<String, Vec<(String, TypeId)>> = FxHashMap::default();
        for (ctor_name, member_name, rhs_idx) in pending {
            let rhs_type = self.infer_commonjs_export_rhs_type(target_file_idx, rhs_idx, None);
            if rhs_type != TypeId::UNDEFINED {
                prototype_props
                    .entry(ctor_name)
                    .or_default()
                    .push((member_name, rhs_type));
            }
        }

        // Phase 3: Flatten into PropertyInfo entries
        let mut result = Vec::new();
        for members in prototype_props.values() {
            for (idx, (member_name, member_type)) in members.iter().enumerate() {
                let name_atom = self.ctx.types.intern_string(member_name);
                result.push(PropertyInfo {
                    name: name_atom,
                    type_id: *member_type,
                    write_type: *member_type,
                    optional: false,
                    readonly: false,
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: idx as u32,
                });
            }
        }

        result
    }

    /// Parse `Ctor.prototype.member` from the LHS of an assignment.
    /// Returns `(constructor_name, member_name)` if the pattern matches.
    fn parse_prototype_member_assignment(
        arena: &tsz_parser::parser::NodeArena,
        idx: tsz_parser::parser::NodeIndex,
    ) -> Option<(String, String)> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let node = arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let outer_access = arena.get_access_expr(node)?;

        // Get the member name (rightmost part: `.member`)
        let member_ident = arena.get_identifier_at(outer_access.name_or_argument)?;
        let member_name = member_ident.escaped_text.clone();

        // Check that the expression is `Ctor.prototype`
        let proto_node = arena.get(outer_access.expression)?;
        if proto_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let proto_access = arena.get_access_expr(proto_node)?;

        let is_prototype = arena
            .get_identifier_at(proto_access.name_or_argument)
            .is_some_and(|ident| ident.escaped_text == "prototype");
        if !is_prototype {
            return None;
        }

        // Get the constructor name
        let ctor_node = arena.get(proto_access.expression)?;
        if ctor_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let ctor_ident = arena.get_identifier(ctor_node)?;
        let ctor_name = ctor_ident.escaped_text.clone();

        Some((ctor_name, member_name))
    }
}
