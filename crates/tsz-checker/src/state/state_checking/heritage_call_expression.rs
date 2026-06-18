//! Heritage call-expression constructor compatibility checks and the type-param
//! reference helpers that support them. Split out of `heritage.rs` to keep each
//! file under the checker-boundary line cap.

use crate::query_boundaries::class_type as class_query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Check whether `node_idx` is lexically enclosed within a class declaration
    /// whose binder symbol equals `target_class_sym`.
    ///
    /// Walks AST parent pointers from `node_idx` upward. When it encounters a
    /// `CLASS_DECLARATION` or `CLASS_EXPRESSION`, it checks whether that class's
    /// binder symbol matches `target_class_sym`. If so, the node is inside the
    /// target class and has access to its private/protected constructor.
    ///
    /// This is used for TS2675: a class with a private constructor can still be
    /// extended by a class that is defined *within* the declaring class's body
    /// (e.g., inside one of its methods).
    pub(crate) fn is_lexically_inside_class(
        &self,
        node_idx: NodeIndex,
        target_class_sym: tsz_binder::SymbolId,
    ) -> bool {
        let mut current = node_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if (parent_node.is_class_like())
                && self
                    .ctx
                    .binder
                    .get_node_symbol(parent)
                    .is_some_and(|sym| sym == target_class_sym)
            {
                return true;
            }
            current = parent;
        }
    }

    /// Find a reference to an enclosing class type parameter inside a base class expression.
    ///
    /// This traverses the runtime expression tree and only inspects embedded type nodes
    /// (e.g., call/new type arguments, type assertions). It intentionally skips nested
    /// function/class expression scopes to avoid shadowing false positives.
    pub(crate) fn find_class_type_param_ref_in_base_expression(
        &self,
        expr_idx: NodeIndex,
        class_type_param_names: &[String],
    ) -> Option<NodeIndex> {
        if expr_idx.is_none() || class_type_param_names.is_empty() {
            return None;
        }

        let mut stack = vec![expr_idx];
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while let Some(current) = stack.pop() {
            if current.is_none() || !visited.insert(current) {
                continue;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };

            // Nested function/class expressions introduce their own type parameter
            // scopes and should not be treated as references to the outer class.
            if matches!(
                node.kind,
                syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::CLASS_EXPRESSION
            ) {
                continue;
            }

            if node.is_type_node() {
                if let Some(found) =
                    self.find_class_type_param_ref_in_type_node(current, class_type_param_names)
                {
                    return Some(found);
                }
                continue;
            }

            for child_idx in self.ctx.arena.get_children(current) {
                if child_idx.is_some() {
                    stack.push(child_idx);
                }
            }
        }

        None
    }

    /// Find a reference to one of `class_type_param_names` inside a type node.
    fn find_class_type_param_ref_in_type_node(
        &self,
        type_idx: NodeIndex,
        class_type_param_names: &[String],
    ) -> Option<NodeIndex> {
        if type_idx.is_none() || class_type_param_names.is_empty() {
            return None;
        }

        let node = self.ctx.arena.get(type_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                    if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        && class_type_param_names.contains(&ident.escaped_text)
                    {
                        return Some(type_ref.type_name);
                    }

                    if let Some(type_args) = &type_ref.type_arguments {
                        for &arg_idx in &type_args.nodes {
                            if let Some(found) = self.find_class_type_param_ref_in_type_node(
                                arg_idx,
                                class_type_param_names,
                            ) {
                                return Some(found);
                            }
                        }
                    }
                }
                None
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                let func_type = self.ctx.arena.get_function_type(node)?;

                let own_params = self.collect_type_parameter_names(&func_type.type_parameters);
                let filtered: Vec<String> = class_type_param_names
                    .iter()
                    .filter(|name| !own_params.contains(*name))
                    .cloned()
                    .collect();

                let names_to_check: &[String] = if own_params.is_empty() {
                    class_type_param_names
                } else if filtered.is_empty() {
                    return None;
                } else {
                    &filtered
                };

                for &param_idx in &func_type.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && let Some(found) = self.find_class_type_param_ref_in_type_node(
                            param.type_annotation,
                            names_to_check,
                        )
                    {
                        return Some(found);
                    }
                }

                self.find_class_type_param_ref_in_type_node(
                    func_type.type_annotation,
                    names_to_check,
                )
            }
            _ => {
                for child_idx in self.ctx.arena.get_children(type_idx) {
                    if let Some(found) = self
                        .find_class_type_param_ref_in_type_node(child_idx, class_type_param_names)
                    {
                        return Some(found);
                    }
                }
                None
            }
        }
    }

    /// Collect type parameter names from a type parameter list.
    fn collect_type_parameter_names(
        &self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<String> {
        let Some(list) = type_parameters else {
            return Vec::new();
        };

        let mut names = Vec::new();
        for &param_idx in &list.nodes {
            if let Some(node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_type_parameter(node)
                && let Some(name_node) = self.ctx.arena.get(param.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                names.push(ident.escaped_text.clone());
            }
        }
        names
    }

    /// TS2449/TS2450: Check if a class or enum referenced in a heritage clause
    /// is used before its declaration in the source order.
    pub(crate) fn check_heritage_class_before_declaration(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        usage_idx: NodeIndex,
    ) {
        use tsz_binder::symbol_flags;

        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return;
        };

        let is_class = symbol.has_any_flags(symbol_flags::CLASS);
        let is_enum = symbol.has_any_flags(symbol_flags::REGULAR_ENUM);
        if !is_class && !is_enum {
            return;
        }

        // Skip check for cross-file symbols (imported from another file).
        // Position comparison only makes sense within the same file.
        if symbol.import_module().is_some() {
            return;
        }
        // If decl_file_idx is set and differs from the current file, the declaration
        // is in another file — TDZ position comparison is meaningless across files.
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return;
        }

        // Get the declaration position
        let Some(decl_idx) = symbol.primary_declaration() else {
            return;
        };

        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        // In multi-file mode, decl_idx may be from a different file's arena.
        // Validate that the node at decl_idx actually matches the expected kind.
        // A mismatch means the declaration is in another file — no TDZ applies.
        if self.ctx.all_arenas.is_some() {
            let kind_ok = (is_class && (decl_node.is_class_like()))
                || (is_enum && decl_node.kind == syntax_kind_ext::ENUM_DECLARATION);
            if !kind_ok {
                return;
            }
        }

        // Skip check for ambient declarations — `declare class` is hoisted
        // and can be referenced before its source position.
        if self.is_ambient_declaration(decl_idx) {
            return;
        }

        // Skip check for ambient declarations - they don't have runtime initialization order
        // Check if the using class (heritage clause) is in an ambient declaration
        if is_class {
            // Find the class declaration that contains this heritage clause usage
            let mut current = usage_idx;
            while let Some(ext) = self.ctx.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                if let Some(parent_node) = self.ctx.arena.get(parent)
                    && parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                {
                    // Check if this class is ambient
                    if self.is_ambient_class_declaration(parent) {
                        return;
                    }
                    break; // Found the containing class, no need to check further
                }
                current = parent;
            }
        }

        // Only flag if usage is before declaration in source order
        if usage_node.pos >= decl_node.pos {
            return;
        }

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // Get the simple name from the symbol, not the full qualified expression text.
        // tsc uses the symbol's simple name (e.g., 'E') not the qualified name ('N.E').
        let name = symbol.escaped_name.clone();

        // For property access expressions like N.E, point the error at the right-hand
        // identifier (E), not the whole expression (N.E). tsc reports the error span
        // on just the class name, not the qualified access path.
        let error_node = if let Some(usage_node_data) = self.ctx.arena.get(usage_idx)
            && usage_node_data.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            if let Some(access) = self.ctx.arena.get_access_expr(usage_node_data) {
                access.name_or_argument
            } else {
                usage_idx
            }
        } else {
            usage_idx
        };

        let (msg_template, code) = if is_class {
            (
                diagnostic_messages::CLASS_USED_BEFORE_ITS_DECLARATION,
                diagnostic_codes::CLASS_USED_BEFORE_ITS_DECLARATION,
            )
        } else {
            (
                diagnostic_messages::ENUM_USED_BEFORE_ITS_DECLARATION,
                diagnostic_codes::ENUM_USED_BEFORE_ITS_DECLARATION,
            )
        };
        let message = format_message(msg_template, &[&name]);
        self.error_at_node(error_node, &message, code);
    }

    /// Check if a type (including intersection members) has generic construct signatures.
    ///
    /// For intersection types like `T & Constructor<MyMixin>`, checks each member
    /// for generic construct signatures. This is needed because `construct_signatures_for_type`
    /// only works on direct `Callable` types, not intersections.
    pub(crate) fn has_generic_construct_signatures(&self, type_id: TypeId) -> bool {
        if let Some(sigs) = class_query::construct_signatures_for_type(self.ctx.types, type_id)
            && sigs.iter().any(|sig| !sig.type_params.is_empty())
        {
            return true;
        }
        // For intersection types, check each member
        if let Some(members) = class_query::intersection_members(self.ctx.types, type_id) {
            for member in &members {
                if self.has_generic_construct_signatures(*member) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn check_heritage_call_expression_constructor_compatibility(
        &mut self,
        expr_idx: NodeIndex,
        base_constructor_type: TypeId,
        explicit_type_args: Option<&tsz_parser::parser::NodeList>,
    ) {
        if self.heritage_call_has_invalid_mixin_constructor_constraint(expr_idx) {
            self.error_at_node(
                expr_idx,
                crate::diagnostics::diagnostic_messages::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
                crate::diagnostics::diagnostic_codes::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
            );
            return;
        }

        let mut signatures = Vec::new();
        self.collect_heritage_call_expression_constructor_signatures(
            base_constructor_type,
            &mut signatures,
            &mut rustc_hash::FxHashSet::default(),
        );
        if signatures.is_empty() {
            return;
        }

        let type_args_nodes: &[NodeIndex] = explicit_type_args
            .map(|args| args.nodes.as_slice())
            .unwrap_or(&[]);
        let provided_count = type_args_nodes.len();

        let provided_types: Vec<TypeId> = type_args_nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        let matching: Vec<&tsz_solver::CallSignature> = signatures
            .iter()
            .filter(|sig| {
                let max = sig.type_params.len();
                let min = sig
                    .type_params
                    .iter()
                    .filter(|tp| tp.default.is_none())
                    .count();
                provided_count >= min && provided_count <= max
            })
            .collect();

        if provided_count > 0 && matching.is_empty() {
            let anchor =
                self.find_heritage_call_expression_type_argument_anchor(expr_idx, type_args_nodes);
            let message = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::NO_BASE_CONSTRUCTOR_HAS_THE_SPECIFIED_NUMBER_OF_TYPE_ARGUMENTS,
                &[],
            );
            self.error(
                anchor,
                1,
                message,
                crate::diagnostics::diagnostic_codes::NO_BASE_CONSTRUCTOR_HAS_THE_SPECIFIED_NUMBER_OF_TYPE_ARGUMENTS,
            );
            return;
        }

        if matching.is_empty() {
            return;
        }

        // When the base constructor type is an intersection (e.g., mixin patterns
        // like `T & (new (...args) => Mixin)`), constructor signatures come from
        // different intersection members and naturally have different return types.
        // tsc doesn't compare return types across intersection members. Invalid
        // mixin constructor constraints are handled by the explicit check above.
        let has_intersection_instance =
            crate::query_boundaries::flow_analysis::instance_type_from_constructor(
                self.ctx.types,
                base_constructor_type,
            )
            .is_some_and(|instance_type| {
                crate::query_boundaries::common::intersection_members(self.ctx.types, instance_type)
                    .is_some()
            });
        let has_prototype_property = crate::query_boundaries::common::has_property_by_str(
            self.ctx.types,
            base_constructor_type,
            "prototype",
        );
        if crate::query_boundaries::common::is_intersection_type(
            self.ctx.types,
            base_constructor_type,
        ) || has_intersection_instance
            || has_prototype_property
        {
            return;
        }

        let mut return_types = Vec::with_capacity(matching.len());
        for sig in matching {
            let mut args = provided_types.clone();
            if args.len() < sig.type_params.len() {
                for param in sig.type_params.iter().skip(args.len()) {
                    let fallback = param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN);
                    args.push(fallback);
                }
            }
            if args.len() > sig.type_params.len() {
                args.truncate(sig.type_params.len());
            }
            let instantiated = self.instantiate_signature(sig, &args);
            return_types.push(instantiated.return_type);
        }

        let Some((first_return, rest)) = return_types.split_first() else {
            return;
        };
        for &candidate_return in rest {
            if !self.are_mutually_assignable(*first_return, candidate_return)
                || !self.are_mutually_assignable(candidate_return, *first_return)
            {
                self.error_at_node(
                expr_idx,
                crate::diagnostics::diagnostic_messages::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
                crate::diagnostics::diagnostic_codes::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
            );
                return;
            }
        }
    }

    fn collect_heritage_call_expression_constructor_signatures(
        &self,
        type_id: TypeId,
        signatures: &mut Vec<tsz_solver::CallSignature>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        if !visited.insert(type_id) {
            return;
        }

        if let Some(sigs) = class_query::construct_signatures_for_type(self.ctx.types, type_id) {
            signatures.extend(sigs);
        }

        if let Some(members) = class_query::intersection_members(self.ctx.types, type_id) {
            for member in &members {
                self.collect_heritage_call_expression_constructor_signatures(
                    *member, signatures, visited,
                );
            }
        }
    }

    fn find_heritage_call_expression_type_argument_anchor(
        &self,
        expr_idx: NodeIndex,
        type_arg_nodes: &[NodeIndex],
    ) -> u32 {
        let (call_expr_start, _) = self.get_node_span(expr_idx).unwrap_or((0, 0));
        let explicit_start = type_arg_nodes
            .first()
            .and_then(|&arg| self.get_node_span(arg).map(|(start, _)| start));

        find_heritage_call_expression_type_argument_anchor_impl(
            call_expr_start,
            explicit_start,
            call_expr_start,
        )
    }
}

const fn find_heritage_call_expression_type_argument_anchor_impl(
    call_expr_start: u32,
    explicit_type_arg_start: Option<u32>,
    fallback_start: u32,
) -> u32 {
    if explicit_type_arg_start.is_some() {
        call_expr_start
    } else {
        fallback_start
    }
}

#[cfg(test)]
mod tests {
    use super::find_heritage_call_expression_type_argument_anchor_impl;

    #[test]
    fn test_prefers_explicit_type_argument_node_start() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, Some(23), 5);
        assert_eq!(anchor, 15);
    }

    #[test]
    fn test_falls_back_to_call_start_when_source_text_missing() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(26, Some(2), 5);
        assert_eq!(anchor, 26);
    }

    #[test]
    fn test_falls_back_to_call_start_without_type_arguments() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, None, 7);
        assert_eq!(anchor, 7);
    }
}
