//! JS `this.prop = ...` assignment-history lookup, the source-order scan
//! that backs `collect_expando_property_assignment_type`'s callers, and the
//! open-container implicit-`any` receiver test.
//!
//! Split out of `expando.rs` (mechanical move, no behavior change) to keep
//! that shard under the architecture's 2000-physical-line ceiling.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
        self.this_alias_root_node(idx).is_some()
    }

    /// Resolves `idx` (a bare `this`, or an identifier aliasing one via
    /// `const self = this;`) to the underlying `this`-keyword node.
    fn this_alias_root_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return Some(idx);
        }
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let init_node = self.ctx.arena.get(var_decl.initializer)?;
        (init_node.kind == SyntaxKind::ThisKeyword as u16).then_some(var_decl.initializer)
    }

    /// Whether a `this.<prop>` (or aliased-`this`) receiver genuinely binds to
    /// a real class instance — the only shape tsc infers members for from a
    /// same-scope prior `this.prop = …` write. `typeof globalThis` and a
    /// post-TS7 `@constructor` function's implicit-`any` `this` are excluded
    /// (oracle-verified against `typescript@7.0.2`): both keep re-reporting
    /// their own missing-member/implicit-any diagnostic instead.
    pub(in crate::types_domain) fn this_property_assignment_receiver_is_class_instance(
        &mut self,
        object_expr_idx: NodeIndex,
    ) -> bool {
        let Some(this_idx) = self.this_alias_root_node(object_expr_idx) else {
            return false;
        };
        !self.is_this_in_nested_function_without_own_this_binding(this_idx)
            && self
                .nearest_enclosing_class_for_this_binding(this_idx)
                .is_some()
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
                    .map(|ident| ident.escaped_text.to_string())
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

    /// The symbol's declaration `NodeIndex` that genuinely belongs to the
    /// CURRENT file's arena, or `None` if this file's binder owns no
    /// declaration of `sym_id`.
    ///
    /// A merged cross-file symbol's `value_declaration` can point into a
    /// DIFFERENT file's arena once the cross-file binder merge has run —
    /// `NodeIndex` is an arena-local offset, so reading one against a
    /// foreign arena silently resolves to an unrelated node instead of
    /// failing. `get_node_symbol` is the current file's own node-to-symbol
    /// map, so a round-trip back to `sym_id` proves `decl_idx` is genuinely
    /// local (the same guard `current_file_owns_expando_container_declaration`
    /// in `expando_container.rs` already uses for this exact hazard).
    fn current_file_declaration_for_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        symbol
            .all_declarations()
            .into_iter()
            .find(|&decl_idx| self.ctx.binder.get_node_symbol(decl_idx) == Some(sym_id))
    }

    /// Whether the expando root symbol's CURRENT-FILE declaration carries an
    /// explicit type annotation (`const c: SFC<P> = ...`). Annotated roots
    /// get member types from the annotation, so the assignment-scan walk
    /// must not run.
    pub(super) fn expando_root_symbol_has_type_annotation(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(decl_idx) = self.current_file_declaration_for_symbol(sym_id) else {
            return false;
        };
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        self.ctx
            .arena
            .get_variable_declaration(node)
            .is_some_and(|decl| decl.type_annotation.is_some())
    }

    /// Whether `block_idx` (a `BLOCK`) lexically re-declares `root_name`
    /// (`let`/`const`/`function`/`class`), shadowing the outer expando root.
    /// Assignments inside such a block target the SHADOWING binding and must
    /// not contribute to the outer root's property types (witness:
    /// expandoFunctionBlockShadowing — a block-local `const Y = function...;
    /// Y.test = 42` leaking `number` onto the top-level `Y.test: string`).
    fn block_shadows_expando_root(&self, block_idx: NodeIndex, root_name: &str) -> bool {
        for stmt_idx in self.ctx.arena.get_children(block_idx) {
            let Some(stmt) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            match stmt.kind {
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    let mut stack = vec![stmt_idx];
                    while let Some(idx) = stack.pop() {
                        let Some(node) = self.ctx.arena.get(idx) else {
                            continue;
                        };
                        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                            && let Some(decl) = self.ctx.arena.get_variable_declaration(node)
                            && self
                                .ctx
                                .arena
                                .get_identifier_at(decl.name)
                                .is_some_and(|ident| ident.escaped_text == root_name)
                        {
                            return true;
                        }
                        stack.extend(self.ctx.arena.get_children(idx));
                    }
                }
                syntax_kind_ext::FUNCTION_DECLARATION | syntax_kind_ext::CLASS_DECLARATION => {
                    let named = self
                        .ctx
                        .arena
                        .get_function(stmt)
                        .map(|function| function.name)
                        .or_else(|| self.ctx.arena.get_class(stmt).map(|class| class.name));
                    if named.is_some_and(|name_idx| {
                        self.ctx
                            .arena
                            .get_identifier_at(name_idx)
                            .is_some_and(|ident| ident.escaped_text == root_name)
                    }) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// The nearest enclosing `BLOCK` of the symbol's CURRENT-FILE
    /// declaration, or `NodeIndex::NONE` for a top-level (source-file-scoped)
    /// root, or when this file owns no declaration of `sym_id` at all.
    pub(super) fn expando_assignment_walk_root(&self, sym_id: tsz_binder::SymbolId) -> NodeIndex {
        let Some(mut current) = self.current_file_declaration_for_symbol(sym_id) else {
            return NodeIndex::NONE;
        };
        for _ in 0..64 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            if !ext.parent.is_some() {
                return NodeIndex::NONE;
            }
            let parent = ext.parent;
            if self
                .ctx
                .arena
                .get(parent)
                .is_some_and(|node| node.kind == syntax_kind_ext::BLOCK)
            {
                return parent;
            }
            current = parent;
        }
        NodeIndex::NONE
    }

    /// The statement children to scan for `walk_root`: the block's children
    /// when scoped, otherwise the current source file's top-level statements.
    pub(super) fn expando_walk_statements(&self, walk_root: NodeIndex) -> Vec<NodeIndex> {
        if walk_root.is_some() {
            return self.ctx.arena.get_children(walk_root);
        }
        self.ctx
            .arena
            .source_files
            .get(self.ctx.current_file_idx)
            .or_else(|| self.ctx.arena.source_files.first())
            .map(|source_file| source_file.statements.nodes.clone())
            .unwrap_or_default()
    }

    pub(super) fn collect_expando_property_assignment_type(
        &mut self,
        idx: NodeIndex,
        expected_key: &str,
        read_pos: u32,
        collected: &mut Vec<(u32, TypeId)>,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if self.is_scope_owner_kind(node.kind) || node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return;
        }
        // A block that lexically re-declares the root name shadows the outer
        // expando root for its whole subtree.
        if node.kind == syntax_kind_ext::BLOCK
            && let Some(root_name) = expected_key.split('.').next()
            && self.block_shadows_expando_root(idx, root_name)
        {
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
                let rhs_idx = Self::checked_js_constructor_initializer_expression(
                    self.ctx.arena,
                    binary.left,
                )
                .unwrap_or_else(|| self.terminal_expando_assignment_rhs(binary.right));
                let rhs_type = self.get_type_of_node(rhs_idx);
                if rhs_type != TypeId::ANY
                    && rhs_type != TypeId::ERROR
                    && rhs_type != TypeId::UNDEFINED
                {
                    collected.push((node.pos, rhs_type));
                }
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_expando_property_assignment_type(
                child_idx,
                expected_key,
                read_pos,
                collected,
            );
        }
    }

    /// Provisional (structural-only) type of one expando member of `root_name`,
    /// used while `sym_id` itself is mid-resolution (on the circular-reference
    /// guard in [`crate::state::type_analysis::symbol_type_helpers::CheckerState::provisional_circular_function_symbol_type`]).
    ///
    /// A member whose RHS is a function expression/declaration/arrow gets its
    /// signature built directly from the declaration (`call_signature_from_function`)
    /// — the same structural extraction `tsc` uses for a function's own type,
    /// independent of checking its body for diagnostics. This breaks the cycle
    /// for `root.member = function () { this.other }`: resolving `this`'s
    /// owner type while checking `member`'s own body must not re-enter
    /// checking that same body. A non-function-valued member is left out of
    /// this transient shape; the fully augmented type (via
    /// `augment_callable_type_with_expandos`) overwrites this provisional
    /// entry once `sym_id`'s resolution completes.
    pub(crate) fn provisional_expando_property_signature_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        root_name: &str,
        property_name: &str,
    ) -> Option<TypeId> {
        let expected_key = format!("{root_name}.{property_name}");
        let walk_root = self.expando_assignment_walk_root(sym_id);
        let mut collected: Vec<(u32, TypeId)> = Vec::new();
        for stmt_idx in self.expando_walk_statements(walk_root) {
            self.collect_expando_property_provisional_signature(
                stmt_idx,
                &expected_key,
                &mut collected,
            );
        }
        if collected.is_empty() {
            return None;
        }
        collected.sort_unstable_by_key(|&(pos, _)| pos);
        let mut types: Vec<TypeId> = collected.into_iter().map(|(_, ty)| ty).collect();
        types.dedup();
        Some(if types.len() == 1 {
            types[0]
        } else {
            self.ctx.types.factory().union_from_slice(&types)
        })
    }

    fn collect_expando_property_provisional_signature(
        &mut self,
        idx: NodeIndex,
        expected_key: &str,
        collected: &mut Vec<(u32, TypeId)>,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if self.is_scope_owner_kind(node.kind) || node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return;
        }
        if node.kind == syntax_kind_ext::BLOCK
            && let Some(root_name) = expected_key.split('.').next()
            && self.block_shadows_expando_root(idx, root_name)
        {
            return;
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && self
                .expando_assignment_access_key(binary.left)
                .is_some_and(|key| key == expected_key)
        {
            let rhs_idx = self.terminal_expando_assignment_rhs(binary.right);
            if let Some(rhs_node) = self.ctx.arena.get(rhs_idx)
                && let Some(func) = self.ctx.arena.get_function(rhs_node)
            {
                let sig = self.call_signature_from_function(func, rhs_idx);
                let func_type =
                    crate::query_boundaries::construct_signatures::function_type_from_call_signature(
                        self.ctx.types,
                        &sig,
                        false,
                    );
                collected.push((node.pos, func_type));
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_expando_property_provisional_signature(child_idx, expected_key, collected);
        }
    }

    fn terminal_expando_assignment_rhs(&self, idx: NodeIndex) -> NodeIndex {
        let idx = self.ctx.arena.skip_parenthesized(idx);
        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
        {
            return self.terminal_expando_assignment_rhs(binary.right);
        }
        idx
    }

    fn expando_assignment_access_key(&mut self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string()),
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                // An optional-chain hop (`obj?.a = 1`) is never a valid
                // assignment target (TS2779) and tsc's expando/special-property
                // detection requires a "bindable static name expression" — a
                // chain of plain property accesses, which an optional hop is
                // not. Such a write must not be read back as an expando
                // property declaration on a later access of the same name.
                if access.question_dot_token {
                    return None;
                }
                let left = self.expando_assignment_access_key(access.expression)?;
                let right = self.ctx.arena.get_identifier_at(access.name_or_argument)?;
                Some(format!("{left}.{}", right.escaped_text))
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if access.question_dot_token {
                    return None;
                }
                let left = self.expando_assignment_access_key(access.expression)?;
                let right = self.expando_element_key_name(access.name_or_argument)?;
                Some(format!("{left}.{right}"))
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

    /// Whether an unknown property on `type_id` is an implicit `any` rather than
    /// a `TS2339`, because the receiver is an *open* JS object container.
    ///
    /// In a JS file a value whose type is an anonymous object shape is open: JS
    /// code routinely builds such containers up by property assignment, often
    /// across files (`var N = {}` in one file, `N.commands.a = 1` in another), so
    /// `tsc` types the access as an implicit `any` and reports it only under
    /// `noImplicitAny`.
    ///
    /// The shape's nominal `symbol` separates an open container from a declared
    /// shape: class instance types carry it so distinct classes do not intern
    /// structurally, and interfaces carry their declaration's symbol. So
    /// `Event.prototype.removeChildren = ...` and `new C().q` keep reporting
    /// TS2339. Arrays and primitives have no object shape at all and are
    /// excluded before the `symbol` test is reached.
    ///
    /// A receiver produced by an object spread (`{ ...base }`) is excluded even
    /// though it is anonymous and symbol-less: `tsc`'s `getSpreadType` never
    /// marks its result `ObjectFlags.JSLiteral` the way a hand-written object
    /// literal is marked, so a spread-derived container stays a strict TS2339
    /// target rather than joining the open-container leniency.
    pub(crate) fn js_open_object_receiver_under_implicit_any(&self, type_id: TypeId) -> bool {
        self.is_js_file()
            && self.ctx.compiler_options.check_js
            && !self.ctx.no_implicit_any()
            && crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                .is_some_and(|shape| shape.symbol.is_none() && !shape.is_spread_literal())
    }
}
