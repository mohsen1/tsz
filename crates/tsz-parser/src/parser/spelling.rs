//! Spelling suggestions for misspelled keywords.
//!
//! Implements TypeScript's `parseErrorForMissingSemicolonAfter` keyword suggestion
//! logic: Levenshtein distance matching and keyword-concatenation detection.

/// Keywords viable for spelling suggestions (length > 2).
/// Matches TypeScript's `viableKeywordSuggestions` which filters
/// `Object.keys(textToKeywordObj)` to keywords with length > 2.
pub const VIABLE_KEYWORD_SUGGESTIONS: &[&str] = &[
    "abstract",
    "accessor",
    "any",
    "async",
    "await",
    "boolean",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "declare",
    "default",
    "delete",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "global",
    "implements",
    "import",
    "infer",
    "instanceof",
    "interface",
    "intrinsic",
    "keyof",
    "let",
    "module",
    "namespace",
    "never",
    "new",
    "null",
    "number",
    "object",
    "override",
    "package",
    "private",
    "protected",
    "public",
    "readonly",
    "require",
    "return",
    "satisfies",
    "set",
    "static",
    "string",
    "super",
    "switch",
    "symbol",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "unique",
    "unknown",
    "using",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Find a spelling suggestion for a word among the given candidates.
///
/// Matches TypeScript's `getSpellingSuggestion` algorithm:
/// - Candidates whose length differs by more than 0.34 * name.len() are skipped.
/// - Candidates shorter than 3 chars only match on case-insensitive equality.
/// - Returns the candidate with the smallest Levenshtein distance that is
///   within 0.4 * name.len() + 1.
pub fn get_spelling_suggestion<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let max_length_diff = (name.len() as f64 * 0.34).floor().max(2.0) as usize;
    let mut best_distance = (name.len() as f64 * 0.4).floor() + 1.0;
    let mut best_candidate: Option<&str> = None;

    for &candidate in candidates {
        if candidate == name {
            continue;
        }

        let len_diff = name.len().abs_diff(candidate.len());
        if len_diff > max_length_diff {
            continue;
        }

        if candidate.len() < 3 && !candidate.eq_ignore_ascii_case(name) {
            continue;
        }

        if let Some(distance) = levenshtein_with_max(name, candidate, best_distance - 0.1) {
            best_distance = distance;
            best_candidate = Some(candidate);
        }
    }

    best_candidate
}

/// Check if an identifier is a keyword concatenated with another word.
///
/// e.g. `"declareconst"` → `"declare const"`,
///      `"interfaceMyInterface"` → `"interface MyInterface"`.
///
/// Only returns a suggestion when the remainder after the keyword is
/// longer than 2 characters, matching TypeScript's `getSpaceSuggestion`.
pub fn get_space_suggestion(text: &str) -> Option<String> {
    for &keyword in VIABLE_KEYWORD_SUGGESTIONS {
        if text.len() > keyword.len() + 2 && text.starts_with(keyword) {
            return Some(format!("{} {}", keyword, &text[keyword.len()..]));
        }
    }
    None
}

/// Return a keyword suggestion for the given identifier, trying spelling
/// first, then keyword-concatenation.
pub fn suggest_keyword(text: &str) -> Option<String> {
    if let Some(s) = get_spelling_suggestion(text, VIABLE_KEYWORD_SUGGESTIONS) {
        return Some(s.to_string());
    }
    get_space_suggestion(text)
}

/// Levenshtein distance with early termination when the distance exceeds `max`.
///
/// Matches TypeScript's `levenshteinWithMax`:
/// - Exact char match: cost 0
/// - Case-only difference: cost 0.1
/// - Other substitution: cost 2.0
/// - Insertion / deletion: cost 1.0
fn levenshtein_with_max(s1: &str, s2: &str, max: f64) -> Option<f64> {
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    let s1_len = s1_chars.len();
    let s2_len = s2_chars.len();
    let big = max + 0.01;

    let mut previous = vec![0.0_f64; s2_len + 1];
    let mut current = vec![0.0_f64; s2_len + 1];

    for i in 0..=s2_len {
        previous[i] = i as f64;
    }

    for i in 1..=s1_len {
        let c1 = s1_chars[i - 1];
        let min_j = if (i as f64) > max {
            (i as f64 - max).ceil() as usize
        } else {
            1
        };
        let max_j = if (s2_len as f64) > max + i as f64 {
            (max + i as f64).floor() as usize
        } else {
            s2_len
        };

        current[0] = i as f64;
        let mut col_min = i as f64;

        for j in 1..min_j {
            current[j] = big;
        }

        for j in min_j..=max_j {
            let substitution_distance = if s1_chars[i - 1]
                .to_lowercase()
                .eq(s2_chars[j - 1].to_lowercase())
            {
                previous[j - 1] + 0.1
            } else {
                previous[j - 1] + 2.0
            };

            let dist = if c1 == s2_chars[j - 1] {
                previous[j - 1]
            } else {
                f64::min(
                    f64::min(
                        previous[j] + 1.0,    // delete
                        current[j - 1] + 1.0, // insert
                    ),
                    substitution_distance, // substitute
                )
            };

            current[j] = dist;
            col_min = f64::min(col_min, dist);
        }

        for j in (max_j + 1)..=s2_len {
            current[j] = big;
        }

        if col_min > max {
            return None;
        }

        std::mem::swap(&mut previous, &mut current);
    }

    let res = previous[s2_len];
    if res > max { None } else { Some(res) }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // levenshtein_with_max
    // =====================================================================

    #[test]
    fn levenshtein_identical_strings() {
        assert_eq!(levenshtein_with_max("abc", "abc", 5.0), Some(0.0));
    }

    #[test]
    fn levenshtein_case_difference_is_cheap() {
        let dist = levenshtein_with_max("Abc", "abc", 5.0).expect("distance calculation should succeed");
        // Case-only substitution costs 0.1
        assert!(dist < 1.0, "case-only diff should be < 1.0, got {dist}");
        assert!((dist - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn levenshtein_single_char_substitution() {
        // "asynd" vs "async": positions 1-4 identical, pos 5: d→c (cost 2.0)
        let dist = levenshtein_with_max("asynd", "async", 3.0).expect("distance calculation should succeed");
        assert!(
            (dist - 2.0).abs() < f64::EPSILON,
            "expected 2.0, got {dist}"
        );
    }

    #[test]
    fn levenshtein_insertion() {
        // "classs" vs "class": one extra 's' (cost 1.0)
        let dist = levenshtein_with_max("classs", "class", 3.0).expect("distance calculation should succeed");
        assert!(
            (dist - 1.0).abs() < f64::EPSILON,
            "expected 1.0, got {dist}"
        );
    }

    #[test]
    fn levenshtein_deletion() {
        // "clas" vs "class": one missing 's' (cost 1.0)
        let dist = levenshtein_with_max("clas", "class", 3.0).expect("distance calculation should succeed");
        assert!(
            (dist - 1.0).abs() < f64::EPSILON,
            "expected 1.0, got {dist}"
        );
    }

    #[test]
    fn levenshtein_exceeds_max() {
        assert!(levenshtein_with_max("abc", "xyz", 1.0).is_none());
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein_with_max("", "", 5.0), Some(0.0));
        assert_eq!(levenshtein_with_max("abc", "", 5.0), Some(3.0));
        assert_eq!(levenshtein_with_max("", "abc", 5.0), Some(3.0));
    }

    // =====================================================================
    // get_spelling_suggestion
    // =====================================================================

    #[test]
    fn spelling_common_misspellings() {
        let cases = [
            ("asynd", "async"),
            ("sasync", "async"),
            ("clasd", "class"),
            ("classs", "class"),
            ("consd", "const"),
            ("constd", "const"),
            ("functiond", "function"),
            ("interfaced", "interface"),
            ("letd", "let"),
            ("typed", "type"),
            ("vard", "var"),
            ("declared", "declare"),
        ];
        for (input, expected) in cases {
            let result = get_spelling_suggestion(input, VIABLE_KEYWORD_SUGGESTIONS);
            assert_eq!(result, Some(expected), "{input} should suggest {expected}");
        }
    }

    #[test]
    fn spelling_no_match_for_distant_words() {
        assert_eq!(
            get_spelling_suggestion("banana", VIABLE_KEYWORD_SUGGESTIONS),
            None
        );
    }

    #[test]
    fn spelling_exact_match_ignored() {
        // An exact match of the name itself should not be returned.
        assert_eq!(get_spelling_suggestion("class", &["class", "const"]), None);
    }

    #[test]
    fn spelling_short_candidates_only_case() {
        // Candidates < 3 chars only match on case difference.
        assert_eq!(get_spelling_suggestion("IF", &["if"]), Some("if"));
        assert_eq!(get_spelling_suggestion("ab", &["xy"]), None);
    }

    // =====================================================================
    // get_space_suggestion
    // =====================================================================

    #[test]
    fn space_keyword_concatenation() {
        assert_eq!(
            get_space_suggestion("declareconst"),
            Some("declare const".to_string())
        );
        assert_eq!(
            get_space_suggestion("interfaceMyInterface"),
            Some("interface MyInterface".to_string())
        );
        assert_eq!(
            get_space_suggestion("functionMyFunction"),
            Some("function MyFunction".to_string())
        );
    }

    #[test]
    fn space_no_suggestion_when_remainder_too_short() {
        // "let" + "a" = "leta" → remainder "a" len=1 which is ≤ 2
        assert_eq!(get_space_suggestion("leta"), None);
        assert_eq!(get_space_suggestion("letab"), None);
    }

    #[test]
    fn space_no_suggestion_for_plain_keyword() {
        assert_eq!(get_space_suggestion("class"), None);
    }

    // =====================================================================
    // suggest_keyword (combined)
    // =====================================================================

    #[test]
    fn suggest_prefers_spelling_over_space() {
        // "classs" should match via Levenshtein (→ "class"), not space
        assert_eq!(suggest_keyword("classs"), Some("class".to_string()));
    }

    #[test]
    fn suggest_falls_back_to_space() {
        assert_eq!(
            suggest_keyword("declareconst"),
            Some("declare const".to_string())
        );
    }

    #[test]
    fn suggest_no_match() {
        assert_eq!(suggest_keyword("xyzzy"), None);
    }
}
