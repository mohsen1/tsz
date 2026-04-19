use wasm_bindgen::prelude::wasm_bindgen;

use crate::char_codes::CharacterCodes;

// =============================================================================
// Comparison enum - matches TypeScript's Comparison const enum
// =============================================================================

/// Comparison result for ordering operations.
/// Matches TypeScript's `Comparison` const enum in src/compiler/corePublic.ts
#[wasm_bindgen]
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    LessThan = -1,
    EqualTo = 0,
    GreaterThan = 1,
}

// =============================================================================
// String Comparison Utilities (Phase 1.1)
// =============================================================================

/// Compare two strings using a case-sensitive ordinal comparison.
///
/// Ordinal comparisons are based on the difference between the unicode code points
/// of both strings. Characters with multiple unicode representations are considered
/// unequal. Ordinal comparisons provide predictable ordering, but place "a" after "B".
#[wasm_bindgen(js_name = compareStringsCaseSensitive)]
pub fn compare_strings_case_sensitive(a: Option<String>, b: Option<String>) -> Comparison {
    match (a, b) {
        (None, None) => Comparison::EqualTo,
        (None, Some(_)) => Comparison::LessThan,
        (Some(_), None) => Comparison::GreaterThan,
        (Some(a), Some(b)) => match a.cmp(&b) {
            std::cmp::Ordering::Equal => Comparison::EqualTo,
            std::cmp::Ordering::Less => Comparison::LessThan,
            std::cmp::Ordering::Greater => Comparison::GreaterThan,
        },
    }
}

/// Compare two strings using a case-insensitive ordinal comparison.
///
/// Case-insensitive comparisons compare both strings one code-point at a time using
/// the integer value of each code-point after applying `to_uppercase` to each string.
/// We always map both strings to their upper-case form as some unicode characters do
/// not properly round-trip to lowercase (such as `ẞ` German sharp capital s).
#[wasm_bindgen(js_name = compareStringsCaseInsensitive)]
pub fn compare_strings_case_insensitive(a: Option<String>, b: Option<String>) -> Comparison {
    match (a, b) {
        (None, None) => Comparison::EqualTo,
        (None, Some(_)) => Comparison::LessThan,
        (Some(_), None) => Comparison::GreaterThan,
        (Some(a), Some(b)) => {
            if a == b {
                return Comparison::EqualTo;
            }
            // Use iterator-based comparison to avoid allocating new strings
            compare_strings_case_insensitive_iter(&a, &b)
        }
    }
}

/// Iterator-based case-insensitive comparison (no allocation).
/// Maps characters to uppercase on-the-fly without creating new strings.
#[inline]
fn compare_strings_case_insensitive_iter(a: &str, b: &str) -> Comparison {
    use std::cmp::Ordering;

    let mut a_chars = a.chars().flat_map(char::to_uppercase);
    let mut b_chars = b.chars().flat_map(char::to_uppercase);

    loop {
        match (a_chars.next(), b_chars.next()) {
            (None, None) => return Comparison::EqualTo,
            (None, Some(_)) => return Comparison::LessThan,
            (Some(_), None) => return Comparison::GreaterThan,
            (Some(a_char), Some(b_char)) => match a_char.cmp(&b_char) {
                Ordering::Less => return Comparison::LessThan,
                Ordering::Greater => return Comparison::GreaterThan,
                Ordering::Equal => continue,
            },
        }
    }
}

/// Compare two strings using a case-insensitive ordinal comparison (eslint-compatible).
///
/// This uses `to_lowercase` instead of `to_uppercase` to match eslint's `sort-imports`
/// rule behavior. The difference affects the relative order of letters and ASCII
/// characters 91-96, of which `_` is a valid identifier character.
#[wasm_bindgen(js_name = compareStringsCaseInsensitiveEslintCompatible)]
pub fn compare_strings_case_insensitive_eslint_compatible(
    a: Option<String>,
    b: Option<String>,
) -> Comparison {
    match (a, b) {
        (None, None) => Comparison::EqualTo,
        (None, Some(_)) => Comparison::LessThan,
        (Some(_), None) => Comparison::GreaterThan,
        (Some(a), Some(b)) => {
            if a == b {
                return Comparison::EqualTo;
            }
            // Use iterator-based comparison to avoid allocating new strings
            compare_strings_case_insensitive_lower_iter(&a, &b)
        }
    }
}

/// Iterator-based case-insensitive comparison using lowercase (no allocation).
/// Used for eslint compatibility.
#[inline]
fn compare_strings_case_insensitive_lower_iter(a: &str, b: &str) -> Comparison {
    use std::cmp::Ordering;

    let mut a_chars = a.chars().flat_map(char::to_lowercase);
    let mut b_chars = b.chars().flat_map(char::to_lowercase);

    loop {
        match (a_chars.next(), b_chars.next()) {
            (None, None) => return Comparison::EqualTo,
            (None, Some(_)) => return Comparison::LessThan,
            (Some(_), None) => return Comparison::GreaterThan,
            (Some(a_char), Some(b_char)) => match a_char.cmp(&b_char) {
                Ordering::Less => return Comparison::LessThan,
                Ordering::Greater => return Comparison::GreaterThan,
                Ordering::Equal => continue,
            },
        }
    }
}

/// Check if two strings are equal (case-sensitive).
#[wasm_bindgen(js_name = equateStringsCaseSensitive)]
pub fn equate_strings_case_sensitive(a: &str, b: &str) -> bool {
    a == b
}

/// Check if two strings are equal (case-insensitive).
/// Uses iterator-based comparison to avoid allocating new strings.
#[wasm_bindgen(js_name = equateStringsCaseInsensitive)]
pub fn equate_strings_case_insensitive(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        // Quick length check - but note that uppercase/lowercase might change length
        // for some unicode characters, so we need the full comparison
    }
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

// =============================================================================
// Path Utilities (Phase 1.2)
// =============================================================================

/// Directory separator used internally (forward slash).
pub const DIRECTORY_SEPARATOR: char = '/';

/// Alternative directory separator (backslash, used on Windows).
pub const ALT_DIRECTORY_SEPARATOR: char = '\\';

/// Determines whether a charCode corresponds to `/` or `\`.
#[allow(clippy::missing_const_for_fn)] // wasm_bindgen does not support const fn
#[wasm_bindgen(js_name = isAnyDirectorySeparator)]
pub fn is_any_directory_separator(char_code: u32) -> bool {
    char_code == DIRECTORY_SEPARATOR as u32 || char_code == ALT_DIRECTORY_SEPARATOR as u32
}

/// Normalize path separators, converting `\` into `/`.
#[wasm_bindgen(js_name = normalizeSlashes)]
pub fn normalize_slashes(path: &str) -> String {
    if path.contains('\\') {
        path.replace('\\', "/")
    } else {
        path.to_string()
    }
}

/// Determines whether a path has a trailing separator (`/` or `\\`).
#[wasm_bindgen(js_name = hasTrailingDirectorySeparator)]
pub fn has_trailing_directory_separator(path: &str) -> bool {
    let last_char = match path.chars().last() {
        Some(c) => c,
        None => return false,
    };
    last_char == DIRECTORY_SEPARATOR || last_char == ALT_DIRECTORY_SEPARATOR
}

/// Determines whether a path starts with a relative path component (i.e. `.` or `..`).
#[wasm_bindgen(js_name = pathIsRelative)]
pub fn path_is_relative(path: &str) -> bool {
    // Matches /^\.\.?($|[\\/])/
    if path.starts_with("./") || path.starts_with(".\\") || path == "." {
        return true;
    }
    if path.starts_with("../") || path.starts_with("..\\") || path == ".." {
        return true;
    }
    false
}

/// Removes a trailing directory separator from a path, if it has one.
/// Uses char-based operations for UTF-8 safety.
#[wasm_bindgen(js_name = removeTrailingDirectorySeparator)]
pub fn remove_trailing_directory_separator(path: &str) -> String {
    if !has_trailing_directory_separator(path) || path.len() <= 1 {
        return path.to_string();
    }
    // Use strip_suffix for UTF-8 safe character removal
    path.strip_suffix(DIRECTORY_SEPARATOR)
        .or_else(|| path.strip_suffix(ALT_DIRECTORY_SEPARATOR))
        .unwrap_or(path)
        .to_string()
}

/// Ensures a path has a trailing directory separator.
#[wasm_bindgen(js_name = ensureTrailingDirectorySeparator)]
pub fn ensure_trailing_directory_separator(path: &str) -> String {
    if has_trailing_directory_separator(path) {
        path.to_string()
    } else {
        format!("{path}/")
    }
}

/// Determines whether a path has an extension.
#[wasm_bindgen(js_name = hasExtension)]
pub fn has_extension(file_name: &str) -> bool {
    get_base_file_name(file_name).contains('.')
}

/// Returns the path except for its containing directory name (basename).
/// Uses char-based operations for UTF-8 safety.
#[wasm_bindgen(js_name = getBaseFileName)]
pub fn get_base_file_name(path: &str) -> String {
    let path = normalize_slashes(path);
    // Remove trailing separator using UTF-8 safe operations
    let path = if has_trailing_directory_separator(&path) && path.len() > 1 {
        path.strip_suffix(DIRECTORY_SEPARATOR)
            .or_else(|| path.strip_suffix(ALT_DIRECTORY_SEPARATOR))
            .unwrap_or(&path)
    } else {
        &path
    };
    // Find last separator - safe because '/' is ASCII and rfind returns valid char boundary
    match path.rfind('/') {
        Some(idx) => path[idx + 1..].to_string(),
        None => path.to_string(),
    }
}

/// Check if path ends with a specific extension.
#[wasm_bindgen(js_name = fileExtensionIs)]
pub fn file_extension_is(path: &str, extension: &str) -> bool {
    path.len() > extension.len() && path.ends_with(extension)
}

/// Convert file name to lowercase for case-insensitive file systems.
///
/// This function handles special Unicode characters that need to remain
/// case-sensitive for proper cross-platform file name handling:
/// - \u{0130} (İ - Latin capital I with dot above)
/// - \u{0131} (ı - Latin small letter dotless i)
/// - \u{00DF} (ß - Latin small letter sharp s)
///
/// These characters are excluded from lowercase conversion to maintain
/// compatibility with case-insensitive file systems that have special
/// handling for these characters (notably Turkish locale on Windows).
///
/// Matches TypeScript's `toFileNameLowerCase` in src/compiler/core.ts
#[wasm_bindgen(js_name = toFileNameLowerCase)]
pub fn to_file_name_lower_case(x: &str) -> String {
    // First, check if we need to do any work (optimization - avoid allocation)
    // The "safe" set of characters that don't need lowercasing:
    // - \u{0130} (İ), \u{0131} (ı), \u{00DF} (ß) - special Turkish chars
    // - a-z (lowercase ASCII letters)
    // - 0-9 (digits)
    // - \ / : - _ . (path separators and common filename chars)
    // - space

    let needs_conversion = x.chars().any(|c| {
        !matches!(c,
            '\u{0130}' | '\u{0131}' | '\u{00DF}' |  // Special Unicode chars
            'a'..='z' | '0'..='9' |  // ASCII lowercase and digits
            '\\' | '/' | ':' | '-' | '_' | '.' | ' '  // Path chars and space
        )
    });

    if !needs_conversion {
        return x.to_string();
    }

    // Convert to lowercase, preserving the special characters
    x.to_lowercase()
}

// =============================================================================
// Character Classification (Phase 1.3 - Scanner Prep)
// =============================================================================

/// Check if character is a line break (LF, CR, LS, PS).
#[allow(clippy::missing_const_for_fn)] // wasm_bindgen does not support const fn
#[wasm_bindgen(js_name = isLineBreak)]
pub fn is_line_break(ch: u32) -> bool {
    ch == CharacterCodes::LINE_FEED
        || ch == CharacterCodes::CARRIAGE_RETURN
        || ch == CharacterCodes::LINE_SEPARATOR
        || ch == CharacterCodes::PARAGRAPH_SEPARATOR
}

/// Check if character is a single-line whitespace (not including line breaks).
#[wasm_bindgen(js_name = isWhiteSpaceSingleLine)]
pub fn is_white_space_single_line(ch: u32) -> bool {
    ch == CharacterCodes::SPACE
        || ch == CharacterCodes::TAB
        || ch == CharacterCodes::VERTICAL_TAB
        || ch == CharacterCodes::FORM_FEED
        || ch == CharacterCodes::NON_BREAKING_SPACE
        || ch == CharacterCodes::NEXT_LINE
        || ch == CharacterCodes::OGHAM
        || (CharacterCodes::EN_QUAD..=CharacterCodes::ZERO_WIDTH_SPACE).contains(&ch)
        || ch == CharacterCodes::NARROW_NO_BREAK_SPACE
        || ch == CharacterCodes::MATHEMATICAL_SPACE
        || ch == CharacterCodes::IDEOGRAPHIC_SPACE
        || ch == CharacterCodes::BYTE_ORDER_MARK
}

/// Check if character is any whitespace (including line breaks).
#[wasm_bindgen(js_name = isWhiteSpaceLike)]
pub fn is_white_space_like(ch: u32) -> bool {
    is_white_space_single_line(ch) || is_line_break(ch)
}

/// Check if character is a decimal digit (0-9).
#[wasm_bindgen(js_name = isDigit)]
pub fn is_digit(ch: u32) -> bool {
    (CharacterCodes::_0..=CharacterCodes::_9).contains(&ch)
}

/// Check if character is an octal digit (0-7).
#[wasm_bindgen(js_name = isOctalDigit)]
pub fn is_octal_digit(ch: u32) -> bool {
    (CharacterCodes::_0..=CharacterCodes::_7).contains(&ch)
}

/// Check if character is a hexadecimal digit (0-9, A-F, a-f).
#[wasm_bindgen(js_name = isHexDigit)]
pub fn is_hex_digit(ch: u32) -> bool {
    is_digit(ch)
        || (CharacterCodes::UPPER_A..=CharacterCodes::UPPER_F).contains(&ch)
        || (CharacterCodes::LOWER_A..=CharacterCodes::LOWER_F).contains(&ch)
}

/// Check if character is an ASCII letter (A-Z, a-z).
#[wasm_bindgen(js_name = isASCIILetter)]
pub fn is_ascii_letter(ch: u32) -> bool {
    (CharacterCodes::UPPER_A..=CharacterCodes::UPPER_Z).contains(&ch)
        || (CharacterCodes::LOWER_A..=CharacterCodes::LOWER_Z).contains(&ch)
}

/// Check if character is a word character (A-Z, a-z, 0-9, _).
#[wasm_bindgen(js_name = isWordCharacter)]
pub fn is_word_character(ch: u32) -> bool {
    is_ascii_letter(ch) || is_digit(ch) || ch == CharacterCodes::UNDERSCORE
}
