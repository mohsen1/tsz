//! For-in / for-of loop variable checking.
//!
//! Extracted from `core.rs` to keep that file focused on
//! general variable declaration checking (`check_variable_declaration`).

use crate::context::TypingRequest;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Eq, PartialEq)]
enum ForOfProtocolRole {
    Iterable,
    Iterator,
}

impl ForOfProtocolRole {
    const fn tag(self) -> u8 {
        match self {
            Self::Iterable => 0,
            Self::Iterator => 1,
        }
    }
}

struct ForOfProtocolCollector<'c> {
    sites: &'c mut Vec<NodeIndex>,
    /// Role tag included to avoid revisiting the same symbol in a different protocol role.
    visited_symbols: &'c mut FxHashSet<(SymbolId, u8)>,
    visited_holders: &'c mut FxHashSet<(NodeIndex, u8)>,
}

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_for_of_header_expression_symbol(
        &self,
        idx: NodeIndex,
    ) -> Option<SymbolId> {
        let name = self.ctx.arena.get_identifier_at(idx)?.escaped_text.as_str();
        let mut current = idx;

        while current.is_some() {
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            let parent = ext.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                && let Some(for_data) = self.ctx.arena.get_for_in_of(parent_node)
                && for_data.expression == current
            {
                let list_node = self.ctx.arena.get(for_data.initializer)?;
                if list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                    return None;
                }
                let list = self.ctx.arena.get_variable(list_node)?;
                for &decl_idx in &list.declarations.nodes {
                    let decl_node = match self.ctx.arena.get(decl_idx) {
                        Some(node) => node,
                        None => continue,
                    };
                    let var_decl = match self.ctx.arena.get_variable_declaration(decl_node) {
                        Some(decl) => decl,
                        None => continue,
                    };
                    let name_node = match self.ctx.arena.get(var_decl.name) {
                        Some(node) => node,
                        None => continue,
                    };
                    if name_node.kind != SyntaxKind::Identifier as u16 {
                        continue;
                    }
                    let ident = match self.ctx.arena.get_identifier(name_node) {
                        Some(ident) => ident,
                        None => continue,
                    };
                    if ident.escaped_text.as_str() != name {
                        continue;
                    }
                    return self
                        .ctx
                        .binder
                        .get_node_symbol(decl_idx)
                        .or_else(|| self.ctx.binder.get_node_symbol(var_decl.name))
                        .or_else(|| {
                            self.ctx
                                .binder
                                .resolve_identifier(self.ctx.arena, var_decl.name)
                        });
                }
                return None;
            }
            current = parent;
        }

        None
    }

    pub(crate) fn is_in_for_of_header_expression_of_declaration(
        &self,
        usage_idx: NodeIndex,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(decl_info) = self.ctx.arena.node_info(decl_idx) else {
            return false;
        };
        let decl_list_idx = decl_info.parent;
        let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx) else {
            return false;
        };
        if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        let Some(for_info) = self.ctx.arena.node_info(decl_list_idx) else {
            return false;
        };
        let for_idx = for_info.parent;
        let Some(for_node) = self.ctx.arena.get(for_idx) else {
            return false;
        };
        if for_node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_data) = self.ctx.arena.get_for_in_of(for_node) else {
            return false;
        };

        let mut current = usage_idx;
        while current.is_some() {
            if current == for_data.expression {
                return true;
            }
            if current == for_idx {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }

        false
    }

    pub(crate) fn is_deferred_object_like_for_in(&mut self, expr_type: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        if query::is_type_parameter_like(self.ctx.types, expr_type)
            || query::is_object_like_type(self.ctx.types, expr_type)
        {
            return true;
        }

        if let Some((base, _index)) = query::get_index_access_types(self.ctx.types, expr_type) {
            let evaluated_base = self.evaluate_type_with_env(base);
            return query::is_type_parameter_like(self.ctx.types, base)
                || query::is_type_parameter_like(self.ctx.types, evaluated_base)
                || query::is_object_like_type(self.ctx.types, evaluated_base);
        }

        // No union arm: a union operand is quantified with ALL, not ANY, and is
        // owned by `for_in_expr_type_is_valid_union`. An ANY arm here would
        // re-accept `string | object` through the leaf path and silently
        // bypass that predicate.

        if let Some(members) = query::intersection_members(self.ctx.types, expr_type) {
            return members
                .iter()
                .any(|&member| self.is_deferred_object_like_for_in(member));
        }

        false
    }

    /// Assign the inferred loop-variable type for `for-in` / `for-of` initializers.
    ///
    /// The initializer is a `VariableDeclarationList` in the Thin AST.
    /// `is_for_in` should be true for for-in loops (to emit TS2404 on type annotations).
    pub(crate) fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        element_type: TypeId,
        is_for_in: bool,
    ) {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return;
        };
        // When there are multiple declarations, TS1188 is already reported by the parser.
        // TSC suppresses per-declaration grammar errors (TS1189/TS1190/TS2483) in this case.
        let single_declaration = list.declarations.nodes.len() == 1;
        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };

            // TS1189/TS1190: The variable declaration of a for-in/for-of statement cannot have an initializer
            // Only check when there's a single declaration (TSC suppresses when TS1188 is reported)
            // tsc anchors at the variable name (not the initializer expression).
            if single_declaration && var_decl.initializer.is_some() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                if is_for_in {
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                        diagnostic_codes::THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    );
                } else {
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::THE_VARIABLE_DECLARATION_OF_A_FOR_OF_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                        diagnostic_codes::THE_VARIABLE_DECLARATION_OF_A_FOR_OF_STATEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    );
                }
            }

            // If there's a type annotation, check that the element type is assignable to it
            if var_decl.type_annotation.is_some() {
                // TS2404: The left-hand side of a 'for...in' statement cannot use a type annotation
                // TSC emits TS2404 and skips the assignability check for for-in loops.
                // TS2483: The left-hand side of a 'for...of' statement cannot use a type annotation
                // Only check with single declaration (TSC suppresses when TS1188 is reported)
                if is_for_in && single_declaration {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    // tsc anchors this error at the `: type` annotation (including
                    // the colon). Our type_annotation node only covers the type
                    // itself (excluding colon). Use the variable name node — its
                    // end position is the colon, giving the closest match to tsc.
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                    );
                } else if !is_for_in && single_declaration {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                    );
                }

                let declared = self.get_type_from_type_node(var_decl.type_annotation);

                // TS2322: Check that element type is assignable to declared type
                // Skip for for-in loops — TSC only emits TS2404 (no assignability check).
                if !is_for_in
                    && declared != TypeId::ANY
                    && !self.type_contains_error(declared)
                    && !self.check_assignable_or_report(element_type, declared, var_decl.name)
                {
                    // Diagnostic emitted by check_assignable_or_report.
                }

                // Assign types for binding patterns (e.g., `for (const [a] of arr)`).
                if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    let binding_request = if declared != TypeId::ANY
                        && declared != TypeId::UNKNOWN
                        && declared != TypeId::ERROR
                    {
                        TypingRequest::with_contextual_type(declared)
                    } else {
                        TypingRequest::NONE
                    };
                    // TS2488: For array binding patterns, check if the element type is iterable
                    // Example: for (const [,] of []) where [] has type never[] with element type never
                    if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        use tsz_parser::NodeIndex;
                        self.check_destructuring_iterability(
                            var_decl.name,
                            declared,
                            NodeIndex::NONE,
                        );
                    }
                    self.assign_binding_pattern_symbol_types_with_request(
                        var_decl.name,
                        declared,
                        &binding_request,
                    );
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, declared);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, declared);
                }
            } else {
                // No type annotation - use element type (with freshness stripped)
                let widened_element_type = if !self.ctx.compiler_options.sound_mode {
                    crate::query_boundaries::common::widen_freshness(self.ctx.types, element_type)
                } else {
                    element_type
                };

                // Assign types for binding patterns (e.g., `for (const [a] of arr)`).
                if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    let binding_request = if widened_element_type != TypeId::ANY
                        && widened_element_type != TypeId::UNKNOWN
                        && widened_element_type != TypeId::ERROR
                    {
                        TypingRequest::with_contextual_type(widened_element_type)
                    } else {
                        TypingRequest::NONE
                    };
                    // TS2488: For array binding patterns, check if the element type is iterable
                    // Example: for (const [,] of []) where [] has type never[] with element type never
                    if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        use tsz_parser::NodeIndex;
                        self.check_destructuring_iterability(
                            var_decl.name,
                            widened_element_type,
                            NodeIndex::NONE,
                        );
                    }
                    self.assign_binding_pattern_symbol_types_with_request(
                        var_decl.name,
                        widened_element_type,
                        &binding_request,
                    );
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, widened_element_type);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, widened_element_type);
                }
            }
        }
    }

    /// TS2407: The right-hand side of a 'for...in' statement must be of type 'any',
    /// an object type or a type parameter.
    pub(crate) fn check_for_in_expression_type(
        &mut self,
        expr_type: TypeId,
        expression: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // Skip if type is error
        if expr_type == TypeId::ERROR {
            return;
        }

        // tsc validates the *unreduced* for-in operand: `checkForInStatement` never
        // routes the right-hand side through `getReducedType`, so an object-type
        // intersection whose members carry a disjoint discriminant (which tsz reduces
        // to `never` — at intern time for concrete members via
        // `intersection_has_disjoint_object_literals`, or through property-access
        // resolution for generic-application members) is still a valid for-in RHS:
        // it is assignable to `object`. Judge the intersection on its raw, pre-reduction
        // structure (resolving each member individually so a generic application like
        // `WithKind<'a'>` exposes its object shape) BEFORE `resolve_type_for_property_access`
        // below folds the whole operand to `never` and hides the object-like members.
        let raw_intersection_is_valid = self.for_in_expr_type_is_valid_intersection(expr_type);

        // Resolve lazy/application types before checking (e.g. Record<string, any>).
        // This resolved form is for the validity checks below ONLY: tsc's
        // `checkForInStatement` reports the RHS's `checkExpression` result verbatim
        // (`typeToString(rightType)`), so a fresh literal (`for (var i in 1)`) must
        // keep displaying `'1'`, not the widened `'number'` this resolution step
        // produces for property-access purposes. `expr_type` (the un-resolved
        // parameter) stays the one used in the message below.
        let resolved_expr_type = self.resolve_type_for_property_access(expr_type);

        // Valid types: any, unknown, object (non-primitive), object types, type parameters
        // Invalid types: primitive types (void, null, undefined, number, string, boolean,
        // bigint, symbol) and `never` (tsc reports TS2407 for `never` as well)
        let is_valid = raw_intersection_is_valid
            || self.for_in_leaf_type_is_valid(resolved_expr_type)
            // Also allow union types that contain valid types
            || self.for_in_expr_type_is_valid_union(resolved_expr_type)
            // A deferred alias that resolves to an object intersection is valid if ANY
            // member is object-like (the raw operand above was not itself an intersection).
            || self.for_in_expr_type_is_valid_intersection(resolved_expr_type);

        // Checked only on the error path, so an accepted operand never pays for
        // the walk: tsc derives a for-in loop variable's type from the loop's
        // own operand (`getIndexType(checkExpression(node.expression))` in
        // `getTypeForVariableLikeDeclaration`), so an operand naming a variable
        // this same loop head declares is a circular resolution —
        // `pushTypeResolution` fails, `reportCircularityError` hands back `any`
        // (reporting TS7022 only under `noImplicitAny`), and `any` clears this
        // gate. tsz resolves the loop variable to its `string` key type instead,
        // which is a primitive and tripped a false TS2407 on `for (var of in of)`
        // and on `recursiveLetConst.ts`'s `for (let v in v)`.
        if !is_valid && !self.for_in_operand_resolution_is_circular(expression) {
            // tsc's `checkForInStatement` reports
            // `typeToString(getNonNullableTypeIfNeeded(checkExpression(node.expression)))`
            // — the RHS's own checked type, not the widened form the validity
            // checks above use. Two corrections on top of `expr_type` (which is
            // already widened by the time it reaches this function, e.g. a
            // fresh numeric literal `1` arrives as `number`):
            // - `getNonNullableTypeIfNeeded` collapses a bare `null`/`undefined`
            //   RHS to `never` before display, in both strict and non-strict
            //   mode (verified against `tsc` 7.0.2 both ways).
            // - Any other fresh literal keeps its literal spelling
            //   (`for (var i in 1)` → `'1'`, not `'number'`); recovered from the
            //   operand node the same way `emit_ts2488_not_iterable` does for
            //   for-of's analogous message.
            let display_type = if expr_type == TypeId::NULL || expr_type == TypeId::UNDEFINED {
                TypeId::NEVER
            } else {
                self.literal_type_from_initializer(expression)
                    .unwrap_or(expr_type)
            };
            let type_str = self.format_type(display_type);
            let message = format_message(
                diagnostic_messages::THE_RIGHT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYPE_OR,
                &[&type_str],
            );
            self.error_at_node(expression, &message, diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYPE_OR);
        }
    }

    /// Whether the for-in operand at `expression` resolves circularly: it
    /// references a variable declared by the very loop head it belongs to.
    ///
    /// Keyed on binder symbol identity (through the same
    /// `expression_references_symbol` walk the for-of circularity check uses),
    /// never on the identifier's spelling — a differently-named loop variable
    /// over an unrelated binding of any type is untouched by this.
    fn for_in_operand_resolution_is_circular(&mut self, expression: NodeIndex) -> bool {
        let Some(decl_list_idx) = self.for_in_statement_declaration_list(expression) else {
            return false;
        };
        !self
            .for_in_circular_loop_head_declarations(decl_list_idx, expression)
            .is_empty()
    }

    /// The `VariableDeclarationList` of the for-in statement whose operand is
    /// `expression`, when the loop head declares its variables inline.
    fn for_in_statement_declaration_list(&self, expression: NodeIndex) -> Option<NodeIndex> {
        let arena = self.ctx.arena;
        let statement_idx = arena.get_extended(expression)?.parent;
        let statement = arena.get(statement_idx)?;
        if statement.kind != syntax_kind_ext::FOR_IN_STATEMENT {
            return None;
        }
        let initializer = arena.get_for_in_of(statement).map(|f| f.initializer)?;
        arena.get(initializer)?;
        Some(initializer)
    }

    /// Name nodes of the loop-head declarations that the for-in operand itself
    /// references — the declarations whose type resolution is circular.
    ///
    /// tsc derives a for-in loop variable's type from the loop's own operand
    /// (`getIndexType(checkExpression(node.expression))` in
    /// `getTypeForVariableLikeDeclaration`), so an operand naming a variable
    /// this same loop head declares cannot be resolved: `pushTypeResolution`
    /// fails, `reportCircularityError` hands back `any`, and reports TS7022
    /// under `noImplicitAny`.
    ///
    /// Returning the declarations rather than a bare flag keeps the TS2407
    /// suppression and the TS7022 report driven by one predicate, so a circular
    /// operand can never be both silently accepted by the object-type gate and
    /// left unreported.
    ///
    /// Keyed on binder symbol identity, never on the identifier's spelling.
    fn for_in_circular_loop_head_declarations(
        &mut self,
        decl_list_idx: NodeIndex,
        expression: NodeIndex,
    ) -> Vec<NodeIndex> {
        let arena = self.ctx.arena;
        let Some(list) = arena.get(decl_list_idx).and_then(|n| arena.get_variable(n)) else {
            return Vec::new();
        };

        let mut circular = Vec::new();
        for &decl_idx in &list.declarations.nodes {
            let Some(var_decl) = arena
                .get(decl_idx)
                .and_then(|node| arena.get_variable_declaration(node))
            else {
                continue;
            };
            // An annotated loop variable is not circular: tsc reads the
            // annotation instead of the operand (and reports TS2404 for it).
            if var_decl.type_annotation.is_some() {
                continue;
            }
            let sym_id = self
                .ctx
                .binder
                .get_node_symbol(decl_idx)
                .or_else(|| self.ctx.binder.get_node_symbol(var_decl.name))
                .or_else(|| self.ctx.binder.resolve_identifier(arena, var_decl.name));
            if let Some(sym_id) = sym_id
                && self.expression_references_symbol(expression, sym_id)
            {
                circular.push(var_decl.name);
            }
        }
        circular
    }

    /// TS7022: report a for-in loop variable whose own operand references it.
    ///
    /// The for-of twin (`check_for_of_self_reference_circularity`) additionally
    /// walks iterator-protocol return sites; that does not apply here, because
    /// for-in has no iterator protocol. Both paths decide the reference itself
    /// through binder symbol identity only, never through the identifier's
    /// spelling — `for (const v in o.v) {}` is clean in tsc.
    pub(crate) fn check_for_in_self_reference_circularity(
        &mut self,
        decl_list_idx: NodeIndex,
        expression_idx: NodeIndex,
    ) {
        // TS7022 is an implicit-any diagnostic: tsc's `reportCircularityError`
        // reports it only `if (noImplicitAny && ...)`. With the flag off the
        // circular loop variable is silently `any`.
        if !self.ctx.no_implicit_any() {
            return;
        }
        for name_idx in self.for_in_circular_loop_head_declarations(decl_list_idx, expression_idx) {
            let Some(name) = self.get_identifier_text_from_idx(name_idx) else {
                continue;
            };
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                name_idx,
                diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                &[&name],
            );
        }
    }

    /// Leaf (non-union, non-intersection) validity test for a for-in operand:
    /// `any`, the `object` intrinsic, a type parameter, or any object-like
    /// type — including deferred index-access / generic-application forms.
    ///
    /// `unknown` is deliberately NOT accepted here: `tsc`'s
    /// `checkForInStatement` only special-cases `any` via `isTypeAny` before
    /// falling through to `allTypesAssignableToKind(rightType, NonPrimitive |
    /// InstantiableNonPrimitive)`, which `unknown` fails just like any other
    /// non-object type — `declare const u: unknown; for (const k in u) {}`
    /// reports TS2407 (oracle-verified, `tsc` 7.0.2, both strictness modes).
    ///
    /// `is_deferred_object_like_for_in` already returns `true` for both
    /// `is_type_parameter_like` and `is_object_like_type`, so this is the single
    /// predicate shared by the scalar and intersection-member checks.
    fn for_in_leaf_type_is_valid(&mut self, ty: TypeId) -> bool {
        ty == TypeId::ANY
            || ty == TypeId::OBJECT
            || self.for_in_operand_is_absorbed_nullable(ty)
            || self.is_deferred_object_like_for_in(ty)
    }

    /// Without `strictNullChecks`, `null` and `undefined` are members of every
    /// type, so `tsc`'s `allTypesAssignableToKind(rightType, NonPrimitive | ...)`
    /// gate accepts them and `for (var a in null) {}` is silent. With the flag
    /// on the operand is stripped to `never` and TS2407 *is* reported — so this
    /// is a flag-dependent assignability fact, not a claim that bottom types are
    /// acceptable operands (a declared `never` operand still reports in both
    /// modes). Verified against `tsc` 7.0.2 both ways.
    fn for_in_operand_is_absorbed_nullable(&self, ty: TypeId) -> bool {
        !self.ctx.compiler_options.strict_null_checks
            && (ty == TypeId::NULL || ty == TypeId::UNDEFINED)
    }

    /// Helper for TS2407: whether a *union* operand is a valid for-in RHS.
    ///
    /// `tsc` quantifies over a union with ALL, not ANY: `allTypesAssignableToKind`
    /// recurses into a union with `every`, so a single non-object constituent
    /// rejects the whole operand — `declare const u: string | object` reports
    /// TS2407 even though `object` on its own would be a fine operand. This
    /// predicate used to return on the FIRST valid member, which silently
    /// accepted every mixed union.
    ///
    /// Two constituent-level rules come with the quantifier and are not
    /// optional; both are oracle-verified against `tsc` 7.0.2 in
    /// `for_in_union_operand_tests.rs`:
    ///
    /// - `null` and `undefined` constituents are STRIPPED, not judged.
    ///   `checkForInStatement` reads
    ///   `getNonNullableTypeIfNeeded(checkExpression(...))`, so
    ///   `{ a: number } | undefined` is clean (and `string | undefined` reports
    ///   with type `'string'`). Without the strip, turning ANY into ALL would
    ///   invent a false positive on every optional object operand.
    /// - A type variable is accepted as a WHOLE operand by the type-parameter
    ///   arm of [`Self::for_in_leaf_type_is_valid`], but as a CONSTITUENT only
    ///   through an object-like base constraint. `function f<T>(u: T)` is clean
    ///   while `function f<T>(u: T | { a: number })` reports; adding a
    ///   constraint (`T extends object`, `T extends { a: number }`,
    ///   `T extends unknown[]`) makes the union clean again, and
    ///   `T extends string` keeps it reporting.
    ///
    /// A union with every constituent stripped (`null | undefined`) is `never`
    /// after the strip, which is not a valid operand — hence the
    /// `saw_unstripped_member` result rather than a vacuous `true`.
    fn for_in_expr_type_is_valid_union(&mut self, expr_type: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        let Some(members) = query::union_members(self.ctx.types, expr_type) else {
            return false;
        };

        let mut saw_unstripped_member = false;
        for &member in &members {
            if member == TypeId::NULL || member == TypeId::UNDEFINED {
                continue;
            }
            saw_unstripped_member = true;
            if !self.for_in_union_member_is_valid(member, &mut Vec::new()) {
                return false;
            }
        }
        saw_unstripped_member
    }

    /// Whether one constituent of a union for-in operand is itself acceptable.
    ///
    /// `tsc` reaches each constituent through
    /// `isTypeAssignableToKind(member, NonPrimitive | InstantiableNonPrimitive)`,
    /// whose object arm is plain assignability to `object`. So the test here is
    /// "is this member object-like", with a type variable contributing only its
    /// base constraint — deliberately stricter than
    /// [`Self::for_in_leaf_type_is_valid`], which judges the operand as a whole.
    ///
    /// `constraint_chain` records the type variables already walked so a
    /// mutually-recursive constraint (`T extends U`, `U extends T` — itself a
    /// TS2313 error) terminates instead of recursing forever.
    fn for_in_union_member_is_valid(
        &mut self,
        member: TypeId,
        constraint_chain: &mut Vec<TypeId>,
    ) -> bool {
        if self.for_in_union_member_is_valid_shallow(member, constraint_chain) {
            return true;
        }
        // Deferred members (aliases, generic applications like `Box<number>`)
        // expose their object shape only after resolution. Resolved
        // individually, exactly as `for_in_expr_type_is_valid_intersection`
        // does, so one member's resolution cannot collapse the whole union.
        let resolved = self.resolve_type_for_property_access(member);
        resolved != member && self.for_in_union_member_is_valid_shallow(resolved, constraint_chain)
    }

    /// One resolution-free step of [`Self::for_in_union_member_is_valid`].
    fn for_in_union_member_is_valid_shallow(
        &mut self,
        member: TypeId,
        constraint_chain: &mut Vec<TypeId>,
    ) -> bool {
        use crate::query_boundaries::dispatch as query;

        if member == TypeId::ANY {
            return true;
        }
        // A nested union carries the same all-constituents rule (including the
        // nullable strip), so route it back through the union predicate.
        if query::union_members(self.ctx.types, member).is_some() {
            return self.for_in_expr_type_is_valid_union(member);
        }
        if query::is_type_parameter_like(self.ctx.types, member) {
            if constraint_chain.contains(&member) {
                return false;
            }
            constraint_chain.push(member);
            let constraint = query::get_base_constraint_of_type(self.ctx.types, member);
            // An unconstrained type parameter answers `unknown` here, which is
            // not object-like — matching `tsc`, which reports on `T | { a }`.
            let valid = constraint != member
                && self.for_in_union_member_is_valid(constraint, constraint_chain);
            constraint_chain.pop();
            return valid;
        }
        query::is_object_like_type(self.ctx.types, member)
            // `A & B` is a subtype of each member, so an intersection
            // constituent is assignable to `object` as soon as ANY of its own
            // members is — the quantifier flips back for intersections.
            || self.for_in_expr_type_is_valid_intersection(member)
    }

    /// Helper for TS2407: Check if an intersection type is a valid for-in operand.
    ///
    /// tsc accepts an intersection RHS when it is assignable to `object` — which
    /// holds as soon as ANY constituent is object-like / type-parameter-like, since
    /// `A & B` is a subtype of each member. `object & T` is valid because it contains
    /// `object`; `WithKind<'a'> & WithKind<'b'>` is valid because each member is an
    /// object type, even though their disjoint discriminant reduces the intersection
    /// to `never`. Each member is resolved for property access *individually* so a
    /// deferred generic application exposes its object shape without collapsing the
    /// whole intersection to `never`.
    fn for_in_expr_type_is_valid_intersection(&mut self, expr_type: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        let Some(members) = query::intersection_members(self.ctx.types, expr_type) else {
            return false;
        };
        for &member in &members {
            if self.for_in_leaf_type_is_valid(member) {
                return true;
            }
            // Resolve deferred members (generic applications, aliases) individually.
            // Resolving a single member does NOT trigger the whole-intersection
            // never-collapse, so `WithKind<'a'>` becomes the object type `{ kind: 'a' }`.
            let resolved_member = self.resolve_type_for_property_access(member);
            if resolved_member != member && self.for_in_leaf_type_is_valid(resolved_member) {
                return true;
            }
        }
        false
    }

    /// Whether an optional-chain access is rooted on an `any`/error receiver.
    ///
    /// tsc propagates `any` through an optional chain, so `any?.b.c` (or a chain
    /// whose root identifier is `any`) is itself `any`. Walking to the root and
    /// testing its type is more reliable than the chain's evaluated type for the
    /// for-in TS2405/TS2780 decision: a preceding invalid `root?.b = <literal>`
    /// can leave a stale assigned-value type on the chain result even though the
    /// root — and therefore the whole chain per tsc — is `any`.
    fn optional_chain_root_receiver_is_any_like(&mut self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        loop {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            let next = match node.kind {
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
                {
                    self.ctx.arena.get_access_expr(node).map(|a| a.expression)
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    self.ctx.arena.get_call_expr(node).map(|c| c.expression)
                }
                _ => break,
            };
            let Some(expr) = next else { return false };
            current = self.ctx.arena.skip_parenthesized_and_assertions(expr);
        }
        let root_type = self.get_type_of_node(current);
        root_type == TypeId::ANY || root_type == TypeId::ERROR
    }

    /// Check assignability for for-in/of expression initializer (non-declaration case).
    ///
    /// For `for (v of expr)` where `v` is a pre-declared variable (not `var v`/`let v`/`const v`),
    /// this checks:
    /// - TS2588: Cannot assign to const variable
    /// - TS2322: Element type not assignable to variable type
    pub(crate) fn check_for_in_of_expression_initializer(
        &mut self,
        initializer: NodeIndex,
        element_type: TypeId,
        is_for_of: bool,
        has_await_modifier: bool,
    ) {
        // TS1106: The left-hand side of a 'for...of' statement may not be 'async'.
        // `for (async of expr)` is ambiguous with `for await (... of ...)`.
        // With `for await`, the `async` identifier is unambiguous, so skip the check.
        if is_for_of
            && !has_await_modifier
            && let Some(init_node) = self.ctx.arena.get(initializer)
            && init_node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(init_node)
            && self.ctx.arena.resolve_identifier_text(ident) == "async"
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                initializer,
                diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_ASYNC,
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_ASYNC,
            );
        }

        // TS2781: The left-hand side of a 'for...of' statement may not be an
        // optional property access. tsc's `checkForOfStatement` calls
        // `checkReferenceExpression` unconditionally, so this fires regardless
        // of the chain's element type.
        //
        // The analogous TS2780 for `for...in` is NOT unconditional. tsc's
        // `checkForInStatement` computes the head's real type first and only
        // reaches `checkReferenceExpression` (the TS2780 source) in the `else`
        // branch of `if !isTypeAssignableTo(indexType, leftType) { TS2405 }
        // else { checkReferenceExpression(...) }` — so TS2405 wins whenever the
        // LHS type is not string/any, and TS2780 only fires once that type check
        // passes. The for-in TS2405/TS2780 selection is owned by the optional-
        // chain block below, which computes the head's read type to decide.
        if is_for_of && self.is_optional_chain_access(initializer) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                initializer,
                diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
            );
        }

        // TS2487: For-of LHS must be a variable or a property access.
        // Unlike for-in, for-of allows destructuring patterns (array/object literals).
        if is_for_of && let Some(init_node) = self.ctx.arena.get(initializer) {
            let unwrapped = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(initializer);
            let init_kind = self
                .ctx
                .arena
                .get(unwrapped)
                .map_or(init_node.kind, |n| n.kind);
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            use tsz_parser::parser::syntax_kind_ext;

            if init_kind != SyntaxKind::Identifier as u16
                && init_kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && init_kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && init_kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && init_kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                );
            }
        }

        // For-in specific LHS checks (TS2491, TS2406, TS2405)
        if !is_for_of && let Some(init_node) = self.ctx.arena.get(initializer) {
            // Unwrap parenthesized/satisfies/as wrappers before checking the kind,
            // so `for ((x satisfies string) in obj)` is treated like `for (x in obj)`.
            let unwrapped = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(initializer);
            let init_kind = self
                .ctx
                .arena
                .get(unwrapped)
                .map_or(init_node.kind, |n| n.kind);
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            use tsz_parser::parser::syntax_kind_ext;

            // TS2491: The left-hand side of a 'for...in' statement cannot be a destructuring pattern.
            if init_kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || init_kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
                );
            }
            // TS2406: The left-hand side of a 'for...in' statement must be a variable or a property access.
            else if init_kind != SyntaxKind::Identifier as u16
                && init_kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && init_kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                if init_kind == syntax_kind_ext::CALL_EXPRESSION
                    || init_kind == syntax_kind_ext::NEW_EXPRESSION
                    // TS2406 also fires for private identifiers (`for (#field in v)`)
                    // because private identifiers are not valid iteration variables.
                    || init_kind == SyntaxKind::PrivateIdentifier as u16
                {
                    self.error_at_node(
                        initializer,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                    );
                }
                // TS2405: The left-hand side of a 'for...in' statement must be of type 'string' or 'any'.
                // Applies to other expression types (BinaryExpression like `a=1`, `this`, etc.)
                else {
                    self.error_at_node(
                        initializer,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                    );
                }
            }
        }

        // TS2405: For for-in, also check that the LHS type is string or any.
        // This applies only to valid LHS forms (identifiers and property/element access).
        // Skip if we already emitted TS2491 (destructuring) or TS2406 (invalid form).
        // Also skip for optional chain accesses — TS2777 already covers those.
        if !is_for_of
            && !self.is_optional_chain_access(initializer)
            && let Some(_init_node) = self.ctx.arena.get(initializer)
            && {
                let unwrapped = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(initializer);
                self.ctx.arena.get(unwrapped).is_some_and(|n| {
                    let k = n.kind;
                    k == SyntaxKind::Identifier as u16
                        || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                })
            }
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let var_type = self.get_type_of_assignment_target(initializer);
            // The LHS type must accept the for-in element type. TSC checks
            // `isTypeAssignableTo(indexType, variableType)` where indexType
            // comes from the source expression's key type (keyof T & string
            // for generic expressions, plain string otherwise).
            // Using `element_type` instead of hardcoded `string` correctly
            // handles `keyof T`, `K extends string`, `K extends keyof T`, etc.
            if var_type != TypeId::STRING
                && var_type != TypeId::ANY
                && var_type != TypeId::UNKNOWN
                && !self
                    .for_in_lhs_relation_outcome(element_type, var_type)
                    .related
            {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                );
            }
        }

        // TS2405 vs TS2780 for an optional-chain for-in head. tsc gates these
        // mutually exclusively: `checkForInStatement` computes the head
        // expression's real type (`checkExpression`, a READ) and reports TS2405
        // when that type is not assignable from the index type (i.e. not
        // string/any), otherwise reaches `checkReferenceExpression`, which
        // reports TS2780 for the optional chain itself.
        //
        // The head's type is read through the value path (`get_type_of_node`),
        // NOT the write-target path (`get_type_of_assignment_target`): a for-in
        // optional-chain head is a write-target context, so that path
        // short-circuits to `any` (erasing the type this decision needs) and,
        // when forced to resolve the chain, runs write-flow probes that leak the
        // spurious TS2339/TS7053 that got #16660 reverted. The read path returns
        // the chain's genuine `T | undefined` type and emits only the
        // diagnostics tsc's `checkExpression` itself would.
        if !is_for_of && self.is_optional_chain_access(initializer) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let head_type = self.get_type_of_node(initializer);
            // `isTypeAssignableTo(indexType, leftType)` — TS2405 fires only when
            // the LHS type rejects the for-in key type. `unknown` and the error
            // type are permissive on both sides, matching tsc's assignability.
            //
            // `any` propagates through an optional chain: when the chain's root
            // receiver is `any`/error, the whole head is `any` (tsc), so the type
            // check passes and TS2780 owns the head. This is checked directly on
            // the root because an invalid prior `a?.b = <literal>` (itself TS2779)
            // can leave a stale assigned-value type on the chain result even on a
            // bare `any` receiver — the root's own type is the reliable witness of
            // the any-propagation `tsc` performs through the chain.
            let lhs_type_accepts_index = head_type == TypeId::STRING
                || head_type == TypeId::ANY
                || head_type == TypeId::UNKNOWN
                || self.optional_chain_root_receiver_is_any_like(initializer)
                || self
                    .for_in_lhs_relation_outcome(element_type, head_type)
                    .related;
            if lhs_type_accepts_index {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                );
            } else {
                self.error_at_node(
                    initializer,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                );
            }
        }

        // Get the type of the initializer expression (this evaluates `v`, `v++`, `obj.prop`, etc.)
        // For destructuring patterns (array/object literals), set the destructuring
        // target flag so that downstream checks (e.g. TS2698 spread validation in
        // object literals) correctly treat `{ ...x }` as a rest binding, not a spread.
        let is_destructuring_init = self.ctx.arena.get(initializer).is_some_and(|n| {
            n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        });
        let prev_destructuring = self.ctx.in_destructuring_target;
        if is_destructuring_init {
            self.ctx.in_destructuring_target = true;
        }
        let var_type = self.get_type_of_assignment_target(initializer);
        self.ctx.in_destructuring_target = prev_destructuring;
        let target_type = if is_for_of
            && let Some(init_node) = self.ctx.arena.get(initializer)
            && init_node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, initializer)
        {
            // For `for (x of y)` with pre-declared identifier `x`, compare against
            // the declared type of `x` (not the current flow-narrowed type).
            self.get_type_of_symbol(sym_id)
        } else {
            var_type
        };

        // TS2588: Cannot assign to const variable
        if is_for_of {
            self.check_const_assignment(initializer);
        }

        // TS2322: Expression-form `for (... of ...)` should follow the same
        // destructuring-assignment path as `({ ... } = value)`. In particular,
        // object-literal targets must validate each binding element separately
        // instead of synthesizing a whole-pattern assignability error.
        let is_array_destructuring_target = self
            .ctx
            .arena
            .get(initializer)
            .is_some_and(|n| n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION);
        let is_object_destructuring_target = self
            .ctx
            .arena
            .get(initializer)
            .is_some_and(|n| n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION);
        if is_for_of
            && is_object_destructuring_target
            && target_type != TypeId::ANY
            && element_type != TypeId::ANY
            && element_type != TypeId::ERROR
            && !self.type_contains_error(target_type)
        {
            self.check_object_destructuring_assignment_from_source_type(
                initializer,
                element_type,
                None,
            );
        } else if is_for_of
            && !is_array_destructuring_target
            && target_type != TypeId::ANY
            && element_type != TypeId::ANY
            && element_type != TypeId::ERROR
            && !self.type_contains_error(target_type)
        {
            let _ = self.check_assignable_or_report(element_type, target_type, initializer);
        }
    }

    /// TS2491: The left-hand side of a 'for...in' statement cannot be a destructuring pattern.
    /// Checks variable declaration list form: `for (let {a, b} in obj)`
    pub(crate) fn check_for_in_destructuring_pattern(&mut self, initializer: NodeIndex) {
        let arena = self.ctx.arena;
        let Some(init_node) = arena.get(initializer) else {
            return;
        };
        let Some(var_data) = arena.get_variable(init_node) else {
            return;
        };
        // Check the first (and typically only) declaration
        if let Some(&first_decl_idx) = var_data.declarations.nodes.first()
            && let Some(decl_node) = arena.get(first_decl_idx)
            && let Some(var_decl) = arena.get_variable_declaration(decl_node)
            && let Some(name_node) = arena.get(var_decl.name)
            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            self.error_at_node(
                var_decl.name,
                "The left-hand side of a 'for...in' statement cannot be a destructuring pattern.",
                crate::diagnostics::diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
            );
        }
    }

    /// TS2491: The left-hand side of a 'for...in' statement cannot be a destructuring pattern.
    /// Checks expression form: `for ([a, b] in obj)` or `for ({a, b} in obj)`
    pub(crate) fn check_for_in_expression_destructuring(&mut self, initializer: NodeIndex) {
        let arena = self.ctx.arena;
        if let Some(init_node) = arena.get(initializer)
            && (init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        {
            self.error_at_node(
                initializer,
                "The left-hand side of a 'for...in' statement cannot be a destructuring pattern.",
                crate::diagnostics::diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
            );
        }
    }

    pub(crate) fn begin_for_of_self_reference_tracking(
        &mut self,
        decl_list_idx: NodeIndex,
    ) -> usize {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return 0;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return 0;
        };

        let mut seen = FxHashSet::default();
        let mut tracked = 0;
        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if var_decl.type_annotation.is_some() {
                continue;
            }

            let sym_id = self
                .ctx
                .binder
                .get_node_symbol(decl_idx)
                .or_else(|| self.ctx.binder.get_node_symbol(var_decl.name))
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, var_decl.name)
                });
            let Some(sym_id) = sym_id else {
                continue;
            };

            if seen.insert(sym_id) {
                self.push_symbol_dependency(sym_id, false);
                tracked += 1;
            }
        }

        if tracked > 0 {
            self.ctx.non_closure_circular_return_tracking_depth += 1;
        }

        tracked
    }

    pub(crate) fn end_for_of_self_reference_tracking(&mut self, tracked_symbol_count: usize) {
        if tracked_symbol_count == 0 {
            return;
        }

        for _ in 0..tracked_symbol_count {
            self.pop_symbol_dependency();
        }
        self.ctx.non_closure_circular_return_tracking_depth = self
            .ctx
            .non_closure_circular_return_tracking_depth
            .saturating_sub(1);
    }

    /// TS7022: Detect self-referencing for-of loop variables.
    ///
    /// When `for (var v of v)` is written with `noImplicitAny`, the iterable
    /// expression `v` references the loop variable before it has a type,
    /// creating a circular dependency.  The element type resolves to `any`,
    /// and TS7022 should be emitted on the variable name.
    ///
    /// This also handles indirect circularity where the iterable expression
    /// contains a reference to the declared variable (e.g., via class methods
    /// that return `v`).
    pub(crate) fn check_for_of_self_reference_circularity(
        &mut self,
        decl_list_idx: NodeIndex,
        expression_idx: NodeIndex,
    ) {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return;
        };

        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };

            // Only applies when there's no type annotation
            if var_decl.type_annotation.is_some() {
                continue;
            }

            // Get the symbol for this declaration
            let sym_id = self
                .ctx
                .binder
                .get_node_symbol(decl_idx)
                .or_else(|| self.ctx.binder.get_node_symbol(var_decl.name))
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, var_decl.name)
                });
            let Some(sym_id) = sym_id else {
                continue;
            };

            // Get the variable name for the diagnostic
            let var_name = self.get_identifier_text_from_idx(var_decl.name);
            let mut circular_return_sites = self.take_pending_circular_return_sites(sym_id);
            for site_idx in
                self.collect_for_of_protocol_circular_return_sites(expression_idx, sym_id)
            {
                if !circular_return_sites.contains(&site_idx) {
                    circular_return_sites.push(site_idx);
                }
            }
            let has_direct_reference = self.expression_references_symbol(expression_idx, sym_id);
            if circular_return_sites.is_empty() && !has_direct_reference {
                continue;
            }

            // TS7022/TS7023 are implicit-any diagnostics, gated on noImplicitAny:
            // tsc's reportCircularityError only reports the "referenced directly or
            // indirectly in its own initializer" error `if (noImplicitAny && ...)`
            // (checker.ts reportCircularityError:12892). With noImplicitAny off the
            // circular variable is silently `any`.
            if let Some(name) = var_name.filter(|_| self.ctx.no_implicit_any()) {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    var_decl.name,
                    diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                    &[&name],
                );
                for site_idx in circular_return_sites {
                    self.emit_circular_return_site_diagnostic(
                        site_idx,
                        Some(name.as_str()),
                        var_decl.name,
                        expression_idx,
                    );
                }
            }
        }
    }

    fn collect_for_of_protocol_circular_return_sites(
        &mut self,
        expr_idx: NodeIndex,
        target_sym: SymbolId,
    ) -> Vec<NodeIndex> {
        let mut sites = Vec::new();
        let mut visited_symbols = FxHashSet::default();
        let mut visited_holders = FxHashSet::default();
        let mut collector = ForOfProtocolCollector {
            sites: &mut sites,
            visited_symbols: &mut visited_symbols,
            visited_holders: &mut visited_holders,
        };
        self.collect_for_of_protocol_sites_from_expression(
            expr_idx,
            target_sym,
            ForOfProtocolRole::Iterable,
            None,
            false,
            &mut collector,
        );
        sites
    }

    fn collect_for_of_protocol_sites_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        target_sym: SymbolId,
        role: ForOfProtocolRole,
        owner_idx: Option<NodeIndex>,
        allow_function_returns: bool,
        collector: &mut ForOfProtocolCollector<'_>,
    ) {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if node.kind == SyntaxKind::ThisKeyword as u16
            && role == ForOfProtocolRole::Iterator
            && let Some(owner_idx) = owner_idx
        {
            self.inspect_for_of_protocol_holder(owner_idx, target_sym, role, collector);
            return;
        }

        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self
                .resolve_for_of_header_expression_symbol(expr_idx)
                .or_else(|| self.resolve_identifier_symbol_without_tracking(expr_idx));
            if let Some(sym_id) = sym_id {
                self.collect_for_of_protocol_sites_from_symbol(
                    sym_id,
                    target_sym,
                    role,
                    allow_function_returns,
                    collector,
                );
                return;
            }
        }

        if matches!(
            node.kind,
            syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        ) {
            self.inspect_for_of_protocol_holder(expr_idx, target_sym, role, collector);
            return;
        }

        if matches!(
            node.kind,
            syntax_kind_ext::CALL_EXPRESSION | syntax_kind_ext::NEW_EXPRESSION
        ) && let Some(call) = self.ctx.arena.get_call_expr(node)
        {
            self.collect_for_of_protocol_sites_from_expression(
                call.expression,
                target_sym,
                role,
                owner_idx,
                node.kind == syntax_kind_ext::CALL_EXPRESSION,
                collector,
            );
            return;
        }

        for child_idx in self.ctx.arena.get_children(expr_idx) {
            self.collect_for_of_protocol_sites_from_expression(
                child_idx, target_sym, role, owner_idx, false, collector,
            );
        }
    }

    fn collect_for_of_protocol_sites_from_symbol(
        &mut self,
        sym_id: SymbolId,
        target_sym: SymbolId,
        role: ForOfProtocolRole,
        allow_function_returns: bool,
        collector: &mut ForOfProtocolCollector<'_>,
    ) {
        if !collector.visited_symbols.insert((sym_id, role.tag())) {
            return;
        }

        let Some(declarations) = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|symbol| symbol.declarations.clone())
        else {
            return;
        };

        for decl_idx in declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            if matches!(
                decl_node.kind,
                syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            ) {
                self.inspect_for_of_protocol_holder(decl_idx, target_sym, role, collector);
                continue;
            }

            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                && var_decl.initializer.is_some()
            {
                self.collect_for_of_protocol_sites_from_expression(
                    var_decl.initializer,
                    target_sym,
                    role,
                    None,
                    false,
                    collector,
                );
                continue;
            }

            if allow_function_returns
                && let Some(func) = self.ctx.arena.get_function(decl_node)
                && func.body.is_some()
            {
                self.inspect_function_like_protocol_returns(
                    func.body,
                    decl_idx,
                    None,
                    Some(role),
                    target_sym,
                    collector,
                );
            }
        }
    }

    fn inspect_for_of_protocol_holder(
        &mut self,
        holder_idx: NodeIndex,
        target_sym: SymbolId,
        role: ForOfProtocolRole,
        collector: &mut ForOfProtocolCollector<'_>,
    ) {
        if !collector.visited_holders.insert((holder_idx, role.tag())) {
            return;
        }

        let Some(holder_node) = self.ctx.arena.get(holder_idx) else {
            return;
        };

        if let Some(class) = self.ctx.arena.get_class(holder_node) {
            for &member_idx in &class.members.nodes {
                self.inspect_for_of_protocol_member(
                    member_idx, holder_idx, target_sym, role, collector,
                );
            }
            return;
        }

        if holder_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && let Some(object_literal) = self.ctx.arena.get_literal_expr(holder_node)
        {
            for &member_idx in &object_literal.elements.nodes {
                self.inspect_for_of_protocol_member(
                    member_idx, holder_idx, target_sym, role, collector,
                );
            }
        }
    }

    fn inspect_for_of_protocol_member(
        &mut self,
        member_idx: NodeIndex,
        owner_idx: NodeIndex,
        target_sym: SymbolId,
        role: ForOfProtocolRole,
        collector: &mut ForOfProtocolCollector<'_>,
    ) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        match member_node.kind {
            syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                    return;
                };
                let Some(name) = self.get_property_name_resolved(method.name) else {
                    return;
                };
                if !self.member_matches_for_of_protocol_role(&name, role) {
                    return;
                }
                self.inspect_function_like_protocol_returns(
                    method.body,
                    method.body,
                    Some(owner_idx),
                    self.next_protocol_role(name.as_str(), role),
                    target_sym,
                    collector,
                );
            }
            syntax_kind_ext::GET_ACCESSOR => {
                let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                    return;
                };
                let Some(name) = self.get_property_name_resolved(accessor.name) else {
                    return;
                };
                if !self.member_matches_for_of_protocol_role(&name, role) {
                    return;
                }
                self.inspect_function_like_protocol_returns(
                    accessor.body,
                    accessor.body,
                    Some(owner_idx),
                    self.next_protocol_role(name.as_str(), role),
                    target_sym,
                    collector,
                );
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    return;
                };
                let Some(name) = self.get_property_name_resolved(prop.name) else {
                    return;
                };
                if !self.member_matches_for_of_protocol_role(&name, role) {
                    return;
                }
                self.inspect_function_like_protocol_initializer(
                    prop.initializer,
                    owner_idx,
                    name.as_str(),
                    role,
                    target_sym,
                    collector,
                );
            }
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let Some(prop) = self.ctx.arena.get_property_assignment(member_node) else {
                    return;
                };
                let Some(name) = self.get_property_name_resolved(prop.name) else {
                    return;
                };
                if !self.member_matches_for_of_protocol_role(&name, role) {
                    return;
                }
                self.inspect_function_like_protocol_initializer(
                    prop.initializer,
                    owner_idx,
                    name.as_str(),
                    role,
                    target_sym,
                    collector,
                );
            }
            _ => {}
        }
    }

    fn inspect_function_like_protocol_initializer(
        &mut self,
        initializer_idx: NodeIndex,
        owner_idx: NodeIndex,
        member_name: &str,
        role: ForOfProtocolRole,
        target_sym: SymbolId,
        collector: &mut ForOfProtocolCollector<'_>,
    ) {
        let initializer_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer_idx);
        let Some(init_node) = self.ctx.arena.get(initializer_idx) else {
            return;
        };
        if let Some(func) = self.ctx.arena.get_function(init_node)
            && func.body.is_some()
        {
            self.inspect_function_like_protocol_returns(
                func.body,
                initializer_idx,
                Some(owner_idx),
                self.next_protocol_role(member_name, role),
                target_sym,
                collector,
            );
        }
    }

    fn inspect_function_like_protocol_returns(
        &mut self,
        body_idx: NodeIndex,
        diagnostic_site_idx: NodeIndex,
        owner_idx: Option<NodeIndex>,
        next_role: Option<ForOfProtocolRole>,
        target_sym: SymbolId,
        collector: &mut ForOfProtocolCollector<'_>,
    ) {
        if body_idx.is_none() {
            return;
        }

        let mut return_exprs = Vec::new();
        self.collect_return_expressions_in_function_body(body_idx, &mut return_exprs);

        let mut has_circular_return = false;
        for expr_idx in return_exprs {
            if self.initializer_has_non_deferred_self_reference(expr_idx, target_sym) {
                has_circular_return = true;
            }
            if let Some(next_role) = next_role {
                self.collect_for_of_protocol_sites_from_expression(
                    expr_idx, target_sym, next_role, owner_idx, false, collector,
                );
            }
        }

        if has_circular_return && !collector.sites.contains(&diagnostic_site_idx) {
            collector.sites.push(diagnostic_site_idx);
        }
    }

    pub(crate) fn collect_return_expressions_in_function_body(
        &self,
        body_idx: NodeIndex,
        return_exprs: &mut Vec<NodeIndex>,
    ) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return_exprs.push(body_idx);
            return;
        }

        if let Some(block) = self.ctx.arena.get_block(body_node) {
            for &stmt_idx in &block.statements.nodes {
                self.collect_return_expressions_in_statement(stmt_idx, return_exprs);
            }
        }
    }

    fn collect_return_expressions_in_statement(
        &self,
        stmt_idx: NodeIndex,
        return_exprs: &mut Vec<NodeIndex>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    return_exprs.push(ret.expression);
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_return_expressions_in_statement(stmt, return_exprs);
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_return_expressions_in_statement(
                        if_data.then_statement,
                        return_exprs,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_return_expressions_in_statement(
                            if_data.else_statement,
                            return_exprs,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt in &clause.statements.nodes {
                                self.collect_return_expressions_in_statement(stmt, return_exprs);
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_return_expressions_in_statement(try_data.try_block, return_exprs);
                    if try_data.catch_clause.is_some() {
                        self.collect_return_expressions_in_statement(
                            try_data.catch_clause,
                            return_exprs,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_return_expressions_in_statement(
                            try_data.finally_block,
                            return_exprs,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_return_expressions_in_statement(catch_data.block, return_exprs);
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_return_expressions_in_statement(loop_data.statement, return_exprs);
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_return_expressions_in_statement(loop_data.statement, return_exprs);
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_return_expressions_in_statement(labeled.statement, return_exprs);
                }
            }
            _ => {}
        }
    }

    fn member_matches_for_of_protocol_role(
        &self,
        member_name: &str,
        role: ForOfProtocolRole,
    ) -> bool {
        match role {
            ForOfProtocolRole::Iterable => {
                matches!(member_name, "[Symbol.iterator]" | "[Symbol.asyncIterator]")
            }
            ForOfProtocolRole::Iterator => member_name == "next",
        }
    }

    fn next_protocol_role(
        &self,
        member_name: &str,
        role: ForOfProtocolRole,
    ) -> Option<ForOfProtocolRole> {
        match role {
            ForOfProtocolRole::Iterable
                if matches!(member_name, "[Symbol.iterator]" | "[Symbol.asyncIterator]") =>
            {
                Some(ForOfProtocolRole::Iterator)
            }
            _ => None,
        }
    }

    /// Check if an expression AST subtree contains a reference to the given symbol.
    fn expression_references_symbol(
        &self,
        node_idx: NodeIndex,
        target_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // Check if this node is an identifier referencing the target symbol
        if node.kind == SyntaxKind::Identifier as u16 {
            let ref_sym = self
                .resolve_for_of_header_expression_symbol(node_idx)
                .or_else(|| self.resolve_identifier_symbol_without_tracking(node_idx));
            if ref_sym == Some(target_sym) {
                return true;
            }
        }

        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return false;
        }

        // The member name of a property access is not a reference to any value
        // binding, so `o.v` must not count as a reference to a variable named
        // `v` that happens to be in scope (`for (const v in o.v) {}` is clean in
        // tsc). Walk only the object side. An *element* access is different —
        // `o[v]` really does read `v` — so only `PropertyAccessExpression` is
        // narrowed here, and its `name_or_argument` is the sole skipped child.
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            return self.expression_references_symbol(access.expression, target_sym);
        }

        // A *written* property name in an object literal is a name, not a value
        // binding either — `pick({ v: 1 })` reads nothing called `v`, so only
        // the initializer side is walked. Two neighbours deliberately keep
        // their default recursion because they really do read the binding: a
        // computed name (`{ [v]: 1 }`) evaluates `v`, and a shorthand
        // (`{ v }`) is a `ShorthandPropertyAssignment` whose name *is* the
        // reference. `tsc` reports the circularity for both and not for the
        // written name.
        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(assignment) = self.ctx.arena.get_property_assignment(node)
            && self
                .ctx
                .arena
                .get(assignment.name)
                .is_some_and(|name| name.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        {
            return self.expression_references_symbol(assignment.initializer, target_sym);
        }

        // Recurse into children
        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.expression_references_symbol(child_idx, target_sym) {
                return true;
            }
        }

        false
    }
}
