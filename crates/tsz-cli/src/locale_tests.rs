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
