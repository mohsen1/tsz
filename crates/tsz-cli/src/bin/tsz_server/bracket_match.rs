//! Byte-level bracket/paren matching helpers used by structure navigation.
//!
//! Split out of `main.rs` to stay under the 2000-LOC boundary (#16733).

use tsz::parser::node::NodeArena;

/// `map` reflects bytes already classified by `build_code_map` for
/// string/comment/regex literals, so this only needs to look at "in code"
/// bytes to decide whether a `/` at `pos` starts a regex literal.
fn is_regex_literal_start(bytes: &[u8], map: &[bool], pos: usize) -> bool {
    let mut j = pos;
    while j > 0 {
        j -= 1;
        let b = bytes[j];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            continue;
        }
        if !map[j] {
            // Inside a string/comment/template — skip.
            continue;
        }
        if matches!(
            b,
            b'=' | b','
                | b'('
                | b'['
                | b'{'
                | b';'
                | b':'
                | b'?'
                | b'!'
                | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b'&'
                | b'|'
                | b'^'
                | b'<'
                | b'>'
                | b'~'
        ) {
            return true;
        }
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'$' {
            // Walk back to the start of the identifier and decide based on
            // whether it is a keyword that introduces an expression.
            let end = j + 1;
            let mut start = j;
            while start > 0 {
                let bb = bytes[start - 1];
                if (bb.is_ascii_alphanumeric() || bb == b'_' || bb == b'$') && map[start - 1] {
                    start -= 1;
                } else {
                    break;
                }
            }
            return matches!(
                &bytes[start..end],
                b"return"
                    | b"typeof"
                    | b"delete"
                    | b"void"
                    | b"in"
                    | b"of"
                    | b"instanceof"
                    | b"new"
                    | b"throw"
                    | b"do"
                    | b"else"
                    | b"case"
                    | b"yield"
                    | b"await"
            );
        }
        // `)`, `]`, `}`, quotes, digits already handled above as alnum: end
        // of expression — `/` is division.
        return false;
    }
    // Start of file.
    true
}

/// Build a boolean map indicating which byte positions are "in code"
/// (i.e., not inside a string literal, comment, or regex literal).
pub(super) fn build_code_map(bytes: &[u8]) -> Vec<bool> {
    let len = bytes.len();
    let mut map = vec![true; len];
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'/' if i + 1 < len => {
                if bytes[i + 1] == b'/' {
                    // Single-line comment
                    map[i] = false;
                    map[i + 1] = false;
                    i += 2;
                    while i < len && bytes[i] != b'\n' {
                        map[i] = false;
                        i += 1;
                    }
                } else if bytes[i + 1] == b'*' {
                    // Multi-line comment
                    map[i] = false;
                    map[i + 1] = false;
                    i += 2;
                    while i < len {
                        if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                            map[i] = false;
                            map[i + 1] = false;
                            i += 2;
                            break;
                        }
                        map[i] = false;
                        i += 1;
                    }
                } else if is_regex_literal_start(bytes, &map, i) {
                    // Regex literal `/.../flags`. Mark its body so brace
                    // scanning ignores `{` / `}` that appear inside.
                    map[i] = false;
                    i += 1;
                    let mut in_class = false;
                    while i < len {
                        match bytes[i] {
                            b'\\' => {
                                map[i] = false;
                                i += 1;
                                if i < len && bytes[i] != b'\n' {
                                    map[i] = false;
                                    i += 1;
                                }
                            }
                            b'[' if !in_class => {
                                in_class = true;
                                map[i] = false;
                                i += 1;
                            }
                            b']' if in_class => {
                                in_class = false;
                                map[i] = false;
                                i += 1;
                            }
                            b'/' if !in_class => {
                                map[i] = false;
                                i += 1;
                                while i < len && bytes[i].is_ascii_alphabetic() {
                                    map[i] = false;
                                    i += 1;
                                }
                                break;
                            }
                            b'\n' => {
                                // Unterminated regex literal.
                                break;
                            }
                            _ => {
                                map[i] = false;
                                i += 1;
                            }
                        }
                    }
                } else {
                    i += 1;
                }
            }
            b'"' | b'\'' => {
                let quote = bytes[i];
                map[i] = false;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        map[i] = false;
                        i += 1;
                        if i < len {
                            map[i] = false;
                            i += 1;
                        }
                    } else if bytes[i] == quote {
                        map[i] = false;
                        i += 1;
                        break;
                    } else if bytes[i] == b'\n' {
                        // Unterminated string at newline
                        break;
                    } else {
                        map[i] = false;
                        i += 1;
                    }
                }
            }
            b'`' => {
                // Template literal - mark everything inside as non-code
                // except for ${...} substitutions
                map[i] = false;
                i += 1;
                let mut depth = 0u32;
                while i < len {
                    if bytes[i] == b'\\' {
                        map[i] = false;
                        i += 1;
                        if i < len {
                            map[i] = false;
                            i += 1;
                        }
                    } else if bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
                        // Template substitution - these are code
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'{' && depth > 0 {
                        depth += 1;
                        i += 1;
                    } else if bytes[i] == b'}' && depth > 0 {
                        depth -= 1;
                        i += 1;
                    } else if bytes[i] == b'`' && depth == 0 {
                        map[i] = false;
                        i += 1;
                        break;
                    } else {
                        if depth == 0 {
                            map[i] = false;
                        }
                        i += 1;
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    map
}

/// Scan forward from `start` (exclusive) to find the matching closing brace.
/// Returns the byte offset of the matching close brace, or None.
pub(super) fn scan_forward(
    bytes: &[u8],
    code_map: &[bool],
    start: usize,
    open: u8,
    close: u8,
) -> Option<usize> {
    let mut depth = 1i32;
    let mut i = start + 1;
    while i < bytes.len() {
        if code_map[i] {
            if bytes[i] == open {
                depth += 1;
            } else if bytes[i] == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// Scan backward from `start` (exclusive) to find the matching opening brace.
/// Returns the byte offset of the matching open brace, or None.
pub(super) fn scan_backward(
    bytes: &[u8],
    code_map: &[bool],
    start: usize,
    close: u8,
    open: u8,
) -> Option<usize> {
    let mut depth = 1i32;
    let mut i = start;
    while i > 0 {
        i -= 1;
        if code_map[i] {
            if bytes[i] == close {
                depth += 1;
            } else if bytes[i] == open {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Find matching angle bracket using AST-based analysis.
/// Returns the byte offset of the matching bracket, or None.
pub(super) fn find_angle_bracket_match(
    arena: &NodeArena,
    source: &str,
    pos: usize,
) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut pairs: Vec<(usize, usize)> = Vec::new();

    // Derive angle bracket positions from NodeList children.
    // The NodeList pos/end may be 0/0 (unset), but we can find the `<` and `>`
    // by looking at the first/last child nodes:
    //   `<` is at first_child.pos - 1
    //   `>` is at last_child.end - 1 (if parser includes `>` in range)
    //        or last_child.end (if parser excludes `>` from range)
    let check_list_nodes = |list: &Option<tsz::parser::base::NodeList>| -> Option<(usize, usize)> {
        let list = list.as_ref()?;
        if list.nodes.is_empty() {
            return None;
        }
        let first = arena.nodes.get(list.nodes.first()?.0 as usize)?;
        let last = arena.nodes.get(list.nodes.last()?.0 as usize)?;

        let open_pos = (first.pos as usize).checked_sub(1)?;
        if bytes.get(open_pos) != Some(&b'<') {
            return None;
        }

        // Try last_child.end - 1 first (parser includes `>` in range)
        let close_candidate1 = last.end as usize;
        if close_candidate1 > 0 && bytes.get(close_candidate1 - 1) == Some(&b'>') {
            return Some((open_pos, close_candidate1 - 1));
        }
        // Try last_child.end (parser excludes `>` from range)
        if bytes.get(close_candidate1) == Some(&b'>') {
            return Some((open_pos, close_candidate1));
        }
        None
    };

    // Collect from all data pools that have type_parameters or type_arguments
    for f in &arena.functions {
        if let Some(pair) = check_list_nodes(&f.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.classes {
        if let Some(pair) = check_list_nodes(&c.type_parameters) {
            pairs.push(pair);
        }
    }
    for iface in &arena.interfaces {
        if let Some(pair) = check_list_nodes(&iface.type_parameters) {
            pairs.push(pair);
        }
    }
    for t in &arena.type_aliases {
        if let Some(pair) = check_list_nodes(&t.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.call_exprs {
        if let Some(pair) = check_list_nodes(&c.type_arguments) {
            pairs.push(pair);
        }
    }
    for t in &arena.type_refs {
        if let Some(pair) = check_list_nodes(&t.type_arguments) {
            pairs.push(pair);
        }
    }
    for s in &arena.signatures {
        if let Some(pair) = check_list_nodes(&s.type_parameters) {
            pairs.push(pair);
        }
    }
    for m in &arena.method_decls {
        if let Some(pair) = check_list_nodes(&m.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.constructors {
        if let Some(pair) = check_list_nodes(&c.type_parameters) {
            pairs.push(pair);
        }
    }
    for ft in &arena.function_types {
        if let Some(pair) = check_list_nodes(&ft.type_parameters) {
            pairs.push(pair);
        }
    }
    for e in &arena.expr_with_type_args {
        if let Some(pair) = check_list_nodes(&e.type_arguments) {
            pairs.push(pair);
        }
    }

    // Type assertions: <type>expr
    for node in &arena.nodes {
        if node.kind == tsz::parser::syntax_kind_ext::TYPE_ASSERTION
            && let Some(ta) = arena.type_assertions.get(node.data_index as usize)
        {
            let open_pos = node.pos as usize;
            if bytes.get(open_pos) != Some(&b'<') {
                continue;
            }
            if let Some(type_node) = arena.nodes.get(ta.type_node.0 as usize) {
                // `>` might be at type_node.end - 1 or type_node.end
                let end = type_node.end as usize;
                if end > 0 && bytes.get(end - 1) == Some(&b'>') {
                    pairs.push((open_pos, end - 1));
                } else if bytes.get(end) == Some(&b'>') {
                    pairs.push((open_pos, end));
                }
            }
        }
    }

    // Search for the position in collected pairs
    for (open, close) in pairs {
        if pos == open {
            return Some(close);
        } else if pos == close {
            return Some(open);
        }
    }

    None
}
