use super::*;

#[test]
fn test_extract_code_from_key() {
    assert_eq!(extract_code_from_key("Cannot_find_name_0_2304"), Some(2304));
    assert_eq!(
        extract_code_from_key("Type_0_is_not_assignable_to_type_1_2322"),
        Some(2322)
    );
    assert_eq!(
        extract_code_from_key("A_0_modifier_cannot_be_used_with_an_import_declaration_1079"),
        Some(1079)
    );
    assert_eq!(extract_code_from_key("no_code"), None);
    assert_eq!(extract_code_from_key("trailing_text_123abc"), None);
}

#[test]
fn test_normalize_locale() {
    assert_eq!(normalize_locale("ja"), Some("ja"));
    assert_eq!(normalize_locale("JA"), Some("ja"));
    assert_eq!(normalize_locale("ja-jp"), Some("ja"));
    assert_eq!(normalize_locale("japanese"), Some("ja"));
    assert_eq!(normalize_locale("zh-cn"), Some("zh-cn"));
    assert_eq!(normalize_locale("zh-hans"), Some("zh-cn"));
    assert_eq!(normalize_locale("pt-br"), Some("pt-br"));
    assert_eq!(normalize_locale("unknown"), None);
}

#[test]
fn test_load_japanese_locale() {
    let locale = LocaleMessages::load("ja").expect("ja locale should load");
    assert_eq!(locale.locale_id(), "ja");

    // TS2304: Cannot find name '{0}'.
    assert!(locale.has_translation(2304));

    let msg = locale.get_message_owned(2304, "Cannot find name '{0}'.");
    assert!(msg.contains("名前"));
}

#[test]
fn test_load_german_locale() {
    let locale = LocaleMessages::load("de").expect("de locale should load");
    assert_eq!(locale.locale_id(), "de");

    // TS2322: Type '{0}' is not assignable to type '{1}'.
    assert!(locale.has_translation(2322));
}

#[test]
fn test_get_message_returns_fallback_for_borrowed_api_even_when_translation_exists() {
    let locale = LocaleMessages::load("ja").expect("ja locale should load");
    let fallback = "Cannot find name '{0}'.";

    assert_eq!(locale.get_message(2304, fallback), fallback);
}

#[test]
fn test_get_message_owned_prefers_translation_when_available() {
    let locale = LocaleMessages::load("ja").expect("ja locale should load");
    let fallback = "Cannot find name '{0}'.";

    let translated = locale.get_message_owned(2304, fallback);
    assert_ne!(translated, fallback);
    assert!(translated.contains("{0}") || translated.contains("名前"));
}

#[test]
fn test_fallback_for_unknown_code() {
    let locale = LocaleMessages::load("ja").expect("ja locale should load");
    let fallback = "Unknown error";
    let msg = locale.get_message_owned(99999, fallback);
    assert_eq!(msg, fallback);
}

#[test]
fn test_default_locale_is_empty() {
    let locale = LocaleMessages::default();
    assert!(locale.is_default());
    assert!(locale.locale_id().is_empty());
}

#[test]
fn test_supported_locales() {
    let locales = supported_locales();
    assert!(locales.contains(&"ja"));
    assert!(locales.contains(&"de"));
    assert!(locales.contains(&"zh-cn"));
    assert_eq!(locales.len(), 13);
}

#[test]
fn test_extract_quoted_strings_handles_multiple_and_escaped_quotes() {
    let params = extract_quoted_strings("Type 'a''b' is not assignable to 'c'.");
    assert_eq!(params, vec!["a''b", "c"]);
}

#[test]
fn test_substitute_params_from_english_replaces_placeholders_in_order() {
    let template = "型 '{0}' を型 '{1}' に割り当てることはできません。";
    let english = "Type 'string' is not assignable to type 'number'.";
    let substituted = substitute_params_from_english(2322, template, english);

    assert_eq!(
        substituted,
        "型 'string' を型 'number' に割り当てることはできません。"
    );
}

#[test]
fn test_substitute_params_from_english_leaves_missing_placeholders_intact() {
    let template = "Cannot use '{0}' with '{1}' and '{2}'.";
    let english = "Cannot use 'a' with 'b'.";
    let substituted = substitute_params_from_english(9999, template, english);

    assert_eq!(substituted, "Cannot use 'a' with 'b' and '{2}'.");
}

#[test]
fn test_parse_locale_json_ignores_non_string_and_non_code_entries() {
    let json = r#"{
        "Cannot_find_name_0_2304": "Translated",
        "Not_a_diagnostic": "ignored",
        "Type_0_2322": 123,
        "Another_1001": "Kept"
    }"#;

    let parsed = parse_locale_json(json).expect("json object should parse");
    assert_eq!(parsed.get(&2304).map(String::as_str), Some("Translated"));
    assert_eq!(parsed.get(&1001).map(String::as_str), Some("Kept"));
    assert!(!parsed.contains_key(&2322));
    assert_eq!(parsed.len(), 2);
}

#[test]
fn test_parse_locale_json_rejects_invalid_or_non_object_json() {
    assert!(parse_locale_json("not json").is_none());
    assert!(parse_locale_json(r#"["array"]"#).is_none());
}

// =============================================================================
// extract_quoted_strings edge cases
// =============================================================================

#[test]
fn test_extract_quoted_strings_empty_input() {
    assert!(extract_quoted_strings("").is_empty());
}

#[test]
fn test_extract_quoted_strings_no_quotes() {
    assert!(extract_quoted_strings("just plain text with no quotes").is_empty());
}

#[test]
fn test_extract_quoted_strings_unterminated_quote_drops_param() {
    // Single unmatched opening quote -> nothing pushed (loop ends without finding closing).
    let params = extract_quoted_strings("Type 'unterminated and beyond");
    assert!(params.is_empty());
}

#[test]
fn test_extract_quoted_strings_empty_quotes_are_ignored() {
    // Empty quotes '' produce no entry because content_start == pos.
    let params = extract_quoted_strings("Type '' is not 'real'.");
    assert_eq!(params, vec!["real"]);
}

#[test]
fn test_extract_quoted_strings_multibyte_content() {
    // Multibyte UTF-8 ('é' is 2 bytes, '中' is 3 bytes) inside quotes round-trips intact.
    let params = extract_quoted_strings("Type 'café' must equal '中文'.");
    assert_eq!(params, vec!["café", "中文"]);
}

#[test]
fn test_extract_quoted_strings_leading_quote() {
    // Quote at byte 0 is handled the same as anywhere else in the message.
    let params = extract_quoted_strings("'leading' content");
    assert_eq!(params, vec!["leading"]);
}

// =============================================================================
// substitute_params_from_english edge cases
// =============================================================================

#[test]
fn test_substitute_params_from_english_out_of_order_placeholders() {
    // Template uses {1} before {0} — substitution happens by index, not appearance order.
    let template = "{1} then {0}";
    let english = "Type 'a' and 'b'.";
    let substituted = substitute_params_from_english(2322, template, english);
    assert_eq!(substituted, "b then a");
}

#[test]
fn test_substitute_params_from_english_repeated_placeholder() {
    // {0} appears twice; both copies are substituted because str::replace replaces all matches.
    let template = "{0} and {0}";
    let english = "Use 'thing'.";
    let substituted = substitute_params_from_english(0, template, english);
    assert_eq!(substituted, "thing and thing");
}

#[test]
fn test_substitute_params_from_english_extra_english_params_ignored() {
    // Template only uses {0}; extra english params {1}, {2} have no placeholder to replace.
    let template = "Only {0} matters";
    let english = "First 'a' second 'b' third 'c'.";
    let substituted = substitute_params_from_english(0, template, english);
    assert_eq!(substituted, "Only a matters");
}

#[test]
fn test_substitute_params_from_english_empty_english_leaves_template() {
    // No quoted params extracted -> template returned unchanged (placeholders intact).
    let template = "Cannot find '{0}'";
    let english = "no quotes here at all";
    let substituted = substitute_params_from_english(0, template, english);
    assert_eq!(substituted, "Cannot find '{0}'");
}

// =============================================================================
// parse_locale_json shape edge cases
// =============================================================================

#[test]
fn test_parse_locale_json_empty_object_returns_empty_map() {
    let parsed = parse_locale_json("{}").expect("empty object should parse");
    assert!(parsed.is_empty());
}

#[test]
fn test_parse_locale_json_skips_null_numeric_array_values() {
    let json = r#"{
        "Cannot_find_name_0_2304": "Translated",
        "Other_2305": null,
        "Yet_another_2306": [1, 2, 3],
        "Numeric_2307": 42,
        "Boolean_2308": true,
        "Object_2309": {"nested": "value"}
    }"#;
    let parsed = parse_locale_json(json).expect("json object should parse");
    assert_eq!(parsed.get(&2304).map(String::as_str), Some("Translated"));
    assert!(!parsed.contains_key(&2305));
    assert!(!parsed.contains_key(&2306));
    assert!(!parsed.contains_key(&2307));
    assert!(!parsed.contains_key(&2308));
    assert!(!parsed.contains_key(&2309));
    assert_eq!(parsed.len(), 1);
}

#[test]
fn test_parse_locale_json_skips_keys_without_trailing_code() {
    let json = r#"{
        "no_trailing_code": "ignored",
        "Has_code_5000": "kept",
        "12345": "code-only-key-no-underscore-also-ignored"
    }"#;
    let parsed = parse_locale_json(json).expect("json object should parse");
    assert_eq!(parsed.get(&5000).map(String::as_str), Some("kept"));
    // "12345" has no underscore so extract_code_from_key returns None.
    assert!(!parsed.contains_key(&12345));
    assert_eq!(parsed.len(), 1);
}

// =============================================================================
// extract_code_from_key edge cases
// =============================================================================

#[test]
fn test_extract_code_from_key_empty_input() {
    assert_eq!(extract_code_from_key(""), None);
}

#[test]
fn test_extract_code_from_key_no_underscore() {
    // No underscore -> rfind returns None.
    assert_eq!(extract_code_from_key("abc"), None);
    // Even pure-numeric without underscore is rejected.
    assert_eq!(extract_code_from_key("12345"), None);
}

#[test]
fn test_extract_code_from_key_trailing_underscore() {
    // Empty suffix after the last underscore fails to parse.
    assert_eq!(extract_code_from_key("abc_"), None);
    assert_eq!(extract_code_from_key("Cannot_find_"), None);
}

#[test]
fn test_extract_code_from_key_negative_number_rejected() {
    // u32 parse rejects negative input even after an underscore.
    assert_eq!(extract_code_from_key("Foo_-1"), None);
}

// =============================================================================
// normalize_locale edge cases
// =============================================================================

#[test]
fn test_normalize_locale_empty_returns_none() {
    assert_eq!(normalize_locale(""), None);
}

#[test]
fn test_normalize_locale_whitespace_not_normalized() {
    // Surrounding whitespace is not trimmed; lookup misses.
    assert_eq!(normalize_locale(" ja "), None);
    assert_eq!(normalize_locale("ja "), None);
    assert_eq!(normalize_locale(" ja"), None);
}

#[test]
fn test_normalize_locale_unsupported_two_letter() {
    assert_eq!(normalize_locale("xx"), None);
    assert_eq!(normalize_locale("zz-zz"), None);
}

#[test]
fn test_normalize_locale_mixed_case_region_tag() {
    // Mixed-case region tags are lowercased before matching.
    assert_eq!(normalize_locale("Ja-JP"), Some("ja"));
    assert_eq!(normalize_locale("ZH-Hans"), Some("zh-cn"));
    assert_eq!(normalize_locale("PT-BR"), Some("pt-br"));
}

// =============================================================================
// LocaleMessages::load failure path
// =============================================================================

#[test]
fn test_load_unknown_locale_returns_none() {
    assert!(LocaleMessages::load("xx").is_none());
    assert!(LocaleMessages::load("klingon").is_none());
    assert!(LocaleMessages::load("").is_none());
}

#[test]
fn test_load_normalizes_alias_locale_id() {
    // Loading an alias ('japanese' -> 'ja') sets locale_id to the canonical short form.
    let locale = LocaleMessages::load("japanese").expect("japanese alias should load");
    assert_eq!(locale.locale_id(), "ja");
    assert!(!locale.is_default());
}
