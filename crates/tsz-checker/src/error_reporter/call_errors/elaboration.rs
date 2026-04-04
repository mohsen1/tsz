//! Call argument elaboration logic (object literal, array literal, function return).

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter::call_errors) fn contextual_keyof_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = arg_idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.ctx.arena.get_call_expr(node)
                && let Some(args) = &call.arguments
            {
                for &candidate_arg in &args.nodes {
                    if candidate_arg == arg_idx {
                        continue;
                    }
                    let candidate_type = self.get_type_of_node(candidate_arg);
                    if candidate_type == TypeId::ERROR || candidate_type == TypeId::ANY {
                        continue;
                    }

                    let candidate_keyof =
                        self.evaluate_type_for_assignability(self.ctx.types.keyof(candidate_type));
                    if candidate_keyof == TypeId::ERROR {
                        continue;
                    }

                    let same_key_space = (self.is_assignable_to(param_type, candidate_keyof)
                        && self.is_assignable_to(candidate_keyof, param_type))
                        || self.format_type_for_assignability_message(param_type)
                            == self.format_type_for_assignability_message(candidate_keyof);
                    if same_key_space {
                        let base = self.format_type_for_assignability_message(candidate_type);
                        return Some(format!("keyof {base}"));
                    }
                }
                break;
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    pub(in crate::error_reporter::call_errors) fn contextual_constraint_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let evaluated_param = self.evaluate_type_for_assignability(param_type);
        let mut current = arg_idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.ctx.arena.get_call_expr(node)
                && let Some(args) = &call.arguments
            {
                let arg_pos = args
                    .nodes
                    .iter()
                    .position(|&candidate| candidate == arg_idx)?;
                let callee_type = self.get_type_of_node(call.expression);
                let arg_count = args.nodes.len();

                let mut display = None;
                let mut ambiguous = false;

                if let Some(shape) =
                    tsz_solver::type_queries::get_function_shape(self.ctx.types, callee_type)
                {
                    let sig = tsz_solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate,
                        is_method: shape.is_method,
                    };
                    if self.call_signature_accepts_arg_count(&sig, arg_count) {
                        self.collect_constraint_parameter_display_candidate(
                            &sig,
                            arg_pos,
                            evaluated_param,
                            &mut display,
                            &mut ambiguous,
                        );
                    }
                }

                if let Some(signatures) =
                    tsz_solver::type_queries::get_call_signatures(self.ctx.types, callee_type)
                {
                    for sig in signatures {
                        if !self.call_signature_accepts_arg_count(&sig, arg_count) {
                            continue;
                        }
                        self.collect_constraint_parameter_display_candidate(
                            &sig,
                            arg_pos,
                            evaluated_param,
                            &mut display,
                            &mut ambiguous,
                        );
                        if ambiguous {
                            break;
                        }
                    }
                }

                return (!ambiguous).then_some(display).flatten();
            }

            current = self.ctx.arena.get_extended(current)?.parent;
        }

        None
    }

    pub(in crate::error_reporter::call_errors) fn contextual_generic_mapped_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let evaluated_arg = self.evaluate_type_for_assignability(arg_type);
        let arg_shape = tsz_solver::type_queries::get_object_shape(self.ctx.types, evaluated_arg)?;
        if arg_shape.properties.is_empty()
            && arg_shape.string_index.is_none()
            && arg_shape.number_index.is_none()
        {
            return None;
        }

        let mut unknown_properties = Vec::with_capacity(arg_shape.properties.len());
        for prop in &arg_shape.properties {
            let mut unknown_prop = tsz_solver::PropertyInfo::new(prop.name, TypeId::UNKNOWN);
            unknown_prop.optional = prop.optional;
            unknown_prop.readonly = prop.readonly;
            unknown_properties.push(unknown_prop);
        }
        let unknown_object = if arg_shape.string_index.is_some() || arg_shape.number_index.is_some()
        {
            let unknown_shape = tsz_solver::ObjectShape {
                properties: unknown_properties,
                string_index: arg_shape.string_index.as_ref().map(|sig| {
                    tsz_solver::IndexSignature {
                        value_type: TypeId::UNKNOWN,
                        ..*sig
                    }
                }),
                number_index: arg_shape.number_index.as_ref().map(|sig| {
                    tsz_solver::IndexSignature {
                        value_type: TypeId::UNKNOWN,
                        ..*sig
                    }
                }),
                ..Default::default()
            };
            self.ctx.types.factory().object_with_index(unknown_shape)
        } else {
            self.ctx.types.factory().object(unknown_properties)
        };

        let evaluated_param = self.evaluate_type_for_assignability(param_type);
        let mut current = arg_idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.ctx.arena.get_call_expr(node)
                && let Some(args) = &call.arguments
            {
                let arg_pos = args
                    .nodes
                    .iter()
                    .position(|&candidate| candidate == arg_idx)?;
                let callee_type = self.get_type_of_node(call.expression);
                let arg_count = args.nodes.len();

                let mut display = None;
                let mut ambiguous = false;

                if let Some(shape) =
                    tsz_solver::type_queries::get_function_shape(self.ctx.types, callee_type)
                {
                    let sig = tsz_solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate,
                        is_method: shape.is_method,
                    };
                    if self.call_signature_accepts_arg_count(&sig, arg_count) {
                        self.collect_generic_mapped_parameter_display_candidate(
                            &sig,
                            arg_pos,
                            unknown_object,
                            evaluated_param,
                            &mut display,
                            &mut ambiguous,
                        );
                    }
                }

                if let Some(signatures) =
                    tsz_solver::type_queries::get_call_signatures(self.ctx.types, callee_type)
                {
                    for sig in signatures {
                        if !self.call_signature_accepts_arg_count(&sig, arg_count) {
                            continue;
                        }
                        self.collect_generic_mapped_parameter_display_candidate(
                            &sig,
                            arg_pos,
                            unknown_object,
                            evaluated_param,
                            &mut display,
                            &mut ambiguous,
                        );
                        if ambiguous {
                            break;
                        }
                    }
                }

                return (!ambiguous).then_some(display).flatten();
            }

            current = self.ctx.arena.get_extended(current)?.parent;
        }

        None
    }

    fn collect_generic_mapped_parameter_display_candidate(
        &mut self,
        sig: &tsz_solver::CallSignature,
        arg_pos: usize,
        unknown_object: TypeId,
        evaluated_param: TypeId,
        display: &mut Option<String>,
        ambiguous: &mut bool,
    ) {
        if *ambiguous || sig.type_params.is_empty() {
            return;
        }
        let Some(raw_param) = self.raw_param_for_argument_index(sig, arg_pos) else {
            return;
        };
        if query_common::type_application(self.ctx.types, raw_param.type_id).is_none() {
            return;
        }

        let mut substitution = query_common::TypeSubstitution::new();
        for tp in &sig.type_params {
            substitution.insert(tp.name, unknown_object);
        }
        if substitution.is_empty() {
            return;
        }

        let candidate =
            query_common::instantiate_type(self.ctx.types, raw_param.type_id, &substitution);
        let evaluated_candidate = self.evaluate_type_for_assignability(candidate);
        let matches_evaluated = evaluated_candidate == evaluated_param
            || (self.is_assignable_to(evaluated_candidate, evaluated_param)
                && self.is_assignable_to(evaluated_param, evaluated_candidate));
        if !matches_evaluated {
            return;
        }

        let candidate_display = self.format_type_diagnostic(candidate);
        if display
            .as_ref()
            .is_some_and(|existing| existing != &candidate_display)
        {
            *ambiguous = true;
            return;
        }
        *display = Some(candidate_display);
    }

    fn collect_constraint_parameter_display_candidate(
        &mut self,
        sig: &tsz_solver::CallSignature,
        arg_pos: usize,
        evaluated_param: TypeId,
        display: &mut Option<String>,
        ambiguous: &mut bool,
    ) {
        if *ambiguous {
            return;
        }

        let Some(raw_param) = self.raw_param_for_argument_index(sig, arg_pos) else {
            return;
        };
        let Some(type_param) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, raw_param.type_id)
        else {
            return;
        };
        let Some(raw_constraint) = type_param.constraint else {
            return;
        };

        let evaluated_constraint = self.evaluate_type_for_assignability(raw_constraint);
        let matches_evaluated = evaluated_constraint == evaluated_param
            || (self.is_assignable_to(evaluated_constraint, evaluated_param)
                && self.is_assignable_to(evaluated_param, evaluated_constraint));
        if !matches_evaluated {
            return;
        }

        let candidate = self.format_type_for_assignability_message(raw_constraint);
        if display
            .as_ref()
            .is_some_and(|existing| existing != &candidate)
        {
            *ambiguous = true;
            return;
        }
        *display = Some(candidate);
    }

    /// Try to elaborate a generic assignability mismatch when the source expression is
    /// a literal that can be decomposed into more precise element/property errors.
    pub(crate) fn try_elaborate_assignment_source_error(
        &mut self,
        source_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        if let Some(node) = self.ctx.arena.get(expr_idx)
            && node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && self.assignment_source_is_return_expression(source_idx)
            && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
        {
            let mut elaborated = false;

            for branch_idx in [cond.when_true, cond.when_false] {
                let branch_idx = self.ctx.arena.skip_parenthesized_and_assertions(branch_idx);
                let branch_type = self.get_type_of_node(branch_idx);
                if branch_type == TypeId::ERROR
                    || branch_type == TypeId::ANY
                    || target_type == TypeId::ERROR
                    || target_type == TypeId::ANY
                    || self.is_assignable_to(branch_type, target_type)
                {
                    continue;
                }

                if self.try_elaborate_assignment_source_error(branch_idx, target_type) {
                    elaborated = true;
                    continue;
                }

                self.error_type_not_assignable_at_with_anchor(branch_type, target_type, branch_idx);
                elaborated = true;
            }

            return elaborated;
        }

        self.try_elaborate_object_literal_arg_error(expr_idx, target_type)
    }

    /// Try to elaborate an argument type mismatch for object/array literal arguments.
    ///
    /// When an object literal argument has a property whose value type doesn't match
    /// the expected property type, tsc reports TS2322 on the specific property name
    /// rather than TS2345 on the whole argument. Similarly for array literals, tsc
    /// reports TS2322 on each element that doesn't match the expected element type.
    ///
    /// Returns `true` if elaboration produced at least one property-level error (TS2322),
    /// meaning the caller should NOT emit TS2345 on the whole argument.
    pub fn try_elaborate_object_literal_arg_error(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) => node,
            None => return false,
        };

        match arg_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.try_elaborate_object_literal_properties(arg_idx, param_type)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.try_elaborate_array_literal_elements(arg_idx, param_type)
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.try_elaborate_function_arg_return_error(arg_idx, param_type)
            }
            _ => false,
        }
    }

    fn try_elaborate_function_arg_return_error(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(arg_node) else {
            return false;
        };

        let Some(expected_return_type) = self.first_callable_return_type(param_type) else {
            return false;
        };

        // Skip elaboration when the expected return type contains unresolved
        // type parameters or inference placeholders. During generic call
        // inference, the expected callback return type may still reference
        // uninstantiated type parameters (e.g., `B` from `compose<A, B, C>`).
        // Checking the body expression type against such placeholders would
        // produce false TS2322 errors since concrete types like `T[]` are
        // not assignable to an unresolved type parameter `B`.
        if self.type_has_unresolved_inference_holes(expected_return_type) {
            return false;
        }

        let Some(body_node) = self.ctx.arena.get(func.body) else {
            return false;
        };

        match body_node.kind {
            // Expression-bodied arrow function: () => ({ ... })
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                self.try_elaborate_object_literal_arg_error(func.body, expected_return_type)
            }
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                // For expression-bodied arrows with simple literal/expression bodies,
                // check if the return expression type is assignable to the expected
                // return type. tsc reports TS2322 on the return expression when the
                // type violates the expected return type (e.g., returning a string
                // where Function is expected in a property assignment context).
                //
                // Skip void expected return types: void-returning callbacks accept any
                // return value, so elaborating would produce false positives.
                if expected_return_type == TypeId::VOID {
                    return false;
                }
                // Skip elaboration when the callback has explicit parameter type
                // annotations. tsc only elaborates return types for fully contextually-
                // typed callbacks (no explicit param annotations). When a developer
                // explicitly annotates parameter types, the error is reported at the
                // argument level (TS2345) rather than drilling into the return expression.
                let has_explicit_param_annotations =
                    func.parameters.nodes.iter().any(|param_idx| {
                        self.ctx
                            .arena
                            .get(*param_idx)
                            .and_then(|n| self.ctx.arena.get_parameter(n))
                            .is_some_and(|p| p.type_annotation.is_some())
                    });
                if has_explicit_param_annotations {
                    return false;
                }
                let body_type = self.get_type_of_node(func.body);
                if body_type == TypeId::ERROR
                    || body_type == TypeId::ANY
                    || expected_return_type == TypeId::ERROR
                    || expected_return_type == TypeId::ANY
                    || self.is_assignable_to(body_type, expected_return_type)
                {
                    return false;
                }
                // Skip elaboration when the body type is itself callable (a function type).
                // When the return type is a function but the expected type is not (or vice
                // versa), tsc reports TS2345 on the whole callback rather than TS2322 on
                // the body expression.
                if self.first_callable_return_type(body_type).is_some()
                    && self
                        .first_callable_return_type(expected_return_type)
                        .is_none()
                {
                    return false;
                }
                // Widen literal types for display (e.g. "abc" → string) to match tsc behavior
                let display_type = self.widen_type_for_display(body_type);
                self.error_type_not_assignable_at(display_type, expected_return_type, func.body);
                true
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                // Conditionals need branch-level elaboration. Let the caller
                // handle these at the argument/assignment level.
                false
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let Some(paren) = self.ctx.arena.get_parenthesized(body_node) else {
                    return false;
                };
                self.try_elaborate_object_literal_arg_error(paren.expression, expected_return_type)
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.try_elaborate_function_block_returns(func.body, expected_return_type)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                // Expression-bodied arrow: () => new Animal()
                // When the new-expression type isn't assignable to the expected
                // return type (e.g. Animal missing 'woof' required by Dog),
                // emit the assignability error at the expression position.
                // This matches tsc which emits TS2741 at `new Animal()` instead
                // of TS2345 on the whole callback.
                let body_type = self.get_type_of_node(func.body);
                if body_type == TypeId::ERROR
                    || body_type == TypeId::ANY
                    || expected_return_type == TypeId::ERROR
                    || expected_return_type == TypeId::ANY
                    || self.is_assignable_to(body_type, expected_return_type)
                {
                    return false;
                }
                self.error_type_not_assignable_at(body_type, expected_return_type, func.body);
                true
            }
            _ => false,
        }
    }

    fn try_elaborate_function_block_returns(
        &mut self,
        block_idx: NodeIndex,
        expected_return_type: TypeId,
    ) -> bool {
        let Some(block_node) = self.ctx.arena.get(block_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(block_node) else {
            return false;
        };

        let mut elaborated = false;
        for &stmt_idx in &block.statements.nodes {
            elaborated |=
                self.try_elaborate_return_statements_in_stmt(stmt_idx, expected_return_type);
        }
        elaborated
    }

    fn try_elaborate_return_statements_in_stmt(
        &mut self,
        stmt_idx: NodeIndex,
        expected_return_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.ctx.arena.get_return_statement(node) else {
                    return false;
                };
                if ret.expression.is_none() {
                    return false;
                }
                if expected_return_type == TypeId::VOID {
                    return false;
                }

                let return_type = self.get_type_of_node(ret.expression);
                !self.check_assignable_or_report_at_without_source_elaboration(
                    return_type,
                    expected_return_type,
                    ret.expression,
                    ret.expression,
                )
            }
            syntax_kind_ext::BLOCK => {
                self.try_elaborate_function_block_returns(stmt_idx, expected_return_type)
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_stmt) = self.ctx.arena.get_if_statement(node) else {
                    return false;
                };
                let mut elaborated = self.try_elaborate_return_statements_in_stmt(
                    if_stmt.then_statement,
                    expected_return_type,
                );
                if if_stmt.else_statement.is_some() {
                    elaborated |= self.try_elaborate_return_statements_in_stmt(
                        if_stmt.else_statement,
                        expected_return_type,
                    );
                }
                elaborated
            }
            _ => false,
        }
    }

    fn first_callable_return_type(&mut self, ty: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::diagnostics::{
            callable_shape_for_type, function_shape, type_application,
        };

        if let (Some(non_nullish), Some(_nullish_cause)) = self.split_nullish_type(ty) {
            return self.first_callable_return_type(non_nullish);
        }

        if let Some(shape) = function_shape(self.ctx.types, ty) {
            return Some(shape.return_type);
        }

        if let Some(shape) = callable_shape_for_type(self.ctx.types, ty) {
            return shape.call_signatures.first().map(|sig| sig.return_type);
        }

        if let Some(app) = type_application(self.ctx.types, ty) {
            return self.first_callable_return_type(app.base);
        }

        None
    }

    /// Elaborate object literal property type mismatches with TS2322.
    fn try_elaborate_object_literal_properties(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // When exactOptionalPropertyTypes is enabled and the failure is due to
        // exact optional property mismatch, don't elaborate per-property errors.
        // The caller will emit a top-level TS2375 instead.
        let source_type = self.get_type_of_node(arg_idx);
        if self.has_exact_optional_property_mismatch(source_type, param_type) {
            return false;
        }

        // Normalize optional/nullish wrappers (e.g., `{...} | undefined`).
        let effective_param_type = if let (Some(non_nullish), Some(_nullish_cause)) =
            self.split_nullish_type(param_type)
        {
            non_nullish
        } else {
            param_type
        };

        // Don't elaborate `never` targets — tsc emits a single TS2345 instead.
        if effective_param_type == TypeId::NEVER {
            return false;
        }

        // Don't elaborate into object literal properties when the target is a
        // primitive type (string, number, boolean, etc.).  Primitives can expose
        // properties via index signatures or prototypes, which causes misleading
        // per-property TS2322 errors instead of the correct top-level mismatch
        // (e.g., "Type '{ 0: number }' is not assignable to type 'string'").
        if tsz_solver::is_primitive_type(self.ctx.types, effective_param_type) {
            return false;
        }

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) => node,
            None => return false,
        };

        let obj = match self.ctx.arena.get_literal_expr(arg_node) {
            Some(obj) => obj.clone(),
            None => return false,
        };

        // When the source object literal is missing required properties from the
        // target, don't elaborate into per-property TS2322 errors. tsc reports
        // TS2345 at the argument level with "Property 'X' is missing" elaboration
        // in these cases, rather than TS2322 on individual matching properties.
        // Without this guard, widened property types (e.g., a string literal `'name'`
        // widened to `string`) can produce false TS2322 errors like
        // `Type '"name"' is not assignable to type '"name"'`.
        if self.target_has_missing_required_properties_from_source(&obj, effective_param_type) {
            return false;
        }

        let diagnostics_before_epc = self.ctx.diagnostics.len();
        self.check_object_literal_excess_properties(source_type, effective_param_type, arg_idx);
        let had_excess_property = self.ctx.diagnostics[diagnostics_before_epc..]
            .iter()
            .any(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                        | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                )
            });
        if had_excess_property {
            return true;
        }

        let mut elaborated = false;

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Only elaborate regular property assignments and shorthand properties
            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_property_assignment(elem_node) {
                        Some(prop) => (prop.name, prop.initializer),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_shorthand_property(elem_node) {
                        Some(prop) => (prop.name, prop.name),
                        None => continue,
                    }
                }
                _ => continue,
            };

            // Get the property name string.
            // For computed property names (e.g., `[SYM]`), fall back to type-level
            // resolution so unique symbols and const-evaluated keys are resolved.
            let is_computed_property = self
                .ctx
                .arena
                .get(prop_name_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            let prop_name = match self.object_literal_property_name_text(prop_name_idx) {
                Some(name) => name,
                None if is_computed_property => {
                    match self.get_property_name_resolved(prop_name_idx) {
                        Some(name) => name,
                        None => continue,
                    }
                }
                None => continue,
            };

            let Some((target_prop_type, target_prop_type_for_diagnostic)) = self
                .object_literal_target_property_type(
                    effective_param_type,
                    prop_name_idx,
                    &prop_name,
                )
            else {
                continue;
            };

            // Get the type of the property value in the object literal.
            // Use the cached (contextually-typed) type for the assignability check.
            // This preserves literal types that were narrowed by contextual typing
            // (e.g., `value: "hello"` in a mapped type context stays as `"hello"`,
            // not widened to `string`).
            //
            // When the cached type is widened (e.g., `string` for a `'name'` literal)
            // and fails assignability, fall back to the literal type. This avoids
            // spurious TS2322 errors like `Type '"name"' is not assignable to type
            // '"name"'` where the source was widened during arg collection but the
            // target preserves the literal from inference.
            let is_function_value = self.ctx.arena.get(prop_value_idx).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                )
            });
            let cached_prop_type = self.get_type_of_node(prop_value_idx);
            let source_prop_type = if !is_function_value
                && cached_prop_type != TypeId::ERROR
                && cached_prop_type != TypeId::ANY
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
                && !self.is_assignable_to(cached_prop_type, target_prop_type)
            {
                // If the cached type fails, try the literal type from the initializer.
                // When a generic call widens literals during inference (e.g., `'name'` → string),
                // the literal type may actually be assignable to the inferred target.
                if let Some(literal_type) = self.literal_type_from_initializer(prop_value_idx) {
                    if self.is_assignable_to(literal_type, target_prop_type) {
                        literal_type
                    } else {
                        cached_prop_type
                    }
                } else {
                    cached_prop_type
                }
            } else {
                cached_prop_type
            };

            // For function values, emit TS2322 at the property level when there's a type mismatch.
            // This applies to both optional and required function properties.
            if is_function_value
                && source_prop_type != TypeId::ERROR
                && source_prop_type != TypeId::ANY
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
                && !self.is_assignable_to(source_prop_type, target_prop_type)
            {
                let source_prop_type_for_diagnostic =
                    self.widen_function_like_call_source(source_prop_type);
                // Use the diagnostic target type if available (for optional properties),
                // otherwise use the effective target type
                let target_for_diag = if target_prop_type != target_prop_type_for_diagnostic {
                    target_prop_type_for_diagnostic
                } else {
                    target_prop_type
                };
                self.error_type_not_assignable_at_with_anchor(
                    source_prop_type_for_diagnostic,
                    target_for_diag,
                    prop_name_idx,
                );
                elaborated = true;
                continue;
            }

            // Only try to elaborate sub-expression errors when the property value
            // is NOT assignable to the target. Without this guard, elaboration can
            // produce false-positive TS2322 errors on nested elements (e.g., array
            // literal elements) even when the overall property type is compatible.
            if source_prop_type != TypeId::ERROR
                && source_prop_type != TypeId::ANY
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
                && !self.is_assignable_to(source_prop_type, target_prop_type)
                && self.ctx.arena.get(prop_value_idx).is_some_and(|node| {
                    matches!(
                        node.kind,
                        syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            | syntax_kind_ext::ARROW_FUNCTION
                            | syntax_kind_ext::FUNCTION_EXPRESSION
                            | syntax_kind_ext::CONDITIONAL_EXPRESSION
                    )
                })
                && self.try_elaborate_assignment_source_error(prop_value_idx, target_prop_type)
            {
                elaborated = true;
                continue;
            }

            // Skip if types are unresolved
            if source_prop_type == TypeId::ERROR
                || source_prop_type == TypeId::ANY
                || target_prop_type == TypeId::ERROR
                || target_prop_type == TypeId::ANY
            {
                continue;
            }

            // Check if the property value type is assignable to the target property type
            if !self.is_assignable_to(source_prop_type, target_prop_type) {
                if self.try_elaborate_assignment_source_error(prop_value_idx, target_prop_type) {
                    elaborated = true;
                    continue;
                }

                // TS2820: before emitting generic TS2322, check if the property
                // value is a string literal that is a near-miss of a target union
                // member. Use the AST literal type (not the widened source_prop_type)
                // so that `"hdpvd"` is compared against `"hddvd" | "bluray"`.
                if let Some(literal_source_type) =
                    self.literal_type_from_initializer(prop_value_idx)
                {
                    let evaluated_target =
                        self.evaluate_type_with_env(target_prop_type_for_diagnostic);
                    if let Some(suggestion) = self
                        .find_string_literal_spelling_suggestion(
                            literal_source_type,
                            target_prop_type,
                        )
                        .or_else(|| {
                            self.find_string_literal_spelling_suggestion(
                                literal_source_type,
                                evaluated_target,
                            )
                        })
                    {
                        let src_str = self.format_type_diagnostic(literal_source_type);
                        let tgt_str = self.format_type_diagnostic(target_prop_type_for_diagnostic);
                        let expanded_tgt_str = self.format_type_diagnostic(evaluated_target);
                        let display_target = if expanded_tgt_str != tgt_str {
                            &expanded_tgt_str
                        } else {
                            &tgt_str
                        };
                        let msg = format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                            &[&src_str, display_target, &suggestion],
                        );
                        let anchor_idx = self.resolve_diagnostic_anchor_node(
                            prop_name_idx,
                            DiagnosticAnchorKind::Exact,
                        );
                        if let Some(anchor) =
                            self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
                        {
                            self.ctx
                                .push_diagnostic(crate::diagnostics::Diagnostic::error(
                                    self.ctx.file_name.clone(),
                                    anchor.start,
                                    anchor.length,
                                    msg,
                                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                                ));
                        }
                        elaborated = true;
                        continue;
                    }
                }

                // For computed property names, emit TS2418 ("Type of computed
                // property's value is '{0}', which is not assignable to type
                // '{1}'.") instead of the generic TS2322.  This matches tsc's
                // behavior in `elaborateElementwise`.  tsc does not widen
                // literal types in the TS2418 message.
                if is_computed_property {
                    // For TS2418, use the literal type from the initializer
                    // expression when available (tsc shows "str" not string).
                    let computed_source = self
                        .literal_type_from_initializer(prop_value_idx)
                        .unwrap_or(source_prop_type);
                    let src_str = self.format_type_for_assignability_message(computed_source);
                    let tgt_str =
                        self.format_type_for_assignability_message(target_prop_type_for_diagnostic);
                    let msg = format_message(
                        diagnostic_messages::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str],
                    );
                    self.error_at_node(
                        prop_name_idx,
                        &msg,
                        diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                } else {
                    let source_prop_type_for_diagnostic =
                        if self.is_fresh_literal_expression(prop_value_idx) {
                            self.widen_literal_type(source_prop_type)
                        } else {
                            source_prop_type
                        };
                    let source_prop_type_for_diagnostic =
                        self.widen_function_like_call_source(source_prop_type_for_diagnostic);
                    // TSC's elaborateElementwise reports TS2322 at the property
                    // name for property-value type mismatches, not TS2741/TS2739.
                    // Use full failure analysis for accurate message formatting
                    // (e.g., union best-match), but downgrade TS2741 to TS2322
                    // since this is a nested elaboration context.
                    self.error_type_not_assignable_at_with_anchor_elaboration(
                        source_prop_type_for_diagnostic,
                        target_prop_type_for_diagnostic,
                        prop_name_idx,
                    );
                }
                elaborated = true;
            }
        }

        // When the object literal has properties that all matched the target (elaborated
        // == false), but the only missing properties are Object.prototype methods
        // (valueOf, toString, etc.), suppress the error — those methods are implicitly
        // present from Object.prototype. However, only suppress when the source actually
        // HAS properties; an empty object literal `{}` has no properties to satisfy the
        // target, so the structural mismatch is real and should produce TS2322/TS2345.
        if !elaborated
            && !obj.elements.nodes.is_empty()
            && self.should_suppress_object_literal_call_mismatch(source_type, effective_param_type)
        {
            return true;
        }

        elaborated
    }

    /// Check whether the target type has required properties that are not present
    /// in the source object literal.
    ///
    /// When missing required properties are detected, tsc reports TS2345 at the
    /// whole argument level with "Property 'X' is missing" elaboration. Elaborating
    /// into per-property TS2322 errors in this case produces misleading diagnostics
    /// because widened literal types (e.g., `'name'` widened to `string`) can fail
    /// comparison against their inferred target literal types.
    fn target_has_missing_required_properties_from_source(
        &mut self,
        obj: &tsz_parser::parser::node::LiteralExprData,
        target_type: TypeId,
    ) -> bool {
        // Collect source property names from the object literal
        let mut source_prop_names = std::collections::HashSet::new();
        for &elem_idx in &obj.elements.nodes {
            if let Some(prop_name) = self.object_literal_property_name_from_elem(elem_idx) {
                source_prop_names.insert(prop_name);
            }
        }

        // Get target property names and check for missing required ones.
        // We use the solver's object shape to get the canonical set of target properties.
        let original_target_type = target_type;
        let target_type = self.resolve_type_for_property_access(target_type);
        let target_type = self.evaluate_type_with_env(target_type);
        let target_type = self.resolve_lazy_type(target_type);
        let target_type = self.evaluate_application_type(target_type);

        // Object.prototype methods that are implicitly present on all objects.
        // These should not count as "missing" for the purpose of suppressing
        // per-property elaboration, matching `should_suppress_object_literal_call_mismatch`.
        static OBJECT_PROTO_METHODS: &[&str] = &[
            "constructor",
            "toString",
            "toLocaleString",
            "valueOf",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ];

        // For type parameters with index signature constraints, don't consider properties
        // as "missing" - index signatures accept any property name.
        let has_index_signature = [original_target_type, target_type]
            .into_iter()
            .chain(crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                original_target_type,
            ))
            .chain(crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                target_type,
            ))
            .filter_map(|candidate| {
                tsz_solver::type_queries::get_object_shape(self.ctx.types, candidate)
            })
            .any(|shape| shape.string_index.is_some() || shape.number_index.is_some());

        if has_index_signature {
            return false;
        }

        if let Some(shape) = crate::query_boundaries::assignability::object_shape_for_type(
            self.ctx.types,
            target_type,
        ) {
            for prop in shape.properties.iter() {
                if prop.optional {
                    continue;
                }
                let name = self.ctx.types.resolve_atom(prop.name);
                if !source_prop_names.contains(name.as_str())
                    && !OBJECT_PROTO_METHODS.contains(&name.as_str())
                {
                    return true;
                }
            }
        }

        false
    }

    /// Extract a property name from an object literal element node.
    /// Falls back to type-level resolution for computed property names
    /// (e.g., unique symbols, const-evaluated keys).
    fn object_literal_property_name_from_elem(&mut self, elem_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let elem_node = self.ctx.arena.get(elem_idx)?;
        let name_idx = match elem_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.ctx.arena.get_property_assignment(elem_node)?.name
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                self.ctx.arena.get_shorthand_property(elem_node)?.name
            }
            _ => return None,
        };
        self.object_literal_property_name_text(name_idx)
            .or_else(|| self.get_property_name_resolved(name_idx))
    }

    /// Elaborate array literal element type mismatches with TS2322.
    fn try_elaborate_array_literal_elements(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // When the target type is `never`, don't elaborate into element-level TS2322 errors.
        if param_type == TypeId::NEVER {
            return false;
        }

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        let arr = match self.ctx.arena.get_literal_expr(arg_node) {
            Some(arr) => arr.clone(),
            None => return false,
        };

        let ctx_helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            param_type,
            self.ctx.compiler_options.no_implicit_any,
        );

        let mut elaborated = false;

        for (index, &elem_idx) in arr.elements.nodes.iter().enumerate() {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Skip spread elements
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                continue;
            }

            // Get the expected element type from the parameter array/tuple type
            let target_element_type = if let Some(t) = ctx_helper.get_tuple_element_type(index) {
                t
            } else if let Some(t) = ctx_helper.get_array_element_type() {
                t
            } else {
                continue;
            };

            let elem_type = self.elaboration_source_expression_type(elem_idx);

            // When the target element type is an index-signature-only type
            // (e.g., `NamedTransform { [name: string]: Transform3D }`),
            // don't drill into per-property errors for object literal elements.
            // Report at the element level instead:
            //   "Type '{ ry: null }' is not assignable to type 'NamedTransform'"
            // rather than the confusing inner error:
            //   "Type 'null' is not assignable to type 'Transform3D'"
            // This only applies to array element context — direct call argument
            // and variable assignment elaboration still drills into properties.
            let skip_deep_elaboration = elem_node.kind
                == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && !self
                    .target_has_named_property_for_any_source_prop(elem_idx, target_element_type);

            // For object/array literal elements, use contextually-typed type
            // to decide whether to elaborate (avoids false positives from widening).
            // Pass the target element type as contextual type so literal types
            // are preserved (e.g., `"bluray"` stays as `"bluray"` instead of
            // widening to `string` when checked against a discriminated union).
            if matches!(
                elem_node.kind,
                syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            ) {
                let contextual_request =
                    crate::context::TypingRequest::with_contextual_type(target_element_type);
                let contextual_elem_type =
                    self.get_type_of_node_with_request(elem_idx, &contextual_request);
                if contextual_elem_type != TypeId::ERROR
                    && contextual_elem_type != TypeId::ANY
                    && target_element_type != TypeId::ERROR
                    && target_element_type != TypeId::ANY
                    && self.is_assignable_to(contextual_elem_type, target_element_type)
                {
                    // Element is contextually assignable — no error needed.
                    continue;
                }
                if !skip_deep_elaboration
                    && self.try_elaborate_assignment_source_error(elem_idx, target_element_type)
                {
                    elaborated = true;
                    continue;
                }
                // Fall through to the non-object element check below.
            }

            // For function/conditional elements, try to elaborate without a guard.
            if matches!(
                elem_node.kind,
                syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::CONDITIONAL_EXPRESSION
            ) && self.try_elaborate_assignment_source_error(elem_idx, target_element_type)
            {
                elaborated = true;
                continue;
            }

            // Skip if types are unresolved
            if elem_type == TypeId::ERROR
                || elem_type == TypeId::ANY
                || target_element_type == TypeId::ERROR
                || target_element_type == TypeId::ANY
            {
                continue;
            }

            if !self.is_assignable_to(elem_type, target_element_type) {
                if !skip_deep_elaboration
                    && self.try_elaborate_assignment_source_error(elem_idx, target_element_type)
                {
                    elaborated = true;
                    continue;
                }

                // When the element is an object literal and property-level elaboration
                // found no issues (returned false above), the widened type (e.g.,
                // `{ kind: string }`) fails assignability but the literal types of all
                // properties actually match the target. This happens with discriminated
                // unions where the literal property types are preserved contextually but
                // the overall element type gets widened. Suppress the false TS2322.
                if elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && self.all_object_literal_properties_assignable_with_literals(
                        elem_idx,
                        target_element_type,
                    )
                {
                    continue;
                }

                tracing::debug!(
                    "try_elaborate_array_literal_elements: elem_type = {:?}, target_element_type = {:?}, file = {}",
                    elem_type,
                    target_element_type,
                    self.ctx.file_name
                );
                self.error_type_not_assignable_at_with_anchor(
                    elem_type,
                    target_element_type,
                    elem_idx,
                );
                elaborated = true;
            }
        }

        elaborated
    }

    /// Check if all properties of an object literal are assignable to the
    /// target type when using literal types from the initializers. This catches
    /// cases where the widened object type (e.g., `{ kind: string }`) fails
    /// assignability against a discriminated union, but the literal property
    /// values (e.g., `"bluray"`) actually match a union member.
    fn all_object_literal_properties_assignable_with_literals(
        &mut self,
        obj_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let obj_node = match self.ctx.arena.get(obj_idx) {
            Some(node) if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        let obj = match self.ctx.arena.get_literal_expr(obj_node) {
            Some(obj) => obj.clone(),
            None => return false,
        };

        if obj.elements.nodes.is_empty() {
            return false;
        }

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_property_assignment(elem_node) {
                        Some(prop) => (prop.name, prop.initializer),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_shorthand_property(elem_node) {
                        Some(prop) => (prop.name, prop.name),
                        None => continue,
                    }
                }
                _ => continue,
            };

            let Some(prop_name) = self.object_literal_property_name_text(prop_name_idx) else {
                continue;
            };

            let Some((target_prop_type, _)) =
                self.object_literal_target_property_type(target_type, prop_name_idx, &prop_name)
            else {
                // Target doesn't have this property — can't confirm assignability
                return false;
            };

            if target_prop_type == TypeId::ERROR || target_prop_type == TypeId::ANY {
                continue;
            }

            // Try literal type first, then cached type
            let source_prop_type =
                if let Some(literal_type) = self.literal_type_from_initializer(prop_value_idx) {
                    literal_type
                } else {
                    self.get_type_of_node(prop_value_idx)
                };

            if source_prop_type == TypeId::ERROR || source_prop_type == TypeId::ANY {
                continue;
            }

            if !self.is_assignable_to(source_prop_type, target_prop_type) {
                return false;
            }
        }

        true
    }

    /// Elaborate object literal property mismatches for variable declarations.
    pub fn try_elaborate_object_literal_properties_for_var_init(
        &mut self,
        init_idx: NodeIndex,
        declared_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return false;
        };

        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        self.try_elaborate_object_literal_properties(init_idx, declared_type)
    }

    /// Elaborate array literal element mismatches for variable declarations.
    pub fn try_elaborate_initializer_elements(
        &mut self,
        init_type: TypeId,
        declared_type: TypeId,
        init_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let init_node = match self.ctx.arena.get(init_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        // Only elaborate when the overall assignment fails.
        if self.is_assignable_to(init_type, declared_type) {
            return false;
        }

        // Arity mismatch — report at whole-assignment level, not per-element.
        if let Some(arr) = self.ctx.arena.get_literal_expr(init_node) {
            let source_count = arr.elements.nodes.len();
            if let Some(target_count) =
                tsz_solver::type_queries::get_fixed_tuple_length(self.ctx.types, declared_type)
                && source_count > target_count
            {
                return false;
            }
        }

        // Delegate to array literal element elaboration
        self.try_elaborate_array_literal_elements(init_idx, declared_type)
    }
}
