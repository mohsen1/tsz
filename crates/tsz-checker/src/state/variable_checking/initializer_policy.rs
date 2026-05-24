//! Initializer type-computation policy for variable declarations.
//!
//! Separated from `core.rs` so the policy (what type to assign to a
//! variable's initializer) can be read and tested independently of the
//! outer orchestration (symbol caching, TS2403, binding-pattern checks).

use crate::context::{TypingRequest, speculation::DiagnosticSpeculationSnapshot};
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Declaration facts needed to compute the variable's initializer type.
///
/// All fields are cheaply copyable so the caller can extract them from the
/// arena-borrowed `VariableDeclarationData` before taking a `&mut self` borrow.
#[derive(Clone, Debug)]
pub(super) struct VarDeclFacts {
    /// The `VariableDeclaration` node index.
    pub decl_idx: NodeIndex,
    /// The binding name (identifier or binding pattern).
    pub name: NodeIndex,
    /// The type annotation node, or `NodeIndex::NONE` if absent.
    pub annotation: NodeIndex,
    /// The initializer expression node, or `NodeIndex::NONE` if absent.
    pub initializer: NodeIndex,
    /// `true` when this declaration is a catch-clause variable.
    pub is_catch_variable: bool,
    /// `true` when the binding name is an object/array binding pattern.
    pub is_destructuring: bool,
    /// The resolved identifier text for the variable, `None` for destructuring.
    pub var_name: Option<String>,
}

impl<'a> CheckerState<'a> {
    /// Compute the final type for a variable declaration and run the
    /// initializer-level assignability checks.
    ///
    /// Returns `(final_type, jsdoc_declared_type)`.  `jsdoc_declared_type` is
    /// `Some` when a JSDoc `@type` annotation determined the declared type; the
    /// caller needs it to drive the subsequent binding-pattern check.
    pub(super) fn compute_variable_decl_type(
        &mut self,
        facts: &VarDeclFacts,
        typing_request: &TypingRequest,
    ) -> (TypeId, Option<TypeId>) {
        let mut jsdoc_declared_type: Option<TypeId> = None;
        let mut has_type_annotation = facts.annotation.is_some();
        let mut declared_type = if has_type_annotation {
            // Check for undefined type names in nested types (e.g., function type parameters)
            // Skip top-level TYPE_REFERENCE to avoid duplicates with get_type_from_type_node
            self.check_type_for_missing_names_skip_top_level_ref(facts.annotation);
            self.check_type_for_parameter_properties(facts.annotation);
            let type_id = self.get_type_from_type_node(facts.annotation);
            let type_id = if facts.initializer.is_none() {
                self.bare_type_alias_annotation_declared_type(facts.annotation, type_id)
                    .unwrap_or(type_id)
            } else {
                type_id
            };
            // TS1196: Catch clause variable type annotation must be 'any' or 'unknown'.
            // When the annotation is invalid, fall back to the catch-variable default
            // (any/unknown) so the catch body sees the same type tsc uses, preventing
            // cascade errors like TS2339 on `e.method()` or destructured names.
            let invalid_catch_annotation = facts.is_catch_variable
                && type_id != TypeId::ANY
                && type_id != TypeId::UNKNOWN
                && !self.type_contains_error(type_id);
            if invalid_catch_annotation {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    facts.annotation,
                    "Catch clause variable type annotation must be 'any' or 'unknown' if specified.",
                    diagnostic_codes::CATCH_CLAUSE_VARIABLE_TYPE_ANNOTATION_MUST_BE_ANY_OR_UNKNOWN_IF_SPECIFIED,
                );
                flow_boundary::resolve_catch_variable_type(
                    self.ctx.use_unknown_in_catch_variables(),
                )
            } else {
                type_id
            }
        } else if facts.is_catch_variable {
            // Route catch variable type resolution through the flow
            // observation boundary for centralized policy.
            flow_boundary::resolve_catch_variable_type(self.ctx.use_unknown_in_catch_variables())
        } else {
            TypeId::ANY
        };
        if !has_type_annotation
            && let Some(jsdoc_type) = self
                .jsdoc_type_annotation_for_node(facts.decl_idx)
                .or_else(|| self.jsdoc_type_annotation_for_node_inference(facts.decl_idx))
        {
            // TS1196: Catch clause variable type annotation must be 'any' or 'unknown'
            // This also applies to JSDoc @type annotations on catch variables in JS files.
            let is_invalid_catch_jsdoc = facts.is_catch_variable
                && jsdoc_type != TypeId::ANY
                && jsdoc_type != TypeId::UNKNOWN
                && !self.type_contains_error(jsdoc_type);
            if is_invalid_catch_jsdoc {
                use crate::diagnostics::diagnostic_codes;
                let jsdoc_type_span = self.jsdoc_type_expression_span_for_node(facts.decl_idx);
                if let Some((start, length)) = jsdoc_type_span {
                    self.error_at_position(
                        start,
                        length,
                        "Catch clause variable type annotation must be 'any' or 'unknown' if specified.",
                        diagnostic_codes::CATCH_CLAUSE_VARIABLE_TYPE_ANNOTATION_MUST_BE_ANY_OR_UNKNOWN_IF_SPECIFIED,
                    );
                } else {
                    self.error_at_node(
                        facts.decl_idx,
                        "Catch clause variable type annotation must be 'any' or 'unknown' if specified.",
                        diagnostic_codes::CATCH_CLAUSE_VARIABLE_TYPE_ANNOTATION_MUST_BE_ANY_OR_UNKNOWN_IF_SPECIFIED,
                    );
                }
            }
            declared_type = if is_invalid_catch_jsdoc {
                flow_boundary::resolve_catch_variable_type(
                    self.ctx.use_unknown_in_catch_variables(),
                )
            } else {
                jsdoc_type
            };
            jsdoc_declared_type = Some(declared_type);
            has_type_annotation = true;
        }
        if !has_type_annotation
            && let Some(merged_type) =
                self.checked_js_remote_class_declared_type_for_variable(facts.decl_idx)
        {
            declared_type = merged_type;
            has_type_annotation = true;
        }
        // If there's a type annotation, that determines the type (even for 'any')
        if has_type_annotation {
            if self.ctx.no_implicit_any()
                && let Some(sf) = self.ctx.arena.source_files.first()
                && let Some(jsdoc) = self.find_jsdoc_for_function(facts.decl_idx)
                && CheckerState::jsdoc_type_tag_function_missing_return(&jsdoc)
                && let Some((_, comment_pos)) = self.try_jsdoc_with_ancestor_walk_and_pos(
                    facts.decl_idx,
                    &sf.comments,
                    &sf.text,
                )
                && let Some(function_pos) =
                    CheckerState::jsdoc_type_tag_function_keyword_pos_in_source(
                        &sf.text,
                        comment_pos,
                    )
            {
                self.ctx.error(
                    function_pos,
                    "function".len() as u32,
                    crate::diagnostics::format_message(
                        crate::diagnostics::diagnostic_messages::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                        &["any"],
                    ),
                    crate::diagnostics::diagnostic_codes::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                );
            }
            if facts.initializer.is_some() {
                // Evaluate the declared type to resolve conditionals before using as context.
                // This ensures types like `type C = string extends string ? "yes" : "no"`
                // provide proper contextual typing for literals, preventing them from widening to string.
                // Only evaluate conditional/mapped/index access types - NOT type aliases or interface
                // references, as evaluating those can change their representation and break variance checking.
                let evaluated_type = if declared_type != TypeId::ANY {
                    self.contextual_type_for_expression(declared_type)
                } else {
                    declared_type
                };
                // Build a TypingRequest for the initializer (but not for 'any')
                let initializer_is_function =
                    self.ctx
                        .arena
                        .get(facts.initializer)
                        .is_some_and(|init_node| {
                            matches!(
                                init_node.kind,
                                syntax_kind_ext::FUNCTION_EXPRESSION
                                    | syntax_kind_ext::ARROW_FUNCTION
                            )
                        });
                let jsdoc_callable_context = initializer_is_function
                    .then(|| {
                        if facts.annotation.is_none() {
                            self.jsdoc_callable_type_annotation_for_node(facts.decl_idx)
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .map(|ty| self.contextual_type_for_expression(ty));
                let jsdoc_blocks_callable_context = initializer_is_function
                    && facts.annotation.is_none()
                    && self
                        .jsdoc_type_annotation_for_node(facts.decl_idx)
                        .is_some()
                    && jsdoc_callable_context.is_none();
                let suppress_initializer_context = evaluated_type != TypeId::ANY
                    && self.suppress_initializer_contextual_type_for_generic_call(
                        facts.initializer,
                        evaluated_type,
                    );
                let suppress_identifier_context = self
                    .should_suppress_identifier_initializer_context_for_index_access(
                        facts.initializer,
                        evaluated_type,
                    );
                let request = if let Some(jsdoc_callable_context) = jsdoc_callable_context {
                    TypingRequest::with_contextual_type(jsdoc_callable_context)
                } else if evaluated_type != TypeId::ANY
                    && !jsdoc_blocks_callable_context
                    && !suppress_initializer_context
                    && !suppress_identifier_context
                {
                    TypingRequest::with_contextual_type(evaluated_type)
                } else {
                    TypingRequest::NONE
                };
                if initializer_is_function
                    && evaluated_type == TypeId::ERROR
                    && facts.annotation.is_some()
                    && self.explicit_annotation_can_defer_implicit_any_context(facts.annotation)
                {
                    self.ctx
                        .implicit_any_contextual_closures
                        .insert(facts.initializer);
                }
                if initializer_is_function && jsdoc_blocks_callable_context {
                    self.ctx
                        .implicit_any_contextual_closures
                        .remove(&facts.initializer);
                    self.ctx
                        .implicit_any_checked_closures
                        .remove(&facts.initializer);
                    self.invalidate_initializer_for_context_change(facts.initializer);
                }
                let conditional_branch_ranges = self
                    .ctx
                    .arena
                    .get(facts.initializer)
                    .filter(|node| node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION)
                    .and_then(|node| self.ctx.arena.get_conditional_expr(node))
                    .map(|cond| {
                        let when_true = self
                            .ctx
                            .arena
                            .get(cond.when_true)
                            .map(|node| (node.pos, node.end));
                        let when_false = self
                            .ctx
                            .arena
                            .get(cond.when_false)
                            .map(|node| (node.pos, node.end));
                        [when_true, when_false]
                    });
                if !request.is_empty()
                    && let Some(init_node) = self.ctx.arena.get(facts.initializer)
                {
                    let init_start = init_node.pos;
                    let init_end = init_node.end;
                    let object_literal_method_name_spans: Vec<(u32, u32)> = if init_node.kind
                        == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    {
                        self.ctx
                            .arena
                            .get_literal_expr(init_node)
                            .map(|literal| {
                                literal
                                    .elements
                                    .nodes
                                    .iter()
                                    .filter_map(|&element_idx| {
                                        let element_node = self.ctx.arena.get(element_idx)?;
                                        if element_node.kind != syntax_kind_ext::METHOD_DECLARATION
                                        {
                                            return None;
                                        }
                                        let method =
                                            self.ctx.arena.get_method_decl(element_node)?;
                                        let name_node = self.ctx.arena.get(method.name)?;
                                        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                                        {
                                            return None;
                                        }
                                        Some((name_node.pos, name_node.end))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    self.ctx.diagnostics.retain(|diag| {
                        diag.code
                            == crate::diagnostics::diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS
                            // TS2693/TS2585/TS1361/TS1362: type-only keywords and
                            // type-only import/export used as values are structural
                            // errors, not contextual-typing artifacts.
                            // They must survive the pre-contextual diagnostic reset.
                            || diag.code == crate::diagnostics::diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
                            || diag.code == crate::diagnostics::diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO
                            || diag.code == crate::diagnostics::diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE
                            || diag.code == crate::diagnostics::diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE
                            // Preserve TS2454 (variable used before assignment) — these
                            // are definite-assignment errors for variables referenced
                            // inside the initializer, not stale contextual-typing
                            // diagnostics that need to be re-evaluated.
                            || diag.code
                                == crate::diagnostics::diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
                            // TS2348/TS2538: invalid calls and invalid
                            // index expressions are structural initializer
                            // diagnostics, not artifacts of contextual
                            // typing from the variable annotation.
                            || diag.code
                                == crate::diagnostics::diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                            || diag.code
                                == crate::diagnostics::diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE
                            // TS2339: "Property does not exist on type" is a structural
                            // error (the object type and property name don't depend on
                            // contextual typing). Preserve it so namespace/module
                            // property-access errors survive the pre-contextual reset.
                            || diag.code
                                == crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                            // TS2304: "Cannot find name" is a name-resolution
                            // failure tied to the source identifier; it must
                            // survive the pre-contextual reset so e.g.
                            // `var x: T = new T()` reports both the
                            // annotation and value-position lookups when `T`
                            // is unresolved.
                            || (diag.code
                                == crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                                && !object_literal_method_name_spans
                                    .iter()
                                    .any(|&(start, end)| diag.start >= start && diag.start < end))
                            // TS2322 diagnostics from the pre-contextual
                            // assignment check can be stale for object
                            // literal methods: contextual method typing may
                            // supply the function shape that makes the final
                            // object assignable. The contextual check below
                            // re-emits real assignment failures.
                            || (diag.code
                                == crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                                && object_literal_method_name_spans.is_empty())
                            // TS2538: "Type 'X' cannot be used as an index
                            // type" is a structural error about the index
                            // expression's shape; it doesn't depend on the
                            // outer contextual type.
                            || diag.code
                                == crate::diagnostics::diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE
                            // TS2348: "Value of type 'X' is not callable.
                            // Did you mean to include 'new'?" is a
                            // structural error about a non-callable value
                            // appearing in call position; not contextual.
                            || diag.code
                                == crate::diagnostics::diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                            || diag.start < init_start
                            || diag.start >= init_end
                    });
                    self.ctx.rebuild_emitted_diagnostics_from_current();
                }
                let init_snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
                let init_diagnostics_len = init_snap.snapshot().diagnostics_len;
                self.maybe_clear_checked_initializer_type_cache(facts.initializer);
                let mut init_type = self.get_type_of_node_with_request(facts.initializer, &request);
                if init_type == TypeId::ERROR
                    && let Some(symbol_type) = self
                        .identifier_initializer_symbol_type_for_index_access_target(
                            facts.initializer,
                            evaluated_type,
                        )
                {
                    init_type = symbol_type;
                }
                // Ensure the contextually-typed init type is stored in node_types
                // for the initializer expression. Error elaboration may re-check
                // the initializer without contextual type, which widens literal
                // types (e.g., "ok" -> string) and overwrites node_types. By
                // seeding node_types here, subsequent context-free lookups
                // (including flow analysis for assignment narrowing, and the
                // fingerprint policy's callable-initializer detection in
                // variable_declaration_anchor) reuse the contextually-inferred
                // result. This must run in release builds too — the prior
                // #[cfg(debug_assertions)] gate caused fingerprint divergence.
                if !request.is_empty() && init_type != TypeId::ERROR {
                    self.ctx.node_types.insert(facts.initializer.0, init_type);
                }
                let (mut init_type_for_relation, remapped_mapped_initializer) = if self
                    .ctx
                    .arena
                    .get(facts.initializer)
                    .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                {
                    self.maybe_clear_checked_initializer_type_cache(facts.initializer);
                    let raw_init_type =
                        self.get_type_of_node_with_request(facts.initializer, &TypingRequest::NONE);
                    if crate::query_boundaries::common::is_remapped_mapped_index_access(
                        self.ctx.types,
                        raw_init_type,
                    ) {
                        (raw_init_type, true)
                    } else {
                        (self.resolve_lazy_type(init_type), false)
                    }
                } else {
                    (self.resolve_lazy_type(init_type), false)
                };
                let jsdoc_new_expression_relation = jsdoc_declared_type.is_some()
                    && facts.annotation.is_none()
                    && self
                        .ctx
                        .arena
                        .get(facts.initializer)
                        .is_some_and(|node| node.kind == syntax_kind_ext::NEW_EXPRESSION);
                if jsdoc_new_expression_relation {
                    let raw_init_snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
                    // Preserve the contextual cache entry. The raw probe below
                    // runs with TypingRequest::NONE and repopulates node_types
                    // with a non-contextual initializer type.
                    let saved_initializer_node_type =
                        self.ctx.node_types.get(&facts.initializer.0).copied();
                    self.maybe_clear_checked_initializer_type_cache(facts.initializer);
                    let raw_init_type =
                        self.get_type_of_node_with_request(facts.initializer, &TypingRequest::NONE);
                    raw_init_snap.rollback(&mut self.ctx.diagnostic_state());
                    if let Some(saved) = saved_initializer_node_type {
                        self.ctx.node_types.insert(facts.initializer.0, saved);
                    } else {
                        self.ctx.node_types.remove(&facts.initializer.0);
                    }
                    init_type_for_relation = self.resolve_lazy_type(raw_init_type);
                }
                let jsdoc_object_initializer_relation = jsdoc_declared_type.is_some()
                    && facts.annotation.is_none()
                    && self.initializer_reaches_object_literal_through_wrappers(facts.initializer);
                if jsdoc_object_initializer_relation {
                    let raw_init_snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
                    let saved_initializer_node_type =
                        self.ctx.node_types.get(&facts.initializer.0).copied();
                    self.maybe_clear_checked_initializer_type_cache(facts.initializer);
                    let raw_init_type =
                        self.get_type_of_node_with_request(facts.initializer, &TypingRequest::NONE);
                    raw_init_snap.rollback(&mut self.ctx.diagnostic_state());
                    if let Some(saved) = saved_initializer_node_type {
                        self.ctx.node_types.insert(facts.initializer.0, saved);
                    } else {
                        self.ctx.node_types.remove(&facts.initializer.0);
                    }
                    init_type_for_relation = self.resolve_lazy_type(raw_init_type);
                }
                if let Some(branch_ranges) = conditional_branch_ranges {
                    // Preserve non-assignability diagnostics from the branch expressions
                    // (e.g. TS2352/TS2873), but drop premature TS2322s produced while
                    // contextually typing the individual branches. The outer variable
                    // declaration check should report the canonical whole-expression error.
                    init_snap.rollback_filtered(&mut self.ctx.diagnostic_state(), |diag| {
                        let in_branch = branch_ranges
                            .iter()
                            .flatten()
                            .any(|(start, end)| diag.start >= *start && diag.start < *end);
                        !(in_branch && diag.code == 2322)
                    });
                }
                let function_initializer_body_has_error = self
                    .ctx
                    .arena
                    .get(facts.initializer)
                    .and_then(|init_node| {
                        if !matches!(
                            init_node.kind,
                            syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                        ) {
                            return None;
                        }
                        let func = self.ctx.arena.get_function(init_node)?;
                        let body_node = self.ctx.arena.get(func.body)?;
                        Some(
                            self.ctx.diagnostics[init_diagnostics_len..]
                                .iter()
                                .any(|diag| {
                                    diag.start >= body_node.pos
                                        && diag.start < body_node.end
                                        && matches!(diag.code, 2322 | 2339)
                                }),
                        )
                    })
                    .unwrap_or(false);
                // Check assignability (skip for 'any' since anything is assignable to any,
                // and skip for TypeId::ERROR since the type annotation failed to resolve).
                // Note: we intentionally do NOT use type_contains_error() here because it
                // recursively traverses all method/property types — interfaces like String
                // have methods that reference unresolved lib types (e.g. Intl.CollatorOptions),
                // causing type_contains_error to return true even though the declared type
                // itself (String interface) is perfectly valid for assignability checking.
                if declared_type != TypeId::ANY && declared_type != TypeId::ERROR {
                    // Augment function initializer with expando properties (suppresses spurious TS2741).
                    let checked_init_type = if initializer_is_function
                        && let Some(ref name) = facts.var_name
                        && let Some(sym_id) = self.ctx.binder.get_node_symbol(facts.decl_idx)
                    {
                        self.augment_callable_type_with_expandos(
                            name,
                            sym_id,
                            init_type_for_relation,
                        )
                    } else {
                        init_type_for_relation
                    };
                    if let Some((source_level, target_level)) = self
                        .constructor_accessibility_mismatch_for_var_decl_by_nodes(
                            facts.name,
                            facts.annotation,
                            facts.initializer,
                        )
                    {
                        self.error_constructor_accessibility_not_assignable(
                            checked_init_type,
                            declared_type,
                            source_level,
                            target_level,
                            facts.decl_idx,
                        );
                    } else if facts.is_destructuring {
                        // For destructuring patterns, try element-level elaboration first
                        // (tsc reports TS2322 on each mismatching element), then fall back
                        // to a generic TS2322 error.
                        if !self.try_elaborate_initializer_elements(
                            checked_init_type,
                            declared_type,
                            facts.initializer,
                        ) {
                            // For object-literal initializers, try per-property elaboration
                            // before the generic fallback. tsc reports TS2322 per mismatching
                            // property (e.g. `var {a}: {a: number} = {a: true}` → error at
                            // `a` in the RHS literal), not a single error at the binding
                            // pattern. Mirror the same guard used in the non-destructuring
                            // path (lines below).
                            let elaborated_obj =
                                self.initializer_reaches_object_literal_through_wrappers(
                                    facts.initializer,
                                ) && !self.is_assignable_to(checked_init_type, declared_type)
                                    && self.try_elaborate_object_literal_properties_for_var_init(
                                        facts.initializer,
                                        declared_type,
                                    );
                            if !elaborated_obj {
                                let skip_generic_outer_error = self
                                    .ctx
                                    .arena
                                    .get(facts.name)
                                    .and_then(|pattern_node| {
                                        self.ctx.arena.get_binding_pattern(pattern_node)
                                    })
                                    .is_some_and(|pattern| {
                                        pattern.elements.nodes.is_empty()
                                            && facts.annotation.is_some()
                                    });
                                if !skip_generic_outer_error {
                                    let _ = self.check_assignable_or_report_generic_at(
                                        checked_init_type,
                                        declared_type,
                                        facts.initializer,
                                        facts.decl_idx,
                                    );
                                }
                            }
                        }
                    } else {
                        let excess_property_target = if facts.annotation.is_some() {
                            self.excess_property_target_from_type_annotation(facts.annotation)
                                .unwrap_or(declared_type)
                        } else {
                            declared_type
                        };
                        let handled_discriminated = self.try_discriminated_union_excess_check(
                            checked_init_type,
                            excess_property_target,
                            facts.initializer,
                        );
                        if handled_discriminated {
                            // Discriminated union excess property check handled the error.
                            // tsc reports TS2353 against the narrowed member instead of
                            // a generic TS2322 for these cases.
                        } else {
                            let elaborated_elements = self.try_elaborate_initializer_elements(
                                checked_init_type,
                                declared_type,
                                facts.initializer,
                            );
                            if !(elaborated_elements
                                || function_initializer_body_has_error
                                || (initializer_is_function
                                    && jsdoc_declared_type.is_some()
                                    && self.async_function_jsdoc_return_type_suppression(
                                        checked_init_type,
                                        declared_type,
                                    )))
                                && !(initializer_is_function
                                    && !self.is_assignable_to(checked_init_type, declared_type)
                                    && self.try_elaborate_assignment_source_error(
                                        facts.initializer,
                                        declared_type,
                                    ))
                            {
                                // TS2353 (excess property) takes priority over TS2741/TS2322.
                                let diags_before = self.ctx.diagnostics.len();
                                self.check_object_literal_excess_properties(
                                    checked_init_type,
                                    excess_property_target,
                                    facts.initializer,
                                );
                                if self.ctx.diagnostics.len() == diags_before {
                                    // Only attempt elaboration when overall assignment fails AND
                                    // the initializer reaches an object literal through paren or
                                    // comma-expression wrappers (e.g. `var x: T = (void 0, {...})`).
                                    // The wrapper gate is required: calling `is_assignable_to`
                                    // on unrelated initializers (`null as any`, identifiers, ...)
                                    // has cache side-effects that perturb downstream JSX and
                                    // contextual-typing decisions (`callsOnComplexSignatures`).
                                    if !(self.initializer_reaches_object_literal_through_wrappers(
                                        facts.initializer,
                                    ) && !self
                                        .is_assignable_to(checked_init_type, declared_type)
                                        && self
                                            .try_elaborate_object_literal_properties_for_var_init(
                                                facts.initializer,
                                                declared_type,
                                            ))
                                    {
                                        // Disable callable-with-type-params suppression
                                        // for variable declarations. The suppression is
                                        // designed for class member checks (TS2416/TS2720)
                                        // but incorrectly hides real TS2322 errors when
                                        // a callable with outer-scope type params is
                                        // assigned to a concrete callable target.
                                        // (e.g., (cb: (x: string, ...rest: T) => void) => void
                                        //   vs (cb: (...args: never) => void) => void)
                                        if jsdoc_new_expression_relation
                                            && !self
                                                .is_assignable_to(checked_init_type, declared_type)
                                        {
                                            self.error_type_not_assignable_generic_at(
                                                checked_init_type,
                                                declared_type,
                                                facts.decl_idx,
                                            );
                                        } else if !self
                                            .type_contains_invalid_mapped_key_type(declared_type)
                                        {
                                            self.ctx.skip_callable_type_param_suppression.set(true);
                                            // Match tsc: explicit type assertions (`as`/`<T>`/`satisfies`)
                                            // on the initializer are opaque for elaboration — anchor at
                                            // the binding with outer types rather than drilling.
                                            let _ = if self
                                                .initializer_is_type_assertion(facts.initializer)
                                            {
                                                self.check_assignable_or_report_at_without_source_elaboration(
                                                    checked_init_type, declared_type, facts.initializer, facts.decl_idx,
                                                )
                                            } else if remapped_mapped_initializer {
                                                self.error_type_not_assignable_generic_at(
                                                    checked_init_type,
                                                    declared_type,
                                                    facts.decl_idx,
                                                );
                                                false
                                            } else {
                                                self.check_assignable_or_report_at(
                                                    checked_init_type,
                                                    declared_type,
                                                    facts.initializer,
                                                    facts.decl_idx,
                                                )
                                            };
                                            self.ctx
                                                .skip_callable_type_param_suppression
                                                .set(false);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Note: Freshness is tracked by the TypeId flags.
                // Fresh vs non-fresh object types are interned distinctly.
            }
            // `const k: unique symbol = Symbol()` — create a proper UniqueSymbol
            // type using the variable's binder symbol as identity.
            if declared_type == TypeId::SYMBOL
                && self.is_const_variable_declaration(facts.decl_idx)
                && self.is_unique_symbol_type_annotation(facts.annotation)
                && let Some(sym_id) = self.ctx.binder.get_node_symbol(facts.decl_idx)
            {
                return (
                    self.ctx
                        .types
                        .unique_symbol(tsz_solver::SymbolRef(sym_id.0)),
                    jsdoc_declared_type,
                );
            }
            return (declared_type, jsdoc_declared_type);
        }
        if facts.initializer.is_some() {
            self.report_malformed_jsdoc_satisfies_tags(facts.decl_idx);
            self.report_duplicate_jsdoc_satisfies_tags(facts.decl_idx);
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(facts.decl_idx)
                && let Some(enum_element_type) =
                    self.jsdoc_enum_annotation_type_for_symbol_decl(sym_id, facts.decl_idx)
            {
                self.check_jsdoc_enum_initializer_values(facts.initializer, enum_element_type);
            }
            // JSDoc @satisfies on variable declarations: provide contextual type
            // for the initializer so that object literal methods and arrow function
            // parameters get contextually typed from the satisfies type.
            // This mirrors the `satisfies Expr` TypeScript syntax behavior.
            let satisfies_info = self.jsdoc_satisfies_annotation_with_pos(facts.decl_idx);
            if let Some((sat_type, keyword_pos)) = satisfies_info {
                let request = TypingRequest::with_contextual_type(sat_type);
                let init_type = self.get_type_of_node_with_request(facts.initializer, &request);
                // Check satisfies assignability
                self.ensure_relation_input_ready(init_type);
                self.ensure_relation_input_ready(sat_type);
                if !self.type_contains_error(sat_type) {
                    let _ = self.check_satisfies_assignable_or_report(
                        init_type,
                        sat_type,
                        facts.initializer,
                        Some(keyword_pos),
                    );
                }
                return (init_type, jsdoc_declared_type);
            }
            self.maybe_clear_checked_initializer_type_cache(facts.initializer);
            // When the binding pattern contains array sub-patterns and the
            // initializer has matching array literals, provide a contextual type
            // so array literals produce positional (tuple) types instead of widened
            // union arrays.  This matches tsc: `var [a, b] = [1, "hello"]` infers
            // a=number, b=string (tuple), not a=string|number (array).
            let request = if facts.is_destructuring {
                self.declaration_pattern_initializer_request(
                    facts.name,
                    facts.initializer,
                    typing_request,
                )
            } else {
                self.redeclaration_initializer_request(
                    facts.decl_idx,
                    facts.name,
                    facts.initializer,
                )
            };
            let preserve_initializer_overload_diagnostics = self
                .ctx
                .arena
                .kind_at(facts.name)
                .is_some_and(|kind| kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                && !self
                    .initializer_supports_binding_pattern_context(facts.name, facts.initializer);
            if preserve_initializer_overload_diagnostics {
                self.invalidate_expression_for_contextual_retry(facts.initializer);
            }
            let prev_preserve_overloads = self
                .ctx
                .preserve_destructuring_initializer_overload_diagnostics;
            self.ctx
                .preserve_destructuring_initializer_overload_diagnostics =
                prev_preserve_overloads || preserve_initializer_overload_diagnostics;
            let mut init_type = self.get_type_of_node_with_request(facts.initializer, &request);
            self.ctx
                .preserve_destructuring_initializer_overload_diagnostics = prev_preserve_overloads;
            // TypeScript treats unannotated empty-array declaration initializers
            // (`let/var/const x = []`) as evolving-any arrays for subsequent writes.
            // Keep expression-level `[]` behavior unchanged by only applying this to
            // direct declaration initializers.
            let init_is_direct_empty_array =
                self.ctx
                    .arena
                    .get(facts.initializer)
                    .is_some_and(|init_node| {
                        init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            && self
                                .ctx
                                .arena
                                .get_literal_expr(init_node)
                                .is_some_and(|lit| lit.elements.nodes.is_empty())
                    });
            if init_is_direct_empty_array
                && query::array_element_type(self.ctx.types, init_type) == Some(TypeId::NEVER)
            {
                init_type = self.ctx.types.factory().array(TypeId::ANY);
            }
            // When strictNullChecks is off, undefined and null widen to any
            // (TypeScript treats `var x = undefined` as `any` without strict)
            if !self.ctx.strict_null_checks()
                && (init_type == TypeId::UNDEFINED || init_type == TypeId::NULL)
            {
                return (TypeId::ANY, jsdoc_declared_type);
            }
            let direct_nullish_initializer = self
                .literal_type_from_initializer(facts.initializer)
                .is_some_and(|ty| ty == TypeId::UNDEFINED || ty == TypeId::NULL);
            // Under noImplicitAny, mutable unannotated bindings initialized with
            // `undefined`/`null` should behave like evolving-any variables so later
            // assignments don't produce TS2322 (TypeScript reports implicit-any diagnostics).
            if self.ctx.no_implicit_any()
                && !self.is_const_variable_declaration(facts.decl_idx)
                && facts.annotation.is_none()
                && direct_nullish_initializer
                && (init_type == TypeId::UNDEFINED || init_type == TypeId::NULL)
            {
                return (TypeId::ANY, jsdoc_declared_type);
            }
            // Note: Freshness is tracked by the TypeId flags.
            // Fresh vs non-fresh object types are interned distinctly.
            if self.is_const_variable_declaration(facts.decl_idx) {
                // When the initializer type is `any` or `unknown` (e.g. from
                // a JSDoc `@type {*}` cast), the assertion determines the type.
                // `literal_type_from_initializer` looks through parenthesized
                // expressions and would find the inner literal (`null`), incorrectly
                // overriding the cast result.
                if init_type != TypeId::ANY
                    && init_type != TypeId::UNKNOWN
                    && let Some(literal_type) =
                        self.literal_type_from_initializer(facts.initializer)
                {
                    return (literal_type, jsdoc_declared_type);
                }
                // `const k = Symbol()` / `const k = Symbol.for(...)` — infer
                // unique symbol type. In TypeScript, unannotated const
                // declarations initialized with global symbol factory calls get
                // a unique symbol type (typeof k), not the general `symbol` type.
                if (self.is_symbol_call_initializer(facts.initializer)
                    || self.is_symbol_for_call_initializer(facts.initializer))
                    && let Some(sym_id) = self.ctx.binder.get_node_symbol(facts.decl_idx)
                {
                    return (
                        self.ctx
                            .types
                            .unique_symbol(tsz_solver::SymbolRef(sym_id.0)),
                        jsdoc_declared_type,
                    );
                }
                return (init_type, jsdoc_declared_type);
            }
            // Only widen when the initializer is a "fresh" literal expression
            // (direct literal in source code). Types from variable references,
            // narrowing, or computed expressions are "non-fresh" and NOT widened.
            // EXCEPTION: Enum member types are always widened for mutable bindings.
            let is_enum_member = self.is_enum_member_type_for_widening(init_type);
            let widened = if is_enum_member || self.is_fresh_literal_expression(facts.initializer) {
                self.widen_initializer_type_for_mutable_binding(init_type)
            } else {
                init_type
            };
            // Route null/undefined widening through the flow observation boundary.
            let final_type = flow_boundary::widen_null_undefined_to_any(
                self.ctx.types,
                widened,
                self.ctx.strict_null_checks(),
            );
            (final_type, jsdoc_declared_type)
        } else {
            // For for-in/for-of loop variables, the element type has already been cached
            // by assign_for_in_of_initializer_types. Use that instead of defaulting to any.
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(facts.decl_idx)
                && let Some(&cached) = self.ctx.symbol_types.get(&sym_id)
                && cached != TypeId::ANY
                && cached != TypeId::ERROR
            {
                return (cached, jsdoc_declared_type);
            }
            (declared_type, jsdoc_declared_type)
        }
    }
}
