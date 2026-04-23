//! Extract Function refactoring for the LSP.
//!
//! Extracts a selected range of statements into a new function, detecting
//! variables that must become parameters (used but declared outside) and
//! variables that must become return values (assigned inside and used after).

use crate::rename::{TextEdit, WorkspaceEdit};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Extract the selected statements into a new function.
    ///
    /// The selection must cover one or more complete statements inside a block
    /// or source file. Variables referenced but declared outside become
    /// parameters; variables assigned inside and referenced after become return
    /// values.
    pub fn extract_function(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        // 1. Convert range to offsets
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let end_offset = self.line_map.position_to_offset(range.end, self.source)?;

        // 2. Find the containing block / source-file and the statements in the selection
        let (container_idx, stmt_indices) =
            self.find_selected_statements(root, start_offset, end_offset)?;

        if stmt_indices.is_empty() {
            return None;
        }

        // 3. Compute the span of the selected statements (first.pos .. last.end)
        let first_node = self.arena.get(*stmt_indices.first()?)?;
        let last_node = self.arena.get(*stmt_indices.last()?)?;
        let selection_start = first_node.pos;
        let selection_end = last_node.end;

        // 4. Collect declared names inside the selection
        let mut declared_in_selection: FxHashMap<String, NodeIndex> = FxHashMap::default();
        for &stmt_idx in &stmt_indices {
            self.collect_declared_names(stmt_idx, &mut declared_in_selection);
        }

        // 5. Collect all identifier references inside the selection
        let mut refs_in_selection: Vec<(String, NodeIndex)> = Vec::new();
        for &stmt_idx in &stmt_indices {
            self.collect_all_identifier_refs(stmt_idx, &mut refs_in_selection);
        }

        // 6. Determine parameters: referenced inside but declared outside
        let mut param_names: Vec<String> = Vec::new();
        let mut param_set: FxHashSet<String> = FxHashSet::default();
        for (name, _ident_idx) in &refs_in_selection {
            if declared_in_selection.contains_key(name) {
                continue;
            }
            if param_set.insert(name.clone()) {
                param_names.push(name.clone());
            }
        }

        // 7. Determine return values: declared (or assigned) inside and referenced
        //    after the selection in the same container
        let return_names = self.find_return_variables(
            container_idx,
            &stmt_indices,
            &declared_in_selection,
            selection_end,
        );

        // 8. Generate a unique function name
        let func_name = self.unique_function_name(container_idx);

        // 9. Build the extracted function body text
        let body_text = self
            .source
            .get(selection_start as usize..selection_end as usize)?;

        // Compute indentation
        let first_stmt_pos = self
            .line_map
            .offset_to_position(selection_start, self.source);
        let body_indent = self.get_indentation_at_position(&first_stmt_pos);

        // The function will be placed at the top-level indent of the container
        let container_node = self.arena.get(container_idx)?;
        let container_pos = self
            .line_map
            .offset_to_position(container_node.pos, self.source);
        let container_indent = if container_node.kind == syntax_kind_ext::SOURCE_FILE {
            String::new()
        } else {
            self.get_indentation_at_position(&container_pos)
        };

        let func_indent = container_indent;
        let func_body_indent = format!("{func_indent}  ");

        // Re-indent the body: strip the original indentation and apply the new one
        let mut reindented_lines = Vec::new();
        for line in body_text.lines() {
            let stripped = if line.starts_with(&body_indent) {
                &line[body_indent.len()..]
            } else {
                line.trim_start()
            };
            if stripped.is_empty() {
                reindented_lines.push(String::new());
            } else {
                reindented_lines.push(format!("{func_body_indent}{stripped}"));
            }
        }

        // Add return statement if needed
        if !return_names.is_empty() {
            if return_names.len() == 1 {
                reindented_lines.push(format!("{func_body_indent}return {};", return_names[0]));
            } else {
                let obj_fields = return_names.join(", ");
                reindented_lines.push(format!("{func_body_indent}return {{ {obj_fields} }};"));
            }
        }

        let params_str = param_names.join(", ");
        let func_body = reindented_lines.join("\n");

        let function_text = format!(
            "{func_indent}function {func_name}({params_str}) {{\n{func_body}\n{func_indent}}}\n"
        );

        // 10. Build the call-site replacement text
        let call_args = param_names.join(", ");
        let call_expr = if return_names.is_empty() {
            format!("{func_name}({call_args});")
        } else if return_names.len() == 1 {
            format!("const {} = {func_name}({call_args});", return_names[0])
        } else {
            let destructure = return_names.join(", ");
            format!("const {{ {destructure} }} = {func_name}({call_args});")
        };

        let call_text = format!("{body_indent}{call_expr}");

        // 11. Build text edits
        let mut edits = Vec::new();

        // a) Replace the selected statements with the function call
        let replace_start = self
            .line_map
            .offset_to_position(selection_start, self.source);
        // Extend end to include trailing whitespace up to (and including) newline
        let mut adjusted_end = selection_end;
        if let Some(rest) = self.source.get(selection_end as usize..) {
            for &byte in rest.as_bytes() {
                if byte == b'\n' {
                    adjusted_end += 1;
                    break;
                }
                if byte == b'\r' {
                    adjusted_end += 1;
                    if rest.as_bytes().get((adjusted_end - selection_end) as usize) == Some(&b'\n')
                    {
                        adjusted_end += 1;
                    }
                    break;
                }
                if byte == b' ' || byte == b'\t' {
                    adjusted_end += 1;
                } else {
                    break;
                }
            }
        }

        let replace_end = self.line_map.offset_to_position(adjusted_end, self.source);

        edits.push(TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: format!("{call_text}\n"),
        });

        // b) Insert the new function after the container (or at end of file)
        let insert_offset = self.function_insertion_offset(container_idx);
        let insert_pos = self.line_map.offset_to_position(insert_offset, self.source);

        edits.push(TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text: format!("\n{function_text}"),
        });

        // 12. Assemble the code action
        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: format!("Extract to function '{func_name}'"),
            kind: CodeActionKind::RefactorExtract,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    // -------------------------------------------------------------------------
    // Helpers: find statements covered by the selection
    // -------------------------------------------------------------------------

    /// Find the container (block or source file) and the subset of its
    /// statements that overlap the selection range.
    fn find_selected_statements(
        &self,
        root: NodeIndex,
        start: u32,
        end: u32,
    ) -> Option<(NodeIndex, Vec<NodeIndex>)> {
        // Try the source-file first
        let root_node = self.arena.get(root)?;
        if let Some(sf) = self.arena.get_source_file(root_node) {
            let stmts = self.filter_statements_in_range(&sf.statements.nodes, start, end);
            if !stmts.is_empty() {
                return Some((root, stmts));
            }
        }

        // Walk into nested blocks
        self.find_block_statements_in_range(root, start, end)
    }

    /// Recursively search for the tightest block whose statements overlap
    /// [start, end).
    fn find_block_statements_in_range(
        &self,
        node_idx: NodeIndex,
        start: u32,
        end: u32,
    ) -> Option<(NodeIndex, Vec<NodeIndex>)> {
        if node_idx.is_none() {
            return None;
        }
        let node = self.arena.get(node_idx)?;

        // If this is a block, check its statements
        if node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(node)
        {
            let stmts = self.filter_statements_in_range(&block.statements.nodes, start, end);
            if !stmts.is_empty() {
                return Some((node_idx, stmts));
            }
        }

        // Recurse into children
        let children = self.arena.get_children(node_idx);
        // Prefer the tightest (deepest) match
        for child in children {
            if let Some(child_node) = self.arena.get(child)
                && child_node.pos <= start
                && child_node.end >= end
                && let Some(result) = self.find_block_statements_in_range(child, start, end)
            {
                return Some(result);
            }
        }

        None
    }

    /// Return statements from `stmts` whose span overlaps [start, end).
    fn filter_statements_in_range(
        &self,
        stmts: &[NodeIndex],
        start: u32,
        end: u32,
    ) -> Vec<NodeIndex> {
        let mut result = Vec::new();
        for &stmt_idx in stmts {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            // A statement is selected if it overlaps [start, end)
            if stmt_node.end > start && stmt_node.pos < end {
                result.push(stmt_idx);
            }
        }
        result
    }

    // -------------------------------------------------------------------------
    // Helpers: collect declared names in selected statements
    // -------------------------------------------------------------------------

    /// Collect names declared by variable declarations inside a statement tree.
    fn collect_declared_names(&self, node_idx: NodeIndex, out: &mut FxHashMap<String, NodeIndex>) {
        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::VARIABLE_STATEMENT | syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(var) = self.arena.get_variable(node) {
                    for &decl_idx in &var.declarations.nodes {
                        self.collect_declared_names(decl_idx, out);
                    }
                }
            }
            syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    self.collect_binding_names(decl.name, out);
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node)
                    && let Some(name) = self.arena.get_identifier_text(func.name)
                {
                    out.insert(name.to_string(), func.name);
                }
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node)
                    && let Some(name) = self.arena.get_identifier_text(class.name)
                {
                    out.insert(name.to_string(), class.name);
                }
            }
            _ => {
                // For compound statements (if, for, etc.), recurse into sub-statements
                let children = self.arena.get_children(node_idx);
                for child in children {
                    self.collect_declared_names(child, out);
                }
            }
        }
    }

    /// Collect names from a binding pattern or plain identifier.
    fn collect_binding_names(&self, name_idx: NodeIndex, out: &mut FxHashMap<String, NodeIndex>) {
        if name_idx.is_none() {
            return;
        }
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(text) = self.arena.get_identifier_text(name_idx) {
                out.insert(text.to_string(), name_idx);
            }
            return;
        }

        // Handle destructuring: ObjectBindingPattern / ArrayBindingPattern
        if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            let children = self.arena.get_children(name_idx);
            for child in children {
                self.collect_binding_element_names(child, out);
            }
        }
    }

    /// Collect names from a single binding element.
    fn collect_binding_element_names(
        &self,
        elem_idx: NodeIndex,
        out: &mut FxHashMap<String, NodeIndex>,
    ) {
        if elem_idx.is_none() {
            return;
        }
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };

        if elem_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(text) = self.arena.get_identifier_text(elem_idx) {
                out.insert(text.to_string(), elem_idx);
            }
            return;
        }

        // BindingElement wraps a name (which can itself be a pattern)
        if elem_node.kind == syntax_kind_ext::BINDING_ELEMENT {
            let children = self.arena.get_children(elem_idx);
            for child in children {
                self.collect_binding_names(child, out);
            }
            return;
        }

        // Recurse for nested patterns
        self.collect_binding_names(elem_idx, out);
    }

    // -------------------------------------------------------------------------
    // Helpers: collect identifier references
    // -------------------------------------------------------------------------

    /// Walk a subtree and collect every identifier reference (name, `node_idx`).
    /// Skips identifiers that are declaration names (left side of variable decls,
    /// function names, etc.) -- we only want *uses*.
    fn collect_all_identifier_refs(&self, node_idx: NodeIndex, out: &mut Vec<(String, NodeIndex)>) {
        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        // Skip into function/class bodies -- identifiers there are in a
        // separate scope and do not need to become parameters.
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            return;
        }

        if node.kind == SyntaxKind::Identifier as u16 {
            // Check if this identifier is a declaration name by looking at parent
            if !self.is_declaration_name(node_idx)
                && let Some(text) = self.arena.get_identifier_text(node_idx)
            {
                out.push((text.to_string(), node_idx));
            }
            return;
        }

        // For property access, only recurse into the expression part, not the
        // property name (which is not a free reference).
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.arena.get_access_expr(node) {
                self.collect_all_identifier_refs(access.expression, out);
            }
            return;
        }

        let children = self.arena.get_children(node_idx);
        for child in children {
            self.collect_all_identifier_refs(child, out);
        }
    }

    /// Return true if `ident_idx` is the *name* position of a declaration
    /// (e.g. the `x` in `const x = ...` or `function x(...)`).
    fn is_declaration_name(&self, ident_idx: NodeIndex) -> bool {
        let Some(ext) = self.arena.get_extended(ident_idx) else {
            return false;
        };
        if ext.parent.is_none() {
            return false;
        }
        let Some(parent) = self.arena.get(ext.parent) else {
            return false;
        };

        match parent.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(parent) {
                    return decl.name == ident_idx;
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(parent) {
                    return func.name == ident_idx;
                }
            }
            syntax_kind_ext::CLASS_DECLARATION | syntax_kind_ext::CLASS_EXPRESSION => {
                if let Some(class) = self.arena.get_class(parent) {
                    return class.name == ident_idx;
                }
            }
            syntax_kind_ext::PARAMETER | syntax_kind_ext::BINDING_ELEMENT => {
                // Parameter and binding element names are declarations
                return true;
            }
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                // The key of { key: value } is not a free reference, but
                // shorthand { key } both declares and references. We treat
                // property assignment names as declarations only when they
                // have a separate initializer.
                if let Some(prop) = self.arena.get_property_assignment(parent)
                    && prop.initializer.is_some()
                    && prop.name == ident_idx
                {
                    return true;
                }
            }
            _ => {}
        }

        false
    }

    // -------------------------------------------------------------------------
    // Helpers: determine return variables
    // -------------------------------------------------------------------------

    /// Find variables that are declared (or assigned) inside the selection and
    /// referenced after the selection in the same container.
    fn find_return_variables(
        &self,
        container_idx: NodeIndex,
        selected_stmts: &[NodeIndex],
        declared_in_selection: &FxHashMap<String, NodeIndex>,
        selection_end: u32,
    ) -> Vec<String> {
        if declared_in_selection.is_empty() {
            return Vec::new();
        }

        // Collect all the statements in the container that come *after* the selection
        let after_stmts = self.statements_after(container_idx, selection_end);

        // Collect identifier refs in the post-selection statements
        let mut refs_after: Vec<(String, NodeIndex)> = Vec::new();
        for &stmt_idx in &after_stmts {
            self.collect_all_identifier_refs(stmt_idx, &mut refs_after);
        }

        // Intersect with declared_in_selection, preserving declaration order
        let referenced_after: FxHashSet<String> =
            refs_after.into_iter().map(|(name, _)| name).collect();

        // Build return list in the order variables were declared in the selection
        let mut seen = FxHashSet::default();
        let mut result = Vec::new();

        // We walk selected stmts to preserve declaration order
        for &stmt_idx in selected_stmts {
            let mut local_decls: FxHashMap<String, NodeIndex> = FxHashMap::default();
            self.collect_declared_names(stmt_idx, &mut local_decls);
            for name in local_decls.keys() {
                if referenced_after.contains(name) && seen.insert(name.clone()) {
                    result.push(name.clone());
                }
            }
        }

        result
    }

    /// Return statements in the container that start at or after `after_offset`.
    fn statements_after(&self, container_idx: NodeIndex, after_offset: u32) -> Vec<NodeIndex> {
        let Some(container_node) = self.arena.get(container_idx) else {
            return Vec::new();
        };

        let stmts: Option<&[NodeIndex]> = if container_node.kind == syntax_kind_ext::SOURCE_FILE {
            self.arena
                .get_source_file(container_node)
                .map(|sf| sf.statements.nodes.as_slice())
        } else if container_node.kind == syntax_kind_ext::BLOCK {
            self.arena
                .get_block(container_node)
                .map(|b| b.statements.nodes.as_slice())
        } else {
            None
        };

        let Some(stmts) = stmts else {
            return Vec::new();
        };

        stmts
            .iter()
            .filter(|&&idx| self.arena.get(idx).is_some_and(|n| n.pos >= after_offset))
            .copied()
            .collect()
    }

    // -------------------------------------------------------------------------
    // Helpers: naming and insertion
    // -------------------------------------------------------------------------

    /// Generate a unique function name scoped to the insertion context.
    fn unique_function_name(&self, container_idx: NodeIndex) -> String {
        let mut names = FxHashSet::default();
        if let Some(scope_id) = self.find_enclosing_scope_id(container_idx) {
            self.collect_scope_names(scope_id, &mut names);
        }

        let base = "extracted";
        if !names.contains(base) {
            return base.to_string();
        }

        let mut suffix = 2u32;
        loop {
            let candidate = format!("{base}{suffix}");
            if !names.contains(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    /// Find the offset at which to insert the new function definition.
    /// We place it right after the enclosing function, or at the end of
    /// the source file.
    fn function_insertion_offset(&self, container_idx: NodeIndex) -> u32 {
        // Walk up from the container to find the enclosing function or source file
        let mut current = container_idx;
        while current.is_some() {
            let Some(node) = self.arena.get(current) else {
                break;
            };

            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.is_function_expression_or_arrow()
            {
                // Insert after this function's end
                return node.end;
            }

            if node.kind == syntax_kind_ext::SOURCE_FILE {
                // Insert at the end of the file
                return node.end;
            }

            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
        }

        // Fallback: end of source
        self.source.len() as u32
    }
}
