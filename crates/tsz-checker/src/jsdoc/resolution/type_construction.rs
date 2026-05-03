//! JSDoc type construction — building complex types from JSDoc annotations.
//!
//! This module handles:
//! - Enum annotation resolution (`jsdoc_enum_annotation_type_for_symbol_decl`)
//! - Assigned value type resolution (`resolve_jsdoc_assigned_value_type`)
//! - Prototype assignment resolution (`resolve_jsdoc_prototype_assignment_type`)
//! - Typedef/callback definition management (`ensure_jsdoc_typedef_def`)
//! - Generic type instantiation (`resolve_jsdoc_generic_type`)
//! - Tuple type parsing (`parse_jsdoc_tuple_type`)
//! - Object literal type parsing (`parse_jsdoc_object_literal_type`)
//! - Mapped type parsing (`parse_jsdoc_mapped_type`)
//! - Call/method signature parsing (`parse_jsdoc_call_signature`, `parse_jsdoc_method_signature`)
//! - Typedef/callback type construction (`type_from_jsdoc_typedef`, `type_from_jsdoc_callback`)

use super::super::types::{JsdocCallbackInfo, JsdocTypedefInfo};
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{
    FunctionShape, IndexSignature, ObjectShape, ParamInfo, PropertyInfo, TupleElement, TypeId,
    TypePredicate, TypePredicateTarget, Visibility,
};
impl<'a> CheckerState<'a> {
    pub(in crate::jsdoc::resolution) fn jsdoc_enum_annotation_type_for_symbol_decl(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        decl: NodeIndex,
    ) -> Option<TypeId> {
        let file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);

        if file_idx == self.ctx.current_file_idx && self.ctx.arena.get(decl).is_some() {
            return self.jsdoc_enum_annotation_type_for_current_checker(decl);
        }

        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;
        let arena = all_arenas.get(file_idx)?;
        let binder = all_binders.get(file_idx)?;
        let source_file = arena.source_files.first()?;

        let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
            arena.as_ref(),
            binder.as_ref(),
            self.ctx.types,
            source_file.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
            tsz_common::perf_counters::CheckerCreationReason::JsDocTypeConstruction,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.current_file_idx = file_idx;
        self.ctx.copy_symbol_file_targets_to_attributed(
            &mut checker.ctx,
            tsz_common::perf_counters::CheckerCreationReason::JsDocTypeConstruction,
        );

        let result = checker.jsdoc_enum_annotation_type_for_current_checker(decl);
        self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        result
    }

    fn jsdoc_enum_annotation_type_for_current_checker(
        &mut self,
        decl: NodeIndex,
    ) -> Option<TypeId> {
        let sf = self.source_file_data_for_node(decl)?;
        if sf.comments.is_empty() || !sf.comments.iter().any(|c| c.is_multi_line) {
            return None;
        }

        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        let node = self.ctx.arena.get(decl)?;
        let jsdoc = self.try_jsdoc_with_ancestor_walk(decl, &comments, &source_text)?;
        if !jsdoc.contains("@enum") {
            return None;
        }

        let type_expr = Self::extract_jsdoc_enum_type_expression(&jsdoc)?.trim();
        let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
        self.ctx.jsdoc_typedef_anchor_pos.set(node.pos);
        let result = self.resolve_jsdoc_reference(type_expr);
        self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
        result.filter(|ty| *ty != TypeId::ERROR && *ty != TypeId::UNKNOWN)
    }

    pub(in crate::jsdoc::resolution) fn jsdoc_declared_value_symbol_prefers_value_type(
        &self,
        sym_id: tsz_binder::SymbolId,
        decl: NodeIndex,
    ) -> bool {
        let arena = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(sym_id, decl))
            .and_then(|arenas| arenas.first().map(Arc::as_ref))
            .or_else(|| self.ctx.binder.symbol_arenas.get(&sym_id).map(Arc::as_ref))
            .unwrap_or_else(|| {
                let file_idx = self
                    .ctx
                    .resolve_symbol_file_index(sym_id)
                    .unwrap_or(self.ctx.current_file_idx);
                self.ctx.get_arena_for_file(file_idx as u32)
            });

        let Some(node) = arena.get(decl) else {
            return false;
        };

        if let Some(var_decl) = arena.get_variable_declaration(node)
            && var_decl.initializer.is_none()
        {
            return true;
        }

        arena
            .source_files
            .first()
            .is_some_and(|sf| sf.file_name.ends_with(".d.ts"))
    }

    fn resolve_jsdoc_assigned_value_type_inner(
        &mut self,
        name: &str,
        allow_prototype_only_fallback: bool,
    ) -> Option<TypeId> {
        if let Some(ty) =
            self.resolve_jsdoc_assigned_value_type_in_arena(name, allow_prototype_only_fallback)
        {
            return Some(ty);
        }

        let Some(all_arenas) = self.ctx.all_arenas.clone() else {
            return allow_prototype_only_fallback
                .then_some(())
                .and_then(|_| self.resolve_jsdoc_prototype_assignment_type(name));
        };
        let Some(all_binders) = self.ctx.all_binders.clone() else {
            return allow_prototype_only_fallback
                .then_some(())
                .and_then(|_| self.resolve_jsdoc_prototype_assignment_type(name));
        };

        for (file_idx, (arena, binder)) in all_arenas.iter().zip(all_binders.iter()).enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }

            for source_file in &arena.source_files {
                let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
                    arena.as_ref(),
                    binder.as_ref(),
                    self.ctx.types,
                    source_file.file_name.clone(),
                    self.ctx.compiler_options.clone(),
                    self,
                    tsz_common::perf_counters::CheckerCreationReason::JsDocTypeConstruction,
                ));
                checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
                checker.ctx.copy_cross_file_state_from(&self.ctx);
                checker.ctx.current_file_idx = file_idx;
                self.ctx.copy_symbol_file_targets_to_attributed(
                    &mut checker.ctx,
                    tsz_common::perf_counters::CheckerCreationReason::JsDocTypeConstruction,
                );

                if let Some(ty) = checker
                    .resolve_jsdoc_assigned_value_type_in_arena(name, allow_prototype_only_fallback)
                {
                    self.ctx.merge_symbol_file_targets_from(&checker.ctx);
                    return Some(ty);
                }
            }
        }

        None
    }

    fn resolve_jsdoc_assigned_value_type_in_arena(
        &mut self,
        name: &str,
        allow_prototype_only_fallback: bool,
    ) -> Option<TypeId> {
        let prototype_type = self.resolve_jsdoc_prototype_assignment_type(name);

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if self.expression_text(binary.left).as_deref() != Some(name) {
                continue;
            }

            let right_type = {
                let rhs = self.ctx.arena.skip_parenthesized(binary.right);
                if self.js_assignment_rhs_is_void_zero(rhs) {
                    None
                } else {
                    self.ctx
                        .arena
                        .get(rhs)
                        .map(|_| self.get_type_of_node(rhs))
                        .and_then(|ty| {
                            (ty != TypeId::ERROR
                                && ty != TypeId::UNKNOWN
                                && ty != TypeId::UNDEFINED)
                                .then_some(ty)
                        })
                }
            };

            if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(idx) {
                let combined =
                    self.combine_jsdoc_instance_and_prototype_type(jsdoc_type, prototype_type);
                return Some(self.relabel_jsdoc_assigned_value_type(name, combined));
            }
            if let Some(stmt_idx) = self.enclosing_expression_statement(idx)
                && let Some(jsdoc_type) = self.js_statement_declared_type(stmt_idx).or_else(|| {
                    let sf = self.source_file_data_for_node(stmt_idx)?;
                    let source_text = sf.text.to_string();
                    let comments = sf.comments.clone();
                    let jsdoc =
                        self.try_jsdoc_with_ancestor_walk(stmt_idx, &comments, &source_text)?;
                    self.resolve_jsdoc_type_from_comment(&jsdoc, self.ctx.arena.get(stmt_idx)?.pos)
                })
            {
                let combined =
                    self.combine_jsdoc_instance_and_prototype_type(jsdoc_type, prototype_type);
                return Some(self.relabel_jsdoc_assigned_value_type(name, combined));
            }
            let left_root = self.expression_root(binary.left);
            if left_root != binary.left
                && let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(left_root)
            {
                let combined =
                    self.combine_jsdoc_instance_and_prototype_type(jsdoc_type, prototype_type);
                return Some(self.relabel_jsdoc_assigned_value_type(name, combined));
            }
            if let Some(instance_type) = right_type {
                let combined =
                    self.combine_jsdoc_instance_and_prototype_type(instance_type, prototype_type);
                return Some(self.relabel_jsdoc_assigned_value_type(name, combined));
            }
        }

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                continue;
            };
            if self.expression_text(expr_stmt.expression).as_deref() != Some(name) {
                continue;
            }
            if let Some(jsdoc_type) = self.js_statement_declared_type(idx) {
                return Some(
                    self.combine_jsdoc_instance_and_prototype_type(jsdoc_type, prototype_type),
                );
            }
        }

        allow_prototype_only_fallback.then_some(())?;
        Some(self.relabel_jsdoc_assigned_value_type(name, prototype_type?))
    }

    fn relabel_jsdoc_assigned_value_type(&mut self, name: &str, ty: TypeId) -> TypeId {
        if ty == TypeId::ANY || ty == TypeId::ERROR || ty == TypeId::UNKNOWN {
            return ty;
        }

        let display_name = name.rsplit('.').next();
        if display_name.is_none() {
            return ty;
        }
        let display_name = display_name.expect("split guaranteed by next() check");
        if display_name.is_empty() {
            return ty;
        }

        let def_id = self.ensure_jsdoc_assigned_value_def(display_name, ty);
        self.ctx.definition_store.register_type_to_def(ty, def_id);
        self.ctx
            .types
            .store_display_alias(ty, self.ctx.types.factory().lazy(def_id));
        ty
    }

    fn ensure_jsdoc_assigned_value_def(
        &mut self,
        name: &str,
        body_type: TypeId,
    ) -> tsz_solver::def::DefId {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        let atom_name = self.ctx.types.intern_string(name);
        if let Some(candidates) = self.ctx.definition_store.find_defs_by_name(atom_name) {
            for def_id in candidates {
                if let Some(def) = self.ctx.definition_store.get(def_id)
                    && matches!(def.kind, DefKind::TypeAlias)
                    && def.body == Some(body_type)
                    && def.type_params.is_empty()
                {
                    return def_id;
                }
            }
        }

        let info = DefinitionInfo::type_alias(atom_name, Vec::new(), body_type);
        self.ctx.definition_store.register(info)
    }

    pub(crate) fn resolve_jsdoc_assigned_value_type(&mut self, name: &str) -> Option<TypeId> {
        self.resolve_jsdoc_assigned_value_type_inner(name, true)
    }

    pub(crate) fn resolve_jsdoc_assigned_value_type_for_write(
        &mut self,
        name: &str,
    ) -> Option<TypeId> {
        self.resolve_jsdoc_assigned_value_type_inner(name, false)
    }

    /// Resolve an anonymous `@typedef {type}` attached to a declaration matching
    /// `name`. In tsc, a nameless `@typedef` inherits the name of the following
    /// declaration, creating a type alias visible in type-position lookups.
    pub(crate) fn resolve_anonymous_typedef_for_name(&mut self, name: &str) -> Option<TypeId> {
        if let Some(ty) = self.resolve_anonymous_typedef_in_arena(name) {
            return Some(ty);
        }

        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;

        for (file_idx, (arena, binder)) in all_arenas.iter().zip(all_binders.iter()).enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }
            for source_file in &arena.source_files {
                let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
                    arena.as_ref(),
                    binder.as_ref(),
                    self.ctx.types,
                    source_file.file_name.clone(),
                    self.ctx.compiler_options.clone(),
                    self,
                    tsz_common::perf_counters::CheckerCreationReason::JsDocTypeConstruction,
                ));
                checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
                checker.ctx.copy_cross_file_state_from(&self.ctx);
                checker.ctx.current_file_idx = file_idx;

                if let Some(ty) = checker.resolve_anonymous_typedef_in_arena(name) {
                    return Some(ty);
                }
            }
        }
        None
    }

    fn resolve_anonymous_typedef_in_arena(&mut self, name: &str) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if self.expression_text(binary.left).as_deref() != Some(name) {
                continue;
            }
            let Some(stmt_idx) = self.enclosing_expression_statement(idx) else {
                continue;
            };
            let Some(sf) = self.source_file_data_for_node(stmt_idx) else {
                continue;
            };
            let source_text = sf.text.to_string();
            let comments = sf.comments.clone();
            let Some(jsdoc) = self.try_jsdoc_with_ancestor_walk(stmt_idx, &comments, &source_text)
            else {
                continue;
            };
            if let Some(base_type_expr) = Self::extract_anonymous_typedef_base_type(&jsdoc)
                && let Some(resolved) = self.resolve_jsdoc_reference(&base_type_expr)
            {
                return Some(resolved);
            }
        }
        None
    }

    fn resolve_jsdoc_prototype_assignment_type(&mut self, name: &str) -> Option<TypeId> {
        let prototype_name = format!("{name}.prototype");

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if self.expression_text(binary.left).as_deref() != Some(prototype_name.as_str()) {
                continue;
            }

            let rhs = self.ctx.arena.skip_parenthesized(binary.right);
            let Some(rhs_node) = self.ctx.arena.get(rhs) else {
                continue;
            };
            if rhs_node.kind != tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }

            let resolved = self.get_type_of_node(rhs);
            if resolved != TypeId::ANY && resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }

        None
    }

    fn combine_jsdoc_instance_and_prototype_type(
        &mut self,
        instance_type: TypeId,
        prototype_type: Option<TypeId>,
    ) -> TypeId {
        let Some(prototype_type) = prototype_type else {
            return instance_type;
        };

        if matches!(instance_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return prototype_type;
        }
        if matches!(
            prototype_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
        ) || instance_type == prototype_type
        {
            return instance_type;
        }

        self.ctx
            .types
            .factory()
            .intersection2(instance_type, prototype_type)
    }
    fn ensure_jsdoc_typedef_def(
        &mut self,
        name: &str,
        body_type: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> tsz_solver::def::DefId {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        let def_id = if type_params.is_empty()
            && let Some(def_id) = self.ctx.definition_store.find_type_alias_by_body(body_type)
        {
            def_id
        } else {
            let atom_name = self.ctx.types.intern_string(name);
            let mut found = None;
            if let Some(candidates) = self.ctx.definition_store.find_defs_by_name(atom_name) {
                for def_id in candidates {
                    if let Some(def) = self.ctx.definition_store.get(def_id)
                        && matches!(def.kind, DefKind::TypeAlias)
                        && def.body == Some(body_type)
                        && def.type_params.as_slice() == type_params
                    {
                        found = Some(def_id);
                        break;
                    }
                }
            }
            found.unwrap_or_else(|| {
                let info = DefinitionInfo::type_alias(atom_name, type_params.to_vec(), body_type);
                self.ctx.definition_store.register(info)
            })
        };

        // Attach a display-alias so diagnostic messages can recover the
        // typedef name `Foo` instead of expanding to the body's structural
        // form (e.g. `{ value?: number; }`). Mirrors tsc's preserve-alias
        // policy for `@typedef`-named types in TS2375 / TS2322 messages.
        // No-op when `body_type == lazy(def_id)` (e.g. recursive typedefs)
        // or when storing would alias an intrinsic — `store_display_alias`
        // applies its own safety guards.
        let alias_lazy = self.ctx.types.factory().lazy(def_id);
        self.ctx.types.store_display_alias(body_type, alias_lazy);

        def_id
    }

    /// Register a DefId for a JSDoc `@typedef` so the type formatter can find the alias name.
    pub(in crate::jsdoc) fn register_jsdoc_typedef_def(&mut self, name: &str, body_type: TypeId) {
        let _ = self.ensure_jsdoc_typedef_def(name, body_type, &[]);
    }

    fn ensure_jsdoc_instantiated_display_def(
        &mut self,
        name: &str,
        type_id: TypeId,
    ) -> tsz_solver::def::DefId {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        let atom_name = self.ctx.types.intern_string(name);
        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(type_id)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && matches!(def.kind, DefKind::TypeAlias)
            && def.name == atom_name
        {
            self.ctx
                .definition_store
                .register_type_to_def(type_id, def_id);
            return def_id;
        }

        let def_id = self
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(atom_name, Vec::new(), type_id));
        self.ctx
            .definition_store
            .register_type_to_def(type_id, def_id);
        def_id
    }
    /// Resolve a generic JSDoc type reference: `Name<Arg1, Arg2, ...>`.
    pub(in crate::jsdoc::resolution) fn resolve_jsdoc_generic_type(
        &mut self,
        base_name: &str,
        type_args: Vec<TypeId>,
    ) -> Option<TypeId> {
        if let Some(instantiated) = self.resolve_jsdoc_generic_typedef_type(base_name, &type_args) {
            return Some(instantiated);
        }

        // Handle import type base names: import('./module').Foo
        if base_name.starts_with("import(")
            && let Some((module_specifier, Some(member_name))) =
                Self::parse_jsdoc_import_type(base_name)
        {
            let sym_id = self.resolve_jsdoc_import_member(&module_specifier, &member_name)?;
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            if resolved == TypeId::ERROR || resolved == TypeId::UNKNOWN {
                return None;
            }
            let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
            if body_type == TypeId::ERROR {
                return None;
            }
            if type_args.is_empty() {
                return Some(body_type);
            }
            if type_params.is_empty() {
                return None;
            }

            use crate::query_boundaries::common::instantiate_generic;
            let instantiated =
                instantiate_generic(self.ctx.types, body_type, &type_params, &type_args);
            self.register_jsdoc_generic_display_name(base_name, &type_args, instantiated);
            return Some(instantiated);
        }

        // Look up the base type in file_locals (includes merged lib types like Partial, Record)
        let sym_id = if let Some(sym_id) = self.ctx.binder.file_locals.get(base_name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags
                & (symbol_flags::TYPE_ALIAS
                    | symbol_flags::CLASS
                    | symbol_flags::INTERFACE
                    | symbol_flags::ENUM))
                == 0
            {
                return None;
            }
            sym_id
        } else {
            let symbols = self.ctx.binder.get_symbols();
            symbols
                .find_all_by_name(base_name)
                .iter()
                .copied()
                .find(|&sym_id| {
                    self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        (symbol.flags
                            & (symbol_flags::TYPE_ALIAS
                                | symbol_flags::CLASS
                                | symbol_flags::INTERFACE
                                | symbol_flags::ENUM))
                            != 0
                    })
                })?
        };
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        if body_type == TypeId::ERROR {
            return None;
        }
        if type_args.is_empty() {
            return Some(body_type);
        }
        if type_params.is_empty() {
            return None;
        }
        // Directly instantiate the type body with the provided type arguments.
        // Do NOT evaluate here — the caller (jsdoc_satisfies_annotation_with_pos)
        // calls judge_evaluate, which will expand mapped types while preserving
        // Lazy(DefId) references in value positions for correct type name display.
        use crate::query_boundaries::common::instantiate_generic;
        let instantiated = instantiate_generic(self.ctx.types, body_type, &type_params, &type_args);
        // Register a display def `Name<Args>` so diagnostics format the
        // instantiated type with its original alias plus the supplied args
        // (`ClassComponent<any>`), matching tsc behavior. The typedef path
        // (`resolve_jsdoc_generic_typedef_type`) does the same registration.
        self.register_jsdoc_generic_display_name(base_name, &type_args, instantiated);
        Some(instantiated)
    }

    /// Register a display def `BaseName<Arg1, Arg2, ...>` for an instantiated
    /// generic JSDoc type reference so diagnostics preserve the original
    /// alias plus the supplied type arguments.
    fn register_jsdoc_generic_display_name(
        &mut self,
        base_name: &str,
        type_args: &[TypeId],
        instantiated: TypeId,
    ) {
        if instantiated == TypeId::ERROR || instantiated == TypeId::UNKNOWN {
            return;
        }
        let args_display = type_args
            .iter()
            .map(|&arg| self.format_type_diagnostic(arg))
            .collect::<Vec<_>>()
            .join(", ");
        let display_name = format!("{base_name}<{args_display}>");
        let _ = self.ensure_jsdoc_instantiated_display_def(&display_name, instantiated);
    }
    pub(in crate::jsdoc::resolution) fn parse_jsdoc_tuple_type(
        &mut self,
        type_expr: &str,
    ) -> Option<TypeId> {
        let inner = type_expr[1..type_expr.len() - 1].trim();
        if inner.is_empty() {
            return Some(self.ctx.types.factory().tuple(Vec::new()));
        }

        let mut elements = Vec::new();
        for elem_str in Self::split_type_args_respecting_nesting(inner) {
            let mut elem = elem_str.trim();
            if elem.is_empty() {
                continue;
            }

            let mut rest = false;
            if let Some(stripped) = elem.strip_prefix("...") {
                rest = true;
                elem = stripped.trim();
            }

            let (name, optional, type_str) = if let Some(colon_idx) =
                Self::find_top_level_char(elem, ':')
            {
                let raw_name = elem[..colon_idx].trim();
                let type_str = elem[colon_idx + 1..].trim();
                let (raw_name, optional) = if let Some(stripped) = raw_name.strip_suffix('?') {
                    (stripped.trim(), true)
                } else {
                    (raw_name, false)
                };
                let name = (!raw_name.is_empty()).then(|| self.ctx.types.intern_string(raw_name));
                (name, optional, type_str)
            } else if !rest && elem.ends_with('?') {
                (None, true, elem[..elem.len() - 1].trim())
            } else {
                (None, false, elem)
            };

            let type_id = self.resolve_jsdoc_type_str(type_str)?;
            elements.push(TupleElement {
                type_id,
                name,
                optional,
                rest,
            });
        }

        Some(self.ctx.types.factory().tuple(elements))
    }

    pub(in crate::jsdoc::resolution) fn parse_jsdoc_index_access_segments(
        type_expr: &str,
    ) -> Option<(&str, &str)> {
        let mut bracket_depth = 0u32;
        let mut open_idx = None;

        for (idx, ch) in type_expr.char_indices() {
            match ch {
                '[' => {
                    if bracket_depth == 0 {
                        open_idx = Some(idx);
                    }
                    bracket_depth += 1;
                }
                ']' => {
                    if bracket_depth == 0 {
                        return None;
                    }
                    bracket_depth -= 1;
                    if bracket_depth == 0 {
                        let open_idx = open_idx?;
                        if idx + ch.len_utf8() != type_expr.len() {
                            return None;
                        }
                        let base = type_expr[..open_idx].trim();
                        let index = type_expr[open_idx + 1..idx].trim();
                        if base.is_empty() || index.is_empty() {
                            return None;
                        }
                        return Some((base, index));
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Parse an inline object literal type: `{ propName: Type, ... }`.
    pub(in crate::jsdoc::resolution) fn parse_jsdoc_object_literal_type(
        &mut self,
        type_expr: &str,
    ) -> Option<TypeId> {
        if let Some(mapped) = self.parse_jsdoc_mapped_type(type_expr) {
            return Some(mapped);
        }

        let factory = self.ctx.types.factory();
        let inner = type_expr[1..type_expr.len() - 1].trim();
        if inner.is_empty() {
            return Some(factory.object(Vec::new()));
        }
        // Split properties by ',' or ';' at top level
        let prop_strs = Self::split_object_properties(inner);
        let mut properties = Vec::new();
        let mut object_shape = ObjectShape::default();
        for prop_str in &prop_strs {
            let prop_str = prop_str.trim();
            if prop_str.is_empty() {
                continue;
            }
            if let Some(paren_idx) = Self::find_top_level_char(prop_str, '(') {
                let colon_idx = Self::find_top_level_char(prop_str, ':');
                if colon_idx.is_none_or(|idx| paren_idx < idx) {
                    if paren_idx == 0 {
                        if let Some(func_ty) = self.parse_jsdoc_call_signature(prop_str) {
                            return Some(func_ty);
                        }
                    } else if let Some(prop) =
                        self.parse_jsdoc_method_signature(prop_str, paren_idx, &properties)
                    {
                        properties.push(prop);
                        continue;
                    }
                }
            }
            if let Some(colon_idx) = Self::find_top_level_char(prop_str, ':') {
                let mut raw_name = prop_str[..colon_idx].trim();
                let type_str = prop_str[colon_idx + 1..].trim();
                let readonly = if let Some(rest) = raw_name.strip_prefix("readonly ") {
                    raw_name = rest.trim();
                    true
                } else {
                    false
                };
                if raw_name.starts_with('[')
                    && raw_name.ends_with(']')
                    && let Some(mut index_sig) =
                        self.parse_jsdoc_object_literal_index_signature(raw_name, type_str)
                {
                    index_sig.readonly |= readonly;
                    match index_sig.key_type {
                        TypeId::STRING => object_shape.string_index = Some(index_sig),
                        TypeId::NUMBER => object_shape.number_index = Some(index_sig),
                        _ => {}
                    }
                    continue;
                }
                let (name, optional) = if let Some(stripped) = raw_name.strip_suffix('?') {
                    (stripped, true)
                } else {
                    (raw_name, false)
                };
                if !name.is_empty() {
                    let prop_type = self.resolve_jsdoc_type_str(type_str).unwrap_or(TypeId::ANY);
                    let name_atom = self.ctx.types.intern_string(name);
                    properties.push(PropertyInfo {
                        name: name_atom,
                        type_id: prop_type,
                        write_type: prop_type,
                        optional,
                        readonly,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: (properties.len() + 1) as u32,
                        is_string_named: false,
                        single_quoted_name: false,
                    });
                }
            }
        }
        if properties.is_empty()
            && object_shape.string_index.is_none()
            && object_shape.number_index.is_none()
        {
            return None;
        }
        if object_shape.string_index.is_some() || object_shape.number_index.is_some() {
            object_shape.properties = properties;
            Some(factory.object_with_index(object_shape))
        } else {
            Some(factory.object(properties))
        }
    }

    fn parse_jsdoc_object_literal_index_signature(
        &mut self,
        raw_name: &str,
        type_str: &str,
    ) -> Option<IndexSignature> {
        let (raw_name, readonly) = if let Some(rest) = raw_name.trim().strip_prefix("readonly ") {
            (rest.trim(), true)
        } else {
            (raw_name.trim(), false)
        };
        let inner = raw_name.strip_prefix('[')?.strip_suffix(']')?.trim();
        let colon_idx = Self::find_top_level_char(inner, ':')?;
        let param_name = inner[..colon_idx].trim();
        let key_type = self.resolve_jsdoc_type_str(inner[colon_idx + 1..].trim())?;
        if key_type != TypeId::STRING && key_type != TypeId::NUMBER {
            return None;
        }

        let value_type = self.resolve_jsdoc_type_str(type_str)?;
        Some(IndexSignature {
            key_type,
            value_type,
            readonly,
            param_name: (!param_name.is_empty()).then(|| self.ctx.types.intern_string(param_name)),
        })
    }

    fn parse_jsdoc_mapped_type(&mut self, type_expr: &str) -> Option<TypeId> {
        let inner = type_expr[1..type_expr.len() - 1].trim();
        if !inner.starts_with('[') {
            return None;
        }

        let mut square_depth = 0u32;
        let mut close_bracket = None;
        for (idx, ch) in inner.char_indices() {
            match ch {
                '[' => square_depth += 1,
                ']' => {
                    square_depth = square_depth.saturating_sub(1);
                    if square_depth == 0 {
                        close_bracket = Some(idx);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_bracket = close_bracket?;
        let header = inner[1..close_bracket].trim();
        let mut after_bracket = inner[close_bracket + 1..].trim();
        let optional_modifier = if let Some(rest) = after_bracket.strip_prefix('?') {
            after_bracket = rest.trim();
            Some(tsz_solver::MappedModifier::Add)
        } else if let Some(rest) = after_bracket.strip_prefix("-?") {
            after_bracket = rest.trim();
            Some(tsz_solver::MappedModifier::Remove)
        } else {
            None
        };
        let template_str = after_bracket.strip_prefix(':')?.trim();

        let in_idx = header.find(" in ")?;
        let type_param_name = header[..in_idx].trim();
        let constraint_str = header[in_idx + 4..].trim();
        if type_param_name.is_empty() || constraint_str.is_empty() || template_str.is_empty() {
            return None;
        }

        let constraint = if let Some(rest) = constraint_str.strip_prefix("keyof") {
            let operand = self.resolve_jsdoc_type_str(rest.trim())?;
            self.ctx.types.factory().keyof(operand)
        } else {
            self.resolve_jsdoc_type_str(constraint_str)?
        };
        let atom = self.ctx.types.intern_string(type_param_name);
        let type_param = tsz_solver::TypeParamInfo {
            name: atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
        };
        let type_param_id = self.ctx.types.factory().type_param(type_param);
        let previous = self
            .ctx
            .type_parameter_scope
            .insert(type_param_name.to_string(), type_param_id);
        let template = self
            .resolve_jsdoc_type_str(template_str)
            .or(Some(TypeId::ANY));
        if let Some(previous) = previous {
            self.ctx
                .type_parameter_scope
                .insert(type_param_name.to_string(), previous);
        } else {
            self.ctx.type_parameter_scope.remove(type_param_name);
        }

        template.map(|template| {
            self.ctx.types.factory().mapped(tsz_solver::MappedType {
                type_param,
                constraint,
                name_type: None,
                template,
                readonly_modifier: None,
                optional_modifier,
            })
        })
    }

    pub(in crate::jsdoc::resolution) fn parse_jsdoc_conditional_type(
        &mut self,
        type_expr: &str,
    ) -> Option<TypeId> {
        let (extends_idx, question_idx, colon_idx) =
            Self::find_jsdoc_conditional_separators(type_expr)?;
        let check_type = self.resolve_jsdoc_type_str(type_expr[..extends_idx].trim())?;
        let extends_start = extends_idx + " extends ".len();
        let extends_type =
            self.resolve_jsdoc_type_str(type_expr[extends_start..question_idx].trim())?;
        let true_type =
            self.resolve_jsdoc_type_str(type_expr[question_idx + 1..colon_idx].trim())?;
        let false_type = self.resolve_jsdoc_type_str(type_expr[colon_idx + 1..].trim())?;
        Some(
            self.ctx
                .types
                .factory()
                .conditional(tsz_solver::ConditionalType {
                    check_type,
                    extends_type,
                    true_type,
                    false_type,
                    is_distributive: true,
                }),
        )
    }

    fn find_jsdoc_conditional_separators(type_expr: &str) -> Option<(usize, usize, usize)> {
        let extends_idx = Self::find_top_level_keyword(type_expr, " extends ")?;
        let question_idx = Self::find_top_level_char(&type_expr[extends_idx..], '?')? + extends_idx;
        let colon_idx =
            Self::find_top_level_char(&type_expr[question_idx + 1..], ':')? + question_idx + 1;
        Some((extends_idx, question_idx, colon_idx))
    }

    fn find_top_level_keyword(s: &str, keyword: &str) -> Option<usize> {
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            if ch == '\'' && !in_double_quote {
                in_single_quote = !in_single_quote;
                continue;
            }
            if ch == '"' && !in_single_quote {
                in_double_quote = !in_double_quote;
                continue;
            }
            if in_single_quote || in_double_quote {
                continue;
            }
            if angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && square_depth == 0
                && s[i..].starts_with(keyword)
            {
                return Some(i);
            }
            match ch {
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                _ => {}
            }
        }
        None
    }

    /// Parse a named method signature from a JSDoc object property string.
    /// Parse a call signature `(params): RetType` and return a function TypeId.
    fn parse_jsdoc_call_signature(&mut self, prop_str: &str) -> Option<TypeId> {
        use tsz_solver::{FunctionShape, ParamInfo};
        let after_open = &prop_str[1..];
        let mut depth = 1u32;
        let mut close_idx = None;
        for (i, ch) in after_open.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_idx = close_idx?;
        let params_inner = after_open[..close_idx].trim();
        let after_close = after_open[close_idx + 1..].trim();
        let return_type = if let Some(rest) = after_close.strip_prefix(':') {
            self.jsdoc_type_from_expression(rest.trim())
                .unwrap_or(TypeId::VOID)
        } else {
            TypeId::VOID
        };
        let mut params = Vec::new();
        if !params_inner.is_empty() {
            for p in Self::split_top_level_params(params_inner) {
                let p = p.trim();
                if p.is_empty() {
                    continue;
                }
                let (name, t_str) = if let Some(colon) = p.find(':') {
                    (Some(p[..colon].trim()), p[colon + 1..].trim())
                } else {
                    (None, p)
                };
                let p_type = self
                    .jsdoc_type_from_expression(t_str)
                    .unwrap_or(TypeId::ANY);
                let atom = name.map(|n| self.ctx.types.intern_string(n));
                params.push(ParamInfo {
                    name: atom,
                    type_id: p_type,
                    optional: false,
                    rest: false,
                });
            }
        }
        let shape = FunctionShape {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        Some(self.ctx.types.factory().function(shape))
    }
    fn parse_jsdoc_method_signature(
        &mut self,
        prop_str: &str,
        paren_idx: usize,
        existing_props: &[PropertyInfo],
    ) -> Option<PropertyInfo> {
        use tsz_solver::{FunctionShape, ParamInfo};
        let method_name = prop_str[..paren_idx].trim();
        if method_name.is_empty() {
            return None;
        }
        // Handle optional method: `name?(...)`
        let (method_name, optional) = if let Some(stripped) = method_name.strip_suffix('?') {
            (stripped.trim(), true)
        } else {
            (method_name, false)
        };
        // Find the matching close paren
        let after_open = &prop_str[paren_idx + 1..];
        let mut depth = 1u32;
        let mut close_idx = None;
        for (i, ch) in after_open.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_idx = close_idx?;
        let params_inner = after_open[..close_idx].trim();
        let after_close = after_open[close_idx + 1..].trim();
        // Return type follows ':'
        let return_type = if let Some(rest) = after_close.strip_prefix(':') {
            let return_type_str = rest.trim();
            self.jsdoc_type_from_expression(return_type_str)
                .unwrap_or(TypeId::VOID)
        } else {
            TypeId::VOID
        };
        // Parse parameters
        let mut params = Vec::new();
        if !params_inner.is_empty() {
            for p in Self::split_top_level_params(params_inner) {
                let p = p.trim();
                if p.is_empty() {
                    continue;
                }
                let (name, t_str) = if let Some(colon) = p.find(':') {
                    (Some(p[..colon].trim()), p[colon + 1..].trim())
                } else {
                    (None, p)
                };
                let p_type = self
                    .jsdoc_type_from_expression(t_str)
                    .unwrap_or(TypeId::ANY);
                let atom = name.map(|n| self.ctx.types.intern_string(n));
                params.push(ParamInfo {
                    name: atom,
                    type_id: p_type,
                    optional: false,
                    rest: false,
                });
            }
        }
        let shape = FunctionShape {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        };
        let method_type = self.ctx.types.factory().function(shape);
        let name_atom = self.ctx.types.intern_string(method_name);
        Some(PropertyInfo {
            name: name_atom,
            type_id: method_type,
            write_type: method_type,
            optional,
            readonly: false,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: (existing_props.len() + 1) as u32,
            is_string_named: false,
            single_quoted_name: false,
        })
    }
    /// Resolve a `@typedef` referenced by name from JSDoc comments.
    ///
    /// In tsc, `@typedef`/`@callback` declarations are hoisted to file scope,
    /// so forward references (usage before definition) are valid.  We scan all
    /// comments in the file regardless of position, matching tsc's behavior.
    pub(crate) fn resolve_jsdoc_typedef_type(
        &mut self,
        type_expr: &str,
        _anchor_pos: u32,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<TypeId> {
        self.resolve_jsdoc_typedef_info(type_expr, comments, source_text)
            .map(|(body_type, _)| body_type)
            .or(Some(TypeId::ANY))
    }

    pub(crate) fn resolve_jsdoc_typedef_info(
        &mut self,
        type_expr: &str,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        // Re-entrancy guard: recursive @typedef like `@typedef {... | Json[]} Json`
        // causes type_from_jsdoc_typedef → jsdoc_type_from_expression →
        // resolve_jsdoc_type_name → resolve_jsdoc_typedef_type infinite loop.
        // If we're already resolving this typedef, return None so the caller
        // falls through to the file_locals symbol lookup which returns a Lazy
        // placeholder that properly defers the recursive reference.
        if self
            .ctx
            .jsdoc_typedef_resolving
            .borrow()
            .contains(type_expr)
        {
            return None;
        }

        let anchor_pos = self.ctx.jsdoc_typedef_anchor_pos.get();

        // Pre-compute the brace depth of the anchor position from file start.
        let anchor_depth = if anchor_pos != u32::MAX && (anchor_pos as usize) <= source_text.len() {
            let mut d: i32 = 0;
            for ch in source_text[..anchor_pos as usize].bytes() {
                match ch {
                    b'{' => d += 1,
                    b'}' => d -= 1,
                    _ => {}
                }
            }
            Some(d)
        } else {
            None
        };

        // Two-pass approach for typedef scoping:
        // Pass 1: Collect same-scope matches and count deeper-scope matches.
        // Pass 2: If no same-scope match, use a deeper-scope match only if
        // there's exactly one (unambiguous). Multiple deeper-scope typedefs
        // with the same name are ambiguous → return None so the name falls
        // through to other resolution paths (matching TSC behavior where
        // function-scoped typedefs with duplicate names are not visible
        // at the module level).
        let mut same_scope_def: Option<JsdocTypedefInfo> = None;
        let mut deeper_defs: Vec<JsdocTypedefInfo> = Vec::new();

        for comment in comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let (is_same_scope, is_deeper) = if let Some(a_depth) = anchor_depth {
                let comment_pos = comment.pos as usize;
                if comment_pos <= source_text.len() {
                    let mut c_depth: i32 = 0;
                    for ch in source_text[..comment_pos].bytes() {
                        match ch {
                            b'{' => c_depth += 1,
                            b'}' => c_depth -= 1,
                            _ => {}
                        }
                    }
                    if c_depth > a_depth {
                        (false, true) // deeper scope
                    } else if c_depth == a_depth {
                        // Same depth: check if in same contiguous scope
                        let (lo, hi) = if comment_pos < anchor_pos as usize {
                            (comment_pos, anchor_pos as usize)
                        } else {
                            (anchor_pos as usize, comment_pos)
                        };
                        let slice = &source_text[lo..hi];
                        let mut depth: i32 = 0;
                        let mut crossed = false;
                        for ch in slice.bytes() {
                            match ch {
                                b'{' => depth += 1,
                                b'}' => {
                                    depth -= 1;
                                    if depth < 0 {
                                        crossed = true;
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                        if !crossed && depth == 0 {
                            (true, false) // same scope
                        } else {
                            (false, false) // same depth but different scope (sibling functions)
                        }
                    } else {
                        (false, false) // shallower scope — visible
                    }
                } else {
                    (false, false)
                }
            } else {
                (true, false)
            };

            let content = get_jsdoc_content(comment, source_text);
            for (name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if name != type_expr {
                    continue;
                }
                if is_same_scope {
                    same_scope_def = Some(typedef_info);
                } else if is_deeper {
                    deeper_defs.push(typedef_info);
                } else {
                    // Shallower or same-depth-different-scope: use as fallback
                    // (last one wins, matching original behavior)
                    if same_scope_def.is_none() {
                        same_scope_def = Some(typedef_info);
                    }
                }
            }
        }

        // Prefer same-scope match. Fall back to deeper-scope only if unambiguous.
        let best_def = if same_scope_def.is_some() {
            same_scope_def
        } else if deeper_defs.len() == 1 {
            deeper_defs.into_iter().next()
        } else {
            // Multiple deeper-scope defs → ambiguous, or no defs at all
            None
        };
        let typedef_info = best_def?;

        // Mark this typedef as being resolved to prevent re-entrancy.
        self.ctx
            .jsdoc_typedef_resolving
            .borrow_mut()
            .insert(type_expr.to_owned());

        let result = self.type_from_jsdoc_typedef(typedef_info);

        self.ctx
            .jsdoc_typedef_resolving
            .borrow_mut()
            .remove(type_expr);

        if let Some((ty, _)) = result.as_ref() {
            self.register_jsdoc_typedef_def(type_expr, *ty);
        }
        result
    }
    fn type_from_jsdoc_typedef(
        &mut self,
        info: JsdocTypedefInfo,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let factory = self.ctx.types.factory();
        let mut type_param_infos = Vec::with_capacity(info.template_params.len());
        let mut scope_updates = Vec::with_capacity(info.template_params.len());
        for template in &info.template_params {
            let constraint = template
                .constraint
                .as_deref()
                .and_then(|expr| self.resolve_jsdoc_type_str(expr));
            let atom = self.ctx.types.intern_string(&template.name);
            let param = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const: false,
            };
            let type_id = factory.type_param(param);
            let previous = self
                .ctx
                .type_parameter_scope
                .insert(template.name.clone(), type_id);
            type_param_infos.push(param);
            scope_updates.push((template.name.clone(), previous));
        }

        let result = if let Some(cb) = info.callback {
            self.type_from_jsdoc_callback(cb, &type_param_infos)
        } else {
            self.type_from_jsdoc_object_typedef(info)
        };

        for (name, previous) in scope_updates.into_iter().rev() {
            if let Some(previous) = previous {
                self.ctx.type_parameter_scope.insert(name, previous);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        result.map(|type_id| (type_id, type_param_infos))
    }

    fn type_from_jsdoc_callback(
        &mut self,
        cb: JsdocCallbackInfo,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> Option<TypeId> {
        let factory = self.ctx.types.factory();
        let mut params = Vec::new();
        let mut this_type = None;
        let nested_entries: Vec<(String, String, bool)> = cb
            .params
            .iter()
            .filter_map(|param| {
                (param.name.contains('.') || param.name.contains("[]")).then_some((
                    param.name.clone(),
                    param.type_expr.clone().unwrap_or_else(|| "any".to_string()),
                    param.optional,
                ))
            })
            .collect();

        for param in &cb.params {
            if param.name.contains('.') || param.name.contains("[]") {
                continue;
            }

            let raw_type_expr = param.type_expr.clone().unwrap_or_else(|| "any".to_string());
            let effective_expr = raw_type_expr.trim_end_matches('=').trim();
            let effective_expr = if param.rest {
                effective_expr.trim_start_matches("...").trim()
            } else {
                effective_expr
            };

            let is_object_base = effective_expr == "Object" || effective_expr == "object";
            let is_array_object_base = effective_expr == "Object[]"
                || effective_expr == "object[]"
                || effective_expr == "Array.<Object>"
                || effective_expr == "Array.<object>"
                || effective_expr == "Array<Object>"
                || effective_expr == "Array<object>";

            let mut type_id =
                if (is_object_base || is_array_object_base) && !nested_entries.is_empty() {
                    self.build_nested_param_object_type_from_entries(
                        &nested_entries,
                        &param.name,
                        is_array_object_base,
                    )
                    .or_else(|| self.jsdoc_type_from_expression(effective_expr))
                    .unwrap_or(TypeId::ANY)
                } else {
                    self.jsdoc_type_from_expression(effective_expr)
                        .unwrap_or(TypeId::ANY)
                };

            if param.rest {
                type_id = factory.array(type_id);
            }

            if param.name == "this" {
                this_type = Some(type_id);
                continue;
            }

            let name_atom = self.ctx.types.intern_string(&param.name);
            params.push(ParamInfo {
                name: Some(name_atom),
                type_id,
                optional: param.optional,
                rest: param.rest,
            });
        }

        let mut type_predicate = None;
        let return_type = if let Some((is_asserts, param_name, type_str)) = cb.predicate {
            let pred_type = type_str
                .as_deref()
                .and_then(|s| self.jsdoc_type_from_expression(s));
            let target = if param_name == "this" {
                TypePredicateTarget::This
            } else {
                let atom = self.ctx.types.intern_string(&param_name);
                TypePredicateTarget::Identifier(atom)
            };
            let parameter_index = if param_name != "this" {
                params.iter().position(|param| {
                    param
                        .name
                        .is_some_and(|name| name == self.ctx.types.intern_string(&param_name))
                })
            } else {
                None
            };
            type_predicate = Some(TypePredicate {
                asserts: is_asserts,
                target,
                type_id: pred_type,
                parameter_index,
            });
            if is_asserts {
                TypeId::VOID
            } else {
                TypeId::BOOLEAN
            }
        } else if let Some(ref ret_expr) = cb.return_type {
            self.jsdoc_type_from_expression(ret_expr)
                .unwrap_or(TypeId::ANY)
        } else {
            TypeId::VOID
        };

        Some(factory.function(FunctionShape {
            type_params: type_params.to_vec(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        }))
    }

    fn type_from_jsdoc_object_typedef(&mut self, info: JsdocTypedefInfo) -> Option<TypeId> {
        let factory = self.ctx.types.factory();
        let base_type = if let Some(base_type_expr) = &info.base_type {
            let expr = base_type_expr.trim();
            if expr != "Object" && expr != "object" {
                return self.resolve_jsdoc_type_str(expr);
            }
            None
        } else {
            None
        };
        let mut top_level = Vec::new();
        let mut nested_entries = Vec::new();
        for prop in info.properties {
            if prop.name.contains('.') {
                nested_entries.push((prop.name, prop.type_expr, prop.optional));
            } else {
                top_level.push(prop);
            }
        }
        let mut prop_infos = Vec::with_capacity(top_level.len());
        for prop in top_level {
            let mut prop_type = if prop.type_expr.trim().is_empty() {
                TypeId::ANY
            } else {
                self.jsdoc_type_from_expression(&prop.type_expr)
                    .unwrap_or(TypeId::ANY)
            };
            let effective_expr = prop.type_expr.trim_end_matches('=').trim();
            let is_array_object_base = effective_expr == "Object[]"
                || effective_expr == "object[]"
                || effective_expr == "Array.<Object>"
                || effective_expr == "Array.<object>"
                || effective_expr == "Array<Object>"
                || effective_expr == "Array<object>";
            if let Some(built) = self.build_nested_param_object_type_from_entries(
                &nested_entries,
                &prop.name,
                is_array_object_base,
            ) {
                prop_type = built;
            }
            if prop.optional
                && self.ctx.strict_null_checks()
                && !self.ctx.exact_optional_property_types()
                && prop_type != TypeId::ANY
                && prop_type != TypeId::UNDEFINED
            {
                prop_type = factory.union2(prop_type, TypeId::UNDEFINED);
            }
            let name_atom = self.ctx.types.intern_string(&prop.name);
            prop_infos.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: prop.optional,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                single_quoted_name: false,
            });
        }
        let object_type = if !prop_infos.is_empty() {
            Some(factory.object(prop_infos))
        } else {
            None
        };
        match (object_type, base_type) {
            (Some(obj), Some(base)) => Some(factory.intersection2(obj, base)),
            (Some(obj), None) => Some(obj),
            (None, Some(base)) => Some(base),
            (None, None) => None,
        }
    }

    fn resolve_jsdoc_generic_typedef_type(
        &mut self,
        base_name: &str,
        type_args: &[TypeId],
    ) -> Option<TypeId> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let source_file = self.ctx.arena.source_files.first()?;
        let mut best_def = None;
        for comment in &source_file.comments {
            if !is_jsdoc_comment(comment, &source_file.text) {
                continue;
            }
            let content = get_jsdoc_content(comment, &source_file.text);
            for (name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if name == base_name {
                    best_def = Some(typedef_info);
                }
            }
        }

        let (body_type, type_params) = self.type_from_jsdoc_typedef(best_def?)?;
        if type_args.is_empty() {
            return Some(body_type);
        }
        if type_params.is_empty() {
            return None;
        }

        use crate::query_boundaries::common::instantiate_generic;
        let instantiated = instantiate_generic(self.ctx.types, body_type, &type_params, type_args);
        self.register_jsdoc_generic_display_name(base_name, type_args, instantiated);
        Some(instantiated)
    }
    // NOTE: jsdoc_has_readonly_tag, jsdoc_access_level, find_orphaned_extends_tags_for_statements,
    // is_in_different_function_scope, find_function_body_end are in lookup.rs
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::TypeInterner;

    #[test]
    fn resolve_jsdoc_assigned_value_type_sees_prototype_property_statement() {
        let source = r#"
function C() { this.x = false; };
/** @type {number} */
C.prototype.x;
new C().x;
"#;
        let options = crate::context::CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..crate::context::CheckerOptions::default()
        };
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.js".to_string(),
            options,
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);
        assert_eq!(
            checker
                .resolve_jsdoc_assigned_value_type("C.prototype.x")
                .map(|ty| checker.format_type(ty)),
            Some("number".to_string())
        );
    }
}
