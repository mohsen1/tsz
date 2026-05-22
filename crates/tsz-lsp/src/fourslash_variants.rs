//! Shape-variant generator for fourslash fixtures.
//!
//! Per `CLAUDE.md` §25, a fourslash test must not assert behavior that only
//! holds for the literal identifier names in its fixture. This module is the
//! missing tooling: opt in by calling `shape_variants(source, &[...])` and
//! running the same assertion against each labeled generated source.
//!
//! ## Preservation contract
//!
//! `apply_variant` rewrites only:
//! - JS identifier tokens that match an `identifier_renames` key (one-pass
//!   lookup, so swaps like `T<->K` are correct);
//! - path substrings inside `// @filename:` directives and string-literal
//!   bodies, per `path_renames`.
//!
//! Everything else is forwarded verbatim: marker comments (`/*name*/`),
//! block and line comments (other than `@filename:` directives), string
//! contents not covered by a path rename, keywords, and operators.
//!
//! The recognizer is a deliberately minimal lexer over the categories
//! above; it does not parse TypeScript. Opt-in is per-test, not corpus-wide,
//! so authors choose adjacent shapes that matter and CI does not balloon.

use rustc_hash::FxHashMap;

/// One concrete spelling variant of a fourslash fixture.
///
/// Variants describe renames at the level of whole JS identifier tokens and
/// path substrings. The variant generator never edits marker names, keyword
/// tokens, or non-string code bodies that do not match an identifier rename
/// key.
#[derive(Debug, Clone)]
pub struct ShapeVariant {
    /// Short identifier for the variant (e.g. `"rename_T_to_K"`), surfaced
    /// via the `Debug` impl when a variant assertion fails so a human can
    /// tell which spelling tripped the failure.
    pub label: &'static str,
    /// Identifier renames applied as whole JS-identifier tokens.
    /// Each `(old, new)` pair renames every occurrence of `old` that
    /// appears as a full identifier token (not as a substring of another
    /// identifier, not inside strings or comments). Both sides must be
    /// valid JS identifiers.
    pub identifier_renames: &'static [(&'static str, &'static str)],
    /// Path renames applied to `// @filename:` directives and to the
    /// bodies of string literals (single-quoted, double-quoted, and
    /// backtick-delimited). Each `(old, new)` pair is substituted as a
    /// plain substring within those contexts only.
    pub path_renames: &'static [(&'static str, &'static str)],
}

/// One generated fourslash source and the label that should be reported if
/// assertions against that source fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeVariantSource {
    pub label: &'static str,
    pub source: String,
}

/// Generate the original source plus one rewritten copy per variant.
///
/// The first element of the result is always the original source. Each
/// subsequent element is the result of applying the corresponding
/// `ShapeVariant` to the source.
///
/// # Opt-in criteria
///
/// Use this from a fourslash test when the assertion under test is about a
/// structural LSP behavior (definition resolution, hover shape, completion
/// keying, rename scope, etc.) that should not depend on the literal
/// identifier name or module path the fixture happens to use. Do not use
/// this when the test specifically asserts a built-in name such as
/// `Promise`, `Iterable`, or a known lib path; those names are not user-
/// chosen and renaming them is meaningless.
pub fn shape_variants(source: &str, variants: &[ShapeVariant]) -> Vec<ShapeVariantSource> {
    let mut out = Vec::with_capacity(variants.len() + 1);
    out.push(ShapeVariantSource {
        label: "original",
        source: source.to_string(),
    });
    for v in variants {
        out.push(ShapeVariantSource {
            label: v.label,
            source: apply_variant(source, v),
        });
    }
    out
}

/// Apply a single `ShapeVariant` to a fourslash source string.
///
/// See the module-level docs for the preservation contract.
pub fn apply_variant(source: &str, variant: &ShapeVariant) -> String {
    if variant.identifier_renames.is_empty() && variant.path_renames.is_empty() {
        return source.to_string();
    }
    let ident_renames: FxHashMap<&str, &str> = variant.identifier_renames.iter().copied().collect();
    let mut out = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = marker_end(bytes, i) {
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let end = block_comment_end(bytes, i + 2);
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let end = line_end(bytes, i);
            let line = &source[i..end];
            out.push_str(&rewrite_line_comment(line, variant.path_renames));
            i = end;
            continue;
        }
        // Backtick-delimited bodies are treated like string bodies — we do not
        // descend into template expressions, which fourslash fixtures rarely
        // use; conservatively skipping them is safe.
        if bytes[i] == b'\'' || bytes[i] == b'"' || bytes[i] == b'`' {
            let quote = bytes[i];
            let end = string_end(bytes, i + 1, quote);
            // `string_end` returns the position past the closing quote when
            // the string is terminated, or end-of-source when unterminated.
            let closed = end > i + 1 && bytes[end - 1] == quote;
            let body_end = if closed { end - 1 } else { end };
            let body = &source[i + 1..body_end];
            out.push(quote as char);
            out.push_str(&apply_path_renames(body, variant.path_renames));
            if closed {
                out.push(quote as char);
            }
            i = end;
            continue;
        }
        if is_ident_start(bytes[i]) {
            let mut j = i + 1;
            while j < bytes.len() && is_ident_continue(bytes[j]) {
                j += 1;
            }
            let ident = &source[i..j];
            if let Some(replacement) = ident_renames.get(ident) {
                out.push_str(replacement);
            } else {
                out.push_str(ident);
            }
            i = j;
            continue;
        }
        // Copy one UTF-8 character verbatim. `source` is `&str`, so the
        // remaining slice is valid UTF-8 and has at least one char.
        let ch = source[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

// -----------------------------------------------------------------------------
// Internal lexing helpers.
// -----------------------------------------------------------------------------

const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

const fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// If `bytes[i..]` starts with a marker comment `/*name*/`, return the byte
/// offset just past the closing `*/`. Delegates to
/// `fourslash::find_marker_end` so the variant generator and
/// `parse_markers` always agree on what counts as a marker.
fn marker_end(bytes: &[u8], i: usize) -> Option<usize> {
    if i + 3 >= bytes.len() || bytes[i] != b'/' || bytes[i + 1] != b'*' {
        return None;
    }
    crate::fourslash::find_marker_end(&bytes[i + 2..]).map(|rel| i + 2 + rel + 2)
}

/// Given the byte offset after a block-comment opener `/*`, return the
/// offset just past the closing `*/`, or end-of-source if unterminated.
fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut j = start;
    while j + 1 < bytes.len() {
        if bytes[j] == b'*' && bytes[j + 1] == b'/' {
            return j + 2;
        }
        j += 1;
    }
    bytes.len()
}

/// Return the byte offset of the next newline at or after `i`, including
/// the newline character itself.
fn line_end(bytes: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < bytes.len() && bytes[j] != b'\n' {
        j += 1;
    }
    if j < bytes.len() { j + 1 } else { j }
}

/// Return the byte offset just past the closing quote of a string starting
/// at `i` (one past the opening quote). Honors `\\` escapes. If
/// unterminated, returns end-of-source.
fn string_end(bytes: &[u8], i: usize, quote: u8) -> usize {
    let mut j = i;
    while j < bytes.len() {
        let b = bytes[j];
        if b == b'\\' && j + 1 < bytes.len() {
            j += 2;
            continue;
        }
        if b == quote {
            return j + 1;
        }
        // Single-quoted and double-quoted strings stop at newline in real
        // JS, but treating them as multi-line is safe here because we
        // never *interpret* the body, only forward it. Continue scanning.
        j += 1;
    }
    bytes.len()
}

fn rewrite_line_comment(line: &str, path_renames: &[(&str, &str)]) -> String {
    let trimmed_start = line.trim_start();
    let Some((prefix, path)) = crate::fourslash::strip_filename_directive(trimmed_start) else {
        return line.to_string();
    };
    let leading_ws = &line[..line.len() - trimmed_start.len()];
    let mut rewritten = String::with_capacity(line.len());
    rewritten.push_str(leading_ws);
    rewritten.push_str(prefix);
    rewritten.push_str(&apply_path_renames(path, path_renames));
    rewritten
}

// Pairs are applied sequentially, so `[("a", "b"), ("b", "c")]` composes
// `a -> c`. Variant authors should declare disjoint substrings to avoid
// surprises. Acceptable for the small `k` (1–2 path renames) and small body
// sizes (string literals, `@filename:` paths) seen in fourslash fixtures.
fn apply_path_renames(body: &str, path_renames: &[(&str, &str)]) -> String {
    if path_renames.is_empty() {
        return body.to_string();
    }
    let mut current = body.to_string();
    for (old, new) in path_renames {
        if !old.is_empty() && current.contains(old) {
            current = current.replace(old, new);
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    const RENAME_T_TO_K: ShapeVariant = ShapeVariant {
        label: "rename_T_to_K",
        identifier_renames: &[("T", "K")],
        path_renames: &[],
    };

    const RENAME_FOO_AND_BAR: ShapeVariant = ShapeVariant {
        label: "rename_foo_bar",
        identifier_renames: &[("foo", "renamedFoo"), ("bar", "renamedBar")],
        path_renames: &[],
    };

    const RENAME_A_TS_PATH: ShapeVariant = ShapeVariant {
        label: "rename_a_ts",
        identifier_renames: &[],
        path_renames: &[("./a", "./renamed-a"), ("a.ts", "renamed-a.ts")],
    };

    #[test]
    fn preserves_marker_names() {
        let src = "function /*def*/T() {}";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "function /*def*/K() {}");
    }

    #[test]
    fn whole_token_only() {
        // `Tree` and `Tea` contain `T` as a prefix but are not whole-token T.
        let src = "type T = Tree; type Tea = T;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "type K = Tree; type Tea = K;");
    }

    #[test]
    fn rename_inside_strings_is_left_alone() {
        let src = r#"const T: string = "T is here"; type T2 = T;"#;
        let out = apply_variant(src, &RENAME_T_TO_K);
        // The string body should be untouched even though it contains "T".
        assert_eq!(out, r#"const K: string = "T is here"; type T2 = K;"#);
    }

    #[test]
    fn rename_inside_block_comments_is_left_alone() {
        let src = "/* T is the type parameter */ type T = number;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "/* T is the type parameter */ type K = number;");
    }

    #[test]
    fn rename_inside_line_comments_is_left_alone() {
        let src = "// T is a comment\ntype T = number;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "// T is a comment\ntype K = number;");
    }

    #[test]
    fn name_swap_in_one_pass() {
        // Swap T <-> K. Naive sequential application would lose one.
        const SWAP: ShapeVariant = ShapeVariant {
            label: "swap",
            identifier_renames: &[("T", "K"), ("K", "T")],
            path_renames: &[],
        };
        let src = "type T = K; type K = T;";
        let out = apply_variant(src, &SWAP);
        assert_eq!(out, "type K = T; type T = K;");
    }

    #[test]
    fn multi_identifier_rename() {
        let src = "function foo() { return bar(); }";
        let out = apply_variant(src, &RENAME_FOO_AND_BAR);
        assert_eq!(out, "function renamedFoo() { return renamedBar(); }");
    }

    #[test]
    fn at_filename_directive_path_is_rewritten() {
        let src = "// @filename: a.ts\nexport const x = 1;\n// @filename: b.ts\nimport './a';";
        let out = apply_variant(src, &RENAME_A_TS_PATH);
        assert!(out.contains("// @filename: renamed-a.ts"));
        assert!(out.contains("'./renamed-a'"));
        // `b.ts` was not renamed and must remain.
        assert!(out.contains("// @filename: b.ts"));
    }

    #[test]
    fn path_renames_do_not_touch_identifiers() {
        // path_renames are substring-only inside strings/filename directives;
        // they must not edit identifiers in code.
        let src = "const a = 1;\nimport './a';";
        let out = apply_variant(src, &RENAME_A_TS_PATH);
        // The identifier `a` is untouched. The string `'./a'` is rewritten.
        assert!(out.contains("const a = 1;"));
        assert!(out.contains("'./renamed-a'"));
    }

    #[test]
    fn empty_variant_returns_input_verbatim() {
        const NO_RENAMES: ShapeVariant = ShapeVariant {
            label: "noop",
            identifier_renames: &[],
            path_renames: &[],
        };
        let src = "const /*x*/x = 1;";
        let out = apply_variant(src, &NO_RENAMES);
        assert_eq!(out, src);
    }

    #[test]
    fn shape_variants_includes_original_first() {
        let src = "type T = number;";
        let outs = shape_variants(src, &[RENAME_T_TO_K]);
        assert_eq!(outs.len(), 2);
        assert_eq!(outs[0].label, "original");
        assert_eq!(outs[0].source, src);
        assert_eq!(outs[1].label, "rename_T_to_K");
        assert_eq!(outs[1].source, "type K = number;");
    }

    #[test]
    fn multi_line_block_comment_is_preserved_verbatim() {
        // A block comment that spans newlines is not a marker and not
        // identifier-rewritten either; its body is forwarded as-is.
        let src = "/* T is\n   the param */ type T = number;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "/* T is\n   the param */ type K = number;");
    }

    #[test]
    fn empty_marker_is_preserved() {
        let src = "foo(/**/);";
        let out = apply_variant(src, &RENAME_FOO_AND_BAR);
        assert_eq!(out, "renamedFoo(/**/);");
    }

    #[test]
    fn non_ascii_bytes_are_copied_as_whole_utf8_chars() {
        // The default-byte branch must copy a multi-byte UTF-8 char in one
        // step. A naive `bytes[i] as char` would emit Latin-1 garbage for
        // the lead byte and then re-enter the loop on a continuation byte.
        let src = "const T = 'π'; type T = number;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "const K = 'π'; type K = number;");
    }

    #[test]
    fn unterminated_string_does_not_panic() {
        // Defensive: an unterminated string in a fixture must not crash the
        // generator. We forward the unterminated body verbatim.
        let src = "const x = 'unterminated;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, src);
    }

    #[test]
    fn backtick_template_body_is_not_rewritten_for_identifiers() {
        // Template-literal bodies are treated like string bodies for the
        // purpose of identifier rewriting (i.e. left alone). This is the
        // conservative choice; fourslash fixtures rarely contain templates.
        let src = "const T = `T inside template`; type T2 = T;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "const K = `T inside template`; type T2 = K;");
    }
}
