//! Convert `.then()` / `.catch()` chains to async/await.
//!
//! Converts promise chains to async/await syntax:
//! - `promise.then(x => doSomething(x))` → `const x = await promise; doSomething(x);`
//! - Wraps in try/catch when `.catch()` is present.

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Convert a `.then()` chain to async/await.
    pub fn convert_to_async_await(&self, _root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;

        // Find a .then() call expression at cursor
        let (call_idx, chain) = self.find_then_chain(start_offset)?;

        if chain.then_calls.is_empty() {
            return None;
        }

        let call_node = self.arena.get(call_idx)?;
        let indent = self.indent_at_offset(call_node.pos);

        // Build the async/await replacement
        let mut lines = Vec::new();
        let mut current_expr = chain.base_expr.clone();

        for (i, then_arg) in chain.then_calls.iter().enumerate() {
            let var_name = if i == 0 {
                "result".to_string()
            } else {
                format!("result{}", i + 1)
            };
            lines.push(format!("{indent}const {var_name} = await {current_expr};"));
            if let Some(callback_body) = then_arg {
                current_expr = callback_body.replace("__PARAM__", &var_name);
            } else {
                current_expr = var_name;
            }
        }

        let mut result = lines.join("\n");

        // Wrap in try/catch if there's a .catch()
        if let Some(catch_body) = &chain.catch_call {
            let inner = result;
            result = format!(
                "{indent}try {{\n{inner}\n{indent}}} catch (error) {{\n{indent}  {catch_body}\n{indent}}}"
            );
        }

        let replace_start = self.line_map.offset_to_position(call_node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(call_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: result,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to async/await".to_string(),
            kind: CodeActionKind::RefactorRewrite,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Find a `.then()` chain starting from the given offset.
    fn find_then_chain(&self, offset: u32) -> Option<(NodeIndex, ThenChain)> {
        let mut current = find_node_at_offset(self.arena, offset);

        while current.is_some() {
            let node = self.arena.get(current)?;

            if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                if let Some(chain) = self.analyze_then_chain(current) {
                    // Find the outermost chained call
                    let outer = self.find_outermost_chain_call(current);
                    return Some((outer, chain));
                }
            }

            current = self.arena.get_extended(current)?.parent;
        }

        None
    }

    /// Analyze a call expression to see if it's part of a .then() chain.
    fn analyze_then_chain(&self, idx: NodeIndex) -> Option<ThenChain> {
        let node = self.arena.get(idx)?;
        let call_data = self.arena.get_call_expr(node)?;

        // Check if this is a .then() or .catch() call
        let expr_node = self.arena.get(call_data.expression)?;
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(expr_node)?;
        let method_name = self.arena.get_identifier_text(access.name_or_argument)?;

        if method_name != "then" && method_name != "catch" {
            return None;
        }

        // Get the base expression
        let base_node = self.arena.get(access.expression)?;
        let base_text = self
            .source
            .get(base_node.pos as usize..base_node.end as usize)?;

        // Get callback argument text (simplified)
        let callback_text = call_data
            .arguments
            .as_ref()
            .and_then(|args| args.nodes.first())
            .and_then(|&arg_idx| {
                let arg_node = self.arena.get(arg_idx)?;
                self.source
                    .get(arg_node.pos as usize..arg_node.end as usize)
                    .map(String::from)
            });

        let mut chain = ThenChain {
            base_expr: base_text.to_string(),
            then_calls: Vec::new(),
            catch_call: None,
        };

        if method_name == "then" {
            chain.then_calls.push(callback_text);
        } else {
            chain.catch_call = callback_text;
        }

        Some(chain)
    }

    fn find_outermost_chain_call(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = idx;
        loop {
            let Some(ext) = self.arena.get_extended(current) else {
                return current;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return current;
            }
            // Check if parent is a property access that's part of .then()/.catch()
            let Some(parent_node) = self.arena.get(parent) else {
                return current;
            };
            if parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let Some(parent_ext) = self.arena.get_extended(parent) else {
                    return current;
                };
                let grandparent = parent_ext.parent;
                if grandparent.is_some() {
                    let Some(gp_node) = self.arena.get(grandparent) else {
                        return current;
                    };
                    if gp_node.kind == syntax_kind_ext::CALL_EXPRESSION {
                        current = grandparent;
                        continue;
                    }
                }
            }
            return current;
        }
    }
}

struct ThenChain {
    base_expr: String,
    then_calls: Vec<Option<String>>,
    catch_call: Option<String>,
}
