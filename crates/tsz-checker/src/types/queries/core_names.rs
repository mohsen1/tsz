//! Name, computed-property, and display-name query helpers for `CheckerState`.

use super::core::get_literal_property_name;
use crate::state::CheckerState;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{SymbolRef, TypeId};

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Section 30: Name Extraction Utilities
    // =========================================================================

    /// Check if a computed property name resolves to a string literal type
    /// (e.g. `[hundredStr]` where `const hundredStr = "100"`).
    pub(crate) fn is_computed_string_property_name(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(computed.expression) else {
            return false;
        };
        if self.ctx.arena.get_identifier(expr_node).is_none() {
            return false;
        }
        let expr_type = self.get_type_of_node(computed.expression);
        crate::query_boundaries::checkers::iterable::is_string_literal_type(
            self.ctx.types,
            expr_type,
        )
    }

    /// Get property name as string from a property name node (identifier, string literal, etc.)
    ///
    /// Also handles computed property names with literal or symbol expressions.
    pub(crate) fn get_property_name(&self, name_idx: NodeIndex) -> Option<String> {
        // Try non-computed property name first
        if let Some(name) = get_literal_property_name(self.ctx.arena, name_idx) {
            return Some(name);
        }

        // Handle computed property names
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
        {
            if let Some(symbol_name) = self.get_symbol_property_name_from_expr(computed.expression)
            {
                return Some(symbol_name);
            }
            // Skip identifiers in computed expressions — they are variable references
            // (e.g. `[an]` where `const an = 0`), not literal property names. Callers
            // that need type-based resolution (e.g. object literal type computation)
            // should fall back to evaluating the expression's type.
            let expr_node = self.ctx.arena.get(computed.expression)?;
            if self.ctx.arena.get_identifier(expr_node).is_some() {
                return None;
            }
            return get_literal_property_name(self.ctx.arena, computed.expression);
        }

        None
    }

    /// Like `get_property_name` but additionally resolves computed property names
    /// by evaluating the expression's type when the syntax alone cannot determine
    /// the name. This handles cases like `const k = 'foo' as const; class C { [k]() {} }`
    /// and `const k = 'foo'; class C { [k]() {} }` (tsc infers the literal type from
    /// the const initializer).
    pub(crate) fn get_property_name_resolved(&mut self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        // For computed property names with identifier expressions (e.g., `[k]` where
        // `const k = 'foo'`), skip `get_property_name` which would incorrectly return
        // the identifier text ("k") instead of the resolved value ("foo").
        // Instead, evaluate the expression type to resolve the actual property name.
        let is_computed_identifier = name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && self
                .ctx
                .arena
                .get_computed_property(name_node)
                .and_then(|computed| self.ctx.arena.get(computed.expression))
                .is_some_and(|expr_node| self.ctx.arena.get_identifier(expr_node).is_some());

        if !is_computed_identifier && let Some(name) = self.get_property_name(name_idx) {
            // When the syntactic resolver returns a `[Symbol.xxx]` name but the
            // property access expression resolves to ERROR (e.g. `Symbol.nonsense`
            // where `nonsense` doesn't exist on SymbolConstructor), discard the
            // name. This prevents creating a phantom named property in the object
            // type, which would cause false TS2322 errors on assignment.
            if name.starts_with("[Symbol.")
                && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
                && self.get_type_of_node(computed.expression) == TypeId::ERROR
            {
                return None;
            }
            if name.starts_with("[Symbol.")
                && let Some(symbol_ref) = self.well_known_symbol_ref_for_name(&name, name_idx)
            {
                self.register_well_known_symbol_name_mapping(&name, symbol_ref);
            }
            return Some(name);
        }

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            if let Some(sym_ref) =
                self.computed_identifier_unique_symbol_property_ref(computed.expression)
            {
                return Some(format!("__unique_{}", sym_ref.0));
            }
            // Preserve literal types so that `const k = 'foo'` (no `as const`)
            // still resolves to the literal `"foo"` rather than widening to `string`.
            let prev = self.ctx.preserve_literal_types;
            self.ctx.preserve_literal_types = true;
            // Set checking_computed_property_name to suppress TS1212 emission from
            // get_type_of_identifier when resolving reserved words like `public`.
            // The proper TS1212/TS1213 diagnostic is emitted by check_computed_property_name.
            let prev_checking = self.ctx.checking_computed_property_name.take();
            self.ctx.checking_computed_property_name = Some(name_idx);
            let prop_name_type = self.get_type_of_node(computed.expression);
            self.ctx.checking_computed_property_name = prev_checking;
            self.ctx.preserve_literal_types = prev;

            let evaluated_prop_name_type = self.evaluate_type_with_env(prop_name_type);
            let resolved_for_property_access =
                self.resolve_type_for_property_access(evaluated_prop_name_type);
            let resolved_prop_name_type = self.resolve_lazy_type(resolved_for_property_access);
            let application_prop_name_type =
                self.evaluate_application_type(resolved_prop_name_type);
            let assignability_prop_name_type = self.evaluate_type_for_assignability(prop_name_type);

            if let Some(sym_id) = self.resolve_computed_unique_symbol_property(computed.expression)
            {
                return Some(format!("__unique_{}", sym_id.0));
            }

            // Fallback: when the computed expression is an identifier referencing a
            // variable initialized with or annotated as `Symbol.xxx`, resolve to the
            // canonical `[Symbol.xxx]` property name.  This handles patterns like:
            //   const observable: typeof Symbol.obs = Symbol.obs;
            //   class C { [observable]() { ... } }
            // where type-based resolution yields plain `symbol` instead of a unique symbol.
            if prop_name_type == TypeId::SYMBOL
                && let Some(well_known) =
                    self.resolve_computed_symbol_property_name(computed.expression)
            {
                if let Some(symbol_ref) = self.resolve_well_known_symbol_ref_from_name(&well_known)
                {
                    self.register_well_known_symbol_name_mapping(&well_known, symbol_ref);
                }
                return Some(well_known);
            }
            // Fallback for property access expressions like `[Symbol.iterator]` where the
            // computed expression is not a bare identifier (so `resolve_computed_symbol_property_name`
            // returns None) but the prop type widened to plain `symbol` — e.g. when `Symbol` is a
            // user-declared const with `{ readonly iterator: unique symbol }` type annotation.
            if prop_name_type == TypeId::SYMBOL
                && let Some(unique_member_name) =
                    self.declared_unique_symbol_member_property_name(computed.expression)
            {
                return Some(unique_member_name);
            }
            if let Some(symbol_name) =
                self.symbol_valued_binding_property_name(computed.expression, prop_name_type)
            {
                return Some(symbol_name);
            }
            // When the computed property type resolves to a unique symbol (e.g.
            // `typeof Symbol.obs`), map it to the canonical `[Symbol.xxx]` format
            // that type literals and interfaces use.  Without this, class members
            // like `[observable]()` (where `const observable = Symbol.obs`) would
            // be stored as `__unique_N` while the target type uses `[Symbol.obs]`,
            // causing false TS2345/TS2322 structural mismatches.
            for candidate in [
                prop_name_type,
                evaluated_prop_name_type,
                resolved_prop_name_type,
                application_prop_name_type,
                assignability_prop_name_type,
            ] {
                if let Some(sym_ref) =
                    crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, candidate)
                {
                    let sym_id = tsz_binder::SymbolId(sym_ref.0);
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        let sym_name = symbol.escaped_name.clone();
                        if symbol.parent.is_some()
                            && let Some(parent_sym) = self.ctx.binder.get_symbol(symbol.parent)
                            && parent_sym.escaped_name == "Symbol"
                        {
                            let well_known = format!("[Symbol.{sym_name}]");
                            self.register_well_known_symbol_name_mapping(&well_known, sym_ref);
                            return Some(well_known);
                        }
                    }
                    return Some(format!("__unique_{}", sym_ref.0));
                }
            }

            for candidate in [
                prop_name_type,
                evaluated_prop_name_type,
                resolved_prop_name_type,
                application_prop_name_type,
                assignability_prop_name_type,
            ] {
                if let Some(atom) =
                    crate::query_boundaries::type_computation::access::literal_property_name(
                        self.ctx.types,
                        candidate,
                    )
                {
                    tracing::trace!(
                        name_idx = name_idx.0,
                        expr_idx = computed.expression.0,
                        prop_name_type = prop_name_type.0,
                        prop_name_type_str = %self.format_type(prop_name_type),
                        evaluated_prop_name_type = evaluated_prop_name_type.0,
                        evaluated_prop_name_type_str = %self.format_type(evaluated_prop_name_type),
                        resolved_prop_name_type = resolved_prop_name_type.0,
                        resolved_prop_name_type_str = %self.format_type(resolved_prop_name_type),
                        application_prop_name_type = application_prop_name_type.0,
                        application_prop_name_type_str = %self.format_type(application_prop_name_type),
                        assignability_prop_name_type = assignability_prop_name_type.0,
                        assignability_prop_name_type_str = %self.format_type(assignability_prop_name_type),
                        chosen_candidate = candidate.0,
                        chosen_name = %self.ctx.types.resolve_atom(atom),
                        "get_property_name_resolved: computed property resolved"
                    );
                    return Some(self.ctx.types.resolve_atom(atom));
                }
            }
            // Fallback for computed keys whose type widens too far to recover a
            // literal property name (notably auto-increment numeric enum members
            // such as `E.B` where `B` has no explicit initializer). Reuse enum
            // constant-expression evaluation so duplicate checks and contextual
            // lookup still see a stable key like "0".
            if let Some(value) = self.evaluate_constant_expression(computed.expression) {
                let canonical = tsz_solver::utils::canonicalize_numeric_name(&format!("{value}"))
                    .unwrap_or_else(|| format!("{value}"));
                tracing::trace!(
                    name_idx = name_idx.0,
                    expr_idx = computed.expression.0,
                    prop_name_type = prop_name_type.0,
                    prop_name_type_str = %self.format_type(prop_name_type),
                    resolved_name = %canonical,
                    "get_property_name_resolved: computed property resolved via constant expression"
                );
                return Some(canonical);
            }
            tracing::trace!(
                name_idx = name_idx.0,
                expr_idx = computed.expression.0,
                prop_name_type = prop_name_type.0,
                prop_name_type_str = %self.format_type(prop_name_type),
                evaluated_prop_name_type = evaluated_prop_name_type.0,
                evaluated_prop_name_type_str = %self.format_type(evaluated_prop_name_type),
                resolved_prop_name_type = resolved_prop_name_type.0,
                resolved_prop_name_type_str = %self.format_type(resolved_prop_name_type),
                application_prop_name_type = application_prop_name_type.0,
                application_prop_name_type_str = %self.format_type(application_prop_name_type),
                assignability_prop_name_type = assignability_prop_name_type.0,
                assignability_prop_name_type_str = %self.format_type(assignability_prop_name_type),
                "get_property_name_resolved: computed property unresolved"
            );
            None
        } else {
            None
        }
    }

    pub(crate) fn computed_property_expression_name_atom(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<Atom> {
        if let Some(symbol_name) = self.get_symbol_property_name_from_expr(expr_idx) {
            return Some(self.ctx.types.intern_string(&symbol_name));
        }

        let sym_ref = self.computed_identifier_unique_symbol_property_ref(expr_idx)?;
        Some(
            self.ctx
                .types
                .intern_string(&format!("__unique_{}", sym_ref.0)),
        )
    }

    pub(crate) fn computed_property_expression_is_symbol_named(&self, expr_idx: NodeIndex) -> bool {
        self.get_symbol_property_name_from_expr(expr_idx).is_some()
            || self
                .declared_unique_symbol_member_property_name(expr_idx)
                .is_some()
            || self
                .computed_identifier_unique_symbol_property_ref(expr_idx)
                .is_some()
    }

    pub(crate) fn computed_property_expression_unique_symbol_type(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        let sym_ref = self.computed_identifier_unique_symbol_property_ref(expr_idx)?;
        Some(self.ctx.types.unique_symbol(sym_ref))
    }

    fn computed_identifier_unique_symbol_property_ref(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<SymbolRef> {
        let expr_node = self.ctx.arena.get(expr_idx)?;
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(expr_node)?;
            return self.computed_identifier_unique_symbol_property_ref(paren.expression);
        }

        let mut sym_id = if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
            self.ctx
                .binder
                .resolve_identifier(self.ctx.arena, expr_idx)
                .or_else(|| self.ctx.binder.file_locals.get(&ident.escaped_text))?
        } else {
            self.resolve_qualified_symbol(expr_idx)?
        };
        let mut hops = 0usize;
        while hops < 32 {
            hops += 1;
            let Some(next) = self.ctx.binder.resolve_import_symbol(sym_id) else {
                break;
            };
            if next == sym_id {
                break;
            }
            sym_id = next;
        }

        self.symbol_has_declared_unique_symbol_property_ref(sym_id)
            .then_some(SymbolRef(sym_id.0))
    }

    fn symbol_has_declared_unique_symbol_property_ref(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.get_symbol_from_any_binder(sym_id) else {
            return false;
        };
        let file_idx = symbol.decl_file_idx;
        let owner_binder = self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .unwrap_or(self.ctx.binder);

        symbol.all_declarations().into_iter().any(|decl_idx| {
            let mut candidate_arenas: Vec<&NodeArena> = Vec::new();
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
            }
            if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            if std::ptr::eq(owner_binder, self.ctx.binder) {
                candidate_arenas.push(self.ctx.arena);
            }

            candidate_arenas.into_iter().any(|arena| {
                self.declaration_has_declared_unique_symbol_property_ref(arena, decl_idx)
            })
        })
    }

    fn declaration_has_declared_unique_symbol_property_ref(
        &self,
        arena: &NodeArena,
        mut decl_idx: NodeIndex,
    ) -> bool {
        let Some(mut node) = arena.get(decl_idx) else {
            return false;
        };
        if node.kind == SyntaxKind::Identifier as u16 {
            let parent = arena.get_extended(decl_idx).map(|ext| ext.parent);
            if let Some(parent_node) = parent.and_then(|idx| arena.get(idx))
                && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            {
                decl_idx = parent.unwrap_or(NodeIndex::NONE);
                node = parent_node;
            } else if let Some(parent_node) = parent.and_then(|idx| arena.get(idx))
                && parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            {
                decl_idx = parent.unwrap_or(NodeIndex::NONE);
                node = parent_node;
            }
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(var_decl) = arena.get_variable_declaration(node) else {
                return false;
            };
            if !arena.is_const_variable_declaration(decl_idx) {
                return false;
            }
            return (var_decl.type_annotation.is_some()
                && crate::types_domain::unique_symbol_arena::is_unique_symbol_type_annotation_unwrapped(
                    arena,
                    var_decl.type_annotation,
                ))
                || self.is_global_symbol_factory_call_initializer(arena, var_decl.initializer);
        }

        if node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            let Some(prop) = arena.get_property_decl(node) else {
                return false;
            };
            return prop.type_annotation.is_some()
                && crate::types_domain::unique_symbol_arena::is_unique_symbol_type_annotation_unwrapped(
                    arena,
                    prop.type_annotation,
                )
                && crate::types_domain::unique_symbol_arena::has_declared_unique_symbol_owner(
                    arena,
                    prop.type_annotation,
                );
        }

        false
    }

    fn is_global_symbol_factory_call_initializer(
        &self,
        arena: &NodeArena,
        init_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(init_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = arena.get_call_expr(node) else {
            return false;
        };
        self.is_global_symbol_factory_callee(arena, call.expression)
    }

    fn is_global_symbol_factory_callee(&self, arena: &NodeArena, callee_idx: NodeIndex) -> bool {
        let Some(callee_node) = arena.get(callee_idx) else {
            return false;
        };
        if let Some(ident) = arena.get_identifier(callee_node) {
            if ident.escaped_text != "Symbol" || !std::ptr::eq(arena, self.ctx.arena) {
                return false;
            }
            return self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, callee_idx)
                .or_else(|| self.ctx.binder.file_locals.get("Symbol"))
                .or_else(|| {
                    self.ctx
                        .lib_contexts
                        .iter()
                        .find_map(|ctx| ctx.binder.file_locals.get("Symbol"))
                })
                .is_some_and(|sym_id| {
                    self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                        || self.ctx.symbol_is_from_lib(sym_id)
                });
        }

        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(name) = arena.get_identifier_text(access.name_or_argument) else {
            return false;
        };
        name == "for" && self.is_global_symbol_factory_callee(arena, access.expression)
    }

    pub(crate) fn is_symbol_property_name(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        if self
            .get_property_name_resolved(name_idx)
            .is_some_and(|name| name.starts_with("[Symbol."))
        {
            return true;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        if self
            .declared_unique_symbol_member_property_name(computed.expression)
            .is_some()
        {
            return true;
        }

        let prev_checking = self.ctx.checking_computed_property_name;
        self.ctx.checking_computed_property_name = Some(name_idx);
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let expr_type = self.get_type_of_node(computed.expression);
        self.ctx.preserve_literal_types = prev_preserve;
        self.ctx.checking_computed_property_name = prev_checking;

        crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, expr_type).is_some()
    }

    fn resolve_computed_unique_symbol_property(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.primary_declaration()?
        };
        let mut decl_idx = decl;
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            decl_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.ctx.arena.is_const_variable_declaration(decl_idx)
        {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let has_unique_annotation = var_decl.type_annotation.is_some()
            && self.is_unique_symbol_type_annotation(var_decl.type_annotation);
        let has_symbol_initializer = var_decl.initializer.is_some()
            && (self.is_symbol_call_initializer(var_decl.initializer)
                || self.is_symbol_for_call_initializer(var_decl.initializer));

        (has_unique_annotation || has_symbol_initializer).then_some(sym_id)
    }

    /// For an identifier expression, trace back to the variable's declaration
    /// and check if the initializer or type annotation references `Symbol.xxx`.
    /// If so, return the canonical `[Symbol.xxx]` property name.
    ///
    /// This handles computed property names like `[observable]` where
    /// `const observable: typeof Symbol.obs = Symbol.obs`.  The declared type
    /// resolves to plain `symbol`, but the structural property key must match
    /// the `[Symbol.obs]` format used by type literals.
    fn resolve_computed_symbol_property_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(expr_node)?;

        // Look up the identifier in the binder to find its declaration
        let sym_id = self.ctx.binder.file_locals.get(&ident.escaped_text)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl = symbol.value_declaration;
        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;

        // Check initializer first: `= Symbol.obs`
        if var_decl.initializer.is_some()
            && let Some(name) = self.get_symbol_property_name_from_expr(var_decl.initializer)
        {
            return Some(name);
        }

        // Check type annotation: `typeof Symbol.obs`
        // The type annotation is a TYPE_QUERY node whose expr_name is `Symbol.obs`.
        if var_decl.type_annotation.is_some() {
            let ann_node = self.ctx.arena.get(var_decl.type_annotation)?;
            if ann_node.kind == syntax_kind_ext::TYPE_QUERY {
                let type_query = self.ctx.arena.get_type_query(ann_node)?;
                if let Some(name) = self.get_symbol_property_name_from_expr(type_query.expr_name) {
                    return Some(name);
                }
            }
        }

        None
    }

    pub(crate) fn declared_unique_symbol_member_property_name(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let (base_expr, member_name) = self.property_access_identifier_parts(expr_idx)?;
        let local_sym_id = self.resolve_identifier_symbol_without_tracking(base_expr)?;
        let sym_id = self
            .ctx
            .resolve_import_alias_and_register(local_sym_id)
            .unwrap_or(local_sym_id);
        let file_idx = self.ctx.resolve_symbol_file_index(sym_id).or_else(|| {
            self.get_cross_file_symbol(sym_id)
                .map(|symbol| symbol.decl_file_idx as usize)
        })?;
        let symbol = self
            .ctx
            .get_binder_for_file(file_idx)
            .and_then(|binder| binder.get_symbol(sym_id))
            .or_else(|| self.get_cross_file_symbol(sym_id))?;
        let value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return None;
        }
        let decl_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let decl_node = decl_arena.get(value_decl)?;
        let var_decl = decl_arena.get_variable_declaration(decl_node)?;
        let type_annotation = var_decl.type_annotation;
        let type_node = crate::types_domain::unique_symbol_arena::unwrap_parenthesized_type(
            decl_arena,
            type_annotation,
        );
        let type_node = decl_arena.get(type_node)?;
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let type_lit = decl_arena.get_type_literal(type_node)?;
        let has_unique_member = type_lit.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = decl_arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                return false;
            }
            let Some(sig) = decl_arena.get_signature(member_node) else {
                return false;
            };
            get_literal_property_name(decl_arena, sig.name).is_some_and(|name| {
                name == member_name
                    && sig.type_annotation.is_some()
                    && crate::types_domain::unique_symbol_arena::is_unique_symbol_type_annotation_unwrapped(
                        decl_arena,
                        sig.type_annotation,
                    )
            })
        });
        has_unique_member.then(|| {
            let base_name = self
                .ctx
                .arena
                .get_identifier_at(base_expr)
                .map(|ident| ident.escaped_text.as_str())
                .unwrap_or("Symbol");
            format!("[{base_name}.{member_name}]")
        })
    }

    fn property_access_identifier_parts(
        &self,
        mut expr_idx: NodeIndex,
    ) -> Option<(NodeIndex, String)> {
        while let Some(node) = self.ctx.arena.get(expr_idx)
            && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
        {
            expr_idx = self.ctx.arena.get_parenthesized(node)?.expression;
        }
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        let base_node = self.ctx.arena.get(access.expression)?;
        self.ctx.arena.get_identifier(base_node)?;
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some((access.expression, ident.escaped_text.clone()));
        }
        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some((access.expression, lit.text.clone()));
        }
        None
    }

    fn register_well_known_symbol_name_mapping(
        &self,
        name: &str,
        symbol_ref: tsz_solver::SymbolRef,
    ) {
        if !name.starts_with("[Symbol.") {
            return;
        }
        let name_key = name.to_string();

        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            env.register_well_known_symbol_name(name_key.clone(), symbol_ref);
        }
        if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
            env.register_well_known_symbol_name(name_key, symbol_ref);
        }
    }

    pub(crate) fn register_well_known_symbol_name_from_canonical(
        &self,
        name: &str,
        fallback_symbol_ref: Option<tsz_solver::SymbolRef>,
    ) -> bool {
        let Some(symbol_ref) = self
            .resolve_well_known_symbol_ref_from_name(name)
            .or(fallback_symbol_ref)
        else {
            return false;
        };
        self.register_well_known_symbol_name_mapping(name, symbol_ref);
        true
    }

    /// Recover the `SymbolRef` behind a canonical `[Symbol.xxx]` property-name
    /// key so `keyof` can contribute the precise `typeof Symbol.xxx`
    /// (`UniqueSymbol(ref)`) key type instead of widening to the generic
    /// `symbol`.  The reliable source of identity is the *type* of the
    /// underlying `Symbol.xxx` access expression: that is exactly the
    /// `UniqueSymbol(ref)` that `typeof Symbol.xxx` produces, so the recovered
    /// ref always matches the use-site.  Falls back to a name-based
    /// global-`Symbol` member lookup when no computed expression node is
    /// available.
    fn well_known_symbol_ref_for_name(
        &mut self,
        name: &str,
        name_idx: NodeIndex,
    ) -> Option<tsz_solver::SymbolRef> {
        if let Some(expr_idx) = self
            .ctx
            .arena
            .get(name_idx)
            .and_then(|node| self.ctx.arena.get_computed_property(node))
            .map(|computed| computed.expression)
        {
            let expr_type = self.get_type_of_node(expr_idx);
            for candidate in [expr_type, self.evaluate_type_with_env(expr_type)] {
                if let Some(symbol_ref) =
                    crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, candidate)
                {
                    return Some(symbol_ref);
                }
            }
        }
        self.resolve_well_known_symbol_ref_from_name(name)
    }

    fn resolve_well_known_symbol_ref_from_name(&self, name: &str) -> Option<tsz_solver::SymbolRef> {
        let member_name = name.strip_prefix("[Symbol.")?.strip_suffix(']')?;
        let lib_binders = self.get_lib_binders();

        if let Some(symbol_ctor) = self.resolve_global_value_symbol("Symbol")
            && let Some(symbol_ctor_sym) = self
                .ctx
                .binder
                .get_symbol_with_libs(symbol_ctor, &lib_binders)
            && let Some(member_sym) = symbol_ctor_sym
                .members
                .as_ref()
                .and_then(|members| members.get(member_name))
                .or_else(|| {
                    symbol_ctor_sym
                        .exports
                        .as_ref()
                        .and_then(|exports| exports.get(member_name))
                })
        {
            return Some(tsz_solver::SymbolRef(member_sym.0));
        }

        for lib_binder in lib_binders.iter() {
            let Some(symbol_ctor) = lib_binder.file_locals.get("Symbol") else {
                continue;
            };
            let Some(symbol_ctor_sym) = lib_binder.get_symbol(symbol_ctor) else {
                continue;
            };
            if let Some(member_sym) = symbol_ctor_sym
                .members
                .as_ref()
                .and_then(|members| members.get(member_name))
                .or_else(|| {
                    symbol_ctor_sym
                        .exports
                        .as_ref()
                        .and_then(|exports| exports.get(member_name))
                })
            {
                return Some(tsz_solver::SymbolRef(member_sym.0));
            }
        }

        None
    }

    pub(crate) fn get_bound_class_name_from_decl(&self, class_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(node)?;

        if class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            return Some(ident.escaped_text.clone());
        }

        let parent_idx = self
            .ctx
            .arena
            .get_extended(class_idx)
            .map(|ext| ext.parent)?;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
        let name_ident = self.ctx.arena.get_identifier_at(var_decl.name)?;
        Some(name_ident.escaped_text.clone())
    }

    /// Get class name from a class declaration node.
    /// Returns "<anonymous>" for unnamed classes.
    pub(crate) fn get_class_name_from_decl(&self, class_idx: NodeIndex) -> String {
        self.get_bound_class_name_from_decl(class_idx)
            .unwrap_or_else(|| "<anonymous>".to_string())
    }

    pub(crate) fn get_class_decl_for_display_type(
        &self,
        type_id: TypeId,
    ) -> Option<(NodeIndex, bool)> {
        if let Some(class_idx) = self.get_class_decl_from_type(type_id) {
            return Some((class_idx, false));
        }

        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
            && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
        {
            return Some((class_idx, true));
        }

        // Generic instance types like `C<number>` appear as
        // `TypeData::Application(base, args)`. The preceding brand and
        // `class_instance_type_to_decl` paths only see uninstantiated
        // instances, so walk the application chain to its underlying base
        // and resolve that. The `current != type_id` guard ensures we only
        // recurse when an application was actually unwrapped.
        let mut current = type_id;
        while let Some(app_base) =
            crate::query_boundaries::common::get_application_base(self.ctx.types, current)
        {
            current = app_base;
        }
        if current != type_id
            && let Some((class_idx, _)) = self.get_class_decl_for_display_type(current)
        {
            return Some((class_idx, false));
        }

        if let Some((&class_idx, _)) = self
            .ctx
            .class_constructor_type_cache
            .iter()
            .find(|entry| *entry.1 == type_id)
        {
            return Some((class_idx, true));
        }

        // Check symbol_types: if a class symbol's resolved type matches this type_id,
        // the type represents that class's constructor. This handles inferred return types
        // like `getClass() { return C; }` where the return type is a fresh TypeId that
        // differs from the cached constructor type.
        for (sym_id, sym_type) in self.ctx.symbol_types.iter() {
            if sym_type == type_id
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
            {
                return Some((class_idx, true));
            }
        }

        let sigs = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            type_id,
        )?;
        for sig in &sigs {
            if let Some(class_idx) = self.get_class_decl_from_type(sig.return_type) {
                return Some((class_idx, true));
            }
            if let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, sig.return_type)
                && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
                && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
            {
                return Some((class_idx, true));
            }
        }

        None
    }

    pub(crate) fn get_class_display_name_from_type(&self, type_id: TypeId) -> Option<String> {
        let (class_idx, is_constructor) = self.get_class_decl_for_display_type(type_id)?;
        let class_name = self.get_class_name_from_decl(class_idx);
        if is_constructor {
            Some(format!("typeof {class_name}"))
        } else {
            Some(class_name)
        }
    }

    /// Get class name with type parameters from a class declaration node.
    /// E.g., for `class D<T>`, returns `"D<T>"` instead of just `"D"`.
    /// Returns "<anonymous>" for unnamed classes.
    pub(crate) fn get_class_name_with_type_params_from_decl(&self, class_idx: NodeIndex) -> String {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return "<anonymous>".to_string();
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return "<anonymous>".to_string();
        };

        if let Some(mut name) = self.get_bound_class_name_from_decl(class_idx) {
            self.append_type_param_names(&mut name, &class.type_parameters);
            return name;
        }

        "<anonymous>".to_string()
    }

    /// Get the name of a class member (property, method, or accessor).
    pub(crate) fn get_member_name(&self, member_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(member_idx)?;

        // Use helper to get name node, then get property name text
        let name_idx = self.get_member_name_node(node)?;
        self.get_property_name(name_idx)
    }

    /// Get the name of a function declaration.
    pub(crate) fn get_function_name_from_node(&self, stmt_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(stmt_idx)?;

        if let Some(func) = self.ctx.arena.get_function(node)
            && func.name.is_some()
        {
            let name_node = self.ctx.arena.get(func.name)?;
            if let Some(id) = self.ctx.arena.get_identifier(name_node) {
                return Some(id.escaped_text.clone());
            }
        }

        None
    }

    /// Get the name of a parameter from its binding name node.
    /// Returns None for destructuring patterns.
    pub(crate) fn get_parameter_name(&self, name_idx: NodeIndex) -> Option<String> {
        let ident = self.ctx.arena.get_identifier_at(name_idx)?;
        Some(ident.escaped_text.clone())
    }
}
