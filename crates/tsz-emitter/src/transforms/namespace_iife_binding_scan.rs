//! Shared detection of bindings inside a namespace body that shadow the
//! namespace name at the IIFE call site.
//!
//! When a namespace is lowered to an IIFE (`(function (N) { ... })(N || (N = {}))`),
//! `tsc` renames the IIFE parameter (`N` -> `N_1`, `N_2`, ...) whenever the
//! namespace body introduces *any* runtime binding named `N` at *any* nesting
//! depth — including a nested function's parameter or a binding declared inside
//! a nested function/class body. The rename is unconditional on whether the
//! namespace name is actually referenced: a lone `function f(N) {}` inside
//! `namespace N { ... }` is enough.
//!
//! The ES2015 printer path (`crates/tsz-emitter/src/emitter/declarations/namespace.rs`)
//! and the ES5 IR transform path (`namespace_es5_ir.rs`) must agree, so the
//! source-text binding scan lives here and is shared by both. The scan operates
//! on the namespace body source text after masking out type-only (`declare`)
//! statements and enum declarations, whose identifiers introduce no
//! function-scope binding that could shadow the parameter.

use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Returns `true` when the namespace body (a `MODULE_BLOCK`) contains a binding
/// named `ns_name` at any depth (nested function parameters, nested
/// `var`/`let`/`const`/`function`/`class`/`import` declarations). Type-only
/// (`declare`) statements and enum declarations are masked before scanning, so
/// their identifiers never trigger a rename.
///
/// `body_node` is the `MODULE_BLOCK` node; `source_text` is the full source
/// file text. Returns `false` when `source_text` is unavailable or the body
/// span cannot be sliced.
pub(crate) fn namespace_body_text_has_binding_named(
    arena: &NodeArena,
    source_text: Option<&str>,
    body_node: &Node,
    ns_name: &str,
) -> bool {
    let Some(text) = source_text else {
        return false;
    };
    let mut mask_ranges = collect_declare_statement_ranges(arena, body_node);
    mask_ranges.extend(collect_enum_decl_ranges(arena, body_node));
    match crate::safe_slice::slice(text, body_node.pos as usize, body_node.end as usize) {
        Ok(body_text) => {
            let body_pos = body_node.pos as usize;
            let masked = mask_ranges_static(body_text, body_pos, &mask_ranges);
            text_has_non_namespace_binding_named(&masked, ns_name)
        }
        Err(_) => false,
    }
}

/// Scan `text` for a binding occurrence of `name`.
///
/// A binding is recognized when `name` appears as a whole word and is:
/// * preceded by a binding keyword (`var`/`let`/`const`/`function`/`class`/
///   `import`),
/// * a bare parameter (`(name` or `,name`) not immediately followed by `(`, or
/// * a parameter-property modifier binding (`private`/`public`/`protected`/
///   `readonly`/`override` in a parameter context).
///
/// A `name` immediately followed by `.` is a qualified member reference
/// (`N.foo`) and never a binding.
fn text_has_non_namespace_binding_named(text: &str, name: &str) -> bool {
    let stripped = strip_comments(text);
    let text = &stripped;
    let name_bytes = name.as_bytes();
    let text_bytes = text.as_bytes();
    let name_len = name_bytes.len();

    let mut i = 0;
    while i + name_len <= text_bytes.len() {
        if let Some(pos) = text[i..].find(name) {
            let abs = i + pos;
            let before_ok = abs == 0
                || (!text_bytes[abs - 1].is_ascii_alphanumeric()
                    && text_bytes[abs - 1] != b'_'
                    && text_bytes[abs - 1] != b'$');
            let after_end = abs + name_len;
            let after_ok = after_end >= text_bytes.len()
                || (!text_bytes[after_end].is_ascii_alphanumeric()
                    && text_bytes[after_end] != b'_'
                    && text_bytes[after_end] != b'$');

            if before_ok && after_ok {
                // A name immediately followed by `.` is a qualified / member
                // reference (`N.foo`) and is never a binding.
                let mut a = after_end;
                while a < text_bytes.len() && text_bytes[a].is_ascii_whitespace() {
                    a += 1;
                }
                let followed_by_dot = a < text_bytes.len() && text_bytes[a] == b'.';
                let followed_by_paren = a < text_bytes.len() && text_bytes[a] == b'(';
                if followed_by_dot {
                    i = abs + 1;
                    continue;
                }
                let mut p = abs;
                while p > 0 && text_bytes[p - 1].is_ascii_whitespace() {
                    p -= 1;
                }
                if p > 0 {
                    let prev_char = text_bytes[p - 1];
                    // Bare `(`/`,` before the name only counts as a binding
                    // when the name is NOT immediately followed by `(`. A name
                    // followed by `(` here is a callee (`if (N.f())`,
                    // `foo(N())`), not a parameter/binding. Genuine parameter
                    // bindings (`function f(N)`, `f(a, N)`) are followed by
                    // `)`, `,`, `:`, `=`, etc. — never `(`.
                    if (prev_char == b'(' || prev_char == b',') && !followed_by_paren {
                        return true;
                    }
                    let preceding = &text[..p];
                    let binding_keywords: &[&str] =
                        &["var", "let", "const", "function", "class", "import"];
                    for &kw in binding_keywords {
                        if preceding.ends_with(kw) {
                            let kw_start = p - kw.len();
                            let kw_before_ok = kw_start == 0
                                || (!text_bytes[kw_start - 1].is_ascii_alphanumeric()
                                    && text_bytes[kw_start - 1] != b'_'
                                    && text_bytes[kw_start - 1] != b'$');
                            if kw_before_ok {
                                return true;
                            }
                        }
                    }
                    let parameter_modifier_keywords: &[&str] =
                        &["private", "public", "protected", "readonly", "override"];
                    for &kw in parameter_modifier_keywords {
                        if preceding.ends_with(kw) {
                            let kw_start = p - kw.len();
                            let kw_before_ok = kw_start == 0
                                || (!text_bytes[kw_start - 1].is_ascii_alphanumeric()
                                    && text_bytes[kw_start - 1] != b'_'
                                    && text_bytes[kw_start - 1] != b'$');
                            if kw_before_ok && keyword_is_in_parameter_context(text_bytes, kw_start)
                            {
                                return true;
                            }
                        }
                    }
                }
            }
            i = abs + 1;
        } else {
            break;
        }
    }
    false
}

/// Determine whether a parameter-property modifier keyword at `kw_start` is in a
/// parameter context (preceded, after skipping other modifiers, by `(` or `,`).
fn keyword_is_in_parameter_context(text_bytes: &[u8], kw_start: usize) -> bool {
    let mut p = kw_start;
    loop {
        while p > 0 && text_bytes[p - 1].is_ascii_whitespace() {
            p -= 1;
        }
        if p == 0 {
            return false;
        }
        let prev_char = text_bytes[p - 1];
        if prev_char == b'(' || prev_char == b',' {
            return true;
        }
        if !prev_char.is_ascii_alphanumeric() && prev_char != b'_' && prev_char != b'$' {
            return false;
        }

        let ident_end = p;
        let mut ident_start = ident_end - 1;
        while ident_start > 0
            && (text_bytes[ident_start - 1].is_ascii_alphanumeric()
                || text_bytes[ident_start - 1] == b'_'
                || text_bytes[ident_start - 1] == b'$')
        {
            ident_start -= 1;
        }
        let Ok(ident) = std::str::from_utf8(&text_bytes[ident_start..ident_end]) else {
            return false;
        };
        if !matches!(
            ident,
            "private" | "public" | "protected" | "readonly" | "override"
        ) {
            return false;
        }
        p = ident_start;
    }
}

/// Strip single-line and block comments from text, replacing them with spaces.
fn strip_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                result.push(b' ');
                i += 1;
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            result.push(b' ');
            result.push(b' ');
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                result.push(b' ');
                i += 1;
            }
            if i + 1 < bytes.len() {
                result.push(b' ');
                result.push(b' ');
                i += 2;
            }
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(result).unwrap_or_default()
}

/// Replace bytes inside `ranges` (absolute source positions) with spaces in
/// `body_text`, where `body_text` starts at absolute offset `body_pos`.
fn mask_ranges_static(body_text: &str, body_pos: usize, ranges: &[(usize, usize)]) -> String {
    if ranges.is_empty() {
        return body_text.to_string();
    }
    let mut bytes = body_text.as_bytes().to_vec();
    for &(start, end) in ranges {
        let local_start = start.saturating_sub(body_pos);
        let local_end = end.saturating_sub(body_pos).min(bytes.len());
        if local_start >= bytes.len() {
            continue;
        }
        for b in &mut bytes[local_start..local_end] {
            if !b.is_ascii_whitespace() {
                *b = b' ';
            }
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| body_text.to_string())
}

/// Collect `(pos, end)` byte ranges of every type-only (`declare`) statement in
/// the namespace body. Their bodies are erased at emit time, so identifiers
/// introduced inside them must not be counted as shadowing bindings.
fn collect_declare_statement_ranges(arena: &NodeArena, body_node: &Node) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let Some(block) = arena.get_module_block(body_node) else {
        return ranges;
    };
    let Some(stmts) = &block.statements else {
        return ranges;
    };
    for &stmt_idx in &stmts.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };
        let (decl_node, decl_pos, decl_end) = if stmt_node.kind
            == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export) = arena.get_export_decl(stmt_node)
            && let Some(inner) = arena.get(export.export_clause)
        {
            (inner, stmt_node.pos as usize, stmt_node.end as usize)
        } else {
            (stmt_node, stmt_node.pos as usize, stmt_node.end as usize)
        };
        let modifiers = match decl_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => arena
                .get_variable(decl_node)
                .and_then(|v| v.modifiers.clone()),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => arena
                .get_function(decl_node)
                .and_then(|f| f.modifiers.clone()),
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                arena.get_class(decl_node).and_then(|c| c.modifiers.clone())
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                arena.get_enum(decl_node).and_then(|e| e.modifiers.clone())
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => arena
                .get_module(decl_node)
                .and_then(|m| m.modifiers.clone()),
            _ => None,
        };
        if arena.has_modifier(&modifiers, SyntaxKind::DeclareKeyword) {
            ranges.push((decl_pos, decl_end));
        }
    }
    ranges
}

/// Collect `(pos, end)` byte ranges of enum declarations and `export import`
/// equals declarations in the namespace body (and nested sub-namespace bodies).
/// Enum members are *properties* of the enum object, not function-scope
/// bindings, and `export import M = Z.M` reuses the IIFE parameter, so neither
/// must be treated as a shadowing binding.
fn collect_enum_decl_ranges(arena: &NodeArena, body_node: &Node) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    collect_enum_decl_ranges_into(arena, body_node, &mut ranges);
    ranges
}

fn collect_enum_decl_ranges_into(
    arena: &NodeArena,
    body_node: &Node,
    ranges: &mut Vec<(usize, usize)>,
) {
    let Some(block) = arena.get_module_block(body_node) else {
        return;
    };
    let Some(stmts) = &block.statements else {
        return;
    };
    for &stmt_idx in &stmts.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };
        let decl_node = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export) = arena.get_export_decl(stmt_node)
            && let Some(inner) = arena.get(export.export_clause)
        {
            inner
        } else {
            stmt_node
        };
        let is_export_qualified = stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION;
        if decl_node.kind == syntax_kind_ext::ENUM_DECLARATION {
            ranges.push((decl_node.pos as usize, decl_node.end as usize));
        } else if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            && is_export_qualified
        {
            // `export import M = Z.M` emits as `M.M = Z.M`: it reuses the IIFE
            // parameter and introduces no local binding.
            ranges.push((stmt_node.pos as usize, stmt_node.end as usize));
        } else if decl_node.kind == syntax_kind_ext::MODULE_DECLARATION
            && let Some(module) = arena.get_module(decl_node)
            && let Some(inner_body) = arena.get(module.body)
        {
            collect_enum_decl_ranges_into(arena, inner_body, ranges);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_function_parameter_binding() {
        // function f(schema) { ... } binds `schema`.
        assert!(text_has_non_namespace_binding_named(
            "{ function f(schema) { return 0; } }",
            "schema"
        ));
    }

    #[test]
    fn detects_function_parameter_binding_renamed_var() {
        // Same shape, different chosen names — must still detect.
        assert!(text_has_non_namespace_binding_named(
            "{ function build(build) { return 0; } }",
            "build"
        ));
    }

    #[test]
    fn detects_var_let_const_binding() {
        assert!(text_has_non_namespace_binding_named("{ var n = 1; }", "n"));
        assert!(text_has_non_namespace_binding_named("{ let n = 1; }", "n"));
        assert!(text_has_non_namespace_binding_named(
            "{ const n = 1; }",
            "n"
        ));
    }

    #[test]
    fn ignores_qualified_member_reference() {
        // `schema.foo = ...` is a member reference, not a binding.
        assert!(!text_has_non_namespace_binding_named(
            "{ schema.createValidator = createValidator; }",
            "schema"
        ));
    }

    #[test]
    fn ignores_callee_reference() {
        // `schema()` is a call, not a binding.
        assert!(!text_has_non_namespace_binding_named(
            "{ return schema(); }",
            "schema"
        ));
    }

    #[test]
    fn ignores_unrelated_names() {
        assert!(!text_has_non_namespace_binding_named(
            "{ function f(x) { return x; } }",
            "schema"
        ));
    }
}
