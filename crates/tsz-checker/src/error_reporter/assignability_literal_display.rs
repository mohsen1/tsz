pub(super) fn display_has_boolean_member_literal_assignability(display: &str) -> bool {
    let bytes = display.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    let mut quote = None;
    let mut escaped = false;
    for i in 0..(bytes.len() - 2) {
        let byte = bytes[i];
        if let Some(quote_byte) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == quote_byte {
                quote = None;
            }
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            continue;
        }
        if byte != b':' || bytes[i + 1] != b' ' {
            continue;
        }
        let rest = &display[i + 2..];
        if display_segment_starts_with_boolean_literal(rest) {
            return true;
        }
    }
    false
}

fn display_segment_starts_with_boolean_literal(segment: &str) -> bool {
    ["true", "false"].into_iter().any(|literal| {
        segment.strip_prefix(literal).is_some_and(|rest| {
            rest.bytes()
                .next()
                .is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_' && b != b'$')
        })
    })
}

#[cfg(test)]
mod tests {
    use super::display_has_boolean_member_literal_assignability;

    #[test]
    fn boolean_member_literal_display_scan_ignores_string_literal_contents() {
        assert!(display_has_boolean_member_literal_assignability(
            "{ c: true; }"
        ));
        assert!(display_has_boolean_member_literal_assignability(
            "{ c: false; }"
        ));
        assert!(!display_has_boolean_member_literal_assignability(
            r#"{ c: "foo: true"; }"#
        ));
        assert!(!display_has_boolean_member_literal_assignability(
            r#"{ c: 'foo: false'; }"#
        ));
        assert!(!display_has_boolean_member_literal_assignability(
            "{ c: trueish; }"
        ));
    }
}
