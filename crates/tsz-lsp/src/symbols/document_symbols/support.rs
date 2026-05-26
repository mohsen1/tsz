use super::*;

impl<'a> DocumentSymbolProvider<'a> {
    /// A class-member name is "complex-computed" when it's a `[expr]` bracket
    /// form whose inner expression isn't a simple identifier or literal.
    pub(super) fn is_complex_computed_name(&self, name_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(name_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(comp) = self.arena.get_computed_property(node) else {
            return false;
        };
        let expr_idx = comp.expression;
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        matches!(
            expr_node.kind,
            k if !(k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::PrivateIdentifier as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION)
        )
    }

    /// Synthetic declaration placeholders become `default` under default export.
    pub(super) fn is_synthetic_placeholder_name(&self, name: &str) -> bool {
        matches!(
            name,
            "<class>" | "<function>" | "<anonymous>" | "<interface>" | "<type>" | "<enum>"
        )
    }

    /// Check if a node kind is a declaration.
    pub(super) const fn is_declaration(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            || kind == syntax_kind_ext::MODULE_DECLARATION
    }

    /// Get range for a keyword (when no identifier exists, e.g. "constructor").
    pub(super) fn get_range_keyword(&self, node_idx: NodeIndex, len: u32) -> Range {
        if let Some(node) = self.arena.get(node_idx) {
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self
                .line_map
                .offset_to_position(node.pos + len, self.source_text);
            Range::new(start, end)
        } else {
            Range::new(Position::new(0, 0), Position::new(0, 0))
        }
    }

    /// Extract text from identifier node.
    pub(super) fn get_name(&self, node_idx: NodeIndex) -> Option<String> {
        if node_idx.is_none() {
            return None;
        }
        if let Some(node) = self.arena.get(node_idx) {
            if node.kind == SyntaxKind::Identifier as u16 {
                return self.arena.get_identifier(node).and_then(|id| {
                    if id.escaped_text.is_empty() {
                        None
                    } else {
                        Some(id.escaped_text.clone())
                    }
                });
            } else if node.kind == SyntaxKind::PrivateIdentifier as u16 {
                return self.arena.get_identifier(node).map(|id| {
                    if id.escaped_text.starts_with('#') {
                        id.escaped_text.clone()
                    } else {
                        format!("#{}", id.escaped_text)
                    }
                });
            } else if node.kind == SyntaxKind::StringLiteral as u16 {
                let start = node.pos as usize;
                let end = node.end as usize;
                if start <= end && end <= self.source_text.len() {
                    return Some(self.source_text[start..end].trim().to_string());
                }
                return self.arena.get_literal(node).map(|l| l.text.clone());
            } else if node.kind == SyntaxKind::NumericLiteral as u16 {
                return self.arena.get_literal(node).map(|l| l.text.clone());
            } else if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                let start = node.pos as usize;
                let end = node.end as usize;
                if start <= end && end <= self.source_text.len() {
                    let slice = &self.source_text[start..end];
                    if let Some(close) = slice.rfind(']') {
                        return Some(slice[..=close].to_string());
                    }
                    return Some(slice.to_string());
                }
            }
        }
        None
    }
}

/// Mirror tsc's `cleanText`: truncate to 150 characters (appending
/// `...`) and strip ECMAScript line terminators, including the
/// trailing backslash from multiline string literal continuations.
pub(super) fn clean_module_text(text: &str) -> String {
    const MAX_LEN: usize = 150;
    let truncated = if text.chars().count() > MAX_LEN {
        let head: String = text.chars().take(MAX_LEN).collect();
        format!("{head}...")
    } else {
        text.to_string()
    };
    let mut out = String::with_capacity(truncated.len());
    let mut chars = truncated.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some('\r' | '\n' | '\u{2028}' | '\u{2029}')) => {
                if let Some('\r') = chars.next()
                    && matches!(chars.peek(), Some('\n'))
                {
                    chars.next();
                }
            }
            '\r' => {
                if matches!(chars.peek(), Some('\n')) {
                    chars.next();
                }
            }
            '\n' | '\u{2028}' | '\u{2029}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Append `declare` to every descendant's `kindModifiers` (skipping duplicates).
pub(super) fn propagate_ambient_modifier(symbols: &mut [DocumentSymbolEntry]) {
    for sym in symbols.iter_mut() {
        let mut buf = sym.kind_modifiers.clone();
        append_modifier(&mut buf, "declare");
        sym.kind_modifiers = buf;
        propagate_ambient_modifier(&mut sym.children);
    }
}

/// Merge sibling Module/Namespace entries that share a name.
pub(super) fn merge_same_name_modules(symbols: &mut Vec<DocumentSymbolEntry>) {
    let mut i = 0;
    while i < symbols.len() {
        let mergeable = is_mergeable_kind(symbols[i].kind);
        if mergeable.is_none() {
            merge_same_name_modules(&mut symbols[i].children);
            i += 1;
            continue;
        }
        let target_group = mergeable.unwrap();
        let name = symbols[i].name.clone();
        let mut j = i + 1;
        while j < symbols.len() {
            let same =
                is_mergeable_kind(symbols[j].kind) == Some(target_group) && symbols[j].name == name;
            if same {
                let other = symbols.remove(j);
                symbols[i].children.extend(other.children);
            } else {
                j += 1;
            }
        }
        merge_same_name_modules(&mut symbols[i].children);
        i += 1;
    }
}

pub(super) fn cap_document_symbols(symbols: &mut Vec<DocumentSymbolEntry>) {
    let mut remaining = MAX_DOCUMENT_SYMBOL_ENTRIES;
    cap_document_symbols_at_depth(symbols, 0, &mut remaining);
}

fn cap_document_symbols_at_depth(
    symbols: &mut Vec<DocumentSymbolEntry>,
    depth: usize,
    remaining: &mut usize,
) {
    let original = std::mem::take(symbols);
    let mut capped = Vec::with_capacity(original.len().min(*remaining));
    let mut iter = original.into_iter().peekable();

    while let Some(mut symbol) = iter.next() {
        if *remaining == 0 {
            break;
        }
        if *remaining == 1 && iter.peek().is_some() {
            capped.push(more_document_symbol(symbol.range));
            *remaining = 0;
            break;
        }

        *remaining -= 1;
        if depth + 1 >= MAX_DOCUMENT_SYMBOL_DEPTH {
            if !symbol.children.is_empty() {
                symbol.children.clear();
                if *remaining > 0 {
                    *remaining -= 1;
                    symbol.children.push(more_document_symbol(symbol.range));
                }
            }
        } else {
            cap_document_symbols_at_depth(&mut symbol.children, depth + 1, remaining);
        }
        capped.push(symbol);
    }

    *symbols = capped;
}

pub(super) fn more_document_symbol(range: Range) -> DocumentSymbolEntry {
    DocumentSymbolEntry {
        name: MORE_DOCUMENT_SYMBOL_NAME.to_string(),
        detail: None,
        kind: SymbolKind::Module,
        kind_modifiers: String::new(),
        range,
        selection_range: range,
        container_name: None,
        children: Vec::new(),
    }
}

const fn is_mergeable_kind(kind: SymbolKind) -> Option<u8> {
    match kind {
        SymbolKind::Module | SymbolKind::Namespace | SymbolKind::Package => Some(1),
        SymbolKind::Interface => Some(2),
        SymbolKind::Enum => Some(3),
        _ => None,
    }
}

/// Helper to append a modifier to a comma-separated string.
pub(super) fn append_modifier(result: &mut String, modifier: &str) {
    if result.split(',').any(|existing| existing == modifier) {
        return;
    }
    if !result.is_empty() {
        result.push(',');
    }
    result.push_str(modifier);
}
