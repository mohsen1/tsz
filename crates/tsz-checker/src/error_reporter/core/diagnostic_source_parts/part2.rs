impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn declared_identifier_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
        expr_display_type: TypeId,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::VARIABLE) {
            return None;
        }
        // Merged INTERFACE+VALUE: `get_type_of_symbol` returns the interface side.
        if symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
        {
            return None;
        }

        let declared_type = self.get_type_of_symbol(sym_id);
        if matches!(declared_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        if let Some(annotation_text) = self.declared_diagnostic_source_annotation_text(expr_idx)
            && let Some(declared_enum_symbol) = self
                .enum_symbol_from_enumish_type(declared_type)
                .or_else(|| self.enum_symbol_from_enumish_type(expr_display_type))
            && Some(declared_enum_symbol) == self.enum_symbol_from_enumish_type(target)
            && !annotation_text.contains(" | ")
            && !annotation_text.contains(" & ")
            && !annotation_text.contains('<')
        {
            return Some(self.format_declared_annotation_for_diagnostic(&annotation_text));
        }
        let expr_enum_display_type = if self
            .enum_symbol_from_enumish_type(expr_display_type)
            .is_some()
        {
            expr_display_type
        } else {
            declared_type
        };
        let expr_enum_symbol = self.enum_symbol_from_enumish_type(expr_enum_display_type);
        let target_enum_symbol = self.enum_symbol_from_enumish_type(target);
        if expr_enum_symbol.is_some()
            && target_enum_symbol.is_some()
            && expr_enum_symbol != target_enum_symbol
        {
            return Some(
                self.format_assignability_type_for_message(expr_enum_display_type, target),
            );
        }
        if self
            .declared_diagnostic_source_annotation_text(expr_idx)
            .is_some_and(|annotation_text| annotation_text.trim_start().starts_with("typeof "))
        {
            return None;
        }
        let type_query_alias_def_id = self.declared_source_type_query_alias_def_id(expr_idx);
        let prefer_declared_display = if declared_type == TypeId::ANY
            && expr_display_type != TypeId::ANY
        {
            let mut decl_idx = symbol.value_declaration;
            let mut decl_node = self.ctx.arena.get(decl_idx)?;
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && ext.parent.is_some()
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
            {
                decl_idx = ext.parent;
                decl_node = parent_node;
            }
            let is_control_flow_typed_any = self
                .ctx
                .arena
                .get_variable_declaration(decl_node)
                .is_some_and(|decl| {
                    decl.type_annotation.is_none()
                        && !self.ctx.arena.is_const_variable_declaration(decl_idx)
                        && match decl.initializer {
                            idx if idx.is_none() => true,
                            idx => {
                                let inner = self.ctx.arena.skip_parenthesized(idx);
                                inner.is_some()
                                    && self.ctx.arena.get(inner).is_some_and(|node| {
                                        node.kind == tsz_scanner::SyntaxKind::NullKeyword as u16
                                            || node.kind
                                                == tsz_scanner::SyntaxKind::UndefinedKeyword as u16
                                            || self.ctx.arena.get_identifier(node).is_some_and(
                                                |ident| ident.escaped_text == "undefined",
                                            )
                                    })
                            }
                        }
                });
            !is_control_flow_typed_any
        } else {
            // A type is strictly narrower when it is a subtype or when flow
            // eliminated declared union members; the subset check handles
            // surviving members structurally compatible with eliminated ones.
            let expr_is_assignability_narrower = expr_display_type != declared_type
                && self
                    .diagnostic_source_narrowing_relation_outcome(expr_display_type, declared_type)
                    .related
                && !self
                    .diagnostic_source_narrowing_relation_outcome(declared_type, expr_display_type)
                    .related;
            let expr_is_union_subset_narrower = expr_display_type != declared_type
                && self.is_strict_union_member_subset(expr_display_type, declared_type);
            !(expr_is_assignability_narrower || expr_is_union_subset_narrower)
        };

        // If flow narrowing narrowed a nullable union to specifically null or
        // undefined, don't override with the broader declared type. For example,
        // `x: number | null` narrowed to `null` should show
        // "Type 'null' is not assignable to type 'string'", not
        // "Type 'number' is not assignable to type 'string'" (which happens
        // because strip_nullish_for_assignability_display strips the null member
        // when the target is non-nullable, leaving only "number").
        if (expr_display_type == TypeId::NULL || expr_display_type == TypeId::UNDEFINED)
            && expr_display_type != declared_type
            && let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, declared_type)
            && members.contains(&expr_display_type)
        {
            return None;
        }

        if let Some(display) = self.identifier_array_object_literal_source_display(expr_idx, target)
        {
            return Some(display);
        }
        if let Some(display) = self.identifier_literal_initializer_source_display(expr_idx, target)
        {
            return Some(display);
        }
        if prefer_declared_display
            && let Some(display) =
                self.declared_numeric_literal_union_alias_source_display(expr_idx, declared_type)
        {
            return Some(display);
        }
        if prefer_declared_display
            && let Some(display) =
                self.recursive_alias_application_source_display(expr_idx, declared_type)
        {
            return Some(display);
        }
        if let Some(display) = self.narrowed_string_literal_residual_union_display(
            declared_type,
            expr_display_type,
            target,
        ) {
            return Some(display);
        }
        if let Some(display) = self.rebuilt_array_source_display(declared_type, target) {
            return Some(display);
        }
        if let Some(display) =
            self.broad_mapped_index_signature_source_display(declared_type, target)
        {
            return Some(display);
        }

        // Preserve literal property types from declared annotations while
        // leaving fresh object-literal display_properties to the widening path.
        if prefer_declared_display
            && self
                .ctx
                .types
                .get_display_properties(declared_type)
                .is_none()
        {
            let widened =
                crate::query_boundaries::common::widen_type(self.ctx.types, declared_type);
            if widened != declared_type {
                let literal_display =
                    self.format_assignability_type_for_message(declared_type, target);
                let widened_display = self.format_assignability_type_for_message(widened, target);
                if literal_display != widened_display {
                    return Some(literal_display);
                }
            }
        }

        if prefer_declared_display
            && declared_type != expr_display_type
            && crate::query_boundaries::diagnostics::finite_mapped_property_surface(
                self.ctx.types,
                declared_type,
            )
            && !diagnostic_query::type_has_displayable_name(self.ctx.types, target)
        {
            return Some(self.format_type_diagnostic(declared_type));
        }

        let mut declared_display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(declared_type));
        let expr_display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_display_type));
        if self.ctx.compiler_options.exact_optional_property_types
            && (crate::query_boundaries::common::callable_shape_for_type(
                self.ctx.types,
                declared_type,
            )
            .is_some_and(|shape| {
                shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| !sig.type_params.is_empty())
            }) || crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                declared_type,
            )
            .is_some_and(|shape| !shape.type_params.is_empty()))
        {
            declared_display_type = declared_type;
        }
        let declared_is_generic_callable = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            declared_display_type,
        )
        .is_some_and(|shape| {
            shape
                .call_signatures
                .iter()
                .chain(shape.construct_signatures.iter())
                .any(|sig| !sig.type_params.is_empty())
        })
            || crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                declared_display_type,
            )
            .is_some_and(|shape| !shape.type_params.is_empty());
        if declared_is_generic_callable
            && let Some(annotation_text) = self.declared_diagnostic_source_annotation_text(expr_idx)
        {
            if self.ctx.compiler_options.exact_optional_property_types
                && prefer_declared_display
                && annotation_text.contains("?:")
            {
                return Some(self.format_declared_annotation_for_diagnostic(&annotation_text));
            }
            // Check if this is a single-call-signature OR single-construct-signature
            // callable that tsc displays in arrow syntax (e.g., `<S>() => S[]` or
            // `new <T>(x: T) => T`). For these, skip annotation text and use the
            // TypeFormatter which correctly produces arrow syntax.
            let should_use_arrow_syntax = crate::query_boundaries::common::callable_shape_for_type(
                self.ctx.types,
                declared_display_type,
            )
            .is_some_and(|shape| {
                let single_call =
                    shape.call_signatures.len() == 1 && shape.construct_signatures.is_empty();
                let single_construct =
                    shape.construct_signatures.len() == 1 && shape.call_signatures.is_empty();
                (single_call || single_construct)
                    && shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            });
            if !should_use_arrow_syntax {
                let annotation_display =
                    self.format_declared_annotation_for_diagnostic(&annotation_text);
                let expr_display =
                    self.format_assignability_type_for_message(expr_display_type, target);
                if prefer_declared_display && annotation_display != expr_display {
                    return Some(annotation_display);
                }
            }
        }
        let declared_display = if let Some(def_id) = type_query_alias_def_id {
            self.format_type_diagnostic_for_assignability_display_skipping_type_alias(
                declared_display_type,
                def_id,
            )
        } else if declared_is_generic_callable {
            let mut formatter =
                tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
                    .with_def_store(&self.ctx.definition_store)
                    .with_diagnostic_mode()
                    .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                    .with_exact_optional_property_types(
                        self.ctx.compiler_options.exact_optional_property_types,
                    );
            formatter.format(declared_display_type).into_owned()
        } else {
            self.format_assignability_type_for_message(declared_display_type, target)
        };
        let expr_display = self.format_assignability_type_for_message(expr_display_type, target);

        (prefer_declared_display && declared_display != expr_display).then(|| {
            self.canonicalize_assignment_numeric_literal_union_display(
                declared_type,
                target,
                declared_display,
            )
        })
    }

    pub(in crate::error_reporter) fn declared_numeric_literal_union_alias_source_display(
        &mut self,
        expr_idx: NodeIndex,
        declared_type: TypeId,
    ) -> Option<String> {
        let evaluated = self.evaluate_type_for_assignability(declared_type);
        if !diagnostic_query::is_number_literal_union(self.ctx.types, evaluated) {
            return None;
        }
        let annotation_text = self.declared_type_annotation_text_for_expression(expr_idx)?;
        Self::annotation_text_is_plain_type_reference(&annotation_text)
            .then(|| self.format_declared_annotation_for_diagnostic(&annotation_text))
    }

    /// Returns `true` when `narrowed`'s union members are a strict subset of
    /// `declared`'s union members. Single non-union types are treated as a
    /// one-element membership set against the declared union.
    ///
    /// This is the "narrowing eliminated some union members" check used by
    /// `declared_identifier_source_display` to recognise that flow narrowing
    /// produced a strictly smaller type even when the surviving member is
    /// structurally compatible with the eliminated ones (so plain
    /// `is_assignable_to(declared, narrowed)` returns true).
    pub(in crate::error_reporter) fn is_strict_union_member_subset(
        &mut self,
        narrowed: TypeId,
        declared: TypeId,
    ) -> bool {
        let Some(declared_members) =
            crate::query_boundaries::common::union_members(self.ctx.types, declared)
        else {
            return false;
        };
        if declared_members.len() < 2 {
            return false;
        }
        let narrowed_members =
            crate::query_boundaries::common::union_members(self.ctx.types, narrowed)
                .unwrap_or_else(|| vec![narrowed].into());
        if narrowed_members.is_empty() || narrowed_members.len() >= declared_members.len() {
            return false;
        }
        narrowed_members
            .iter()
            .all(|m| declared_members.contains(m))
    }

    fn narrowed_string_literal_residual_union_display(
        &mut self,
        declared_type: TypeId,
        expr_display_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        if target != TypeId::NEVER || declared_type == expr_display_type {
            return None;
        }
        let source_members =
            crate::query_boundaries::common::union_members(self.ctx.types, expr_display_type)?;
        let declared_members =
            crate::query_boundaries::common::union_members(self.ctx.types, declared_type)?;
        if source_members.len() < 2 || source_members.len() >= declared_members.len() {
            return None;
        }
        if !source_members.iter().all(|&member| {
            crate::query_boundaries::common::string_literal_value(self.ctx.types, member).is_some()
        }) || !declared_members.iter().all(|&member| {
            crate::query_boundaries::common::string_literal_value(self.ctx.types, member).is_some()
        }) {
            return None;
        }
        if !source_members
            .iter()
            .all(|member| declared_members.contains(member))
        {
            return None;
        }

        let mut ordered = source_members.to_vec();
        ordered.sort_by_key(|member| {
            std::cmp::Reverse(
                declared_members
                    .iter()
                    .position(|declared| declared == member)
                    .unwrap_or(usize::MAX),
            )
        });
        Some(
            ordered
                .into_iter()
                .map(|member| self.format_assignability_type_for_message(member, target))
                .collect::<Vec<_>>()
                .join(" | "),
        )
    }

    pub(in crate::error_reporter) fn rebuilt_array_source_display(
        &mut self,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        if let Some(display) = self.static_schema_array_structural_display(source_type, target) {
            return Some(display);
        }
        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, source_type)?;
        if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        let widened_element =
            self.normalize_assignability_display_type(self.widen_type_for_display(element_type));
        let rebuilt = self.ctx.types.array(widened_element);
        // Preserve the readonly modifier: tsc displays `readonly number[]` not `number[]`
        // when the source type was a readonly array (ReadonlyType(Array(...))).
        let rebuilt = if crate::query_boundaries::type_computation::complex::is_readonly_type(
            self.ctx.types,
            source_type,
        ) {
            self.ctx.types.readonly_type(rebuilt)
        } else {
            rebuilt
        };
        Some(self.format_assignability_type_for_message(rebuilt, target))
    }

    fn call_object_literal_intersection_source_display(
        &mut self,
        expr_idx: NodeIndex,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(node)?;
        let first_arg = *call.arguments.as_ref()?.nodes.first()?;
        let object_display = self.object_literal_source_type_display(first_arg, Some(target))?;

        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, source_type)?;
        let mut displays = Vec::with_capacity(members.len());
        let mut replaced_object_member = false;

        for &member in members.iter() {
            let evaluated = self.evaluate_type_for_assignability(member);
            let is_object_like_member =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
                    .is_some()
                    || crate::query_boundaries::common::get_merged_object_shape_for_type(
                        self.ctx.types,
                        evaluated,
                    )
                    .is_some();
            if !replaced_object_member && is_object_like_member {
                displays.push(object_display.clone());
                replaced_object_member = true;
            } else {
                displays.push(self.format_assignability_type_for_message(member, target));
            }
        }

        replaced_object_member.then(|| displays.join(" & "))
    }

    pub(in crate::error_reporter) fn has_more_specific_diagnostic_at_span(
        &self,
        start: u32,
        length: u32,
    ) -> bool {
        self.ctx.diagnostics.iter().any(|diag| {
            diag.start == start
                && diag.length == length
                && diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag.code
                    != diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV
        })
    }

    pub(crate) fn has_diagnostic_code_within_span(&self, start: u32, end: u32, code: u32) -> bool {
        self.ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == code && diag.start >= start && diag.start < end)
    }
}
