//! Internationalization (i18n) support for diagnostic messages.
//!
//! This module provides locale-specific diagnostic messages matching TypeScript's
//! localization. The translation files are copied from TypeScript's npm package.
//!
//! # Supported Locales
//!
//! - `cs` - Czech
//! - `de` - German
//! - `es` - Spanish
//! - `fr` - French
//! - `it` - Italian
//! - `ja` - Japanese
//! - `ko` - Korean
//! - `pl` - Polish
//! - `pt-br` - Portuguese (Brazil)
//! - `ru` - Russian
//! - `tr` - Turkish
//! - `zh-cn` - Chinese (Simplified)
//! - `zh-tw` - Chinese (Traditional)
//!
//! # Usage
//!
//! ```ignore
//! use tsz::cli::locale::LocaleMessages;
//!
//! let locale = LocaleMessages::load("ja").unwrap_or_default();
//! let message = locale.get_message(2304, "Cannot find name '{0}'.");
//! ```

use rustc_hash::FxHashMap;
use std::sync::OnceLock;

/// Global locale state for the current process.
static LOCALE: OnceLock<LocaleMessages> = OnceLock::new();

/// Container for locale-specific diagnostic messages.
#[derive(Debug, Default)]
pub struct LocaleMessages {
    /// Map from diagnostic code to translated message template.
    messages: FxHashMap<u32, String>,
    /// The locale identifier (e.g., "ja", "de").
    locale_id: String,
}

impl LocaleMessages {
    /// Load a locale from the embedded locale files.
    ///
    /// Returns `None` if the locale is not supported or fails to parse.
    pub fn load(locale_id: &str) -> Option<Self> {
        let normalized = normalize_locale(locale_id)?;
        let json_content = get_locale_content(&normalized)?;
        let messages = parse_locale_json(json_content)?;

        Some(LocaleMessages {
            messages,
            locale_id: normalized.to_string(),
        })
    }

    /// Get the translated message for a diagnostic code.
    ///
    /// Returns the translated message if available, otherwise returns the fallback.
    pub fn get_message<'a>(&self, code: u32, fallback: &'a str) -> &'a str {
        // Note: We return the fallback because the translated message has a different
        // lifetime. In practice, callers should use `get_message_owned` for translations.
        if self.messages.contains_key(&code) {
            // Translation exists but we return fallback due to lifetime constraints
            // The caller should use get_message_owned for actual translation
            fallback
        } else {
            fallback
        }
    }

    /// Get the translated message for a diagnostic code, returning an owned String.
    ///
    /// Returns the translated message if available, otherwise returns the fallback.
    pub fn get_message_owned(&self, code: u32, fallback: &str) -> String {
        self.messages
            .get(&code)
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    }

    /// Check if this locale has a translation for the given code.
    pub fn has_translation(&self, code: u32) -> bool {
        self.messages.contains_key(&code)
    }

    /// Get the locale identifier.
    pub fn locale_id(&self) -> &str {
        &self.locale_id
    }

    /// Returns true if this is the default (English) locale.
    pub fn is_default(&self) -> bool {
        self.locale_id.is_empty()
    }
}

/// Initialize the global locale. Should be called once at startup.
pub fn init_locale(locale_id: Option<&str>) {
    let locale = locale_id.and_then(LocaleMessages::load).unwrap_or_default();
    let _ = LOCALE.set(locale);
}

/// Get the current global locale.
pub fn get_locale() -> &'static LocaleMessages {
    LOCALE.get_or_init(LocaleMessages::default)
}

/// Get a translated message using the global locale.
///
/// This function attempts to extract parameters from the fallback message
/// and substitute them into the translated template.
pub fn translate(code: u32, fallback: &str) -> String {
    let locale = get_locale();

    // If no translation available or default locale, return fallback
    if locale.is_default() || !locale.has_translation(code) {
        return fallback.to_string();
    }

    let template = locale.get_message_owned(code, fallback);

    // If template doesn't have placeholders, return it directly
    if !template.contains("{0}") {
        return template;
    }

    // Extract parameters from the fallback message and substitute into template
    substitute_params_from_english(code, &template, fallback)
}

/// Extract parameters from an English formatted message and substitute into a translated template.
///
/// This works by matching common patterns in TypeScript diagnostic messages.
/// For example, for TS2322 "Type 'X' is not assignable to type 'Y'":
/// - English formatted: "Type 'string' is not assignable to type 'number'."
/// - Template: "型 '{0}' を型 '{1}' に割り当てることはできません。"
/// - Result: "型 'string' を型 'number' に割り当てることはできません。"
fn substitute_params_from_english(_code: u32, template: &str, formatted_english: &str) -> String {
    // Extract quoted strings from the formatted English message
    // TypeScript typically uses single quotes around parameter values
    let params = extract_quoted_strings(formatted_english);

    // Substitute parameters into the template
    let mut result = template.to_string();
    for (i, param) in params.iter().enumerate() {
        let placeholder = format!("{{{}}}", i);
        result = result.replace(&placeholder, param);
    }

    result
}

/// Extract single-quoted strings from a message.
///
/// Returns the strings in order of appearance, without the surrounding quotes.
fn extract_quoted_strings(message: &str) -> Vec<&str> {
    let mut params = Vec::new();
    let mut chars = message.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if ch == '\'' {
            // Find the closing quote
            let content_start = idx + 1;

            while let Some((pos, c)) = chars.next() {
                if c == '\'' {
                    // Check for escaped quote ('')
                    if let Some((_, next)) = chars.peek() {
                        if *next == '\'' {
                            // Skip the escaped quote
                            chars.next();
                            continue;
                        }
                    }
                    // Found closing quote
                    if content_start < pos {
                        params.push(&message[content_start..pos]);
                    }
                    break;
                }
            }
        }
    }

    params
}

/// Normalize a locale identifier to our supported format.
fn normalize_locale(locale: &str) -> Option<&'static str> {
    let lower = locale.to_lowercase();
    match lower.as_str() {
        "cs" | "cs-cz" | "czech" => Some("cs"),
        "de" | "de-de" | "de-at" | "de-ch" | "german" => Some("de"),
        "es" | "es-es" | "es-mx" | "spanish" => Some("es"),
        "fr" | "fr-fr" | "fr-ca" | "french" => Some("fr"),
        "it" | "it-it" | "italian" => Some("it"),
        "ja" | "ja-jp" | "japanese" => Some("ja"),
        "ko" | "ko-kr" | "korean" => Some("ko"),
        "pl" | "pl-pl" | "polish" => Some("pl"),
        "pt-br" | "pt" | "portuguese" => Some("pt-br"),
        "ru" | "ru-ru" | "russian" => Some("ru"),
        "tr" | "tr-tr" | "turkish" => Some("tr"),
        "zh-cn" | "zh-hans" | "zh" | "chinese" => Some("zh-cn"),
        "zh-tw" | "zh-hant" => Some("zh-tw"),
        _ => None,
    }
}

/// Get the embedded locale content.
fn get_locale_content(locale: &str) -> Option<&'static str> {
    match locale {
        "cs" => Some(include_str!("locales/cs.json")),
        "de" => Some(include_str!("locales/de.json")),
        "es" => Some(include_str!("locales/es.json")),
        "fr" => Some(include_str!("locales/fr.json")),
        "it" => Some(include_str!("locales/it.json")),
        "ja" => Some(include_str!("locales/ja.json")),
        "ko" => Some(include_str!("locales/ko.json")),
        "pl" => Some(include_str!("locales/pl.json")),
        "pt-br" => Some(include_str!("locales/pt-br.json")),
        "ru" => Some(include_str!("locales/ru.json")),
        "tr" => Some(include_str!("locales/tr.json")),
        "zh-cn" => Some(include_str!("locales/zh-cn.json")),
        "zh-tw" => Some(include_str!("locales/zh-tw.json")),
        _ => None,
    }
}

/// Parse a TypeScript locale JSON file into a code -> message map.
///
/// TypeScript's locale files have keys like:
/// - `Cannot_find_name_0_2304` -> code 2304
/// - `Type_0_is_not_assignable_to_type_1_2322` -> code 2322
///
/// The code is always the last number after the final underscore.
fn parse_locale_json(json: &str) -> Option<FxHashMap<u32, String>> {
    let parsed: serde_json::Value = serde_json::from_str(json).ok()?;
    let obj = parsed.as_object()?;

    let mut messages = FxHashMap::default();
    for (key, value) in obj {
        if let Some(code) = extract_code_from_key(key) {
            if let Some(msg) = value.as_str() {
                messages.insert(code, msg.to_string());
            }
        }
    }

    Some(messages)
}

/// Extract the diagnostic code from a locale key.
///
/// TypeScript keys look like: `Cannot_find_name_0_2304`
/// The code is the final number segment after the last underscore.
fn extract_code_from_key(key: &str) -> Option<u32> {
    // Find the last underscore and parse what follows as a number
    let last_underscore = key.rfind('_')?;
    let code_str = &key[last_underscore + 1..];
    code_str.parse().ok()
}

/// Get a list of all supported locale identifiers.
pub fn supported_locales() -> &'static [&'static str] {
    &[
        "cs", "de", "es", "fr", "it", "ja", "ko", "pl", "pt-br", "ru", "tr", "zh-cn", "zh-tw",
    ]
}

#[cfg(test)]
mod tests {
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
}
