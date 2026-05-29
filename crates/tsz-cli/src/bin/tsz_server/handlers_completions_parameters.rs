//! Parameter-name recovery helpers for completion handlers.

use rustc_hash::FxHashSet;

pub(super) fn trailing_function_parameter_names_at_declaration_end(
    source_text: &str,
    offset: u32,
) -> FxHashSet<String> {
    let Some(params) = trailing_function_parameter_list_at_declaration_end(source_text, offset)
    else {
        return FxHashSet::default();
    };

    FunctionParameterList::new(params).parameter_names()
}

struct FunctionParameterList<'a> {
    text: &'a str,
}

impl<'a> FunctionParameterList<'a> {
    const fn new(text: &'a str) -> Self {
        Self { text }
    }

    fn parameter_names(&self) -> FxHashSet<String> {
        let mut out = FxHashSet::default();
        for segment in self.text.split(',') {
            if let Some(name) = parameter_name_from_segment(segment) {
                out.insert(name.to_string());
            }
        }
        out
    }
}

fn trailing_function_parameter_list_at_declaration_end(
    source_text: &str,
    offset: u32,
) -> Option<&str> {
    let end = (offset as usize).min(source_text.len());
    let trimmed = source_text[..end].trim_end();
    if !trimmed.ends_with('}') {
        return None;
    }

    let open = trailing_body_open_brace(trimmed)?;
    let before_body = &trimmed[..open];
    let function_kw = before_body.rfind("function")?;
    let after_kw = &before_body[function_kw + "function".len()..];
    let paren_rel = after_kw.find('(')?;
    let open_paren = function_kw + "function".len() + paren_rel;
    let close_rel = before_body[open_paren + 1..].find(')')?;
    let close_paren = open_paren + 1 + close_rel;
    Some(&before_body[open_paren + 1..close_paren])
}

fn trailing_body_open_brace(trimmed: &str) -> Option<usize> {
    let bytes = trimmed.as_bytes();
    let close = bytes.len().checked_sub(1)?;
    let mut depth = 0i32;
    let mut i = close + 1;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parameter_name_from_segment(segment: &str) -> Option<&str> {
    let mut part = segment.trim();
    if part.starts_with("...") {
        part = part[3..].trim_start();
    }
    let ident_end = part
        .find(|c: char| !(c == '_' || c == '$' || c.is_ascii_alphanumeric()))
        .unwrap_or(part.len());
    if ident_end == 0 {
        return None;
    }
    let ident = &part[..ident_end];
    ident
        .chars()
        .next()
        .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
        .then_some(ident)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source_text: &str) -> FxHashSet<String> {
        trailing_function_parameter_names_at_declaration_end(source_text, source_text.len() as u32)
    }

    #[test]
    fn extracts_identifiers_from_trailing_function_declaration() {
        let blocked = names("function handle(first: string, second = 1, ...rest: unknown[]) {}");

        assert!(blocked.contains("first"));
        assert!(blocked.contains("second"));
        assert!(blocked.contains("rest"));
        assert_eq!(blocked.len(), 3);
    }

    #[test]
    fn accepts_identifier_start_variants() {
        let blocked = names("function handle(_local: string, $value: number, 9bad: string) {}");

        assert!(blocked.contains("_local"));
        assert!(blocked.contains("$value"));
        assert!(!blocked.contains("9bad"));
        assert_eq!(blocked.len(), 2);
    }

    #[test]
    fn returns_empty_when_cursor_is_not_after_trailing_body() {
        let blocked = trailing_function_parameter_names_at_declaration_end(
            "function handle(first: string) {",
            32,
        );

        assert!(blocked.is_empty());
    }

    #[test]
    fn returns_empty_when_function_parameter_list_is_incomplete() {
        let blocked = names("function handle(first: string { }");

        assert!(blocked.is_empty());
    }
}
