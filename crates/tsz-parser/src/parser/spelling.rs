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
/// - Candidates whose length differs by more than `name.len() * 34 / 100` are skipped.
/// - Candidates shorter than 3 chars only match on case-insensitive equality.
/// - Returns the candidate with the smallest Levenshtein distance that is
///   within `name.len() * 4 / 10 + 1`.
pub fn get_spelling_suggestion<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let name_len = name.len();
    let max_length_diff = (name_len * 34 / 100).max(2);
    let mut best_distance = name_len * 4 + 10;
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

        if let Some(distance) = levenshtein_with_max(name, candidate, best_distance - 1) {
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
fn levenshtein_with_max(s1: &str, s2: &str, max: usize) -> Option<usize> {
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    let s1_len = s1_chars.len();
    let s2_len = s2_chars.len();
    let big = max + 1;

    let mut previous = vec![0_usize; s2_len + 1];
    let mut current = vec![0_usize; s2_len + 1];

    for (i, prev) in previous.iter_mut().enumerate().take(s2_len + 1) {
        *prev = i * 10;
    }

    for i in 1..=s1_len {
        let c1 = s1_chars[i - 1];
        let scaled_i = i * 10;
        let min_j = if scaled_i > max {
            scaled_i.saturating_sub(max).div_ceil(10)
        } else {
            1
        };
        let max_j = if s2_len * 10 > max + scaled_i {
            (max + scaled_i) / 10
        } else {
            s2_len
        };

        let scaled_i_for_cost = i * 10;
        current[0] = scaled_i_for_cost;
        let mut col_min = scaled_i_for_cost;

        for (_, current_j) in current.iter_mut().take(min_j).enumerate().skip(1) {
            *current_j = big;
        }

        for j in min_j..=max_j {
            let c2 = s2_chars[j - 1];
            let substitution_distance = if c1.eq_ignore_ascii_case(&c2) {
                previous[j - 1] + 1
            } else {
                previous[j - 1] + 20
            };

            let dist = if c1 == s2_chars[j - 1] {
                previous[j - 1]
            } else {
                usize::min(
                    usize::min(
                        previous[j] + 10,    // delete
                        current[j - 1] + 10, // insert
                    ),
                    substitution_distance, // substitute
                )
            };

            current[j] = dist;
            col_min = usize::min(col_min, dist);
        }

        for (_, current_j) in current
            .iter_mut()
            .take(s2_len + 1)
            .enumerate()
            .skip(max_j + 1)
        {
            *current_j = big;
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
#[path = "../../tests/spelling.rs"]
mod tests;
