use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Render an identifier-source display for an array-of-object-literal
    /// initializer (e.g. `let foo = [{ a: 1 }, { a: 2 }];`) so the
    /// assignability diagnostic can show the inferred element shape rather
    /// than just the identifier's static type.
    ///
    /// ## Phase 1 step-3: `StableLocation`-based declaration lookup
    ///
    /// Declaration *identity* is read from
    /// [`tsz_binder::Symbol::stable_declarations`] (with a fallback to
    /// [`tsz_binder::Symbol::stable_value_declaration`] when present). The
    /// concrete `NodeIndex` is rehydrated on demand via
    /// [`CheckerContext::node_at_stable_location`][nasl] so the consumer
    /// no longer assumes the arena that produced the symbol's stored
    /// `NodeIndex` is still resident. This is the third consumer migrated
    /// under the [global query graph plan][plan] (Phase 1 step 3,
    /// following PRs #1055 and #1066).
    ///
    /// The variable-declaration / array-literal walk is fundamentally
    /// AST-bound and continues to use a live `NodeIndex` locally; the
    /// load-bearing change is that the `NodeIndex` no longer comes from
    /// the symbol's arena-dependent field.
    ///
    /// [nasl]: crate::context::CheckerContext::node_at_stable_location
    /// [plan]: ../../../../../docs/plan/ROADMAP.md
    pub(in crate::error_reporter) fn identifier_array_object_literal_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & tsz_binder::symbol_flags::VARIABLE) == 0 {
            return None;
        }

        // Phase 1 step-3: identify the variable declaration via its
        // `StableLocation`, not via `symbol.declarations.first()`. Prefer
        // the first entry of `stable_declarations` (mirrors the legacy
        // `declarations.first()` preference order); fall back to
        // `stable_value_declaration` if the parallel slot is empty. The
        // parallel `stable_*` fields are populated in lockstep by the
        // binder, so this is equivalent whenever the legacy `NodeIndex`
        // fields are populated.
        let stable_loc = match symbol.stable_declarations.first() {
            Some(loc) if loc.is_known() => *loc,
            _ if symbol.stable_value_declaration.is_known() => symbol.stable_value_declaration,
            _ => return None,
        };

        // Resolve the `StableLocation` to a live `(NodeIndex, arena)` pair
        // and walk the variable-declaration body. We collect the
        // top-level shape (ordered names + initializer NodeIndex per
        // element) into owned data so the arena borrow is dropped before
        // any `&mut self` calls below (`get_property_name`,
        // `get_type_of_node`, ...). All `NodeIndex` values below come
        // from the *rehydrated* arena, not the symbol's stored
        // `NodeIndex`.
        let (ordered_name_idxs, element_property_idxs) = {
            let (decl_idx, arena) = self.ctx.node_at_stable_location(stable_loc)?;
            let decl = arena.get_variable_declaration_at(decl_idx)?;
            let init_idx = decl.initializer.into_option()?;
            let init_idx = arena.skip_parenthesized_and_assertions(init_idx);
            let init_node = arena.get(init_idx)?;
            if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                return None;
            }
            let literal = arena.get_literal_expr(init_node)?;
            if literal.elements.nodes.is_empty() {
                return None;
            }

            let mut ordered_name_idxs: Vec<NodeIndex> = Vec::new();
            let mut element_property_idxs: Vec<Vec<(NodeIndex, NodeIndex)>> = Vec::new();
            for (element_index, &element_idx) in literal.elements.nodes.iter().enumerate() {
                let element_node = arena.get(element_idx)?;
                if element_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    return None;
                }
                let object = arena.get_literal_expr(element_node)?;
                if element_index == 0 {
                    let mut row = Vec::with_capacity(object.elements.nodes.len());
                    for &child_idx in &object.elements.nodes {
                        let child = arena.get(child_idx)?;
                        let prop = arena.get_property_assignment(child)?;
                        ordered_name_idxs.push(prop.name);
                        row.push((prop.name, prop.initializer));
                    }
                    element_property_idxs.push(row);
                    continue;
                }

                if object.elements.nodes.len() != ordered_name_idxs.len() {
                    return None;
                }
                let mut row = Vec::with_capacity(object.elements.nodes.len());
                for &child_idx in &object.elements.nodes {
                    let child = arena.get(child_idx)?;
                    let prop = arena.get_property_assignment(child)?;
                    row.push((prop.name, prop.initializer));
                }
                element_property_idxs.push(row);
            }
            (ordered_name_idxs, element_property_idxs)
        };

        // Resolve the names *after* dropping the arena borrow.
        let mut ordered_names: Vec<String> = Vec::with_capacity(ordered_name_idxs.len());
        for name_idx in &ordered_name_idxs {
            ordered_names.push(self.get_property_name(*name_idx)?);
        }

        // Walk the rows, validate name parity element-by-element, and
        // collect `TypeId`s per ordered name.
        let mut property_values: Vec<Vec<TypeId>> = vec![Vec::new(); ordered_names.len()];
        for (element_index, row) in element_property_idxs.iter().enumerate() {
            if element_index == 0 {
                for (prop_index, &(_, init_idx)) in row.iter().enumerate() {
                    property_values[prop_index].push(self.get_type_of_node(init_idx));
                }
                continue;
            }
            for (prop_index, &(name_idx, init_idx)) in row.iter().enumerate() {
                let name = self.get_property_name(name_idx)?;
                if name != ordered_names[prop_index] {
                    return None;
                }
                property_values[prop_index].push(self.get_type_of_node(init_idx));
            }
        }

        let fields = ordered_names
            .into_iter()
            .zip(property_values)
            .map(|(name, value_types)| {
                let widened_types = value_types
                    .into_iter()
                    .map(|ty| self.widen_type_for_display(ty))
                    .collect::<Vec<_>>();
                let value_type = if widened_types.len() == 1 {
                    widened_types[0]
                } else {
                    self.ctx.types.factory().union(widened_types)
                };
                let display = self.format_assignability_type_for_message(value_type, target);
                format!("{name}: {display}")
            })
            .collect::<Vec<_>>()
            .join("; ");
        Some(format!("{{ {fields}; }}[]"))
    }

    /// Render a literal-source display for a `let`/`const` initializer of
    /// `true` or `false` when the assignability target is `undefined` or
    /// an enum (used for "Type 'true' is not assignable to ..." style
    /// diagnostics).
    ///
    /// ## Phase 1 step-3: `StableLocation`-based declaration lookup
    ///
    /// See [`Self::identifier_array_object_literal_source_display`] for
    /// the migration rationale. Same pattern: read `stable_*` for
    /// declaration identity, rehydrate `NodeIndex` via
    /// `ctx.node_at_stable_location`, walk the variable-declaration body
    /// against the rehydrated arena.
    pub(in crate::error_reporter) fn identifier_literal_initializer_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let target = self.evaluate_type_for_assignability(target);
        if target != TypeId::UNDEFINED
            && crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_none()
        {
            return None;
        }
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & tsz_binder::symbol_flags::VARIABLE) == 0 {
            return None;
        }

        // Phase 1 step-3: identify the primary variable declaration via
        // its `StableLocation`. Same preference order as
        // `identifier_array_object_literal_source_display` above.
        let stable_loc = match symbol.stable_declarations.first() {
            Some(loc) if loc.is_known() => *loc,
            _ if symbol.stable_value_declaration.is_known() => symbol.stable_value_declaration,
            _ => return None,
        };

        let (decl_idx, arena) = self.ctx.node_at_stable_location(stable_loc)?;
        let decl = arena.get_variable_declaration_at(decl_idx)?;
        if decl.type_annotation.is_some() || decl.initializer.is_none() {
            return None;
        }

        let init_idx = arena.skip_parenthesized(decl.initializer);
        let init_node = arena.get(init_idx)?;
        match init_node.kind {
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            _ => None,
        }
    }
}

// =============================================================================
// Phase 1 step-3 regression tests: `StableLocation` rehydration
// =============================================================================
//
// These tests validate the migration of
// `identifier_array_object_literal_source_display` and
// `identifier_literal_initializer_source_display` away from the
// arena-dependent `Symbol::declarations[0]: NodeIndex` toward the
// file-stable `Symbol::stable_declarations` / `stable_value_declaration`
// fields introduced by PR #1055. The critical invariant they lock in is
// that a `StableLocation` captured from one binder/arena pair can be
// resolved against a freshly re-parsed arena of the same source — the
// Phase 5 "bounded arena residency" precondition.

#[cfg(test)]
mod tests {
    use crate::context::{CheckerContext, CheckerOptions};
    use tsz_binder::BinderState;
    use tsz_parser::ParserState;
    use tsz_solver::TypeInterner;

    /// Resolving `Symbol::stable_declarations.first()` for a `let`
    /// initialized with an array-literal must return the same variable
    /// declaration node that `Symbol::declarations[0]` points at in the
    /// same binder. This is the invariant that the new code path relies
    /// on for behavior-equivalence with the legacy `NodeIndex` lookup.
    #[test]
    fn stable_declaration_resolves_to_variable_decl_node() {
        let source = "let xs = [{ a: 1 }, { a: 2 }];\n".to_string();

        let mut parser = ParserState::new("syn.ts".to_string(), source.clone());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let sym_id = binder.file_locals.get("xs").expect("variable symbol xs");
        let symbol = binder.symbols.get(sym_id).expect("symbol data");
        let stable = *symbol
            .stable_declarations
            .first()
            .expect("variable xs must have at least one stable_declarations entry");
        assert!(
            stable.is_known(),
            "variable xs must have a known stable_declarations[0] span"
        );
        let legacy_node_idx = *symbol
            .declarations
            .first()
            .expect("variable xs must have at least one declarations entry");

        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena,
            &binder,
            &types,
            "syn.ts".to_string(),
            CheckerOptions::default(),
        );

        let (resolved_idx, resolved_arena) = ctx
            .node_at_stable_location(stable)
            .expect("node_at_stable_location must resolve the variable-decl span");

        assert_eq!(
            resolved_idx, legacy_node_idx,
            "StableLocation must rehydrate to the same NodeIndex as declarations[0]"
        );
        let resolved_node = resolved_arena
            .get(resolved_idx)
            .expect("resolved NodeIndex must exist in arena");
        assert_eq!(resolved_node.pos, stable.pos);
        assert_eq!(resolved_node.end, stable.end);

        // Sanity: the rehydrated index is actually a VariableDeclaration
        // we can walk for an initializer (the production code path).
        let decl = resolved_arena
            .get_variable_declaration_at(resolved_idx)
            .expect("rehydrated NodeIndex must be a VariableDeclaration");
        assert!(
            decl.initializer.is_some(),
            "let xs = [...] must have a populated initializer"
        );
    }

    /// Phase 5 load-bearing scenario: capture a `StableLocation` from one
    /// binder/arena, drop it, re-parse the same source with a fresh
    /// arena, and verify the captured location still resolves correctly
    /// against the new arena. This proves
    /// `identifier_array_object_literal_source_display` survives Phase 5
    /// arena eviction-and-rehydrate.
    #[test]
    fn stable_location_round_trips_across_arena_reparse_for_var_decl() {
        let source = "let xs = [{ a: 1 }, { a: 2 }];\nlet other = 1;\n".to_string();

        // Capture the first arena's StableLocation for `xs`, then let
        // the first arena/binder go out of scope.
        let captured = {
            let mut parser = ParserState::new("syn.ts".to_string(), source.clone());
            let root = parser.parse_source_file();
            let arena = parser.get_arena();
            let mut binder = BinderState::new();
            binder.bind_source_file(arena, root);
            let sym_id = binder.file_locals.get("xs").expect("variable symbol xs");
            let symbol = binder.symbols.get(sym_id).expect("symbol data");
            *symbol
                .stable_declarations
                .first()
                .expect("variable xs must have a stable_declarations entry")
        };
        assert!(
            captured.is_known(),
            "captured StableLocation must carry a real (pos, end) span"
        );

        // Fresh parse + bind of the identical source. The captured
        // StableLocation must resolve in this new arena.
        let mut parser = ParserState::new("syn.ts".to_string(), source);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena,
            &binder,
            &types,
            "syn.ts".to_string(),
            CheckerOptions::default(),
        );

        let (resolved_idx, resolved_arena) = ctx
            .node_at_stable_location(captured)
            .expect("captured StableLocation must rehydrate against a freshly parsed arena");
        let node = resolved_arena
            .get(resolved_idx)
            .expect("resolved NodeIndex must exist in the new arena");
        assert_eq!(node.pos, captured.pos);
        assert_eq!(node.end, captured.end);

        // Sanity: still walks as a VariableDeclaration with an
        // array-literal initializer.
        let decl = resolved_arena
            .get_variable_declaration_at(resolved_idx)
            .expect("rehydrated NodeIndex must still be a VariableDeclaration");
        assert!(decl.initializer.is_some());

        // The new binder's `declarations[0]` NodeIndex should agree with
        // the helper's resolution — binder population is deterministic
        // for identical source text.
        let sym_id = binder
            .file_locals
            .get("xs")
            .expect("variable symbol xs in reparsed binder");
        let new_symbol = binder
            .symbols
            .get(sym_id)
            .expect("symbol data in reparsed binder");
        assert_eq!(
            resolved_idx,
            *new_symbol
                .declarations
                .first()
                .expect("reparsed variable xs must have a declarations entry"),
            "re-resolution must agree with the re-parsed binder's NodeIndex"
        );
    }

    /// Regression: a variable whose declaration has only an
    /// `initializer` (no array-literal shape) should also survive the
    /// `StableLocation` round-trip, exercising the
    /// `identifier_literal_initializer_source_display` code path.
    #[test]
    fn stable_location_round_trips_for_boolean_initializer() {
        let source = "let flag = true;\n".to_string();

        let captured = {
            let mut parser = ParserState::new("syn.ts".to_string(), source.clone());
            let root = parser.parse_source_file();
            let arena = parser.get_arena();
            let mut binder = BinderState::new();
            binder.bind_source_file(arena, root);
            let sym_id = binder
                .file_locals
                .get("flag")
                .expect("variable symbol flag");
            let symbol = binder.symbols.get(sym_id).expect("symbol data");
            *symbol
                .stable_declarations
                .first()
                .expect("variable flag must have a stable_declarations entry")
        };
        assert!(captured.is_known());

        let mut parser = ParserState::new("syn.ts".to_string(), source);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena,
            &binder,
            &types,
            "syn.ts".to_string(),
            CheckerOptions::default(),
        );

        let (resolved_idx, resolved_arena) = ctx
            .node_at_stable_location(captured)
            .expect("captured StableLocation must rehydrate against the reparsed arena");
        let decl = resolved_arena
            .get_variable_declaration_at(resolved_idx)
            .expect("rehydrated NodeIndex must be a VariableDeclaration");
        // The initializer must be present (and untyped); this is exactly
        // what `identifier_literal_initializer_source_display` checks
        // before walking the initializer.
        assert!(decl.initializer.is_some());
        assert!(decl.type_annotation.is_none());
    }
}
