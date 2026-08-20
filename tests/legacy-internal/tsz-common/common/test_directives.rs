//! Canonical parser for TypeScript test-file directives (`// @option: value`).
//!
//! TypeScript's test harness configures each test file through comment
//! directives such as `// @strict: true`, `// @lib: es5,dom`, or
//! `// @filename: a.ts`. Historically tsz parsed these in several
//! independent places (conformance harness, emit harness, fourslash
//! harness, checker test pragmas, support scripts) with subtly different
//! whitespace, casing, and multi-value semantics — an active source of
//! cross-harness drift (see issue #13127). This module is the single
//! canonical grammar; every harness routes its directive recognition
//! through it.
//!
//! Canonical grammar (derived from the TypeScript harness recognizer
//! `^//\s*@(\w+)\s*:\s*([^\r\n]*)`, extended with the leading-whitespace
//! tolerance the tsz harnesses have always applied):
//!
//! - Key/value directive: optional leading whitespace, `//`, optional
//!   whitespace, `@`, an ASCII `[A-Za-z0-9_]+` key, optional whitespace,
//!   `:`, then the value (rest of line). Keys are case-insensitive;
//!   values are surrounding-whitespace trimmed.
//! - Flag directive (no colon): optional leading whitespace, `//`,
//!   optional whitespace, `@`, an ASCII `[A-Za-z0-9_-]+` name, optional
//!   trailing whitespace, end of line. This is the `// @ts-check` /
//!   `// @ts-nocheck` family.
//! - List-valued options (`@lib`, `@symlink`) split on commas; each
//!   element is trimmed and empties are dropped.
//! - Variant-valued scalar options (`@target: es5,es2015`) take the
//!   first comma-separated value in single-variant harness runs.
//!
//! The full-file splitter [`parse_test_file`] reproduces the conformance
//! harness semantics exactly: the checked-in tsc result cache was
//! generated through it, so its behavior is load-bearing for cache
//! identity (option keys lowercased, last duplicate wins, first-seen
//! option order preserved, directive lines removed from file content,
//! `@ts-check`/`@ts-nocheck` mapped to `checkjs` and kept as content
//! inside `@filename` sections).

use std::collections::HashMap;

/// A recognized `// @key: value` directive line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectiveLine<'a> {
    /// Directive key as written (original casing).
    pub key: &'a str,
    /// Value with surrounding whitespace trimmed.
    pub value: &'a str,
    /// Untrimmed value: every byte after the `:`.
    pub raw_value: &'a str,
    /// Byte length of the line prefix through the `:` (leading
    /// whitespace, `//`, `@`, key, and colon). `line[..prefix_len]`
    /// followed by `raw_value` reconstructs the line.
    pub prefix_len: usize,
}

impl DirectiveLine<'_> {
    /// Case-insensitive key comparison against a lowercase option name.
    pub const fn key_is(&self, lower_name: &str) -> bool {
        self.key.eq_ignore_ascii_case(lower_name)
    }

    /// Key normalized to lowercase (allocates).
    pub fn key_lower(&self) -> String {
        self.key.to_ascii_lowercase()
    }
}

const fn is_directive_key_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

const fn is_flag_name_byte(b: u8) -> bool {
    is_directive_key_byte(b) || b == b'-'
}

/// Consume `//` plus surrounding whitespace and the `@` sigil; return the
/// byte offset where the directive name starts, or `None` if `line` does
/// not open a comment directive.
fn directive_name_start(line: &str) -> Option<usize> {
    let rest = line.trim_start().strip_prefix("//")?;
    let body = rest.trim_start().strip_prefix('@')?;
    Some(line.len() - body.len())
}

/// Recognize a `// @key: value` directive line.
///
/// Returns `None` for flag-form directives (no colon), triple-slash
/// references, and ordinary content lines.
pub fn parse_directive_line(line: &str) -> Option<DirectiveLine<'_>> {
    let name_start = directive_name_start(line)?;
    let bytes = line.as_bytes();
    let mut i = name_start;
    while i < bytes.len() && is_directive_key_byte(bytes[i]) {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let key = &line[name_start..i];
    let after_key = line[i..].trim_start();
    let rest = after_key.strip_prefix(':')?;
    Some(DirectiveLine {
        key,
        value: rest.trim(),
        raw_value: rest,
        prefix_len: line.len() - rest.len(),
    })
}

/// Recognize a flag-form directive line: `// @name` with no colon and
/// nothing but whitespace after the name (e.g. `// @ts-check`).
///
/// Returns the name as written (original casing).
pub fn parse_flag_directive_line(line: &str) -> Option<&str> {
    let name_start = directive_name_start(line)?;
    let bytes = line.as_bytes();
    let mut i = name_start;
    while i < bytes.len() && is_flag_name_byte(bytes[i]) {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let name = &line[name_start..i];
    line[i..].trim().is_empty().then_some(name)
}

/// Split a list-valued directive value (`@lib: es5,dom`) into its
/// elements: comma-separated, trimmed, empties dropped.
pub fn split_list_values(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|s| !s.is_empty())
}

/// First comma-separated value of a variant-valued scalar directive
/// (`@target: es5,es2015` -> `es5`), trimmed. Single-variant harness
/// runs use the first variant, matching the conformance cache generator.
pub fn first_list_value(value: &str) -> &str {
    value.split(',').next().unwrap_or("").trim()
}

/// Parse a boolean directive value. Multi-variant values are clipped at
/// the first `,` or `;` before matching `true`/`false`, mirroring the
/// first-variant rule of [`first_list_value`].
pub fn parse_bool_value(value: &str) -> Option<bool> {
    let end = value.find([',', ';']).unwrap_or(value.len());
    match value[..end].trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

struct TestFileLines<'a> {
    rest: &'a str,
}

impl<'a> Iterator for TestFileLines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }

        let bytes = self.rest.as_bytes();
        for (idx, byte) in bytes.iter().enumerate() {
            if *byte != b'\r' && *byte != b'\n' {
                continue;
            }

            let line = &self.rest[..idx];
            let next_idx = if *byte == b'\r' && bytes.get(idx + 1) == Some(&b'\n') {
                idx + 2
            } else {
                idx + 1
            };
            self.rest = &self.rest[next_idx..];
            return Some(line);
        }

        let line = self.rest;
        self.rest = "";
        Some(line)
    }
}

/// Iterate test-file lines while accepting every newline spelling the
/// TypeScript corpus uses (`\n`, `\r\n`, and CR-only legacy fixtures).
const fn test_file_lines(content: &str) -> TestFileLines<'_> {
    TestFileLines { rest: content }
}

/// Directives parsed from a whole test file.
#[derive(Debug, Default, Clone)]
pub struct TestDirectives {
    /// Compiler options keyed by lowercased directive name. Duplicate
    /// directives keep the last value.
    pub options: HashMap<String, String>,
    /// First-seen order of option keys (used to generate tsconfig.json
    /// with stable key order across harnesses).
    pub option_order: Vec<String>,
    /// Files declared by `@filename` directives, in declaration order.
    pub filenames: Vec<(String, String)>,
}

/// Split a test file into directives and `@filename` sections.
///
/// Semantics (cache-anchored — see module docs):
/// - A UTF-8 BOM at the start of the content is stripped before parsing.
/// - Every `// @key: value` line anywhere in the file is consumed as an
///   option (never kept as file content); `@filename` starts a new file
///   section.
/// - Flag-form lines: `@ts-check`/`@ts-nocheck` map to `checkjs`
///   true/false and are kept as content inside `@filename` sections
///   (they are real source comments tsc preserves); all other flag-form
///   lines are dropped.
/// - All other lines belong to the current `@filename` section, or to no
///   file when no `@filename` directive has been seen.
pub fn parse_test_file(content: &str) -> TestDirectives {
    let mut directives = TestDirectives::default();
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);

    let mut current_filename: Option<String> = None;
    let mut current_content: Vec<&str> = Vec::new();

    let record_option = |directives: &mut TestDirectives, key: String, value: &str| {
        let value = value.strip_suffix(';').unwrap_or(value).trim_end();
        if !directives.options.contains_key(&key) {
            directives.option_order.push(key.clone());
        }
        directives.options.insert(key, value.to_string());
    };

    for line in test_file_lines(content) {
        if let Some(directive) = parse_directive_line(line) {
            if directive.key_is("filename") {
                if let Some(filename) = current_filename.take() {
                    directives
                        .filenames
                        .push((filename, current_content.join("\n")));
                }
                current_content.clear();
                current_filename = Some(directive.value.to_string());
            } else {
                record_option(&mut directives, directive.key_lower(), directive.value);
            }
        } else if let Some(flag) = parse_flag_directive_line(line) {
            let value = if flag.eq_ignore_ascii_case("ts-check") {
                "true"
            } else if flag.eq_ignore_ascii_case("ts-nocheck") {
                "false"
            } else {
                continue;
            };
            record_option(&mut directives, "checkjs".to_string(), value);
            if current_filename.is_some() {
                current_content.push(line);
            }
        } else {
            current_content.push(line);
        }
    }

    if let Some(filename) = current_filename {
        directives
            .filenames
            .push((filename, current_content.join("\n")));
    }

    directives
}

#[cfg(test)]
mod tests;
