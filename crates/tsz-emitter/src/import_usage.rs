//! Shared text-based heuristics for detecting value vs type-only usage of imports.
//!
//! Both the lowering pass and the emitter need to determine whether an import
//! binding is used in value positions (requiring helper emission / runtime code)
//! or only in type positions (safe to erase).  These functions provide text-based
//! analysis that works without full type information.

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
                .is_none_or(|ch| !is_ident_or_member_access_char(ch))
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

/// Returns true if `ch` preceding an identifier means it is NOT a standalone
/// variable reference. Includes identifier chars and `.` (property access).
const fn is_ident_or_member_access_char(ch: char) -> bool {
    ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric()
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
    // Track brace depth to skip multi-line type declaration bodies
    // (interface, type alias, declare blocks)
    let mut type_brace_depth: u32 = 0;
    for line in source.lines() {
        let trimmed = line.trim();

        // If we're inside a type declaration body, count braces to find the end
        if type_brace_depth > 0 {
            for ch in trimmed.chars() {
                match ch {
                    '{' => type_brace_depth += 1,
                    '}' => type_brace_depth -= 1,
                    _ => {}
                }
                if type_brace_depth == 0 {
                    break;
                }
            }
            result.push('\n');
            continue;
        }

        // Check if this line starts a type-only declaration (possibly multi-line)
        let is_type_only_start = trimmed.starts_with("declare ")
            || trimmed.starts_with("import type ")
            || trimmed.starts_with("import type{")
            || trimmed.starts_with("export type ")
            || trimmed.starts_with("export type{")
            || trimmed.starts_with("export interface ")
            || trimmed.starts_with("export declare ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("interface ")
            // Other import statements - identifiers from other
            // imports should not count as value usages of *this* import.
            // But `import X = Y` (namespace aliases) reference identifiers as
            // values and must be kept — only strip module-style imports.
            || (trimmed.starts_with("import ") && !is_namespace_alias_import(trimmed))
            || trimmed.starts_with("import{")
            // Direct re-exports from other modules (`export { x } from "mod"`,
            // `export * from "mod"`) don't reference local bindings, so strip them.
            // But `export { a }` (without `from`) re-exports a local binding —
            // it IS a value usage and must be kept in the haystack.
            || trimmed.starts_with("export *")
            || (trimmed.starts_with("export{") && is_reexport_from(trimmed))
            || (trimmed.starts_with("export {") && is_reexport_from(trimmed))
            || (trimmed.starts_with("export import ") && !is_namespace_alias_import(&trimmed["export ".len()..]));

        if is_type_only_start {
            // Check if this line opens a brace block (multi-line declaration)
            for ch in trimmed.chars() {
                match ch {
                    '{' => type_brace_depth += 1,
                    '}' => {
                        type_brace_depth = type_brace_depth.saturating_sub(1);
                    }
                    _ => {}
                }
            }
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

/// Strip only purely type-level declaration lines from source text.
///
/// Unlike [`strip_type_only_content`], this does NOT strip inline type
/// annotations (`: Type`, generics, `as`, `satisfies`, etc.), and it keeps
/// `import X = Y;` / `export import X = Y;` lines because those reference
/// identifiers that may be namespace aliases.  Use this to detect whether
/// an identifier is *referenced at all* (in any position), rather than
/// whether it is used specifically as a value.
///
/// Strips:
/// - `declare` (ambient declarations)
/// - `import type` / `export type` (type-only imports/exports)
/// - `interface` / `type` (type declarations)
/// - `export declare` / `export interface`
pub fn strip_type_declaration_lines(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut type_brace_depth: u32 = 0;
    for line in source.lines() {
        let trimmed = line.trim();

        if type_brace_depth > 0 {
            for ch in trimmed.chars() {
                match ch {
                    '{' => type_brace_depth += 1,
                    '}' => type_brace_depth -= 1,
                    _ => {}
                }
                if type_brace_depth == 0 {
                    break;
                }
            }
            result.push('\n');
            continue;
        }

        let is_type_only_start = trimmed.starts_with("declare ")
            || trimmed.starts_with("import type ")
            || trimmed.starts_with("import type{")
            || trimmed.starts_with("export type ")
            || trimmed.starts_with("export type{")
            || trimmed.starts_with("export interface ")
            || trimmed.starts_with("export declare ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("interface ");

        if is_type_only_start {
            for ch in trimmed.chars() {
                match ch {
                    '{' => type_brace_depth += 1,
                    '}' => {
                        type_brace_depth = type_brace_depth.saturating_sub(1);
                    }
                    _ => {}
                }
            }
            result.push('\n');
            continue;
        }
        // Keep the line as-is (no annotation stripping)
        result.push_str(line);
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
/// - Parameter type annotations inside `()` (e.g., `(param: Type)`)
/// - Generic type arguments in call expressions (e.g., `func<Type>(...)`)
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
    let is_var_line = trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("export const ")
        || trimmed.starts_with("export let ")
        || trimmed.starts_with("export var ");

    // Track nesting depth for parameter type annotation detection
    let mut paren_depth = 0u32;
    let mut brace_depth = 0u32;

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
            // Track brace depth (to distinguish object literals from parameter lists)
            b'{' => {
                brace_depth += 1;
                result.push('{');
                i += 1;
            }
            b'}' if brace_depth > 0 => {
                brace_depth -= 1;
                result.push('}');
                i += 1;
            }
            // Track paren depth for parameter type annotation detection
            b'(' => {
                paren_depth += 1;
                result.push('(');
                i += 1;
            }
            b')' => {
                paren_depth = paren_depth.saturating_sub(1);
                result.push(')');
                i += 1;
            }
            // Generic type arguments: `ident<Type>(` — strip `<Type>`
            b'<' if i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'$')
                && is_generic_type_args(bytes, i) =>
            {
                // Skip the entire <...> block
                i = skip_angle_bracket_block(bytes, i);
            }
            // After `):`, `?:`, or `]:` — this is a type annotation
            b':' if i > 0
                && (bytes[i - 1] == b')' || bytes[i - 1] == b'?' || bytes[i - 1] == b']') =>
            {
                // Skip until `{`, `=>`, `,`, `)`, or end of line
                i += 1;
                i = skip_type_annotation(bytes, i);
            }
            // Inside parentheses (but not braces): `:` after an identifier is a parameter type annotation
            b':' if paren_depth > 0
                && brace_depth == 0
                && i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'$') =>
            {
                i += 1;
                i = skip_type_annotation(bytes, i);
            }
            // On const/let/var lines, `:` before the first `=` is a type annotation.
            // Use `skip_var_type_annotation` which stops at bare `=` so the value
            // initializer is preserved (e.g. `const x: T = val;` → `const x = val;`).
            b':' if is_var_line && is_var_type_annotation_colon(line, i) => {
                i += 1;
                i = skip_var_type_annotation(bytes, i);
            }
            _ => {
                result.push(bytes[i] as char);
                i += 1;
            }
        }
    }

    // Strip `as Type` type assertions (value as SomeType → value)
    // Also strip `satisfies Type` (value satisfies SomeType → value)
    // These are type-only positions that should not count as value usages.
    for keyword in [" as ", " satisfies "] {
        while let Some(kw_pos) = result.find(keyword) {
            // Check it's not inside a string by verifying balanced quotes before it
            let before = &result[..kw_pos];
            let single_quotes = before.chars().filter(|&c| c == '\'').count();
            let double_quotes = before.chars().filter(|&c| c == '"').count();
            if single_quotes % 2 != 0 || double_quotes % 2 != 0 {
                break; // inside a string, stop
            }
            // Find end of type expression after the keyword
            let type_start = kw_pos + keyword.len();
            let type_end = skip_type_annotation(result.as_bytes(), type_start);
            // Replace `value as Type` with `value`
            result = format!("{}{}", &result[..kw_pos], &result[type_end..]);
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

/// Check if an `export { ... }` line is a re-export from another module
/// (contains `from "..."` or `from '...'`).
/// Check if a line starting with `import ` is a namespace alias import (`import X = Y;`).
/// These create value references and must not be stripped.
fn is_namespace_alias_import(trimmed: &str) -> bool {
    // `import X = Y;` — look for `=` after the identifier (and no `from` or `{`)
    // Skip "import " prefix
    let Some(after_import) = trimmed.strip_prefix("import ") else {
        return false;
    };
    // Namespace alias imports have the form: `import <identifier> = <entity>;`
    // Module imports have: `import { ... } from "..."` or `import X from "..."`
    // The key distinguisher is the `=` sign after the first identifier.
    !after_import.starts_with('{')
        && !after_import.starts_with('*')
        && after_import.contains('=')
        && !after_import.contains("from ")
}

fn is_reexport_from(trimmed: &str) -> bool {
    // Look for `from` keyword followed by a string literal after the closing `}`
    if let Some(brace_end) = trimmed.find('}') {
        let after_brace = trimmed[brace_end + 1..].trim();
        after_brace.starts_with("from ")
            || after_brace.starts_with("from\"")
            || after_brace.starts_with("from'")
    } else {
        false
    }
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

/// Check if `<` at position `i` starts a generic type argument list.
/// Heuristic: try to find matching `>` with balanced nesting, followed by `(`.
/// This distinguishes `f<T>()` (generic call) from `a < b` (comparison).
fn is_generic_type_args(bytes: &[u8], start: usize) -> bool {
    let len = bytes.len();
    let mut depth = 0u32;
    let mut j = start;
    while j < len {
        match bytes[j] {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    // Check what follows the closing `>`
                    let mut k = j + 1;
                    while k < len && bytes[k] == b' ' {
                        k += 1;
                    }
                    // `>` followed by `(` → generic call, or `)` → end of type arg in param
                    // `>` followed by `,` → type arg in list, or `>` → nested generic
                    return k < len && matches!(bytes[k], b'(' | b')' | b',' | b'>' | b';');
                }
            }
            // If we hit something that can't be in a type argument, it's a comparison
            b'{' | b'}' | b';' | b'=' => return false,
            _ => {}
        }
        j += 1;
    }
    false
}

/// Skip past a balanced `<...>` block, returning the position after `>`.
fn skip_angle_bracket_block(bytes: &[u8], start: usize) -> usize {
    let len = bytes.len();
    let mut depth = 0u32;
    let mut j = start;
    while j < len {
        match bytes[j] {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            b'{' | b'}' | b';' => return start + 1, // bail
            _ => {}
        }
        j += 1;
    }
    start + 1 // bail
}

/// Skip past a type annotation in source text, stopping at `{`, `=>`, `,`,
/// `)`, or end of meaningful content. Handles nested `<>` for generics.
/// Like [`skip_type_annotation`] but also stops at a bare `=` (assignment).
/// Used for variable declaration type annotations where `const x: T = val;`
/// should strip only `: T`, preserving `= val`.
fn skip_var_type_annotation(bytes: &[u8], mut i: usize) -> usize {
    let len = bytes.len();
    let mut angle_depth = 0u32;
    let mut paren_depth = 0u32;
    while i < len {
        match bytes[i] {
            // `=` that is not `==`, `===`, or `=>` → assignment, stop here
            b'=' if angle_depth == 0 && paren_depth == 0 => {
                if i + 1 < len && (bytes[i + 1] == b'=' || bytes[i + 1] == b'>') {
                    // `==`, `===`, `=>` — skip (part of type expression)
                    i += 2;
                    if i < len && bytes[i] == b'=' {
                        i += 1; // ===
                    }
                    continue;
                }
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
