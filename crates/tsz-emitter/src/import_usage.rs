/// Shared text-based heuristics for detecting value vs type-only usage of imports.
///
/// Both the lowering pass and the emitter need to determine whether an import
/// binding is used in value positions (requiring helper emission / runtime code)
/// or only in type positions (safe to erase).  These functions provide text-based
/// analysis that works without full type information.

/// Check if `haystack` contains `ident` as a standalone identifier (not part of
/// a larger word).
pub fn contains_identifier_occurrence(haystack: &str, ident: &str) -> bool {
    if ident.is_empty() {
        return false;
    }
    let mut search_from = 0usize;
    while let Some(rel) = haystack[search_from..].find(ident) {
        let pos = search_from + rel;
        let before_ok = if pos == 0 {
            true
        } else {
            haystack[..pos]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        };
        let after_idx = pos + ident.len();
        let after_ok = if after_idx >= haystack.len() {
            true
        } else {
            haystack[after_idx..]
                .chars()
                .next()
                .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        };
        if before_ok && after_ok {
            return true;
        }
        search_from = pos + ident.len();
    }
    false
}

/// Strip type-only content from source text so that identifiers in type
/// positions are not mistaken for value usages.
///
/// This handles:
/// - Lines starting with `declare` (ambient declarations)
/// - Lines that are `import type` or `export type` statements
/// - Lines starting with `type` or `interface` (type alias / interface declarations)
/// - Type annotations after `):`/`?:`/`]:` (return types, optional param types)
/// - Type annotations on `const`/`let`/`var` lines before `=`
/// - `implements` clauses (always type-only, unlike `extends` which is value-level)
/// - Other `import`/`export` statements (identifiers in other imports are not value usages)
pub fn strip_type_only_content(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim();
        // Skip entirely type-only lines
        if trimmed.starts_with("declare ")
            || trimmed.starts_with("import type ")
            || trimmed.starts_with("import type{")
            || trimmed.starts_with("export type ")
            || trimmed.starts_with("export type{")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("interface ")
            // Other import statements - identifiers from other
            // imports should not count as value usages of *this* import
            || trimmed.starts_with("import ")
            || trimmed.starts_with("import{")
            // Re-export statements (but NOT value-level export declarations
            // like `export var/let/const/function/class/default/enum/abstract/async`)
            || trimmed.starts_with("export{")
            || trimmed.starts_with("export {")
            || trimmed.starts_with("export *")
            || trimmed.starts_with("export import ")
        {
            result.push('\n');
            continue;
        }
        // For remaining lines, strip type annotations
        let stripped = strip_type_annotations_safe(line);
        result.push_str(&stripped);
        result.push('\n');
    }
    result
}

/// Strip type annotations from a line of code while preserving value positions.
///
/// Strips:
/// - Return type annotations: `): Type` or `): Type {`
/// - Optional parameter types: `?: Type`
/// - Array element types: `]: Type`
/// - Variable type annotations on const/let/var lines BEFORE `=`
/// - `implements` clauses (always type-only in class declarations)
///
/// Does NOT strip:
/// - Object literal values after `:` (e.g., `{ key: value }`)
/// - Ternary operator expressions
/// - `extends` in class declarations (base class is value-level / runs at runtime)
fn strip_type_annotations_safe(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    // Check if this is a const/let/var line for variable type annotation stripping
    let trimmed = line.trim();
    let is_var_line =
        trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ");

    while i < len {
        match bytes[i] {
            // Skip string literals entirely to avoid false matches on `:` inside strings
            b'"' | b'\'' | b'`' => {
                let quote = bytes[i];
                result.push(quote as char);
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' && i + 1 < len {
                        result.push(bytes[i] as char);
                        result.push(bytes[i + 1] as char);
                        i += 2;
                    } else if bytes[i] == quote {
                        result.push(quote as char);
                        i += 1;
                        break;
                    } else {
                        result.push(bytes[i] as char);
                        i += 1;
                    }
                }
            }
            // Skip line comments — don't let identifiers in comments count
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                break; // discard rest of line
            }
            // After `):`, `?:`, or `]:` — this is a type annotation
            b':' if i > 0
                && (bytes[i - 1] == b')' || bytes[i - 1] == b'?' || bytes[i - 1] == b']') =>
            {
                // Skip until `{`, `=>`, `,`, `)`, or end of line
                i += 1;
                i = skip_type_annotation(bytes, i);
            }
            // On const/let/var lines, `:` before the first `=` is a type annotation
            b':' if is_var_line && is_var_type_annotation_colon(line, i) => {
                i += 1;
                i = skip_type_annotation(bytes, i);
            }
            _ => {
                result.push(bytes[i] as char);
                i += 1;
            }
        }
    }

    // Strip `implements` clauses (always type-only)
    if let Some(impl_pos) = result.find(" implements ") {
        // Check it's not inside a string by verifying balanced quotes before it
        let before = &result[..impl_pos];
        let single_quotes = before.chars().filter(|&c| c == '\'').count();
        let double_quotes = before.chars().filter(|&c| c == '"').count();
        if single_quotes % 2 == 0 && double_quotes % 2 == 0 {
            // Find the `{` that opens the class body after implements
            if let Some(brace_pos) = result[impl_pos..].find('{') {
                let after_impl = &result[impl_pos + brace_pos..];
                result = format!("{} {}", &result[..impl_pos], after_impl);
            }
        }
    }

    result
}

/// Check if a `:` at position `colon_pos` in a const/let/var line is a type
/// annotation (before `=`) rather than an object literal property separator
/// (after `=`).
fn is_var_type_annotation_colon(line: &str, colon_pos: usize) -> bool {
    let bytes = line.as_bytes();
    // Find the first `=` that is not `==` or `=>`
    let mut j = 0;
    while j < bytes.len() {
        if bytes[j] == b'=' {
            // Skip `==`, `===`, `=>`
            if j + 1 < bytes.len() && (bytes[j + 1] == b'=' || bytes[j + 1] == b'>') {
                j += 2;
                if j < bytes.len() && bytes[j] == b'=' {
                    j += 1; // ===
                }
                continue;
            }
            // This is an assignment `=`
            return colon_pos < j;
        }
        j += 1;
    }
    // No `=` found — the whole line is a declaration without initializer
    // e.g., `let x: number;` — the `:` IS a type annotation
    true
}

/// Skip past a type annotation in source text, stopping at `{`, `=>`, `,`,
/// `)`, or end of meaningful content. Handles nested `<>` for generics.
fn skip_type_annotation(bytes: &[u8], mut i: usize) -> usize {
    let len = bytes.len();
    let mut angle_depth = 0u32;
    let mut paren_depth = 0u32;
    while i < len {
        match bytes[i] {
            b'=' if i + 1 < len && bytes[i + 1] == b'>' && angle_depth == 0 && paren_depth == 0 => {
                return i;
            }
            b'{' | b',' | b')' | b';' if angle_depth == 0 && paren_depth == 0 => {
                return i;
            }
            b'<' => angle_depth += 1,
            b'>' if angle_depth > 0 => angle_depth -= 1,
            b'(' => paren_depth += 1,
            b')' if paren_depth > 0 => paren_depth -= 1,
            _ => {}
        }
        i += 1;
    }
    i
}
