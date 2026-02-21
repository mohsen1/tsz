//! Type checking query helpers: type parameter scope, function implementation
//! checking, class member analysis, and library interface heritage merging.

use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::is_compiler_managed_type;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Section 39: Type Parameter Scope Utilities
    // =========================================================================

    /// Pop type parameters from scope, restoring previous values.
    /// Used to restore the type parameter scope after exiting a generic context.
    pub(crate) fn pop_type_parameters(&mut self, updates: Vec<(String, Option<TypeId>, bool)>) {
        for (name, previous, shadowed_class_param) in updates.into_iter().rev() {
            if let Some(prev_type) = previous {
                self.ctx
                    .type_parameter_scope
                    .insert(name.clone(), prev_type);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
            if shadowed_class_param && let Some(ref mut c) = self.ctx.enclosing_class {
                c.type_param_names.push(name);
            }
        }
    }

    /// Push parameter names into `typeof_param_scope` so that `typeof paramName`
    /// in return type annotations can resolve to the parameter's declared type.
    pub(crate) fn push_typeof_param_scope(&mut self, params: &[tsz_solver::ParamInfo]) {
        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.insert(name, param.type_id);
            }
        }
    }

    /// Remove parameter names from `typeof_param_scope` after return type resolution.
    pub(crate) fn pop_typeof_param_scope(&mut self, params: &[tsz_solver::ParamInfo]) {
        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.remove(&name);
            }
        }
    }

    /// Check for unused type parameters in a declaration and emit TS6133.
    ///
    /// This scans all identifiers within the declaration body for type parameter
    /// name references. Any type parameter that is not referenced gets a TS6133
    /// diagnostic. Called only from the checking path (not type resolution).
    pub(crate) fn check_unused_type_params(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
        body_root: NodeIndex,
    ) {
        use tsz_scanner::SyntaxKind;

        // Type parameters are checked under noUnusedParameters, not noUnusedLocals.
        // See: unusedTypeParametersNotCheckedByNoUnusedLocals conformance test.
        if !self.ctx.no_unused_parameters() {
            return;
        }

        let Some(list) = type_parameters else {
            return;
        };

        // Collect type parameter names and their declaration name NodeIndices
        let mut params: Vec<(String, NodeIndex)> = Vec::new();
        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };
            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|id_data| id_data.escaped_text.clone())
                .unwrap_or_default();
            if !name.is_empty() && !name.starts_with('_') {
                params.push((name, data.name));
            }
        }

        if params.is_empty() {
            return;
        }

        let Some(root_node) = self.ctx.arena.get(body_root) else {
            return;
        };
        let mut pos_start = root_node.pos;
        let mut pos_end = root_node.end;

        // For merged declarations (e.g., class + interface with same name),
        // check type parameter usage across ALL declarations, not just the current one.
        // This prevents false positives like "class C<T> {} interface C<T> { a: T }".
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(body_root)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            // If there are multiple declarations, expand the range to include all
            if symbol.declarations.len() > 1 {
                for &decl_idx in &symbol.declarations {
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                        pos_start = pos_start.min(decl_node.pos);
                        pos_end = pos_end.max(decl_node.end);
                    }
                }
            }
        }

        let decl_indices: Vec<NodeIndex> = params.iter().map(|(_, idx)| *idx).collect();
        let mut used = vec![false; params.len()];

        // Scan all nodes in the arena for identifiers within the declaration range
        let arena_len = self.ctx.arena.len();
        for i in 0..arena_len {
            let idx = NodeIndex(i as u32);
            // Skip the type parameter declaration identifiers themselves
            if decl_indices.contains(&idx) {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.pos < pos_start || node.end > pos_end {
                continue;
            }
            if node.kind == SyntaxKind::Identifier as u16
                && let Some(ident) = self.ctx.arena.get_identifier(node)
            {
                let name_str = ident.escaped_text.as_str();
                for (j, (param_name, _)) in params.iter().enumerate() {
                    if !used[j] && param_name == name_str {
                        used[j] = true;
                    }
                }
            }
        }

        // Emit TS6133 for unused type parameters
        let file_name = self.ctx.file_name.clone();
        for (j, (name, decl_idx)) in params.iter().enumerate() {
            if used[j] {
                continue;
            }
            if let Some(name_node) = self.ctx.arena.get(*decl_idx) {
                // Match tsc: TS6133 on unused type parameters anchors at the '<' token.
                let start = name_node.pos.saturating_sub(1);
                let length = name_node.end.saturating_sub(name_node.pos);
                self.ctx.push_diagnostic(crate::diagnostics::Diagnostic {
                    file: file_name.clone(),
                    start,
                    length,
                    message_text: format!("'{name}' is declared but its value is never read."),
                    category: crate::diagnostics::DiagnosticCategory::Error,
                    code: 6133,
                    related_information: Vec::new(),
                });
            }
        }
    }

    /// Collect all `infer` type parameter names from a type node.
    /// This is used to add inferred type parameters to the scope when checking conditional types.
    pub(crate) fn collect_infer_type_parameters(&self, type_idx: NodeIndex) -> Vec<String> {
        let mut params = Vec::new();
        self.collect_infer_type_parameters_inner(type_idx, &mut params);
        params
    }

    /// Inner implementation for collecting infer type parameters.
    /// Recursively walks the type node to find all infer type parameter names.
    fn collect_infer_type_parameters_inner(&self, type_idx: NodeIndex, params: &mut Vec<String>) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node)
                    && let Some(param_node) = self.ctx.arena.get(infer.type_parameter)
                    && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                    && let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    let name = ident.escaped_text.clone();
                    if !params.contains(&name) {
                        params.push(name);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(ref args) = type_ref.type_arguments
                {
                    for &arg_idx in &args.nodes {
                        self.collect_infer_type_parameters_inner(arg_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.collect_infer_type_parameters_inner(member_idx, params);
                    }
                }
            }
            // Function and Constructor Types: check parameters and return type
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    // Check type parameters (they may have infer in constraints)
                    if let Some(ref tps) = func_type.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_type_parameters_inner(tp_idx, params);
                        }
                    }
                    // Check parameters
                    for &param_idx in &func_type.parameters.nodes {
                        self.collect_infer_type_parameters_inner(param_idx, params);
                    }
                    // Check return type
                    if func_type.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(func_type.type_annotation, params);
                    }
                }
            }
            // Array Types: check element type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                    self.collect_infer_type_parameters_inner(array_type.element_type, params);
                }
            }
            // Tuple Types: check all elements
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple_type) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple_type.elements.nodes {
                        self.collect_infer_type_parameters_inner(elem_idx, params);
                    }
                }
            }
            // Type Literals (Object types): check all members
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.collect_infer_type_parameters_inner(member_idx, params);
                    }
                }
            }
            // Type Operators: keyof, readonly, unique - check operand
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.collect_infer_type_parameters_inner(op.type_node, params);
                }
            }
            // Indexed Access Types: T[K] - check both object and index
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.collect_infer_type_parameters_inner(indexed.object_type, params);
                    self.collect_infer_type_parameters_inner(indexed.index_type, params);
                }
            }
            // Mapped Types: check type parameter (constraint) and type template
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.collect_infer_type_parameters_inner(mapped.type_parameter, params);
                    if mapped.type_node.is_some() {
                        self.collect_infer_type_parameters_inner(mapped.type_node, params);
                    }
                    if mapped.name_type.is_some() {
                        self.collect_infer_type_parameters_inner(mapped.name_type, params);
                    }
                }
            }
            // Conditional Types: check check_type, extends_type, true_type, false_type
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    self.collect_infer_type_parameters_inner(cond.check_type, params);
                    self.collect_infer_type_parameters_inner(cond.extends_type, params);
                    self.collect_infer_type_parameters_inner(cond.true_type, params);
                    self.collect_infer_type_parameters_inner(cond.false_type, params);
                }
            }
            // Template Literal Types: check all type spans
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        self.collect_infer_type_parameters_inner(span_idx, params);
                    }
                }
            }
            // Template Literal Type Spans: recurse into the type expression
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN => {
                if let Some(span) = self.ctx.arena.get_template_span(node) {
                    self.collect_infer_type_parameters_inner(span.expression, params);
                }
            }
            // Parenthesized Types: unwrap and check inner type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_infer_type_parameters_inner(wrapped.expression, params);
                }
            }
            // Optional, Rest Types: unwrap and check inner type
            k if k == syntax_kind_ext::OPTIONAL_TYPE || k == syntax_kind_ext::REST_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.collect_infer_type_parameters_inner(wrapped.type_node, params);
                }
            }
            // Named Tuple Members: check the type annotation
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(member) = self.ctx.arena.get_named_tuple_member(node) {
                    self.collect_infer_type_parameters_inner(member.type_node, params);
                }
            }
            // Type Parameters: check constraint and default for nested infer
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                if let Some(type_param) = self.ctx.arena.get_type_parameter(node) {
                    // Check constraint: <T extends infer U>
                    if type_param.constraint != NodeIndex::NONE {
                        self.collect_infer_type_parameters_inner(type_param.constraint, params);
                    }
                    // Check default: <T = infer U>
                    if type_param.default != NodeIndex::NONE {
                        self.collect_infer_type_parameters_inner(type_param.default, params);
                    }
                }
            }
            _ => {
                // Signatures (PropertySignature, MethodSignature, CallSignature, ConstructSignature):
                // recurse into type parameters, parameters, and return type
                if let Some(sig) = self.ctx.arena.get_signature(node) {
                    if let Some(ref tps) = sig.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_type_parameters_inner(tp_idx, params);
                        }
                    }
                    if let Some(ref sig_params) = sig.parameters {
                        for &param_idx in &sig_params.nodes {
                            self.collect_infer_type_parameters_inner(param_idx, params);
                        }
                    }
                    if sig.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(sig.type_annotation, params);
                    }
                } else if let Some(index_sig) = self.ctx.arena.get_index_signature(node) {
                    // IndexSignature: recurse into parameters and type annotation
                    for &param_idx in &index_sig.parameters.nodes {
                        self.collect_infer_type_parameters_inner(param_idx, params);
                    }
                    if index_sig.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(index_sig.type_annotation, params);
                    }
                } else if let Some(param) = self.ctx.arena.get_parameter(node) {
                    // Parameters: check the type annotation
                    if param.type_annotation != NodeIndex::NONE {
                        self.collect_infer_type_parameters_inner(param.type_annotation, params);
                    }
                }
            }
        }
    }

    // Section 40: Node and Name Utilities
    // ------------------------------------

    /// Get the text content of a node from the source file.
    pub(crate) fn node_text(&self, node_idx: NodeIndex) -> Option<String> {
        let (start, end) = self.get_node_span(node_idx)?;
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let start = start as usize;
        let end = end as usize;
        if start >= end || end > source.len() {
            return None;
        }
        Some(source[start..end].to_string())
    }

    /// Get the name of a parameter for error messages.
    pub(crate) fn parameter_name_for_error(&self, name_idx: NodeIndex) -> String {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return "this".to_string();
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                return lit.text.clone();
            }
        }

        self.node_text(name_idx)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "parameter".to_string())
    }

    /// Get the name of a property for error messages.
    pub(crate) fn property_name_for_error(&self, name_idx: NodeIndex) -> Option<String> {
        self.get_property_name(name_idx).or_else(|| {
            self.node_text(name_idx)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
        })
    }

    /// Collect all nodes within an initializer expression that reference a given name.
    /// Used for TS2372: parameter cannot reference itself.
    ///
    /// Recursively walks the initializer AST and collects every identifier node
    /// that matches `name`. Stops recursion at scope boundaries (function expressions,
    /// arrow functions, class expressions) since those introduce new scopes where
    /// the identifier would not be a self-reference of the outer parameter.
    ///
    /// Returns a list of `NodeIndex` values, one for each self-referencing identifier.
    /// TSC emits a separate TS2372 error for each occurrence.
    pub(crate) fn collect_self_references(
        &self,
        init_idx: NodeIndex,
        name: &str,
    ) -> Vec<NodeIndex> {
        let mut refs = Vec::new();
        self.collect_self_references_recursive(init_idx, name, &mut refs);
        refs
    }

    /// Collect property-access occurrences whose property name matches `name`.
    ///
    /// This is used for accessor recursion detection (TS7023). It intentionally
    /// ignores bare identifiers so captured outer variables like `return x` in
    /// `get x() { ... }` are not treated as self-recursive references.
    pub(crate) fn collect_property_name_references(
        &self,
        init_idx: NodeIndex,
        name: &str,
    ) -> Vec<NodeIndex> {
        let mut refs = Vec::new();
        self.collect_property_name_references_recursive(init_idx, name, &mut refs);
        refs
    }

    /// Recursive helper for `collect_self_references`.
    fn collect_self_references_recursive(
        &self,
        node_idx: NodeIndex,
        name: &str,
        refs: &mut Vec<NodeIndex>,
    ) {
        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // If this node is an identifier matching the parameter name, record it
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            if ident.escaped_text == name {
                refs.push(node_idx);
            }
            return;
        }

        // Stop at scope boundaries: function expressions, arrow functions,
        // and class expressions introduce new scopes where the name would
        // refer to something different (not the outer parameter).
        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => {
                return;
            }
            _ => {}
        }

        // Recurse into all children of this node
        let children = self.ctx.arena.get_children(node_idx);
        for child_idx in children {
            self.collect_self_references_recursive(child_idx, name, refs);
        }
    }

    /// Recursive helper for `collect_property_name_references`.
    fn collect_property_name_references_recursive(
        &self,
        node_idx: NodeIndex,
        name: &str,
        refs: &mut Vec<NodeIndex>,
    ) {
        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text == name
        {
            refs.push(access.name_or_argument);
        }

        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => {
                return;
            }
            _ => {}
        }

        let children = self.ctx.arena.get_children(node_idx);
        for child_idx in children {
            self.collect_property_name_references_recursive(child_idx, name, refs);
        }
    }

    // Section 41: Function Implementation Checking
    // --------------------------------------------

    /// Infer the return type of a getter from its body.
    pub(crate) fn infer_getter_return_type(&mut self, body_idx: NodeIndex) -> TypeId {
        self.infer_return_type_from_body(tsz_parser::parser::NodeIndex::NONE, body_idx, None)
    }

    /// Check that all top-level function overload signatures have implementations.
    /// Reports errors 2389, 2391.
    pub(crate) fn check_function_implementations(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < statements.len() {
            let stmt_idx = statements[i];
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                i += 1;
                continue;
            };

            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && let Some(func) = self.ctx.arena.get_function(node)
                && func.body.is_none()
            {
                // Suppress TS2391 when a parse error occurs within the function declaration span.
                // When `body.is_none()` and there are parse errors within the function span,
                // the function was likely malformed (e.g. `function f() => 4;`).
                // This doesn't affect cases like `function f(a {` because the parser gives
                // those a body (`body_none=false`) so they never reach this path.
                if self.has_syntax_parse_errors() {
                    let fn_start = node.pos;
                    let fn_end = node.end;
                    let has_error_in_fn = self
                        .ctx
                        .syntax_parse_error_positions
                        .iter()
                        .any(|&p| p >= fn_start && p <= fn_end);
                    if has_error_in_fn {
                        i += 1;
                        continue;
                    }
                }
                let is_declared = self.is_ambient_declaration(stmt_idx);
                // Use func.is_async as the parser stores async as a flag, not a modifier
                let is_async = func.is_async;
                // TSC reports TS2389/TS2391 at the function name, not the declaration.
                let name_node = func.name;
                let error_node = if name_node.is_some() {
                    name_node
                } else {
                    stmt_idx
                };

                // TS1040: 'async' modifier cannot be used in an ambient context
                if is_declared && is_async {
                    self.error_at_node(
                        stmt_idx,
                        "'async' modifier cannot be used in an ambient context.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                    );
                    i += 1;
                    continue;
                }

                if is_declared {
                    if let Some(name) = self.get_function_name_from_node(stmt_idx) {
                        let (has_impl, impl_name, impl_idx) =
                            self.find_function_impl(statements, i + 1, &name);
                        if has_impl
                            && impl_name.as_deref() == Some(name.as_str())
                            && let Some(impl_idx) = impl_idx
                            && !self.is_ambient_declaration(impl_idx)
                        {
                            self.error_at_node(
                                error_node,
                                crate::diagnostics::diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                                crate::diagnostics::diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                            );
                        }
                    }
                    i += 1;
                    continue;
                }
                if is_async {
                    i += 1;
                    continue;
                }
                // Function overload signature - check for implementation
                let func_name = self.get_function_name_from_node(stmt_idx);
                if let Some(name) = func_name {
                    let (has_impl, impl_name, impl_idx) =
                        self.find_function_impl(statements, i + 1, &name);
                    if !has_impl {
                        self.error_at_node(
                                    error_node,
                                    "Function implementation is missing or not immediately following the declaration.",
                                    diagnostic_codes::FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION
                                );
                    } else if let Some(impl_idx) = impl_idx {
                        if let Some(actual_name) = impl_name
                            && actual_name != name
                        {
                            // Implementation has wrong name — report at the implementation name.
                            let impl_error_node = self
                                .ctx
                                .arena
                                .get(impl_idx)
                                .and_then(|n| self.ctx.arena.get_function(n))
                                .map(|f| f.name)
                                .filter(|n| n.is_some())
                                .unwrap_or(impl_idx);
                            self.error_at_node(
                                impl_error_node,
                                &format!("Function implementation name must be '{name}'."),
                                diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                            );
                        } else {
                            let impl_is_declared = self.is_ambient_declaration(impl_idx);
                            if is_declared != impl_is_declared {
                                self.error_at_node(
                                    error_node,
                                    crate::diagnostics::diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                                    crate::diagnostics::diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                                );
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }

    // Section 42: Class Member Utilities
    // ------------------------------------

    /// Check if a class member is static.
    pub(crate) fn class_member_is_static(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .is_some_and(|prop| self.has_static_modifier(&prop.modifiers)),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|method| self.has_static_modifier(&method.modifiers)),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .is_some_and(|accessor| self.has_static_modifier(&accessor.modifiers)),
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => true,
            _ => false,
        }
    }

    /// Get the declaring type for a private member.
    pub(crate) fn private_member_declaring_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if !matches!(
                node.kind,
                k if k == syntax_kind_ext::PROPERTY_DECLARATION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
            ) {
                continue;
            }

            let Some(ext) = self.ctx.arena.get_extended(decl_idx) else {
                continue;
            };
            if ext.parent.is_none() {
                continue;
            }
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                continue;
            };
            if parent_node.kind != syntax_kind_ext::CLASS_DECLARATION
                && parent_node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            let Some(class) = self.ctx.arena.get_class(parent_node) else {
                continue;
            };
            let is_static = self.class_member_is_static(decl_idx);
            return Some(if is_static {
                self.get_class_constructor_type(ext.parent, class)
            } else {
                self.get_class_instance_type(ext.parent, class)
            });
        }

        None
    }

    /// Check if a type annotation node is a simple type reference to a given class.
    /// Returns true if the type annotation is a `TypeReference` to the class by name.
    fn type_annotation_refers_to_current_class(
        &self,
        type_annotation_idx: NodeIndex,
        class_idx: NodeIndex,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_annotation_idx) else {
            return false;
        };

        // Check if it's a type reference
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }

        let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
            return false;
        };

        // Get the name from the type reference
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };

        let type_ref_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            &ident.escaped_text
        } else {
            return false;
        };

        // Get the class name
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };

        let Some(class) = self.ctx.arena.get_class(class_node) else {
            return false;
        };

        if class.name.is_none() {
            return false;
        }

        let Some(class_name_node) = self.ctx.arena.get(class.name) else {
            return false;
        };

        let class_name = if let Some(ident) = self.ctx.arena.get_identifier(class_name_node) {
            &ident.escaped_text
        } else {
            return false;
        };

        // Compare names
        type_ref_name == class_name
    }

    /// Get the type annotation of an explicit `this` parameter if present.
    /// Returns `Some(type_annotation_idx)` if the first parameter is named "this" with a type annotation.
    /// Returns None otherwise.
    fn get_explicit_this_type_annotation(&self, params: &[NodeIndex]) -> Option<NodeIndex> {
        let first_param_idx = params.first().copied()?;
        let param_node = self.ctx.arena.get(first_param_idx)?;
        let param = self.ctx.arena.get_parameter(param_node)?;

        // Check if parameter name is "this"
        // Must check both ThisKeyword and Identifier("this") to match parser behavior
        let is_this = if let Some(name_node) = self.ctx.arena.get(param.name) {
            if name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
                true
            } else if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text == "this"
            } else {
                false
            }
        } else {
            false
        };

        // Explicit `this` parameter must have a type annotation
        (is_this && param.type_annotation.is_some()).then_some(param.type_annotation)
    }

    /// Get the this type for a class member.
    pub(crate) fn class_member_this_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let class_idx = class_info.class_idx;
        let cached_instance_this = class_info.cached_instance_this_type;
        let is_static = self.class_member_is_static(member_idx);

        // Check if this method/accessor has an explicit `this` parameter.
        // If so, extract and return its type instead of the default class type.
        if let Some(node) = self.ctx.arena.get(member_idx) {
            let explicit_this_type_annotation = match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node) {
                        self.get_explicit_this_type_annotation(&method.parameters.nodes)
                    } else {
                        None
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                        self.get_explicit_this_type_annotation(&accessor.parameters.nodes)
                    } else {
                        None
                    }
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                        self.get_explicit_this_type_annotation(&accessor.parameters.nodes)
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(type_annotation_idx) = explicit_this_type_annotation {
                // Check if the explicit `this` type refers to the current class.
                // If so, we should use the cached instance type to avoid resolution timing issues.
                let refers_to_current_class =
                    self.type_annotation_refers_to_current_class(type_annotation_idx, class_idx);

                if refers_to_current_class && !is_static {
                    // For instance methods with `this: CurrentClass`, use the cached instance type
                    // This ensures we get the fully-constructed class type with all properties
                    if let Some(cached) = cached_instance_this {
                        return Some(cached);
                    }
                    if let Some(node) = self.ctx.arena.get(class_idx)
                        && let Some(class) = self.ctx.arena.get_class(node)
                    {
                        return Some(self.get_class_instance_type(class_idx, class));
                    }
                }

                // Otherwise, resolve the explicit type normally
                let explicit_this_type = self.get_type_from_type_node(type_annotation_idx);
                return Some(explicit_this_type);
            }
        }

        if !is_static {
            if let Some(cached) = cached_instance_this {
                return Some(cached);
            }

            // Use the current class type parameters in scope for instance `this`.
            if let Some(node) = self.ctx.arena.get(class_idx)
                && let Some(class) = self.ctx.arena.get_class(node)
            {
                let this_type = self.get_class_instance_type(class_idx, class);
                if let Some(info) = self.ctx.enclosing_class.as_mut()
                    && info.class_idx == class_idx
                {
                    info.cached_instance_this_type = Some(this_type);
                }
                return Some(this_type);
            }
        }

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(class_idx) {
            if is_static {
                return Some(self.get_type_of_symbol(sym_id));
            }
            return self.class_instance_type_from_symbol(sym_id);
        }

        let class = self.ctx.arena.get_class_at(class_idx)?;
        Some(if is_static {
            self.get_class_constructor_type(class_idx, class)
        } else {
            self.get_class_instance_type(class_idx, class)
        })
    }

    // Section 43: Accessor Type Checking
    // -----------------------------------

    /// Recursively check for TS7006 in nested function/arrow expressions within a node.
    /// This handles cases like `async function foo(a = x => x)` where the nested arrow function
    /// parameter `x` should trigger TS7006 if it lacks a type annotation.
    pub(crate) fn check_for_nested_function_ts7006(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Check if this is a function or arrow expression
        let is_function = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => true,
            k if k == syntax_kind_ext::ARROW_FUNCTION => true,
            _ => false,
        };

        if is_function {
            // Check all parameters of this function for TS7006
            if let Some(func) = self.ctx.arena.get_function(node) {
                for (pi, &param_idx) in func.parameters.nodes.iter().enumerate() {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        // Nested functions in default values don't have contextual types
                        self.maybe_report_implicit_any_parameter(param, false, pi);
                    }
                }
            }

            // Recursively check the function body for more nested functions
            if let Some(func) = self.ctx.arena.get_function(node)
                && func.body.is_some()
            {
                self.check_for_nested_function_ts7006(func.body);
            }
        } else {
            // Recursively check child nodes for function expressions
            match node.kind {
                // Binary expressions - check both sides
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                        self.check_for_nested_function_ts7006(bin_expr.left);
                        self.check_for_nested_function_ts7006(bin_expr.right);
                    }
                }
                // Conditional expressions - check condition, then/else branches
                k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                    if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                        self.check_for_nested_function_ts7006(cond.condition);
                        self.check_for_nested_function_ts7006(cond.when_true);
                        if cond.when_false.is_some() {
                            self.check_for_nested_function_ts7006(cond.when_false);
                        }
                    }
                }
                // Call expressions - only check the callee, NOT arguments.
                // Arguments to call expressions get proper contextual types from
                // the call resolution path (collect_call_argument_types_with_context),
                // so arrow/function expressions in arguments will have their TS7006
                // correctly suppressed by the contextual type. Walking arguments here
                // would emit false TS7006 before contextual typing has a chance to run.
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call) = self.ctx.arena.get_call_expr(node) {
                        self.check_for_nested_function_ts7006(call.expression);
                    }
                }
                // New expressions - same treatment: only check the callee, skip arguments
                // since constructor resolution provides contextual types for arguments.
                k if k == syntax_kind_ext::NEW_EXPRESSION => {
                    if let Some(new_expr) = self.ctx.arena.get_call_expr(node) {
                        self.check_for_nested_function_ts7006(new_expr.expression);
                    }
                }
                // Parenthesized expression - check contents
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                        self.check_for_nested_function_ts7006(paren.expression);
                    }
                }
                // Type assertion - check expression
                k if k == syntax_kind_ext::TYPE_ASSERTION => {
                    if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                        self.check_for_nested_function_ts7006(assertion.expression);
                    }
                }
                // Spread element - check expression
                k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                    if let Some(spread) = self.ctx.arena.get_spread(node) {
                        self.check_for_nested_function_ts7006(spread.expression);
                    }
                }
                _ => {
                    // For other node types, we don't recursively check
                    // This covers literals, identifiers, array/object literals, etc.
                }
            }
        }
    }

    // Section 45: Symbol Resolution Utilities
    // ----------------------------------------

    /// Resolve a library type by name from lib.d.ts and other library contexts.
    ///
    /// This function resolves types from library definition files like lib.d.ts,
    /// es2015.d.ts, etc., which provide built-in JavaScript types and DOM APIs.
    ///
    /// ## Library Contexts:
    /// - Searches through loaded library contexts (lib.d.ts, es2015.d.ts, etc.)
    /// - Each lib context has its own binder and arena
    /// - Types are "lowered" from lib arena to main arena
    ///
    /// ## Declaration Merging:
    /// - Interfaces can have multiple declarations that are merged
    /// - All declarations are lowered together to create merged type
    /// - Essential for types like `Array` which have multiple lib declarations
    ///
    /// ## Global Augmentations:
    /// - User's `declare global` blocks are merged with lib types
    /// - Allows extending built-in types like `Window`, `String`, etc.
    ///
    /// ## Examples:
    /// ```typescript
    /// // Built-in types from lib.d.ts
    /// let arr: Array<number>;  // resolve_lib_type_by_name("Array")
    /// let obj: Object;         // resolve_lib_type_by_name("Object")
    /// let prom: Promise<string>; // resolve_lib_type_by_name("Promise")
    ///
    /// // Global augmentation
    /// declare global {
    ///   interface Window {
    ///     myCustomProperty: string;
    ///   }
    /// }
    /// // lib Window type is merged with augmentation
    /// ```
    /// Merge base interface members into a lib interface type by walking
    /// heritage (`extends`) clauses in declaration-specific arenas.
    ///
    /// This is needed because `merge_interface_heritage_types` uses `self.ctx.arena`
    /// (the user file arena) and cannot read lib declarations that live in lib arenas.
    /// Takes the interface name and looks up declarations from the binder.
    pub(crate) fn merge_lib_interface_heritage(
        &mut self,
        mut derived_type: TypeId,
        name: &str,
    ) -> TypeId {
        use tsz_parser::parser::node::NodeAccess;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // Guard against infinite recursion in recursive generic hierarchies
        // (e.g., interface B<T extends B<T,S>> extends A<B<T,S>, B<T,S>>)
        if !self.ctx.enter_recursion() {
            return derived_type;
        }

        let lib_contexts = self.ctx.lib_contexts.clone();

        // Look up the symbol and its declarations
        let Some(sym_id) = self.ctx.binder.file_locals.get(name) else {
            self.ctx.leave_recursion();
            return derived_type;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            self.ctx.leave_recursion();
            return derived_type;
        };

        let fallback_arena: &NodeArena = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .or_else(|| lib_contexts.first().map(|ctx| ctx.arena.as_ref()))
            .unwrap_or(self.ctx.arena);

        let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = symbol
            .declarations
            .iter()
            .flat_map(|&decl_idx| {
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    arenas
                        .iter()
                        .map(|arc| (decl_idx, arc.as_ref()))
                        .collect::<Vec<_>>()
                } else {
                    vec![(decl_idx, fallback_arena)]
                }
            })
            .collect();

        // Collect base type info: name and type argument node indices with their arena.
        // We collect these first to avoid borrow conflicts during resolution.
        struct HeritageBase<'a> {
            name: String,
            type_arg_indices: Vec<NodeIndex>,
            arena: &'a NodeArena,
        }
        let mut bases: Vec<HeritageBase<'_>> = Vec::new();

        for &(decl_idx, arena) in &decls_with_arenas {
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = arena.get(type_idx) else {
                        continue;
                    };

                    // Extract the base type name and type arguments
                    let (expr_idx, type_arguments) =
                        if let Some(eta) = arena.get_expr_type_args(type_node) {
                            (eta.expression, eta.type_arguments.as_ref())
                        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                            if let Some(tr) = arena.get_type_ref(type_node) {
                                (tr.type_name, tr.type_arguments.as_ref())
                            } else {
                                (type_idx, None)
                            }
                        } else {
                            (type_idx, None)
                        };

                    if let Some(base_name) = arena.get_identifier_text(expr_idx) {
                        let type_arg_indices = type_arguments
                            .map(|args| args.nodes.clone())
                            .unwrap_or_default();
                        bases.push(HeritageBase {
                            name: base_name.to_string(),
                            type_arg_indices,
                            arena,
                        });
                    }
                }
            }
        }

        // Now resolve each base type and merge, applying type argument substitution
        for base in &bases {
            if let Some(mut base_type) = self.resolve_lib_type_by_name(&base.name) {
                // If there are type arguments, resolve them and substitute
                if !base.type_arg_indices.is_empty() {
                    let base_sym = self.ctx.binder.file_locals.get(&base.name);
                    if let Some(base_sym_id) = base_sym {
                        let base_params = self.get_type_params_for_symbol(base_sym_id);
                        if !base_params.is_empty() {
                            let mut type_args = Vec::new();
                            for &arg_idx in &base.type_arg_indices {
                                // Resolve type arguments from the lib arena.
                                // Heritage type args are typically simple type
                                // references (e.g., `string`, `number`).
                                let ty = self.resolve_lib_heritage_type_arg(arg_idx, base.arena);
                                type_args.push(ty);
                            }
                            // Pad/truncate args to match params
                            while type_args.len() < base_params.len() {
                                let param = &base_params[type_args.len()];
                                type_args.push(
                                    param
                                        .default
                                        .or(param.constraint)
                                        .unwrap_or(TypeId::UNKNOWN),
                                );
                            }
                            type_args.truncate(base_params.len());

                            let substitution = tsz_solver::TypeSubstitution::from_args(
                                self.ctx.types,
                                &base_params,
                                &type_args,
                            );
                            base_type = tsz_solver::instantiate_type(
                                self.ctx.types,
                                base_type,
                                &substitution,
                            );
                        }
                    }
                }
                derived_type = self.merge_interface_types(derived_type, base_type);
            }
        }

        self.ctx.leave_recursion();
        derived_type
    }

    /// Resolve a type argument node from a lib arena to a TypeId.
    /// Handles simple keyword types (string, number, etc.), type references
    /// to other lib types, and the derived interface's own type parameters.
    fn resolve_lib_heritage_type_arg(&mut self, node_idx: NodeIndex, arena: &NodeArena) -> TypeId {
        use tsz_parser::parser::node::NodeAccess;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(node) = arena.get(node_idx) else {
            return TypeId::UNKNOWN;
        };

        // Handle keyword types (string, number, boolean, etc.)
        match node.kind {
            k if k == SyntaxKind::StringKeyword as u16 => return TypeId::STRING,
            k if k == SyntaxKind::NumberKeyword as u16 => return TypeId::NUMBER,
            k if k == SyntaxKind::BooleanKeyword as u16 => return TypeId::BOOLEAN,
            k if k == SyntaxKind::VoidKeyword as u16 => return TypeId::VOID,
            k if k == SyntaxKind::UndefinedKeyword as u16 => return TypeId::UNDEFINED,
            k if k == SyntaxKind::NullKeyword as u16 => return TypeId::NULL,
            k if k == SyntaxKind::NeverKeyword as u16 => return TypeId::NEVER,
            k if k == SyntaxKind::UnknownKeyword as u16 => return TypeId::UNKNOWN,
            k if k == SyntaxKind::AnyKeyword as u16 => return TypeId::ANY,
            k if k == SyntaxKind::ObjectKeyword as u16 => return TypeId::OBJECT,
            k if k == SyntaxKind::SymbolKeyword as u16 => return TypeId::SYMBOL,
            k if k == SyntaxKind::BigIntKeyword as u16 => return TypeId::BIGINT,
            _ => {}
        }

        // Handle type references (e.g., other interface names or type params)
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = arena.get_type_ref(node)
            && let Some(name) = arena.get_identifier_text(type_ref.type_name)
        {
            // Check primitive/keyword type names first
            match name {
                "string" => return TypeId::STRING,
                "number" => return TypeId::NUMBER,
                "boolean" => return TypeId::BOOLEAN,
                "void" => return TypeId::VOID,
                "undefined" => return TypeId::UNDEFINED,
                "null" => return TypeId::NULL,
                "never" => return TypeId::NEVER,
                "unknown" => return TypeId::UNKNOWN,
                "any" => return TypeId::ANY,
                "object" => return TypeId::OBJECT,
                "symbol" => return TypeId::SYMBOL,
                "bigint" => return TypeId::BIGINT,
                _ => {}
            }
            // Check type parameter scope
            if let Some(&type_id) = self.ctx.type_parameter_scope.get(name) {
                return type_id;
            }
            // Try to resolve as a lib type
            if let Some(ty) = self.resolve_lib_type_by_name(name) {
                return ty;
            }
        }

        // For identifiers, try resolving the name
        if let Some(name) = arena.get_identifier_text(node_idx) {
            if let Some(&type_id) = self.ctx.type_parameter_scope.get(name) {
                return type_id;
            }
            if let Some(ty) = self.resolve_lib_type_by_name(name) {
                return ty;
            }
        }

        TypeId::UNKNOWN
    }

    pub(crate) fn resolve_lib_type_by_name(&mut self, name: &str) -> Option<TypeId> {
        use tsz_lowering::TypeLowering;
        use tsz_parser::parser::node::NodeAccess;

        tracing::trace!(name, "resolve_lib_type_by_name: called");
        let mut lib_type_id: Option<TypeId> = None;
        let factory = self.ctx.types.factory();

        // Clone lib_contexts to allow access within the resolver closure
        let lib_contexts = self.ctx.lib_contexts.clone();
        // Collect lowered types from the symbol's declarations.
        // The main file's binder already has merged declarations from all lib files.
        let mut lib_types: Vec<TypeId> = Vec::new();

        // CRITICAL: Look up the symbol in the MAIN file's binder (self.ctx.binder),
        // not in lib_ctx.binder. The main file's binder has lib symbols merged with
        // unique SymbolIds via merge_lib_contexts_into_binder during binding.
        // lib_ctx.binder is a SEPARATE merged binder with DIFFERENT SymbolIds.
        // Using lib_ctx.binder's SymbolIds with self.ctx.get_or_create_def_id causes
        // SymbolId collisions and wrong type resolution.
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            // Get the symbol's declaration(s) from the main file's binder
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                // Get the fallback arena from lib_contexts if available, otherwise use main arena
                let fallback_arena: &NodeArena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map(std::convert::AsRef::as_ref)
                    .or_else(|| lib_contexts.first().map(|ctx| ctx.arena.as_ref()))
                    .unwrap_or(self.ctx.arena);

                // Build declaration -> arena pairs using declaration_arenas
                // This is critical for merged interfaces like Array which have
                // declarations in es5.d.ts, es2015.d.ts, etc.
                // Use the MAIN file's binder's declaration_arenas, not lib_ctx.binder.
                let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = symbol
                    .declarations
                    .iter()
                    .flat_map(|&decl_idx| {
                        if let Some(arenas) =
                            self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                        {
                            arenas
                                .iter()
                                .map(|arc| (decl_idx, arc.as_ref()))
                                .collect::<Vec<_>>()
                        } else {
                            vec![(decl_idx, fallback_arena)]
                        }
                    })
                    .collect();

                // Create resolver that looks up names in the MAIN file's binder
                // CRITICAL: Use self.ctx.binder, not lib_contexts binders, to avoid SymbolId collisions
                let binder = &self.ctx.binder;
                let resolver = |node_idx: NodeIndex| -> Option<u32> {
                    // For merged declarations, we need to check the arena for this specific node.
                    // IMPORTANT: NodeIndex values are arena-specific — the same index can refer
                    // to different nodes in different arenas. We must check ALL arenas and only
                    // return a match when the identifier is found in file_locals. Don't break
                    // early on a mismatch since another arena may have the correct identifier
                    // at the same NodeIndex.
                    for (_, arena) in &decls_with_arenas {
                        if let Some(ident_name) = arena.get_identifier_text(node_idx) {
                            if is_compiler_managed_type(ident_name) {
                                continue;
                            }
                            if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                return Some(found_sym.0);
                            }
                            // Don't break - another arena may have a different identifier
                            // at the same NodeIndex that resolves successfully
                        }
                    }
                    // Also try fallback arena
                    if let Some(ident_name) = fallback_arena.get_identifier_text(node_idx) {
                        if is_compiler_managed_type(ident_name) {
                            return None;
                        }
                        if let Some(found_sym) = binder.file_locals.get(ident_name) {
                            return Some(found_sym.0);
                        }
                    }
                    None
                };

                // Create def_id_resolver that converts SymbolIds to DefIds
                // This is required for Phase 4.2 which uses TypeData::Lazy(DefId) everywhere
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                    resolver(node_idx)
                        .map(|sym_id| self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)))
                };

                // Name-based resolver: resolves identifier text directly without NodeIndex.
                // This is the reliable fallback for cross-arena lowering where NodeIndex
                // values from the current arena don't match nodes in the declaration arenas.
                let name_resolver = |name: &str| -> Option<tsz_solver::DefId> {
                    if is_compiler_managed_type(name) {
                        return None;
                    }
                    binder
                        .file_locals
                        .get(name)
                        .map(|sym_id| self.ctx.get_or_create_def_id(sym_id))
                };

                // Create base lowering with the fallback arena and both resolvers
                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &resolver,
                    &def_id_resolver,
                    &|_| None,
                )
                .with_name_def_id_resolver(&name_resolver);

                // Try to lower as interface first (handles declaration merging)
                if !symbol.declarations.is_empty() {
                    // Check if any declaration is a type alias — if so, skip interface
                    // lowering. Type aliases like Record<K,T>, Partial<T>, Pick<T,K>
                    // would incorrectly succeed interface lowering with 0 type params,
                    // preventing the proper type alias path from running.
                    let is_type_alias = (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0;

                    if !is_type_alias {
                        // Use lower_merged_interface_declarations for proper multi-arena support
                        let (ty, params) =
                            lowering.lower_merged_interface_declarations(&decls_with_arenas);

                        // If lowering succeeded (not ERROR), use the result
                        if ty != TypeId::ERROR {
                            // Record type parameters for generic interfaces
                            if !params.is_empty() {
                                // Cache type params for Application expansion
                                let file_sym_id =
                                    self.ctx.binder.file_locals.get(name).unwrap_or(sym_id);
                                let def_id = self.ctx.get_or_create_def_id(file_sym_id);
                                self.ctx.insert_def_type_params(def_id, params);
                            }

                            lib_types.push(ty);
                        }
                    }

                    // Interface lowering skipped or returned ERROR - try as type alias
                    // Type aliases like Partial<T>, Pick<T,K>, Record<K,T> have their
                    // declaration in symbol.declarations but are not interface nodes
                    if lib_types.is_empty() {
                        for (decl_idx, decl_arena) in &decls_with_arenas {
                            if let Some(node) = decl_arena.get(*decl_idx)
                                && let Some(alias) = decl_arena.get_type_alias(node)
                            {
                                let alias_lowering = lowering.with_arena(decl_arena);
                                let (ty, params) =
                                    alias_lowering.lower_type_alias_declaration(alias);
                                if ty != TypeId::ERROR {
                                    // Cache type parameters for Application expansion
                                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                                    self.ctx.insert_def_type_params(def_id, params.clone());

                                    // CRITICAL: Register the type body in TypeEnvironment so that
                                    // evaluate_application can resolve it via resolve_lazy(def_id).
                                    // Without this, Partial<T>, Pick<T,K>, etc. resolve to unknown.
                                    if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                                        env.insert_def_with_params(def_id, ty, params);
                                    }

                                    // CRITICAL: Return Lazy(DefId) instead of the structural body.
                                    // Application types only expand when the base is Lazy, not when
                                    // it's the actual MappedType/Object/etc. This allows evaluate_application
                                    // to trigger and substitute type parameters correctly.
                                    let lazy_type = self.ctx.types.factory().lazy(def_id);
                                    lib_types.push(lazy_type);

                                    // Type aliases don't merge across files, take the first one
                                    break;
                                }
                            }
                        }
                    }
                }

                // For value declarations (vars, consts, functions)
                let decl_idx = symbol.value_declaration;
                if decl_idx.0 != u32::MAX {
                    // Get the correct arena for the value declaration from main binder
                    let value_arena = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .map_or(fallback_arena, |arc| arc.as_ref());
                    let value_lowering = lowering.with_arena(value_arena);
                    let val_type = value_lowering.lower_type(decl_idx);
                    // Only include non-ERROR types. Value declaration lowering can fail
                    // when type references (e.g., `PromiseConstructor`) can't be resolved
                    // during TypeLowering. Including ERROR in the lib_types vector would
                    // cause intersection2 to collapse a valid interface type to ERROR.
                    if val_type != TypeId::ERROR {
                        lib_types.push(val_type);
                    }
                }
            }
        }

        // Merge all found types from different lib files using intersection
        if lib_types.len() == 1 {
            lib_type_id = Some(lib_types[0]);
        } else if lib_types.len() > 1 {
            let mut merged = lib_types[0];
            for &ty in &lib_types[1..] {
                merged = factory.intersection(vec![merged, ty]);
            }
            lib_type_id = Some(merged);
        }

        // Merge heritage (extends) from lib interface declarations.
        // This propagates base interface members (e.g., Iterator.next() into ArrayIterator).
        if let Some(ty) = lib_type_id {
            lib_type_id = Some(self.merge_lib_interface_heritage(ty, name));
        }

        // Check for global augmentations that should merge with this type.
        // Augmentations may come from the current file or other files (cross-file merge).
        if let Some(augmentation_decls) = self.ctx.binder.global_augmentations.get(name)
            && !augmentation_decls.is_empty()
        {
            // Group augmentation declarations by arena.
            // Declarations with arena=None use the current file's arena.
            let current_arena: &NodeArena = self.ctx.arena;
            let binder_ref = self.ctx.binder;

            let binder_for_arena = |arena_ref: &NodeArena| -> Option<&tsz_binder::BinderState> {
                let arenas = self.ctx.all_arenas.as_ref()?;
                let binders = self.ctx.all_binders.as_ref()?;
                let arena_ptr = arena_ref as *const NodeArena;
                for (idx, arena) in arenas.iter().enumerate() {
                    if Arc::as_ptr(arena) == arena_ptr {
                        return binders.get(idx).map(Arc::as_ref);
                    }
                }
                None
            };

            // Collect declarations grouped by arena pointer identity
            let mut current_file_decls: Vec<NodeIndex> = Vec::new();
            let mut cross_file_groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)> =
                FxHashMap::default();

            for aug in augmentation_decls {
                if let Some(ref arena) = aug.arena {
                    let key = Arc::as_ptr(arena) as usize;
                    cross_file_groups
                        .entry(key)
                        .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                        .1
                        .push(aug.node);
                } else {
                    current_file_decls.push(aug.node);
                }
            }

            let resolve_in_scope = |binder: &tsz_binder::BinderState,
                                    arena_ref: &NodeArena,
                                    node_idx: NodeIndex|
             -> Option<u32> {
                let ident_name = arena_ref.get_identifier_text(node_idx)?;
                let mut scope_id = binder.find_enclosing_scope(arena_ref, node_idx)?;
                while scope_id != tsz_binder::ScopeId::NONE {
                    let scope = binder.scopes.get(scope_id.0 as usize)?;
                    if let Some(sym_id) = scope.table.get(ident_name) {
                        return Some(sym_id.0);
                    }
                    scope_id = scope.parent;
                }
                None
            };

            // Helper: lower augmentation declarations using a given arena
            let mut lower_with_arena = |arena_ref: &NodeArena, decls: &[NodeIndex]| {
                let decl_binder = binder_for_arena(arena_ref).unwrap_or(binder_ref);
                let resolver = |node_idx: NodeIndex| -> Option<u32> {
                    if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                        return Some(sym_id.0);
                    }
                    if let Some(sym_id) = resolve_in_scope(decl_binder, arena_ref, node_idx) {
                        return Some(sym_id);
                    }
                    let ident_name = arena_ref.get_identifier_text(node_idx)?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    if let Some(found_sym) = decl_binder.file_locals.get(ident_name) {
                        return Some(found_sym.0);
                    }
                    if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                        for binder in all_binders.iter() {
                            if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                return Some(found_sym.0);
                            }
                        }
                    }
                    if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                        for binder in all_binders.iter() {
                            if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                return Some(found_sym.0);
                            }
                        }
                    }
                    for ctx in &lib_contexts {
                        if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                            return Some(found_sym.0);
                        }
                    }
                    None
                };
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                    if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                        return Some(
                            self.ctx
                                .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                        );
                    }
                    if let Some(sym_id) = resolve_in_scope(decl_binder, arena_ref, node_idx) {
                        return Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)));
                    }
                    let ident_name = arena_ref.get_identifier_text(node_idx)?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    let sym_id = decl_binder.file_locals.get(ident_name).or_else(|| {
                        if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                            for binder in all_binders.iter() {
                                if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                    return Some(found_sym);
                                }
                            }
                        }
                        lib_contexts
                            .iter()
                            .find_map(|ctx| ctx.binder.file_locals.get(ident_name))
                    })?;
                    Some(
                        self.ctx
                            .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                    )
                };
                let lowering = TypeLowering::with_hybrid_resolver(
                    arena_ref,
                    self.ctx.types,
                    &resolver,
                    &def_id_resolver,
                    &|_| None,
                );
                let aug_type = lowering.lower_interface_declarations(decls);
                lib_type_id = if let Some(lib_type) = lib_type_id {
                    Some(factory.intersection(vec![lib_type, aug_type]))
                } else {
                    Some(aug_type)
                };
            };

            // Lower current-file augmentations
            if !current_file_decls.is_empty() {
                lower_with_arena(current_arena, &current_file_decls);
            }

            // Lower cross-file augmentations (each group uses its own arena)
            for (arena, decls) in cross_file_groups.values() {
                lower_with_arena(arena.as_ref(), decls);
            }
        }

        // For generic lib interfaces, we already cached the type params in the
        // interface lowering code above. The type is already correctly lowered
        // and can be returned directly.
        lib_type_id
    }
}
