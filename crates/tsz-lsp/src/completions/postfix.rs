//! Postfix completion support for LSP.
//!
//! Provides postfix completions that transform expressions by appending
//! a snippet after a dot. For example:
//!
//! - `expr.if` → `if (expr) { $0 }`
//! - `expr.for` → `for (const item of expr) { $0 }`
//! - `expr.forof` → `for (const item of expr) { $0 }`
//! - `expr.forin` → `for (const key in expr) { $0 }`
//! - `expr.not` → `!expr`
//! - `expr.return` → `return expr;`
//! - `expr.log` → `console.log(expr)`
//! - `expr.await` → `await expr`
//! - `expr.typeof` → `typeof expr`
//! - `expr.cast` → `expr as $1`
//! - `expr.new` → `new expr($0)`
//! - `expr.var` → `const $1 = expr;`
//! - `expr.spread` → `...expr`

use super::{CompletionItem, CompletionItemKind, sort_priority};
use tsz_common::position::{LineMap, Position};
use tsz_parser::parser::node::NodeArena;

/// A postfix completion template.
struct PostfixTemplate {
    /// The trigger text after the dot (e.g., "if", "for", "return").
    label: &'static str,
    /// A short description of what the postfix does.
    detail: &'static str,
    /// A function that generates the replacement text given the expression text.
    /// Returns (insert_text, is_snippet).
    generate: fn(&str) -> (String, bool),
}

const POSTFIX_TEMPLATES: &[PostfixTemplate] = &[
    PostfixTemplate {
        label: "if",
        detail: "if (expr) { }",
        generate: |expr| (format!("if ({expr}) {{\n\t$0\n}}"), true),
    },
    PostfixTemplate {
        label: "else",
        detail: "if (!expr) { }",
        generate: |expr| (format!("if (!{expr}) {{\n\t$0\n}}"), true),
    },
    PostfixTemplate {
        label: "for",
        detail: "for (const item of expr) { }",
        generate: |expr| {
            (
                format!("for (const ${{1:item}} of {expr}) {{\n\t$0\n}}"),
                true,
            )
        },
    },
    PostfixTemplate {
        label: "forof",
        detail: "for (const item of expr) { }",
        generate: |expr| {
            (
                format!("for (const ${{1:item}} of {expr}) {{\n\t$0\n}}"),
                true,
            )
        },
    },
    PostfixTemplate {
        label: "forin",
        detail: "for (const key in expr) { }",
        generate: |expr| {
            (
                format!("for (const ${{1:key}} in {expr}) {{\n\t$0\n}}"),
                true,
            )
        },
    },
    PostfixTemplate {
        label: "foreach",
        detail: "expr.forEach((item) => { })",
        generate: |expr| {
            (
                format!("{expr}.forEach((${{1:item}}) => {{\n\t$0\n}})"),
                true,
            )
        },
    },
    PostfixTemplate {
        label: "not",
        detail: "!expr",
        generate: |expr| (format!("!{expr}"), false),
    },
    PostfixTemplate {
        label: "return",
        detail: "return expr;",
        generate: |expr| (format!("return {expr};"), false),
    },
    PostfixTemplate {
        label: "log",
        detail: "console.log(expr)",
        generate: |expr| (format!("console.log({expr})"), false),
    },
    PostfixTemplate {
        label: "await",
        detail: "await expr",
        generate: |expr| (format!("await {expr}"), false),
    },
    PostfixTemplate {
        label: "typeof",
        detail: "typeof expr",
        generate: |expr| (format!("typeof {expr}"), false),
    },
    PostfixTemplate {
        label: "cast",
        detail: "expr as Type",
        generate: |expr| (format!("{expr} as ${{1:unknown}}"), true),
    },
    PostfixTemplate {
        label: "new",
        detail: "new expr()",
        generate: |expr| (format!("new {expr}($0)"), true),
    },
    PostfixTemplate {
        label: "var",
        detail: "const name = expr;",
        generate: |expr| (format!("const ${{1:name}} = {expr};"), true),
    },
    PostfixTemplate {
        label: "let",
        detail: "let name = expr;",
        generate: |expr| (format!("let ${{1:name}} = {expr};"), true),
    },
    PostfixTemplate {
        label: "spread",
        detail: "...expr",
        generate: |expr| (format!("...{expr}"), false),
    },
    PostfixTemplate {
        label: "void",
        detail: "void expr",
        generate: |expr| (format!("void {expr}"), false),
    },
];

/// Check if the position is after a dot on an expression and generate
/// postfix completions.
///
/// Returns a list of postfix completion items, or an empty vec if the
/// cursor is not in a postfix position.
pub fn get_postfix_completions(
    _arena: &NodeArena,
    line_map: &LineMap,
    source_text: &str,
    position: Position,
) -> Vec<CompletionItem> {
    // Convert position to offset
    let offset = match line_map.position_to_offset(position, source_text) {
        Some(o) => o,
        None => return Vec::new(),
    };

    // Look backwards from the cursor to find the dot and the expression before it
    let before_cursor = &source_text[..offset as usize];
    let trimmed = before_cursor.trim_end();

    // The user is typing `expr.` or `expr.par` (partial postfix)
    // Find the last dot
    let dot_pos = match trimmed.rfind('.') {
        Some(p) => p,
        None => return Vec::new(),
    };

    // Get the partial text after the dot (what user typed so far)
    let partial = &trimmed[dot_pos + 1..];

    // Get the expression text before the dot
    let expr_text = trimmed[..dot_pos].trim_end();
    if expr_text.is_empty() {
        return Vec::new();
    }

    // Basic validation: the expression should look like an identifier or
    // a property access chain (not a keyword like `if`, `for`, etc.)
    if !looks_like_expression_end(expr_text) {
        return Vec::new();
    }

    // The range to replace: from the start of the expression to the cursor
    // We need to find where the expression starts
    let expr_start = find_expression_start(expr_text, before_cursor, dot_pos);

    let expr_for_template = &before_cursor[expr_start..dot_pos];

    // Generate completions for matching templates
    POSTFIX_TEMPLATES
        .iter()
        .filter(|t| t.label.starts_with(partial))
        .map(|template| {
            let (insert_text, is_snippet) = (template.generate)(expr_for_template);
            let mut item =
                CompletionItem::new(template.label.to_string(), CompletionItemKind::Keyword)
                    .with_detail(template.detail.to_string())
                    .with_insert_text(insert_text)
                    .with_sort_text(sort_priority::LOCATION_PRIORITY);
            item.is_snippet = is_snippet;
            // Store the replacement span as byte offsets
            item.replacement_span = Some((expr_start as u32, offset));
            item
        })
        .collect()
}

/// Check if the text looks like it ends with an expression (identifier,
/// closing paren/bracket, string literal, etc.)
fn looks_like_expression_end(text: &str) -> bool {
    let last_char = match text.chars().last() {
        Some(c) => c,
        None => return false,
    };

    last_char.is_alphanumeric()
        || last_char == '_'
        || last_char == '$'
        || last_char == ')'
        || last_char == ']'
        || last_char == '\''
        || last_char == '"'
        || last_char == '`'
}

/// Find the start of the expression before the dot by scanning backwards.
fn find_expression_start(_expr_text: &str, full_text: &str, dot_pos: usize) -> usize {
    // Simple heuristic: scan backwards through identifier characters and
    // property access chains (dots, brackets).
    let bytes = full_text.as_bytes();
    let mut pos = dot_pos;

    // Skip the expression text backwards
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;

    while pos > 0 {
        pos -= 1;
        let ch = bytes[pos];

        match ch {
            b')' => paren_depth += 1,
            b'(' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                } else {
                    pos += 1;
                    break;
                }
            }
            b']' => bracket_depth += 1,
            b'[' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                } else {
                    pos += 1;
                    break;
                }
            }
            b'.' => {
                // Continue scanning (property access chain)
            }
            c if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' => {
                // Continue scanning (identifier)
            }
            _ if paren_depth > 0 || bracket_depth > 0 => {
                // Inside parens/brackets, continue
            }
            _ => {
                pos += 1;
                break;
            }
        }
    }

    pos
}
