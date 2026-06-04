use super::{Server, TsServerRequest, TsServerResponse};

use tsz::binder::SymbolId;

use tsz::lsp::definition::GoToDefinition;

use tsz::lsp::highlighting::DocumentHighlightProvider;

use tsz::lsp::hover::HoverProvider;

use tsz::lsp::position::{LineMap, Position, Range};

use tsz::lsp::project::Project;

use tsz::lsp::references::FindReferences;

use tsz::lsp::rename::RenameProvider;

use tsz::lsp::symbols::document_symbols::DocumentSymbolProvider;

use tsz::parser::node::NodeAccess;

use tsz_solver::construction::TypeInterner;

/// Bundled context for a parsed file, reducing parameter count in helpers.
pub(super) struct ParsedFileContext<'a> {
    pub(super) arena: &'a tsz::parser::node::NodeArena,
    pub(super) binder: &'a tsz::binder::BinderState,
    pub(super) line_map: &'a LineMap,
    pub(super) root: tsz::parser::NodeIndex,
    pub(super) source_text: &'a str,
    pub(super) file: &'a str,
}

fn import_context_for_range(source_text: &str, range: Range) -> Option<(Position, Position)> {
    if range.start.line != range.end.line {
        return None;
    }
    let line_text = source_text.lines().nth(range.start.line as usize)?;
    let trimmed = line_text.trim_start();
    let is_import = trimmed.starts_with("import ")
        || trimmed.starts_with("import{")
        || trimmed.starts_with("import\"")
        || trimmed.starts_with("import'");
    if !is_import {
        return None;
    }
    Some((
        Position::new(range.start.line, 0),
        Position::new(range.start.line, line_text.len() as u32),
    ))
}

/// Map a `DocumentSymbol`'s kind + `kind_modifiers` to the tsserver `ScriptElementKind` string.
fn symbol_kind_to_tsserver(
    kind: tsz::lsp::symbols::document_symbols::SymbolKind,
    kind_modifiers: &str,
) -> &'static str {
    use tsz::lsp::symbols::document_symbols::SymbolKind;
    match kind {
        SymbolKind::Module => "module",
        SymbolKind::Class => "class",
        SymbolKind::Method => "method",
        SymbolKind::Property | SymbolKind::Field => "property",
        // SynthesizedConstructor shares the "constructor" tsserver kind because it is
        // backed by a synthesized function node and occupies the same protocol slot.
        SymbolKind::Constructor | SymbolKind::SynthesizedConstructor => "constructor",
        SymbolKind::Enum => "enum",
        SymbolKind::Interface => "interface",
        SymbolKind::Function => "function",
        SymbolKind::Variable => {
            if kind_modifiers.contains("let") {
                "let"
            } else {
                "var"
            }
        }
        SymbolKind::Constant => "const",
        SymbolKind::EnumMember => "enum member",
        SymbolKind::TypeParameter => "type parameter",
        SymbolKind::Struct => "type",
        SymbolKind::Alias => "alias",
        SymbolKind::Getter => "getter",
        SymbolKind::Setter => "setter",
        SymbolKind::CallSignature => "call",
        SymbolKind::ConstructSignature => "construct",
        SymbolKind::IndexSignature => "index",
        SymbolKind::Unknown => "",
        _ => "unknown",
    }
}

/// Mirror tsc's navigationBar `isExternalModule` check (narrower than
/// the binder's which also treats CommonJS indicators as making a
/// file modular). For the nav entry's root label, tsc emits
/// `"<file>"` module only when the file contains ES
/// import/export/import.meta, or uses a module-only extension
/// (.mts/.cts/.mjs/.cjs).
fn is_es_module_for_navbar(
    arena: &tsz::parser::node::NodeArena,
    root: tsz::parser::NodeIndex,
    file: &str,
) -> bool {
    use tsz::parser::syntax_kind_ext;
    let lower = file.to_lowercase();
    if lower.ends_with(".mts")
        || lower.ends_with(".cts")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
    {
        return true;
    }
    let Some(node) = arena.get(root) else {
        return false;
    };
    let Some(sf) = arena.get_source_file(node) else {
        return false;
    };
    for &stmt_idx in &sf.statements.nodes {
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };
        if matches!(
            stmt.kind,
            k if k == syntax_kind_ext::IMPORT_DECLARATION
                || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                || k == syntax_kind_ext::EXPORT_DECLARATION
                || k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                || k == syntax_kind_ext::EXPORT_ASSIGNMENT
        ) {
            return true;
        }
        // Top-level `export`-prefixed declaration also counts.
        if has_export_modifier(arena, stmt_idx) {
            return true;
        }
    }
    false
}

fn has_export_modifier(
    arena: &tsz::parser::node::NodeArena,
    node_idx: tsz::parser::NodeIndex,
) -> bool {
    use tsz::parser::syntax_kind_ext;
    use tsz_scanner::SyntaxKind;
    let Some(node) = arena.get(node_idx) else {
        return false;
    };
    let modifiers = match node.kind {
        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
            arena.get_function(node).and_then(|f| f.modifiers.as_ref())
        }
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            arena.get_class(node).and_then(|c| c.modifiers.as_ref())
        }
        k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
            arena.get_interface(node).and_then(|i| i.modifiers.as_ref())
        }
        k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
            .get_type_alias(node)
            .and_then(|a| a.modifiers.as_ref()),
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            arena.get_enum(node).and_then(|e| e.modifiers.as_ref())
        }
        k if k == syntax_kind_ext::MODULE_DECLARATION => {
            arena.get_module(node).and_then(|m| m.modifiers.as_ref())
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            arena.get_variable(node).and_then(|v| v.modifiers.as_ref())
        }
        _ => None,
    };
    let Some(mods) = modifiers else {
        return false;
    };
    mods.nodes.iter().any(|&m_idx| {
        arena
            .get(m_idx)
            .is_some_and(|m| m.kind == SyntaxKind::ExportKeyword as u16)
    })
}

/// Mirror tsc's `escapeString(s, '"')` — replace control characters
/// and backslash/double-quote with their JS escape sequences, and
/// encode non-printable high chars as `\uNNNN`. Used on the filename
/// stem before wrapping it in double quotes for external-module
/// navbar/navtree root entries, so a filename like `my fil<TAB>e`
/// renders as `"my fil\te"` rather than embedding a literal tab.
fn escape_string_double_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\x0b' => out.push_str("\\v"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '\u{0085}' => out.push_str("\\u0085"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Sort a navtree/navbar symbol slice in-place, recursively sorting each
/// node's children. Mirrors TypeScript's `compareChildren`: primary key
/// is case-insensitive name, tiebreaker is source position.
///
/// Computed property names (`[key]`, `[1]`, `["foo"]`) are treated as
/// nameless — tsc's `tryGetName` returns undefined for them and the
/// comparer then falls back to source position only. Matching that
/// keeps their relative order stable (otherwise `[a]` gets sorted
/// before `[E.A]` purely by bracket-text, and the navbar diverges
/// from the source-ordered expected output).
fn sort_symbols_deep(symbols: &mut [tsz::lsp::symbols::document_symbols::DocumentSymbol]) {
    use tsz::lsp::symbols::document_symbols::SymbolKind;
    // Children of an expando-promoted class (synthesized constructor +
    // BinaryExpression / CallExpression nav nodes from
    // `X.prototype.y = …` / `Object.defineProperty(X, …)`) sort by
    // source position — tsc's `compareChildren` falls through to
    // `compareValues(node.pos, node.pos)` for expando nodes since
    // their `tryGetName` returns undefined (or the owner's name).
    let is_expando_container = symbols
        .iter()
        .any(|s| matches!(s.kind, SymbolKind::SynthesizedConstructor));
    if is_expando_container {
        symbols.sort_by(|a, b| {
            (a.range.start.line, a.range.start.character)
                .cmp(&(b.range.start.line, b.range.start.character))
        });
        for sym in symbols.iter_mut() {
            sort_symbols_deep(&mut sym.children);
        }
        return;
    }
    fn sort_key(sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol) -> Option<String> {
        // Mirror tsc's `tryGetName`: constructors and interface-type
        // signatures are truly nameless. Computed property names
        // where the inner expression is a simple literal (`[1]`,
        // `["foo"]`) unwrap to the literal's value so they sort
        // alongside identifier-named siblings — `[1]` compares as
        // `"1"`, `["A7"]` as `"A7"`. Complex computed expressions
        // stay nameless.
        if sym.name == "()" || sym.name == "new()" || sym.name == "[]" {
            return None;
        }
        if matches!(sym.kind, SymbolKind::Constructor) {
            return None;
        }
        if let Some(stripped) = sym.name.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let inner = stripped.trim();
            // Numeric literal: `[1]`, `[42]`, `[3.14]`.
            if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return Some(inner.to_lowercase());
            }
            // String literal: `["foo"]` or `['foo']`. Strip the quotes.
            if (inner.starts_with('"') && inner.ends_with('"') && inner.len() >= 2)
                || (inner.starts_with('\'') && inner.ends_with('\'') && inner.len() >= 2)
            {
                return Some(inner[1..inner.len() - 1].to_lowercase());
            }
            // Anything else (`[Symbol.iterator]`, `[a]`, `[1+1]`) —
            // tsc's `tryGetName` returns undefined → nameless.
            return None;
        }
        Some(sym.name.to_lowercase())
    }
    // tsc's `compareChildren` tiebreaker is `navigationBarNodeKind` — the
    // AST SyntaxKind of the underlying node. Map our higher-level
    // `SymbolKind` to those underlying values so two siblings with the
    // same name (e.g. `class Foo {}` + `let Foo = 1;`) sort in the same
    // order tsc produces. Numbers come from TypeScript's SyntaxKind enum.
    const fn kind_rank(k: SymbolKind) -> u16 {
        match k {
            SymbolKind::Property | SymbolKind::Field => 171,
            SymbolKind::Method => 174,
            SymbolKind::Constructor => 176,
            SymbolKind::Getter => 177,
            SymbolKind::Setter => 178,
            SymbolKind::EnumMember => 304,
            SymbolKind::Variable
            | SymbolKind::Constant
            | SymbolKind::Boolean
            | SymbolKind::Array
            | SymbolKind::Object
            | SymbolKind::Null
            | SymbolKind::Number
            | SymbolKind::String => 260,
            // SynthesizedConstructor uses the FunctionDeclaration ordinal (262) because
            // it is backed by a function node and tsc's comparer uses that SyntaxKind.
            SymbolKind::Function
            | SymbolKind::Event
            | SymbolKind::Operator
            | SymbolKind::SynthesizedConstructor => 262,
            SymbolKind::Class => 263,
            SymbolKind::Interface => 264,
            SymbolKind::Struct => 265, // type alias
            SymbolKind::Enum => 266,
            SymbolKind::Module | SymbolKind::Namespace | SymbolKind::Package | SymbolKind::File => {
                267
            }
            SymbolKind::Alias => 280, // ImportSpecifier / NamespaceImport
            SymbolKind::TypeParameter => 170,
            SymbolKind::Key => 172,
            SymbolKind::CallSignature => 180,
            SymbolKind::ConstructSignature => 181,
            SymbolKind::IndexSignature => 182,
            // Unknown maps to BinaryExpression (227) — expando assignments
            // like `X.y = 42` are BinaryExpression nav nodes in tsc.
            SymbolKind::Unknown => 227,
        }
    }
    symbols.sort_by(|a, b| {
        match (sort_key(a), sort_key(b)) {
            (Some(na), Some(nb)) => match na.cmp(&nb) {
                std::cmp::Ordering::Equal => kind_rank(a.kind).cmp(&kind_rank(b.kind)),
                other => other,
            },
            // tsc: `compareStringsCaseInsensitive(undefined, x)` sorts
            // undefined before any string. Computed-name items therefore
            // sort ahead of identifier-name items at the same level, and
            // amongst themselves fall back to source position (kind rank
            // alone can't distinguish two `[computed]` class methods).
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            // Nameless against nameless: tsc's tiebreaker is kind
            // ordinal (call < construct < index). Same-kind pairs
            // (e.g. two computed-name methods) fall back to source
            // position.
            (None, None) => match kind_rank(a.kind).cmp(&kind_rank(b.kind)) {
                std::cmp::Ordering::Equal => (a.range.start.line, a.range.start.character)
                    .cmp(&(b.range.start.line, b.range.start.character)),
                other => other,
            },
        }
    });
    for sym in symbols.iter_mut() {
        sort_symbols_deep(&mut sym.children);
    }
}

include!("handlers_info_parts/part1.rs");
include!("handlers_info_parts/part2.rs");
