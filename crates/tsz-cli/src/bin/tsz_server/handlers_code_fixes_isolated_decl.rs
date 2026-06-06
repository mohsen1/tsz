//! Code-fix helpers for TS9010 — variable missing type annotation under
//! `--isolatedDeclarations`.
//!
//! Contains `apply_isolated_decl_type_annotation_fix` and its supporting
//! helper `infer_type_for_isolated_decl_initializer`.

use super::Server;
use super::handlers_code_fixes_utils::positions_overlap;

pub(super) const FIX_MISSING_TYPE_ANNOTATION_FIX_ID: &str = "fixMissingTypeAnnotationOnExports";

impl Server {
    /// Infer the annotation type for an isolated-declarations initializer using
    /// AST-structural analysis only (no full type-checker run required).
    ///
    /// Rule: When an exported variable's initializer is a JSX expression
    /// (`JsxElement`, `JsxSelfClosingElement`, `JsxFragment`), the annotation type
    /// is `JSX.Element` — this is the standardized JSX element type defined by
    /// the JSX namespace in TypeScript's JSX support.
    pub(super) fn infer_type_for_isolated_decl_initializer(
        arena: &tsz::parser::node::NodeArena,
        init_idx: tsz::parser::NodeIndex,
    ) -> Option<String> {
        use tsz::parser::syntax_kind_ext::{
            JSX_ELEMENT, JSX_FRAGMENT, JSX_SELF_CLOSING_ELEMENT, PARENTHESIZED_EXPRESSION,
        };
        const JSX_ELEMENT_TYPE: &str = "JSX.Element";

        let node = arena.get(init_idx)?;

        // JSX expressions always have type JSX.Element in TypeScript — structural
        // inspection of the node kind is sufficient; no type-checker needed.
        if node.kind == JSX_ELEMENT
            || node.kind == JSX_SELF_CLOSING_ELEMENT
            || node.kind == JSX_FRAGMENT
        {
            return Some(JSX_ELEMENT_TYPE.to_string());
        }

        // Recurse through parenthesized wrappers: `(<div/>)` → JSX.Element
        if node.kind == PARENTHESIZED_EXPRESSION {
            let paren = arena.get_parenthesized(node)?;
            if paren.expression.is_some() {
                return Self::infer_type_for_isolated_decl_initializer(arena, paren.expression);
            }
        }

        // Other initializer kinds require full type-checker inference.
        None
    }

    /// Generate code fix actions for TS9010 (variable missing type annotation)
    /// under `--isolatedDeclarations`.
    ///
    /// Returns two actions per applicable diagnostic:
    /// 1. Direct annotation: `const x: T = expr;`
    /// 2. Satisfies+cast: `const x = (expr) satisfies T as T;`
    pub(super) fn apply_isolated_decl_type_annotation_fix(
        file_path: &str,
        content: &str,
        arena: &tsz::parser::node::NodeArena,
        line_map: &tsz::lsp::position::LineMap,
        diagnostics: &[tsz::checker::diagnostics::Diagnostic],
        error_codes: &[u32],
        request_span: Option<(tsz::lsp::position::Position, tsz::lsp::position::Position)>,
    ) -> Vec<serde_json::Value> {
        use tsz::parser::syntax_kind_ext::VARIABLE_DECLARATION;

        const TS9010: u32 = 9010;
        // `VariableDeclaration` is always within a few AST levels of its name node.
        const MAX_ANCESTOR_WALK: usize = 20;

        if !error_codes.contains(&TS9010) {
            return vec![];
        }

        // Determine the offset for the variable name. Prefer a server-generated
        // TS9010 diagnostic (exact name position); fall back to the client-supplied
        // request span when the server did not emit TS9010 (e.g. because
        // isolatedDeclarations is not enabled in the server's inferred options but
        // the client reports the diagnostic at a known span).
        let name_offset: u32 = if let Some(diag) = diagnostics.iter().find(|d| {
            d.code == TS9010
                && request_span.is_none_or(|(start, end)| {
                    let diag_pos = line_map.offset_to_position(d.start, content);
                    let diag_end = line_map.offset_to_position(d.start + d.length, content);
                    positions_overlap(start, end, diag_pos, diag_end)
                })
        }) {
            diag.start
        } else if let Some((span_start, _)) = request_span {
            // Client-provided span: convert line/col position to byte offset.
            let Some(offset) = line_map.position_to_offset(span_start, content) else {
                return vec![];
            };
            offset
        } else {
            return vec![];
        };

        // Find the name identifier at the variable name offset
        let name_idx = tsz::lsp::utils::find_node_at_offset(arena, name_offset);
        if name_idx.is_none() {
            return vec![];
        }

        // Walk up to the enclosing `VariableDeclaration` (at most a few levels)
        let mut decl_idx = tsz::parser::NodeIndex::NONE;
        let mut current = name_idx;
        for _ in 0..MAX_ANCESTOR_WALK {
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == VARIABLE_DECLARATION {
                decl_idx = current;
                break;
            }
            let Some(parent) = arena.parent_of(current) else {
                break;
            };
            current = parent;
        }
        if decl_idx.is_none() {
            return vec![];
        }

        let Some(decl_node) = arena.get(decl_idx) else {
            return vec![];
        };
        let Some(decl) = arena.get_variable_declaration(decl_node) else {
            return vec![];
        };

        let init_idx = decl.initializer;
        if init_idx.is_none() {
            return vec![];
        }

        // Infer the annotation type from the initializer's AST shape
        let Some(type_string) = Self::infer_type_for_isolated_decl_initializer(arena, init_idx)
        else {
            return vec![];
        };

        // Position for the direct annotation: insert `: T` right after the name
        let Some(name_node) = arena.get(decl.name) else {
            return vec![];
        };
        let name_end_pos = line_map.offset_to_position(name_node.end, content);

        // Position for the satisfies+cast: wrap the initializer expression
        let Some(init_node) = arena.get(init_idx) else {
            return vec![];
        };

        // Skip leading whitespace/trivia to find the initializer content start
        let init_content_start = {
            let bytes = content.as_bytes();
            let mut pos = init_node.pos as usize;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            pos as u32
        };
        let init_end = init_node.end;

        let init_start_pos = line_map.offset_to_position(init_content_start, content);
        let init_end_pos = line_map.offset_to_position(init_end, content);

        let fix_all_desc =
            "Add annotations of inferred types to all items with missing annotations";

        vec![
            serde_json::json!({
                "fixName": FIX_MISSING_TYPE_ANNOTATION_FIX_ID,
                "description": format!("Add annotation of type '{type_string}'"),
                "changes": [{
                    "fileName": file_path,
                    "textChanges": [{
                        "start": {
                            "line": name_end_pos.line + 1,
                            "offset": name_end_pos.character + 1
                        },
                        "end": {
                            "line": name_end_pos.line + 1,
                            "offset": name_end_pos.character + 1
                        },
                        "newText": format!(": {type_string}")
                    }]
                }],
                "fixId": FIX_MISSING_TYPE_ANNOTATION_FIX_ID,
                "fixAllDescription": fix_all_desc,
            }),
            serde_json::json!({
                "fixName": FIX_MISSING_TYPE_ANNOTATION_FIX_ID,
                "description": format!("Add satisfies and an inline type assertion with '{type_string}'"),
                "changes": [{
                    "fileName": file_path,
                    "textChanges": [
                        {
                            "start": {
                                "line": init_start_pos.line + 1,
                                "offset": init_start_pos.character + 1
                            },
                            "end": {
                                "line": init_start_pos.line + 1,
                                "offset": init_start_pos.character + 1
                            },
                            "newText": "("
                        },
                        {
                            "start": {
                                "line": init_end_pos.line + 1,
                                "offset": init_end_pos.character + 1
                            },
                            "end": {
                                "line": init_end_pos.line + 1,
                                "offset": init_end_pos.character + 1
                            },
                            "newText": format!(") satisfies {type_string} as {type_string}")
                        }
                    ]
                }],
                "fixId": FIX_MISSING_TYPE_ANNOTATION_FIX_ID,
                "fixAllDescription": fix_all_desc,
            }),
        ]
    }
}
