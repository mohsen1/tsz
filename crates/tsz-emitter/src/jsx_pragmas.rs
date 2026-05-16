/// Extract a `@<tag> <factory>` pragma from the leading comments, returning
/// the factory expression such as `h` or `React.createElement`.
///
/// Mirrors tsc's classic JSX pragma handling: only comments before code are
/// scanned, the tag must be followed by a pragma boundary, and the value is a
/// dot-separated identifier chain.
fn extract_jsx_factory_like_pragma(source: &str, tag: &str) -> Option<String> {
    let scan_limit = source.len().min(4096);
    let text = &source[..scan_limit];
    let bytes = text.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            let comment_start = pos + 2;
            if let Some(end_offset) = text[comment_start..].find("*/") {
                let comment_body = &text[comment_start..comment_start + end_offset];
                let mut start = 0usize;
                let mut after_idx: Option<usize> = None;
                while let Some(rel) = comment_body[start..].find(tag) {
                    let abs = start + rel;
                    let after = abs + tag.len();
                    let body_bytes = comment_body.as_bytes();
                    if after >= body_bytes.len()
                        || (body_bytes[after] as char).is_ascii_whitespace()
                    {
                        after_idx = Some(after);
                        break;
                    }
                    start = after;
                    if start >= comment_body.len() {
                        break;
                    }
                }
                if let Some(after) = after_idx {
                    let factory: String = comment_body[after..]
                        .trim_start()
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '.')
                        .collect();
                    if !factory.is_empty() && is_dotted_identifier_chain(&factory) {
                        return Some(factory);
                    }
                }
                pos = comment_start + end_offset + 2;
            } else {
                break;
            }
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
            if let Some(nl) = text[pos..].find('\n') {
                pos += nl + 1;
            } else {
                break;
            }
            continue;
        }
        break;
    }
    None
}

fn is_pragma_boundary(body: &str, pos: usize) -> bool {
    let bytes = body.as_bytes();
    pos >= bytes.len() || (bytes[pos] as char).is_ascii_whitespace()
}

fn find_complete_pragma_tag(body: &str, tag: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(rel) = body[start..].find(tag) {
        let abs = start + rel;
        let after = abs + tag.len();
        if is_pragma_boundary(body, after) {
            return Some(after);
        }
        start = abs + tag.len();
        if start >= body.len() {
            break;
        }
    }
    None
}

fn is_dotted_identifier_chain(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.split('.').all(|seg| {
        let mut chars = seg.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '_' || first == '$' || first.is_alphabetic()) {
            return false;
        }
        chars.all(|c| c == '_' || c == '$' || c.is_alphanumeric())
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct JsxPragmaFacts {
    pub(crate) runtime: Option<JsxRuntimePragma>,
    pub(crate) factory: Option<String>,
    pub(crate) fragment_factory: Option<String>,
}

impl JsxPragmaFacts {
    pub(crate) fn from_source(source: &str) -> Self {
        Self {
            runtime: extract_jsx_runtime_pragma(source),
            factory: extract_jsx_factory(source),
            fragment_factory: extract_jsx_fragment_factory(source),
        }
    }

    pub(crate) fn classic_factory_roots(
        &self,
        jsx_factory: Option<&str>,
        jsx_fragment_factory: Option<&str>,
    ) -> Vec<String> {
        classic_jsx_factory_roots_from_facts(
            self.factory.as_deref(),
            self.fragment_factory.as_deref(),
            jsx_factory,
            jsx_fragment_factory,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsxRuntimePragma {
    Classic,
    Automatic,
}

/// Compute value roots referenced by classic JSX factory settings.
///
/// The caller is responsible for checking that classic JSX emit is active.
/// This keeps import elision and helper scheduling aligned with the printer's
/// factory lookup, including per-file `@jsx` / `@jsxFrag` pragmas.
fn classic_jsx_factory_roots_from_facts(
    pragma_factory: Option<&str>,
    pragma_fragment_factory: Option<&str>,
    jsx_factory: Option<&str>,
    jsx_fragment_factory: Option<&str>,
) -> Vec<String> {
    let jsx_factory = pragma_factory
        .or(jsx_factory)
        .unwrap_or("React.createElement")
        .to_string();
    let jsx_fragment_factory = pragma_fragment_factory
        .or(jsx_fragment_factory)
        .unwrap_or("React.Fragment")
        .to_string();

    let mut roots = Vec::new();
    for factory in [jsx_factory, jsx_fragment_factory] {
        let Some(root) = factory.split('.').next() else {
            continue;
        };
        if root.is_empty() || !is_dotted_identifier_chain(root) {
            continue;
        }
        if !roots.iter().any(|existing| existing == root) {
            roots.push(root.to_string());
        }
    }
    roots
}

/// Extract the last valid `@jsxRuntime classic` or `@jsxRuntime automatic`
/// pragma from block comments. Invalid prefix/value matches are ignored.
pub(crate) fn extract_jsx_runtime_pragma(source: &str) -> Option<JsxRuntimePragma> {
    if !source.contains("@jsxRuntime") {
        return None;
    }

    let mut result = None;
    let bytes = source.as_bytes();
    let mut pos = 0;
    while pos + 1 < bytes.len() {
        if bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            let comment_start = pos + 2;
            if let Some(end_offset) = source[comment_start..].find("*/") {
                let comment_body = &source[comment_start..comment_start + end_offset];
                if let Some(after) = find_complete_pragma_tag(comment_body, "@jsxRuntime") {
                    let rest = comment_body[after..].trim_start();
                    let value_end = rest
                        .char_indices()
                        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || *c == '$'))
                        .map(|(i, _)| i)
                        .unwrap_or(rest.len());
                    let value = &rest[..value_end];
                    let value_terminated =
                        value_end == rest.len() || rest.as_bytes()[value_end].is_ascii_whitespace();
                    if value_terminated {
                        match value {
                            "classic" => result = Some(JsxRuntimePragma::Classic),
                            "automatic" => result = Some(JsxRuntimePragma::Automatic),
                            _ => {}
                        }
                    }
                }
                pos = comment_start + end_offset + 2;
            } else {
                break;
            }
            continue;
        }
        pos += 1;
    }
    result
}

/// Extract a classic JSX `@jsx <factory>` pragma value from leading comments.
pub(crate) fn extract_jsx_factory(source: &str) -> Option<String> {
    extract_jsx_factory_like_pragma(source, "@jsx")
}

/// Extract a classic JSX `@jsxFrag <factory>` pragma value from leading comments.
pub(crate) fn extract_jsx_fragment_factory(source: &str) -> Option<String> {
    extract_jsx_factory_like_pragma(source, "@jsxFrag")
}
