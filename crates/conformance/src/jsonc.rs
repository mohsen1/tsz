//! Minimal JSONC parsing shared by conformance classification and fixture setup.
//!
//! TypeScript configuration files permit comments and trailing commas. The
//! conformance harness must therefore never fall back to an empty object when
//! strict JSON parsing rejects otherwise valid authored configuration.

use anyhow::Context;

pub fn parse_jsonc(source: &str) -> anyhow::Result<serde_json::Value> {
    let uncommented = strip_comments(source.trim_start_matches('\u{feff}'))?;
    let normalized = strip_trailing_commas(&uncommented);
    serde_json::from_str(&normalized).context("invalid JSONC")
}

fn strip_comments(input: &str) -> anyhow::Result<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut in_string = false;

    while index < bytes.len() {
        if in_string {
            output.push(bytes[index]);
            if bytes[index] == b'\\' && index + 1 < bytes.len() {
                index += 1;
                output.push(bytes[index]);
            } else if bytes[index] == b'"' {
                in_string = false;
            }
            index += 1;
        } else if bytes[index] == b'"' {
            in_string = true;
            output.push(bytes[index]);
            index += 1;
        } else if bytes.get(index..index + 2) == Some(b"//") {
            while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                output.push(b' ');
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            output.extend_from_slice(b"  ");
            index += 2;
            while index < bytes.len() && bytes.get(index..index + 2) != Some(b"*/") {
                output.push(if matches!(bytes[index], b'\n' | b'\r') {
                    bytes[index]
                } else {
                    b' '
                });
                index += 1;
            }
            if index >= bytes.len() {
                anyhow::bail!("unterminated JSONC block comment");
            }
            output.extend_from_slice(b"  ");
            index += 2;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }

    Ok(String::from_utf8(output).expect("JSONC source began as UTF-8"))
}

fn strip_trailing_commas(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut in_string = false;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            in_string = !in_string;
            output.push(byte);
            index += 1;
        } else if in_string && byte == b'\\' && index + 1 < bytes.len() {
            output.push(byte);
            index += 1;
            output.push(bytes[index]);
            index += 1;
        } else if !in_string && byte == b',' {
            let mut lookahead = index + 1;
            while bytes.get(lookahead).is_some_and(u8::is_ascii_whitespace) {
                lookahead += 1;
            }
            if matches!(bytes.get(lookahead), Some(b'}' | b']')) {
                output.push(b' ');
                index += 1;
            } else {
                output.push(byte);
                index += 1;
            }
        } else {
            output.push(byte);
            index += 1;
        }
    }

    String::from_utf8(output).expect("JSONC source began as UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_trailing_commas_and_string_like_comments() {
        let value = parse_jsonc(
            r#"{
                // line comment
                "compilerOptions": {
                    "traceResolution": true,
                    "url": "https://example.test/*literal*/",
                },
                /* block comment */
            }"#,
        )
        .expect("valid JSONC");

        assert_eq!(value["compilerOptions"]["traceResolution"], true);
        assert_eq!(
            value["compilerOptions"]["url"],
            "https://example.test/*literal*/"
        );
    }

    #[test]
    fn malformed_jsonc_is_an_error() {
        assert!(parse_jsonc(r#"{"compilerOptions": {"#).is_err());
        assert!(parse_jsonc(r#"{"compilerOptions": {}} /*"#).is_err());
    }
}
