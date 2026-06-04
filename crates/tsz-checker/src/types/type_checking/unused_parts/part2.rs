impl<'a> CheckerState<'a> {
    /// Check if there's a method implementation with the given name after position `start`.
    ///
    /// ## Parameters
    /// - `members`: Slice of member node indices
    /// - `start`: Position to start searching from
    /// - `_name`: The method name to search for
    ///
    /// Returns (found: bool, name: Option<String>).
    pub(crate) fn find_method_impl(
        &self,
        members: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>, Option<usize>) {
        for (offset, member_idx) in members.iter().skip(start).copied().enumerate() {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::METHOD_DECLARATION {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    let member_name = self.get_method_name_from_node(member_idx);
                    if member_name.as_deref() != Some(name) {
                        if method.body.is_some() {
                            // Different name but has body - wrong-named implementation (TS2389)
                            return (true, member_name, Some(start + offset));
                        }
                        // Different name, no body - no implementation found
                        return (false, None, None);
                    }
                    if method.body.is_some() {
                        // Found the implementation with matching name
                        return (true, member_name, Some(start + offset));
                    }
                    // Same name but no body - another overload signature, keep looking
                }
            } else {
                // Non-method member encountered - no implementation found
                return (false, None, None);
            }
        }
        (false, None, None)
    }

    /// Find a function implementation with the given name after position `start`.
    ///
    /// Recursively searches through statements to find a matching function implementation.
    /// Handles overload signatures by continuing to search through same-name overloads.
    ///
    /// ## Parameters
    /// - `statements`: Slice of statement node indices
    /// - `start`: Position to start searching from
    /// - `name`: The function name to search for
    ///
    /// Returns (found: bool, name: Option<String>, node: Option<NodeIndex>).
    pub(crate) fn find_function_impl(
        &self,
        statements: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>, Option<NodeIndex>) {
        if start >= statements.len() {
            return (false, None, None);
        }

        let stmt_idx = statements[start];
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return (false, None, None);
        };

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.ctx.arena.get_function(node)
        {
            // Check if this is an implementation (has body)
            if func.body.is_some() {
                // This is an implementation - check if name matches
                let impl_name = self.get_function_name_from_node(stmt_idx);
                return (true, impl_name, Some(stmt_idx));
            }

            // Another overload signature without body - need to look further
            // but we should check if this is the same function name
            let overload_name = self.get_function_name_from_node(stmt_idx);
            if overload_name.as_ref() == Some(&name.to_string()) {
                // Same function, continue looking for implementation
                return self.find_function_impl(statements, start + 1, name);
            }
        }

        // NOTE: A class declaration with the same name does NOT serve as a
        // function implementation. TSC reports TS2391 even when a class with the
        // same name follows the overload signatures (they merge, but the function
        // still needs its own body).
        (false, None, None)
    }

    /// Checks if a symbol name appears to be used in a JSDoc comment.
    /// This restricts the pattern search to actual block-comment ranges so
    /// JSDoc-looking text inside string literals, regular code, or
    /// regex/template literals does not falsely suppress TS6196 / TS6133.
    /// Mirrors tsc's behavior where only real `/** ... */` comments
    /// contribute symbol references via `@type`, `@import`, `@param`,
    /// `@returns`, `@template`, and `{@link ...}` tags.
    fn is_symbol_used_in_jsdoc(&self, name: &str) -> bool {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let text: &str = &sf.text;

        // `@template T` *declares* a scoped type parameter; it does not
        // *reference* an existing symbol. Don't include
        // `format!("@template {name}")` here — that would falsely mark
        // an unrelated value/local of the same name as "used", and the
        // raw substring would also match `@template TypeParam` as a
        // hit for `@template T` (issue #3506).
        let patterns = [
            format!("{{@link {name}}}"),
            format!("{{@link {name}."),
            format!("@import {{{name}}}"),
            format!("@import {{ {name} }}"),
            format!("@type {{{name}}}"),
            format!("@type {{{name}[]}}"),
            format!("@type {{ {name} }}"),
            format!("@param {{{name}}}"),
            format!("@param {{{name}[]}}"),
            format!("@param {{ {name} }}"),
            format!("@returns {{{name}}}"),
            format!("@returns {{{name}[]}}"),
            format!("@returns {{ {name} }}"),
        ];

        let import_brace_pattern = format!("{{ {name} }}");

        for range in &sf.comments {
            // Only real JSDoc block comments can contribute type references.
            if !tsz_common::comments::is_jsdoc_comment(range, text) {
                continue;
            }
            let comment_text = range.get_text(text);

            for p in &patterns {
                if comment_text.contains(p) {
                    return true;
                }
            }

            // Match JSDoc import with whitespace: `@import { Type } from ...`.
            if comment_text.contains(&import_brace_pattern)
                && Self::jsdoc_contains_tag(comment_text, "import")
            {
                return true;
            }
        }

        false
    }
}
