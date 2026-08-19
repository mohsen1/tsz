//! Keyof type-alias display recovery for assignability diagnostics.

use crate::error_reporter::display_budget;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// True when a `keyof` operand has no user-visible name: it is an object type
    /// (typically an inline object type literal) with no binder symbol *and* no
    /// type-alias name for display. Such an operand cannot be written as
    /// `keyof Name`, so tsc renders the reduced key set (`"a" | "b"`) instead of
    /// the `keyof { ... }` spelling. A named interface / class / alias fails this
    /// predicate and keeps its `keyof Name` form.
    pub(in crate::error_reporter) fn keyof_operand_is_anonymous(&self, operand: TypeId) -> bool {
        crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, operand)
            .is_some_and(|shape| shape.symbol.is_none())
            && self.lookup_type_alias_name_for_display(operand).is_none()
    }

    /// True when `ty` is a `keyof X` whose operand `X` yields a finite, statically
    /// enumerable unit-literal key set (`"a" | "b"`, `0 | 1`), which tsc treats as
    /// a literal context — the assignment-source literal must then be displayed
    /// as-written, not widened. An index signature contributes the `string` or
    /// `number` primitive base to the key set, and a mapped/computed/generic
    /// operand has no statically-enumerable literal keys; both lack a literal
    /// context, so tsc widens the source there.
    pub(in crate::error_reporter) fn keyof_target_has_concrete_object_operand(
        &mut self,
        ty: TypeId,
    ) -> bool {
        let Some(operand) =
            crate::query_boundaries::diagnostics::keyof_inner_type(self.ctx.types, ty)
        else {
            return false;
        };
        if self.keyof_operand_yields_concrete_literal_keyset(operand) {
            return true;
        }
        // Operands whose shape is not directly visible — enum types and enum
        // namespaces (`keyof typeof E` excludes the implicit numeric index),
        // `typeof x` queries, and class instance references — are judged by
        // the evaluated key set itself: a finite unit-literal set is a
        // literal context regardless of how the operand was spelled.
        let evaluated = self.evaluate_type_for_assignability(ty);
        evaluated != ty
            && crate::query_boundaries::diagnostics::is_finite_unit_literal_keyset(
                self.ctx.types.as_type_database(),
                evaluated,
            )
    }

    /// True when the `X` in `keyof X` reduces to a finite literal key set: a plain
    /// object type with no string/number index signature, or a union/intersection
    /// of such objects.
    ///
    /// Composites distribute through `keyof` with dual set operations on the
    /// members' key sets:
    /// - `keyof (A | B)` = `keyof A & keyof B` — the *intersection* of the key
    ///   sets, which stays a finite literal set as soon as **any** member
    ///   contributes one (intersecting with a finite literal set cannot introduce
    ///   a primitive base).
    /// - `keyof (A & B)` = `keyof A | keyof B` — the *union* of the key sets, so
    ///   **every** member must contribute a finite literal key set (a single
    ///   `string`/`number` base from an index signature pollutes it).
    ///
    /// tsc keeps the assignment-source literal as-written whenever the target key
    /// set is such a literal context. Recognising the composite forms also makes
    /// the decision depend only on the operand's structure rather than on whether
    /// the `keyof` happened to be reduced to its key set already (which is
    /// evaluation-order sensitive for union operands and previously made the
    /// widening flip on incidental source formatting).
    fn keyof_operand_yields_concrete_literal_keyset(&mut self, operand: TypeId) -> bool {
        // The members are collected into an owned `Vec` so the borrow of
        // `self.ctx.types` taken by `union_members` / `intersection_members` is
        // released before the `&mut self` recursive call below.
        if let Some(members) =
            crate::query_boundaries::diagnostics::union_members(self.ctx.types, operand)
        {
            let members: Vec<TypeId> = members.iter().copied().collect();
            return members
                .into_iter()
                .any(|member| self.keyof_operand_yields_concrete_literal_keyset(member));
        }
        if let Some(members) =
            crate::query_boundaries::diagnostics::intersection_members(self.ctx.types, operand)
        {
            let members: Vec<TypeId> = members.iter().copied().collect();
            return !members.is_empty()
                && members
                    .into_iter()
                    .all(|member| self.keyof_operand_yields_concrete_literal_keyset(member));
        }
        // A direct object operand exposes its shape; otherwise resolve a
        // non-generic alias to its body first so the shape becomes visible.
        crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, operand)
            .or_else(|| {
                let def_id =
                    crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, operand)?;
                let body = self.ctx.type_env.borrow().get_def(def_id).or_else(|| {
                    self.ctx
                        .definition_store
                        .get(def_id)
                        .and_then(|def| def.body)
                })?;
                crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, body)
            })
            .is_some_and(|shape| shape.string_index.is_none() && shape.number_index.is_none())
    }

    pub(crate) fn keyof_type_alias_body_display(&mut self, ty: TypeId) -> Option<String> {
        if let Some(def_id) = self
            .ctx
            .definition_store
            .find_type_alias_by_body(ty)
            .or_else(|| {
                let def_id = self.ctx.definition_store.find_def_for_type(ty)?;
                let def = self.ctx.definition_store.get(def_id)?;
                (def.kind == tsz_solver::def::DefKind::TypeAlias).then_some(def_id)
            })
        {
            return self.keyof_type_alias_definition_display(def_id);
        }

        self.ctx
            .definition_store
            .all_type_alias_defs()
            .into_iter()
            .find_map(|def_id| {
                if !display_budget::try_consume_visit() {
                    return None;
                }
                let def = self.ctx.definition_store.get(def_id)?;
                if !def.type_params.is_empty() {
                    return None;
                }
                let body = def.body?;
                crate::query_boundaries::diagnostics::keyof_inner_type(self.ctx.types, body)?;
                let evaluated = self.evaluate_type_for_assignability(body);
                if display_budget::is_exhausted() {
                    return None;
                }
                (evaluated == ty).then_some(def_id)
            })
            .and_then(|def_id| self.keyof_type_alias_definition_display(def_id))
    }

    /// Whether a `keyof` operand is *value-derived* (`keyof typeof x`): a
    /// `typeof` query or enum type structurally (solver classifier), or an
    /// object shape whose declaring symbol is a value — an enum or namespace
    /// symbol — rather than a type declaration (interface/class keep the
    /// `keyof Name` spelling).
    pub(in crate::error_reporter) fn keyof_display_operand_is_value_derived(
        &self,
        operand: TypeId,
    ) -> bool {
        crate::query_boundaries::diagnostics::keyof_operand_is_value_derived(
            self.ctx.types.as_type_database(),
            operand,
        ) || crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, operand)
            .and_then(|shape| shape.symbol)
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .is_some_and(|symbol| {
                symbol.has_any_flags(
                    tsz_binder::symbol_flags::ENUM | tsz_binder::symbol_flags::VALUE_MODULE,
                )
            })
    }

    /// Operand-kind display for a bare `keyof` type in assignability
    /// messages (`ty` is the `KeyOf` type, `operand` its inner type):
    ///
    /// * free type parameter operand — always the `keyof T` spelling, even
    ///   when `T` has an inline anonymous constraint whose keys could be
    ///   enumerated (the anonymous-object branch reaches the constraint via
    ///   `get_object_shape`'s `TypeParameter` look-through, so the guard runs
    ///   first);
    /// * value-derived operand (`keyof typeof E`) — the evaluated key union;
    /// * named alias / symbol-bearing operand — the `keyof Name` spelling;
    /// * anonymous object operand (`keyof { ... }`) — the evaluated key set:
    ///   tsc only prints `keyof X` when `X` is a named reference.
    ///
    /// `None` when no branch owns the display (caller falls through to its
    /// generic paths).
    pub(in crate::error_reporter) fn keyof_operand_display_for_assignability_message(
        &mut self,
        ty: TypeId,
        keyof_inner: TypeId,
    ) -> Option<String> {
        if let Some(param_info) = crate::query_boundaries::diagnostics::type_param_info(
            self.ctx.types.as_type_database(),
            keyof_inner,
        ) {
            let param_name = self.ctx.types.resolve_atom_ref(param_info.name);
            return Some(format!("keyof {param_name}"));
        }

        if let Some(display) = self.value_derived_keyof_reduced_display(ty, keyof_inner) {
            return Some(display);
        }

        if let Some(alias_name) = self.lookup_type_alias_name_for_display(keyof_inner) {
            return Some(format!("keyof {alias_name}"));
        }

        if let Some(shape) =
            crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, keyof_inner)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            return Some(format!("keyof {}", symbol.escaped_name));
        }

        if crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, keyof_inner)
            .is_some_and(|shape| shape.symbol.is_none())
        {
            let evaluated = self.evaluate_type_with_env(ty);
            if evaluated != ty
                && evaluated != TypeId::ERROR
                && crate::query_boundaries::diagnostics::keyof_inner_type(self.ctx.types, evaluated)
                    .is_none()
            {
                return Some(self.format_type_for_assignability_message(evaluated));
            }
        }
        None
    }

    /// The reduced key-union display for a `keyof` type whose operand is
    /// value-derived, or `None` when the operand is a named type reference or
    /// the `keyof` does not reduce. `ty` is the `KeyOf` type and `operand` its
    /// inner type.
    pub(in crate::error_reporter) fn value_derived_keyof_reduced_display(
        &mut self,
        ty: TypeId,
        operand: TypeId,
    ) -> Option<String> {
        if !self.keyof_display_operand_is_value_derived(operand) {
            return None;
        }
        let evaluated = self.evaluate_type_with_env(ty);
        if evaluated == ty
            || evaluated == TypeId::ERROR
            || crate::query_boundaries::diagnostics::keyof_inner_type(self.ctx.types, evaluated)
                .is_some()
        {
            return None;
        }
        Some(self.format_type_for_assignability_message(evaluated))
    }

    pub(crate) fn keyof_type_alias_definition_display(
        &mut self,
        def_id: tsz_solver::def::DefId,
    ) -> Option<String> {
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias || !def.type_params.is_empty() {
            return None;
        }
        let direct_body = def.body?;
        // An alias chain (`type K2 = K1; type K1 = keyof ...`) peels
        // transparently to the `keyof` body of the terminal alias.
        let body =
            if crate::query_boundaries::diagnostics::keyof_inner_type(self.ctx.types, direct_body)
                .is_some()
            {
                direct_body
            } else {
                crate::query_boundaries::diagnostics::keyof_alias_display_body(
                    self.ctx.types.as_type_database(),
                    &self.ctx.definition_store,
                    direct_body,
                )?
            };
        let inner = crate::query_boundaries::diagnostics::keyof_inner_type(self.ctx.types, body)?;
        // A value-derived operand (`keyof typeof E` — enum, enum namespace, or
        // object value) renders as its evaluated key union in the pinned
        // typescript@7.0.2 oracle, never as `keyof E` or the alias name; only
        // a named *type* operand (`keyof I`, interface/class) keeps the
        // `keyof` spelling.
        if self.keyof_display_operand_is_value_derived(inner) {
            let evaluated = self.evaluate_type_for_assignability(body);
            // Rendered through the literal-key path (not the type formatter)
            // so a global `union -> keyof Name` display alias on the reduced
            // union cannot repaint it back to the `keyof` spelling.
            if let Some(display) = self.finite_literal_keyset_display(evaluated) {
                return Some(display);
            }
            return None;
        }
        if let Some(alias_name) = self.lookup_type_alias_name_for_display(inner) {
            return Some(format!("keyof {alias_name}"));
        }
        if let Some(shape) =
            crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, inner)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            return Some(format!("keyof {}", symbol.escaped_name));
        }
        None
    }

    pub(in crate::error_reporter) fn keyof_type_alias_annotation_display_for_expression(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        if let Some(type_node_idx) = self.declared_assignment_type_annotation_node(expr_idx)
            && let Some(display) = self.keyof_type_alias_annotation_node_display(type_node_idx)
        {
            return Some(display);
        }
        let annotation = self.declared_type_annotation_text_for_expression(expr_idx)?;
        self.keyof_type_alias_annotation_display(&annotation)
    }

    pub(in crate::error_reporter) fn declared_assignment_type_annotation_node(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.node_symbols.get(&expr_idx.0).copied())?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut declarations = Vec::new();
        if symbol.value_declaration.is_some() {
            declarations.push(symbol.value_declaration);
        }
        declarations.extend(symbol.declarations.iter().copied());

        declarations.into_iter().find_map(|decl_idx| {
            let decl_idx = if self
                .ctx
                .arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::Identifier as u16)
            {
                self.ctx
                    .arena
                    .get_extended(decl_idx)
                    .map(|ext| ext.parent)
                    .filter(|parent| parent.is_some())
                    .unwrap_or(decl_idx)
            } else {
                decl_idx
            };
            let decl = self.ctx.arena.get(decl_idx)?;
            if let Some(param) = self.ctx.arena.get_parameter(decl)
                && param.type_annotation.is_some()
            {
                return Some(param.type_annotation);
            }
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl)
                && var_decl.type_annotation.is_some()
            {
                return Some(var_decl.type_annotation);
            }
            if let Some(prop_decl) = self.ctx.arena.get_property_decl(decl)
                && prop_decl.type_annotation.is_some()
            {
                return Some(prop_decl.type_annotation);
            }
            None
        })
    }

    fn keyof_type_alias_annotation_node_display(
        &mut self,
        type_node_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = self.ctx.arena.get(type_node_idx)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.ctx.arena.get_type_ref(type_node)?;
        let sym_id = match self.resolve_qualified_symbol_in_type_position(type_ref.type_name) {
            TypeSymbolResolution::Type(sym_id) | TypeSymbolResolution::ValueOnly(sym_id) => sym_id,
            TypeSymbolResolution::NotFound => return None,
        };
        let def_id = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| {
                self.ctx
                    .definition_store
                    .lookup_by_symbol(sym_id.0, file_idx as u32)
            })
            .or_else(|| self.ctx.definition_store.find_def_by_symbol(sym_id.0))?;
        self.keyof_type_alias_definition_display(def_id)
    }

    pub(in crate::error_reporter) fn keyof_type_alias_annotation_display(
        &mut self,
        annotation: &str,
    ) -> Option<String> {
        let name = simple_or_namespace_member_name(annotation.trim())?;
        if name != annotation.trim() {
            return None;
        }
        let name_atom = self.ctx.types.intern_string(name);
        self.ctx
            .definition_store
            .find_defs_by_name(name_atom)?
            .into_iter()
            .find_map(|def_id| {
                let def = self.ctx.definition_store.get(def_id)?;
                (def.kind == tsz_solver::def::DefKind::TypeAlias
                    && def.type_params.is_empty()
                    && def.name == name_atom)
                    .then_some(def_id)
            })
            .and_then(|def_id| self.keyof_type_alias_definition_display(def_id))
            .or_else(|| self.keyof_type_alias_textual_definition_display(name))
    }

    fn keyof_type_alias_textual_definition_display(&mut self, name: &str) -> Option<String> {
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let pattern = format!("type {name} = keyof ");
        let start = source.rfind(&pattern)? + pattern.len();
        let rest = &source[start..];
        let end = rest
            .char_indices()
            .find_map(|(idx, ch)| {
                (idx > 0 && matches!(ch, ';' | '\n' | '\r' | ',' | ')' | '{')).then_some(idx)
            })
            .unwrap_or(rest.len());
        let operand = rest[..end].trim();
        // This textual fallback only validly reconstructs a *bare named* operand
        // (`type X = keyof Foo`). A compound operand cannot be stitched together
        // from source text: the scan above stops at the first `{`, so a
        // parenthesized operand such as `keyof ({ a: 1 } & { b: 2 })` would
        // otherwise leave a dangling `(` and emit the malformed `keyof (`.
        //
        // For such an alias `tsc` renders the *evaluated key set* (`"a" | "b"`),
        // because the `keyof` of an anonymous composite carries no writable name.
        // Eager `keyof` evaluation has already erased the operator from the alias
        // body, but the source spelling here proves the alias is a `keyof`, so
        // render the reduced literal key union directly.
        if operand.is_empty()
            || operand.contains('|')
            || operand.contains('&')
            || operand.contains('[')
            || operand.contains('{')
            || operand.contains('(')
            || operand.contains(')')
            || operand.contains("=>")
        {
            return self.keyof_alias_reduced_keyset_display(name);
        }
        Some(format!(
            "keyof {}",
            self.format_annotation_like_type(operand)
        ))
    }

    /// Render the evaluated key set of a non-generic `keyof` type alias by its
    /// members (`"a" | "b"`), matching how `tsc` displays the `keyof` of an
    /// anonymous composite operand whose result has no writable `keyof Name`
    /// form. Returns `None` unless the alias body reduces to a finite union (or
    /// a single instance) of string/number literal keys.
    fn keyof_alias_reduced_keyset_display(&mut self, name: &str) -> Option<String> {
        let name_atom = self.ctx.types.intern_string(name);
        let def_id = self
            .ctx
            .definition_store
            .find_defs_by_name(name_atom)?
            .into_iter()
            .find(|&def_id| {
                self.ctx.definition_store.get(def_id).is_some_and(|def| {
                    def.kind == tsz_solver::def::DefKind::TypeAlias
                        && def.type_params.is_empty()
                        && def.name == name_atom
                })
            })?;
        let body = self.ctx.definition_store.get(def_id)?.body?;
        let evaluated = self.evaluate_type_for_assignability(body);
        self.finite_literal_keyset_display(evaluated)
    }

    /// Format `ty` as a literal key union (`"a" | "b"`) when it is a finite union
    /// (or a single instance) of string/number literals. Members are rendered
    /// directly from their literal values rather than through the type formatter,
    /// so an unrelated global `union -> keyof Name` display alias on a shared
    /// literal cannot repaint a reduced key. Returns `None` for any other shape.
    fn finite_literal_keyset_display(&mut self, ty: TypeId) -> Option<String> {
        if let Some(value) = crate::query_boundaries::diagnostics::literal_value(self.ctx.types, ty)
        {
            return self.literal_key_display(value);
        }
        let members: Vec<TypeId> =
            crate::query_boundaries::diagnostics::union_members(self.ctx.types, ty)?
                .iter()
                .copied()
                .collect();
        if members.is_empty() {
            return None;
        }
        let mut parts = Vec::with_capacity(members.len());
        for member in members {
            let value =
                crate::query_boundaries::diagnostics::literal_value(self.ctx.types, member)?;
            parts.push(self.literal_key_display(value)?);
        }
        Some(parts.join(" | "))
    }

    /// Render a single literal key as `tsc` does in a key union: string keys are
    /// quoted (`"a"`), number keys are bare (`0`). Non-key literals (`boolean`)
    /// return `None`.
    fn literal_key_display(&self, value: tsz_solver::LiteralValue) -> Option<String> {
        match value {
            tsz_solver::LiteralValue::String(atom) => {
                Some(format!("\"{}\"", self.ctx.types.resolve_atom_ref(atom)))
            }
            tsz_solver::LiteralValue::Number(value) => {
                Some(tsz_solver::utils::js_number_to_string(value.0).into_owned())
            }
            tsz_solver::LiteralValue::BigInt(atom) => {
                Some(format!("{}n", self.ctx.types.resolve_atom_ref(atom)))
            }
            tsz_solver::LiteralValue::Boolean(_) => None,
        }
    }
}

fn simple_or_namespace_member_name(display: &str) -> Option<&str> {
    if display.starts_with("typeof ")
        || display.starts_with("import(")
        || display.contains('<')
        || display.contains('[')
        || display.contains(' ')
    {
        return None;
    }
    let name = display.rsplit_once('.').map_or(display, |(_, short)| short);
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
        return None;
    }
    chars
        .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        .then_some(name)
}
