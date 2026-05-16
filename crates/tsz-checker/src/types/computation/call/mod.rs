//! Call expression type computation for `CheckerState`.
//!
//! Handles call expression type resolution including overload resolution,
//! argument type checking, type argument validation, and call result processing.
//! Identifier resolution is in `identifier.rs` and tagged
//! template expression handling is in `tagged_template.rs`.
//!
//! Split into submodules:
//! - `inner` — the main `get_type_of_call_expression_inner` implementation

mod abstract_constructor_args;
mod inner;
mod literal_key_preservation;
mod namespace_conflict;
mod nominal_lib_object_callbacks;
mod post_generic;
mod resolution_evidence;
mod tail_helpers;

use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call as call_checker;
use crate::state::CheckerState;
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{ParamInfo, TypeId, TypePredicate, TypePredicateTarget};

impl<'a> CheckerState<'a> {
    pub(crate) fn assertion_predicate_for_call(
        &mut self,
        call_idx: NodeIndex,
    ) -> Option<(TypePredicate, Vec<ParamInfo>)> {
        if self
            .ctx
            .call_type_predicates
            .is_invalid_assertion_call(call_idx.0)
        {
            return None;
        }

        if let Some((predicate, params)) = self
            .ctx
            .call_type_predicates
            .get(&call_idx.0)
            .filter(|(predicate, _)| predicate.asserts)
            .cloned()
        {
            return Some((predicate, params));
        }

        let call = self
            .ctx
            .arena
            .get(call_idx)
            .and_then(|node| self.ctx.arena.get_call_expr(node))?;
        let callee_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(call.expression);
        let callee_type = self
            .ctx
            .node_types
            .get(&callee_idx.0)
            .copied()
            .unwrap_or_else(|| self.get_type_of_node(callee_idx));
        let signature = call_checker::extract_predicate_signature(self.ctx.types, callee_type)?;
        signature
            .predicate
            .asserts
            .then_some((signature.predicate, signature.params))
    }

    pub(crate) fn store_call_type_predicate(
        &mut self,
        call_idx: NodeIndex,
        callee_idx: NodeIndex,
        predicate: (TypePredicate, Vec<ParamInfo>),
    ) {
        let assertion_target_is_valid =
            !predicate.0.asserts || self.validate_assertion_call_target(call_idx, callee_idx);
        if assertion_target_is_valid {
            self.ctx.call_type_predicates.insert(call_idx.0, predicate);
        }
    }

    pub(crate) fn assertion_call_asserted_expression(
        &self,
        call_idx: NodeIndex,
        predicate: TypePredicate,
        params: &[ParamInfo],
    ) -> Option<NodeIndex> {
        let call = self
            .ctx
            .arena
            .get(call_idx)
            .and_then(|node| self.ctx.arena.get_call_expr(node))?;
        let args = call.arguments.as_ref()?;
        match predicate.target {
            TypePredicateTarget::Identifier(_) => {
                let param_index = predicate.parameter_index.or_else(|| {
                    let TypePredicateTarget::Identifier(target_name) = predicate.target else {
                        return None;
                    };
                    params
                        .iter()
                        .position(|param| param.name == Some(target_name))
                })?;
                args.nodes.get(param_index).copied()
            }
            TypePredicateTarget::This => {
                let callee_node = self.ctx.arena.get(
                    self.ctx
                        .arena
                        .skip_parenthesized_and_assertions(call.expression),
                )?;
                let access = self.ctx.arena.get_access_expr(callee_node)?;
                Some(access.expression)
            }
        }
    }

    pub(crate) fn validate_assertion_call_target(
        &mut self,
        call_idx: NodeIndex,
        callee_idx: NodeIndex,
    ) -> bool {
        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        if !self.is_identifier_or_qualified_assertion_target(callee_idx) {
            self.error_at_node(
                call_idx,
                diagnostic_messages::ASSERTIONS_REQUIRE_THE_CALL_TARGET_TO_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME,
                diagnostic_codes::ASSERTIONS_REQUIRE_THE_CALL_TARGET_TO_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME,
            );
            self.ctx
                .call_type_predicates
                .mark_invalid_assertion_call(call_idx.0);
            return false;
        }

        if !self.assertion_call_target_has_explicit_annotations(callee_idx) {
            self.error_at_node(
                call_idx,
                diagnostic_messages::ASSERTIONS_REQUIRE_EVERY_NAME_IN_THE_CALL_TARGET_TO_BE_DECLARED_WITH_AN_EXPLICIT,
                diagnostic_codes::ASSERTIONS_REQUIRE_EVERY_NAME_IN_THE_CALL_TARGET_TO_BE_DECLARED_WITH_AN_EXPLICIT,
            );
            self.ctx
                .call_type_predicates
                .mark_invalid_assertion_call(call_idx.0);
            return false;
        }

        true
    }

    fn is_identifier_or_qualified_assertion_target(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.ctx.arena.get_access_expr(node).is_some_and(|access| {
                    self.is_identifier_or_qualified_assertion_target(access.expression)
                })
            }
            _ => false,
        }
    }

    fn assertion_call_target_has_explicit_annotations(&mut self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return true;
        };
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .resolve_identifier_symbol(expr_idx)
                .is_none_or(|sym_id| self.symbol_has_explicit_assertion_annotation(sym_id)),
            k if k == SyntaxKind::ThisKeyword as u16 || k == SyntaxKind::SuperKeyword as u16 => {
                true
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let Some(access) = self.ctx.arena.get_access_expr(node) else {
                    return true;
                };
                self.assertion_call_target_has_explicit_annotations(access.expression)
                    && self.assertion_property_has_explicit_annotation(
                        access.expression,
                        access.name_or_argument,
                    )
            }
            _ => true,
        }
    }

    fn assertion_property_has_explicit_annotation(
        &mut self,
        receiver_idx: NodeIndex,
        name_idx: NodeIndex,
    ) -> bool {
        if let Some(&sym_id) = self.ctx.binder.node_symbols.get(&name_idx.0) {
            return self.symbol_has_explicit_assertion_annotation(sym_id);
        }

        let receiver_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(receiver_idx);
        let Some(receiver_node) = self.ctx.arena.get(receiver_idx) else {
            return true;
        };
        let Some(property_name) = self.get_property_name(name_idx) else {
            return true;
        };

        if receiver_node.kind == SyntaxKind::Identifier as u16
            && let Some(ns_sym_id) = self.resolve_identifier_symbol(receiver_idx)
            && let Some(ns_symbol) = self.ctx.binder.get_symbol(ns_sym_id)
            && let Some(exports) = ns_symbol.exports.as_ref()
            && let Some(member_sym_id) = exports.get(&property_name)
        {
            return self.symbol_has_explicit_assertion_annotation(member_sym_id);
        }

        if receiver_node.kind == SyntaxKind::ThisKeyword as u16
            && let Some(member_idx) = self.enclosing_class_member_by_name(&property_name)
        {
            return self.declaration_has_explicit_assertion_annotation(member_idx);
        }

        true
    }

    fn enclosing_class_member_by_name(&self, property_name: &str) -> Option<NodeIndex> {
        self.ctx
            .enclosing_class
            .as_ref()?
            .member_nodes
            .iter()
            .copied()
            .find(|&member_idx| {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    return false;
                };
                let name_idx = self.member_declaration_name(member_node);
                name_idx
                    .and_then(|idx| self.get_property_name(idx))
                    .is_some_and(|name| name == property_name)
            })
    }

    fn member_declaration_name(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
            return Some(prop.name);
        }
        if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
            return Some(method.name);
        }
        if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
            return Some(accessor.name);
        }
        None
    }

    fn symbol_has_explicit_assertion_annotation(&mut self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };
        let Some(decl_idx) = symbol.primary_declaration() else {
            return true;
        };
        self.declaration_has_explicit_assertion_annotation(decl_idx)
    }

    fn declaration_has_explicit_assertion_annotation(&mut self, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return true;
        };
        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) {
            // `const foo = (a) => { … }` with `@returns {asserts a is B}` is a
            // valid assertion target in JS files: tsc treats the JSDoc
            // `@returns` predicate as the explicit annotation. Without this
            // arm, every JS-file arrow-bound assertion function fires a
            // spurious TS2775 at the call site. The function/method/accessor
            // arms below already include the same check; this mirrors them
            // for arrow-/function-expression initializers bound through a
            // variable declaration.
            return var_decl.type_annotation.is_some()
                || self.declaration_has_jsdoc_type_tag(decl_idx)
                || self.declaration_has_jsdoc_assertion_return(decl_idx)
                || self.for_of_variable_has_explicit_iterable_source_annotation(decl_idx)
                || self
                    .require_initializer_exports_explicit_assertion_function(var_decl.initializer);
        }
        if let Some(param) = self.ctx.arena.get_parameter(decl_node) {
            return param.type_annotation.is_some()
                || self.declaration_has_jsdoc_type_tag(decl_idx);
        }
        if let Some(prop) = self.ctx.arena.get_property_decl(decl_node) {
            return prop.type_annotation.is_some()
                || self.declaration_has_jsdoc_type_tag(decl_idx)
                || self.declaration_has_jsdoc_assertion_return(decl_idx);
        }
        if let Some(method) = self.ctx.arena.get_method_decl(decl_node) {
            return method.type_annotation.is_some()
                || self.declaration_has_jsdoc_type_tag(decl_idx)
                || self.declaration_has_jsdoc_assertion_return(decl_idx);
        }
        if let Some(accessor) = self.ctx.arena.get_accessor(decl_node) {
            return accessor.type_annotation.is_some()
                || self.declaration_has_jsdoc_type_tag(decl_idx)
                || self.declaration_has_jsdoc_assertion_return(decl_idx);
        }
        if let Some(func) = self.ctx.arena.get_function(decl_node) {
            return func.type_annotation.is_some()
                || self.declaration_has_jsdoc_type_tag(decl_idx)
                || self.declaration_has_jsdoc_assertion_return(decl_idx);
        }
        if let Some(sig) = self.ctx.arena.get_signature(decl_node) {
            return sig.type_annotation.is_some();
        }
        true
    }

    fn for_of_variable_has_explicit_iterable_source_annotation(
        &mut self,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(list_idx) = self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(for_idx) = self.ctx.arena.get_extended(list_idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(for_node) = self.ctx.arena.get(for_idx) else {
            return false;
        };
        if for_node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_data) = self.ctx.arena.get_for_in_of(for_node) else {
            return false;
        };
        let expression = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(for_data.expression);
        let Some(expr_node) = self.ctx.arena.get(expression) else {
            return false;
        };
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                self.assertion_call_target_has_explicit_annotations(expression)
            }
            _ => false,
        }
    }

    fn require_initializer_exports_explicit_assertion_function(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        if !self.is_js_file() || initializer.is_none() {
            return false;
        }

        let Some(module_specifier) = self.get_require_module_specifier(initializer) else {
            return false;
        };
        let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, &module_specifier)
            .or_else(|| self.ctx.resolve_import_target(&module_specifier))
        else {
            return false;
        };
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        if !target_arena
            .source_files
            .first()
            .is_some_and(|source_file| source_file.is_declaration_file)
        {
            return false;
        }

        let Some(exports) = self.resolve_effective_module_exports_from_file(
            &module_specifier,
            Some(self.ctx.current_file_idx),
        ) else {
            return false;
        };
        let Some(export_equals_sym_id) = exports.get("export=") else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };
        let Some(export_symbol) = target_binder.get_symbol(export_equals_sym_id) else {
            return false;
        };
        let Some(decl_idx) = export_symbol.primary_declaration() else {
            return false;
        };

        Self::declaration_has_syntactic_type_annotation(target_arena, decl_idx)
    }

    fn declaration_has_syntactic_type_annotation(arena: &NodeArena, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = arena.get(decl_idx) else {
            return true;
        };
        if let Some(var_decl) = arena.get_variable_declaration(decl_node) {
            return var_decl.type_annotation.is_some();
        }
        if let Some(param) = arena.get_parameter(decl_node) {
            return param.type_annotation.is_some();
        }
        if let Some(prop) = arena.get_property_decl(decl_node) {
            return prop.type_annotation.is_some();
        }
        if let Some(method) = arena.get_method_decl(decl_node) {
            return method.type_annotation.is_some();
        }
        if let Some(accessor) = arena.get_accessor(decl_node) {
            return accessor.type_annotation.is_some();
        }
        if let Some(func) = arena.get_function(decl_node) {
            return func.type_annotation.is_some();
        }
        if let Some(sig) = arena.get_signature(decl_node) {
            return sig.type_annotation.is_some();
        }
        true
    }

    fn declaration_has_jsdoc_type_tag(&self, decl_idx: NodeIndex) -> bool {
        self.find_jsdoc_for_assertion_declaration(decl_idx)
            .is_some_and(|jsdoc| Self::jsdoc_extract_type_tag_expr(&jsdoc).is_some())
    }

    fn declaration_has_jsdoc_assertion_return(&self, decl_idx: NodeIndex) -> bool {
        self.find_jsdoc_for_assertion_declaration(decl_idx)
            .is_some_and(|jsdoc| Self::jsdoc_returns_type_predicate(&jsdoc).is_some())
    }

    fn find_jsdoc_for_assertion_declaration(&self, decl_idx: NodeIndex) -> Option<String> {
        if !self.is_js_file() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if self.ctx.arena.get_function(decl_node).is_some()
            || self.ctx.arena.get_method_decl(decl_node).is_some()
            || self.ctx.arena.get_accessor(decl_node).is_some()
        {
            return self.find_jsdoc_for_function(decl_idx);
        }

        let sf = self.source_file_data_for_node(decl_idx)?;
        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        self.try_jsdoc_with_ancestor_walk(decl_idx, &comments, &source_text)
    }

    fn first_unannotated_callback_param_name_in_call(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let call = self
            .ctx
            .arena
            .get(idx)
            .and_then(|node| self.ctx.arena.get_call_expr(node))?;
        let args = call.arguments.as_ref()?;
        for &arg_idx in &args.nodes {
            if let Some(param_name) = self.first_unannotated_callback_param_name_in_node(arg_idx) {
                return Some(param_name);
            }
        }
        None
    }

    fn first_unannotated_callback_param_name_in_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let node = self.ctx.arena.get(idx)?;

        if node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::METHOD_DECLARATION
            || node.kind == syntax_kind_ext::GET_ACCESSOR
            || node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            let params = if let Some(func) = self.ctx.arena.get_function(node) {
                Some(func.parameters.nodes.as_slice())
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                Some(method.parameters.nodes.as_slice())
            } else {
                self.ctx
                    .arena
                    .get_accessor(node)
                    .map(|accessor| accessor.parameters.nodes.as_slice())
            }?;

            for &param_idx in params {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if param.type_annotation.is_some() || self.is_this_parameter_name(param.name) {
                    continue;
                }
                return Some(param.name);
            }
            return None;
        }

        if let Some(literal) = self.ctx.arena.get_literal_expr(node) {
            for &element_idx in &literal.elements.nodes {
                let Some(element) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                let child_idx = if let Some(prop) = self.ctx.arena.get_property_assignment(element)
                {
                    prop.initializer
                } else if let Some(spread) = self.ctx.arena.get_spread(element) {
                    spread.expression
                } else {
                    element_idx
                };
                if let Some(param_name) =
                    self.first_unannotated_callback_param_name_in_node(child_idx)
                {
                    return Some(param_name);
                }
            }
        }

        None
    }

    fn node_is_empty_array_literal_for_evolving_call(&self, idx: NodeIndex) -> bool {
        self.ctx.arena.get(idx).is_some_and(|node| {
            node.kind == tsz_parser::parser::syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_literal_expr(node)
                    .is_some_and(|lit| lit.elements.nodes.is_empty())
        })
    }

    fn reference_has_reachable_empty_array_assignment_for_call(
        &self,
        reference: NodeIndex,
    ) -> bool {
        let Some(flow_node) = self.flow_node_for_reference_usage(reference) else {
            return false;
        };
        let analyzer = self.flow_analyzer();
        let mut worklist = vec![flow_node];
        let mut visited = rustc_hash::FxHashSet::default();
        while let Some(current) = worklist.pop() {
            if !visited.insert(current) {
                continue;
            }
            let Some(flow) = self.ctx.binder.flow_nodes.get(current) else {
                continue;
            };
            if flow.has_any_flags(tsz_binder::flow_flags::ASSIGNMENT)
                && let Some(rhs) = analyzer.assignment_rhs_for_reference(flow.node, reference)
                && self.node_is_empty_array_literal_for_evolving_call(rhs)
            {
                return true;
            }
            for &antecedent in flow.antecedent.iter().rev() {
                if antecedent.is_some() {
                    worklist.push(antecedent);
                }
            }
        }
        false
    }

    pub(crate) fn receiver_reference_for_evolving_array_mutation(
        &self,
        receiver: NodeIndex,
    ) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let receiver = self.ctx.arena.skip_parenthesized_and_assertions(receiver);
        let Some(node) = self.ctx.arena.get(receiver) else {
            return receiver;
        };
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
        {
            if binary.operator_token == SyntaxKind::CommaToken as u16 {
                return self.receiver_reference_for_evolving_array_mutation(binary.right);
            }
            if crate::query_boundaries::common::is_assignment_operator(binary.operator_token) {
                return self.receiver_reference_for_evolving_array_mutation(binary.left);
            }
        }
        receiver
    }

    pub(crate) fn reference_has_direct_empty_array_initializer_for_evolving_mutation(
        &self,
        reference: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(sym_id) = self.resolve_identifier_symbol(reference) else {
            return false;
        };
        self.ctx
            .binder
            .get_symbol(sym_id)
            .and_then(|symbol| {
                let decl_idx = symbol.value_declaration;
                let mut decl_node = self.ctx.arena.get(decl_idx)?;
                if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    && let Some(parent_idx) = self.ctx.arena.parent_of(decl_idx)
                    && let Some(parent_node) = self.ctx.arena.get(parent_idx)
                    && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                {
                    decl_node = parent_node;
                }
                let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
                if decl.type_annotation.is_some() || decl.initializer.is_none() {
                    return Some(false);
                }
                Some(self.node_is_empty_array_literal_for_evolving_call(decl.initializer))
            })
            .unwrap_or(false)
    }

    pub(crate) fn reference_is_reachable_evolving_array_mutation_target(
        &mut self,
        reference: NodeIndex,
    ) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol(reference) else {
            return false;
        };
        (self.assignment_target_is_control_flow_typed_any_symbol(sym_id)
            && self.reference_has_reachable_empty_array_assignment_for_call(reference))
            || self.reference_has_direct_empty_array_initializer_for_evolving_mutation(reference)
    }

    fn type_is_array_or_union_of_arrays(&self, type_id: TypeId) -> bool {
        use crate::query_boundaries::common;

        if common::array_element_type(self.ctx.types, type_id).is_some() {
            return true;
        }
        common::union_members(self.ctx.types, type_id).is_some_and(|members| {
            !members.is_empty()
                && members
                    .iter()
                    .all(|&member| common::array_element_type(self.ctx.types, member).is_some())
        })
    }

    fn call_is_simple_evolving_array_mutation(&mut self, callee_expr: NodeIndex) -> bool {
        use crate::query_boundaries::common;
        use tsz_parser::parser::syntax_kind_ext;

        let callee_expr = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(callee_expr);
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && callee_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(method_name) = self.get_property_name(access.name_or_argument) else {
            return false;
        };
        if method_name != "push" && method_name != "unshift" {
            return false;
        }

        let receiver_ref = self.receiver_reference_for_evolving_array_mutation(access.expression);
        let Some(sym_id) = self.resolve_identifier_symbol(receiver_ref) else {
            return false;
        };
        let is_control_flow_any = self.assignment_target_is_control_flow_typed_any_symbol(sym_id)
            && self.reference_has_reachable_empty_array_assignment_for_call(receiver_ref);
        let is_direct_empty_array =
            self.reference_has_direct_empty_array_initializer_for_evolving_mutation(receiver_ref);
        if !is_control_flow_any && !is_direct_empty_array {
            return false;
        }

        let receiver_type = self.get_type_of_node(access.expression);
        if is_direct_empty_array {
            self.type_is_array_or_union_of_arrays(receiver_type)
        } else {
            common::union_members(self.ctx.types, receiver_type).is_none()
                && common::array_element_type(self.ctx.types, receiver_type).is_some()
        }
    }

    /// Determine whether a call/new callee that resolved to `TypeId::ERROR`
    /// emitted a name/value resolution diagnostic at the callee site. Used to
    /// suppress contextual `any` for callback arguments so TS7006 still fires
    /// after the callee's name lookup failed.
    pub(crate) fn callee_suppresses_contextual_any(
        &self,
        callee_idx: NodeIndex,
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };

        let is_simple_error_path = matches!(
            callee_node.kind,
            k if k == tsz_scanner::SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        );
        if !is_simple_error_path {
            return false;
        }

        let has_callee_side_failure =
            self.ctx.speculative_diagnostics_since(snap).iter().any(|diag| {
                diag.start >= callee_node.pos
                    && diag.start < callee_node.end
                    && matches!(
                        diag.code,
                        diagnostic_codes::CANNOT_FIND_NAME
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS
                            | diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                            | diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN
                            | diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE
                            | diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                            | diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE
                            | diagnostic_codes::TYPE_HAS_NO_CALL_SIGNATURES
                    )
            });

        has_callee_side_failure || self.property_access_base_is_error_symbol(callee_idx)
    }

    fn property_access_base_is_error_symbol(&self, callee_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && callee_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let base_expr = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(access.expression);
        let Some(base_node) = self.ctx.arena.get(base_expr) else {
            return false;
        };
        if base_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }

        self.resolve_identifier_symbol(base_expr)
            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
            == Some(TypeId::ERROR)
    }

    fn reemit_namespace_value_error_for_call_callee(&mut self, callee_idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return;
        };

        let base_expr = match callee_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.ctx
                    .arena
                    .get_access_expr(callee_node)
                    .map(|access| access.expression)
            }
            _ => None,
        };

        let Some(base_expr) = base_expr else {
            return;
        };
        let base_expr = self.ctx.arena.skip_parenthesized_and_assertions(base_expr);

        let _ = self.report_namespace_value_access_for_type_only_import_equals_expr(base_expr);
    }

    #[allow(clippy::too_many_arguments)]
    fn finalize_call_after_argument_collection(
        &mut self,
        idx: NodeIndex,
        callee_expr: NodeIndex,
        args: &[NodeIndex],
        mut arg_types: Vec<TypeId>,
        callee_type: TypeId,
        callee_type_for_resolution: TypeId,
        base_contextual_param_types: &[Option<TypeId>],
        non_generic_contextual_types: Option<&[Option<TypeId>]>,
        check_excess_properties: bool,
        callable_ctx: crate::call_checker::CallableContext,
        is_generic_call: bool,
        contextual_type: Option<TypeId>,
        force_bivariant_callbacks: bool,
        actual_this_type: Option<TypeId>,
        is_super_call: bool,
        is_optional_chain: bool,
        had_return_context_substitution: bool,
        shape_this_type: Option<TypeId>,
        pushed_this_type_from_shape: bool,
    ) -> TypeId {
        use crate::query_boundaries::assignability as assign_query;
        use crate::query_boundaries::checkers::call as call_checker;
        use crate::query_boundaries::checkers::call::is_type_parameter_type;
        use crate::query_boundaries::common;
        use crate::query_boundaries::common::ContextualTypeContext;
        use tsz_parser::parser::syntax_kind_ext;

        self.ensure_relation_input_ready(callee_type_for_resolution);

        let callee_type_for_call = self.evaluate_application_type(callee_type_for_resolution);
        let callee_type_for_call = self.resolve_lazy_type(callee_type_for_call);
        let callee_type_for_call = self.resolve_lazy_members_in_union(callee_type_for_call);
        let callee_type_for_call =
            self.replace_function_type_for_call(callee_type, callee_type_for_call);
        if callee_type_for_call == TypeId::ANY {
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None,
                crate::call_checker::CallableContext::none(),
            );
            return if is_optional_chain {
                common::union_with_undefined(self.ctx.types, TypeId::ANY)
            } else {
                TypeId::ANY
            };
        }

        self.ensure_relation_input_ready(callee_type_for_call);

        let (generic_inference_arg_types, sanitized_generic_inference) = if is_generic_call {
            self.sanitize_generic_inference_arg_types(callee_expr, args, &arg_types)
        } else {
            (arg_types.clone(), false)
        };
        let generic_inference_arg_source_markers = if is_generic_call {
            self.call_arg_source_type_annotation_markers(args, generic_inference_arg_types.len())
        } else {
            Vec::new()
        };
        let call_resolution_contextual_type = contextual_type;

        let (
            mut result,
            mut instantiated_predicate,
            mut generic_instantiated_params,
            mut relation_evidence,
        ) = if is_super_call {
            (
                self.resolve_new_with_checker_adapter(
                    callee_type_for_call,
                    &generic_inference_arg_types,
                    force_bivariant_callbacks,
                    call_resolution_contextual_type,
                ),
                None,
                None,
                Vec::new(),
            )
        } else if generic_inference_arg_source_markers.iter().any(|&m| m) {
            let resolution = self.resolve_call_with_checker_adapter_and_arg_sources_evidence(
                callee_type_for_call,
                &generic_inference_arg_types,
                force_bivariant_callbacks,
                call_resolution_contextual_type,
                actual_this_type,
                &generic_inference_arg_source_markers,
            );
            (
                resolution.result,
                resolution.selected_type_predicate,
                resolution.instantiated_params,
                resolution.relation_evidence,
            )
        } else {
            let resolution = self.resolve_call_with_checker_adapter_evidence(
                callee_type_for_call,
                &generic_inference_arg_types,
                force_bivariant_callbacks,
                call_resolution_contextual_type,
                actual_this_type,
            );
            (
                resolution.result,
                resolution.selected_type_predicate,
                resolution.instantiated_params,
                resolution.relation_evidence,
            )
        };
        let needs_real_type_recheck = is_generic_call
            && args.iter().enumerate().any(|(i, &arg_idx)| {
                self.argument_needs_refresh_for_contextual_call(
                    arg_idx,
                    base_contextual_param_types.get(i).copied().flatten(),
                )
            });

        if !is_generic_call
            && let crate::query_boundaries::common::CallResult::ArgumentTypeMismatch {
                index,
                fallback_return,
                ..
            } = result.clone()
            && let Some(expected) = non_generic_contextual_types
                .and_then(|types| types.get(index).copied().flatten())
                .map(|expected| self.evaluate_contextual_type(expected))
            && let Some(&arg_idx) = args.get(index)
            && let Some(actual) = Some(self.refreshed_generic_call_arg_type_with_context(
                arg_idx,
                arg_types.get(index).copied().unwrap_or(TypeId::UNKNOWN),
                Some(expected),
            ))
        {
            let fresh_subtype = assign_query::is_fresh_subtype_of(self.ctx.types, actual, expected);
            let recover_object_literal =
                fresh_subtype
                    && !self.object_literal_has_computed_property_names(arg_idx)
                    && self.ctx.arena.get(arg_idx).is_some_and(|node| {
                        node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    });
            if recover_object_literal {
                if expected != TypeId::ANY
                    && expected != TypeId::UNKNOWN
                    && !is_type_parameter_type(self.ctx.types, expected)
                    && !self.contextual_type_is_unresolved_for_argument_refresh(expected)
                {
                    self.check_object_literal_excess_properties(actual, expected, arg_idx);
                }
                let recovered_return = if fallback_return != TypeId::ERROR {
                    Some(fallback_return)
                } else {
                    assign_query::get_function_return_type(self.ctx.types, callee_type_for_call)
                };
                if let Some(return_type) = recovered_return {
                    result = crate::query_boundaries::common::CallResult::Success(return_type);
                }
            }
        }

        let retry_contextual_param_types = if is_generic_call && had_return_context_substitution {
            generic_instantiated_params.as_ref().map(|params| {
                self.contextual_param_types_from_instantiated_params(params, args.len())
            })
        } else {
            None
        };
        let has_contextual_signature_instantiation_arg =
            args.iter().enumerate().any(|(i, &arg_idx)| {
                let expected_type = retry_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(i).copied().flatten())
                    .or_else(|| base_contextual_param_types.get(i).copied().flatten());
                self.expression_needs_contextual_signature_instantiation(arg_idx, expected_type)
            });
        let has_contextual_refresh_arg = args.iter().enumerate().any(|(i, &arg_idx)| {
            self.argument_needs_refresh_for_contextual_call(
                arg_idx,
                retry_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(i).copied().flatten())
                    .or_else(|| base_contextual_param_types.get(i).copied().flatten()),
            )
        });
        let should_retry_generic_call = if is_generic_call
            && (!had_return_context_substitution || has_contextual_signature_instantiation_arg)
            && has_contextual_refresh_arg
        {
            if let Some(ctx_type) = contextual_type {
                match &result {
                    crate::query_boundaries::common::CallResult::Success(ret) => {
                        let contextual_return = self.evaluate_contextual_type(ctx_type);
                        !self.is_assignable_to_with_env(*ret, contextual_return)
                    }
                    _ => true,
                }
            } else {
                true
            }
        } else {
            false
        };

        if is_generic_call
            && should_retry_generic_call
            && let Some(instantiated_params) = generic_instantiated_params.as_ref()
        {
            self.clear_contextual_resolution_cache();
            for (i, &arg_idx) in args.iter().enumerate() {
                if self.argument_needs_refresh_for_contextual_call(
                    arg_idx,
                    base_contextual_param_types.get(i).copied().flatten(),
                ) {
                    self.invalidate_expression_for_contextual_retry(arg_idx);
                }
            }
            let refreshed_contextual_types = self
                .contextual_param_types_from_instantiated_params(instantiated_params, args.len())
                .into_iter()
                .map(|param_type| {
                    param_type
                        .map(|param_type| self.normalize_contextual_call_param_type(param_type))
                })
                .collect::<Vec<_>>();
            arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, _arg_count| {
                    refreshed_contextual_types
                        .get(i)
                        .copied()
                        .flatten()
                        .or_else(|| base_contextual_param_types.get(i).copied().flatten())
                },
                check_excess_properties,
                None,
                callable_ctx,
            );

            let (retry_generic_arg_types, retry_sanitized) =
                self.sanitize_generic_inference_arg_types(callee_expr, args, &arg_types);
            let retry_arg_source_markers =
                self.call_arg_source_type_annotation_markers(args, retry_generic_arg_types.len());
            let retry = if is_super_call {
                (
                    self.resolve_new_with_checker_adapter(
                        callee_type_for_call,
                        &retry_generic_arg_types,
                        force_bivariant_callbacks,
                        contextual_type,
                    ),
                    None,
                    None,
                    Vec::new(),
                )
            } else if retry_arg_source_markers.iter().any(|&m| m) {
                let resolution = self.resolve_call_with_checker_adapter_and_arg_sources_evidence(
                    callee_type_for_call,
                    &retry_generic_arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                    &retry_arg_source_markers,
                );
                (
                    resolution.result,
                    resolution.selected_type_predicate,
                    resolution.instantiated_params,
                    resolution.relation_evidence,
                )
            } else {
                let resolution = self.resolve_call_with_checker_adapter_evidence(
                    callee_type_for_call,
                    &retry_generic_arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                );
                (
                    resolution.result,
                    resolution.selected_type_predicate,
                    resolution.instantiated_params,
                    resolution.relation_evidence,
                )
            };
            result = if retry_sanitized || needs_real_type_recheck {
                if let Some(instantiated_params) = retry.2.as_ref() {
                    self.recheck_generic_call_arguments_with_real_types(
                        retry.0.clone(),
                        instantiated_params,
                        args,
                        &arg_types,
                    )
                } else {
                    retry.0
                }
            } else {
                retry.0
            };
            instantiated_predicate = retry.1;
            generic_instantiated_params = retry.2;
            relation_evidence = retry.3;
        }

        if is_generic_call
            && let crate::query_boundaries::common::CallResult::Success(return_type) = result
            && let Some(ctx_type) =
                contextual_type.filter(|&ct| ct != TypeId::ANY && ct != TypeId::UNKNOWN)
            && (common::contains_type_parameters(self.ctx.types, return_type)
                || common::contains_infer_types(self.ctx.types, return_type)
                || common::contains_type_by_id(self.ctx.types, return_type, TypeId::UNKNOWN))
            && let Some(shape) = call_checker::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type_for_call,
                args.len(),
            )
        {
            let mut return_context_substitution =
                self.compute_return_context_substitution_from_shape(&shape, Some(ctx_type));
            let return_param_names: rustc_hash::FxHashSet<_> = self
                .function_like_return_parameter_type_params(&shape)
                .into_iter()
                .collect();
            let same_return_context_application =
                common::application_info(self.ctx.types, shape.return_type)
                    .zip(common::application_info(self.ctx.types, ctx_type))
                    .is_some_and(|((return_base, _), (ctx_base, _))| return_base == ctx_base);
            let return_context_specializes_return_params = !return_param_names.is_empty()
                && self.contextual_return_type_specializes_wrapped_params(
                    shape.return_type,
                    ctx_type,
                    &return_param_names,
                    &mut rustc_hash::FxHashSet::default(),
                );
            if !return_param_names.is_empty()
                && !same_return_context_application
                && !return_context_specializes_return_params
            {
                let mut filtered = crate::query_boundaries::common::TypeSubstitution::new();
                for (&name, &type_id) in return_context_substitution.map() {
                    if !return_param_names.contains(&name) {
                        filtered.insert(name, type_id);
                    }
                }
                return_context_substitution = filtered;
            }

            if !return_context_substitution.is_empty() {
                let instantiated_return = crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    return_type,
                    &return_context_substitution,
                );
                if instantiated_return != return_type {
                    result =
                        crate::query_boundaries::common::CallResult::Success(instantiated_return);
                }
            }
        }

        if let Some(predicate) = instantiated_predicate {
            let stored_predicate =
                call_checker::extract_predicate_signature(self.ctx.types, callee_type_for_call)
                    .filter(|sig| {
                        sig.predicate.type_id.is_some_and(|pred_ty| {
                            common::type_param_info(self.ctx.types, pred_ty).is_some()
                        })
                    })
                    .map(|sig| (sig.predicate, sig.params))
                    .unwrap_or(predicate);
            self.store_call_type_predicate(idx, callee_expr, stored_predicate);
        } else {
            let is_sound_union = if common::is_union_type(self.ctx.types, callee_type_for_call) {
                call_checker::is_valid_union_predicate(self.ctx.types, callee_type_for_call)
            } else {
                true
            };
            if is_sound_union
                && let Some(extracted) =
                    call_checker::extract_predicate_signature(self.ctx.types, callee_type_for_call)
            {
                self.store_call_type_predicate(
                    idx,
                    callee_expr,
                    (extracted.predicate, extracted.params),
                );
            }
        }

        let (mut result, mut allow_contextual_mismatch_deferral) = self
            .finalize_generic_call_result(
                callee_type_for_call,
                generic_instantiated_params.as_ref(),
                args,
                &arg_types,
                result,
                sanitized_generic_inference,
                needs_real_type_recheck,
                shape_this_type,
            );
        let finalized_contextual_param_types = generic_instantiated_params
            .as_ref()
            .map(|params| self.contextual_param_types_from_instantiated_params(params, args.len()));
        let forced_block_body_callback_mismatch = self
            .current_block_body_callback_return_mismatch_arg(args, |checker, index| {
                finalized_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(index).copied().flatten())
                    .or_else(|| {
                        ContextualTypeContext::with_expected_and_options(
                            checker.ctx.types,
                            callee_type_for_call,
                            checker.ctx.compiler_options.no_implicit_any,
                        )
                        .get_parameter_type_for_call(index, args.len())
                    })
            })
            .inspect(|&(index, actual, expected)| {
                if let crate::query_boundaries::common::CallResult::Success(return_type) = result {
                    allow_contextual_mismatch_deferral = false;
                    result = crate::query_boundaries::common::CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: return_type,
                    };
                }
            })
            .is_some();
        let forced_binding_pattern_unknown_context_mismatch = self
            .current_binding_pattern_callback_unknown_context_arg(args, |checker, index| {
                finalized_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(index).copied().flatten())
                    .or_else(|| {
                        ContextualTypeContext::with_expected_and_options(
                            checker.ctx.types,
                            callee_type_for_call,
                            checker.ctx.compiler_options.no_implicit_any,
                        )
                        .get_parameter_type_for_call(index, args.len())
                    })
            })
            .inspect(|&(index, actual, expected)| {
                if matches!(
                    result,
                    crate::query_boundaries::common::CallResult::Success(_)
                ) && let Some(&arg_idx) = args.get(index)
                {
                    allow_contextual_mismatch_deferral = false;
                    self.error_argument_not_assignable_at(actual, expected, arg_idx);
                }
            })
            .is_some();
        if forced_block_body_callback_mismatch {
            allow_contextual_mismatch_deferral = false;
        }
        if let crate::query_boundaries::common::CallResult::ArgumentTypeMismatch {
            actual: _,
            expected: _,
            fallback_return,
            ..
        } = result
            && !forced_block_body_callback_mismatch
            && !forced_binding_pattern_unknown_context_mismatch
            && fallback_return != TypeId::ERROR
        {}

        if let crate::query_boundaries::common::CallResult::ArgumentTypeMismatch {
            fallback_return,
            ..
        } = result
            && self.call_is_simple_evolving_array_mutation(callee_expr)
        {
            result = crate::query_boundaries::common::CallResult::Success(fallback_return);
        }

        let call_context = super::call_result::CallResultContext {
            callee_expr,
            call_idx: idx,
            args,
            arg_types: &arg_types,
            callee_type: callee_type_for_call,
            callee_has_declared_generic_signature: is_generic_call,
            is_super_call,
            is_optional_chain,
            allow_contextual_mismatch_deferral,
            relation_evidence: &relation_evidence,
        };
        if pushed_this_type_from_shape {
            self.ctx.this_type_stack.pop();
        }
        self.handle_call_result(result, call_context)
    }

    /// Get the type of a call expression (e.g., `foo()`, `obj.method()`).
    ///
    /// Computes the return type of function/method calls.
    /// Handles:
    /// - Dynamic imports (returns `Promise<any>`)
    /// - Super calls (returns `void`)
    /// - Optional chaining (`obj?.method()`)
    /// - Overload resolution
    /// - Argument type checking
    /// - Type argument validation (TS2344)
    #[allow(dead_code)]
    pub(crate) fn get_type_of_call_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_call_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_call_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        // Check call depth limit to prevent infinite recursion
        if !self.ctx.call_depth.borrow_mut().enter() {
            return TypeId::ERROR;
        }

        let result = self.get_type_of_call_expression_inner(idx, request);

        // TS2590: Check if the call produced a union type that is too complex.
        // The solver sets a flag during union normalization when the constituent
        // count exceeds the threshold. We check and clear it here to emit the
        // diagnostic at the call expression that triggered it.
        if self.ctx.types.take_union_too_complex() {
            use crate::diagnostics::diagnostic_messages;
            let diagnostic_idx = self
                .first_unannotated_callback_param_name_in_call(idx)
                .unwrap_or(idx);
            self.error_at_node(
                diagnostic_idx,
                diagnostic_messages::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
                diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
            );
            if diagnostic_idx != idx
                && self.ctx.no_implicit_any()
                && let Some(param_name) = self.get_parameter_name(diagnostic_idx)
            {
                self.error_at_node_msg(
                    diagnostic_idx,
                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                    &[&param_name, "any"],
                );
            }
        }

        self.ctx.call_depth.borrow_mut().leave();
        result
    }

    /// Check if a call is a dynamic import and handle all associated diagnostics.
    /// Returns `Some(type_id)` if this is a dynamic import (the caller should return it),
    /// or `None` if this is not a dynamic import.
    fn check_and_resolve_dynamic_import(
        &mut self,
        idx: NodeIndex,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> Option<TypeId> {
        if !self.is_dynamic_import(call) {
            return None;
        }
        let in_import_type_context = self.is_import_call_in_type_context(idx);

        // TS1323: Dynamic imports require a module kind that supports them
        if !in_import_type_context && !self.ctx.compiler_options.module.supports_dynamic_import() {
            self.error_at_node(
                idx,
                crate::diagnostics::diagnostic_messages::DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022,
                diagnostic_codes::DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022,
            );
        }

        // TS1325: Check for spread elements in import arguments
        if let Some(ref args_list) = call.arguments {
            for &arg_idx in &args_list.nodes {
                if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                    && arg_node.kind == tsz_parser::parser::syntax_kind_ext::SPREAD_ELEMENT
                {
                    self.error_at_node(
                        arg_idx,
                        crate::diagnostics::diagnostic_messages::ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT,
                        diagnostic_codes::ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT,
                    );
                }
            }
        }

        // TS1324: Second argument only supported for certain module kinds.
        // Only emit when dynamic imports are supported (TS1323 not emitted),
        // otherwise TS1323 already covers the unsupported case.
        if let Some(ref args_list) = call.arguments
            && args_list.nodes.len() >= 2
            && self.ctx.compiler_options.module.supports_dynamic_import()
            && !self
                .ctx
                .compiler_options
                .module
                .supports_dynamic_import_options()
        {
            self.error_at_node(
                args_list.nodes[1],
                crate::diagnostics::diagnostic_messages::DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO,
                diagnostic_codes::DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO,
            );
        }

        // TS7036: Check specifier type is assignable to `string`
        self.check_dynamic_import_specifier_type(call);
        // TS2322/TS2559: Check options arg against ImportCallOptions
        self.check_dynamic_import_options_type(call);
        self.check_dynamic_import_module_specifier(call);

        // TS2712: Dynamic import requires Promise constructor support from the
        // active libs / declarations. This is lib-driven, not target-driven:
        // `@target: es2015` with `@lib: es5` still needs the diagnostic.
        if self.ctx.promise_constructor_diagnostics_required() {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
                diagnostic_codes::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
            );
        }

        // Dynamic imports return Promise<typeof module>
        // This creates Promise<ModuleNamespace> where ModuleNamespace contains all exports
        Some(self.get_dynamic_import_type(call))
    }

    fn is_import_call_in_type_context(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..12 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            match parent_node.kind {
                syntax_kind_ext::TYPE_REFERENCE
                | syntax_kind_ext::TYPE_QUERY
                | syntax_kind_ext::IMPORT_TYPE => return true,
                syntax_kind_ext::QUALIFIED_NAME => current = parent_idx,
                _ => return false,
            }
        }
        false
    }

    /// Handle `unknown` and `never` callee types with appropriate diagnostics.
    /// Returns `Some(type_id)` if the callee type was handled (caller should return),
    /// or `None` to continue with normal call resolution.
    fn check_callee_unknown_or_never(
        &mut self,
        callee_type: TypeId,
        callee_expr: NodeIndex,
        args: &[NodeIndex],
    ) -> Option<TypeId> {
        use crate::call_checker::CallableContext;
        use tsz_parser::parser::syntax_kind_ext;

        // TS18046: Calling an expression of type `unknown` is not allowed.
        // tsc emits TS18046 instead of TS2349 when the callee is `unknown`.
        // Without strictNullChecks, unknown is treated like any (callable, returns any).
        if callee_type == TypeId::UNKNOWN {
            if !self.ctx.compiler_options.strict_null_checks {
                // Without strictNullChecks, unknown is treated as callable for
                // argument typing, but the checker still reports TS2349.
                self.error_not_callable_at(callee_type, callee_expr);
                // Still need to check arguments for definite assignment (TS2454)
                let check_excess_properties = false;
                self.collect_call_argument_types_with_context(
                    args,
                    |_i, _arg_count| None,
                    check_excess_properties,
                    None,
                    CallableContext::none(),
                );
                return Some(TypeId::ANY);
            }
            if self.error_is_of_type_unknown(callee_expr) {
                // Still need to check arguments for definite assignment (TS2454)
                let check_excess_properties = false;
                self.collect_call_argument_types_with_context(
                    args,
                    |_i, _arg_count| None,
                    check_excess_properties,
                    None,
                    CallableContext::none(),
                );
                return Some(TypeId::ERROR);
            }
            // Without strictNullChecks, treat unknown like any: callable, returns any
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None,
                CallableContext::none(),
            );
            return Some(TypeId::ANY);
        }

        // Calling `never` returns `never` (bottom type propagation).
        // tsc treats `never` as having no call signatures.
        // For method calls (e.g., `a.toFixed()` where `a: never`), TS2339 is already
        // emitted by the property access check, so we suppress the redundant TS2349.
        // For direct calls on `never` (e.g., `f()` where `f: never`), emit TS2349.
        if callee_type == TypeId::NEVER {
            let is_method_call = matches!(
                self.ctx.arena.kind_at(callee_expr),
                Some(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                )
            );
            if !is_method_call {
                self.error_not_callable_at(callee_type, callee_expr);
            }
            return Some(TypeId::NEVER);
        }

        None
    }
}

// Identifier resolution is in `identifier.rs`.
// Tagged template expression handling is in `tagged_template.rs`.
// TDZ checking, value declaration resolution, and other helpers are in
// `call_helpers.rs`.
