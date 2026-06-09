//! JSDoc `@typedef` / `@callback` document-symbol collection.
//!
//! tsz does not parse JSDoc into structured AST nodes; the binder and checker
//! both discover JSDoc tags by scanning the raw comment ranges cached on
//! [`tsz_parser::parser::node::SourceFileData`] (`comments`) and parsing the
//! tag text directly (see `tsz_checker`'s `parse_jsdoc_typedefs` and the
//! binder's `core_jsdoc`). Document symbols follow the same model: tsserver's
//! navigation tree surfaces every `@typedef` and `@callback` declaration as a
//! `type` leaf (`ScriptElementKind.typeElement`), regardless of whether the
//! host file is JS or TS, so this scanner walks the file's JSDoc comments and
//! produces one [`DocumentSymbolEntry`] per type-alias name.
//!
//! Name extraction mirrors the checker's tag grammar, including the
//! type-then-name form (`@typedef {T}` with the name on a later line) and the
//! object-wrapper form (`@typedef {{ ... }} Name`), by tracking brace depth
//! across lines. Because the underlying tag node is unavailable, ranges are
//! computed from byte offsets into the raw comment text: the selection range
//! covers the name identifier and the full range spans from the `@` tag marker
//! to the end of the name.
//!
//! Scope: only the two `type`-kinded type-alias tags are handled here.
//! tsserver's `isJSDocTypeAlias` also covers `@enum`, but tsc surfaces that
//! with a distinct `ScriptElementKind` (`enum`) and pairs it with the host
//! object declaration's members, so it is deliberately left to a separate
//! change rather than folded in as a third `type` leaf.

use tsz_common::comments::{CommentRange, is_jsdoc_comment};
use tsz_common::position::{LineMap, Position, Range};

use super::model::{DocumentSymbolEntry, SymbolKind};

/// Walk the file's JSDoc comments and emit a `type` document-symbol entry for
/// every `@typedef` / `@callback` declaration, in source order.
pub(super) fn collect_jsdoc_type_aliases(
    comments: &[CommentRange],
    source: &str,
    line_map: &LineMap,
) -> Vec<DocumentSymbolEntry> {
    // Cheap reject: avoid scanning every comment when the file cannot contain a
    // type-alias tag at all. Mirrors the checker's pre-scan in `jsdoc::lookup`.
    if comments.is_empty() || (!source.contains("@typedef") && !source.contains("@callback")) {
        return Vec::new();
    }

    let mut entries = Vec::new();
    for comment in comments {
        if !is_jsdoc_comment(comment, source) {
            continue;
        }
        scan_comment(comment, source, line_map, &mut entries);
    }
    entries
}

/// Parser state while walking the lines of a single JSDoc comment.
enum Mode {
    /// Looking for a fresh `@typedef` / `@callback` tag at the start of a line.
    Scan,
    /// Inside a `@typedef`'s brace-delimited type expression that has not yet
    /// closed; `depth` is the current unbalanced `{` count and `tag_abs` is the
    /// absolute byte offset of the owning `@typedef` marker.
    InType { depth: usize, tag_abs: usize },
    /// The `@typedef`'s type expression closed with no trailing name on its
    /// line; the name is the next bare token on a following content line.
    ExpectName { tag_abs: usize },
}

fn scan_comment(
    comment: &CommentRange,
    source: &str,
    line_map: &LineMap,
    out: &mut Vec<DocumentSymbolEntry>,
) {
    let raw = comment.get_text(source);
    let comment_base = comment.pos as usize;
    let mut mode = Mode::Scan;

    let mut line_start = 0usize;
    for line in raw.split_inclusive('\n') {
        let abs_line_start = comment_base + line_start;
        line_start += line.len();

        let (skipped, content) = strip_jsdoc_line_decoration(line);
        let content_abs = abs_line_start + skipped;

        match mode {
            Mode::InType { depth, tag_abs } => match advance_braces(content, depth) {
                BraceScan::Open { depth } => {
                    mode = Mode::InType { depth, tag_abs };
                }
                BraceScan::Closed { residual } => {
                    mode = finish_typedef_after_type(
                        content,
                        content_abs,
                        residual,
                        tag_abs,
                        source,
                        line_map,
                        out,
                    );
                }
            },
            Mode::ExpectName { tag_abs } => {
                let trimmed = content.trim_start();
                if trimmed.is_empty() {
                    // Blank continuation line; keep waiting for the name.
                    mode = Mode::ExpectName { tag_abs };
                } else if trimmed.starts_with('@') {
                    // A new tag terminates the pending typedef without a name;
                    // re-dispatch this line as a fresh tag.
                    mode = scan_tag_line(content, content_abs, source, line_map, out);
                } else {
                    push_name_token(content, content_abs, tag_abs, source, line_map, out);
                    mode = Mode::Scan;
                }
            }
            Mode::Scan => {
                mode = scan_tag_line(content, content_abs, source, line_map, out);
            }
        }
    }
}

/// Dispatch a line that may begin a `@typedef` / `@callback` tag. Returns the
/// mode to continue scanning subsequent lines with.
fn scan_tag_line(
    content: &str,
    content_abs: usize,
    source: &str,
    line_map: &LineMap,
    out: &mut Vec<DocumentSymbolEntry>,
) -> Mode {
    let trimmed = content.trim_start();
    if !trimmed.starts_with('@') {
        return Mode::Scan;
    }
    // Account for whitespace skipped by `trim_start` so offsets stay accurate.
    let lead = content.len() - trimmed.len();
    let tag_abs = content_abs + lead;

    if let Some(rest) = strip_tag(trimmed, "typedef") {
        let rest_abs = tag_abs + "@typedef".len();
        return begin_typedef(rest, rest_abs, tag_abs, source, line_map, out);
    }
    if let Some(rest) = strip_tag(trimmed, "callback") {
        let rest_abs = tag_abs + "@callback".len();
        // A callback's name must appear on the tag line itself.
        push_name_token(rest, rest_abs, tag_abs, source, line_map, out);
    }
    Mode::Scan
}

/// Handle the body of a `@typedef` tag once the `@typedef` marker is consumed.
/// `rest` is the text after `@typedef`; `rest_abs` its absolute offset.
fn begin_typedef(
    rest: &str,
    rest_abs: usize,
    tag_abs: usize,
    source: &str,
    line_map: &LineMap,
    out: &mut Vec<DocumentSymbolEntry>,
) -> Mode {
    let lead = rest.len() - rest.trim_start().len();
    let trimmed = &rest[lead..];
    if trimmed.starts_with('{') {
        let brace_abs = rest_abs + lead;
        match advance_braces(trimmed, 0) {
            BraceScan::Open { depth } => Mode::InType { depth, tag_abs },
            BraceScan::Closed { residual } => finish_typedef_after_type(
                trimmed, brace_abs, residual, tag_abs, source, line_map, out,
            ),
        }
    } else {
        // `@typedef Name` — no type expression; the name is on this line.
        push_name_token(rest, rest_abs, tag_abs, source, line_map, out);
        Mode::Scan
    }
}

/// After a typedef's type expression closes (`residual` is the byte index just
/// past the closing brace within `content`), capture a same-line trailing name
/// if present, otherwise wait for it on a following line.
fn finish_typedef_after_type(
    content: &str,
    content_abs: usize,
    residual: Option<usize>,
    tag_abs: usize,
    source: &str,
    line_map: &LineMap,
    out: &mut Vec<DocumentSymbolEntry>,
) -> Mode {
    let Some(residual) = residual else {
        return Mode::ExpectName { tag_abs };
    };
    let after = &content[residual..];
    let after_trimmed = after.trim_start();
    if after_trimmed.is_empty() || after_trimmed.starts_with('@') {
        return Mode::ExpectName { tag_abs };
    }
    push_name_token(
        after,
        content_abs + residual,
        tag_abs,
        source,
        line_map,
        out,
    );
    Mode::Scan
}

/// Result of scanning a (partial) brace-delimited type expression on one line.
enum BraceScan {
    /// The expression is still open after this line; `depth` braces remain.
    Open { depth: usize },
    /// The expression closed; `residual` is the byte index just past the
    /// closing `}` within the scanned slice, or `None` when it ended the slice.
    Closed { residual: Option<usize> },
}

/// Scan `s` from byte index `start`, updating an existing brace `depth`. Quotes
/// (`'`, `"`, `` ` ``) are honored so braces inside string literals in a type
/// expression do not unbalance the count, matching the checker's
/// `parse_jsdoc_curly_type_expr`.
fn advance_braces(s: &str, mut depth: usize) -> BraceScan {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in s.char_indices() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let next = idx + ch.len_utf8();
                    return BraceScan::Closed {
                        residual: (next < s.len()).then_some(next),
                    };
                }
            }
            _ => {}
        }
    }
    BraceScan::Open { depth }
}

/// Extract the first whitespace-delimited identifier from `text` and, if it is
/// a valid type-alias name, push a `type` entry for it. `text_abs` is the
/// absolute byte offset of `text[0]`; `tag_abs` is the offset of the owning
/// `@typedef` / `@callback` marker (used for the enclosing range).
fn push_name_token(
    text: &str,
    text_abs: usize,
    tag_abs: usize,
    source: &str,
    line_map: &LineMap,
    out: &mut Vec<DocumentSymbolEntry>,
) {
    let lead = text.len() - text.trim_start().len();
    let rest = &text[lead..];
    if rest.is_empty() || rest.starts_with('@') {
        return;
    }
    let Some(token) = rest.split_whitespace().next() else {
        return;
    };
    let name = normalize_name(token);
    if !is_valid_type_alias_name(name) {
        return;
    }

    let name_abs = text_abs + lead;
    let name_end = name_abs + name.len();
    let selection = byte_range(line_map, source, name_abs, name_end);
    let range = byte_range(line_map, source, tag_abs.min(name_abs), name_end);

    out.push(DocumentSymbolEntry {
        name: name.to_string(),
        detail: None,
        kind: SymbolKind::Struct,
        kind_modifiers: String::new(),
        range,
        selection_range: selection,
        container_name: None,
        children: Vec::new(),
    });
}

/// Strip a JSDoc tag prefix (`@typedef`, `@callback`) when `line` begins with
/// it followed by a tag boundary. Returns the remainder after the tag name.
fn strip_tag<'a>(line: &'a str, tag: &str) -> Option<&'a str> {
    let rest = line.strip_prefix('@')?.strip_prefix(tag)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if !c.is_ascii_alphanumeric() && c != '_' => Some(rest),
        Some(_) => None,
    }
}

/// Strip leading whitespace and a single JSDoc line decoration (`/**`, `/*`, or
/// `*`) plus following whitespace, returning the bytes skipped and the content.
fn strip_jsdoc_line_decoration(line: &str) -> (usize, &str) {
    let after_ws = line.trim_start();
    let undecorated = after_ws
        .strip_prefix("/**")
        .or_else(|| after_ws.strip_prefix("/*"))
        .or_else(|| after_ws.strip_prefix('*'))
        .unwrap_or(after_ws);
    let content = undecorated.trim_start();
    (line.len() - content.len(), content)
}

/// Normalize a raw name token to the bare alias name: drop a trailing comma /
/// semicolon, a glued comment terminator (`*/`), and any generic parameter list
/// (`Name<T>` → `Name`). Mirrors the checker's `normalize_jsdoc_typedef_name`.
fn normalize_name(token: &str) -> &str {
    let punctuation = |c| c == ',' || c == ';';
    let mut name = token.trim_end_matches(punctuation);
    if let Some(stripped) = name.strip_suffix("*/") {
        name = stripped;
    }
    if let Some(angle) = name.find('<') {
        name = &name[..angle];
    }
    // The `*/` / generic truncation can re-expose a trailing separator.
    name.trim_end_matches(punctuation)
}

/// A type-alias name is an identifier optionally containing `.` (namespaced
/// callback names) — matching the checker's callback-name validation and
/// rejecting type-expression fragments such as `{Object}`.
fn is_valid_type_alias_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.')
}

/// Build an LSP [`Range`] spanning the byte interval `[start, end)`.
fn byte_range(line_map: &LineMap, source: &str, start: usize, end: usize) -> Range {
    let start = u32::try_from(start).unwrap_or(u32::MAX);
    let end = u32::try_from(end).unwrap_or(u32::MAX);
    Range::new(
        line_map.offset_to_position(start, source),
        line_map.offset_to_position(end, source),
    )
}

/// Whether `a` is at or before `b` in source order. Used by the caller to
/// insert type-alias entries into the top-level symbol list in source order.
pub(super) const fn position_le(a: Position, b: Position) -> bool {
    a.line < b.line || (a.line == b.line && a.character <= b.character)
}
