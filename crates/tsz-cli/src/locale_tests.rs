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
