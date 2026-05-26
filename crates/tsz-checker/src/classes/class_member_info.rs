//! Class member extraction and override helper checks.

use std::borrow::Cow;

use crate::class_checker::{
    ClassMemberInfo, MemberVisibility, base_class_name_for_diagnostic,
    format_property_name_for_diagnostic,
};
use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::query_boundaries::class::{
    should_report_member_type_mismatch_bivariant, should_report_own_member_type_mismatch,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

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

    /// Collect the names of a symbol's namespace exports (merged `namespace X { ... }`
    /// declarations). Used to decide whether a class-namespace merge could
    /// shadow or conflict with a base class's static members for the TS2417
    /// static-side compatibility check.
    pub(crate) fn collect_namespace_export_names_for_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && let Some(exports) = symbol.exports.as_ref()
        {
            for (name, _sym_id) in exports.iter() {
                if !name.is_empty() {
                    names.insert(name.clone());
                }
            }
        }
        names
    }

    /// Collect all property names from a type via the solver query boundary API.
    /// Used for type-level override checking when the base class is a complex
    /// expression (function call, intersection constructor).
    pub(crate) fn collect_property_names_from_type(
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
    pub(crate) fn check_override_members_against_type(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        derived_class_name: &str,
        base_class_name: &str,
        base_instance_member_names: &rustc_hash::FxHashSet<String>,
        base_static_member_names: &rustc_hash::FxHashSet<String>,
        no_implicit_override: bool,
        base_types: (Option<TypeId>, Option<TypeId>),
    ) {
        use crate::query_boundaries::class::build_own_member_summary;

        let (base_instance_type, base_static_type) = base_types;

        let (_derived_type_params, derived_type_param_updates) =
            self.push_type_parameters(&class_data.type_parameters);

        let derived_accessor_pair_types = self.class_accessor_pair_getter_types(class_data);

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
            let base_type_for_member = if info.is_static {
                base_static_type
            } else {
                base_instance_type
            };
            let base_prop_result = base_type_for_member.map(|base_type_id| {
                self.resolve_property_access_with_env(base_type_id, &info.name)
            });
            // For intersections like `Protected & Protected2`, a property
            // missing from every member can still resolve as `Success` with
            // `type_id == never`. That's not an actual override — treat
            // `never` as "no such member" so we don't emit a spurious
            // TS2416 ("not assignable to ... 'never'"). tsc's heritage
            // override check only fires when the base actually declares
            // the property.
            let member_exists_via_type = base_prop_result.is_some_and(|result| {
                matches!(
                    result,
                    crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. }
                        if type_id != TypeId::NEVER
                )
            });
            let member_exists_in_base = if info.is_static {
                base_static_member_names.contains(&info.name)
            } else {
                base_instance_member_names.contains(&info.name)
            } || member_exists_via_type;

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
                if !member_exists_in_base {
                    // Member not found in base — check for suggestion
                    let suggestion_pool = if info.is_static {
                        base_static_member_names
                    } else {
                        base_instance_member_names
                    };
                    if let Some(suggestion) =
                        self.find_override_name_suggestion(suggestion_pool, &info.name)
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
                && member_exists_in_base
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

            if !info.is_static
                && info.is_accessor
                && !info.is_method
                && member_exists_in_base
                && base_type_for_member.is_some()
                && self.heritage_display_base_has_setter_only_accessor(base_class_name, &info.name)
            {
                self.error_at_node(
                    info.name_idx,
                    &format!(
                        "'{}' is defined as a property in class '{}', but is overridden here in '{}' as an accessor.",
                        info.name, base_class_name, derived_class_name
                    ),
                    diagnostic_codes::IS_DEFINED_AS_A_PROPERTY_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_ACCESSOR,
                );
                continue;
            }

            // Accessor pair canonicalization: skip the SET_ACCESSOR when a
            // sibling GET_ACCESSOR exists. The getter's iteration already ran
            // the compat check against the accessor pair's canonical property
            // type (the getter return type).
            if info.is_setter
                && derived_accessor_pair_types.contains_key(&(info.name.clone(), info.is_static))
            {
                continue;
            }

            // Check property type compatibility with base (TS2416)
            // Only check if the member actually exists in the base type —
            // otherwise there's no override to be incompatible with.
            if let Some(base_type_id) = base_type_for_member
                && member_exists_in_base
            {
                let member_type = info.type_id;
                // Skip if either type is ANY
                if member_type != TypeId::ANY {
                    use crate::query_boundaries::common::PropertyAccessResult;
                    let base_prop_result = base_prop_result.unwrap_or_else(|| {
                        self.resolve_property_access_with_env(base_type_id, &info.name)
                    });
                    if let PropertyAccessResult::Success {
                        type_id: base_type, ..
                    } = base_prop_result
                        && base_type != TypeId::ANY
                    {
                        let resolved_member_type = self.resolve_type_query_type(member_type);
                        let resolved_base_type = self.resolve_type_query_type(base_type);

                        let should_report = if info.is_method || info.is_static {
                            should_report_member_type_mismatch_bivariant(
                                self,
                                resolved_member_type,
                                resolved_base_type,
                                info.name_idx,
                            )
                        } else {
                            should_report_own_member_type_mismatch(
                                self,
                                resolved_member_type,
                                resolved_base_type,
                                info.name_idx,
                            )
                        };

                        if should_report {
                            if info.is_static {
                                let error_idx =
                                    class_data.name.into_option().unwrap_or(info.name_idx);
                                self.error_at_node(
                                    error_idx,
                                    &format!(
                                        "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'."
                                    ),
                                    diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                                );
                            } else {
                                let member_type_str = self.format_type(member_type);
                                let base_type_str = self.format_type(base_type);
                                let display_name = format_property_name_for_diagnostic(&info.name);
                                let base_class_display_name = self
                                    .array_or_tuple_alias_target_text_for_name(base_class_name)
                                    .map(Cow::Owned)
                                    .unwrap_or_else(|| {
                                        base_class_name_for_diagnostic(base_class_name)
                                    });
                                self.error_at_node(
                                    info.name_idx,
                                    &format!(
                                        "Property '{display_name}' in type '{derived_class_name}' is not assignable to the same property in base type '{base_class_display_name}'."
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
        }

        // Also check constructor parameter properties
        self.check_constructor_parameter_property_overrides(
            class_data,
            None,
            None,
            base_class_name,
            derived_class_name,
            base_instance_member_names,
            no_implicit_override,
        );

        self.pop_type_parameters(derived_type_param_updates);
    }

    fn heritage_display_base_has_setter_only_accessor(
        &self,
        base_class_name: &str,
        member_name: &str,
    ) -> bool {
        base_class_name.split("typeof ").skip(1).any(|suffix| {
            let ident = suffix
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>();
            if ident.is_empty() {
                return false;
            }
            let Some(sym_id) = self.ctx.binder.file_locals.get(&ident) else {
                return false;
            };
            let (has_get_or_auto, has_set) =
                self.class_symbol_member_accessor_shape(sym_id, member_name);
            has_set && !has_get_or_auto
        })
    }

    fn class_symbol_member_accessor_shape(
        &self,
        class_sym_id: tsz_binder::SymbolId,
        member_name: &str,
    ) -> (bool, bool) {
        let Some(symbol) = self.get_symbol_globally(class_sym_id) else {
            return (false, false);
        };
        let mut has_get_or_auto = false;
        let mut has_set = false;
        for &decl_idx in &symbol.declarations {
            let Some(class_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(class_data) = self.ctx.arena.get_class(class_node) else {
                continue;
            };
            for &member_idx in &class_data.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                match member_node.kind {
                    syntax_kind_ext::PROPERTY_DECLARATION => {
                        let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                            continue;
                        };
                        if self.has_accessor_modifier(&prop.modifiers)
                            && self
                                .get_property_name(prop.name)
                                .is_some_and(|name| name == member_name)
                        {
                            has_get_or_auto = true;
                        }
                    }
                    syntax_kind_ext::GET_ACCESSOR => {
                        let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                            continue;
                        };
                        if self
                            .get_property_name(accessor.name)
                            .is_some_and(|name| name == member_name)
                        {
                            has_get_or_auto = true;
                        }
                    }
                    syntax_kind_ext::SET_ACCESSOR => {
                        let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                            continue;
                        };
                        if self
                            .get_property_name(accessor.name)
                            .is_some_and(|name| name == member_name)
                        {
                            has_set = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        (has_get_or_auto, has_set)
    }

    /// Report errors for members with `override` in a class that has no base class.
    ///
    /// This consolidates the duplicated "no heritage" / "no resolved base" override
    /// checking patterns that were previously inlined in multiple places inside
    /// `check_property_inheritance_compatibility`. The `class_data` members are
    /// iterated via pre-built `OwnMemberSummary` when available; otherwise they
    /// are extracted inline.
    pub(crate) fn report_overrides_without_base(
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
    pub(crate) fn find_override_name_suggestion(
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
                let name_node = self.ctx.arena.get(prop.name)?;
                let name = if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    self.get_property_name_resolved(prop.name)
                        .or_else(|| self.get_property_name(prop.name))?
                } else {
                    self.get_property_name(prop.name)?
                };
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
                    is_setter: false,
                    is_abstract,
                    has_override: self.has_override_modifier(&prop.modifiers)
                        || self.has_jsdoc_override_tag(member_idx),
                    is_jsdoc_override: !self.has_override_modifier(&prop.modifiers)
                        && self.has_jsdoc_override_tag(member_idx),
                    has_dynamic_name: self.is_computed_name_dynamic(prop.name),
                    has_computed_non_literal_name: self.is_computed_non_literal_name(prop.name),
                    from_interface: false,
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
                                    is_setter: false,
                                    is_abstract: self.has_abstract_modifier(&method.modifiers),
                                    has_override: true,
                                    is_jsdoc_override: !self
                                        .has_override_modifier(&method.modifiers)
                                        && self.has_jsdoc_override_tag(member_idx),
                                    has_dynamic_name: true,
                                    has_computed_non_literal_name: true,
                                    from_interface: false,
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
                    is_setter: false,
                    is_abstract,
                    has_override: self.has_override_modifier(&method.modifiers)
                        || self.has_jsdoc_override_tag(member_idx),
                    is_jsdoc_override: !self.has_override_modifier(&method.modifiers)
                        && self.has_jsdoc_override_tag(member_idx),
                    has_dynamic_name: self.is_computed_name_dynamic(method.name),
                    has_computed_non_literal_name: self.is_computed_non_literal_name(method.name),
                    from_interface: false,
                })
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node)?;
                let name_node = self.ctx.arena.get(accessor.name)?;
                let name = if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    self.get_property_name_resolved(accessor.name)
                        .or_else(|| self.get_property_name(accessor.name))?
                } else {
                    self.get_property_name(accessor.name)?
                };
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
                    is_setter: false,
                    is_abstract,
                    has_override: self.has_override_modifier(&accessor.modifiers)
                        || self.has_jsdoc_override_tag(member_idx),
                    is_jsdoc_override: !self.has_override_modifier(&accessor.modifiers)
                        && self.has_jsdoc_override_tag(member_idx),
                    has_dynamic_name: self.is_computed_name_dynamic(accessor.name),
                    has_computed_non_literal_name: self.is_computed_non_literal_name(accessor.name),
                    from_interface: false,
                })
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node)?;
                let name_node = self.ctx.arena.get(accessor.name)?;
                let name = if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    self.get_property_name_resolved(accessor.name)
                        .or_else(|| self.get_property_name(accessor.name))?
                } else {
                    self.get_property_name(accessor.name)?
                };
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
                    is_setter: true,
                    is_abstract,
                    has_override: self.has_override_modifier(&accessor.modifiers)
                        || self.has_jsdoc_override_tag(member_idx),
                    is_jsdoc_override: !self.has_override_modifier(&accessor.modifiers)
                        && self.has_jsdoc_override_tag(member_idx),
                    has_dynamic_name: self.is_computed_name_dynamic(accessor.name),
                    has_computed_non_literal_name: self.is_computed_non_literal_name(accessor.name),
                    from_interface: false,
                })
            }
            _ => None,
        }
    }
}
