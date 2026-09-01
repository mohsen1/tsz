pub(crate) fn quote_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"'); // TypeScript `escapeString`, double-quote mode.
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\0' {
            let digit = characters.peek().is_some_and(char::is_ascii_digit);
            quoted.push_str(if digit { "\\x00" } else { "\\0" });
        } else if let Some(index) = "\u{8}\t\n\u{b}\u{c}\r\\\"".find(character) {
            quoted.extend(['\\', char::from(b"btnvfr\\\""[index])]);
        } else if character <= '\u{1f}' || "\u{85}\u{2028}\u{2029}".contains(character) {
            use std::fmt::Write;
            write!(quoted, "\\u{:04X}", u32::from(character)).expect("String write cannot fail");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('"');
    quoted
}
