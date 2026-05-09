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
use tsz_common::diagnostics::get_message_template;

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
        let json_content = get_locale_content(normalized)?;
        let messages = parse_locale_json(json_content)?;

        Some(Self {
            messages,
            locale_id: normalized.to_string(),
        })
    }

    /// Get the translated message for a diagnostic code.
    ///
    /// Returns the translated message if available, otherwise returns the fallback.
    pub fn get_message<'a>(&'a self, code: u32, fallback: &'a str) -> &'a str {
        self.messages.get(&code).map_or(fallback, String::as_str)
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
    pub const fn is_default(&self) -> bool {
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
    if !contains_placeholder(&template) {
        return template;
    }

    // Extract parameters from the fallback message and substitute into template
    substitute_params_from_english(code, &template, fallback)
}

/// Extract parameters from an English formatted message and substitute into a translated template.
///
/// Prefer matching against TypeScript's generated English diagnostic template. That recovers
/// unquoted parameters such as TS2554's numeric counts, while the quoted-string fallback keeps
/// compatibility with hand-authored diagnostics that still follow TypeScript's common quoting
/// pattern. For example, for TS2322 "Type 'X' is not assignable to type 'Y'":
/// - English formatted: "Type 'string' is not assignable to type 'number'."
/// - Template: "型 '{0}' を型 '{1}' に割り当てることはできません。"
/// - Result: "型 'string' を型 'number' に割り当てることはできません。"
fn substitute_params_from_english(code: u32, template: &str, formatted_english: &str) -> String {
    if let Some(english_template) = get_message_template(code)
        && let Some(params) = extract_params_from_template(english_template, formatted_english)
        && params.iter().any(Option::is_some)
    {
        return substitute_params(template, &params);
    }

    let params = extract_quoted_strings(formatted_english)
        .into_iter()
        .map(|param| Some(param.to_string()))
        .collect::<Vec<_>>();

    substitute_params(template, &params)
}

/// Substitute indexed placeholders with extracted parameter values.
fn substitute_params(template: &str, params: &[Option<String>]) -> String {
    // Substitute parameters into the template
    let mut result = template.to_string();
    for (i, param) in params.iter().enumerate() {
        if let Some(param) = param {
            let placeholder = format!("{{{i}}}");
            result = result.replace(&placeholder, param);
        }
    }

    result
}

#[derive(Debug, PartialEq, Eq)]
enum TemplatePart<'a> {
    Literal(&'a str),
    Placeholder(usize),
}

fn contains_placeholder(template: &str) -> bool {
    parse_template_parts(template)
        .iter()
        .any(|part| matches!(part, TemplatePart::Placeholder(_)))
}

fn extract_params_from_template(
    english_template: &str,
    formatted_english: &str,
) -> Option<Vec<Option<String>>> {
    let parts = parse_template_parts(english_template);
    if !parts
        .iter()
        .any(|part| matches!(part, TemplatePart::Placeholder(_)))
    {
        return Some(Vec::new());
    }

    let mut params = Vec::<Option<String>>::new();
    let mut formatted_pos = 0;

    for (idx, part) in parts.iter().enumerate() {
        match part {
            TemplatePart::Literal(literal) => {
                if !formatted_english[formatted_pos..].starts_with(literal) {
                    return None;
                }
                formatted_pos += literal.len();
            }
            TemplatePart::Placeholder(param_idx) => {
                if matches!(parts.get(idx + 1), Some(TemplatePart::Placeholder(_))) {
                    return None;
                }

                let next_literal = parts[idx + 1..].iter().find_map(|part| match part {
                    TemplatePart::Literal(literal) if !literal.is_empty() => Some(*literal),
                    _ => None,
                });

                let param_end = if let Some(next_literal) = next_literal {
                    formatted_english[formatted_pos..]
                        .find(next_literal)
                        .map(|offset| formatted_pos + offset)?
                } else {
                    formatted_english.len()
                };

                if params.len() <= *param_idx {
                    params.resize_with(param_idx + 1, || None);
                }
                params[*param_idx] = Some(formatted_english[formatted_pos..param_end].to_string());
                formatted_pos = param_end;
            }
        }
    }

    if formatted_pos == formatted_english.len() {
        Some(params)
    } else {
        None
    }
}

fn parse_template_parts(template: &str) -> Vec<TemplatePart<'_>> {
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut search_start = 0;

    while let Some(open_offset) = template[search_start..].find('{') {
        let open = search_start + open_offset;

        if let Some((placeholder_end, param_idx)) = parse_placeholder_at(template, open) {
            if open > literal_start {
                parts.push(TemplatePart::Literal(&template[literal_start..open]));
            }
            parts.push(TemplatePart::Placeholder(param_idx));
            literal_start = placeholder_end;
            search_start = placeholder_end;
        } else {
            search_start = open + 1;
        }
    }

    if literal_start < template.len() {
        parts.push(TemplatePart::Literal(&template[literal_start..]));
    }

    parts
}

fn parse_placeholder_at(template: &str, open: usize) -> Option<(usize, usize)> {
    let close = open + 1 + template[open + 1..].find('}')?;
    let index = &template[open + 1..close];
    if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((close + 1, index.parse().ok()?))
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
                    if let Some((_, next)) = chars.peek()
                        && *next == '\''
                    {
                        // Skip the escaped quote
                        chars.next();
                        continue;
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
        "cs" => Some(include_str!("../locales/cs.json")),
        "de" => Some(include_str!("../locales/de.json")),
        "es" => Some(include_str!("../locales/es.json")),
        "fr" => Some(include_str!("../locales/fr.json")),
        "it" => Some(include_str!("../locales/it.json")),
        "ja" => Some(include_str!("../locales/ja.json")),
        "ko" => Some(include_str!("../locales/ko.json")),
        "pl" => Some(include_str!("../locales/pl.json")),
        "pt-br" => Some(include_str!("../locales/pt-br.json")),
        "ru" => Some(include_str!("../locales/ru.json")),
        "tr" => Some(include_str!("../locales/tr.json")),
        "zh-cn" => Some(include_str!("../locales/zh-cn.json")),
        "zh-tw" => Some(include_str!("../locales/zh-tw.json")),
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
        if let Some(code) = extract_code_from_key(key)
            && let Some(msg) = value.as_str()
        {
            messages.insert(code, msg.to_string());
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
pub const fn supported_locales() -> &'static [&'static str] {
    &[
        "cs", "de", "es", "fr", "it", "ja", "ko", "pl", "pt-br", "ru", "tr", "zh-cn", "zh-tw",
    ]
}

#[cfg(test)]
#[path = "../locale_tests.rs"]
mod tests;
