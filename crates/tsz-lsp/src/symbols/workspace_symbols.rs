//! Workspace Symbols implementation for LSP.
//!
//! Searches all indexed symbol definitions across the workspace and returns
//! matching results sorted by a fuzzy scorer that mirrors tsserver's
//! `patternMatcher.ts`:
//! - Exact (case-sensitive > case-insensitive)
//! - Prefix (case-sensitive > case-insensitive)
//! - Camel-case acronym from start (consecutive humps)
//! - Camel-case acronym anywhere
//! - Substring (case-sensitive > case-insensitive)
//!
//! Within a tier, ties break by file proximity to the active file, by symbol
//! length (shorter wins), then alphabetically.

use super::document_symbols::SymbolKind;
use super::symbol_index::SymbolIndex;
use tsz_common::position::Location;

/// Maximum number of results returned by a workspace symbol search.
const MAX_RESULTS: usize = 100;

/// Information about a symbol found in the workspace.
///
/// Corresponds to the LSP `SymbolInformation` type.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolInformation {
    /// The name of the symbol.
    pub name: String,
    /// The kind of the symbol (function, class, variable, etc.).
    pub kind: SymbolKind,
    /// The location where the symbol is defined.
    pub location: Location,
}

impl SymbolInformation {
    /// Create a new symbol information entry.
    pub const fn new(name: String, kind: SymbolKind, location: Location) -> Self {
        Self {
            name,
            kind,
            location,
        }
    }
}

/// Relevance category for sorting search results.
///
/// Ordered so that `Ord` ranks better matches lower (better = first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MatchKind {
    /// Whole symbol equals query, character for character.
    ExactCaseSensitive = 0,
    /// Whole symbol equals query, case-insensitive.
    ExactCaseInsensitive = 1,
    /// Symbol starts with the query, character for character.
    PrefixCaseSensitive = 2,
    /// Symbol starts with the query, case-insensitive.
    PrefixCaseInsensitive = 3,
    /// Query maps onto consecutive camel-case humps that start at hump 0
    /// (e.g. `UD` -> `UserData`, `getU` -> `getUserById`).
    CamelCaseContiguousFromStart = 4,
    /// Query maps onto consecutive camel-case humps that start mid-name
    /// (e.g. `UD` -> `BaseUserData`).
    CamelCaseContiguous = 5,
    /// Query maps onto camel-case humps in order, possibly skipping humps
    /// (e.g. `UD` -> `UserStartData`).
    CamelCaseAnywhere = 6,
    /// Query appears as a case-sensitive substring of the symbol name.
    SubstringCaseSensitive = 7,
    /// Query appears as a case-insensitive substring of the symbol name.
    SubstringCaseInsensitive = 8,
}

/// Result of scoring a single symbol against a query.
///
/// The lexicographic order of this tuple is the relevance order used for
/// ranking results. Smaller is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MatchScore {
    /// Match tier; smaller = better.
    pub kind: MatchKind,
    /// 0 = same file as the active editor, 1 = sibling within the same
    /// parent directory, increasing for distance. `u32::MAX` when no
    /// active file is known.
    pub distance_from_active: u32,
    /// Symbol-name length in Unicode scalar values (shorter wins on ties).
    /// Counted in chars rather than bytes so multibyte identifiers
    /// (`Ångström`, `名前`) are not penalised against ASCII names.
    pub name_len: u32,
}

/// Provider for workspace-wide symbol search.
///
/// Searches across all indexed symbols in a project's `SymbolIndex` and
/// returns LSP-formatted `SymbolInformation` results.
pub struct WorkspaceSymbolsProvider<'a> {
    index: &'a SymbolIndex,
    active_file: Option<&'a str>,
}

impl<'a> WorkspaceSymbolsProvider<'a> {
    /// Create a new workspace symbols provider.
    pub const fn new(index: &'a SymbolIndex) -> Self {
        Self {
            index,
            active_file: None,
        }
    }

    /// Create a provider that knows which file the user is editing.
    ///
    /// Symbols whose definition lives in the active file (or near it in the
    /// directory tree) rank higher when match tiers tie.
    pub const fn with_active_file(index: &'a SymbolIndex, active_file: Option<&'a str>) -> Self {
        Self { index, active_file }
    }

    /// Find symbols matching the given query string.
    ///
    /// Performs fuzzy matching across the symbol index. At most 100 results
    /// are returned. An empty query yields no results.
    pub fn find_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        if query.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<(MatchScore, SymbolInformation)> = Vec::new();

        for name in self.index.all_definition_names() {
            let Some(kind) = score_match(name, query) else {
                continue;
            };

            let lookup_kind = self
                .index
                .get_definition_kind(name)
                .unwrap_or_else(|| Self::infer_symbol_kind(name));

            for location in self.index.find_definitions(name) {
                let distance = self.active_file.map_or(u32::MAX, |active| {
                    path_distance(active, &location.file_path)
                });
                let score = MatchScore {
                    kind,
                    distance_from_active: distance,
                    name_len: u32::try_from(name.chars().count()).unwrap_or(u32::MAX),
                };
                matches.push((
                    score,
                    SymbolInformation::new(name.to_string(), lookup_kind, location),
                ));
            }
        }

        matches.sort_by_cached_key(|(score, info)| {
            (
                *score,
                info.name.to_lowercase(),
                info.location.file_path.clone(),
            )
        });

        matches
            .into_iter()
            .take(MAX_RESULTS)
            .map(|(_, info)| info)
            .collect()
    }

    /// Infer the symbol kind from the symbol name.
    ///
    /// Used only when the symbol index has no stored kind. Naming-convention
    /// heuristics over user-chosen identifiers are deliberately limited; the
    /// real kind comes from the binder via the index.
    pub(crate) fn infer_symbol_kind(name: &str) -> SymbolKind {
        if name.is_empty() {
            return SymbolKind::Variable;
        }

        let bytes = name.as_bytes();

        // SCREAMING_SNAKE_CASE -> Constant.
        if bytes.len() > 1
            && bytes
                .iter()
                .all(|b| b.is_ascii_uppercase() || *b == b'_' || b.is_ascii_digit())
        {
            return SymbolKind::Constant;
        }

        // PascalCase -> Class, with `I`-prefix carved out for Interface.
        if bytes[0].is_ascii_uppercase() {
            if bytes.len() > 1 && bytes[0] == b'I' && bytes[1].is_ascii_uppercase() {
                return SymbolKind::Interface;
            }
            return SymbolKind::Class;
        }

        SymbolKind::Variable
    }
}

/// Score `name` against `query`, returning the best match tier or `None`
/// if the symbol does not match at all.
///
/// Tiers are ordered so that `MatchKind`'s natural ordering picks the
/// best match: exact > prefix > camel-case acronym > substring, with
/// case-sensitive variants beating case-insensitive ones within a kind.
///
/// Case-insensitive comparison uses Unicode lowercasing (`str::to_lowercase`)
/// so identifiers with non-ASCII letters (`Ångström`, `名前`, etc.) match
/// the same way tsserver's JS-side `.toLowerCase()` does.
pub(crate) fn score_match(name: &str, query: &str) -> Option<MatchKind> {
    if query.is_empty() || name.is_empty() {
        return None;
    }

    if name == query {
        return Some(MatchKind::ExactCaseSensitive);
    }

    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();

    if name_lower == query_lower {
        return Some(MatchKind::ExactCaseInsensitive);
    }
    if name.starts_with(query) {
        return Some(MatchKind::PrefixCaseSensitive);
    }
    if name_lower.starts_with(&query_lower) {
        return Some(MatchKind::PrefixCaseInsensitive);
    }

    let camel = camel_case_match(name, query).map(|c| match c {
        CamelMatch::ContiguousFromStart => MatchKind::CamelCaseContiguousFromStart,
        CamelMatch::Contiguous => MatchKind::CamelCaseContiguous,
        CamelMatch::Anywhere => MatchKind::CamelCaseAnywhere,
    });

    let substring = if name.contains(query) {
        Some(MatchKind::SubstringCaseSensitive)
    } else if name_lower.contains(&query_lower) {
        Some(MatchKind::SubstringCaseInsensitive)
    } else {
        None
    };

    [camel, substring].into_iter().flatten().min()
}

/// Return true when `a` and `b` are equal under Unicode lowercase mapping.
///
/// Compares lowercase iterations of each character, which yields the same
/// result as `c.to_lowercase()` would when collected into a `String` but
/// without allocating. Used by camel-hump matching where we need to test
/// one query character against one name character.
fn chars_equal_ignore_case(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Quality of a camel-case hump match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CamelMatch {
    /// All humps matched in order, starting at hump 0, with no gaps.
    ContiguousFromStart,
    /// All humps matched in order, with no gaps, but starting mid-name.
    Contiguous,
    /// All humps matched in order; one or more humps were skipped.
    Anywhere,
}

/// Try to match `query` against the camel-case humps of `name`.
///
/// Each character of the query (taken left-to-right, case-insensitive)
/// must align with the *first* character of a hump in the name, in
/// ascending hump order. A "hump" is a position in the name that begins
/// a logical word: the first character, a character preceded by a
/// non-alphanumeric separator, an upper-case character preceded by a
/// lower-case character, or a digit preceded by a non-digit.
fn camel_case_match(name: &str, query: &str) -> Option<CamelMatch> {
    let humps = hump_positions(name);
    if humps.is_empty() {
        return None;
    }

    let name_chars: Vec<char> = name.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();

    if query_chars.len() > humps.len() {
        return None;
    }

    let mut next_hump = 0usize;
    let mut first_hump: Option<usize> = None;
    let mut last_hump: Option<usize> = None;
    let mut contiguous = true;

    for &qc in &query_chars {
        let mut found: Option<usize> = None;
        for (hi, &hump_pos) in humps.iter().enumerate().skip(next_hump) {
            if chars_equal_ignore_case(name_chars[hump_pos], qc) {
                found = Some(hi);
                break;
            }
        }
        let hi = found?;
        if let Some(prev) = last_hump
            && hi != prev + 1
        {
            contiguous = false;
        }
        first_hump.get_or_insert(hi);
        last_hump = Some(hi);
        next_hump = hi + 1;
    }

    let first = first_hump?;
    Some(match (first == 0, contiguous) {
        (true, true) => CamelMatch::ContiguousFromStart,
        (_, true) => CamelMatch::Contiguous,
        _ => CamelMatch::Anywhere,
    })
}

/// Compute the positions of camel-case hump starts in `name`.
///
/// A hump start is the first character of a logical word:
/// - index 0 always counts (when alphanumeric),
/// - any alphanumeric character preceded by a non-alphanumeric separator,
/// - an upper-case character preceded by a lower-case character,
/// - a digit preceded by a non-digit, or
/// - a letter preceded by a digit.
///
/// Case detection is Unicode-aware so identifiers like `Ångström` or
/// `Δlpha` expose humps the same way as ASCII names. `_` and `$` are
/// separators rather than humps, so `_privateHelper` exposes humps for
/// `p` and `H` (not `_`).
fn hump_positions(name: &str) -> Vec<usize> {
    let chars: Vec<char> = name.chars().collect();
    let mut humps = Vec::new();
    let mut prev: Option<char> = None;
    for (i, &c) in chars.iter().enumerate() {
        let is_sep = !c.is_alphanumeric();
        let starts_word = match prev {
            None => !is_sep,
            Some(p) if !p.is_alphanumeric() => !is_sep,
            Some(p) if p.is_lowercase() && c.is_uppercase() => true,
            Some(p) if !p.is_numeric() && c.is_numeric() => true,
            Some(p) if p.is_numeric() && c.is_alphabetic() => true,
            _ => false,
        };
        if starts_word {
            humps.push(i);
        }
        prev = Some(c);
    }
    humps
}

/// Distance between two file paths, measured as the number of directory
/// hops between them. Same file is 0; sibling files in the same parent
/// directory are 1; each additional differing directory segment adds
/// another hop. Files in unrelated trees return a finite distance,
/// capped at `u32::MAX - 1` for pathologically long paths.
///
/// Common prefix is computed over *directory* segments only (the file
/// name itself is excluded). That avoids underflow when one path is a
/// segment-prefix of the other (e.g. active `src/index.ts` and
/// candidate `src/index.ts/generated/widget.ts`, an unusual but valid
/// shape under TS extension-stripping).
fn path_distance(active: &str, candidate: &str) -> u32 {
    if active == candidate {
        return 0;
    }
    let a: Vec<&str> = path_segments(active).collect();
    let b: Vec<&str> = path_segments(candidate).collect();
    let a_dirs = a.len().saturating_sub(1);
    let b_dirs = b.len().saturating_sub(1);
    let common = a
        .iter()
        .take(a_dirs)
        .zip(b.iter().take(b_dirs))
        .take_while(|(x, y)| x == y)
        .count();
    // `common <= min(a_dirs, b_dirs)`, so the subtraction can't underflow.
    let hops = (a_dirs + b_dirs).saturating_sub(2 * common);
    u32::try_from(hops + 1).unwrap_or(u32::MAX - 1)
}

fn path_segments(p: &str) -> impl Iterator<Item = &str> {
    p.split(['/', '\\']).filter(|s| !s.is_empty())
}

#[cfg(test)]
#[path = "../../tests/workspace_symbols_tests.rs"]
mod workspace_symbols_tests;
