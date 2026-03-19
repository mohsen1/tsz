use crate::char_codes::CharacterCodes;
use crate::{
    Comparison, compare_strings_case_insensitive,
    compare_strings_case_insensitive_eslint_compatible, compare_strings_case_sensitive,
    ensure_trailing_directory_separator, equate_strings_case_insensitive, file_extension_is,
    get_base_file_name, has_extension, has_trailing_directory_separator,
    is_any_directory_separator, is_ascii_letter, is_digit, is_hex_digit, is_line_break,
    is_octal_digit, is_white_space_like, is_white_space_single_line, is_word_character,
    normalize_slashes, path_is_relative, remove_trailing_directory_separator,
    to_file_name_lower_case,
};

#[test]
fn test_compare_strings_case_sensitive_handles_none_and_ordinal_ordering() {
    assert_eq!(
        compare_strings_case_sensitive(None, None),
        Comparison::EqualTo
    );
    assert_eq!(
        compare_strings_case_sensitive(None, Some("a".to_string())),
        Comparison::LessThan
    );
    assert_eq!(
        compare_strings_case_sensitive(Some("b".to_string()), None),
        Comparison::GreaterThan
    );
    assert_eq!(
        compare_strings_case_sensitive(Some("B".to_string()), Some("a".to_string())),
        Comparison::LessThan
    );
}

#[test]
fn test_compare_strings_case_insensitive_handles_unicode_expansion() {
    assert_eq!(
        compare_strings_case_insensitive(Some("straße".to_string()), Some("STRASSE".to_string())),
        Comparison::EqualTo
    );
    assert!(equate_strings_case_insensitive("straße", "STRASSE"));
    assert!(!equate_strings_case_insensitive("straße", "STRASZE"));
}

#[test]
fn test_eslint_compatible_case_insensitive_comparison_uses_lowercase_ordering() {
    assert_eq!(
        compare_strings_case_insensitive(Some("_a".to_string()), Some("Za".to_string())),
        Comparison::GreaterThan
    );
    assert_eq!(
        compare_strings_case_insensitive_eslint_compatible(
            Some("_a".to_string()),
            Some("Za".to_string())
        ),
        Comparison::LessThan
    );
}

#[test]
fn test_path_helpers_normalize_relative_and_trailing_separator_behavior() {
    assert_eq!(normalize_slashes(r"foo\bar\baz.ts"), "foo/bar/baz.ts");
    assert!(is_any_directory_separator('/' as u32));
    assert!(is_any_directory_separator('\\' as u32));
    assert!(!is_any_directory_separator('x' as u32));

    assert!(path_is_relative("."));
    assert!(path_is_relative("./src"));
    assert!(path_is_relative(".."));
    assert!(path_is_relative(r"..\src"));
    assert!(!path_is_relative(".config"));
    assert!(!path_is_relative("/abs/path"));

    assert!(has_trailing_directory_separator("foo/"));
    assert!(has_trailing_directory_separator(r"foo\"));
    assert!(!has_trailing_directory_separator("foo"));

    assert_eq!(remove_trailing_directory_separator("foo/"), "foo");
    assert_eq!(remove_trailing_directory_separator(r"foo\"), "foo");
    assert_eq!(remove_trailing_directory_separator("/"), "/");
    assert_eq!(ensure_trailing_directory_separator("foo"), "foo/");
    assert_eq!(ensure_trailing_directory_separator("foo/"), "foo/");
}

#[test]
fn test_file_name_helpers_cover_windows_and_trailing_separator_cases() {
    assert_eq!(get_base_file_name("/usr/lib/file.ts"), "file.ts");
    assert_eq!(get_base_file_name(r"C:\work\src\index.ts"), "index.ts");
    assert_eq!(get_base_file_name("/usr/lib/"), "lib");

    assert!(has_extension("file.ts"));
    assert!(has_extension(".gitignore"));
    assert!(!has_extension("Makefile"));

    assert!(file_extension_is("file.d.ts", ".ts"));
    assert!(file_extension_is("file.ts", ".ts"));
    assert!(!file_extension_is("ts", ".ts"));
    assert!(!file_extension_is("file.ts", ".tsx"));
}

#[test]
fn test_to_file_name_lower_case_lowers_ascii_and_preserves_special_case_only_strings() {
    assert_eq!(
        to_file_name_lower_case("SRC/COMPONENT.TS"),
        "src/component.ts"
    );
    assert_eq!(to_file_name_lower_case("İıß"), "İıß");
}

#[test]
fn test_character_classification_helpers_cover_boundaries() {
    assert!(is_line_break(CharacterCodes::LINE_FEED));
    assert!(is_line_break(CharacterCodes::PARAGRAPH_SEPARATOR));
    assert!(!is_line_break(CharacterCodes::SPACE));

    assert!(is_white_space_single_line(CharacterCodes::SPACE));
    assert!(is_white_space_single_line(CharacterCodes::BYTE_ORDER_MARK));
    assert!(!is_white_space_single_line(CharacterCodes::LINE_FEED));
    assert!(is_white_space_like(CharacterCodes::LINE_FEED));
    assert!(is_white_space_like(CharacterCodes::SPACE));

    assert!(is_digit(CharacterCodes::_0));
    assert!(is_digit(CharacterCodes::_9));
    assert!(!is_digit(CharacterCodes::LOWER_A));

    assert!(is_octal_digit(CharacterCodes::_7));
    assert!(!is_octal_digit(CharacterCodes::_8));

    assert!(is_hex_digit(CharacterCodes::UPPER_F));
    assert!(is_hex_digit(CharacterCodes::LOWER_A));
    assert!(!is_hex_digit(CharacterCodes::LOWER_G));

    assert!(is_ascii_letter(CharacterCodes::UPPER_Z));
    assert!(is_ascii_letter(CharacterCodes::LOWER_A));
    assert!(!is_ascii_letter(CharacterCodes::_0));

    assert!(is_word_character(CharacterCodes::UNDERSCORE));
    assert!(is_word_character(CharacterCodes::_4));
    assert!(is_word_character(CharacterCodes::LOWER_Z));
    assert!(!is_word_character('-' as u32));
}
