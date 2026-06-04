impl<'a> CallHierarchyProvider<'a> {
    const fn is_call_hierarchy_callable_kind(kind: u16) -> bool {
        kind == syntax_kind_ext::METHOD_SIGNATURE
            || kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
    }

    fn is_call_hierarchy_callable_node(&self, node_idx: NodeIndex) -> bool {
        self.arena.get(node_idx).is_some_and(|node| {
            node.is_function_like() || Self::is_call_hierarchy_callable_kind(node.kind)
        })
    }

    /// Prepare a call hierarchy item at the given position.
    ///
    /// Finds the function, method, or constructor at the cursor and returns
    /// a `CallHierarchyItem` describing it. Returns `None` if the cursor
    /// is not on a callable symbol.
    pub fn prepare(&self, _root: NodeIndex, position: Position) -> Option<CallHierarchyItem> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // Prefer exact symbol resolution at callsites before falling back to
        // enclosing-function probing.
        if let Some(item) = self.prepare_item_from_reference(node_idx) {
            return Some(item);
        }

        // Walk up from the found node to find the enclosing function-like node.
        // If the cursor is on an identifier that is the name of a function, use
        // the function itself; otherwise walk parents.
        self.find_function_at_or_around(node_idx)
            .and_then(|func_idx| self.make_call_hierarchy_item(func_idx))
    }

    /// Find all incoming calls (callers) for the function at the given position.
    ///
    /// Scans the AST for identifier nodes whose `escaped_text` matches the
    /// target function name and that appear inside `CallExpression` nodes,
    /// then groups them by their containing function to produce caller items.
    pub fn incoming_calls(
        &self,
        _root: NodeIndex,
        position: Position,
    ) -> Vec<CallHierarchyIncomingCall> {
        let mut results = Vec::new();

        let offset = match self.line_map.position_to_offset(position, self.source_text) {
            Some(o) => o,
            None => return results,
        };

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return results;
        }

        // Find the target callable at this position. Prefer explicit symbol
        // resolution from callsites (e.g. `bar()` -> `const bar = function(){}`).
        let mut func_idx = self
            .resolve_reference_callable(node_idx)
            .or_else(|| self.find_function_at_or_around(node_idx));
        if func_idx.is_none() {
            func_idx = self.export_equals_anonymous_function_callable();
        }
        let func_idx = match func_idx {
            Some(idx) => idx,
            None => return results,
        };

        let target_kind = self.get_function_symbol_kind(func_idx);
        // Get the query name for this callable. Constructors use the containing class name.
        let target_name = if target_kind == SymbolKind::Constructor {
            match self.constructor_target_name(func_idx) {
                Some(name) => name,
                None => return results,
            }
        } else {
            match self.get_function_name_idx(func_idx) {
                Some(name_idx) => match self.get_identifier_text(name_idx) {
                    Some(name) => name,
                    None => return results,
                },
                None => return results,
            }
        };

        let name_idx = self.get_function_name_idx(func_idx);
        let target_symbol_id = name_idx
            .and_then(|idx| self.binder.get_node_symbol(idx))
            .or_else(|| self.binder.get_node_symbol(func_idx));
        let target_namespace_hint = self.enclosing_namespace_name(func_idx);
        let target_member_container_hint = self.member_container_hint_for_callable(func_idx);
        let target_is_member_like =
            (matches!(target_kind, SymbolKind::Method | SymbolKind::Property)
                || self
                    .property_declaration_for_function_initializer(func_idx)
                    .is_some())
                && self
                    .arena
                    .get(func_idx)
                    .is_some_and(|node| node.kind != syntax_kind_ext::METHOD_SIGNATURE);

        // Scan all identifier nodes in the arena that match the target name and
        // appear in a relevant call/reference context, grouping by containing function.
        let mut callers: FxHashMap<u32, Vec<Range>> = FxHashMap::default();
        let mut script_from_ranges = Vec::new();

        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let idx = NodeIndex(i as u32);

            // Skip the declaration name itself
            if name_idx == Some(idx) {
                continue;
            }

            // Check if this identifier's escaped_text matches
            let ident_data = match self.arena.get_identifier(node) {
                Some(d) => d,
                None => continue,
            };
            if ident_data.escaped_text != target_name {
                continue;
            }

            if target_is_member_like {
                // Member-like targets should only consider property-access references and
                // reject unrelated same-name members from other containers.
                let Some(receiver_idx) = self.property_access_receiver(idx) else {
                    continue;
                };
                if let Some(container_name) = target_member_container_hint.as_deref()
                    && !self.receiver_matches_container(receiver_idx, container_name)
                {
                    continue;
                } else if target_member_container_hint.is_none() {
                    continue;
                }
            } else if !self.is_inside_call_or_decorator_reference(idx) {
                // Function-like targets should remain call-expression driven.
                continue;
            } else if target_kind == SymbolKind::Function && self.is_property_access_name(idx) {
                // Function targets should not treat `obj.sameName()` member calls as
                // references to a same-named free function.
                continue;
            }

            if !target_is_member_like
                && !self.is_inside_decorator_reference(idx)
                && let Some(target_symbol) = target_symbol_id
            {
                let reference_symbol = self
                    .binder
                    .node_symbols
                    .get(&idx.0)
                    .copied()
                    .or_else(|| self.resolve_callee_symbol(idx).map(|(sym, _, _)| sym));
                if let Some(reference_symbol) = reference_symbol
                    && reference_symbol != target_symbol
                {
                    continue;
                }
            }
            if !target_is_member_like
                && target_kind == SymbolKind::Function
                && let Some(target_namespace) = target_namespace_hint.as_deref()
                && !self.is_property_access_name(idx)
                && self
                    .enclosing_namespace_name(idx)
                    .as_deref()
                    .is_some_and(|caller_namespace| caller_namespace != target_namespace)
            {
                continue;
            }

            // Walk up to find the containing function
            let range = node_range(self.arena, self.line_map, self.source_text, idx);
            if let Some(containing_func) = self.find_containing_function(idx) {
                // Don't list the function as calling itself from its own declaration
                if containing_func == func_idx {
                    continue;
                }
                let caller_idx = self
                    .class_parent_for_constructor(containing_func)
                    .unwrap_or(containing_func);
                callers.entry(caller_idx.0).or_default().push(range);
            } else if let Some(caller_idx) = self.decorated_declaration_caller(idx) {
                callers.entry(caller_idx.0).or_default().push(range);
            } else {
                script_from_ranges.push(range);
            }
        }

        // Convert grouped callers into CallHierarchyIncomingCall items
        results.reserve(callers.len() + usize::from(!script_from_ranges.is_empty()));
        for (caller_idx_raw, ranges) in callers {
            let caller_idx = NodeIndex(caller_idx_raw);
            if let Some(item) = self.make_call_hierarchy_item_for_caller(caller_idx) {
                results.push(CallHierarchyIncomingCall {
                    from: item,
                    from_ranges: ranges,
                });
            }
        }

        if !script_from_ranges.is_empty() {
            results.push(CallHierarchyIncomingCall {
                from: self.script_call_hierarchy_item(),
                from_ranges: script_from_ranges,
            });
        }

        results
    }

    /// Issue #3753: find callers in this file that invoke `target_name` and
    /// whose binding resolves to `target_decl_idx` (typically the local
    /// import binding for a cross-file target). Mirrors the within-file
    /// scan that `incoming_calls` performs but is parameterized by the
    /// declaration node directly so callers don't need to be a function-
    /// like node — `import { x } from "./a"` is a valid input here even
    /// though the import binding is not itself callable in this file.
    pub fn incoming_calls_for_decl_in_file(
        &self,
        target_decl_idx: NodeIndex,
        target_name: &str,
    ) -> Vec<CallHierarchyIncomingCall> {
        let mut results = Vec::new();

        let target_symbol_id = self
            .binder
            .get_node_symbol(target_decl_idx)
            .or_else(|| self.binder.file_locals.get(target_name));

        let mut callers: FxHashMap<u32, Vec<Range>> = FxHashMap::default();
        let mut script_from_ranges = Vec::new();

        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let idx = NodeIndex(i as u32);
            if idx == target_decl_idx {
                continue;
            }
            let ident_data = match self.arena.get_identifier(node) {
                Some(d) => d,
                None => continue,
            };
            if ident_data.escaped_text != target_name {
                continue;
            }
            if !self.is_inside_call_or_decorator_reference(idx) {
                continue;
            }
            if self.is_property_access_name(idx) {
                continue;
            }
            // Symbol cross-check: when the binder agrees on a target symbol,
            // require the call site to resolve to the same one. This rejects
            // local same-name shadowing that doesn't go through the import.
            if let Some(target_symbol) = target_symbol_id {
                let reference_symbol = self
                    .binder
                    .node_symbols
                    .get(&idx.0)
                    .copied()
                    .or_else(|| self.resolve_callee_symbol(idx).map(|(sym, _, _)| sym));
                if let Some(reference_symbol) = reference_symbol
                    && reference_symbol != target_symbol
                {
                    continue;
                }
            }

            let range = node_range(self.arena, self.line_map, self.source_text, idx);
            if let Some(containing_func) = self.find_containing_function(idx) {
                let caller_idx = self
                    .class_parent_for_constructor(containing_func)
                    .unwrap_or(containing_func);
                callers.entry(caller_idx.0).or_default().push(range);
            } else if let Some(caller_idx) = self.decorated_declaration_caller(idx) {
                callers.entry(caller_idx.0).or_default().push(range);
            } else {
                script_from_ranges.push(range);
            }
        }

        results.reserve(callers.len() + usize::from(!script_from_ranges.is_empty()));
        for (caller_idx_raw, ranges) in callers {
            let caller_idx = NodeIndex(caller_idx_raw);
            if let Some(item) = self.make_call_hierarchy_item_for_caller(caller_idx) {
                results.push(CallHierarchyIncomingCall {
                    from: item,
                    from_ranges: ranges,
                });
            }
        }
        if !script_from_ranges.is_empty() {
            results.push(CallHierarchyIncomingCall {
                from: self.script_call_hierarchy_item(),
                from_ranges: script_from_ranges,
            });
        }
        results
    }

    /// Issue #3753 (namespace imports): find callers in this file that
    /// invoke `<namespace>.<member_name>(…)` where the namespace
    /// identifier resolves to `namespace_decl_idx` (typically a
    /// `NAMESPACE_IMPORT` binding from the importing file).
    ///
    /// Mirrors `incoming_calls_for_decl_in_file` but scans for property-
    /// access call sites instead of bare identifier call sites — needed
    /// because `ns.target()` is not a direct reference to `target`, it
    /// is a member access whose receiver is the imported namespace.
    pub fn incoming_calls_for_namespace_member(
        &self,
        namespace_decl_idx: NodeIndex,
        member_name: &str,
    ) -> Vec<CallHierarchyIncomingCall> {
        let mut results = Vec::new();
        let namespace_local = self
            .arena
            .get(namespace_decl_idx)
            .and_then(|n| self.arena.get_identifier(n))
            .map(|id| id.escaped_text.clone());
        let Some(namespace_local) = namespace_local else {
            return results;
        };
        let namespace_symbol = self.binder.get_node_symbol(namespace_decl_idx);

        let mut callers: FxHashMap<u32, Vec<Range>> = FxHashMap::default();
        let mut script_from_ranges = Vec::new();

        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let idx = NodeIndex(i as u32);
            // Match the property-access *name* — `ns.target` puts
            // `target` here as the name, `ns` as the receiver.
            if !self.is_property_access_name(idx) {
                continue;
            }
            let ident_data = match self.arena.get_identifier(node) {
                Some(d) => d,
                None => continue,
            };
            if ident_data.escaped_text != member_name {
                continue;
            }
            let Some(receiver_idx) = self.property_access_receiver(idx) else {
                continue;
            };
            let Some(recv_node) = self.arena.get(receiver_idx) else {
                continue;
            };
            if recv_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(recv_ident) = self.arena.get_identifier(recv_node) else {
                continue;
            };
            if recv_ident.escaped_text != namespace_local {
                continue;
            }
            // When the binder records a symbol for the receiver, require
            // it to match the namespace decl's symbol. Rejects unrelated
            // same-named locals that shadow the namespace.
            if let Some(target_sym) = namespace_symbol {
                let receiver_sym = self
                    .binder
                    .node_symbols
                    .get(&receiver_idx.0)
                    .copied()
                    .or_else(|| self.resolve_callee_symbol(receiver_idx).map(|(s, _, _)| s));
                if let Some(receiver_sym) = receiver_sym
                    && receiver_sym != target_sym
                {
                    continue;
                }
            }
            // The call site must be a CallExpression with this property
            // access as its callee.
            if !self.is_inside_call_or_decorator_reference(idx) {
                continue;
            }

            let range = node_range(self.arena, self.line_map, self.source_text, idx);
            if let Some(containing_func) = self.find_containing_function(idx) {
                let caller_idx = self
                    .class_parent_for_constructor(containing_func)
                    .unwrap_or(containing_func);
                callers.entry(caller_idx.0).or_default().push(range);
            } else if let Some(caller_idx) = self.decorated_declaration_caller(idx) {
                callers.entry(caller_idx.0).or_default().push(range);
            } else {
                script_from_ranges.push(range);
            }
        }

        results.reserve(callers.len() + usize::from(!script_from_ranges.is_empty()));
        for (caller_idx_raw, ranges) in callers {
            let caller_idx = NodeIndex(caller_idx_raw);
            if let Some(item) = self.make_call_hierarchy_item_for_caller(caller_idx) {
                results.push(CallHierarchyIncomingCall {
                    from: item,
                    from_ranges: ranges,
                });
            }
        }
        if !script_from_ranges.is_empty() {
            results.push(CallHierarchyIncomingCall {
                from: self.script_call_hierarchy_item(),
                from_ranges: script_from_ranges,
            });
        }
        results
    }

    /// Find all outgoing calls from the function at the given position.
    ///
    /// Walks the function's body AST looking for `CallExpression` nodes,
    /// then resolves each callee to build outgoing call items.
    pub fn outgoing_calls(
        &self,
        _root: NodeIndex,
        position: Position,
    ) -> Vec<CallHierarchyOutgoingCall> {
        let mut results = Vec::new();

        let offset = match self.line_map.position_to_offset(position, self.source_text) {
            Some(o) => o,
            None => return results,
        };

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return results;
        }

        // Find the target callable at this position. Prefer explicit symbol
        // resolution from callsites (e.g. `bar()` -> `const bar = function(){}`).
        let mut func_idx = self
            .resolve_reference_callable(node_idx)
            .or_else(|| self.find_function_at_or_around(node_idx));
        if func_idx.is_none() {
            func_idx = self.export_equals_anonymous_function_callable();
        }
        let func_idx = match func_idx {
            Some(idx) => idx,
            None => return results,
        };

        let prepared_bounds = self.prepare(_root, position).and_then(|item| {
            if item.kind == SymbolKind::Module {
                return None;
            }
            let start = self
                .line_map
                .position_to_offset(item.range.start, self.source_text)?;
            let end = self
                .line_map
                .position_to_offset(item.range.end, self.source_text)?;
            Some((start, end))
        });

        // Collect all CallExpression nodes within the callable's body bounds.
        let mut call_nodes = Vec::new();
        if let Some((start, end)) = prepared_bounds.or_else(|| self.callable_range_bounds(func_idx))
        {
            self.collect_call_expressions_in_bounds(start, end, &mut call_nodes);
        } else {
            let body_idx = match self.get_function_body(func_idx) {
                Some(idx) => idx,
                None => return results,
            };
            self.collect_call_expressions(body_idx, &mut call_nodes);
        }

        // Group calls by the resolved callee.
        // Key: NodeIndex of the callee's declaration, Value: aggregated callee info.
        let mut callees: FxHashMap<u32, OutgoingCalleeEntry> = FxHashMap::default();

        for call_idx in call_nodes {
            if !self.call_expression_belongs_to_callable(call_idx, func_idx) {
                continue;
            }

            let call_node = match self.arena.get(call_idx) {
                Some(n) => n,
                None => continue,
            };

            let call_data = match self.arena.get_call_expr(call_node) {
                Some(d) => d,
                None => continue,
            };

            // Get the callee expression (the thing being called)
            let callee_expr = call_data.expression;
            if callee_expr.is_none() {
                continue;
            }

            // Determine the identifier to resolve
            let callee_ident = self.get_callee_identifier(callee_expr);
            if callee_ident.is_none() {
                continue;
            }

            // Build the call-site range (the range of the callee expression in the source)
            let call_range = node_range(self.arena, self.line_map, self.source_text, callee_ident);

            // Resolve the callee to its declaration using escaped_text and the
            // binder's symbol tables. This avoids depending on
            // ScopeWalker::resolve_node which requires the arena interner to be
            // populated (not the case when the arena is borrowed).
            let resolved = self.resolve_callee_symbol(callee_ident);

            if let Some((symbol_id, decl_idx, name)) = resolved {
                let _ = symbol_id; // used implicitly via decl_idx
                let import_resolution = self.import_resolution_request_for_decl(decl_idx, &name);
                let entry = callees.entry(decl_idx.0).or_insert_with(|| {
                    let item = self.make_call_hierarchy_item_for_declaration(decl_idx, &name);
                    (item, Vec::new(), import_resolution)
                });
                entry.1.push(call_range);
            } else if let Some((decl_idx, name)) =
                self.resolve_static_block_local_symbol(callee_ident, func_idx)
            {
                let entry = callees.entry(decl_idx.0).or_insert_with(|| {
                    let item = self.make_call_hierarchy_item_for_declaration(decl_idx, &name);
                    (item, Vec::new(), None)
                });
                entry.1.push(call_range);
            } else if let Some(ident_node) = self.arena.get(callee_ident) {
                // Last-resort fallback: no symbol found at all
                if let Some(ident_data) = self.arena.get_identifier(ident_node) {
                    let name = ident_data.escaped_text.clone();
                    let ident_range =
                        node_range(self.arena, self.line_map, self.source_text, callee_ident);
                    let entry = callees.entry(callee_ident.0).or_insert_with(|| {
                        let item = CallHierarchyItem {
                            name,
                            kind: SymbolKind::Function,
                            uri: self.file_name.clone(),
                            range: ident_range,
                            selection_range: ident_range,
                            container_name: None,
                        };
                        (Some(item), Vec::new(), None)
                    });
                    entry.1.push(call_range);
                }
            }
        }

        // Convert grouped callees into CallHierarchyOutgoingCall items
        results.reserve(callees.len());
        for (_decl_idx, (item_opt, ranges, import_resolution)) in callees {
            if let Some(item) = item_opt {
                results.push(CallHierarchyOutgoingCall {
                    to: item,
                    from_ranges: ranges,
                    import_resolution,
                });
            }
        }

        results.sort_by(|a, b| {
            let a_start = a
                .from_ranges
                .iter()
                .map(|r| (r.start.line, r.start.character))
                .min()
                .unwrap_or((u32::MAX, u32::MAX));
            let b_start = b
                .from_ranges
                .iter()
                .map(|r| (r.start.line, r.start.character))
                .min()
                .unwrap_or((u32::MAX, u32::MAX));
            a_start
                .cmp(&b_start)
                .then_with(|| a.to.name.cmp(&b.to.name))
        });

        results
    }

    fn call_expression_belongs_to_callable(
        &self,
        call_idx: NodeIndex,
        callable_idx: NodeIndex,
    ) -> bool {
        let mut current = call_idx;
        loop {
            let Some(ext) = self.arena.get_extended(current) else {
                return true;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return callable_idx.is_none();
            }
            let Some(parent_node) = self.arena.get(parent) else {
                return false;
            };
            if parent_node.is_function_like()
                || Self::is_call_hierarchy_callable_kind(parent_node.kind)
            {
                return parent == callable_idx;
            }
            current = parent;
        }
    }

    fn resolve_static_block_local_symbol(
        &self,
        ident_idx: NodeIndex,
        callable_idx: NodeIndex,
    ) -> Option<(NodeIndex, String)> {
        let static_block_idx = if self
            .arena
            .get(callable_idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION)
        {
            callable_idx
        } else {
            self.enclosing_static_block(callable_idx)?
        };
        let name = self.get_identifier_text(ident_idx)?;
        let block_node = self.arena.get(static_block_idx)?;
        let block = self.arena.get_block(block_node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                continue;
            }
            let function = self.arena.get_function(stmt_node)?;
            if !function.name.is_some() {
                continue;
            }
            if self.get_identifier_text(function.name).as_deref() == Some(name.as_str()) {
                return Some((stmt_idx, name));
            }
        }
        None
    }

    /// Get the name of the target function at the given position.
    /// Used by cross-file incoming call resolution.
    pub fn get_target_function_name(&self, _root: NodeIndex, position: Position) -> Option<String> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let func_idx = self
            .resolve_reference_callable(node_idx)
            .or_else(|| self.find_function_at_or_around(node_idx))
            .or_else(|| self.export_equals_anonymous_function_callable())?;

        let kind = self.get_function_symbol_kind(func_idx);
        if kind == SymbolKind::Constructor {
            self.constructor_target_name(func_idx)
        } else {
            self.get_function_name_idx(func_idx)
                .and_then(|idx| self.get_identifier_text(idx))
        }
    }

    /// Find all call sites of a function named `target_name` in this file,
    /// grouped by containing function. Used for cross-file incoming calls.
    pub fn find_incoming_calls_by_name(&self, target_name: &str) -> Vec<CallHierarchyIncomingCall> {
        let mut callers: FxHashMap<u32, Vec<Range>> = FxHashMap::default();
        let mut script_from_ranges = Vec::new();

        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let idx = NodeIndex(i as u32);

            let ident_data = match self.arena.get_identifier(node) {
                Some(d) => d,
                None => continue,
            };
            if ident_data.escaped_text != target_name {
                continue;
            }

            // Only consider identifiers that appear in call expressions
            if !self.is_inside_call_or_decorator_reference(idx) {
                continue;
            }

            let range = node_range(self.arena, self.line_map, self.source_text, idx);
            if let Some(containing_func) = self.find_containing_function(idx) {
                let caller_idx = self
                    .class_parent_for_constructor(containing_func)
                    .unwrap_or(containing_func);
                callers.entry(caller_idx.0).or_default().push(range);
            } else {
                script_from_ranges.push(range);
            }
        }

        let mut results = Vec::new();
        for (caller_idx_raw, ranges) in callers {
            let caller_idx = NodeIndex(caller_idx_raw);
            if let Some(item) = self.make_call_hierarchy_item_for_caller(caller_idx) {
                results.push(CallHierarchyIncomingCall {
                    from: item,
                    from_ranges: ranges,
                });
            }
        }

        if !script_from_ranges.is_empty() {
            results.push(CallHierarchyIncomingCall {
                from: self.script_call_hierarchy_item(),
                from_ranges: script_from_ranges,
            });
        }

        results
    }

    /// Resolve a callee identifier to its symbol using `escaped_text` and
    /// the binder's symbol tables directly. This avoids the `ScopeWalker`
    /// path which depends on `arena.get_identifier_text()` (atom-based).
    ///
    /// Returns `(SymbolId, declaration NodeIndex, name)` on success.
    fn resolve_callee_symbol(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<(tsz_binder::SymbolId, NodeIndex, String)> {
        // 1. Check if the node itself is a declaration in node_symbols
        if let Some(&sym_id) = self.binder.node_symbols.get(&ident_idx.0)
            && let Some(symbol) = self.binder.symbols.get(sym_id)
            && let Some(&decl) = symbol.declarations.first()
        {
            return Some((sym_id, decl, symbol.escaped_name.clone()));
        }

        // 2. Get the identifier's escaped_text and look it up in file_locals
        let node = self.arena.get(ident_idx)?;
        let ident_data = self.arena.get_identifier(node)?;
        let name = &ident_data.escaped_text;

        if let Some(sym_id) = self.binder.file_locals.get(name)
            && let Some(symbol) = self.binder.symbols.get(sym_id)
            && let Some(&decl) = symbol.declarations.first()
        {
            return Some((sym_id, decl, name.clone()));
        }

        None
    }

    /// Find the function-like node at or containing the given node.
    ///
    /// First checks whether `node_idx` itself is a function-like node or the
    /// name of one. Then walks up through parents.
    fn find_function_at_or_around(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        if node_idx.is_none() {
            return None;
        }

        let node = self.arena.get(node_idx)?;

        // If we are directly on a function-like node, return it.
        if node.is_function_like() || Self::is_call_hierarchy_callable_kind(node.kind) {
            return Some(node_idx);
        }

        // If the node is an identifier, check if its parent is a function-like
        // declaration (i.e., we are on the function name).
        if node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.arena.get_extended(node_idx)
        {
            let parent = ext.parent;
            if parent.is_some()
                && let Some(parent_node) = self.arena.get(parent)
                && (parent_node.is_class_like())
                && let Some(class_decl) = self.arena.get_class(parent_node)
                && class_decl.name == node_idx
                && let Some(ctor_idx) = self.class_constructor_node(parent)
            {
                return Some(ctor_idx);
            }
            if let Some(property_initializer) =
                self.property_initializer_function_for_name(node_idx, parent)
            {
                return Some(property_initializer);
            }
            if let Some(variable_initializer) =
                self.variable_initializer_function_for_name(node_idx, parent)
            {
                return Some(variable_initializer);
            }
            if parent.is_some()
                && let Some(parent_node) = self.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::METHOD_SIGNATURE
            {
                return Some(parent);
            }
            if parent.is_some()
                && let Some(parent_node) = self.arena.get(parent)
                && parent_node.is_function_like()
            {
                return Some(parent);
            }
        }

        // Walk up through parents to find an enclosing function-like node.
        self.find_containing_function(node_idx)
    }

    fn property_initializer_function_for_name(
        &self,
        ident_idx: NodeIndex,
        parent_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if parent_idx.is_none() {
            return None;
        }
        let parent = self.arena.get(parent_idx)?;
        if parent.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return None;
        }
        let prop = self.arena.get_property_decl(parent)?;
        if prop.name != ident_idx || prop.initializer.is_none() {
            return None;
        }
        let init_node = self.arena.get(prop.initializer)?;
        init_node.is_function_like().then_some(prop.initializer)
    }

    fn variable_initializer_function_for_name(
        &self,
        ident_idx: NodeIndex,
        parent_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if parent_idx.is_none() {
            return None;
        }
        let parent = self.arena.get(parent_idx)?;
        if parent.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(parent)?;
        if var_decl.name != ident_idx || var_decl.initializer.is_none() {
            return None;
        }
        let init_node = self.arena.get(var_decl.initializer)?;
        init_node.is_function_like().then_some(var_decl.initializer)
    }

    fn property_declaration_for_function_initializer(
        &self,
        func_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if func_idx.is_none() {
            return None;
        }
        let ext = self.arena.get_extended(func_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent = self.arena.get(parent_idx)?;
        if parent.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return None;
        }
        let prop = self.arena.get_property_decl(parent)?;
        (prop.initializer == func_idx).then_some(parent_idx)
    }

    fn variable_declaration_for_function_initializer(
        &self,
        func_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if func_idx.is_none() {
            return None;
        }
        let ext = self.arena.get_extended(func_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent = self.arena.get(parent_idx)?;
        if parent.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(parent)?;
        (var_decl.initializer == func_idx).then_some(parent_idx)
    }

    fn property_name_selection_range(&self, prop_idx: NodeIndex) -> Option<Range> {
        let prop_node = self.arena.get(prop_idx)?;
        let prop_decl = self.arena.get_property_decl(prop_node)?;
        self.identifier_selection_range(prop_decl.name)
    }

    fn identifier_selection_range(&self, ident_idx: NodeIndex) -> Option<Range> {
        let name_text = self.get_identifier_text(ident_idx)?;
        let name_node = self.arena.get(ident_idx)?;
        let start_offset = name_node.pos;
        let end_offset = start_offset.saturating_add(name_text.len() as u32);
        Some(Range::new(
            self.line_map
                .offset_to_position(start_offset, self.source_text),
            self.line_map
                .offset_to_position(end_offset, self.source_text),
        ))
    }

    fn find_function_body_end_offset_from_source(&self, func_start_offset: u32) -> Option<u32> {
        let bytes = self.source_text.as_bytes();
        let mut i = func_start_offset as usize;
        while i < bytes.len() && bytes[i] != b'{' {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let mut depth = 0i32;
        for (idx, ch) in bytes.iter().enumerate().skip(i) {
            if *ch == b'{' {
                depth += 1;
            } else if *ch == b'}' {
                depth -= 1;
                if depth == 0 {
                    return Some((idx + 1) as u32);
                }
            }
        }
        None
    }

    fn callable_range_bounds(&self, func_idx: NodeIndex) -> Option<(u32, u32)> {
        if let Some(prop_idx) = self.property_declaration_for_function_initializer(func_idx) {
            let prop_node = self.arena.get(prop_idx)?;
            let prop_decl = self.arena.get_property_decl(prop_node)?;
            let init_node = self.arena.get(prop_decl.initializer)?;
            let start = init_node.pos;
            let end = self
                .find_function_body_end_offset_from_source(start)
                .unwrap_or(init_node.end);
            return Some((start, end));
        }

        let body = self.get_function_body(func_idx)?;
        let body_node = self.arena.get(body)?;
        let func_node = self.arena.get(func_idx)?;
        let start = func_node.pos;
        let end = self
            .find_function_body_end_offset_from_source(start)
            .unwrap_or(body_node.end);
        Some((start, end))
    }

    fn container_name_for_callable(&self, callable_idx: NodeIndex) -> Option<String> {
        let callable_node = self.arena.get(callable_idx)?;
        if callable_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            || self.enclosing_static_block(callable_idx).is_some()
        {
            return None;
        }

        let mut current = callable_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.is_class_like() {
                let class_decl = self.arena.get_class(parent_node)?;
                if class_decl.name.is_some() {
                    return self.get_identifier_text(class_decl.name);
                }
                return Some("<class>".to_string());
            }
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                let module_decl = self.arena.get_module(parent_node)?;
                if module_decl.name.is_some() {
                    return self.get_identifier_text(module_decl.name);
                }
            }
            current = parent;
        }
    }

    fn enclosing_static_block(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                return Some(parent);
            }
            if parent_node.is_class_like() {
                return None;
            }
            current = parent;
        }
    }

    fn class_constructor_node(&self, class_idx: NodeIndex) -> Option<NodeIndex> {
        let class_node = self.arena.get(class_idx)?;
        let class_decl = self.arena.get_class(class_node)?;
        for &member_idx in class_decl.members.nodes.iter() {
            let member_node = self.arena.get(member_idx)?;
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                return Some(member_idx);
            }
        }
        None
    }

    fn class_parent_for_constructor(&self, func_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(func_idx)?;
        if node.kind != syntax_kind_ext::CONSTRUCTOR {
            return None;
        }
        let mut current = func_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.is_class_like() {
                return Some(parent);
            }
            current = parent;
        }
    }

    fn constructor_target_name(&self, func_idx: NodeIndex) -> Option<String> {
        let class_idx = self.class_parent_for_constructor(func_idx)?;
        let class_node = self.arena.get(class_idx)?;
        let class_decl = self.arena.get_class(class_node)?;
        self.get_identifier_text(class_decl.name)
    }

    /// Walk up the parent chain to find the nearest containing function-like node.
    fn find_containing_function(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.is_function_like()
                || Self::is_call_hierarchy_callable_kind(parent_node.kind)
            {
                return Some(parent);
            }
            current = parent;
        }
    }

    fn is_property_access_name(&self, node_idx: NodeIndex) -> bool {
        let Some(ext) = self.arena.get_extended(node_idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }
        let Some(parent_node) = self.arena.get(parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        self.arena
            .get_access_expr(parent_node)
            .is_some_and(|access| access.name_or_argument == node_idx)
    }

    fn property_access_receiver(&self, name_idx: NodeIndex) -> Option<NodeIndex> {
        if !self.is_property_access_name(name_idx) {
            return None;
        }
        let ext = self.arena.get_extended(name_idx)?;
        let parent = ext.parent;
        let parent_node = self.arena.get(parent)?;
        let access = self.arena.get_access_expr(parent_node)?;
        Some(access.expression)
    }

    fn receiver_matches_container(&self, receiver_idx: NodeIndex, container_name: &str) -> bool {
        let Some(receiver_node) = self.arena.get(receiver_idx) else {
            return false;
        };

        match receiver_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .get_identifier_text(receiver_idx)
                .is_some_and(|text| text == container_name),
            k if k == SyntaxKind::ThisKeyword as u16 || k == SyntaxKind::SuperKeyword as u16 => {
                true
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => self
                .arena
                .get_call_expr(receiver_node)
                .and_then(|call| {
                    let callee_ident = self.get_callee_identifier(call.expression);
                    self.get_identifier_text(callee_ident)
                })
                .is_some_and(|text| text == container_name),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(receiver_node)
                    && self
                        .get_identifier_text(access.name_or_argument)
                        .is_some_and(|text| text == container_name)
                {
                    return true;
                }
                self.arena
                    .get_access_expr(receiver_node)
                    .is_some_and(|access| {
                        self.receiver_matches_container(access.expression, container_name)
                    })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .arena
                .get_parenthesized(receiver_node)
                .is_some_and(|expr| {
                    self.receiver_matches_container(expr.expression, container_name)
                }),
            _ => false,
        }
    }

    fn member_container_hint_for_callable(&self, callable_idx: NodeIndex) -> Option<String> {
        if let Some(class_name) = self.container_name_for_callable(callable_idx) {
            return Some(class_name);
        }

        let mut current = callable_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                let var_decl = self.arena.get_variable_declaration(parent_node)?;
                if var_decl.initializer == current {
                    let current_node = self.arena.get(current)?;
                    if current_node.is_function_like() {
                        return None;
                    }
                }
                return self.get_identifier_text(var_decl.name);
            }
            current = parent;
        }
    }

    fn is_inside_decorator_reference(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        for _ in 0..8 {
            let Some(ext) = self.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::DECORATOR {
                return true;
            }
            current = parent;
        }
        false
    }

    fn decorated_declaration_caller(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        if !self.is_inside_decorator_reference(node_idx) {
            return None;
        }
        let mut current = node_idx;
        for _ in 0..12 {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.is_class_like() {
                return Some(parent);
            }
            if parent_node.is_function_like()
                || Self::is_call_hierarchy_callable_kind(parent_node.kind)
            {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    fn enclosing_namespace_name(&self, node_idx: NodeIndex) -> Option<String> {
        let mut current = node_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                let module_decl = self.arena.get_module(parent_node)?;
                if module_decl.name.is_some() {
                    return self.get_identifier_text(module_decl.name);
                }
            }
            current = parent;
        }
    }

    /// Check if a node is used in a call-like reference context.
    ///
    /// Includes:
    /// - `CallExpression`/`NewExpression`/`TaggedTemplateExpression` references
    /// - Decorator references (`@foo`, `@foo()`, `@ns.foo`)
    fn is_inside_call_or_decorator_reference(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        // Walk up a few levels to see if we hit a reference context.
        for _ in 0..8 {
            if let Some(ext) = self.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    return false;
                }
                if let Some(parent_node) = self.arena.get(parent) {
                    if parent_node.kind == syntax_kind_ext::CALL_EXPRESSION {
                        return true;
                    }
                    // Also count NewExpression as a "call"
                    if parent_node.kind == syntax_kind_ext::NEW_EXPRESSION {
                        return true;
                    }
                    if parent_node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                        return true;
                    }
                    if parent_node.kind == syntax_kind_ext::DECORATOR {
                        return true;
                    }
                }
                current = parent;
            } else {
                return false;
            }
        }
        false
    }

    /// Get the name `NodeIndex` of a function-like node.
    fn get_function_name_idx(&self, func_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(func_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                let func = self.arena.get_function(node)?;
                if func.name.is_some() {
                    return Some(func.name);
                }
                if let Some(prop_idx) = self.property_declaration_for_function_initializer(func_idx)
                    && let Some(prop_node) = self.arena.get(prop_idx)
                    && let Some(prop_decl) = self.arena.get_property_decl(prop_node)
                {
                    return Some(prop_decl.name);
                }
                if let Some(var_idx) = self.variable_declaration_for_function_initializer(func_idx)
                    && let Some(var_node) = self.arena.get(var_idx)
                    && let Some(var_decl) = self.arena.get_variable_declaration(var_node)
                {
                    return Some(var_decl.name);
                }
                None
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                if method.name.is_none() {
                    None
                } else {
                    Some(method.name)
                }
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                let signature = self.arena.get_signature(node)?;
                if signature.name.is_none() {
                    None
                } else {
                    Some(signature.name)
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                // Constructors don't have a name node, return the function node itself
                None
            }
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => None,
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(node)?;
                if accessor.name.is_none() {
                    None
                } else {
                    Some(accessor.name)
                }
            }
            _ => None,
        }
    }

    /// Get the body `NodeIndex` of a function-like node.
    fn get_function_body(&self, func_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(func_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                let func = self.arena.get_function(node)?;
                if func.body.is_none() {
                    None
                } else {
                    Some(func.body)
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                if method.body.is_none() {
                    None
                } else {
                    Some(method.body)
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                let ctor = self.arena.get_constructor(node)?;
                if ctor.body.is_none() {
                    None
                } else {
                    Some(ctor.body)
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(node)?;
                if accessor.body.is_none() {
                    None
                } else {
                    Some(accessor.body)
                }
            }
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => Some(func_idx),
            _ => None,
        }
    }

    /// Recursively collect all call-like expression nodes within a subtree.
    ///
    /// Uses a simple offset-range scan: any node in the arena whose kind is
    /// `CALL_EXPRESSION`/`NEW_EXPRESSION` and whose [pos, end) is within the body range is
    /// collected. This avoids the need for a full recursive visitor.
    fn collect_call_expressions(&self, body_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let body_node = match self.arena.get(body_idx) {
            Some(n) => n,
            None => return,
        };
        let body_start = body_node.pos;
        let body_end = body_node.end;

        for (i, node) in self.arena.nodes.iter().enumerate() {
            if (node.kind == syntax_kind_ext::CALL_EXPRESSION
                || node.kind == syntax_kind_ext::NEW_EXPRESSION)
                && node.pos >= body_start
                && node.end <= body_end
            {
                out.push(NodeIndex(i as u32));
            }
        }
    }

    fn collect_call_expressions_in_bounds(&self, start: u32, end: u32, out: &mut Vec<NodeIndex>) {
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if (node.kind == syntax_kind_ext::CALL_EXPRESSION
                || node.kind == syntax_kind_ext::NEW_EXPRESSION)
                && node.pos >= start
                && node.end <= end
            {
                out.push(NodeIndex(i as u32));
            }
        }
    }

    /// Extract the callee identifier from a call expression's `expression` field.
    ///
    /// Handles:
    /// - Simple identifiers: `foo()` -> `foo`
    /// - Property access: `obj.method()` -> `method`
    fn get_callee_identifier(&self, expr_idx: NodeIndex) -> NodeIndex {
        if expr_idx.is_none() {
            return NodeIndex::NONE;
        }
        let expr_node = match self.arena.get(expr_idx) {
            Some(n) => n,
            None => return NodeIndex::NONE,
        };

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => expr_idx,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // For property access (a.b), the identifier is name_or_argument
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    if access.name_or_argument.is_some() {
                        access.name_or_argument
                    } else {
                        NodeIndex::NONE
                    }
                } else {
                    NodeIndex::NONE
                }
            }
            _ => NodeIndex::NONE,
        }
    }
}
