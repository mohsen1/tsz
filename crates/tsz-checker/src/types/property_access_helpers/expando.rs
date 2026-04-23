//! Expando function/property detection, JS expando reads, and CommonJS export helpers.
//!
//! Covers the property chain resolution, expando assignment detection, cross-file
//! expando type resolution, synthesized array iterator methods, and CommonJS
//! export member name resolution.

use crate::context::is_js_file_name;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn property_access_chain_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        if let Some(text) = arena.identifier_text_owned(idx) {
            return Some(text);
        }
        let node = arena.get(idx)?;
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = arena.get_access_expr(node)?;
            let left = Self::property_access_chain_in_arena(arena, access.expression)?;
            let right = arena
                .get_identifier_at(access.name_or_argument)?
                .escaped_text
                .clone();
            return Some(format!("{left}.{right}"));
        }
        None
    }

    fn expando_assignment_access_key_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let node = arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone()),
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = arena.get_access_expr(node)?;
                let left = Self::expando_assignment_access_key_in_arena(arena, access.expression)?;
                let right = arena.get_identifier_at(access.name_or_argument)?;
                Some(format!("{left}.{}", right.escaped_text))
            }
            _ => None,
        }
    }

    /// Returns `true` if the node at `idx` is a `void 0` expression or the identifier
    /// `undefined`. These are sentinel "uninitialized" markers: tsc does NOT include them
    /// as expando property types (it emits TS2339 instead of TS18048 when such a
    /// property is later read or used in a binary expression).
    fn is_void_zero_or_undefined_rhs_in_arena(arena: &NodeArena, idx: NodeIndex) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };

        // `undefined` identifier
        if node.kind == SyntaxKind::Identifier as u16 {
            return arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "undefined");
        }

        // `void <expr>` — most commonly `void 0`
        if node.kind == syntax_kind_ext::VOID_EXPRESSION
            || node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        {
            let Some(unary) = arena.get_unary_expr(node) else {
                return false;
            };
            return unary.operator == SyntaxKind::VoidKeyword as u16;
        }

        false
    }

    fn root_symbol_for_expando_read(&self, object_expr_idx: NodeIndex) -> Option<SymbolId> {
        self.resolve_identifier_symbol(object_expr_idx)
            .or_else(|| self.resolve_qualified_symbol(object_expr_idx))
    }

    fn expando_read_root_keys(&self, object_expr_idx: NodeIndex) -> Vec<String> {
        let mut keys = Vec::new();

        if let Some(obj_key) = Self::property_access_chain_in_arena(self.ctx.arena, object_expr_idx)
        {
            keys.push(obj_key.clone());
            if let Some((_, last_segment)) = obj_key.rsplit_once('.') {
                keys.push(last_segment.to_string());
            }
        }

        if let Some(sym_id) = self.root_symbol_for_expando_read(object_expr_idx)
            && let Some(symbol) = self.get_cross_file_symbol(sym_id)
        {
            let escaped_name = symbol.escaped_name.to_string();
            if !keys.iter().any(|key| key == &escaped_name) {
                keys.push(escaped_name);
            }
        }

        keys
    }

    fn root_symbol_supports_js_expando_read(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
        else {
            return false;
        };

        if symbol.has_any_flags(
            symbol_flags::FUNCTION
                | symbol_flags::CLASS
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE,
        ) {
            return true;
        }

        if !symbol.has_any_flags(symbol_flags::VARIABLE) {
            return false;
        }

        let decl_idx = symbol.value_declaration;
        let file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(decl_node) = arena.get(decl_idx) else {
            return false;
        };
        let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(init_node) = arena.get(var_decl.initializer) else {
            return false;
        };

        init_node.is_function_expression_or_arrow()
            || init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
    }

    fn expando_root_js_file_idx(&self, object_expr_idx: NodeIndex) -> Option<usize> {
        let sym_id = self.root_symbol_for_expando_read(object_expr_idx)?;
        let file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let file_name = arena
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str())
            .unwrap_or(self.ctx.file_name.as_str());
        (is_js_file_name(file_name) && self.root_symbol_supports_js_expando_read(sym_id))
            .then_some(file_idx)
    }

    pub(in crate::types_domain) fn is_js_prototype_object_literal_expando_write(
        &mut self,
        this_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let owner_idx = match self.this_has_contextual_owner(this_expr_idx) {
            Some(owner_idx) => owner_idx,
            None => return false,
        };
        let owner_node = match self.ctx.arena.get(owner_idx) {
            Some(owner_node) => owner_node,
            None => return false,
        };
        if owner_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let Some(owner_expr) = self.js_prototype_owner_expression_for_node(owner_idx) else {
            return false;
        };
        let Some(owner_target) = self.js_prototype_owner_function_target(owner_expr) else {
            return false;
        };
        let Some(instance_type) = self.js_constructor_body_instance_type_for_function(owner_target)
        else {
            return false;
        };

        !crate::query_boundaries::property_access::type_has_property(
            self.ctx.types,
            instance_type,
            property_name,
        )
    }

    fn source_file_has_expando_assignment(
        arena: &NodeArena,
        idx: NodeIndex,
        expected_key: &str,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && Self::expando_assignment_access_key_in_arena(arena, binary.left)
                .is_some_and(|key| key == expected_key)
            && !Self::is_void_zero_or_undefined_rhs_in_arena(arena, binary.right)
        {
            return true;
        }

        for child_idx in arena.get_children(idx) {
            if Self::source_file_has_expando_assignment(arena, child_idx, expected_key) {
                return true;
            }
        }

        false
    }

    fn js_file_has_expando_assignment_for_keys(
        &self,
        file_idx: usize,
        root_keys: &[String],
        property_name: &str,
    ) -> bool {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };

        root_keys.iter().any(|root_key| {
            let expected_key = format!("{root_key}.{property_name}");
            source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    Self::source_file_has_expando_assignment(arena, stmt_idx, &expected_key)
                })
        })
    }

    fn cross_file_expando_property_read_type(
        &mut self,
        file_idx: usize,
        expected_key: &str,
    ) -> Option<TypeId> {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        let file_name = arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        let mut checker = Box::new(CheckerState::with_parent_cache(
            arena,
            binder,
            self.ctx.types,
            file_name,
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);
        checker.ctx.current_file_idx = file_idx;

        let source_file = arena.source_files.first()?;
        let mut best_match: Option<(u32, TypeId)> = None;
        for &stmt_idx in &source_file.statements.nodes {
            checker.collect_expando_property_assignment_type(
                stmt_idx,
                expected_key,
                u32::MAX,
                &mut best_match,
            );
        }
        best_match.map(|(_, ty)| ty)
    }

    fn js_expando_property_read_type_from_all_files(
        &mut self,
        root_keys: &[String],
        property_name: &str,
        preferred_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let mut file_indices = Vec::new();
        if let Some(file_idx) = preferred_file_idx {
            file_indices.push(file_idx);
        }
        if let Some(all_arenas) = self.ctx.all_arenas.as_ref() {
            for file_idx in 0..all_arenas.len() {
                if !file_indices.contains(&file_idx) {
                    file_indices.push(file_idx);
                }
            }
        } else if !file_indices.contains(&self.ctx.current_file_idx) {
            file_indices.push(self.ctx.current_file_idx);
        }

        for file_idx in file_indices {
            let arena = self.ctx.get_arena_for_file(file_idx as u32);
            let file_name = arena
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
                .unwrap_or(self.ctx.file_name.as_str());
            if !is_js_file_name(file_name) {
                continue;
            }

            for root_key in root_keys {
                let expected_key = format!("{root_key}.{property_name}");
                if !self.js_file_has_expando_assignment_for_keys(
                    file_idx,
                    std::slice::from_ref(root_key),
                    property_name,
                ) {
                    continue;
                }
                if let Some(ty) =
                    self.cross_file_expando_property_read_type(file_idx, &expected_key)
                {
                    return Some(ty);
                }
            }
        }

        None
    }

    pub(in crate::types_domain) fn synthesized_array_iterator_method_type(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        if !matches!(property_name, "values" | "keys" | "entries") {
            return None;
        }

        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, object_type)
                .or_else(|| {
                    crate::query_boundaries::common::get_tuple_element_type_union(
                        self.ctx.types,
                        object_type,
                    )
                })?;

        let iterator_base = self
            .resolve_entity_name_text_to_def_id_for_lowering("ArrayIterator")
            .map(|def_id| self.ctx.types.lazy(def_id))
            .or_else(|| {
                self.resolve_entity_name_text_to_def_id_for_lowering("IterableIterator")
                    .map(|def_id| self.ctx.types.lazy(def_id))
            })?;

        let return_arg = match property_name {
            "values" => element_type,
            "keys" => TypeId::NUMBER,
            "entries" => self.ctx.types.tuple(vec![
                tsz_solver::TupleElement {
                    type_id: TypeId::NUMBER,
                    name: None,
                    optional: false,
                    rest: false,
                },
                tsz_solver::TupleElement {
                    type_id: element_type,
                    name: None,
                    optional: false,
                    rest: false,
                },
            ]),
            _ => return None,
        };

        let return_type = self.ctx.types.application(iterator_base, vec![return_arg]);

        Some(self.ctx.types.function(tsz_solver::FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        }))
    }

    pub(in crate::types_domain) fn prior_js_prototype_object_literal_assignment_node(
        &self,
        prototype_root_expr: NodeIndex,
        read_pos: u32,
    ) -> Option<NodeIndex> {
        let root_key = Self::property_access_chain_in_arena(self.ctx.arena, prototype_root_expr)?;
        let expected_key = format!("{root_key}.prototype");
        let mut latest_match: Option<(u32, NodeIndex)> = None;

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION || node.pos >= read_pos {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if Self::expando_assignment_access_key_in_arena(self.ctx.arena, binary.left).as_deref()
                != Some(expected_key.as_str())
            {
                continue;
            }

            let rhs_idx = self.ctx.arena.skip_parenthesized(binary.right);
            let Some(rhs_node) = self.ctx.arena.get(rhs_idx) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            if latest_match.is_none_or(|(best_pos, _)| node.pos >= best_pos) {
                latest_match = Some((node.pos, rhs_idx));
            }
        }

        latest_match.map(|(_, rhs_idx)| rhs_idx)
    }

    pub(in crate::types_domain) fn prior_js_prototype_object_literal_declares_property(
        &self,
        prototype_root_expr: NodeIndex,
        property_name: &str,
        read_pos: u32,
    ) -> Option<bool> {
        let rhs_idx =
            self.prior_js_prototype_object_literal_assignment_node(prototype_root_expr, read_pos)?;
        let rhs_node = self.ctx.arena.get(rhs_idx)?;
        let obj_lit = self.ctx.arena.get_literal_expr(rhs_node)?;

        Some(obj_lit.elements.nodes.iter().copied().any(|elem_idx| {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };
            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(elem_node)
                    .and_then(|method| self.get_property_name(method.name)),
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                    .ctx
                    .arena
                    .get_accessor(elem_node)
                    .and_then(|accessor| self.get_property_name(accessor.name)),
                _ => None,
            };
            elem_prop_name.is_some_and(|name| name == property_name)
        }))
    }

    /// Check if a property access is an expando function assignment pattern.
    ///
    /// TypeScript allows assigning properties to function and class declarations:
    /// ```typescript
    /// function foo() {}
    /// foo.bar = 1;  // OK - expando pattern, no TS2339
    /// ```
    ///
    /// Returns true if:
    /// 1. The property access is the LHS of a `=` assignment
    /// 2. The object expression is an identifier bound to a function/class declaration,
    ///    or a variable initialized with a function expression / arrow function
    /// 3. The object type is a function type
    pub(in crate::types_domain) fn is_expando_function_assignment(
        &mut self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> bool {
        use crate::query_boundaries::property_access::is_function_type;

        let prototype_root_expr = self.ctx.arena.get(object_expr_idx).and_then(|node| {
            if node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                return None;
            }
            let access = self.ctx.arena.get_access_expr(node)?;
            let name = self.ctx.arena.get(access.name_or_argument)?;
            let ident = self.ctx.arena.get_identifier(name)?;
            (ident.escaped_text == "prototype").then_some(access.expression)
        });

        // Keep the current receiver type as a fast signal, but don't return
        // early on non-function shapes. Checked-JS expando writes can reach
        // this path before the receiver type has stabilized, and the symbol/
        // declaration checks below are the more authoritative source.
        let object_type_is_function = is_function_type(self.ctx.types, object_type);

        // Check if property access is LHS of a `=` assignment
        let parent_idx = match self.ctx.arena.get_extended(property_access_idx) {
            Some(ext) if ext.parent.is_some() => ext.parent,
            _ => return false,
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16
            || binary.left != property_access_idx
        {
            return false;
        }

        // Resolve object symbol for both simple identifiers and qualified chains.
        let symbol_target_expr = prototype_root_expr.unwrap_or(object_expr_idx);
        let sym_id = self
            .resolve_identifier_symbol(symbol_target_expr)
            .or_else(|| self.resolve_qualified_symbol(symbol_target_expr));

        if let Some(sym_id) = sym_id
            && let Some(symbol) = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id))
        {
            let symbol_flags = symbol.flags;
            let symbol_value_declaration = symbol.value_declaration;
            let symbol_declarations = symbol.declarations.clone();
            let symbol_escaped_name = symbol.escaped_name.clone();

            if self.is_js_file()
                && self.ctx.compiler_options.check_js
                && prototype_root_expr.is_none()
                && let Some(root_ident) = self.ctx.arena.get(symbol_target_expr)
                && root_ident.kind == SyntaxKind::Identifier as u16
                && let Some(ident) = self.ctx.arena.get_identifier(root_ident)
                && let Some(non_js_value_type) =
                    self.cross_file_global_value_type_by_name(&ident.escaped_text, false)
                && non_js_value_type != TypeId::ANY
                && non_js_value_type != TypeId::UNKNOWN
                && !is_function_type(self.ctx.types, non_js_value_type)
            {
                return false;
            }

            let prop_name = self
                .ctx
                .arena
                .get(property_access_idx)
                .and_then(|n| self.ctx.arena.get_access_expr(n))
                .and_then(|a| {
                    self.ctx
                        .arena
                        .get(a.name_or_argument)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map(|id| id.escaped_text.clone())
                });

            if let Some(prop_name) = prop_name.as_deref()
                && let Some(prototype_root_expr) = prototype_root_expr
                && let Some(read_pos) = self.ctx.arena.pos_at(property_access_idx)
                && self
                    .prior_js_prototype_object_literal_declares_property(
                        prototype_root_expr,
                        prop_name,
                        read_pos,
                    )
                    .is_some_and(|declares| !declares)
            {
                return false;
            }

            let declaration_is_function_value_in_arena =
                |arena: &tsz_parser::parser::node::NodeArena, decl_idx: NodeIndex| -> bool {
                    if decl_idx.is_none() {
                        return false;
                    }
                    let Some(node) = arena.get(decl_idx) else {
                        return false;
                    };
                    match node.kind {
                        syntax_kind_ext::FUNCTION_DECLARATION => true,
                        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                            let Some(ext) = arena.get_extended(decl_idx) else {
                                return false;
                            };
                            if ext.parent.is_none() {
                                return false;
                            };
                            let parent_idx = ext.parent;
                            let Some(parent_node) = arena.get(parent_idx) else {
                                return false;
                            };
                            let Some(binary) = arena.get_binary_expr(parent_node) else {
                                return false;
                            };
                            if binary.left != decl_idx
                                || !self.is_assignment_operator(binary.operator_token)
                            {
                                return false;
                            }
                            arena
                                .get(binary.right)
                                .is_some_and(|rhs| rhs.is_function_expression_or_arrow())
                        }
                        syntax_kind_ext::BINARY_EXPRESSION => {
                            let Some(binary_node) = arena.get(decl_idx) else {
                                return false;
                            };
                            let Some(binary) = arena.get_binary_expr(binary_node) else {
                                return false;
                            };
                            if !self.is_assignment_operator(binary.operator_token) {
                                return false;
                            }
                            arena
                                .get(binary.right)
                                .is_some_and(|rhs| rhs.is_function_expression_or_arrow())
                        }
                        syntax_kind_ext::VARIABLE_DECLARATION => {
                            let Some(var_decl) = arena.get_variable_declaration(node) else {
                                return false;
                            };
                            let Some(init_node) = arena.get(var_decl.initializer) else {
                                return false;
                            };
                            init_node.is_function_expression_or_arrow()
                        }
                        _ => false,
                    }
                };

            let declaration_arenas_for_declaration = |sym_id: SymbolId, decl_idx: NodeIndex| {
                let mut arenas = Vec::new();

                if self.ctx.arena.get(decl_idx).is_some() {
                    arenas.push(self.ctx.arena);
                }

                if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                    let symbol_arena_ref = symbol_arena.as_ref();
                    if !std::ptr::eq(symbol_arena_ref, self.ctx.arena) {
                        arenas.push(symbol_arena_ref);
                    }
                }

                if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
                    && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                {
                    if let Some(symbol_arena) = binder.symbol_arenas.get(&sym_id) {
                        let symbol_arena_ref = symbol_arena.as_ref();
                        if !arenas.iter().any(|a| std::ptr::eq(*a, symbol_arena_ref)) {
                            arenas.push(symbol_arena_ref);
                        }
                    }

                    if let Some(arenas_for_decl) =
                        binder.declaration_arenas.get(&(sym_id, decl_idx))
                    {
                        for arena in arenas_for_decl.iter() {
                            let arena_ref = arena.as_ref();
                            if !arenas.iter().any(|a| std::ptr::eq(*a, arena_ref)) {
                                arenas.push(arena_ref);
                            }
                        }
                    }
                }

                if let Some(arenas_for_decl) =
                    self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                {
                    for arena in arenas_for_decl.iter() {
                        let arena_ref = arena.as_ref();
                        if !arenas.iter().any(|a| std::ptr::eq(*a, arena_ref)) {
                            arenas.push(arena_ref);
                        }
                    }
                }

                arenas
            };

            let declaration_is_function_value = |decl_idx: NodeIndex| -> bool {
                let mut observed = false;
                for arena in declaration_arenas_for_declaration(sym_id, decl_idx) {
                    if arena.get(decl_idx).is_none() {
                        continue;
                    }
                    observed = true;
                    if !declaration_is_function_value_in_arena(arena, decl_idx) {
                        return false;
                    }
                }
                observed
            };

            let is_declared_function_or_class =
                (symbol_flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0;
            let is_callable_variable = (symbol_flags
                & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE))
                != 0
                && symbol_value_declaration.is_some()
                && {
                    let decl_idx = symbol_value_declaration;
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|decl_node| self.ctx.arena.get_variable_declaration(decl_node))
                        .and_then(|decl| self.ctx.arena.get(decl.initializer))
                        .is_some_and(|init_node| init_node.is_function_expression_or_arrow())
                };
            if !is_declared_function_or_class && !is_callable_variable {
                return false;
            }

            let mut declaration_indices = symbol_declarations;
            if symbol_value_declaration.is_some()
                && !declaration_indices.contains(&symbol_value_declaration)
            {
                declaration_indices.push(symbol_value_declaration);
            }
            for (&(entry_sym_id, decl_idx), _) in self.ctx.binder.declaration_arenas.iter() {
                if entry_sym_id == sym_id && !declaration_indices.contains(&decl_idx) {
                    declaration_indices.push(decl_idx);
                }
            }

            let has_mixed_non_callable_declaration =
                declaration_indices.iter().copied().any(|decl_idx| {
                    !self.declaration_is_checked_js_constructor_value_declaration(sym_id, decl_idx)
                        && !declaration_is_function_value(decl_idx)
                });
            let has_expando_declaration_pattern = !self.is_js_file()
                || !self.ctx.compiler_options.check_js
                || (!has_mixed_non_callable_declaration
                    && declaration_indices.iter().copied().all(|decl_idx| {
                        !self.declaration_is_checked_js_constructor_value_declaration(
                            sym_id, decl_idx,
                        )
                    }));
            if !has_expando_declaration_pattern {
                return false;
            }
            // For class declarations, don't treat as expando if the property
            // exists as an instance member. Accessing instance members on the
            // constructor type (e.g., `Base.instanceProp = 2`) should produce
            // TS2339, not be silently accepted as an expando.
            if prototype_root_expr.is_none()
                && (symbol_flags & symbol_flags::CLASS) != 0
                && let Some(prop_name) = prop_name.as_deref()
            {
                let obj_key = symbol_escaped_name.as_str();
                if self.class_has_instance_member(obj_key, prop_name) {
                    return false;
                }
            }
            return true;
        }

        // Namespace member fallback: allow expando assignment for function-typed
        // members accessed through namespace/value-module chains (e.g., `app.foo.bar = ...`).
        // Binder tracks these expandos by chain key, so reads can observe them later.
        fn root_identifier(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            if let Some(text) = arena.identifier_text_owned(idx) {
                return Some(text);
            }
            let node = arena.get(idx)?;
            if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                return root_identifier(arena, access.expression);
            }
            None
        }

        if object_type_is_function
            && let Some(root_name) = root_identifier(self.ctx.arena, object_expr_idx)
            && let Some(root_sym) = self.ctx.binder.file_locals.get(&root_name)
            && let Some(root_symbol) = self.ctx.binder.get_symbol(root_sym)
            && root_symbol
                .has_any_flags(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE)
        {
            return true;
        }

        // CommonJS exports behave like namespace-like value objects in JS/checkJs.
        // When an exported member is function-typed, assignments such as
        // `module.exports.f.self = module.exports.f` should use the same expando
        // path as plain `f.self = ...`.
        if self
            .current_file_commonjs_export_member_name(object_expr_idx)
            .is_some()
        {
            return true;
        }

        false
    }

    pub(in crate::types_domain) fn is_js_expando_object_assignment(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        object_type: TypeId,
        property_name: &str,
    ) -> bool {
        if !self.is_js_file()
            || !self.ctx.compiler_options.check_js
            || !crate::query_boundaries::common::is_object_like_type(self.ctx.types, object_type)
        {
            return false;
        }

        if !self.property_access_is_write_target_or_base(property_access_idx) {
            return false;
        }

        // Imported aliases should not behave as local JS expando objects.
        // Preserve TS2339 for writes like `importedCtor.prototype.foo = ...`.
        let mut root_idx = object_expr_idx;
        while let Some(root_node) = self.ctx.arena.get(root_idx) {
            if root_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && root_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                break;
            }
            let Some(root_access) = self.ctx.arena.get_access_expr(root_node) else {
                break;
            };
            root_idx = root_access.expression;
        }
        if let Some(root_node) = self.ctx.arena.get(root_idx)
            && root_node.kind == SyntaxKind::Identifier as u16
            && let Some(root_sym_id) = self.resolve_identifier_symbol(root_idx)
            && let Some(root_symbol) = self.ctx.binder.get_symbol(root_sym_id)
            && root_symbol.has_any_flags(symbol_flags::ALIAS)
            && root_symbol.import_module.is_some()
        {
            return false;
        }

        self.is_expando_property_read(object_expr_idx, property_name)
            || (self.property_access_is_direct_write_target(property_access_idx)
                && self
                    .current_file_commonjs_export_member_name(property_access_idx)
                    .is_some())
    }

    /// Check if a property access reads an expando property assigned via `X.prop = value`.
    ///
    /// Checks the current file's binder first, then all other binders in multi-file
    /// mode (for global-scope cross-file expando access). Also handles import chains
    /// like `a.C1.staticProp` by resolving the object expression to its source symbol
    /// and checking the source file's binder.
    pub(in crate::types_domain) fn is_expando_property_read(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        if self.is_current_file_commonjs_export_base_syntax(object_expr_idx)
            && !self.is_current_file_commonjs_export_base_for_expando(object_expr_idx)
        {
            return false;
        }

        let Some(obj_key) = Self::property_access_chain_in_arena(self.ctx.arena, object_expr_idx)
        else {
            return false;
        };

        // Do not treat imported aliases as prototype-expando roots.
        // In checkJs, writes like `importedCtor.prototype.foo = ...` should still
        // be checked against the imported instance shape (TS2339), not silently
        // accepted as local expandos.
        if let Some(object_node) = self.ctx.arena.get(object_expr_idx)
            && object_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(object_access) = self.ctx.arena.get_access_expr(object_node)
            && self
                .ctx
                .arena
                .get_identifier_at(object_access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "prototype")
            && let Some(root_sym_id) = self.resolve_identifier_symbol(object_access.expression)
            && let Some(root_symbol) = self.ctx.binder.get_symbol(root_sym_id)
            && root_symbol.has_any_flags(symbol_flags::ALIAS)
            && root_symbol.import_module.is_some()
        {
            return false;
        }

        // Don't treat as expando if the object is a class and the property exists
        // as an instance member of that class. In that case, accessing it on the
        // constructor type (typeof ClassName) should produce TS2339, not silently
        // succeed as an expando. This distinguishes `Base.a = 2` where `a` is an
        // instance getter/setter (should error) from `Base.newProp = 2` where
        // `newProp` is a genuine expando (should succeed).
        if self.class_has_instance_member(&obj_key, property_name) {
            return false;
        }

        if let Some(sym_id) = self.root_symbol_for_expando_read(object_expr_idx)
            && (self.is_js_file() && self.ctx.compiler_options.check_js)
            && let Some(symbol) = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id))
        {
            let mut declaration_indices = symbol.all_declarations();
            for (&(entry_sym_id, decl_idx), _) in self.ctx.binder.declaration_arenas.iter() {
                if entry_sym_id == sym_id && !declaration_indices.contains(&decl_idx) {
                    declaration_indices.push(decl_idx);
                }
            }

            let is_callable_variable = (symbol.flags
                & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE))
                != 0
                && symbol.value_declaration.is_some()
                && {
                    let decl_idx = symbol.value_declaration;
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|decl_node| self.ctx.arena.get_variable_declaration(decl_node))
                        .and_then(|decl| self.ctx.arena.get(decl.initializer))
                        .is_some_and(|init_node| init_node.is_function_expression_or_arrow())
                };
            let is_declared_function_or_class =
                (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0;

            let declaration_is_function_value_in_arena =
                |arena: &tsz_parser::parser::node::NodeArena, decl_idx: NodeIndex| -> bool {
                    if decl_idx.is_none() {
                        return false;
                    }
                    let Some(node) = arena.get(decl_idx) else {
                        return false;
                    };
                    match node.kind {
                        syntax_kind_ext::FUNCTION_DECLARATION => true,
                        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                            let Some(ext) = arena.get_extended(decl_idx) else {
                                return false;
                            };
                            if ext.parent.is_none() {
                                return false;
                            };
                            let parent_idx = ext.parent;
                            let Some(parent_node) = arena.get(parent_idx) else {
                                return false;
                            };
                            let Some(binary) = arena.get_binary_expr(parent_node) else {
                                return false;
                            };
                            if binary.left != decl_idx
                                || !self.is_assignment_operator(binary.operator_token)
                            {
                                return false;
                            }
                            arena
                                .get(binary.right)
                                .is_some_and(|rhs| rhs.is_function_expression_or_arrow())
                        }
                        syntax_kind_ext::BINARY_EXPRESSION => {
                            let Some(binary_node) = arena.get(decl_idx) else {
                                return false;
                            };
                            let Some(binary) = arena.get_binary_expr(binary_node) else {
                                return false;
                            };
                            if !self.is_assignment_operator(binary.operator_token) {
                                return false;
                            }
                            arena
                                .get(binary.right)
                                .is_some_and(|rhs| rhs.is_function_expression_or_arrow())
                        }
                        syntax_kind_ext::VARIABLE_DECLARATION => {
                            let Some(var_decl) = arena.get_variable_declaration(node) else {
                                return false;
                            };
                            let Some(init_node) = arena.get(var_decl.initializer) else {
                                return false;
                            };
                            init_node.is_function_expression_or_arrow()
                        }
                        _ => false,
                    }
                };

            let declaration_arenas_for_declaration = |sym_id: SymbolId, decl_idx: NodeIndex| {
                let mut arenas = Vec::new();

                if self.ctx.arena.get(decl_idx).is_some() {
                    arenas.push(self.ctx.arena);
                }

                if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                    let symbol_arena_ref = symbol_arena.as_ref();
                    if !std::ptr::eq(symbol_arena_ref, self.ctx.arena) {
                        arenas.push(symbol_arena_ref);
                    }
                }

                if let Some(arenas_for_decl) =
                    self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                {
                    for arena in arenas_for_decl.iter() {
                        let arena_ref = arena.as_ref();
                        if !arenas.iter().any(|a| std::ptr::eq(*a, arena_ref)) {
                            arenas.push(arena_ref);
                        }
                    }
                }

                arenas
            };

            let declaration_is_function_value = |decl_idx: NodeIndex| -> bool {
                let mut observed = false;
                for arena in declaration_arenas_for_declaration(sym_id, decl_idx) {
                    if arena.get(decl_idx).is_none() {
                        continue;
                    }
                    observed = true;
                    if !declaration_is_function_value_in_arena(arena, decl_idx) {
                        return false;
                    }
                }
                observed
            };

            let has_mixed_non_callable_declaration =
                declaration_indices.iter().copied().any(|decl_idx| {
                    !self.declaration_is_checked_js_constructor_value_declaration(sym_id, decl_idx)
                        && !declaration_is_function_value(decl_idx)
                });
            let has_callable_decl = declaration_indices
                .iter()
                .copied()
                .any(declaration_is_function_value)
                || is_declared_function_or_class
                || is_callable_variable;
            let has_expando_declaration_pattern =
                declaration_indices.iter().copied().all(|decl_idx| {
                    !self.declaration_is_checked_js_constructor_value_declaration(sym_id, decl_idx)
                        || declaration_is_function_value(decl_idx)
                });
            if has_callable_decl
                && (has_mixed_non_callable_declaration || !has_expando_declaration_pattern)
            {
                return false;
            }
        }

        // Object-literal variables can legitimately assign back to properties they
        // already declare in their semantic shape. Those writes should not opt the
        // property into the expando-forward-read path.
        if self.object_literal_root_declares_property(object_expr_idx, property_name) {
            return false;
        }

        // 1. Check current file's binder
        if self
            .ctx
            .binder
            .expando_properties
            .get(&obj_key)
            .is_some_and(|props| props.contains(property_name))
        {
            return true;
        }

        // 2. Check global expando index (O(1) instead of O(N) binder scan)
        if let Some(expando_idx) = &self.ctx.global_expando_index {
            if expando_idx
                .get(&obj_key)
                .is_some_and(|props| props.contains(property_name))
            {
                return true;
            }
        } else if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder
                    .expando_properties
                    .get(&obj_key)
                    .is_some_and(|props| props.contains(property_name))
                {
                    return true;
                }
            }
        }

        // 3. For qualified access chains like `a.C1` where `a` is an import namespace,
        //    the source file's binder stores the expando under just "C1" (the original
        //    symbol name), not "a.C1". Extract the last segment and check all binders.
        if let Some(last_dot) = obj_key.rfind('.') {
            let last_segment = &obj_key[last_dot + 1..];
            if let Some(expando_idx) = &self.ctx.global_expando_index {
                if expando_idx
                    .get(last_segment)
                    .is_some_and(|props| props.contains(property_name))
                {
                    return true;
                }
            } else if let Some(all_binders) = &self.ctx.all_binders {
                for binder in all_binders.iter() {
                    if binder
                        .expando_properties
                        .get(last_segment)
                        .is_some_and(|props| props.contains(property_name))
                    {
                        return true;
                    }
                }
            }
        }

        if let Some(file_idx) = self.expando_root_js_file_idx(object_expr_idx) {
            return self.js_file_has_expando_assignment_for_keys(
                file_idx,
                &self.expando_read_root_keys(object_expr_idx),
                property_name,
            );
        }

        false
    }

    pub(in crate::types_domain) fn expando_property_read_type(
        &mut self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let read_node = self.ctx.arena.get(property_access_idx)?;
        let obj_key = Self::property_access_chain_in_arena(self.ctx.arena, object_expr_idx)?;
        let expected_key = format!("{obj_key}.{property_name}");
        let recursion_key = format!("{}:{expected_key}", self.ctx.current_file_idx);
        if !self
            .ctx
            .expando_property_resolution_set
            .insert(recursion_key.clone())
        {
            return None;
        }
        let source_file = self
            .ctx
            .arena
            .source_files
            .get(self.ctx.current_file_idx)
            .or_else(|| self.ctx.arena.source_files.first())?;
        let mut best_match: Option<(u32, TypeId)> = None;

        for &stmt_idx in &source_file.statements.nodes {
            self.collect_expando_property_assignment_type(
                stmt_idx,
                &expected_key,
                read_node.pos,
                &mut best_match,
            );
        }

        if let Some((_, ty)) = best_match {
            self.ctx
                .expando_property_resolution_set
                .remove(&recursion_key);
            return Some(ty);
        }

        let root_keys = self.expando_read_root_keys(object_expr_idx);
        let preferred_file_idx = self.expando_root_js_file_idx(object_expr_idx);
        let result = self.js_expando_property_read_type_from_all_files(
            &root_keys,
            property_name,
            preferred_file_idx,
        );
        self.ctx
            .expando_property_resolution_set
            .remove(&recursion_key);
        result
    }

    pub(in crate::types_domain) fn refine_expando_property_read_type(
        &mut self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        property_name: &str,
        fallback_type: TypeId,
    ) -> TypeId {
        if fallback_type != TypeId::ANY {
            return fallback_type;
        }

        self.expando_property_read_type(property_access_idx, object_expr_idx, property_name)
            .unwrap_or(fallback_type)
    }

    pub(crate) fn declared_expando_property_type_for_root(
        &mut self,
        sym_id: SymbolId,
        root_name: &str,
        property_name: &str,
    ) -> TypeId {
        let preferred_file_idx = self.ctx.resolve_symbol_file_index(sym_id).or_else(|| {
            let arena = self
                .ctx
                .get_arena_for_file(self.ctx.current_file_idx as u32);
            let file_name = arena
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
                .unwrap_or(self.ctx.file_name.as_str());
            is_js_file_name(file_name).then_some(self.ctx.current_file_idx)
        });
        self.js_expando_property_read_type_from_all_files(
            &[root_name.to_string()],
            property_name,
            preferred_file_idx,
        )
        .unwrap_or(TypeId::ANY)
    }

    pub(in crate::types_domain) fn prior_js_this_property_assignment_type(
        &mut self,
        property_access_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let scope_root = self.find_enclosing_function_or_source_file(property_access_idx);
        let read_pos = self.ctx.arena.get(property_access_idx)?.pos;
        let mut best_match: Option<(u32, TypeId)> = None;
        self.collect_prior_js_this_property_assignment_type(
            scope_root,
            scope_root,
            property_name,
            read_pos,
            &mut best_match,
        );
        best_match.map(|(_, ty)| ty)
    }

    pub(in crate::types_domain) fn js_object_expr_is_this_or_alias(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let Some(sym_id) = self.resolve_identifier_symbol(idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let decl_node = match self.ctx.arena.get(symbol.value_declaration) {
            Some(node) => node,
            None => return false,
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        init_node.kind == SyntaxKind::ThisKeyword as u16
    }

    fn collect_prior_js_this_property_assignment_type(
        &mut self,
        idx: NodeIndex,
        scope_root: NodeIndex,
        property_name: &str,
        read_pos: u32,
        best_match: &mut Option<(u32, TypeId)>,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if idx != scope_root
            && (self.is_scope_owner_kind(node.kind)
                || node.kind == syntax_kind_ext::CLASS_DECLARATION)
        {
            return;
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && node.pos < read_pos
            && self
                .js_this_assignment_target_name(binary.left)
                .is_some_and(|name| name == property_name)
        {
            let rhs_idx = self.ctx.arena.skip_parenthesized(binary.right);
            let rhs_type = self.get_type_of_node(rhs_idx);
            if rhs_type != TypeId::ANY
                && rhs_type != TypeId::ERROR
                && best_match.is_none_or(|(best_pos, _)| node.pos >= best_pos)
            {
                *best_match = Some((node.pos, rhs_type));
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_prior_js_this_property_assignment_type(
                child_idx,
                scope_root,
                property_name,
                read_pos,
                best_match,
            );
        }
    }

    fn js_this_assignment_target_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let object_node = self.ctx.arena.get(access.expression)?;
                if object_node.kind != SyntaxKind::ThisKeyword as u16 {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.clone())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let object_node = self.ctx.arena.get(access.expression)?;
                if object_node.kind != SyntaxKind::ThisKeyword as u16 {
                    return None;
                }
                self.current_file_commonjs_static_member_name(access.name_or_argument)
            }
            _ => None,
        }
    }

    fn collect_expando_property_assignment_type(
        &mut self,
        idx: NodeIndex,
        expected_key: &str,
        read_pos: u32,
        best_match: &mut Option<(u32, TypeId)>,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if self.is_scope_owner_kind(node.kind) || node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return;
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && node.pos < read_pos
            && self
                .expando_assignment_access_key(binary.left)
                .is_some_and(|key| key == expected_key)
            && !Self::is_void_zero_or_undefined_rhs_in_arena(self.ctx.arena, binary.right)
        {
            // In JS/Salsa files, `x.y = void 0` is a property declaration placeholder,
            // not a meaningful type assignment. Skip it so the property type doesn't
            // become `undefined`, which would cause spurious TS18048 diagnostics.
            if !self.js_assignment_rhs_is_void_zero(binary.right) {
                let rhs_idx = self.ctx.arena.skip_parenthesized(binary.right);
                let rhs_type = self.get_type_of_node(rhs_idx);
                if rhs_type != TypeId::ANY
                    && rhs_type != TypeId::ERROR
                    && rhs_type != TypeId::UNDEFINED
                    && best_match.is_none_or(|(best_pos, _)| node.pos >= best_pos)
                {
                    *best_match = Some((node.pos, rhs_type));
                }
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_expando_property_assignment_type(
                child_idx,
                expected_key,
                read_pos,
                best_match,
            );
        }
    }

    fn expando_assignment_access_key(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone()),
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let left = self.expando_assignment_access_key(access.expression)?;
                let right = self.ctx.arena.get_identifier_at(access.name_or_argument)?;
                Some(format!("{left}.{}", right.escaped_text))
            }
            _ => None,
        }
    }

    pub(in crate::types_domain) fn expando_property_read_before_assignment(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        if self.property_access_is_write_target_or_base(property_access_idx) {
            return false;
        }
        if self.expando_read_is_self_default_initializer(property_access_idx) {
            return false;
        }
        if self.is_current_file_commonjs_export_base_for_expando(object_expr_idx) {
            if !self.is_js_file() || !self.ctx.compiler_options.check_js {
                return false;
            }
            return self.commonjs_export_read_before_assignment(property_access_idx, property_name);
        }
        if !self.expando_read_is_within_initializing_scope(property_access_idx, object_expr_idx) {
            return false;
        }
        if !self.is_expando_capable_read_root(object_expr_idx, property_name) {
            return false;
        }

        if let Some(file_idx) = self.expando_root_js_file_idx(object_expr_idx)
            && file_idx != self.ctx.current_file_idx
        {
            return false;
        }

        let Some(flow_node) = self.flow_node_for_reference_usage(property_access_idx) else {
            return false;
        };

        !self
            .flow_analyzer_for_property_reads()
            .is_definitely_assigned(property_access_idx, flow_node)
    }

    fn is_expando_capable_read_root(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        self.is_expando_property_read(object_expr_idx, property_name)
            || ((self.is_js_file() && self.ctx.compiler_options.check_js)
                && self.is_js_prototype_read_root(object_expr_idx, property_name))
    }

    pub(in crate::types_domain) fn current_file_commonjs_export_member_name(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.clone())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.commonjs_static_member_name_for_expando(access.name_or_argument)
            }
            _ => None,
        }
    }

    fn is_current_file_commonjs_export_base_for_expando(&self, idx: NodeIndex) -> bool {
        if self
            .ctx
            .js_export_surface_cache
            .get(&self.ctx.current_file_idx)
            .and_then(|surface| surface.direct_export_type)
            .is_some_and(|direct_export_type| {
                !crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                    self.ctx.types,
                    direct_export_type,
                )
            })
        {
            return false;
        }

        self.is_current_file_commonjs_export_base_syntax(idx)
    }

    fn is_current_file_commonjs_export_base_syntax(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "exports")
                && self
                    .resolve_identifier_symbol_without_tracking(idx)
                    .is_none();
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        let module_is_unshadowed = !self
            .resolve_identifier_symbol_without_tracking(access.expression)
            .is_some_and(|sym_id| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.decl_file_idx == self.ctx.current_file_idx as u32)
            });
        self.ctx
            .arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| ident.escaped_text == "module" && module_is_unshadowed)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    fn commonjs_static_member_name_for_expando(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx.arena.get_literal(node).map(|lit| lit.text.clone())
            }
            _ => None,
        }
    }

    fn commonjs_export_read_before_assignment(
        &self,
        property_access_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(read_node) = self.ctx.arena.get(property_access_idx) else {
            return false;
        };
        let read_pos = read_node.pos;
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        let mut assigned_before = false;
        let mut assigned_after = false;
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_commonjs_export_assignment_order(
                stmt_idx,
                property_name,
                read_pos,
                &mut assigned_before,
                &mut assigned_after,
            );
            if assigned_before && assigned_after {
                break;
            }
        }

        assigned_after && !assigned_before
    }

    fn collect_commonjs_export_assignment_order(
        &self,
        idx: NodeIndex,
        property_name: &str,
        read_pos: u32,
        assigned_before: &mut bool,
        assigned_after: &mut bool,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if self.is_scope_owner_kind(node.kind) || node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return;
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && let Some(name) = self.commonjs_export_assignment_name(binary.left)
            && name == property_name
        {
            if node.pos < read_pos {
                *assigned_before = true;
            } else if node.pos > read_pos {
                *assigned_after = true;
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_commonjs_export_assignment_order(
                child_idx,
                property_name,
                read_pos,
                assigned_before,
                assigned_after,
            );
        }
    }

    fn commonjs_export_assignment_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.clone())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.commonjs_static_member_name_for_expando(access.name_or_argument)
            }
            _ => None,
        }
    }
}
