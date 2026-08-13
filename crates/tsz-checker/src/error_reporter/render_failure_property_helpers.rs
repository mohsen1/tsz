use super::*;
use crate::query_boundaries::diagnostics as diagnostic_query;

impl<'a> CheckerState<'a> {
    // Extracted from `render_failure.rs` to keep property rendering helpers under the file-size cap.

    /// Render the property key for the single-property TS2741 message.
    ///
    /// tsc qualifies an enum-member-derived key as `[E.B]` here (the single
    /// "Property '…' is missing" message), so this path consults the enum
    /// origin of the key. The multi-property TS2739/TS2740 list uses bare
    /// member names instead — see [`Self::missing_property_list_name_for_display`].
    pub(super) fn missing_property_name_for_display(
        &mut self,
        property_name: tsz_common::interner::Atom,
        target: TypeId,
    ) -> String {
        // tsc 7.0.2 renders enum-derived mapped keys as the bare literal
        // value ('0', 'sa'), never as a qualified '[E.A]'; only symbol keys
        // keep their bracketed display.
        if let Some(display) = self.symbol_keyed_property_name_for_display(property_name, target) {
            return display;
        }
        self.ctx.types.resolve_atom_ref(property_name).to_string()
    }

    /// Render a symbol-valued computed property key (`[sym]`) for missing-property
    /// diagnostics. Unique-symbol keys are stored internally under the synthetic
    /// `__unique_<SymbolId>` binding-identity atom; `tsc` displays them as
    /// `[<symbolName>]`, never the internal atom.
    ///
    /// `owner` is the type that requires the property. It is consulted so a
    /// user-authored string property that merely *looks* like `"__unique_3"`
    /// (and is therefore not symbol-named) keeps its string spelling, matching
    /// `tsc`. Returns `None` when the key is not a resolvable symbol key.
    pub(super) fn symbol_keyed_property_name_for_display(
        &mut self,
        property_name: tsz_common::interner::Atom,
        owner: TypeId,
    ) -> Option<String> {
        let key = self.ctx.types.resolve_atom_ref(property_name).to_string();
        let id: u32 = key.strip_prefix("__unique_")?.parse().ok()?;
        let prop =
            crate::query_boundaries::common::find_property_by_str(self.ctx.types, owner, &key)
                .or_else(|| {
                    let evaluated = self.evaluate_type_with_env(owner);
                    crate::query_boundaries::common::find_property_by_str(
                        self.ctx.types,
                        evaluated,
                        &key,
                    )
                })?;
        if !prop.is_symbol_named {
            return None;
        }
        let symbol = self.ctx.binder.get_symbol(tsz_binder::SymbolId(id))?;
        Some(format!("[{}]", symbol.escaped_name))
    }

    /// Render a property key for the multi-property TS2739/TS2740 list
    /// ("… is missing the following properties from type '…': a, b").
    ///
    /// Unlike the single-property TS2741 message, tsc lists bare member names
    /// here even when the keys originate from an enum (`b, c`, not
    /// `[E.B], [E.C]`), so this path never qualifies the key with its enum
    /// member origin.
    pub(super) fn missing_property_list_name_for_display(
        &mut self,
        property_name: tsz_common::interner::Atom,
        owner: TypeId,
    ) -> String {
        if let Some(display) = self.symbol_keyed_property_name_for_display(property_name, owner) {
            return display;
        }
        self.ctx.types.resolve_atom_ref(property_name).to_string()
    }

    /// Format a missing-property name list for the TS2739/TS2740 "missing the
    /// following properties" message. tsc lists up to 5 names inline; for 6+ it
    /// lists the first 4 then "and N more". Returns the joined list and the
    /// "and N more" count (present only when truncated).
    pub(crate) fn truncated_missing_property_list(
        &mut self,
        ordered: &[tsz_common::interner::Atom],
        owner: TypeId,
    ) -> (String, Option<usize>) {
        let is_truncated = ordered.len() > 5;
        let display_count = if is_truncated { 4 } else { 5 };
        let joined = ordered
            .iter()
            .take(display_count)
            .map(|name| self.missing_property_list_name_for_display(*name, owner))
            .collect::<Vec<_>>()
            .join(", ");
        let more = is_truncated.then(|| ordered.len() - display_count);
        (joined, more)
    }

    pub(super) fn enum_mapped_property_name_for_display(
        &mut self,
        property_name: tsz_common::interner::Atom,
        target: TypeId,
    ) -> Option<String> {
        let property_key = self.ctx.types.resolve_atom_ref(property_name).to_string();

        // A mapped type `{ [K in E]: V }` iterates the members of the enum `E`,
        // so every generated property key originates from an enum member. tsc
        // renders such computed keys as `[E.B]` rather than the underlying
        // literal value `b`. Recover the iteration constraint from the target
        // (the mapped type itself, an alias to it, or a `Lazy` reference whose
        // body is the mapped type) and match the missing key to its member.
        if let Some(constraint) = self.mapped_iteration_key_constraint(target)
            && let Some(display) =
                self.enum_key_property_name_for_display(&property_key, constraint)
        {
            return Some(display);
        }

        // `Record<E, V>`-style applications carry the enum as a type argument.
        if let Some((_, args)) = diagnostic_query::application_info(self.ctx.types, target)
            && let Some(display) = args
                .into_iter()
                .find_map(|arg| self.enum_key_property_name_for_display(&property_key, arg))
        {
            return Some(display);
        }

        // A mapped type over a *concrete* key constraint (`{ [K in E]: V }` where
        // `E` is a fully-known enum, not a type parameter) is instantiated to a
        // plain object during lowering, so neither the `Mapped` node nor the enum
        // origin survives on the resulting type. The iteration constraint is then
        // recoverable only from the alias declaration's AST, where the mapped
        // syntax (and its `in E` clause) is still intact.
        let constraint = self.alias_declaration_mapped_key_constraint(target)?;
        self.enum_key_property_name_for_display(&property_key, constraint)
    }

    /// Recover the key constraint of a mapped type whose alias body was
    /// eagerly instantiated to a plain object (`type M = { [K in E]: V }`).
    ///
    /// The concrete-key mapped type is materialized during lowering, so the
    /// `Mapped` node is gone from the resolved type. The declaration AST still
    /// carries the `{ [K in E]: V }` syntax, so resolve the alias `target`
    /// refers to and read the `in <constraint>` clause off its mapped-type node.
    /// Restricted to plain key-domain mapped types (no `as` name-type remap),
    /// where every generated key is exactly an iteration-constraint member.
    fn alias_declaration_mapped_key_constraint(&mut self, target: TypeId) -> Option<TypeId> {
        use tsz_parser::parser::NodeIndex;
        let def_id = diagnostic_query::lazy_def_id(self.ctx.types, target)
            .or_else(|| self.ctx.definition_store.find_def_for_type(target))?;
        let symbol_id = tsz_binder::SymbolId(self.ctx.definition_store.get(def_id)?.symbol_id?);
        let body_node = self.type_alias_type_node(symbol_id)?;
        let mapped_node = self.ctx.arena.get(body_node)?;
        // `get_mapped_type` is kind-gated (returns `None` unless the node is a
        // `MAPPED_TYPE`), so a plain `type M = Foo` alias falls through here.
        let mapped = self.ctx.arena.get_mapped_type(mapped_node)?;
        // A `[K in E as R]` name-type remap breaks the 1:1 key↔member
        // correspondence, so this recovery applies only to the plain form.
        if mapped.name_type != NodeIndex::NONE {
            return None;
        }
        let type_param_node = self.ctx.arena.get(mapped.type_parameter)?;
        let type_param = self.ctx.arena.get_type_parameter(type_param_node)?;
        if type_param.constraint == NodeIndex::NONE {
            return None;
        }
        Some(self.get_type_from_type_node(type_param.constraint))
    }

    /// Recover the key constraint (e.g. the enum `E` in `{ [K in E]: V }`) of a
    /// mapped type reachable from `target`.
    ///
    /// `target` may be the mapped type directly, a `Lazy(DefId)` reference to a
    /// type alias whose body is the mapped type, or an evaluated object whose
    /// display alias is the mapped type.
    pub(super) fn mapped_iteration_key_constraint(&mut self, target: TypeId) -> Option<TypeId> {
        let lazy_body = {
            let env = self.ctx.type_env.try_borrow().ok();
            crate::query_boundaries::flow::resolve_lazy_def_with_env(
                self.ctx.types,
                env.as_deref(),
                target,
            )
        };
        let candidates = [
            target,
            lazy_body,
            self.ctx.types.get_display_alias(target).unwrap_or(target),
        ];
        let mapped_id = candidates
            .into_iter()
            .find_map(|t| diagnostic_query::mapped_type_id(self.ctx.types, t))?;
        Some(self.ctx.types.mapped_type(mapped_id).constraint)
    }

    pub(super) fn enum_key_property_name_for_display(
        &mut self,
        property_key: &str,
        key_type: TypeId,
    ) -> Option<String> {
        if let Some(members) = diagnostic_query::union_members(self.ctx.types, key_type) {
            return members
                .iter()
                .find_map(|&member| self.enum_key_property_name_for_display(property_key, member));
        }

        let def_id = diagnostic_query::enum_def_id(self.ctx.types, key_type)
            .or_else(|| diagnostic_query::lazy_def_id(self.ctx.types, key_type))?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind == tsz_solver::def::DefKind::Enum && !def.enum_members.is_empty() {
            return self.enum_property_name_from_parent_def(property_key, &def);
        }

        self.enum_property_name_from_member_type(property_key, key_type, &def)
    }

    pub(super) fn enum_property_name_from_parent_def(
        &mut self,
        property_key: &str,
        enum_def: &tsz_solver::def::DefinitionInfo,
    ) -> Option<String> {
        let enum_name = self.ctx.types.resolve_atom_ref(enum_def.name).to_string();
        let enum_symbol_id = tsz_binder::SymbolId(enum_def.symbol_id?);
        let enum_symbol = self.ctx.binder.get_symbol(enum_symbol_id)?;
        let exports = enum_symbol.exports.as_ref()?;

        for (member_atom, _) in &enum_def.enum_members {
            let member_name = self.ctx.types.resolve_atom_ref(*member_atom).to_string();
            let Some(member_symbol_id) = exports.get(&member_name) else {
                continue;
            };
            let Some(member_type) = self.ctx.symbol_types.get(&member_symbol_id) else {
                continue;
            };
            if self.enum_member_type_matches_property_key(member_type, property_key) {
                return Some(format!("[{enum_name}.{member_name}]"));
            }
        }

        None
    }

    pub(super) fn enum_property_name_from_member_type(
        &mut self,
        property_key: &str,
        member_type: TypeId,
        member_def: &tsz_solver::def::DefinitionInfo,
    ) -> Option<String> {
        if !self.enum_member_type_matches_property_key(member_type, property_key) {
            return None;
        }

        let member_symbol_id = tsz_binder::SymbolId(member_def.symbol_id?);
        let member_symbol = self.ctx.binder.get_symbol(member_symbol_id)?;
        if member_symbol.parent.is_none() {
            return None;
        }
        let enum_symbol = self.ctx.binder.get_symbol(member_symbol.parent)?;
        Some(format!(
            "[{}.{}]",
            enum_symbol.escaped_name, member_symbol.escaped_name
        ))
    }

    pub(super) fn enum_member_type_matches_property_key(
        &self,
        member_type: TypeId,
        property_key: &str,
    ) -> bool {
        let value_type =
            diagnostic_query::enum_member_type(self.ctx.types, member_type).unwrap_or(member_type);
        diagnostic_query::literal_value(self.ctx.types, value_type)
            .and_then(|literal| self.literal_property_key_text(literal))
            .is_some_and(|key| key == property_key)
    }

    pub(super) fn literal_property_key_text(
        &self,
        literal: tsz_solver::LiteralValue,
    ) -> Option<String> {
        match literal {
            tsz_solver::LiteralValue::String(atom) | tsz_solver::LiteralValue::BigInt(atom) => {
                Some(self.ctx.types.resolve_atom_ref(atom).to_string())
            }
            tsz_solver::LiteralValue::Number(value) => {
                Some(tsz_solver::utils::js_number_to_string(value.0).into_owned())
            }
            tsz_solver::LiteralValue::Boolean(value) => Some(value.to_string()),
        }
    }

    /// Render an `OptionalPropertyRequired` failure: a source property that is
    /// present but optional assigned to a required target property. tsc reports
    /// TS2327 ("Property '_' is optional in type '_' but required in type '_'."),
    /// not the absent-property message TS2741. The rule is structural, so it
    /// covers inline `{ x?: T }` and mapped (`Partial<T>`, `{ [K in keyof T]?:
    /// T[K] }`) sources alike.
    pub(super) fn render_optional_property_required(
        &mut self,
        ctx: &RenderContext,
        property_name: tsz_common::interner::Atom,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let source_str = self
                .private_identifier_missing_source_base_display(source, property_name)
                .unwrap_or_else(|| self.format_type_diagnostic(source));
            let target_str = self
                .checked_js_global_element_access_fallback_target_display(idx)
                .unwrap_or_else(|| self.format_type_diagnostic(target));
            let detail = format_message(
                diagnostic_messages::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &source_str, &target_str],
            );
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.push_elaboration(
                detail,
                diagnostic_codes::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                0,
            );
            diag
        } else {
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let source_str = self
                .private_identifier_missing_source_base_display(source, property_name)
                .unwrap_or_else(|| self.format_type_diagnostic(source));
            let target_str = self
                .checked_js_global_element_access_fallback_target_display(idx)
                .unwrap_or_else(|| self.format_type_diagnostic(target));
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &source_str, &target_str],
            );
            Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            )
        }
    }

    pub(super) fn private_identifier_missing_source_base_display(
        &mut self,
        source: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let property_name = self.ctx.types.resolve_atom_ref(property_name);
        if !property_name.starts_with('#') {
            return None;
        }

        let source_shape = diagnostic_query::object_shape_for_type(self.ctx.types, source)?;
        let source_symbol = self.ctx.binder.get_symbol(source_shape.symbol?)?;
        let source_declarations = source_symbol.declarations.clone();

        for decl_idx in source_declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let Some(heritage_clauses) = &interface.heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    let expr_idx = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else if type_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE
                    {
                        self.ctx
                            .arena
                            .get_type_ref(type_node)
                            .map_or(type_idx, |type_ref| type_ref.type_name)
                    } else {
                        type_idx
                    };

                    let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                        continue;
                    };
                    let Some(base_symbol) = self
                        .get_cross_file_symbol(base_sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(base_sym_id))
                    else {
                        continue;
                    };
                    let base_declarations = base_symbol.declarations.clone();

                    for base_decl_idx in base_declarations {
                        let Some(base_node) = self.ctx.arena.get(base_decl_idx) else {
                            continue;
                        };
                        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                            continue;
                        };
                        let base_type = self.get_class_instance_type(base_decl_idx, base_class);
                        return Some(self.format_type_diagnostic(base_type));
                    }
                }
            }
        }

        None
    }

    pub(super) fn render_property_nominal_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        ctx: &RenderContext,
        property_name: tsz_common::interner::Atom,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        if let Some((prop_name, owner_name, visibility)) =
            self.private_or_protected_member_missing_display(source, target, Some(property_name))
        {
            let widened_source = self.widen_type_for_display(source);
            let src_str = if source == TypeId::OBJECT {
                "{}".to_string()
            } else {
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = self.format_type_diagnostic(target);
            let message = self.private_or_protected_assignability_message(
                &src_str,
                &tgt_str,
                &prop_name,
                &owner_name,
                visibility,
                None,
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        let (source_str, target_str) =
            self.format_top_level_assignability_message_types_at(source, target, idx);
        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        let mut diag = Diagnostic::error(
            file_name,
            start,
            length,
            base,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        if let Some(detail) = self.nominal_mismatch_detail(source, target, property_name) {
            diag.push_elaboration(detail, reason.diagnostic_code(), 0);
        }
        diag
    }

    /// Render a nominal mismatch on an ES private identifier (`#name`).
    ///
    /// tsc: `Type 'A' is not assignable to type 'B'.` elaborated with TS18015
    /// `Property '#x' in type 'A' refers to a different member that cannot be
    /// accessed from within type 'B'.` — the source class first, the target
    /// second, both spelled as in the top-level assignability message.
    pub(super) fn render_private_identifier_member_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        ctx: &RenderContext,
        property_name: tsz_common::interner::Atom,
    ) -> Diagnostic {
        let (source_str, target_str) =
            self.format_top_level_assignability_message_types_at(ctx.source, ctx.target, ctx.idx);
        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        let mut diag = Diagnostic::error(
            ctx.file_name.clone(),
            ctx.start,
            ctx.length,
            base,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        // The elaboration names each side's *declaring class* (uninstantiated),
        // not the top-level `source_str`/`target_str` — so `G<number>` still
        // elaborates as `G`, matching `tsc`. The top-level strings remain the
        // fallback for a side with no resolvable owning class.
        let detail = self.private_identifier_mismatch_detail(
            ctx.source,
            ctx.target,
            property_name,
            &source_str,
            &target_str,
        );
        diag.push_elaboration(detail, reason.diagnostic_code(), 0);
        diag
    }

    pub(super) fn render_return_type_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        ctx: &RenderContext,
        source_return: TypeId,
        target_return: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );

            // tsc's elaboration shape for return-type mismatches goes
            // straight from the top-level message into the inner mismatch
            // (e.g. "Type 'Object' is not assignable to type 'string'.")
            // without an intermediate "Return type 'X' is not assignable
            // to 'Y'." line. Only emit the "Return type ..." fallback when
            // there is no nested reason that already carries the inner
            // mismatch — otherwise we'd double-elaborate the same gap.
            if let Some(nested) = nested_reason
                && depth < 5
            {
                let nested_diag = self.render_failure_reason(
                    nested,
                    source_return,
                    target_return,
                    idx,
                    depth + 1,
                );
                diag.push_elaboration_at(
                    nested_diag.file,
                    nested_diag.start,
                    nested_diag.length,
                    nested_diag.message_text,
                    nested_diag.code,
                    0,
                );
            } else {
                let ret_source_str = self.format_type_diagnostic(source_return);
                let ret_target_str = self.format_type_diagnostic(target_return);
                let ret_msg = format!(
                    "Return type '{ret_source_str}' is not assignable to '{ret_target_str}'."
                );
                diag.push_elaboration(ret_msg, reason.diagnostic_code(), 0);
            }

            diag
        } else {
            let source_str = self.format_type_diagnostic(source_return);
            let target_str = self.format_type_diagnostic(target_return);
            let message =
                format!("Return type '{source_str}' is not assignable to '{target_str}'.");
            let mut diag =
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code());

            if let Some(nested) = nested_reason
                && depth < 5
            {
                let nested_diag = self.render_failure_reason(
                    nested,
                    source_return,
                    target_return,
                    idx,
                    depth + 1,
                );
                diag.push_elaboration_at(
                    nested_diag.file,
                    nested_diag.start,
                    nested_diag.length,
                    nested_diag.message_text,
                    nested_diag.code,
                    0,
                );
            }
            diag
        }
    }

    /// Format a `TypePredicate` the way tsc's `typePredicateToString` does
    /// (`x is T`, `this is T`, `asserts x is T`) — used for the TS1226
    /// `Type predicate '{0}' is not assignable to '{1}'.` arguments. Mirrors
    /// the solver's own `format_signature_with_predicate` predicate leg
    /// (`crates/tsz-solver/src/diagnostics/format/compound.rs`), duplicated
    /// here because that helper only renders a predicate as part of a whole
    /// signature, never standalone.
    fn format_type_predicate_for_diagnostic(&mut self, pred: &tsz_solver::TypePredicate) -> String {
        let target_name = match pred.target {
            tsz_solver::TypePredicateTarget::This => "this".to_string(),
            tsz_solver::TypePredicateTarget::Identifier(atom) => {
                self.ctx.types.resolve_atom_ref(atom).to_string()
            }
        };
        let type_part = pred
            .type_id
            .map(|ty| format!(" is {}", self.format_type_diagnostic(ty)));
        if pred.asserts {
            format!("asserts {target_name}{}", type_part.unwrap_or_default())
        } else {
            format!("{target_name}{}", type_part.unwrap_or_default())
        }
    }

    /// Render a type-predicate assignability failure: TS1224
    /// (`source_predicate: None` — the target demands a type guard the
    /// source signature doesn't declare at all) or TS1226 (`Some` — both
    /// sides declare a predicate but are incompatible), each nested under
    /// the ordinary `Type 'S' is not assignable to type 'T'.` TS2322 header.
    pub(super) fn render_type_predicate_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        ctx: &RenderContext,
        source_predicate: Option<&tsz_solver::TypePredicate>,
        target_predicate: &tsz_solver::TypePredicate,
        source_signature: Option<TypeId>,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();

        let own_message = match source_predicate {
            None => {
                let sig_str = source_signature
                    .and_then(|ty| {
                        self.ctx
                            .create_diagnostic_type_formatter()
                            .format_overload_signature(ty)
                    })
                    .unwrap_or_else(|| self.format_type_diagnostic(source));
                format_message(
                    diagnostic_messages::SIGNATURE_MUST_BE_A_TYPE_PREDICATE,
                    &[&sig_str],
                )
            }
            Some(source_pred) => {
                let source_str = self.format_type_predicate_for_diagnostic(source_pred);
                let target_str = self.format_type_predicate_for_diagnostic(target_predicate);
                format_message(
                    diagnostic_messages::TYPE_PREDICATE_IS_NOT_ASSIGNABLE_TO,
                    &[&source_str, &target_str],
                )
            }
        };

        // Both predicates narrow to a concrete type: tsc relates those types
        // directly beneath the TS1226 line (`Type 'S' is not assignable to
        // type 'T'.`), recursing through the same structured chain a plain
        // assignability failure would use.
        let predicate_type_pair = source_predicate
            .and_then(|p| p.type_id)
            .zip(target_predicate.type_id);

        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.push_elaboration(own_message, reason.diagnostic_code(), 0);
            if let Some(nested) = nested_reason
                && let Some((s_ty, t_ty)) = predicate_type_pair
            {
                let nested_diag = self.render_failure_reason(nested, s_ty, t_ty, idx, depth + 1);
                diag.push_elaboration_at(
                    nested_diag.file,
                    nested_diag.start,
                    nested_diag.length,
                    nested_diag.message_text,
                    nested_diag.code,
                    1,
                );
                diag.related_information
                    .extend(nested_diag.related_information);
            }
            diag
        } else {
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                own_message,
                reason.diagnostic_code(),
            );
            if let Some(nested) = nested_reason
                && let Some((s_ty, t_ty)) = predicate_type_pair
            {
                let nested_diag = self.render_failure_reason(nested, s_ty, t_ty, idx, depth + 1);
                diag.push_elaboration_at(
                    nested_diag.file,
                    nested_diag.start,
                    nested_diag.length,
                    nested_diag.message_text,
                    nested_diag.code,
                    0,
                );
                diag.related_information
                    .extend(nested_diag.related_information);
            }
            diag
        }
    }

    /// Locate the span of an excess property name within a source expression.
    ///
    /// Walks any surrounding parenthesized expression, `||`/`??`/`,` combinator,
    /// or conditional `? :` to reach the object literal that declares the
    /// property and returns the span of that property's name token. tsc
    /// underlines the property (e.g. `b` in `{ a: '', b: 123 } || ...`) rather
    /// than the containing literal's `{`; preserving that anchor is required
    /// for TS2353 fingerprint parity.
    pub(crate) fn find_excess_property_anchor(
        &self,
        idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<(u32, u32)> {
        use tsz_parser::parser::syntax_kind_ext;
        const MAX_DEPTH: u32 = 8;
        // Stack holds (node, depth). Popping left-before-right requires pushing
        // right first (LIFO) so the leftmost operand is inspected first — matches
        // tsc's left-to-right property enumeration for `||` / `??` / `,`.
        let mut stack: Vec<(NodeIndex, u32)> = vec![(idx, 0)];
        while let Some((current, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                if let Some(span) =
                    self.excess_property_name_span_in_literal(current, property_name)
                {
                    return Some(span);
                }
                continue;
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                stack.push((paren.expression, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
            {
                stack.push((bin.right, depth + 1));
                stack.push((bin.left, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            {
                stack.push((cond.when_false, depth + 1));
                stack.push((cond.when_true, depth + 1));
                continue;
            }
        }
        None
    }

    pub(crate) fn excess_property_name_display_for_site(
        &self,
        idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        const MAX_DEPTH: u32 = 8;
        let mut stack: Vec<(NodeIndex, u32)> = vec![(idx, 0)];
        while let Some((current, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                if let Some(display) =
                    self.excess_property_name_display_in_literal(current, property_name)
                {
                    return Some(display);
                }
                continue;
            }
            // The excess emit may anchor directly on the offending property
            // element (e.g. mapped-type targets report on the property element,
            // not the enclosing literal), so resolve the name from there too.
            if let Some(name_idx) = self.property_element_name_idx(node)
                && let Some(display) =
                    self.property_name_node_source_display(name_idx, property_name)
            {
                return Some(display);
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                stack.push((paren.expression, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
            {
                stack.push((bin.right, depth + 1));
                stack.push((bin.left, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            {
                stack.push((cond.when_false, depth + 1));
                stack.push((cond.when_true, depth + 1));
                continue;
            }
        }
        None
    }

    pub(super) fn excess_property_name_span_in_literal(
        &self,
        literal_idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<(u32, u32)> {
        use tsz_parser::parser::syntax_kind_ext;
        let node = self.ctx.arena.get(literal_idx)?;
        let literal = self.ctx.arena.get_literal_expr(node)?;
        for &elem in &literal.elements.nodes {
            let elem_node = self.ctx.arena.get(elem)?;
            if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_property_assignment(elem_node)
                && self.property_name_matches_atom(prop.name, property_name)
            {
                return self.property_name_span(prop.name);
            }
            if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node)
                && self.property_name_matches_atom(prop.name, property_name)
            {
                return self.property_name_span(prop.name);
            }
            if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
                && let Some(method) = self.ctx.arena.get_method_decl(elem_node)
                && self.property_name_matches_atom(method.name, property_name)
            {
                return self.property_name_span(method.name);
            }
        }
        None
    }

    fn excess_property_name_display_in_literal(
        &self,
        literal_idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let node = self.ctx.arena.get(literal_idx)?;
        let literal = self.ctx.arena.get_literal_expr(node)?;
        for &elem in &literal.elements.nodes {
            let elem_node = self.ctx.arena.get(elem)?;
            let Some(name_idx) = self.property_element_name_idx(elem_node) else {
                continue;
            };

            if let Some(display) = self.property_name_node_source_display(name_idx, property_name) {
                return Some(display);
            }
        }
        None
    }

    /// The property-name node of an object-literal member (property assignment,
    /// shorthand, or method), if `node` is one. Shared by the literal-walk and
    /// property-element-anchored excess-property name resolution paths.
    fn property_element_name_idx(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
            self.ctx
                .arena
                .get_property_assignment(node)
                .map(|prop| prop.name)
        } else if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
            self.ctx
                .arena
                .get_shorthand_property(node)
                .map(|prop| prop.name)
        } else if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            self.ctx
                .arena
                .get_method_decl(node)
                .map(|method| method.name)
        } else {
            None
        }
    }

    /// Render a property-name node by its source text when `tsc` would — a
    /// computed name (`[sym]`) keeps its brackets and a string-literal name
    /// (`'someKey'`) keeps its quotes, so the excess-property message reads
    /// `''someKey''` (outer quotes from the diagnostic template, inner from the
    /// literal). Identifier and numeric keys fall through to `None` so the caller
    /// uses the interned-atom default. `name_idx` is the property-name node.
    fn property_name_node_source_display(
        &self,
        name_idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let name_node = self.ctx.arena.get(name_idx)?;
        if !self.property_name_matches_atom(name_idx, property_name) {
            return None;
        }
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let display = self
                .get_source_text_for_node(name_idx)
                .trim()
                .trim_end_matches(':')
                .trim_end()
                .to_string();
            return (!display.is_empty()).then_some(display);
        }
        // A string-literal property name: recover the quoted source text the
        // interned atom dropped. Gated on a quote-prefixed source so it never
        // fires for identifier or numeric keys regardless of node-kind encoding.
        if self.ctx.arena.get_literal(name_node).is_some() {
            let display = self.get_source_text_for_node(name_idx).trim().to_string();
            if display.len() >= 2 && (display.starts_with('\'') || display.starts_with('"')) {
                return Some(display);
            }
        }
        None
    }

    pub(super) fn property_name_matches_atom(
        &self,
        name_idx: NodeIndex,
        target: tsz_common::interner::Atom,
    ) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        let resolved = self.ctx.types.resolve_atom_ref(target);
        let target_str: &str = &resolved;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return ident.escaped_text.as_str() == target_str;
        }
        if let Some(literal) = self.ctx.arena.get_literal(name_node) {
            return literal.text.as_str() == target_str;
        }
        if name_node.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
        {
            if let Some(suffix) = target_str.strip_prefix("__unique_")
                && let Ok(target_symbol_id) = suffix.parse::<u32>()
                && let Some(local_symbol_id) = self.resolve_identifier_symbol(computed.expression)
            {
                let symbol_id = self
                    .ctx
                    .resolve_import_alias_and_register(local_symbol_id)
                    .unwrap_or(local_symbol_id);
                if symbol_id.0 == target_symbol_id {
                    return true;
                }
            }
            return self
                .computed_property_expression_name_atom(computed.expression)
                .is_some_and(|resolved| resolved == target);
        }
        false
    }

    pub(super) fn property_name_span(&self, name_idx: NodeIndex) -> Option<(u32, u32)> {
        let node = self.ctx.arena.get(name_idx)?;
        Some((node.pos, node.end.saturating_sub(node.pos)))
    }

    /// Return the most-specific of the three target candidates that is an
    /// intersection type, in priority order `evaluated > target > target_type`.
    /// Returns `None` when none is an intersection.
    ///
    /// Shared by single-property (`render_missing_property`) and multi-property
    /// (`render_missing_properties`) intersection fall-back paths so the candidate
    /// priority stays consistent.
    pub(super) fn resolve_intersection_target_for_display(
        &mut self,
        target_type: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<TypeId> {
        self.resolve_intersection_target_for_display_kind(target_type, target, anchor_idx)
            .map(|(intersection, _recovered)| intersection)
    }

    /// Like [`Self::resolve_intersection_target_for_display`] but also reports
    /// whether the intersection was a genuine `Intersection` type (`false`) or
    /// was recovered from a merged object's display alias (`true`). The
    /// recovered case must render its top-level target from the written
    /// annotation, since the merged object's display alias does not preserve the
    /// user's alias name (`PlainWrap`) or the inline-vs-named form tsc echoes.
    pub(super) fn resolve_intersection_target_for_display_kind(
        &mut self,
        target_type: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<(TypeId, bool)> {
        // A merged multi-declaration interface reference (e.g. the lib's
        // `Map<K, V>`) evaluates to an intersection of its per-declaration
        // shapes, but tsc reports a missing-property failure against it as ONE
        // named interface surface (flat TS2740/TS2739/TS2741 naming
        // `Map<string, number>`), never with the intersection downgrade or the
        // per-member elaboration this helper drives.
        if [target, target_type].into_iter().any(|t| {
            diagnostic_query::is_interface_reference(self.ctx.types, &self.ctx.definition_store, t)
        }) {
            return None;
        }
        let evaluated = self.evaluate_type_with_env(target);
        if let Some(direct) = [evaluated, target, target_type]
            .into_iter()
            .find(|&t| diagnostic_query::is_intersection_type(self.ctx.types, t))
        {
            return Some((direct, false));
        }
        // tsz eagerly merges concrete object-intersections (`{ a } & { b }`) into
        // a single object (`{ a; b }`) so member lookup is O(1). The merged object
        // carries the `INTERSECTION_MERGED` shape flag and a stable
        // `merged_intersection_origin` back to the original `Intersection`.
        // Recover that intersection here so a missing-property mismatch is
        // elaborated member-by-member like tsc ("required in type 'B'") with the
        // top-level `TS2322`, instead of collapsing to the merged object and
        // reporting a flat `TS2741`/`TS2739` against `{ a; b }`.
        //
        // The flag is part of the merged object's identity, so it never aliases a
        // plain object literal of the same shape — it is the reliable structural
        // signal that the target genuinely is an intersection, regardless of how
        // it was produced (a written `A & B`, a generic instantiation
        // `Wrap<X> = X & B`, an alias, a conditional, or an `infer` capture). The
        // origin map (unlike the display alias, which a later `Application`
        // evaluation repaints) always yields the original members for the
        // elaboration. A written intersection annotation is honored as a fallback
        // for the rare resolver gap where the flagged merge sits behind a defer
        // the diagnostic target does not observe.
        if let Some(origin) = [evaluated, target, target_type].into_iter().find_map(|t| {
            diagnostic_query::is_merged_intersection_object(self.ctx.types, t)
                .then(|| self.ctx.types.get_merged_intersection_origin(t))
                .flatten()
                .filter(|&origin| diagnostic_query::is_intersection_type(self.ctx.types, origin))
        }) {
            return Some((origin, true));
        }
        if !self.target_annotation_denotes_intersection(anchor_idx) {
            return None;
        }
        [evaluated, target, target_type].into_iter().find_map(|t| {
            let alias = self.ctx.types.get_display_alias(t)?;
            diagnostic_query::is_intersection_type(self.ctx.types, alias).then_some((alias, true))
        })
    }

    /// Top-level target string for a *recovered* (merged-object) intersection
    /// target, shared by the single- and multi-missing render paths.
    ///
    /// The merged object's display alias does not preserve the user's alias name
    /// or the inline-vs-named form tsc echoes, so render from the written
    /// annotation: an inline `A & B` literal keeps the structural `&` form (the
    /// recovered intersection), while a type-alias reference keeps the alias
    /// name. (Genuine, non-recovered intersections are rendered by the caller.)
    pub(super) fn recovered_intersection_top_level_display(
        &mut self,
        intersection: TypeId,
        target: TypeId,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if self.target_annotation_is_intersection_literal(anchor_idx) {
            self.format_type_diagnostic(intersection)
        } else {
            self.format_assignability_type_for_message(target, source)
        }
    }

    /// Return `true` when `target` or its evaluated form is an intersection type.
    ///
    /// Used as a boolean predicate when only a single candidate is available
    /// (e.g. `render_type_mismatch` where `target_type` is not in scope).
    pub(super) fn is_intersection_target(&mut self, target: TypeId) -> bool {
        let evaluated = self.evaluate_type_with_env(target);
        [evaluated, target]
            .into_iter()
            .any(|t| diagnostic_query::is_intersection_type(self.ctx.types, t))
    }
}
