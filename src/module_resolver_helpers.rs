//! Module resolution helper functions and types.
//!
//! Pure helper functions for package.json parsing, path manipulation,
//! semver comparison, and pattern matching used by the module resolver.

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

pub(crate) fn parse_package_specifier(specifier: &str) -> (String, Option<String>) {
    // Handle scoped packages (@scope/pkg)
    if let Some(without_at) = specifier.strip_prefix('@') {
        if let Some(scope_sep) = without_at.find('/') {
            let scope = &without_at[..scope_sep];
            let rest = &without_at[scope_sep + 1..];

            if let Some(sub_sep) = rest.find('/') {
                return (
                    format!("@{}/{}", scope, &rest[..sub_sep]),
                    Some(rest[sub_sep + 1..].to_string()),
                );
            }
            return (specifier.to_string(), None);
        }
        return (specifier.to_string(), None);
    }

    // Handle regular packages
    if let Some(slash_idx) = specifier.find('/') {
        (
            specifier[..slash_idx].to_string(),
            Some(specifier[slash_idx + 1..].to_string()),
        )
    } else {
        (specifier.to_string(), None)
    }
}

/// Convert a package name to its @types equivalent.
/// For scoped packages like `@see/saw`, this produces `@types/see__saw`.
/// For regular packages like `foo`, this produces `@types/foo`.
pub(crate) fn types_package_name(package_name: &str) -> String {
    let stripped = package_name.strip_prefix('@').unwrap_or(package_name);
    format!("@types/{}", stripped.replace('/', "__"))
}

/// Match an export pattern against a subpath
pub(crate) fn match_export_pattern(pattern: &str, subpath: &str) -> Option<String> {
    if !pattern.contains('*') {
        return (pattern == subpath).then(String::new);
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() != 2 {
        return None;
    }

    let prefix = parts[0];
    let suffix = parts[1];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());

    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

/// Match an imports pattern against a specifier (#-prefixed)
pub(crate) fn match_imports_pattern(pattern: &str, specifier: &str) -> Option<String> {
    if !pattern.contains('*') {
        return (pattern == specifier).then(String::new);
    }

    // Strip # prefix for matching
    let pattern = pattern.strip_prefix('#').unwrap_or(pattern);
    let specifier = specifier.strip_prefix('#').unwrap_or(specifier);

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() != 2 {
        return None;
    }

    let prefix = parts[0];
    let suffix = parts[1];

    if !specifier.starts_with(prefix) || !specifier.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = specifier.len().saturating_sub(suffix.len());

    if end < start {
        return None;
    }

    Some(specifier[start..end].to_string())
}

/// Match a typesVersions pattern against a subpath
pub(crate) fn match_types_versions_pattern(pattern: &str, subpath: &str) -> Option<String> {
    if !pattern.contains('*') {
        return (pattern == subpath).then(String::new);
    }

    let star_pos = pattern.find('*')?;
    let (prefix, suffix) = pattern.split_at(star_pos);
    let suffix = &suffix[1..]; // Skip the '*'

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());

    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

pub(crate) fn types_versions_compiler_version(value: Option<&str>) -> SemVer {
    value
        .and_then(parse_semver)
        .unwrap_or_else(default_types_versions_compiler_version)
}

pub(crate) const fn default_types_versions_compiler_version() -> SemVer {
    TYPES_VERSIONS_COMPILER_VERSION_FALLBACK
}

pub(crate) fn select_types_versions_paths(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    select_types_versions_paths_for_version(types_versions, compiler_version)
}

pub(crate) fn select_types_versions_paths_for_version(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let map = types_versions.as_object()?;
    let mut best_score: Option<RangeScore> = None;
    let mut best_key: Option<&str> = None;
    let mut best_value: Option<&serde_json::Map<String, serde_json::Value>> = None;

    for (key, value) in map {
        let Some(value_map) = value.as_object() else {
            continue;
        };
        let Some(score) = match_types_versions_range(key, compiler_version) else {
            continue;
        };
        let is_better = match best_score {
            None => true,
            Some(best) => {
                score > best
                    || (score == best && best_key.is_none_or(|best_key| key.as_str() < best_key))
            }
        };

        if is_better {
            best_score = Some(score);
            best_key = Some(key);
            best_value = Some(value_map);
        }
    }

    best_value
}

pub(crate) fn types_versions_specificity(pattern: &str) -> usize {
    if let Some(star) = pattern.find('*') {
        star + (pattern.len() - star - 1)
    } else {
        pattern.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct RangeScore {
    constraints: usize,
    min_version: SemVer,
    key_len: usize,
}

pub(crate) fn match_types_versions_range(
    range: &str,
    compiler_version: SemVer,
) -> Option<RangeScore> {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len: range.len(),
        });
    }

    let mut best: Option<RangeScore> = None;
    for segment in range.split("||") {
        let segment = segment.trim();
        let Some(score) =
            match_types_versions_range_segment(segment, compiler_version, range.len())
        else {
            continue;
        };
        if best.is_none_or(|current| score > current) {
            best = Some(score);
        }
    }

    best
}

pub(crate) fn match_types_versions_range_segment(
    segment: &str,
    compiler_version: SemVer,
    key_len: usize,
) -> Option<RangeScore> {
    if segment.is_empty() {
        return None;
    }
    if segment == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len,
        });
    }

    let mut min_version = SemVer::ZERO;
    let mut constraints = 0usize;

    for token in segment.split_whitespace() {
        if token.is_empty() || token == "*" {
            continue;
        }
        let (op, version) = parse_range_token(token)?;
        if !compare_range(compiler_version, op, version) {
            return None;
        }
        constraints += 1;
        if matches!(op, RangeOp::Gt | RangeOp::Gte | RangeOp::Eq) && version > min_version {
            min_version = version;
        }
    }

    Some(RangeScore {
        constraints,
        min_version,
        key_len,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RangeOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

pub(crate) fn parse_range_token(token: &str) -> Option<(RangeOp, SemVer)> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    let (op, rest) = if let Some(rest) = token.strip_prefix(">=") {
        (RangeOp::Gte, rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        (RangeOp::Lte, rest)
    } else if let Some(rest) = token.strip_prefix('>') {
        (RangeOp::Gt, rest)
    } else if let Some(rest) = token.strip_prefix('<') {
        (RangeOp::Lt, rest)
    } else {
        (RangeOp::Eq, token)
    };

    parse_semver(rest).map(|version| (op, version))
}

pub(crate) fn compare_range(version: SemVer, op: RangeOp, other: SemVer) -> bool {
    match op {
        RangeOp::Gt => version > other,
        RangeOp::Gte => version >= other,
        RangeOp::Lt => version < other,
        RangeOp::Lte => version <= other,
        RangeOp::Eq => version == other,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
}

impl SemVer {
    const ZERO: Self = Self {
        major: 0,
        minor: 0,
        patch: 0,
    };
}

// NOTE: Keep this in sync with the TypeScript version this compiler targets.
pub(crate) const TYPES_VERSIONS_COMPILER_VERSION_FALLBACK: SemVer = SemVer {
    major: 6,
    minor: 0,
    patch: 0,
};

pub(crate) fn parse_semver(value: &str) -> Option<SemVer> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some(SemVer {
        major,
        minor,
        patch,
    })
}

/// Apply wildcard substitution to a target path
pub(crate) fn apply_wildcard_substitution(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

pub(crate) fn split_path_extension(path: &Path) -> Option<(PathBuf, &'static str)> {
    let path_str = path.to_string_lossy();
    for ext in KNOWN_EXTENSIONS {
        if path_str.ends_with(ext) {
            let base = &path_str[..path_str.len().saturating_sub(ext.len())];
            if base.is_empty() {
                return None;
            }
            return Some((PathBuf::from(base), ext.trim_start_matches('.')));
        }
    }
    None
}

pub(crate) fn try_file_with_suffixes(path: &Path, suffixes: &[String]) -> Option<PathBuf> {
    let (base, extension) = split_path_extension(path)?;
    try_file_with_suffixes_and_extension(&base, extension, suffixes)
}

pub(crate) fn try_file_with_suffixes_and_extension(
    base: &Path,
    extension: &str,
    suffixes: &[String],
) -> Option<PathBuf> {
    for suffix in suffixes {
        let Some(candidate) = path_with_suffix_and_extension(base, suffix, extension) else {
            continue;
        };
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn path_with_suffix_and_extension(
    base: &Path,
    suffix: &str,
    extension: &str,
) -> Option<PathBuf> {
    let file_name = base.file_name()?.to_string_lossy();
    let mut candidate = base.to_path_buf();
    let mut new_name = String::with_capacity(file_name.len() + suffix.len() + extension.len() + 1);
    new_name.push_str(&file_name);
    new_name.push_str(suffix);
    new_name.push('.');
    new_name.push_str(extension);
    candidate.set_file_name(new_name);
    Some(candidate)
}

pub(crate) fn try_arbitrary_extension_declaration(path: &Path, extension: &str) -> Option<PathBuf> {
    let declaration = path.with_extension(format!("d.{extension}.ts"));
    if declaration.is_file() {
        return Some(declaration);
    }
    None
}

pub(crate) fn resolve_explicit_unknown_extension(path: &Path) -> Option<PathBuf> {
    path.extension()?;
    if split_path_extension(path).is_some() {
        return None;
    }
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    None
}

pub(crate) const KNOWN_EXTENSIONS: [&str; 12] = [
    ".d.mts", ".d.cts", ".d.ts", ".mts", ".cts", ".tsx", ".ts", ".mjs", ".cjs", ".jsx", ".js",
    ".json",
];
pub(crate) const TS_EXTENSION_CANDIDATES: [&str; 7] =
    ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
pub(crate) const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
pub(crate) const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];
pub(crate) const CLASSIC_EXTENSION_CANDIDATES: [&str; 7] = TS_EXTENSION_CANDIDATES;

/// Extension candidates when allowJs is enabled (TypeScript + JavaScript)
pub(crate) const TS_JS_EXTENSION_CANDIDATES: [&str; 11] = [
    "ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts", "js", "jsx", "mjs", "cjs",
];

pub(crate) fn node16_extension_substitution(path: &Path, extension: &str) -> Option<Vec<PathBuf>> {
    let replacements: &[&str] = match extension {
        "js" => &["ts", "tsx", "d.ts"],
        "jsx" => &["tsx", "d.ts"],
        "mjs" => &["mts", "d.mts"],
        "cjs" => &["cts", "d.cts"],
        _ => return None,
    };

    Some(
        replacements
            .iter()
            .map(|ext| path.with_extension(ext))
            .collect(),
    )
}

pub(crate) fn declaration_substitution_for_main(path: &Path) -> Option<PathBuf> {
    let extension = path.extension().and_then(|ext| ext.to_str())?;
    match extension {
        "js" | "jsx" => Some(path.with_extension("d.ts")),
        "mjs" => Some(path.with_extension("d.mts")),
        "cjs" => Some(path.with_extension("d.cts")),
        _ => None,
    }
}

/// Simplified package.json structure for resolution
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Fields deserialized from JSON; not all read directly
pub(crate) struct PackageJson {
    pub name: Option<String>,
    pub version: Option<String>,
    pub main: Option<String>,
    pub module: Option<String>,
    pub types: Option<String>,
    pub typings: Option<String>,
    #[serde(rename = "type")]
    pub package_type: Option<String>,
    pub exports: Option<PackageExports>,
    pub imports: Option<FxHashMap<String, PackageExports>>,
    /// TypeScript typesVersions field for version-specific type definitions
    #[serde(rename = "typesVersions")]
    pub types_versions: Option<serde_json::Value>,
}

/// Package exports field can be a string, map, or conditional
///
/// Map variant: keys start with "." (subpath patterns like ".", "./foo")
/// Conditional variant: keys don't start with "." (condition names like "import", "default")
///   Uses Vec to preserve JSON key order (required for correct condition matching)
#[derive(Debug, Clone)]
pub(crate) enum PackageExports {
    String(String),
    Map(FxHashMap<String, Self>),
    Conditional(Vec<(String, Self)>),
    /// null in JSON — indicates an explicitly blocked export
    Null,
}

impl<'de> serde::Deserialize<'de> for PackageExports {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct PackageExportsVisitor;

        impl<'de> de::Visitor<'de> for PackageExportsVisitor {
            type Value = PackageExports;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string, object, or null")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::String(v.to_string()))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::Null)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::Null)
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut map_entries = FxHashMap::default();
                let mut cond_entries = Vec::new();
                let mut is_subpath_map = None;

                while let Some((key, value)) = map.next_entry::<String, PackageExports>()? {
                    if is_subpath_map.is_none() {
                        is_subpath_map = Some(key.starts_with('.'));
                    }
                    if is_subpath_map == Some(true) {
                        map_entries.insert(key, value);
                    } else {
                        cond_entries.push((key, value));
                    }
                }

                if is_subpath_map.unwrap_or(false) {
                    Ok(PackageExports::Map(map_entries))
                } else {
                    Ok(PackageExports::Conditional(cond_entries))
                }
            }
        }

        deserializer.deserialize_any(PackageExportsVisitor)
    }
}
