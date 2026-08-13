//! Expando function/property detection, JS expando reads, and CommonJS export helpers.
//!
//! Covers the property chain resolution, expando assignment detection, cross-file
//! expando type resolution, synthesized array iterator methods, and CommonJS
//! export member name resolution.

use crate::context::is_js_file_name;
use crate::state::CheckerState;
use crate::symbols_domain::name_text::static_element_access_key_text_in_arena;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Walk a property/element-access chain to its identifier root and return true
    /// if that root resolves to a binding that semantically *imports* another
    /// module's namespace — either an ESM / `import =` ALIAS symbol with a
    /// recorded `import_module`, or a `const X = require("…")` JS binding.
    ///
    /// JSDoc-style "assigned value type" recovery (`function foo() {} foo.bar = 1`)
    /// must not fire on these roots: writes like `mod.bar = 1` against an imported
    /// namespace are TS2339, not local expando declarations.
    pub(crate) fn property_access_root_is_imported_namespace(
        &self,
        object_expr_idx: NodeIndex,
    ) -> bool {
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

        let Some(root_node) = self.ctx.arena.get(root_idx) else {
            return false;
        };
        if root_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        if let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(root_idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, root_idx))
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.has_any_flags(symbol_flags::ALIAS)
            && symbol.import_module().is_some()
        {
            return true;
        }

        self.is_require_call_bound_identifier(root_idx)
    }

    pub(super) fn property_access_chain_in_arena(
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        if let Some(text) = arena.identifier_text_owned(idx) {
            return Some(text);
        }
        let node = arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = arena.get_access_expr(node)?;
                let left = Self::property_access_chain_in_arena(arena, access.expression)?;
                let right = arena
                    .get_identifier_at(access.name_or_argument)?
                    .escaped_text
                    .clone();
                Some(format!("{left}.{right}"))
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = arena.get_access_expr(node)?;
                let left = Self::property_access_chain_in_arena(arena, access.expression)?;
                let right =
                    static_element_access_key_text_in_arena(arena, access.name_or_argument)?;
                Some(format!("{left}.{right}"))
            }
            _ => None,
        }
    }

    pub(super) fn expando_assignment_access_key_in_arena(
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string()),
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = arena.get_access_expr(node)?;
                let left = Self::expando_assignment_access_key_in_arena(arena, access.expression)?;
                let right = arena.get_identifier_at(access.name_or_argument)?;
                Some(format!("{left}.{}", right.escaped_text))
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = arena.get_access_expr(node)?;
                let left = Self::expando_assignment_access_key_in_arena(arena, access.expression)?;
                let right =
                    static_element_access_key_text_in_arena(arena, access.name_or_argument)?;
                Some(format!("{left}.{right}"))
            }
            _ => None,
        }
    }

    /// Returns `true` if the node at `idx` is a `void 0` expression or the identifier
    /// `undefined`. These are sentinel "uninitialized" markers: tsc does NOT include them
    /// as expando property types (it emits TS2339 instead of TS18048 when such a
    /// property is later read or used in a binary expression).
    pub(super) fn is_void_zero_or_undefined_rhs_in_arena(
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> bool {
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

        if init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && self.variable_declaration_has_jsdoc_type_annotation(decl_idx)
        {
            return false;
        }

        // Only an EMPTY object literal (`var X = {}`) is an expando host: tsc's
        // `getExpandoInitializer` treats a non-empty literal (`var X = { a: 1 }`)
        // as a closed shape, so a later `X.b` read is `TS2339`, not an expando
        // member. Function/class expression initializers stay hosts regardless.
        init_node.is_function_expression_or_arrow()
            || init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || arena.is_empty_object_literal(var_decl.initializer)
    }

    /// tsc 7.0.2 binds an assignment-declared expando member only when the
    /// write appears in the host's OWN declaring file (oracle-pinned for
    /// function, class, and `var X = {}` hosts, TS and JS script files alike;
    /// see `js_cross_file_expando_declaration_tests`). A foreign-file write is
    /// an ordinary property assignment against the host's declared shape:
    /// `TS2339` under `noImplicitAny`, with the open-container leniency still
    /// silencing `{}`-typed receivers when it is off.
    pub(crate) fn expando_write_host_is_foreign_file(&self, sym_id: SymbolId) -> bool {
        self.ctx
            .resolve_symbol_file_index(sym_id)
            .is_some_and(|file_idx| file_idx != self.ctx.current_file_idx)
    }

    fn root_symbol_supports_js_direct_expando_write(&self, sym_id: SymbolId) -> bool {
        if self.expando_write_host_is_foreign_file(sym_id) {
            return false;
        }

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

        // Mirror `root_symbol_supports_js_expando_read`: a `var X = {}` object
        // literal is an expando host. The binder's per-file expando tracking only
        // records the write when the writing file can resolve the root, so a
        // cross-file (or forward-referenced) `X.member = value` whose `X = {}`
        // declaration lives in another file is missed there; this cross-file-aware
        // predicate keeps the write from surfacing a spurious TS2339 on `{}`,
        // matching the read side that already resolves such members. A JSDoc
        // `@type` annotation opts the variable out of the expando model.
        if init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && self.variable_declaration_has_jsdoc_type_annotation(decl_idx)
        {
            return false;
        }

        // Emptiness gate, mirroring the read side and tsc's
        // `getExpandoInitializer`: a `var X = {}` empty literal hosts expando
        // writes, but `var X = { a: 1 }` is a closed shape whose later `X.b = …`
        // write is an ordinary property assignment (`TS2339` under
        // `noImplicitAny`; silenced under the open-container leniency otherwise).
        init_node.is_function_expression_or_arrow()
            || init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || arena.is_empty_object_literal(var_decl.initializer)
    }

    fn variable_declaration_has_jsdoc_type_annotation(&self, decl_idx: NodeIndex) -> bool {
        let Some(source_file) = self.source_file_data_for_node(decl_idx) else {
            return false;
        };
        if source_file.comments.is_empty() || !source_file.comments.iter().any(|c| c.is_multi_line)
        {
            return false;
        }
        let source_text = source_file.text.to_string();
        let comments = source_file.comments.clone();
        self.try_jsdoc_with_ancestor_walk(decl_idx, &comments, &source_text)
            .as_deref()
            .and_then(Self::extract_jsdoc_type_expression)
            .is_some()
    }

    pub(super) fn expando_root_js_file_idx(&self, object_expr_idx: NodeIndex) -> Option<usize> {
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

        // An explicit type annotation makes the declared type authoritative, so
        // the write is an ordinary property assignment and must report TS2339
        // when the property is absent. `const f: () => void = () => {}` takes no
        // expando properties even though its initializer is a function, while
        // the inferred `const f = () => {}` still does.
        if let Some(sym_id) = sym_id
            && self.expando_root_symbol_has_type_annotation(sym_id)
        {
            return false;
        }

        if let Some(sym_id) = sym_id
            && self.expando_write_host_is_foreign_file(sym_id)
        {
            return false;
        }

        if let Some(sym_id) = sym_id
            && let Some(symbol) = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id))
        {
            let symbol_flags = symbol.flags;
            let symbol_value_declaration = symbol.value_declaration;
            let symbol_declarations = symbol.declarations.clone();
            let symbol_escaped_name = symbol.escaped_name.clone();

            // In TS files, `fn.prop = e` is an expando DECLARATION only when
            // the assignment shares its enclosing container (nearest
            // function-like/module, `NONE` = source file; blocks and loop/if
            // heads are transparent) with `fn`'s declaration — mirrors the
            // binder's record-time gate. A nested-function assignment falls
            // through to the normal property check and reports TS2339. Scoped
            // to locally-bound symbols so a cross-arena declaration index is
            // never compared against this file's nodes.
            fn nearest_expando_container(
                arena: &tsz_parser::parser::node::NodeArena,
                start: NodeIndex,
            ) -> NodeIndex {
                let mut current = start;
                for _ in 0..256 {
                    let Some(ext) = arena.get_extended(current) else {
                        return NodeIndex::NONE;
                    };
                    let parent = ext.parent;
                    if parent.is_none() {
                        return NodeIndex::NONE;
                    }
                    let Some(node) = arena.get(parent) else {
                        return NodeIndex::NONE;
                    };
                    if node.is_function_like()
                        || node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                    {
                        return parent;
                    }
                    current = parent;
                }
                NodeIndex::NONE
            }
            if !self.is_js_file()
                && symbol_value_declaration.is_some()
                && self.ctx.binder.get_symbol(sym_id).is_some()
                && nearest_expando_container(self.ctx.arena, property_access_idx)
                    != nearest_expando_container(self.ctx.arena, symbol_value_declaration)
            {
                return false;
            }

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
                // Mirrors the TS2339 site (`resolve.rs`): constructor evidence
                // is irrelevant to prototype closure — only `noImplicitAny`
                // gates it. #17226 gap 2.
                && self.ctx.no_implicit_any()
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

            // A class declaration is an expando host only in a JS file. In a
            // TS file classes already own their static namespace, so
            // `class C {} C.x = 1` stays `TS2339` (oracle-verified,
            // `typeFromPropertyAssignment29.ts`), matching the binder's own
            // `is_js_class_root` recording gate in `expression_flow.rs`.
            let is_declared_function_or_class = (symbol_flags & symbol_flags::FUNCTION) != 0
                || (self.is_js_file() && (symbol_flags & symbol_flags::CLASS) != 0);
            let is_callable_variable = (symbol_flags
                & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE))
                != 0
                && symbol_value_declaration.is_some()
                && {
                    let decl_idx = symbol_value_declaration;
                    // TS files require a `const`-bound (or `using`-bound)
                    // initializer ("must be const" — a `var`/`let` function
                    // expression is not an expando host); JS files keep the
                    // permissive `var` idiom.
                    (self.is_js_file() || self.ctx.arena.is_var_const_like_declaration(decl_idx))
                        && self
                            .ctx
                            .arena
                            .get(decl_idx)
                            .and_then(|decl_node| {
                                self.ctx.arena.get_variable_declaration(decl_node)
                            })
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
            // Previously this site iterated the entire `declaration_arenas` map
            // filtering by `entry_sym_id == sym_id`. With the program-wide map
            // now shared across all per-file binders via `Arc`, a full iteration
            // would be O(N_program) per call; the `sym_to_decl_indices` secondary
            // index collapses that to a point lookup.
            if let Some(extra_indices) = self.ctx.binder.sym_to_decl_indices.get(&sym_id) {
                for &decl_idx in extra_indices {
                    if !declaration_indices.contains(&decl_idx) {
                        declaration_indices.push(decl_idx);
                    }
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

        // Namespace-member fallback (checked JS only): under the permissive JS
        // model a namespace/value-module chain whose target member is
        // function-typed hosts expando writes like `app.foo.bar = ...`, and the
        // binder tracks them by chain key so reads observe them later. In TS
        // files this is never a valid expando declaration — the primary symbol
        // path above, with its same-container gate, is authoritative, so
        // `app.foo.bar = e` against a namespace-declared function stays TS2339.
        // The binder mirrors the split, declining to record TS nested chains.
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

        if self.is_js_file()
            && object_type_is_function
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
            && root_symbol.import_module().is_some()
        {
            return false;
        }

        if self.is_expando_property_read(object_expr_idx, property_name) {
            return true;
        }

        if self.property_access_is_direct_write_target(property_access_idx) {
            if self
                .current_file_commonjs_export_member_name(property_access_idx)
                .is_some()
            {
                return true;
            }

            if let Some(obj_key) =
                Self::property_access_chain_in_arena(self.ctx.arena, object_expr_idx)
                && !self.class_has_instance_member(&obj_key, property_name)
                && let Some(sym_id) = self.root_symbol_for_expando_read(object_expr_idx)
            {
                return self.root_symbol_supports_js_direct_expando_write(sym_id);
            }
        }

        false
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
            && root_symbol.import_module().is_some()
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
            // Previously this site iterated the entire `declaration_arenas` map
            // filtering by `entry_sym_id == sym_id`. With the program-wide map
            // now shared across all per-file binders via `Arc`, a full iteration
            // would be O(N_program) per call; the `sym_to_decl_indices` secondary
            // index collapses that to a point lookup.
            if let Some(extra_indices) = self.ctx.binder.sym_to_decl_indices.get(&sym_id) {
                for &decl_idx in extra_indices {
                    if !declaration_indices.contains(&decl_idx) {
                        declaration_indices.push(decl_idx);
                    }
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
            let is_declared_class = (symbol.flags & symbol_flags::CLASS) != 0;

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
                && !is_declared_class
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

        if let Some(sym_id) = self.root_symbol_for_expando_read(object_expr_idx)
            && !self.root_symbol_supports_js_expando_read(sym_id)
        {
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
            // A nested chain re-discovered only by this source scan (the binder
            // never recorded it) is a valid expando member only when its
            // immediate base link is itself a declared expando. Without this the
            // scan would accept the very write it is inspecting — e.g.
            // `chrome.devtools.inspectedWindow = {}` where `chrome.devtools` is a
            // `{}`-typed `Object.defineProperty` member. `prototype` chains keep
            // their dedicated handling.
            if !self.nested_expando_base_link_is_declared(object_expr_idx) {
                return false;
            }
            return self.js_file_has_expando_assignment_for_keys(
                file_idx,
                &self.expando_read_root_keys(object_expr_idx),
                property_name,
            );
        }

        false
    }

    /// Whether a nested expando base chain (`a.b` in `a.b.c = e`) is itself a
    /// declared expando member. A single-identifier base has no intermediate
    /// link and is vacuously valid. `prototype` links are exempt — `prototype`
    /// is a built-in member carried by the prototype-expando paths, not by
    /// assignment records.
    fn nested_expando_base_link_is_declared(&self, object_expr_idx: NodeIndex) -> bool {
        let Some(object_node) = self.ctx.arena.get(object_expr_idx) else {
            return true;
        };
        let (base_expr, member_name) = match object_node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let Some(access) = self.ctx.arena.get_access_expr(object_node) else {
                    return true;
                };
                let Some(member) = self
                    .ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.to_string())
                else {
                    return true;
                };
                (access.expression, member)
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let Some(access) = self.ctx.arena.get_access_expr(object_node) else {
                    return true;
                };
                let Some(member) = static_element_access_key_text_in_arena(
                    self.ctx.arena,
                    access.name_or_argument,
                ) else {
                    return true;
                };
                (access.expression, member)
            }
            // Single-identifier base: no intermediate link to validate.
            _ => return true,
        };

        if member_name == "prototype" {
            return true;
        }

        // Being a declared member is necessary but not sufficient: the base
        // link must itself be an expando HOST — its declaring write's RHS an
        // empty literal, function, or class expression. `a.b = { k: 1 }`
        // declares `b` as a closed shape, so a nested `a.b.c` member is
        // TS2339 under `noImplicitAny`, mirroring tsc's
        // `getExpandoInitializer` emptiness rule (#17226 gap 1).
        self.is_expando_property_read(base_expr, &member_name)
            && self.nested_expando_base_link_rhs_is_host(base_expr, &member_name)
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
        let mut collected: Vec<(u32, TypeId)> = Vec::new();

        for &stmt_idx in &source_file.statements.nodes {
            self.collect_expando_property_assignment_type(
                stmt_idx,
                &expected_key,
                read_node.pos,
                &mut collected,
            );
        }

        // Historical last-position-wins semantics for the prior-assignment read.
        let best_match = collected.into_iter().max_by_key(|&(pos, _)| pos);
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

        // Only recover an expando-assigned type when the receiver actually
        // qualifies for expando property reads — the same eligibility gate the
        // non-optional property-read path applies (see the
        // `is_expando_property_read` branch in
        // `property_access_type/identifier_resolution.rs`). This function is only
        // reached from the optional-chain property fast path, whose caller passes
        // the raw property-resolution result as `fallback_type`; without the gate
        // it would recover a value for *any* `any`-typed receiver by walking the
        // file for a matching `recv.prop = <value>` assignment, even when that
        // receiver is a plain `any` and not an expando root.
        //
        // That mismatch is exactly `#16710`: `declare const obj: any; obj?.a = 1;`
        // followed by `obj?.a` narrowed away from `any` to the written value's
        // type, while `tsc` keeps it `any` (the write target is itself the
        // invalid-optional-assignment `TS2779`, and `any` has no synthesized
        // members). The non-optional spelling `obj.a` already stays `any` because
        // it is gated this way; gating here keeps the optional-chain fast path
        // consistent with it, while genuine expando roots (whose property already
        // resolves to a concrete type, so `fallback_type != ANY`, or which pass
        // `is_expando_property_read`) are unaffected.
        if !self.is_expando_property_read(object_expr_idx, property_name) {
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
        // Resolve the declared type from assignments in the current file by
        // walking its statements. Unlike the cross-file reader below, this walk
        // is not gated to `.js` files, so a TS-file expando such as
        // `function F(): void {} F.p = 1` types `p` as `number` (matching tsc)
        // instead of `any`. The resolved RHS type is widened for display exactly
        // as tsc's `getWidenedType` widens a special-property assignment
        // (`1` -> `number`, fresh `{ foo: 1 }` -> `{ foo: number; }`), while a
        // non-fresh `as const` literal is preserved.
        let expected_key = format!("{root_name}.{property_name}");
        let recursion_key = format!("declared:{}:{expected_key}", self.ctx.current_file_idx);
        // The walk only applies to expando roots WITHOUT a declared type: a
        // variable annotated `const c: SFC<P> = ...` gets its member types
        // from the annotation, and its property assignments are CHECKED
        // against those declared types (contextually typed), never
        // synthesized from the RHS. Re-typing the member from the (widened)
        // assignment RHS both loses the contextual narrowing and replaces a
        // declared literal member with its base (witness:
        // expandoFunctionContextualTypes' `defaultProps = { color: "red" }`
        // failing against `Partial<{ color: "red" | "blue" }>`, and
        // expandoFunctionExpressionsWithDynamicNames2's `[sym]: true` member
        // widening to `boolean`).
        if !self.expando_root_symbol_has_type_annotation(sym_id)
            && self
                .ctx
                .expando_property_resolution_set
                .insert(recursion_key.clone())
        {
            let mut collected: Vec<(u32, TypeId)> = Vec::new();
            // Walk from the QUERIED symbol's enclosing lexical scope, not
            // unconditionally from the top level: a block-scoped shadowing
            // root (`if (...) { const Y = function Y() {}; Y.test = 42; }`)
            // must collect the block's assignments, while the outer root's
            // walk (started at the source file) skips that shadowing block
            // via `block_shadows_expando_root`.
            let walk_root = self.expando_assignment_walk_root(sym_id);
            for stmt_idx in self.expando_walk_statements(walk_root) {
                self.collect_expando_property_assignment_type(
                    stmt_idx,
                    &expected_key,
                    u32::MAX,
                    &mut collected,
                );
            }
            self.ctx
                .expando_property_resolution_set
                .remove(&recursion_key);
            if !collected.is_empty() {
                // Every assignment is a DECLARATION of the property; the
                // property type is the union of the (widened) assignment
                // types, exactly as tsc merges multi-branch expando
                // assignments (`if (b) { g.both = 'hi' } else { g.both = 0 }`
                // gives `string | number`, and neither branch is checked
                // against the other).
                let mut widened: Vec<TypeId> = collected
                    .iter()
                    .map(|&(_, ty)| {
                        crate::query_boundaries::widening::widen_type_for_display_preserving_non_fresh(
                            self.ctx.types,
                            ty,
                        )
                    })
                    .collect();
                widened.sort_unstable_by_key(|ty| ty.0);
                widened.dedup();
                return if widened.len() == 1 {
                    widened[0]
                } else {
                    self.ctx.types.factory().union_from_slice(&widened)
                };
            }
        }

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

    /// First same-file assignment source position for each expando property of
    /// `root_name`, keyed by the property's simple name. tsc lists expando
    /// members in source order; the binder records them in an unordered set, so
    /// this recovers a deterministic ordering key. Only file/block-scope
    /// assignments are visited (mirroring `collect_expando_property_assignment_type`),
    /// so nested-function assignments — which stay `TS2339` — never contribute.
    pub(crate) fn expando_property_source_positions(
        &self,
        root_name: &str,
    ) -> rustc_hash::FxHashMap<String, u32> {
        let mut positions = rustc_hash::FxHashMap::default();
        let Some(source_file) = self
            .ctx
            .arena
            .source_files
            .get(self.ctx.current_file_idx)
            .or_else(|| self.ctx.arena.source_files.first())
        else {
            return positions;
        };
        let prefix = format!("{root_name}.");
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_expando_property_positions(stmt_idx, &prefix, &mut positions);
        }
        positions
    }

    fn collect_expando_property_positions(
        &self,
        idx: NodeIndex,
        prefix: &str,
        positions: &mut rustc_hash::FxHashMap<String, u32>,
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
            && let Some(key) =
                Self::expando_assignment_access_key_in_arena(self.ctx.arena, binary.left)
            && let Some(prop) = key.strip_prefix(prefix)
            && !prop.contains('.')
        {
            positions.entry(prop.to_string()).or_insert(node.pos);
        }
        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_expando_property_positions(child_idx, prefix, positions);
        }
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

    /// Whether the expando root symbol's value declaration carries an explicit
    /// type annotation (`const c: SFC<P> = ...`). Annotated roots get member
    /// types from the annotation, so the assignment-scan walk must not run.
    fn expando_root_symbol_has_type_annotation(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let decl_idx = symbol.value_declaration;
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

    /// The nearest enclosing `BLOCK` of the symbol's value declaration, or
    /// `NodeIndex::NONE` for a top-level (source-file-scoped) root.
    fn expando_assignment_walk_root(&self, sym_id: tsz_binder::SymbolId) -> NodeIndex {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return NodeIndex::NONE;
        };
        let mut current = symbol.value_declaration;
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
    fn expando_walk_statements(&self, walk_root: NodeIndex) -> Vec<NodeIndex> {
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
