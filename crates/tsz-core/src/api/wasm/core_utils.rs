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

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- compare_strings_case_sensitive ------------------------------

    #[test]
    fn compare_strings_case_sensitive_none_handling() {
        assert_eq!(
            compare_strings_case_sensitive(None, None),
            Comparison::EqualTo
        );
        assert_eq!(
            compare_strings_case_sensitive(None, Some(String::from("a"))),
            Comparison::LessThan
        );
        assert_eq!(
            compare_strings_case_sensitive(Some(String::from("a")), None),
            Comparison::GreaterThan
        );
    }

    #[test]
    fn compare_strings_case_sensitive_orders_by_unicode_code_point() {
        // 'B' (66) is less than 'a' (97) under ordinal comparison.
        assert_eq!(
            compare_strings_case_sensitive(Some("B".into()), Some("a".into())),
            Comparison::LessThan,
        );
        assert_eq!(
            compare_strings_case_sensitive(Some("abc".into()), Some("abc".into())),
            Comparison::EqualTo,
        );
    }

    // ---------- compare_strings_case_insensitive ----------------------------

    #[test]
    fn compare_strings_case_insensitive_treats_case_alike() {
        assert_eq!(
            compare_strings_case_insensitive(Some("ABC".into()), Some("abc".into())),
            Comparison::EqualTo,
        );
    }

    #[test]
    fn compare_strings_case_insensitive_none_handling() {
        assert_eq!(
            compare_strings_case_insensitive(None, None),
            Comparison::EqualTo
        );
        assert_eq!(
            compare_strings_case_insensitive(None, Some(String::from("a"))),
            Comparison::LessThan
        );
        assert_eq!(
            compare_strings_case_insensitive(Some(String::from("a")), None),
            Comparison::GreaterThan
        );
    }

    // ---------- equate_strings_*--------------------------------------------

    #[test]
    fn equate_strings_case_sensitive_distinguishes_case() {
        assert!(equate_strings_case_sensitive("abc", "abc"));
        assert!(!equate_strings_case_sensitive("abc", "ABC"));
    }

    #[test]
    fn equate_strings_case_insensitive_collapses_case() {
        assert!(equate_strings_case_insensitive("abc", "ABC"));
        assert!(equate_strings_case_insensitive("Hello", "hELLO"));
        assert!(!equate_strings_case_insensitive("abc", "abd"));
    }

    // ---------- is_any_directory_separator ----------------------------------

    #[test]
    fn is_any_directory_separator_accepts_both_slashes() {
        assert!(is_any_directory_separator(b'/' as u32));
        assert!(is_any_directory_separator(b'\\' as u32));
        assert!(!is_any_directory_separator(b'a' as u32));
        assert!(!is_any_directory_separator(b'.' as u32));
    }

    // ---------- normalize_slashes -------------------------------------------

    #[test]
    fn normalize_slashes_replaces_backslashes_with_forward() {
        assert_eq!(normalize_slashes("a\\b\\c"), "a/b/c");
    }

    #[test]
    fn normalize_slashes_returns_input_when_no_backslashes() {
        // Optimization branch: returns owned copy of input.
        assert_eq!(normalize_slashes("a/b/c"), "a/b/c");
        assert_eq!(normalize_slashes(""), "");
    }

    // ---------- has_trailing_directory_separator ----------------------------

    #[test]
    fn has_trailing_directory_separator_branches() {
        assert!(has_trailing_directory_separator("a/"));
        assert!(has_trailing_directory_separator("a\\"));
        assert!(!has_trailing_directory_separator("a"));
        assert!(!has_trailing_directory_separator(""));
    }

    // ---------- path_is_relative -------------------------------------------

    #[test]
    fn path_is_relative_recognizes_dot_and_dot_dot_prefixes() {
        for p in &["./", ".\\", "../", "..\\", ".", ".."] {
            assert!(path_is_relative(p), "expected relative: {p:?}");
        }
    }

    #[test]
    fn path_is_relative_rejects_absolute_and_bare_names() {
        for p in &["/a", "C:\\a", "a/b", "..a", ".a"] {
            assert!(!path_is_relative(p), "expected NOT relative: {p:?}");
        }
    }

    // ---------- remove / ensure trailing directory separator ----------------

    #[test]
    fn remove_trailing_directory_separator_strips_one_separator() {
        assert_eq!(remove_trailing_directory_separator("a/"), "a");
        assert_eq!(remove_trailing_directory_separator("a\\"), "a");
        assert_eq!(remove_trailing_directory_separator("a"), "a");
        // length <= 1 is a guard branch — single-char paths are returned as-is.
        assert_eq!(remove_trailing_directory_separator("/"), "/");
    }

    #[test]
    fn ensure_trailing_directory_separator_adds_one_when_missing() {
        assert_eq!(ensure_trailing_directory_separator("a"), "a/");
        // Already has one — return unchanged.
        assert_eq!(ensure_trailing_directory_separator("a/"), "a/");
        assert_eq!(ensure_trailing_directory_separator("a\\"), "a\\");
    }

    // ---------- has_extension / file_extension_is / get_base_file_name ------

    #[test]
    fn has_extension_via_basename() {
        assert!(has_extension("a.ts"));
        assert!(!has_extension("a"));
        // A dot in the directory should NOT count — only the basename matters.
        assert!(!has_extension("a.dir/file"));
    }

    #[test]
    fn get_base_file_name_extracts_last_segment() {
        assert_eq!(get_base_file_name("a/b/c.ts"), "c.ts");
        assert_eq!(get_base_file_name("c.ts"), "c.ts");
        // Trailing-separator handling.
        assert_eq!(get_base_file_name("a/b/"), "b");
        // Backslash is normalized.
        assert_eq!(get_base_file_name("a\\b\\c"), "c");
    }

    #[test]
    fn file_extension_is_strict_about_path_length() {
        // Path strictly LONGER than extension required.
        assert!(file_extension_is("a.ts", ".ts"));
        // Equal length → false (means the path IS the extension).
        assert!(!file_extension_is(".ts", ".ts"));
        // Mismatch → false.
        assert!(!file_extension_is("a.tsx", ".ts"));
    }

    // ---------- to_file_name_lower_case -------------------------------------

    #[test]
    fn to_file_name_lower_case_basic_ascii() {
        assert_eq!(to_file_name_lower_case("ABC.TS"), "abc.ts");
        // Already-lowercase + safe chars short-circuits to clone.
        assert_eq!(to_file_name_lower_case("abc.ts"), "abc.ts");
    }

    #[test]
    fn to_file_name_lower_case_preserves_special_unicode() {
        // \u{0130} (İ), \u{0131} (ı), \u{00DF} (ß) intentionally NOT lowercased.
        assert_eq!(to_file_name_lower_case("\u{0130}"), "\u{0130}");
        assert_eq!(to_file_name_lower_case("\u{0131}"), "\u{0131}");
        assert_eq!(to_file_name_lower_case("\u{00DF}"), "\u{00DF}");
    }

    // ---------- is_line_break / whitespace ----------------------------------

    #[test]
    fn is_line_break_recognizes_lf_cr_ls_ps() {
        assert!(is_line_break(0x0A)); // \n
        assert!(is_line_break(0x0D)); // \r
        assert!(is_line_break(0x2028)); // line separator
        assert!(is_line_break(0x2029)); // paragraph separator
        assert!(!is_line_break(b' ' as u32));
    }

    #[test]
    fn is_white_space_single_line_includes_horizontal_only() {
        // Spaces and tabs — yes.
        assert!(is_white_space_single_line(b' ' as u32));
        assert!(is_white_space_single_line(b'\t' as u32));
        // Newlines — NO (those are line breaks, not single-line whitespace).
        assert!(!is_white_space_single_line(0x0A));
    }

    #[test]
    fn is_white_space_like_includes_both_newlines_and_horizontal() {
        // Horizontal whitespace.
        assert!(is_white_space_like(b' ' as u32));
        // Newlines also count.
        assert!(is_white_space_like(0x0A));
        // Letters do not.
        assert!(!is_white_space_like(b'a' as u32));
    }

    // ---------- digit / hex / octal / letter / word ------------------------

    #[test]
    fn is_digit_octal_hex_letter_word_classifications() {
        assert!(is_digit(b'0' as u32));
        assert!(is_digit(b'9' as u32));
        assert!(!is_digit(b'a' as u32));

        assert!(is_octal_digit(b'0' as u32));
        assert!(is_octal_digit(b'7' as u32));
        assert!(!is_octal_digit(b'8' as u32));
        assert!(!is_octal_digit(b'9' as u32));

        assert!(is_hex_digit(b'0' as u32));
        assert!(is_hex_digit(b'9' as u32));
        assert!(is_hex_digit(b'a' as u32));
        assert!(is_hex_digit(b'f' as u32));
        assert!(is_hex_digit(b'F' as u32));
        assert!(!is_hex_digit(b'g' as u32));

        assert!(is_ascii_letter(b'a' as u32));
        assert!(is_ascii_letter(b'Z' as u32));
        assert!(!is_ascii_letter(b'0' as u32));
        assert!(!is_ascii_letter(b'_' as u32));

        // word = letter | digit | underscore.
        assert!(is_word_character(b'a' as u32));
        assert!(is_word_character(b'0' as u32));
        assert!(is_word_character(b'_' as u32));
        assert!(!is_word_character(b'-' as u32));
        assert!(!is_word_character(b' ' as u32));
    }
}
