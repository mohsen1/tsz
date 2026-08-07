/// Validate JSON syntax and return parse diagnostics for violations.
///
/// TypeScript's JSON parser enforces strict JSON rules when parsing `.json` files.
/// This validates property names must be double-quoted string literals (TS1327),
/// and that every property value and array element is one of tsc's
/// `validateJsonValue` shapes: a double-quoted string, a numeric literal
/// (optionally `-`-prefixed), `true`/`false`/`null`, or a nested object/array
/// (TS1328). Violations include single-quoted strings, computed property
/// names (`[expr]`), and unquoted identifiers used as a property name or
/// value.
fn validate_json_syntax(source: &str) -> Vec<ParseDiagnostic> {
    let mut diagnostics = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    let is_ident_start = |b: u8| b.is_ascii_alphabetic() || b == b'_' || b == b'$';
    let is_ident_part = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';

    // Root-level recovery for invalid bare identifier runs in JSON files.
    // tsc emits a specific sequence:
    //   first identifier  -> TS1005 "'{' expected." + TS1136
    //   next identifiers  -> TS1005 "',' expected." + TS1136
    //   end of run        -> TS1005 "'}' expected."
    //
    // Valid JSON roots `true` / `false` / `null` are explicitly allowed.
    // Leading trivia (whitespace and comments) is skipped first, so a bare
    // identifier root preceded by a `// header` comment still triggers the
    // recovery rather than being masked by the unrecognized `/`.
    let j = skip_json_trivia(bytes, 0, len);
    if j < len && is_ident_start(bytes[j]) {
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut k = j;
        loop {
            let start = k;
            while k < len && is_ident_part(bytes[k]) {
                k += 1;
            }
            spans.push((start, k));

            // Trivia (whitespace and comments alike) separates identifiers in
            // this recovery run, matching how tsc's scanner tokenizes the same
            // bytes — so `foo /*c*/ bar` recovers identically to `foo bar`.
            k = skip_json_trivia(bytes, k, len);

            if k < len && is_ident_start(bytes[k]) {
                continue;
            }
            break;
        }

        if k >= len {
            let single_keyword_root = spans.len() == 1
                && std::str::from_utf8(&bytes[spans[0].0..spans[0].1])
                    .map(|s| matches!(s, "true" | "false" | "null"))
                    .unwrap_or(false);

            if !single_keyword_root {
                for (idx, (start, _end)) in spans.iter().enumerate() {
                    let expected_msg = if idx == 0 {
                        "'{' expected."
                    } else {
                        "',' expected."
                    };
                    diagnostics.push(ParseDiagnostic {
                        start: *start as u32,
                        length: 1,
                        message: expected_msg.to_string(),
                        code: tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                        related: None,
                    });
                    diagnostics.push(ParseDiagnostic {
                        start: *start as u32,
                        length: 1,
                        message: tsz_common::diagnostics::diagnostic_messages::PROPERTY_ASSIGNMENT_EXPECTED.to_string(),
                        code: tsz_common::diagnostics::diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
                        related: None,
                    });
                }
                if let Some((_, end)) = spans.last() {
                    diagnostics.push(ParseDiagnostic {
                        start: *end as u32,
                        length: 1,
                        message: "'}' expected.".to_string(),
                        code: tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                        related: None,
                    });
                }
            }
        }
    }

    // Track whether we're inside an object and expecting a property name.
    // JSON property names must be double-quoted strings per the JSON spec.
    // We use a simple state machine: after `{` or `,` inside an object,
    // the next non-whitespace token must be `"` (property name) or `}` (end).
    let mut object_depth: i32 = 0;
    let mut array_depth: i32 = 0;
    let mut expecting_property_name = false;
    let mut expecting_value = false;

    while i < len {
        // Skip whitespace and JSON comment trivia (`//` line and `/* */`
        // block). tsc's JSON scanner treats both as trivia. This runs before
        // value/property classification below so a comment sitting between a
        // property's `:` and its value is skipped rather than mis-reported as
        // TS1328. Strings are consumed atomically by the string-skip block
        // further down, so a `//` or `/* */` *inside* a string never reaches
        // this point and stays part of the value.
        let after_trivia = skip_json_trivia(bytes, i, len);
        if after_trivia != i {
            i = after_trivia;
            continue;
        }

        let b = bytes[i];

        // When expecting a property value (just past a property's `:`) or an
        // array element (just past `[` or a `,` inside an array), check what
        // the value starts with. Valid starts fall through to the normal
        // handling below (e.g. `{`/`[` still open a nested object/array, `"`
        // is still skipped as a string); this block only decides validity and
        // reports TS1328/TS1327 for the ones that are not.
        if expecting_value && (object_depth > 0 || array_depth > 0) {
            let is_valid_value_start = match b {
                // `"`/`{`/`[` are real value starts. `}`/`]`/`,` are not a
                // value themselves, but an empty array/object or a recovery
                // point for a missing element/value; leave those to the
                // dedicated structural handling below rather than guessing
                // at a diagnostic here.
                b'"' | b'{' | b'[' | b'}' | b']' | b',' | b'0'..=b'9' => true,
                b'-' => i + 1 < len && bytes[i + 1].is_ascii_digit(),
                b't' => matches_json_keyword(bytes, i, len, b"true", is_ident_part),
                b'f' => matches_json_keyword(bytes, i, len, b"false", is_ident_part),
                b'n' => matches_json_keyword(bytes, i, len, b"null", is_ident_part),
                b'\'' => {
                    // Single-quoted value: same quote-style diagnostic as a
                    // single-quoted property name, not TS1328.
                    diagnostics.push(ParseDiagnostic {
                        start: i as u32,
                        length: 1,
                        message: tsz_common::diagnostics::diagnostic_messages::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED.to_string(),
                        code: tsz_common::diagnostics::diagnostic_codes::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED,
                        related: None,
                    });
                    true
                }
                _ => false,
            };
            if !is_valid_value_start {
                diagnostics.push(ParseDiagnostic {
                    start: i as u32,
                    length: 1,
                    message: tsz_common::diagnostics::diagnostic_messages::PROPERTY_VALUE_CAN_ONLY_BE_STRING_LITERAL_NUMERIC_LITERAL_TRUE_FALSE_NULL_OBJECT.to_string(),
                    code: tsz_common::diagnostics::diagnostic_codes::PROPERTY_VALUE_CAN_ONLY_BE_STRING_LITERAL_NUMERIC_LITERAL_TRUE_FALSE_NULL_OBJECT,
                    related: None,
                });
            }
            expecting_value = false;
        }

        if b == b'{' {
            object_depth += 1;
            expecting_property_name = true;
            i += 1;
            continue;
        }

        if b == b'}' {
            object_depth -= 1;
            expecting_property_name = false;
            i += 1;
            continue;
        }

        if b == b'[' && !expecting_property_name {
            array_depth += 1;
            expecting_value = true;
            i += 1;
            continue;
        }

        if b == b']' && array_depth > 0 {
            array_depth -= 1;
            i += 1;
            continue;
        }

        if b == b',' {
            // After a comma, the innermost open container decides what comes
            // next: a property name for an object, another element for an
            // array.
            if object_depth > array_depth {
                expecting_property_name = true;
            } else if array_depth > 0 {
                expecting_value = true;
            }
            i += 1;
            continue;
        }

        if b == b':' {
            expecting_property_name = false;
            expecting_value = true;
            i += 1;
            continue;
        }

        // When expecting a property name, check what we got
        if expecting_property_name && object_depth > 0 {
            if b == b'"' {
                // Valid double-quoted property name - skip past the string
                expecting_property_name = false;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escape sequence
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                continue;
            }

            // Not a double-quoted string in property name position → TS1327
            diagnostics.push(ParseDiagnostic {
                start: i as u32,
                length: 1,
                message: tsz_common::diagnostics::diagnostic_messages::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED.to_string(),
                code: tsz_common::diagnostics::diagnostic_codes::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED,
                related: None,
            });
            expecting_property_name = false;
        }

        // Skip over strings (double-, single-, and back-quoted) to avoid their
        // contents being misread as structural tokens (braces, commas, colons).
        if b == b'"' || b == b'\'' || b == b'`' {
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            continue;
        }

        i += 1;
    }

    diagnostics
}

/// Advance `idx` past JSON trivia: ASCII whitespace and `//` line / `/* */`
/// block comments, which tsc's JSON scanner skips as trivia. Stops at the
/// first byte that begins neither. A `/` not followed by `/` or `*` is not a
/// comment and is left in place for the caller to classify. An unterminated
/// block comment consumes to end of input, matching the scanner treating the
/// rest of the file as comment.
///
/// Callers must only invoke this outside of string literals: a `//` or `/* */`
/// appearing inside a JSON string is ordinary string content, and the caller's
/// string handling is responsible for consuming the string whole before this
/// runs.
fn skip_json_trivia(bytes: &[u8], mut idx: usize, len: usize) -> usize {
    loop {
        idx = tsz_common::text_scan::skip_ascii_whitespace(bytes, idx);

        if idx + 1 < len && bytes[idx] == b'/' && bytes[idx + 1] == b'/' {
            idx += 2;
            while idx < len && bytes[idx] != b'\n' {
                idx += 1;
            }
            continue;
        }

        if idx + 1 < len && bytes[idx] == b'/' && bytes[idx + 1] == b'*' {
            idx += 2;
            while idx + 1 < len && !(bytes[idx] == b'*' && bytes[idx + 1] == b'/') {
                idx += 1;
            }
            // Consume the closing `*/`, or run to end of input if the block
            // comment is unterminated.
            idx = (idx + 2).min(len);
            continue;
        }

        break;
    }
    idx
}

/// Returns whether `bytes[i..]` starts with the exact keyword `word` (`true`,
/// `false`, or `null`), not just a longer identifier with `word` as a prefix
/// (e.g. `truest` must not match `true`).
fn matches_json_keyword(
    bytes: &[u8],
    i: usize,
    len: usize,
    word: &[u8],
    is_ident_part: impl Fn(u8) -> bool,
) -> bool {
    let end = i + word.len();
    end <= len && &bytes[i..end] == word && (end == len || !is_ident_part(bytes[end]))
}
