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
    let dist =
        levenshtein_with_max("Abc", "abc", 5.0).expect("distance calculation should succeed");
    // Case-only substitution costs 0.1
    assert!(dist < 1.0, "case-only diff should be < 1.0, got {dist}");
    assert!((dist - 0.1).abs() < f64::EPSILON);
}

#[test]
fn levenshtein_single_char_substitution() {
    // "asynd" vs "async": positions 1-4 identical, pos 5: d→c (cost 2.0)
    let dist =
        levenshtein_with_max("asynd", "async", 3.0).expect("distance calculation should succeed");
    assert!(
        (dist - 2.0).abs() < f64::EPSILON,
        "expected 2.0, got {dist}"
    );
}

#[test]
fn levenshtein_insertion() {
    // "classs" vs "class": one extra 's' (cost 1.0)
    let dist =
        levenshtein_with_max("classs", "class", 3.0).expect("distance calculation should succeed");
    assert!(
        (dist - 1.0).abs() < f64::EPSILON,
        "expected 1.0, got {dist}"
    );
}

#[test]
fn levenshtein_deletion() {
    // "clas" vs "class": one missing 's' (cost 1.0)
    let dist =
        levenshtein_with_max("clas", "class", 3.0).expect("distance calculation should succeed");
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
