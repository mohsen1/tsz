//! Class/interface declaration checking (inheritance, implements, abstract members).

use crate::classes_domain::class_summary::ClassChainSummary;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_member_type_mismatch_bivariant,
};
use crate::query_boundaries::common::TypeSubstitution;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Extracted info about a single class member (property, method, or accessor).
#[derive(Clone)]
pub(crate) struct ClassMemberInfo {
    pub(crate) name: String,
    pub(crate) type_id: TypeId,
    pub(crate) name_idx: NodeIndex,
    pub(crate) visibility: MemberVisibility,
    pub(crate) is_method: bool,
    pub(crate) is_static: bool,
    pub(crate) is_accessor: bool,
    pub(crate) is_abstract: bool,
    pub(crate) has_override: bool,
    /// True when `override` comes from a JSDoc `@override` tag (not the keyword).
    /// Used to emit TS4118-4123 (JSDoc variants) instead of TS4112-4117.
    pub(crate) is_jsdoc_override: bool,
    pub(crate) has_dynamic_name: bool,
    /// True when the member name is a computed property whose expression is NOT
    /// a direct string/number literal. tsc uses this (`isComputedNonLiteralName`)
    /// to skip `noImplicitOverride` checks for computed names like `[someVar]`.
    pub(crate) has_computed_non_literal_name: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemberVisibility {
    Public,
    Protected,
    Private,
}

// =============================================================================
// Class and Interface Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Determine if a class member name is dynamic (e.g. `[foo]` or computed expressions),
    /// which cannot be used with an `override` modifier.
    ///
    /// In TypeScript, a computed property name is "late-bindable" (not dynamic) only when the
    /// expression's type resolves to a string/number literal or unique symbol. For example:
    /// - `const prop = "foo"` → type is `"foo"` (literal) → NOT dynamic
    /// - `let prop = "foo"` → type is `string` (widened) → dynamic
    /// - `const sym: symbol = Symbol()` → type is `symbol` → dynamic
    /// - `const sym = Symbol()` → type is `unique symbol` → NOT dynamic
    fn is_computed_name_dynamic(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return true;
        };
        if self.ctx.arena.get(computed.expression).is_none() {
            return true;
        }

        // Well-known symbols like Symbol.iterator are never dynamic
        if self
            .get_symbol_property_name_from_expr(computed.expression)
            .is_some()
        {
            return false;
        }

        self.is_computed_expression_dynamic(computed.expression)
    }

    fn is_computed_expression_dynamic(&mut self, expression_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expression_idx) else {
            return true;
        };
        let kind = expr_node.kind;

        // For identifiers, check the resolved TYPE — only string/number literals
        // and unique symbols are considered non-dynamic (late-bindable).
        if kind == SyntaxKind::Identifier as u16 {
            let expr_type = self.get_type_of_node(expression_idx);
            if crate::query_boundaries::checkers::property::is_type_usable_as_property_name(
                self.ctx.types,
                expr_type,
            ) {
                return false; // Literal or unique symbol → not dynamic
            }
            // Workaround: our solver doesn't yet infer unique symbols for `const x = Symbol()`.
            // When the identifier references a `const` without type annotation, tsc infers a
            // narrow type (literal/unique symbol), so we treat it as non-dynamic.
            if (expr_type == TypeId::SYMBOL
                || expr_type == TypeId::STRING
                || expr_type == TypeId::NUMBER)
                && let Some(sym_id) = self.resolve_identifier_symbol(expression_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.value_declaration.is_some()
            {
                let decl_idx = symbol.value_declaration;
                if self.ctx.arena.is_const_variable_declaration(decl_idx)
                    && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && let Some(decl_data) = self.ctx.arena.get_variable_declaration(decl_node)
                    && !decl_data.type_annotation.is_some()
                {
                    return false; // const without type annotation → non-dynamic
                }
            }
            return true; // Widened type (string/number/symbol with annotation or let/var) → dynamic
        }

        // String/number literals in computed names are always non-dynamic
        if matches!(
            kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) {
            return false;
        }

        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(expr_node)
        {
            return self.is_computed_expression_dynamic(paren.expression);
        }

        true
    }

    /// Check if a member name is a computed property whose expression is NOT a direct
    /// string/number literal. This matches tsc's `isComputedNonLiteralName()`.
    /// Used to skip `noImplicitOverride` checks for computed names like `[someVar]`.
    fn is_computed_non_literal_name(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return true;
        };
        let Some(expr_node) = self.ctx.arena.get(computed.expression) else {
            return true;
        };
        // Only direct string/number literals are considered "literal" computed names
        !matches!(
            expr_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        )
    }

    /// Collect base member names for override suggestions.
    #[allow(dead_code)]
    fn collect_base_member_names_for_override(
        &mut self,
        class_idx: NodeIndex,
        target_is_static: bool,
        members: &mut rustc_hash::FxHashSet<String>,
        visited: &mut rustc_hash::FxHashSet<NodeIndex>,
    ) {
        if !visited.insert(class_idx) {
            return;
        }

        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return;
        };

        for &member_idx in &class_data.members.nodes {
            if let Some(info) = self.extract_class_member_info(member_idx, true)
                && info.is_static == target_is_static
            {
                members.insert(info.name);
            }
        }

        if let Some(base_idx) = self.get_base_class_idx(class_idx) {
            self.collect_base_member_names_for_override(
                base_idx,
                target_is_static,
                members,
                visited,
            );
        }
    }

    /// Collect all property names from a type via the solver query boundary API.
    /// Used for type-level override checking when the base class is a complex
    /// expression (function call, intersection constructor).
    fn collect_property_names_from_type(
        &mut self,
        type_id: TypeId,
    ) -> rustc_hash::FxHashSet<String> {
        // Resolve Lazy types through the checker's type environment first
        let resolved = self.resolve_lazy_type(type_id);
        // Use the query boundary function which properly traverses Object/Intersection/Union
        let atoms =
            crate::query_boundaries::diagnostics::collect_property_name_atoms_for_diagnostics(
                self.ctx.types,
                resolved,
                5, // max_depth sufficient for class hierarchies
            );
        atoms
            .into_iter()
            .map(|atom| self.ctx.types.resolve_atom_ref(atom).to_string())
            .collect()
    }

    /// Check override members against a type-level base class (when AST resolution fails).
    /// Used for complex heritage expressions: function calls, intersection constructors, etc.
    /// Also checks property type compatibility (TS2416) when `base_instance_type` is provided.
    ///
    /// Uses `OwnMemberSummary` to avoid re-extracting member info; all members are
    /// pre-collected via `build_own_member_summary`.
    fn check_override_members_against_type(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        derived_class_name: &str,
        base_class_name: &str,
        base_member_names: &rustc_hash::FxHashSet<String>,
        no_implicit_override: bool,
        base_instance_type: Option<TypeId>,
    ) {
        use crate::query_boundaries::class::build_own_member_summary;

        let (_derived_type_params, derived_type_param_updates) =
            self.push_type_parameters(&class_data.type_parameters);

        let own = build_own_member_summary(self, class_data);
        let all_members: Vec<_> = own
            .all_instance_members
            .into_iter()
            .chain(own.all_static_members)
            .collect();

        for info in all_members {
            if info.name.starts_with('#') {
                continue;
            }

            if info.has_dynamic_name {
                if info.has_override {
                    self.error_at_node(
                        info.name_idx,
                        &crate::diagnostics::format_message(
                            diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                            &[],
                        ),
                        diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                    );
                }
                continue;
            }

            if info.has_override {
                if !base_member_names.contains(&info.name) {
                    // Member not found in base — check for suggestion
                    if let Some(suggestion) =
                        self.find_override_name_suggestion(base_member_names, &info.name)
                    {
                        self.error_at_node(
                            info.name_idx,
                            &crate::diagnostics::format_message(
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                                &[base_class_name, &suggestion],
                            ),
                            diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                        );
                    } else {
                        self.error_at_node(
                            info.name_idx,
                            &crate::diagnostics::format_message(
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                                &[base_class_name],
                            ),
                            diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                        );
                    }
                }
            } else if no_implicit_override
                && base_member_names.contains(&info.name)
                && !info.has_computed_non_literal_name
            {
                self.error_at_node(
                    info.name_idx,
                    &crate::diagnostics::format_message(
                        if self.ctx.is_js_file() {
                            diagnostic_messages::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                        } else {
                            diagnostic_messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE
                        },
                        &[base_class_name],
                    ),
                    if self.ctx.is_js_file() {
                        diagnostic_codes::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                    } else {
                        diagnostic_codes::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE
                    },
                );
            }

            // Check property type compatibility with base (TS2416)
            if let Some(base_type_id) = base_instance_type
                && base_member_names.contains(&info.name)
                && !info.is_static
            {
                let member_type = info.type_id;
                // Skip if either type is ANY
                if member_type != TypeId::ANY {
                    use crate::query_boundaries::common::PropertyAccessResult;
                    let base_prop_result =
                        self.resolve_property_access_with_env(base_type_id, &info.name);
                    if let PropertyAccessResult::Success {
                        type_id: base_type, ..
                    } = base_prop_result
                        && base_type != TypeId::ANY
                    {
                        let resolved_member_type = self.resolve_type_query_type(member_type);
                        let resolved_base_type = self.resolve_type_query_type(base_type);

                        let should_report = if info.is_method {
                            should_report_member_type_mismatch_bivariant(
                                self,
                                resolved_member_type,
                                resolved_base_type,
                                info.name_idx,
                            )
                        } else {
                            should_report_member_type_mismatch(
                                self,
                                resolved_member_type,
                                resolved_base_type,
                                info.name_idx,
                            )
                        };

                        if should_report {
                            let member_type_str = self.format_type(member_type);
                            let base_type_str = self.format_type(base_type);

                            self.error_at_node(
                                    info.name_idx,
                                    &format!(
                                        "Property '{}' in type '{}' is not assignable to the same property in base type '{}'.",
                                        info.name, derived_class_name, base_class_name
                                    ),
                                    diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                                );
                            self.report_type_not_assignable_detail(
                                    info.name_idx,
                                    &member_type_str,
                                    &base_type_str,
                                    diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                                );
                        }
                    }
                }
            }
        }

        // Also check constructor parameter properties
        self.check_constructor_parameter_property_overrides(
            class_data,
            None,
            None,
            base_class_name,
            derived_class_name,
            base_member_names,
            no_implicit_override,
        );

        self.pop_type_parameters(derived_type_param_updates);
    }

    /// Report errors for members with `override` in a class that has no base class.
    ///
    /// This consolidates the duplicated "no heritage" / "no resolved base" override
    /// checking patterns that were previously inlined in multiple places inside
    /// `check_property_inheritance_compatibility`. The `class_data` members are
    /// iterated via pre-built `OwnMemberSummary` when available; otherwise they
    /// are extracted inline.
    fn report_overrides_without_base(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        derived_class_name: &str,
        no_implicit_override: bool,
    ) {
        use crate::query_boundaries::class::build_own_member_summary;

        // Fast path: if noImplicitOverride is off, we only care about explicit `override`
        // modifiers. Do a quick scan of members to see if any have the override keyword
        // before building the expensive full member summary.
        if !no_implicit_override {
            let has_any_override = class_data.members.nodes.iter().any(|&member_idx| {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    return false;
                };
                match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(member_node)
                        .is_some_and(|p| self.has_override_modifier(&p.modifiers)),
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .ctx
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|m| self.has_override_modifier(&m.modifiers)),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.ctx
                            .arena
                            .get_accessor(member_node)
                            .is_some_and(|a| self.has_override_modifier(&a.modifiers))
                    }
                    _ => false,
                }
            });
            if !has_any_override {
                // No explicit override modifiers and noImplicitOverride is off.
                // Still need to check constructor parameter property overrides.
                self.check_constructor_parameter_property_overrides(
                    class_data,
                    None,
                    None,
                    derived_class_name,
                    derived_class_name,
                    &rustc_hash::FxHashSet::default(),
                    no_implicit_override,
                );
                return;
            }
        }

        let own = build_own_member_summary(self, class_data);

        // Check class body members
        let all_members: Vec<_> = own
            .all_instance_members
            .iter()
            .chain(own.all_static_members.iter())
            .collect();

        for info in &all_members {
            if !info.has_override {
                continue;
            }
            if info.has_dynamic_name {
                self.error_at_node(
                    info.name_idx,
                    &crate::diagnostics::format_message(
                        diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                        &[],
                    ),
                    diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                );
                continue;
            }
            self.error_at_node(
                info.name_idx,
                &crate::diagnostics::format_message(
                    if info.is_jsdoc_override {
                        diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_ITS_CONTAIN
                    } else {
                        diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N
                    },
                    &[derived_class_name],
                ),
                if info.is_jsdoc_override {
                    diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_ITS_CONTAIN
                } else {
                    diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N
                },
            );
        }

        // Also check constructor parameter properties
        self.check_constructor_parameter_property_overrides(
            class_data,
            None,
            None,
            derived_class_name,
            derived_class_name,
            &rustc_hash::FxHashSet::default(),
            no_implicit_override,
        );
    }

    /// Find a close member name from base class members for "Did you mean ...?".
    fn find_override_name_suggestion(
        &self,
        base_names: &rustc_hash::FxHashSet<String>,
        target_name: &str,
    ) -> Option<String> {
        let name_len = target_name.len();
        if base_names.is_empty() {
            return None;
        }

        // tsc does not suggest for very short names (≤ 3 chars) because the
        // edit distance threshold is too loose to produce meaningful matches.
        if name_len <= 3 {
            return None;
        }

        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        let mut best_distance = name_len * 4 / 10 + 1;
        let mut best_candidate: Option<String> = None;

        for candidate in base_names {
            if candidate == target_name {
                continue;
            }
            if name_len.abs_diff(candidate.len()) > maximum_length_difference {
                continue;
            }
            if candidate.len() < 3 && candidate.to_lowercase() != target_name.to_lowercase() {
                continue;
            }

            if let Some(distance) =
                Self::override_name_levenshtein_with_max(target_name, candidate, best_distance)
                && distance < best_distance
            {
                best_distance = distance;
                best_candidate = Some(candidate.clone());
            }
        }

        best_candidate
    }

    /// Compute edit distance with an upper bound, used for override suggestions.
    fn override_name_levenshtein_with_max(
        s1: &str,
        s2: &str,
        max_distance: usize,
    ) -> Option<usize> {
        if s1.len() > s2.len() {
            return Self::override_name_levenshtein_with_max(s2, s1, max_distance);
        }

        let (short, long) = (s1.as_bytes(), s2.as_bytes());
        let (short_len, long_len) = (short.len(), long.len());
        if long_len - short_len > max_distance {
            return None;
        }

        let mut previous: Vec<usize> = (0..=long_len).collect();
        let mut current: Vec<usize> = vec![0; long_len + 1];

        for (i, &lhs) in short.iter().enumerate() {
            current[0] = i + 1;
            let mut row_min = current[0];
            for (j, &rhs) in long.iter().enumerate() {
                let insert = previous[j + 1] + 1;
                let delete = current[j] + 1;
                let replace = previous[j] + usize::from(lhs != rhs);
                let value = insert.min(delete).min(replace);
                current[j + 1] = value;
                if value < row_min {
                    row_min = value;
                }
            }
            if row_min > max_distance {
                return None;
            }
            previous.copy_from_slice(&current);
        }

        let distance = previous[long_len];
        if distance <= max_distance {
            Some(distance)
        } else {
            None
        }
    }

    /// Report explicit/implicit override errors for constructor parameter properties.
    fn check_constructor_parameter_property_overrides(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        base_class_idx: Option<NodeIndex>,
        base_chain_summary: Option<&ClassChainSummary>,
        base_class_name: &str,
        derived_class_name: &str,
        base_instance_member_names: &rustc_hash::FxHashSet<String>,
        no_implicit_override: bool,
    ) {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }

            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if !self.has_parameter_property_modifier(&param.modifiers) {
                    continue;
                }
                let Some(param_name) = self.get_property_name(param.name) else {
                    continue;
                };

                let has_override = self.has_override_modifier(&param.modifiers)
                    || self.has_jsdoc_override_tag(param_idx);
                let base_member = match (base_class_idx, base_chain_summary) {
                    (Some(base_idx), Some(summary)) => {
                        let _ = base_idx;
                        summary.lookup(&param_name, false, true).cloned()
                    }
                    (Some(base_idx), None) => {
                        self.find_member_in_class_chain(base_idx, &param_name, false, 0, true)
                    }
                    (None, _) => None,
                };

                if has_override {
                    if base_class_idx.is_none() {
                        // tsc points at the parameter declaration (starting at the
                        // first modifier like 'public'), not just the identifier name.
                        // Use ctx.error() directly to bypass normalized_anchor_span
                        // which would strip modifiers and point at just the name.
                        self.ctx.error(
                            param_node.pos,
                            param_node.end - param_node.pos,
                            crate::diagnostics::format_message(
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                                &[base_class_name],
                            ),
                            diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                        );
                        continue;
                    }

                    if base_member.is_none() {
                        // tsc points at the parameter declaration (starting at the
                        // first modifier like 'public'), not just the identifier name.
                        if let Some(suggestion) = self
                            .find_override_name_suggestion(base_instance_member_names, &param_name)
                        {
                            self.ctx.error(
                                param_node.pos,
                                param_node.end - param_node.pos,
                                crate::diagnostics::format_message(
                                    diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                                    &[base_class_name, &suggestion],
                                ),
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                            );
                        } else {
                            self.ctx.error(
                                param_node.pos,
                                param_node.end - param_node.pos,
                                crate::diagnostics::format_message(
                                    diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                                    &[base_class_name],
                                ),
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                            );
                        }
                    }
                } else if no_implicit_override && base_member.is_some() {
                    // tsc points TS4115 at the parameter declaration (starting at the
                    // first modifier like 'public'), not just the identifier name.
                    self.ctx.error(
                        param_node.pos,
                        param_node.end - param_node.pos,
                        crate::diagnostics::format_message(
                            diagnostic_messages::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME,
                            &[base_class_name],
                        ),
                        diagnostic_codes::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME,
                    );
                }

                // TS2610: constructor parameter property overrides a base accessor
                // A parameter property like `constructor(public p: string)` acts as an
                // instance property. If the base class defines the same name as an
                // accessor (get/set), this is an accessor/property kind mismatch.
                if let Some(ref base_info) = base_member
                    && base_info.is_accessor
                    && !base_info.is_abstract
                {
                    self.error_at_node(
                            param.name,
                            &format!(
                                "'{param_name}' is defined as an accessor in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an instance property."
                            ),
                            diagnostic_codes::IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_INSTANCE_PROP,
                        );
                }
            }
        }
    }

    /// Extract name, type, and flags from a class member node.
    ///
    /// If `skip_private` is true, returns `None` for private members.
    pub(crate) fn extract_class_member_info(
        &mut self,
        member_idx: NodeIndex,
        skip_private: bool,
    ) -> Option<ClassMemberInfo> {
        let member_node = self.ctx.arena.get(member_idx)?;
        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(member_node)?;
                let name = self.get_property_name(prop.name)?;
                if skip_private && self.has_private_modifier(&prop.modifiers) {
                    return None;
                }
                let visibility = if self.has_private_modifier(&prop.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&prop.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };
                let is_static = self.has_static_modifier(&prop.modifiers);
                let is_accessor = self.has_accessor_modifier(&prop.modifiers);
                let prop_type = if let Some(declared_type) =
                    self.effective_class_property_declared_type(member_idx, prop)
                {
                    declared_type
                } else if prop.initializer.is_some() {
                    // Use cached initializer type if available. Calling get_type_of_node
                    // when no cache exists can trigger false diagnostics (e.g., TS2551)
                    // if this method is invoked during constructor type building: the
                    // this_type_stack may contain the constructor type rather than the
                    // instance type, causing `this.prop` in instance initializers to
                    // resolve against the static side. If no cache exists and we're
                    // outside the member-checking context, use ANY.
                    let init_type =
                        if let Some(&cached) = self.ctx.node_types.get(&prop.initializer.0) {
                            cached
                        } else if !is_static && self.ctx.enclosing_class.is_none() {
                            // Instance property initializer evaluated outside of
                            // member-checking context (e.g., during class summary
                            // construction triggered by constructor type building).
                            // The this_type_stack is unreliable here — use ANY.
                            TypeId::ANY
                        } else {
                            self.get_type_of_node(prop.initializer)
                        };
                    if self.has_readonly_modifier(&prop.modifiers) {
                        init_type
                    } else {
                        self.widen_literal_type(init_type)
                    }
                } else {
                    TypeId::ANY
                };
                let is_abstract = self.has_abstract_modifier(&prop.modifiers);
                Some(ClassMemberInfo {
                    name,
                    type_id: prop_type,
                    name_idx: prop.name,
                    visibility,
                    is_method: false,
                    is_static,
                    is_accessor,
                    is_abstract,
                    has_override: self.has_override_modifier(&prop.modifiers)
                        || self.has_jsdoc_override_tag(member_idx),
                    is_jsdoc_override: !self.has_override_modifier(&prop.modifiers)
                        && self.has_jsdoc_override_tag(member_idx),
                    has_dynamic_name: self.is_computed_name_dynamic(prop.name),
                    has_computed_non_literal_name: self.is_computed_non_literal_name(prop.name),
                })
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(member_node)?;
                let name = match self.get_property_name(method.name) {
                    Some(n) => n,
                    None => {
                        // Computed property — try type-based resolution for late-bindable
                        // names like `[prop]` where `const prop = "foo"`.
                        if let Some(resolved) = self.get_property_name_resolved(method.name) {
                            // Name resolved (e.g., const string) — use the resolved name
                            // and let normal override logic handle TS4113 vs OK.
                            resolved
                        } else {
                            // Truly unresolvable name (e.g., [sym] where sym is a Symbol).
                            // We still need to check override + dynamic name (TS4127).
                            let has_override = self.has_override_modifier(&method.modifiers)
                                || self.has_jsdoc_override_tag(member_idx);
                            if has_override {
                                return Some(ClassMemberInfo {
                                    name: String::from("__computed"),
                                    type_id: TypeId::ANY,
                                    name_idx: method.name,
                                    visibility: MemberVisibility::Public,
                                    is_method: true,
                                    is_static: self.has_static_modifier(&method.modifiers),
                                    is_accessor: false,
                                    is_abstract: self.has_abstract_modifier(&method.modifiers),
                                    has_override: true,
                                    is_jsdoc_override: !self
                                        .has_override_modifier(&method.modifiers)
                                        && self.has_jsdoc_override_tag(member_idx),
                                    has_dynamic_name: true,
                                    has_computed_non_literal_name: true,
                                });
                            }
                            return None;
                        }
                    }
                };
                if skip_private && self.has_private_modifier(&method.modifiers) {
                    return None;
                }
                let visibility = if self.has_private_modifier(&method.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&method.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };
                let is_static = self.has_static_modifier(&method.modifiers);
                let factory = self.ctx.types.factory();
                use tsz_solver::FunctionShape;
                let signature = self.call_signature_from_method(method, member_idx);
                let method_type = factory.function(FunctionShape {
                    type_params: signature.type_params,
                    params: signature.params,
                    this_type: signature.this_type,
                    return_type: signature.return_type,
                    type_predicate: signature.type_predicate,
                    is_constructor: false,
                    is_method: true,
                });
                let is_abstract = self.has_abstract_modifier(&method.modifiers);
                Some(ClassMemberInfo {
                    name,
                    type_id: method_type,
                    name_idx: method.name,
                    visibility,
                    is_method: true,
                    is_static,
                    is_accessor: false,
                    is_abstract,
                    has_override: self.has_override_modifier(&method.modifiers)
                        || self.has_jsdoc_override_tag(member_idx),
                    is_jsdoc_override: !self.has_override_modifier(&method.modifiers)
                        && self.has_jsdoc_override_tag(member_idx),
                    has_dynamic_name: self.is_computed_name_dynamic(method.name),
                    has_computed_non_literal_name: self.is_computed_non_literal_name(method.name),
                })
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node)?;
                let name = self.get_property_name(accessor.name)?;
                if skip_private && self.has_private_modifier(&accessor.modifiers) {
                    return None;
                }
                let visibility = if self.has_private_modifier(&accessor.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&accessor.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };
                let is_static = self.has_static_modifier(&accessor.modifiers);
                let accessor_type = if accessor.type_annotation.is_some() {
                    self.get_type_from_type_node(accessor.type_annotation)
                } else {
                    self.infer_getter_return_type(accessor.body)
                };
                let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                Some(ClassMemberInfo {
                    name,
                    type_id: accessor_type,
                    name_idx: accessor.name,
                    visibility,
                    is_method: false,
                    is_static,
                    is_accessor: true,
                    is_abstract,
                    has_override: self.has_override_modifier(&accessor.modifiers)
                        || self.has_jsdoc_override_tag(member_idx),
                    is_jsdoc_override: !self.has_override_modifier(&accessor.modifiers)
                        && self.has_jsdoc_override_tag(member_idx),
                    has_dynamic_name: self.is_computed_name_dynamic(accessor.name),
                    has_computed_non_literal_name: self.is_computed_non_literal_name(accessor.name),
                })
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node)?;
                let name = self.get_property_name(accessor.name)?;
                if skip_private && self.has_private_modifier(&accessor.modifiers) {
                    return None;
                }
                let visibility = if self.has_private_modifier(&accessor.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&accessor.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };
                let is_static = self.has_static_modifier(&accessor.modifiers);
                let accessor_type = accessor
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&p| self.ctx.arena.get_parameter_at(p))
                    .map_or(TypeId::ANY, |param| {
                        if param.type_annotation.is_some() {
                            self.get_type_from_type_node(param.type_annotation)
                        } else {
                            TypeId::ANY
                        }
                    });
                let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                Some(ClassMemberInfo {
                    name,
                    type_id: accessor_type,
                    name_idx: accessor.name,
                    visibility,
                    is_method: false,
                    is_static,
                    is_accessor: true,
                    is_abstract,
                    has_override: self.has_override_modifier(&accessor.modifiers)
                        || self.has_jsdoc_override_tag(member_idx),
                    is_jsdoc_override: !self.has_override_modifier(&accessor.modifiers)
                        && self.has_jsdoc_override_tag(member_idx),
                    has_dynamic_name: self.is_computed_name_dynamic(accessor.name),
                    has_computed_non_literal_name: self.is_computed_non_literal_name(accessor.name),
                })
            }
            _ => None,
        }
    }

    // =========================================================================
    // Inheritance Checking
    // =========================================================================

    /// Check that property types in derived class are compatible with base class (error 2416).
    /// For each property/accessor in the derived class, checks if there's a corresponding
    /// member in the base class with incompatible type.
    pub(crate) fn check_property_inheritance_compatibility(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};

        // Find base class from heritage clauses (extends, not implements)
        // If there are no heritage clauses, we still need to check for
        // invalid `override` members (TS4112) since override requires extends.
        let heritage_clauses = match class_data.heritage_clauses {
            Some(ref hc) => hc,
            None => {
                // No heritage clauses — still check for override members (TS4112)
                let derived_class_name = if class_data.name.is_some() {
                    self.ctx
                        .arena
                        .get(class_data.name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map_or_else(
                            || String::from("(Anonymous class)"),
                            |id| id.escaped_text.clone(),
                        )
                } else {
                    String::from("(Anonymous class)")
                };
                self.report_overrides_without_base(
                    class_data,
                    &derived_class_name,
                    self.ctx.no_implicit_override(),
                );
                return;
            }
        };

        let mut base_class_idx: Option<NodeIndex> = None;
        let mut base_class_name = String::new();
        let mut base_type_argument_nodes: Option<Vec<NodeIndex>> = None;
        // Save heritage expression info for type-level fallback when AST resolution fails
        let mut heritage_expr_idx: Option<NodeIndex> = None;
        let mut heritage_type_idx: Option<NodeIndex> = None;
        // Track the base class symbol for namespace-merged static type check (TS2417).
        // Set when the heritage clause resolves to a class symbol. The actual TS2417
        // check only fires when the *derived* class has a merged namespace.
        let mut base_sym_for_ns_static_check: Option<tsz_binder::SymbolId> = None;

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (token = ExtendsKeyword = 96)
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause (the base class)
            if let Some(&type_idx) = heritage.types.nodes.first()
                && let Some(type_node) = self.ctx.arena.get(type_idx)
            {
                // Handle both cases:
                // 1. ExpressionWithTypeArguments (e.g., Base<T>)
                // 2. Simple Identifier (e.g., Base)
                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        // For simple identifiers without type arguments, the type_node itself is the identifier
                        (type_idx, None)
                    };
                heritage_expr_idx = Some(expr_idx);
                heritage_type_idx = Some(type_idx);
                if let Some(args) = type_arguments {
                    base_type_argument_nodes = Some(args.nodes.clone());
                }

                // Unwrap parenthesized expressions to find the actual base expression.
                // e.g., `class E extends (class { ... })` — the inner expr is a class expression.
                let mut resolved_expr_idx = expr_idx;
                while let Some(rn) = self.ctx.arena.get(resolved_expr_idx) {
                    if rn.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                        && let Some(paren) = self.ctx.arena.get_parenthesized(rn)
                    {
                        resolved_expr_idx = paren.expression;
                        continue;
                    }
                    break;
                }

                // Check if the base expression is a class expression directly
                let resolved_node = self.ctx.arena.get(resolved_expr_idx);
                let is_class_expr =
                    resolved_node.is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION);

                if is_class_expr {
                    // Direct class expression as base — use it directly
                    base_class_idx = Some(resolved_expr_idx);
                    if let Some(rn) = resolved_node
                        && let Some(cls) = self.ctx.arena.get_class(rn)
                        && cls.name.is_some()
                        && let Some(name_node) = self.ctx.arena.get(cls.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        base_class_name = ident.escaped_text.clone();
                    } else {
                        base_class_name = String::from("(Anonymous class)");
                    }
                } else {
                    // Get the class name from the expression (identifier)
                    if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                    {
                        base_class_name = ident.escaped_text.clone();
                    }

                    // Find the base class declaration via heritage symbol resolution
                    // This handles namespace scoping correctly
                    if let Some(sym_id) = self.resolve_heritage_symbol(expr_idx) {
                        // Track the base symbol for the namespace-merged static check (TS2417).
                        // Always store the base symbol here; the check at line ~1731 only
                        // fires when the *derived* class has a merged namespace (which is the
                        // condition that can make `typeof Derived` incompatible with
                        // `typeof Base`). Previously we only stored the symbol when the
                        // *base* had a namespace, but tsc also reports TS2417 when the
                        // derived class's namespace introduces conflicting static members
                        // even if the base class has no namespace at all.
                        base_sym_for_ns_static_check = Some(sym_id);
                        // Resolve to an in-arena class declaration when possible.
                        // Cross-file/module heritage often resolves to symbols whose
                        // declaration nodes live in another arena; returning `None`
                        // here is intentional so the type-level fallback path can
                        // handle the base class structurally.
                        base_class_idx = self.get_class_declaration_from_symbol(sym_id);
                    }
                }
            }
            break; // Only one extends clause is valid
        }

        let derived_class_name = if class_data.name.is_some() {
            if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    let mut name = ident.escaped_text.clone();
                    // Append type parameters for tsc parity: "Foo<T, U>"
                    self.append_type_param_names(&mut name, &class_data.type_parameters);
                    name
                } else {
                    String::from("(Anonymous class)")
                }
            } else {
                String::from("(Anonymous class)")
            }
        } else {
            String::from("(Anonymous class)")
        };
        // tsc does not enforce noImplicitOverride in ambient/declare class declarations.
        let is_ambient_class = self.has_declare_modifier(&class_data.modifiers);
        let no_implicit_override = self.ctx.no_implicit_override() && !is_ambient_class;

        let Some(base_idx) = base_class_idx else {
            // No AST-level class declaration found. Try type-level fallback for complex
            // heritage expressions (function calls, intersection types, etc.).
            if let Some(h_expr_idx) = heritage_expr_idx {
                let type_arguments = heritage_type_idx.and_then(|tidx| {
                    self.ctx
                        .arena
                        .get(tidx)
                        .and_then(|n| self.ctx.arena.get_expr_type_args(n))
                        .and_then(|e| e.type_arguments.as_ref())
                });
                if let Some(instance_type) =
                    self.base_instance_type_from_expression(h_expr_idx, type_arguments)
                {
                    let heritage_sym_id = self.resolve_heritage_symbol(h_expr_idx);
                    // Use intersection display name if available (preserves "I1 & I2"
                    // instead of showing merged "{ m1: ...; m2: ... }")
                    let type_base_name = self
                        .intersection_instance_display_name(h_expr_idx, type_arguments)
                        .or_else(|| {
                            heritage_sym_id.and_then(|sym_id| {
                                self.format_symbol_reference_with_type_arguments(
                                    sym_id,
                                    type_arguments,
                                )
                            })
                        })
                        .unwrap_or_else(|| self.format_type(instance_type));
                    let base_member_names = self.collect_property_names_from_type(instance_type);

                    self.check_override_members_against_type(
                        class_data,
                        &derived_class_name,
                        &type_base_name,
                        &base_member_names,
                        no_implicit_override,
                        Some(instance_type),
                    );
                    return;
                }
            }

            // True fallback: no extends clause resolved at all — emit TS4112
            self.report_overrides_without_base(
                class_data,
                &derived_class_name,
                no_implicit_override,
            );

            return;
        };

        // Get the base class data. If the resolved node is not a class declaration
        // (e.g., variable typed as intersection of constructors), use type-level fallback.
        let Some(base_node) = self.ctx.arena.get(base_idx) else {
            return;
        };

        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            // base_idx points to a non-class node (e.g., variable declaration).
            // Fall back to type-level resolution via base_instance_type_from_expression.
            if let Some(h_expr_idx) = heritage_expr_idx {
                let type_arguments = heritage_type_idx.and_then(|tidx| {
                    self.ctx
                        .arena
                        .get(tidx)
                        .and_then(|n| self.ctx.arena.get_expr_type_args(n))
                        .and_then(|e| e.type_arguments.as_ref())
                });
                if let Some(instance_type) =
                    self.base_instance_type_from_expression(h_expr_idx, type_arguments)
                {
                    let heritage_sym_id = self.resolve_heritage_symbol(h_expr_idx);
                    let type_base_name = self
                        .intersection_instance_display_name(h_expr_idx, type_arguments)
                        .or_else(|| {
                            heritage_sym_id.and_then(|sym_id| {
                                self.format_symbol_reference_with_type_arguments(
                                    sym_id,
                                    type_arguments,
                                )
                            })
                        })
                        .unwrap_or_else(|| self.format_type(instance_type));
                    let base_member_names = self.collect_property_names_from_type(instance_type);

                    self.check_override_members_against_type(
                        class_data,
                        &derived_class_name,
                        &type_base_name,
                        &base_member_names,
                        no_implicit_override,
                        Some(instance_type),
                    );
                    return;
                }
            }
            return;
        };

        // Append type parameters to base class name for tsc parity: "A<T>"
        self.append_type_param_names(&mut base_class_name, &base_class.type_parameters);

        let (_derived_type_params, derived_type_param_updates) =
            self.push_type_parameters(&class_data.type_parameters);

        let mut type_args = Vec::new();
        if let Some(nodes) = base_type_argument_nodes.as_ref() {
            for arg_idx in nodes {
                type_args.push(self.get_type_from_type_node(*arg_idx));
            }
        }

        let (base_type_params, base_type_param_updates) =
            self.push_type_parameters(&base_class.type_parameters);
        if type_args.len() < base_type_params.len() {
            for param in base_type_params.iter().skip(type_args.len()) {
                let fallback = param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN);
                type_args.push(fallback);
            }
        }
        if type_args.len() > base_type_params.len() {
            type_args.truncate(base_type_params.len());
        }
        let mut substitution =
            TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);

        // When the extends clause has explicit type arguments, rebuild the base class name
        // with formatted type arguments (e.g., `Base<{ bar: string; }>`) instead of
        // generic parameter names (`Base<T>`). tsc shows the supplied type arguments.
        if base_type_argument_nodes.is_some() && !type_args.is_empty() {
            // Strip the previously appended type params (e.g., remove "<T>" from "Base<T>")
            if let Some(lt_pos) = base_class_name.find('<') {
                base_class_name.truncate(lt_pos);
            }
            let arg_strs: Vec<String> = type_args.iter().map(|&t| self.format_type(t)).collect();
            base_class_name.push('<');
            base_class_name.push_str(&arg_strs.join(", "));
            base_class_name.push('>');
        }

        // Base type parameters are only needed to build the extends-clause substitution here.
        self.pop_type_parameters(base_type_param_updates);

        // Compose substitutions through the entire inheritance chain.
        // The chain summary stores raw (uninstantiated) member types from ancestor classes.
        // For example, if L<RT> extends T<RT[RT['a']]> and T<A> has member a: A,
        // the chain summary stores a: A (T's raw type param). The initial substitution
        // only maps RT -> X_type, leaving A unresolved. We need to also map A -> the
        // instantiated extends clause type arg, so A maps to the correct concrete type.
        self.compose_ancestor_substitutions(base_idx, &mut substitution);

        let base_chain_summary = self.summarize_class_chain(base_idx);
        let base_instance_member_names: rustc_hash::FxHashSet<String> = base_chain_summary
            .visible_instance_names()
            .cloned()
            .collect();
        let base_static_member_names: rustc_hash::FxHashSet<String> =
            base_chain_summary.visible_static_names().cloned().collect();

        self.check_constructor_parameter_property_overrides(
            class_data,
            Some(base_idx),
            Some(&base_chain_summary),
            &base_class_name,
            &derived_class_name,
            &base_instance_member_names,
            no_implicit_override,
        );

        // Track names that already had TS2610/TS2611 emitted (avoid duplicate for get+set pairs)
        let mut accessor_mismatch_reported: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        let mut class_extends_error_reported = false;

        // Check each member in the derived class
        for &member_idx in &class_data.members.nodes {
            let Some(info) = self.extract_class_member_info(member_idx, false) else {
                continue;
            };
            let (
                member_name,
                member_type,
                member_name_idx,
                member_visibility,
                is_method,
                is_static,
                is_accessor,
                has_override,
                is_jsdoc_override,
                has_dynamic_name,
                is_abstract,
                has_computed_non_literal_name,
            ) = (
                info.name,
                info.type_id,
                info.name_idx,
                info.visibility,
                info.is_method,
                info.is_static,
                info.is_accessor,
                info.has_override,
                info.is_jsdoc_override,
                info.has_dynamic_name,
                info.is_abstract,
                info.has_computed_non_literal_name,
            );

            // Skip override checking for private identifiers (#foo)
            // Private fields are scoped to the class that declares them and
            // do NOT participate in the inheritance hierarchy
            if member_name.starts_with('#') {
                continue;
            }

            // Detect overload signatures (method declarations without body) so we
            // can skip the type compatibility check for them later.  We do NOT
            // skip the entire loop iteration because override / accessor / kind
            // mismatch checks still need to run for bodyless method declarations.
            let is_overload_signature = is_method && {
                self.ctx
                    .arena
                    .get(member_idx)
                    .and_then(|n| self.ctx.arena.get_method_decl(n))
                    .is_some_and(|m| m.body.is_none())
            };

            let base_info = base_chain_summary
                .lookup(&member_name, is_static, true)
                .cloned();

            if has_override {
                // Cannot use `override` when name is computed dynamically.
                if has_dynamic_name {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                            &[],
                        ),
                        diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                    );
                    continue;
                }
            }

            if has_dynamic_name {
                // Dynamic names are allowed regardless of `noImplicitOverride`; they cannot
                // satisfy normal override checks because their exact identity cannot be
                // statically proven as an inherited symbol.
            } else if has_override {
                // `override` requires a matching visible base member.
                if base_info.is_none() {
                    let suggestion_names = if is_static {
                        &base_static_member_names
                    } else {
                        &base_instance_member_names
                    };
                    if let Some(suggestion) =
                        self.find_override_name_suggestion(suggestion_names, &member_name)
                    {
                        // TS4117 (keyword) or TS4123 (JSDoc): "Did you mean ...?"
                        let (msg, code) = if is_jsdoc_override {
                            (
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D_2,
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D_2,
                            )
                        } else {
                            (
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                            )
                        };
                        self.error_at_node(
                            member_name_idx,
                            &crate::diagnostics::format_message(
                                msg,
                                &[&base_class_name, &suggestion],
                            ),
                            code,
                        );
                    } else {
                        // TS4113 (keyword) or TS4122 (JSDoc): not declared in base
                        let (msg, code) = if is_jsdoc_override {
                            (
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D,
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D,
                            )
                        } else {
                            (
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                            )
                        };
                        self.error_at_node(
                            member_name_idx,
                            &crate::diagnostics::format_message(msg, &[&base_class_name]),
                            code,
                        );
                    }
                    continue;
                }
            } else if no_implicit_override && base_info.is_some() && !has_computed_non_literal_name
            {
                // tsc does not require `override` for `declare` property re-declarations.
                // A `declare property: T` in a derived class is a type-only ambient annotation
                // (no runtime effect) and is not considered a true override.
                let is_declare_property = !is_method
                    && !is_accessor
                    && self
                        .ctx
                        .arena
                        .get(member_idx)
                        .and_then(|n| self.ctx.arena.get_property_decl(n))
                        .is_some_and(|prop| self.has_declare_modifier(&prop.modifiers));
                if is_declare_property {
                    continue;
                }
                // tsc does not require `override` when a concrete member implements an
                // abstract base method. Abstract members MUST be implemented, so
                // providing a concrete implementation is not an "accidental" override —
                // only abstract-to-abstract re-declarations require the `override` keyword.
                let base_is_abstract_method = base_info
                    .as_ref()
                    .is_some_and(|base| base.is_abstract && base.is_method);
                if !is_abstract && base_is_abstract_method {
                    continue;
                }
                if base_info
                    .as_ref()
                    .is_some_and(|base| base.is_abstract && base.is_method)
                {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            if self.ctx.is_js_file() {
                                diagnostic_messages::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                            } else {
                                diagnostic_messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METH
                            },
                            &[&base_class_name],
                        ),
                        if self.ctx.is_js_file() {
                            diagnostic_codes::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                        } else {
                            diagnostic_codes::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METH
                        },
                    );
                } else {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            if self.ctx.is_js_file() {
                                diagnostic_messages::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                            } else {
                                diagnostic_messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE
                            },
                            &[&base_class_name],
                        ),
                        if self.ctx.is_js_file() {
                            diagnostic_codes::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                        } else {
                            diagnostic_codes::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE
                        },
                    );
                }
                continue;
            }

            let base_scope = self.push_type_parameters(&base_class.type_parameters);

            // Find matching member including private/protected members to detect
            // class-level visibility/branding incompatibilities (TS2415).
            let base_any_info = base_chain_summary
                .lookup(&member_name, is_static, false)
                .cloned();

            if let Some(ref base_any_info) = base_any_info
                && self
                    .class_member_visibility_conflicts(member_visibility, base_any_info.visibility)
            {
                // When both derived and base members are private, tsc checks type
                // compatibility and emits TS2416 if the types differ, rather than
                // emitting TS2415 (branding conflict). Only emit TS2415 when the
                // types are compatible or when visibility differs.
                if member_visibility == MemberVisibility::Private
                    && base_any_info.visibility == MemberVisibility::Private
                {
                    let base_type =
                        instantiate_type(self.ctx.types, base_any_info.type_id, &substitution);
                    if member_type != TypeId::ANY
                        && base_type != TypeId::ANY
                        && should_report_member_type_mismatch(
                            self,
                            member_type,
                            base_type,
                            member_name_idx,
                        )
                    {
                        // TS2416: Private member type incompatibility
                        self.pop_type_parameters(base_scope.1);
                        self.error_at_node(
                            member_name_idx,
                            &format!(
                                "Property '{member_name}' in type '{derived_class_name}' is not assignable to the same property in base type '{base_class_name}'."
                            ),
                            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                        );
                        let member_type_str = self.format_type(member_type);
                        let base_type_str = self.format_type(base_type);
                        self.report_type_not_assignable_detail(
                            member_name_idx,
                            &member_type_str,
                            &base_type_str,
                            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                        );
                        continue;
                    }
                }
                self.pop_type_parameters(base_scope.1);
                if !class_extends_error_reported {
                    if is_static {
                        self.error_at_node(
                            class_data.name,
                            &format!(
                                "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'."
                            ),
                            diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                        );
                    } else {
                        self.error_at_node(
                            class_data.name,
                            &format!(
                                "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'."
                            ),
                            diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                        );
                    }
                    class_extends_error_reported = true;
                }
                continue;
            }

            // Look for a matching member in the base class hierarchy (skip private members)
            // First check direct members of the base class, then walk up the chain
            let base_info = base_chain_summary
                .lookup(&member_name, is_static, true)
                .cloned();

            self.pop_type_parameters(base_scope.1);

            let Some(base_info) = base_info else {
                continue;
            };

            let base_type = instantiate_type(self.ctx.types, base_info.type_id, &substitution);

            // TS2610/TS2611: Check accessor/property kind mismatch
            // Only applies to non-method, non-static members. Fires regardless of types (even ANY).
            // Static members are allowed to override accessors with properties and vice versa.
            if !is_method
                && !is_static
                && !base_info.is_method
                && !base_info.is_abstract
                && !accessor_mismatch_reported.contains(&member_name)
            {
                if !is_accessor && base_info.is_accessor {
                    // TS2610: derived property overrides base accessor
                    accessor_mismatch_reported.insert(member_name.clone());
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "'{member_name}' is defined as an accessor in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an instance property."
                        ),
                        diagnostic_codes::IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_INSTANCE_PROP,
                    );
                    continue;
                }
                if is_accessor && !base_info.is_accessor {
                    // TS2611: derived accessor overrides base property
                    accessor_mismatch_reported.insert(member_name.clone());
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "'{member_name}' is defined as a property in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an accessor."
                        ),
                        diagnostic_codes::IS_DEFINED_AS_A_PROPERTY_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_ACCESSOR,
                    );
                    continue;
                }
            }

            // TS2423/TS2425/TS2426: Check for method/property/accessor kind mismatch (INSTANCE members only)
            // Static members use TS2417 instead
            if !is_static {
                // TS2423: Base has method, derived has accessor
                if is_accessor && !is_method && base_info.is_method {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{base_class_name}' defines instance member function '{member_name}', but extended class '{derived_class_name}' defines it as instance member accessor."
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_FUNCTION_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                    continue;
                }

                // TS2425: Base has property (not method, not accessor), derived has method
                if is_method && !base_info.is_method && !base_info.is_accessor {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{base_class_name}' defines instance member property '{member_name}', but extended class '{derived_class_name}' defines it as instance member function."
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_PROPERTY_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                    continue;
                }

                // TS2423: Base has method, derived has accessor
                if is_accessor && base_info.is_method {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{base_class_name}' defines instance member function '{member_name}', but extended class '{derived_class_name}' defines it as instance member accessor."
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_FUNCTION_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                    continue;
                }

                // TS2426: Base has accessor, derived has method
                // Note: do NOT `continue` here — tsc also emits TS2416 for type incompatibility
                // alongside the kind mismatch error, so the type check below must still run.
                if is_method && base_info.is_accessor {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{base_class_name}' defines instance member accessor '{member_name}', but extended class '{derived_class_name}' defines it as instance member function."
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_ACCESSOR_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                }
            }

            // Skip type compatibility check if either type is ANY
            if member_type == TypeId::ANY || base_type == TypeId::ANY {
                continue;
            }

            // Skip type compatibility for overload signatures. tsc checks
            // inheritance using the combined overloaded type from the symbol,
            // not individual AST declarations.  Individual overloads may be
            // narrower than the base method's type, producing false TS2416.
            if is_overload_signature {
                continue;
            }

            // Resolve TypeQuery types (typeof) before comparison
            let resolved_member_type = self.resolve_type_query_type(member_type);
            let resolved_base_type = self.resolve_type_query_type(base_type);

            // Check type compatibility through centralized mismatch policy.
            // Methods always use bivariant relation checks.
            // Static properties also use bivariant checks — tsc checks the static
            // side structurally (typeof Derived vs typeof Base) with the normal
            // assignability relation, which without strictFunctionTypes is bivariant.
            // Only instance property overrides use strict assignability (TS2416).
            let should_report_mismatch = if is_method || is_static {
                should_report_member_type_mismatch_bivariant(
                    self,
                    resolved_member_type,
                    resolved_base_type,
                    member_name_idx,
                )
            } else {
                should_report_member_type_mismatch(
                    self,
                    resolved_member_type,
                    resolved_base_type,
                    member_name_idx,
                )
            };

            if should_report_mismatch {
                let member_type_str = self.format_type(member_type);
                let base_type_str = self.format_type(base_type);

                // TS2417: Static members use different error message and code
                // TS2416: Instance members use standard property incompatibility error
                if is_static {
                    // TS2417: Class static side '{0}' incorrectly extends base class static side '{1}'.
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'."
                        ),
                        diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                    );
                } else {
                    // TS2416: Instance member incompatibility
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Property '{member_name}' in type '{derived_class_name}' is not assignable to the same property in base type '{base_class_name}'."
                        ),
                        diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                    );
                    self.report_type_not_assignable_detail(
                        member_name_idx,
                        &member_type_str,
                        &base_type_str,
                        diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                    );
                }
            }
        }

        // Check constructor parameter properties for type/visibility compatibility
        // with base class members. The main member loop above only handles
        // PROPERTY_DECLARATION/METHOD_DECLARATION/ACCESSOR nodes. Parameter properties
        // (e.g., `constructor(public p?: number)`) are syntactic sugar for class properties
        // but live inside the constructor node, so they need separate handling.
        if !class_extends_error_reported {
            self.check_parameter_property_compatibility(
                class_data,
                &base_chain_summary,
                &derived_class_name,
                &base_class_name,
                &substitution,
            );
        }

        // Check index signature compatibility between derived and base classes (TS2415)
        self.check_class_index_signature_compatibility(
            class_data,
            base_class,
            &derived_class_name,
            &base_class_name,
            &substitution,
            class_extends_error_reported,
        );

        // TS2417: Whole-type static side check for namespace-merged classes.
        //
        // The member-by-member loop above only examines AST class body members.
        // When the base class has a merged namespace (e.g.,
        // `namespace Shape.Utils { export function convert(): Shape { ... } }`),
        // `typeof Shape` includes those namespace exports. If the derived class
        // also has a merged namespace with conflicting exports, `typeof Derived`
        // is structurally incompatible with `typeof Base` — tsc reports this as TS2417.
        //
        // We only check when the DERIVED class also has a namespace (NAMESPACE_MODULE
        // flag), since a derived class without any namespace cannot have conflicting
        // namespace exports. This avoids false positives for classes that simply
        // don't replicate namespace exports from their base class.
        if !class_extends_error_reported && let Some(base_sym) = base_sym_for_ns_static_check {
            let derived_sym = self.ctx.binder.get_node_symbol(class_idx);
            if let Some(derived_sym) = derived_sym {
                let derived_symbol_flags = self
                    .ctx
                    .binder
                    .get_symbol(derived_sym)
                    .map_or(0, |s| s.flags);
                let derived_has_namespace = derived_symbol_flags
                    & (tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE)
                    != 0;
                if derived_has_namespace {
                    let derived_ctor_type = self.get_type_of_symbol(derived_sym);
                    let base_ctor_type = self.get_type_of_symbol(base_sym);
                    if derived_ctor_type != TypeId::UNKNOWN
                        && derived_ctor_type != TypeId::ERROR
                        && base_ctor_type != TypeId::UNKNOWN
                        && base_ctor_type != TypeId::ERROR
                        && !self.is_assignable_to(derived_ctor_type, base_ctor_type)
                    {
                        self.error_at_node(
                                class_data.name,
                                &format!(
                                    "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'."
                                ),
                                diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                            );
                    }
                }
            }
        }

        self.pop_type_parameters(derived_type_param_updates);
    }

    /// Check constructor parameter properties against base class members for
    /// type and visibility compatibility (TS2415).
    ///
    /// tsc emits TS2415 at the class name when a parameter property (e.g.,
    /// `constructor(public p?: number)`) is incompatible with the corresponding
    /// base class member. This can be due to:
    /// - Visibility conflict: derived public vs base private
    /// - Type incompatibility: derived `number | undefined` vs base `number`
    fn check_parameter_property_compatibility(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        base_chain_summary: &ClassChainSummary,
        derived_class_name: &str,
        base_class_name: &str,
        substitution: &tsz_solver::TypeSubstitution,
    ) {
        use crate::query_boundaries::common::instantiate_type;

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };

            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if !self.has_parameter_property_modifier(&param.modifiers) {
                    continue;
                }
                let Some(param_name) = self.get_property_name(param.name) else {
                    continue;
                };

                let derived_visibility = if self.has_private_modifier(&param.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&param.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };

                // Find matching member in base class (including private, for visibility checks)
                let base_any_info = base_chain_summary
                    .lookup(&param_name, false, false)
                    .cloned();

                // Check visibility conflict (TS2415)
                if let Some(ref base_any_info) = base_any_info
                    && self.class_member_visibility_conflicts(
                        derived_visibility,
                        base_any_info.visibility,
                    )
                {
                    self.error_at_node(
                        class_data.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'."
                        ),
                        diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
                    return; // Only one TS2415 per class
                }

                // Check type compatibility — find visible base member
                let base_info = base_chain_summary.lookup(&param_name, false, true).cloned();
                let Some(base_info) = base_info else {
                    continue;
                };
                let base_type = instantiate_type(self.ctx.types, base_info.type_id, substitution);

                // Get the parameter property type, accounting for optionality
                let mut prop_type = if param.type_annotation.is_some() {
                    self.get_type_from_type_node(param.type_annotation)
                } else {
                    TypeId::ANY
                };

                // Optional parameter properties (`p?: T`) have type `T | undefined`
                // under strictNullChecks
                if param.question_token && self.ctx.strict_null_checks() {
                    let factory = self.ctx.types.factory();
                    prop_type = factory.union2(prop_type, TypeId::UNDEFINED);
                }

                // Skip if either type is ANY
                if prop_type == TypeId::ANY || base_type == TypeId::ANY {
                    continue;
                }

                // Check type compatibility through centralized mismatch policy
                if should_report_member_type_mismatch(self, prop_type, base_type, param.name) {
                    // tsc emits TS2415 at the class name for parameter property
                    // type incompatibility (not TS2416 at the member)
                    self.error_at_node(
                        class_data.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'."
                        ),
                        diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
                    return; // Only one TS2415 per class
                }
            }
        }
    }

    /// Append type parameter names (e.g., `<T, U>`) to a class/interface name string.
    /// This matches tsc's display format for TS2415/TS2430 error messages.
    pub(crate) fn append_type_param_names(
        &self,
        name: &mut String,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) {
        let Some(list) = type_parameters else {
            return;
        };
        let mut param_names = Vec::new();
        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };
            if let Some(name_node) = self.ctx.arena.get(data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                param_names.push(ident.escaped_text.as_str());
            }
        }
        if !param_names.is_empty() {
            name.push('<');
            name.push_str(&param_names.join(", "));
            name.push('>');
        }
    }

    /// Walk the inheritance chain from `class_idx` upward and compose type parameter
    /// substitutions into `substitution`. This ensures that type parameters from
    /// ancestor classes (not just the immediate base) are correctly mapped.
    ///
    /// For example, given `X extends L<X>` where `L<RT> extends T<RT[RT['a']]>`:
    /// - The initial substitution maps `RT -> X_type`
    /// - This method walks L -> T, finding `T<A>` with extends arg `RT[RT['a']]`
    /// - It instantiates the extends arg with the current substitution: `X[X['a']]`
    /// - It adds `A -> X[X['a']]` to the substitution
    fn compose_ancestor_substitutions(
        &mut self,
        class_idx: NodeIndex,
        substitution: &mut TypeSubstitution,
    ) {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited = FxHashSet::default();

        while visited.insert(current) {
            let Some(class) = self.ctx.arena.get_class_at(current) else {
                break;
            };

            let heritage_clauses = match class.heritage_clauses.as_ref() {
                Some(hc) => hc.clone(),
                None => break,
            };

            let mut next_class = None;

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_ta) = self.ctx.arena.get_expr_type_args(type_node) {
                        (expr_ta.expression, expr_ta.type_arguments.as_ref().cloned())
                    } else {
                        (type_idx, None)
                    };

                // No type arguments means no intermediate substitution needed
                let Some(ta) = type_arguments else {
                    // Still need to walk up the chain in case there are further ancestors
                    if let Some(parent_idx) = self.get_base_class_idx(current) {
                        next_class = Some(parent_idx);
                    }
                    break;
                };

                // Resolve the parent class
                let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    break;
                };
                let Some(parent_class_idx) = self.get_class_declaration_from_symbol(base_sym_id)
                else {
                    break;
                };
                let Some(parent_class) = self.ctx.arena.get_class_at(parent_class_idx) else {
                    break;
                };

                // Push current class's type params so we can resolve extends type args
                let (_, current_tp_updates) = self.push_type_parameters(&class.type_parameters);

                // Resolve extends clause type arguments
                let mut extends_type_args = Vec::new();
                for &arg_idx in &ta.nodes {
                    extends_type_args.push(self.get_type_from_type_node(arg_idx));
                }

                self.pop_type_parameters(current_tp_updates);

                // Get parent's type parameters
                let (parent_type_params, parent_tp_updates) =
                    self.push_type_parameters(&parent_class.type_parameters);
                self.pop_type_parameters(parent_tp_updates);

                // For each parent type parameter, instantiate the extends type arg
                // with the current (accumulated) substitution and add the mapping
                for (i, param) in parent_type_params.iter().enumerate() {
                    if substitution.get(param.name).is_some() {
                        continue; // Already mapped
                    }
                    let arg_type = if i < extends_type_args.len() {
                        extends_type_args[i]
                    } else {
                        param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN)
                    };
                    let instantiated = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        arg_type,
                        substitution,
                    );
                    substitution.insert(param.name, instantiated);
                }

                next_class = Some(parent_class_idx);
                break; // Only process first extends clause
            }

            match next_class {
                Some(nc) => current = nc,
                None => break,
            }
        }
    }

    // Index signature compatibility (TS2415), interface extension compatibility (TS2430),
    // member lookup in class chains, and visibility conflict detection are in
    // `class_checker_compat.rs`.
}
