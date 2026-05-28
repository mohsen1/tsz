use std::path::Path;

use crate::config::ResolvedCompilerOptions;
use tsz::emitter::ModuleKind;
use tsz::module_resolver::{ImportKind, is_path_relative};
use tsz::parser::NodeIndex;
use tsz::parser::ParserState;
use tsz::parser::node::{NodeAccess, NodeArena};
use tsz::scanner::SyntaxKind;
use tsz::scanner::scanner_impl::ScannerState;

use super::{
    AmbientModuleDeclarationSpecifierPolicy, CollectedModuleSpecifier, ModuleResolutionCache,
    SourceDiscoveryModuleRequest, implied_resolution_mode_for_file_with_cache,
};

#[allow(dead_code)]
pub(crate) fn collect_module_specifiers_from_text(path: &Path, text: &str) -> Vec<String> {
    collect_module_requests_from_text(path, text)
        .into_iter()
        .map(|(specifier, _, _, _)| specifier)
        .collect()
}

pub(crate) fn collect_module_requests_from_text(
    path: &Path,
    text: &str,
) -> Vec<SourceDiscoveryModuleRequest> {
    // Fast path: skip the full parse if the text cannot contain any module specifiers.
    // This avoids a redundant parse for files that will be parsed again in build_program.
    if !text_may_contain_module_specifiers(text) {
        return Vec::new();
    }
    if let Some(requests) = collect_simple_module_requests_from_text(text) {
        return requests;
    }
    let file_name = path.to_string_lossy().into_owned();
    let mut parser = ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let mut requests: Vec<_> = collect_module_specifiers_for_source_discovery(&arena, source_file)
        .into_iter()
        .map(
            |(specifier, specifier_idx, import_kind, resolution_mode_override)| {
                (
                    specifier,
                    import_kind,
                    resolution_mode_override,
                    module_specifier_has_type_json_import_attribute(&arena, specifier_idx),
                )
            },
        )
        .collect();
    if let Some(source) = arena.get_source_file_at(source_file) {
        requests.extend(collect_jsdoc_import_requests(source).into_iter().map(
            |(specifier, import_kind, resolution_mode_override)| {
                (specifier, import_kind, resolution_mode_override, false)
            },
        ));
    }
    requests
}

/// Quick text scan to determine if a source file might contain module specifiers.
/// Returns false only when we can guarantee there are no imports/exports/requires.
fn text_may_contain_module_specifiers(text: &str) -> bool {
    // All module specifier patterns require at least one of these keywords:
    // - `import` for ES imports and dynamic import()
    // - `require` for CommonJS require calls, including trivia before `(`
    // - `from '` or `from "` for re-exports like `export { x } from 'y'`
    // - `declare module` for ambient module declarations
    text.contains("import")
        || text.contains("require")
        || text.contains("from '")
        || text.contains("from \"")
        || text.contains("declare module")
}

#[derive(Debug)]
struct DiscoveryToken {
    kind: SyntaxKind,
    text: Option<String>,
}

fn collect_simple_module_requests_from_text(
    text: &str,
) -> Option<Vec<SourceDiscoveryModuleRequest>> {
    if text.contains("@import") {
        return None;
    }

    let mut scanner = ScannerState::new(text.to_string(), true);
    let mut tokens = Vec::new();
    loop {
        let kind = scanner.scan();
        if kind == SyntaxKind::EndOfFileToken {
            break;
        }
        let token_text = if kind == SyntaxKind::StringLiteral {
            Some(strip_scanned_string_literal(scanner.get_token_text_ref())?)
        } else {
            None
        };
        tokens.push(DiscoveryToken {
            kind,
            text: token_text,
        });
    }

    let mut requests = Vec::new();
    let mut brace_depth = 0usize;
    let mut i = 0usize;
    while i < tokens.len() {
        match tokens[i].kind {
            SyntaxKind::OpenBraceToken => {
                brace_depth += 1;
                i += 1;
            }
            SyntaxKind::CloseBraceToken => {
                brace_depth = brace_depth.saturating_sub(1);
                i += 1;
            }
            SyntaxKind::DeclareKeyword if brace_depth == 0 => {
                if tokens
                    .get(i + 1)
                    .is_some_and(|t| t.kind == SyntaxKind::ModuleKeyword)
                {
                    let module_name = tokens.get(i + 2).and_then(|t| t.text.as_ref())?;
                    if is_path_relative(module_name) {
                        requests.push((module_name.clone(), ImportKind::EsmImport, None, false));
                    }
                    if tokens
                        .get(i + 3)
                        .is_some_and(|t| t.kind == SyntaxKind::OpenBraceToken)
                    {
                        i = skip_ambient_module_body_without_dependencies(&tokens, i + 3)?;
                        continue;
                    }
                    i += 3;
                    continue;
                }
                i += 1;
            }
            SyntaxKind::ModuleKeyword if brace_depth == 0 => return None,
            SyntaxKind::ImportKeyword => {
                if brace_depth != 0 {
                    return None;
                }
                let (request, next_i) = collect_simple_import_request(&tokens, i)?;
                requests.push((request, ImportKind::EsmImport, None, false));
                i = next_i;
            }
            SyntaxKind::ExportKeyword => {
                if brace_depth != 0 {
                    return None;
                }
                if let Some((request, next_i)) = collect_simple_export_request(&tokens, i)? {
                    requests.push((request, ImportKind::EsmReExport, None, false));
                    i = next_i;
                } else {
                    i += 1;
                }
            }
            SyntaxKind::RequireKeyword => return None,
            _ => {
                i += 1;
            }
        }
    }

    Some(requests)
}

fn strip_scanned_string_literal(text: &str) -> Option<String> {
    let quote = text.as_bytes().first().copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    if text.as_bytes().last().copied() != Some(quote) || text.len() < 2 {
        return None;
    }

    let value = &text[1..text.len() - 1];
    if value.contains('\\') {
        return None;
    }
    Some(value.to_string())
}

fn skip_ambient_module_body_without_dependencies(
    tokens: &[DiscoveryToken],
    open_brace: usize,
) -> Option<usize> {
    let mut depth = 1usize;
    let mut i = open_brace + 1;
    while i < tokens.len() {
        match tokens[i].kind {
            SyntaxKind::OpenBraceToken => depth += 1,
            SyntaxKind::CloseBraceToken => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            SyntaxKind::ImportKeyword | SyntaxKind::RequireKeyword => return None,
            SyntaxKind::FromKeyword
                if tokens
                    .get(i + 1)
                    .is_some_and(|token| token.kind == SyntaxKind::StringLiteral) =>
            {
                return None;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn collect_simple_import_request(
    tokens: &[DiscoveryToken],
    import_idx: usize,
) -> Option<(String, usize)> {
    let next = tokens.get(import_idx + 1)?;
    match next.kind {
        SyntaxKind::StringLiteral => {
            reject_import_attributes_after_string(tokens, import_idx + 2)?;
            Some((next.text.clone()?, import_idx + 2))
        }
        SyntaxKind::OpenParenToken | SyntaxKind::DotToken => None,
        _ => {
            let mut i = import_idx + 1;
            while i < tokens.len() {
                match tokens[i].kind {
                    SyntaxKind::SemicolonToken
                    | SyntaxKind::EqualsToken
                    | SyntaxKind::RequireKeyword
                    | SyntaxKind::CloseBraceToken => return None,
                    SyntaxKind::OpenBraceToken => {
                        let close = matching_brace(tokens, i)?;
                        i = close + 1;
                        continue;
                    }
                    SyntaxKind::FromKeyword
                        if tokens
                            .get(i + 1)
                            .is_some_and(|token| token.kind == SyntaxKind::StringLiteral) =>
                    {
                        reject_import_attributes_after_string(tokens, i + 2)?;
                        return Some((tokens[i + 1].text.clone()?, i + 2));
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
    }
}

fn collect_simple_export_request(
    tokens: &[DiscoveryToken],
    export_idx: usize,
) -> Option<Option<(String, usize)>> {
    let mut i = export_idx + 1;
    if tokens
        .get(i)
        .is_some_and(|token| token.kind == SyntaxKind::TypeKeyword)
    {
        i += 1;
    }

    match tokens.get(i).map(|token| token.kind) {
        Some(SyntaxKind::AsteriskToken | SyntaxKind::OpenBraceToken) => {}
        _ => return Some(None),
    }

    while i < tokens.len() {
        match tokens[i].kind {
            SyntaxKind::SemicolonToken => return Some(None),
            SyntaxKind::OpenBraceToken => {
                let close = matching_brace(tokens, i)?;
                i = close + 1;
                continue;
            }
            SyntaxKind::FromKeyword
                if tokens
                    .get(i + 1)
                    .is_some_and(|token| token.kind == SyntaxKind::StringLiteral) =>
            {
                reject_import_attributes_after_string(tokens, i + 2)?;
                return Some(Some((tokens[i + 1].text.clone()?, i + 2)));
            }
            _ => {}
        }
        i += 1;
    }

    Some(None)
}

fn matching_brace(tokens: &[DiscoveryToken], open_brace: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut i = open_brace + 1;
    while i < tokens.len() {
        match tokens[i].kind {
            SyntaxKind::OpenBraceToken => depth += 1,
            SyntaxKind::CloseBraceToken => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn reject_import_attributes_after_string(
    tokens: &[DiscoveryToken],
    after_string: usize,
) -> Option<()> {
    match tokens.get(after_string).map(|token| token.kind) {
        Some(SyntaxKind::WithKeyword | SyntaxKind::AssertKeyword) => None,
        _ => Some(()),
    }
}

fn collect_jsdoc_import_requests(
    source: &tsz::parser::node::SourceFileData,
) -> Vec<(
    String,
    tsz::module_resolver::ImportKind,
    Option<tsz::module_resolver::ImportingModuleKind>,
)> {
    use tsz::module_resolver::ImportKind;
    use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

    if source.comments.is_empty() || !source.text.contains("@import") {
        return Vec::new();
    }

    let source_text = source.text.as_ref();
    let mut requests = Vec::new();
    for comment in &source.comments {
        if !is_jsdoc_comment(comment, source_text) {
            continue;
        }

        let content = get_jsdoc_content(comment, source_text);
        for line in content.lines() {
            let trimmed = line.trim_start_matches('*').trim();
            let Some(rest) = strip_jsdoc_import_tag_prefix(trimmed) else {
                continue;
            };
            if let Some(specifier) = parse_jsdoc_import_module_specifier(rest) {
                requests.push((specifier, ImportKind::EsmImport, None));
            }
        }
    }

    requests
}

fn strip_jsdoc_import_tag_prefix(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("@import")?;
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(rest)
}

fn parse_jsdoc_import_module_specifier(rest: &str) -> Option<String> {
    let rest = rest.trim();
    let from_idx = find_jsdoc_import_from_keyword(rest)?;
    let before_from = rest[..from_idx].trim();
    if matches!(
        before_from.split_whitespace().next(),
        Some("type" | "defer")
    ) && before_from.contains(char::is_whitespace)
    {
        return None;
    }

    let after_from = rest[from_idx + 4..].trim_start();
    let mut chars = after_from.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '"' && quote != '\'' && quote != '`' {
        return None;
    }

    let mut specifier = String::new();
    let mut escaped = false;
    for (_, ch) in chars {
        if escaped {
            specifier.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(specifier);
        }
        specifier.push(ch);
    }

    None
}

fn find_jsdoc_import_from_keyword(rest: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut last_from = None;

    for (idx, ch) in rest.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == active_quote {
                quote = None;
            }
            continue;
        }

        if ch == '"' || ch == '\'' || ch == '`' {
            quote = Some(ch);
            continue;
        }

        if rest[idx..].starts_with("from")
            && !rest[..idx]
                .chars()
                .next_back()
                .is_some_and(is_jsdoc_import_keyword_part)
            && !rest[idx + 4..]
                .chars()
                .next()
                .is_some_and(is_jsdoc_import_keyword_part)
        {
            last_from = Some(idx);
        }
    }

    last_from
}

const fn is_jsdoc_import_keyword_part(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
pub(crate) fn collect_module_specifiers(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<CollectedModuleSpecifier> {
    collect_module_specifiers_impl(
        arena,
        source_file,
        AmbientModuleDeclarationSpecifierPolicy::All,
    )
}

pub(crate) fn collect_module_specifiers_for_check(
    arena: &NodeArena,
    source_file: NodeIndex,
    is_external_module: bool,
) -> Vec<CollectedModuleSpecifier> {
    collect_module_specifiers_impl(
        arena,
        source_file,
        AmbientModuleDeclarationSpecifierPolicy::Check { is_external_module },
    )
}

fn collect_module_specifiers_for_source_discovery(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<CollectedModuleSpecifier> {
    collect_module_specifiers_impl(
        arena,
        source_file,
        AmbientModuleDeclarationSpecifierPolicy::SourceDiscovery,
    )
}

fn collect_module_specifiers_impl(
    arena: &NodeArena,
    source_file: NodeIndex,
    ambient_declaration_policy: AmbientModuleDeclarationSpecifierPolicy,
) -> Vec<CollectedModuleSpecifier> {
    use tsz::module_resolver::ImportKind;

    let Some(source) = arena.get_source_file_at(source_file) else {
        return Vec::new();
    };
    let mut specifiers = Vec::with_capacity(source.statements.nodes.len().min(64));

    // Helper to strip surrounding quotes from a module specifier
    let strip_quotes =
        |s: &str| -> String { s.trim_matches(|c| c == '"' || c == '\'').to_string() };

    for &stmt_idx in &source.statements.nodes {
        if stmt_idx.is_none() {
            continue;
        }
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };

        // Handle ES6 imports: import { x } from './module'
        // and import equals with require: import x = require('./module')
        if let Some(import_decl) = arena.get_import_decl(stmt) {
            // Check if this is an import equals declaration (kind 272 = CJS require)
            // vs a regular import declaration (kind 273 = ESM import)
            let is_import_equals =
                stmt.kind == tsz::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION;

            if let Some(text) = arena.get_literal_text(import_decl.module_specifier) {
                let kind = if is_import_equals {
                    ImportKind::CjsRequire
                } else {
                    ImportKind::EsmImport
                };
                specifiers.push((
                    strip_quotes(text),
                    import_decl.module_specifier,
                    kind,
                    import_attributes_resolution_mode(arena, import_decl.attributes),
                ));
            } else {
                // Handle import equals declaration: import x = require('./module')
                // The module_specifier might be a CallExpression for require()
                if let Some(spec_text) =
                    extract_require_specifier(arena, import_decl.module_specifier)
                {
                    specifiers.push((
                        spec_text,
                        import_decl.module_specifier,
                        ImportKind::CjsRequire,
                        import_attributes_resolution_mode(arena, import_decl.attributes),
                    ));
                }
            }
        }

        // Handle exports: export { x } from './module'
        if let Some(export_decl) = arena.get_export_decl(stmt) {
            if export_decl.export_clause.is_some()
                && let Some(import_decl) = arena.get_import_decl_at(export_decl.export_clause)
            {
                let import_kind = if arena.get(export_decl.export_clause).is_some_and(|node| {
                    node.kind == tsz::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                }) {
                    ImportKind::CjsRequire
                } else {
                    ImportKind::EsmReExport
                };
                if let Some(text) = arena.get_literal_text(import_decl.module_specifier) {
                    specifiers.push((
                        strip_quotes(text),
                        import_decl.module_specifier,
                        import_kind,
                        import_attributes_resolution_mode(arena, export_decl.attributes),
                    ));
                } else if let Some(spec_text) =
                    extract_require_specifier(arena, import_decl.module_specifier)
                {
                    specifiers.push((
                        spec_text,
                        import_decl.module_specifier,
                        ImportKind::CjsRequire,
                        import_attributes_resolution_mode(arena, export_decl.attributes),
                    ));
                }
            } else if let Some(text) = arena.get_literal_text(export_decl.module_specifier) {
                specifiers.push((
                    strip_quotes(text),
                    export_decl.module_specifier,
                    ImportKind::EsmReExport,
                    import_attributes_resolution_mode(arena, export_decl.attributes),
                ));
            }
        }

        // Handle ambient module declarations: declare module "x" { ... }
        if let Some(module_decl) = arena.get_module(stmt) {
            let has_declare = module_decl.modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&mod_idx| {
                    arena
                        .get(mod_idx)
                        .is_some_and(|node| node.kind == SyntaxKind::DeclareKeyword as u16)
                })
            });
            if has_declare && let Some(text) = arena.get_literal_text(module_decl.name) {
                let specifier = strip_quotes(text);
                // Relative names can be module augmentations of concrete sibling
                // files. Non-relative names only need driver resolution in
                // non-declaration external modules, where the lookup proves
                // whether a bare augmentation target exists for TS2664.
                let include_non_relative = match ambient_declaration_policy {
                    #[cfg(test)]
                    AmbientModuleDeclarationSpecifierPolicy::All => true,
                    AmbientModuleDeclarationSpecifierPolicy::SourceDiscovery => false,
                    AmbientModuleDeclarationSpecifierPolicy::Check { is_external_module } => {
                        is_external_module && !source.is_declaration_file
                    }
                };
                if include_non_relative || tsz::module_resolver::is_path_relative(&specifier) {
                    specifiers.push((specifier, module_decl.name, ImportKind::EsmImport, None));
                }
            }
            if let Some(body_node) = arena.get(module_decl.body)
                && let Some(block) = arena.get_module_block(body_node)
                && let Some(statements) = &block.statements
            {
                for &inner_idx in &statements.nodes {
                    if inner_idx.is_none() {
                        continue;
                    }
                    let Some(inner_stmt) = arena.get(inner_idx) else {
                        continue;
                    };
                    if let Some(import_decl) = arena.get_import_decl(inner_stmt) {
                        let is_import_equals = inner_stmt.kind
                            == tsz::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION;
                        if let Some(text) = arena.get_literal_text(import_decl.module_specifier) {
                            specifiers.push((
                                strip_quotes(text),
                                import_decl.module_specifier,
                                if is_import_equals {
                                    ImportKind::CjsRequire
                                } else {
                                    ImportKind::EsmImport
                                },
                                import_attributes_resolution_mode(arena, import_decl.attributes),
                            ));
                        } else if let Some(spec_text) =
                            extract_require_specifier(arena, import_decl.module_specifier)
                        {
                            specifiers.push((
                                spec_text,
                                import_decl.module_specifier,
                                ImportKind::CjsRequire,
                                import_attributes_resolution_mode(arena, import_decl.attributes),
                            ));
                        }
                    }

                    if let Some(export_decl) = arena.get_export_decl(inner_stmt) {
                        if export_decl.export_clause.is_some()
                            && let Some(import_decl) =
                                arena.get_import_decl_at(export_decl.export_clause)
                        {
                            let import_kind =
                                if arena.get(export_decl.export_clause).is_some_and(|node| {
                                    node.kind
                                        == tsz::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                                }) {
                                    ImportKind::CjsRequire
                                } else {
                                    ImportKind::EsmReExport
                                };
                            if let Some(text) = arena.get_literal_text(import_decl.module_specifier)
                            {
                                specifiers.push((
                                    strip_quotes(text),
                                    import_decl.module_specifier,
                                    import_kind,
                                    import_attributes_resolution_mode(
                                        arena,
                                        export_decl.attributes,
                                    ),
                                ));
                            } else if let Some(spec_text) =
                                extract_require_specifier(arena, import_decl.module_specifier)
                            {
                                specifiers.push((
                                    spec_text,
                                    import_decl.module_specifier,
                                    ImportKind::CjsRequire,
                                    import_attributes_resolution_mode(
                                        arena,
                                        export_decl.attributes,
                                    ),
                                ));
                            }
                        } else if let Some(text) =
                            arena.get_literal_text(export_decl.module_specifier)
                        {
                            specifiers.push((
                                strip_quotes(text),
                                export_decl.module_specifier,
                                ImportKind::EsmReExport,
                                import_attributes_resolution_mode(arena, export_decl.attributes),
                            ));
                        }
                    }
                }
            }
        }
    }

    collect_non_static_module_specifiers(
        arena,
        !source.is_declaration_file,
        &strip_quotes,
        &mut specifiers,
    );

    specifiers
}

fn leftmost_import_type_call(arena: &NodeArena, mut idx: NodeIndex) -> Option<NodeIndex> {
    use tsz::parser::syntax_kind_ext;
    use tsz::scanner::SyntaxKind;

    for _ in 0..64 {
        let node = arena.get(idx)?;
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = arena.get_qualified_name(node)?;
            idx = qn.left;
            continue;
        }
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = arena.get_call_expr(node)?;
        let callee = arena.get(call.expression)?;
        return (callee.kind == SyntaxKind::ImportKeyword as u16).then_some(idx);
    }
    None
}

fn collect_non_static_module_specifiers(
    arena: &NodeArena,
    include_runtime_specifiers: bool,
    strip_quotes: &dyn Fn(&str) -> String,
    specifiers: &mut Vec<CollectedModuleSpecifier>,
) {
    use tsz::module_resolver::ImportKind;
    use tsz::parser::syntax_kind_ext;

    let mut dynamic_imports = Vec::new();
    let mut commonjs_requires = Vec::new();
    let mut import_types = Vec::new();

    let mut push_import_type_specifier = |call_idx: NodeIndex| {
        let Some(call_node) = arena.get(call_idx) else {
            return;
        };
        let Some(call) = arena.get_call_expr(call_node) else {
            return;
        };
        let Some(args) = call.arguments.as_ref() else {
            return;
        };
        let Some(&arg_idx) = args.nodes.first() else {
            return;
        };
        if let Some(text) = arena.get_literal_text(arg_idx) {
            import_types.push((
                strip_quotes(text),
                arg_idx,
                ImportKind::EsmImport,
                import_type_resolution_mode_override(arena, call),
            ));
        }
    };

    for i in 0..arena.nodes.len() {
        let node = &arena.nodes[i];
        match node.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION && include_runtime_specifiers => {
                let idx = NodeIndex(i as u32);
                if let Some(call) = arena.get_call_expr(node)
                    && is_dynamic_import_callee(arena, call.expression)
                    && let Some(args) = call.arguments.as_ref()
                    && let Some(&arg_idx) = args.nodes.first()
                    && arg_idx.is_some()
                    && let Some(text) = arena.get_literal_text(arg_idx)
                {
                    dynamic_imports.push((
                        strip_quotes(text),
                        arg_idx,
                        ImportKind::DynamicImport,
                        None,
                    ));
                }

                if let Some(specifier) = extract_require_specifier(arena, idx) {
                    commonjs_requires.push((specifier, idx, ImportKind::CjsRequire, None));
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = arena.get_type_ref(node) else {
                    continue;
                };
                let Some(call_idx) = leftmost_import_type_call(arena, type_ref.type_name) else {
                    continue;
                };
                push_import_type_specifier(call_idx);
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                let Some(type_query) = arena.get_type_query(node) else {
                    continue;
                };
                let Some(call_idx) = leftmost_import_type_call(arena, type_query.expr_name) else {
                    continue;
                };
                push_import_type_specifier(call_idx);
            }
            _ => {}
        }
    }

    specifiers.extend(dynamic_imports);
    specifiers.extend(commonjs_requires);
    specifiers.extend(import_types);
}

fn is_dynamic_import_callee(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };
    if node.kind == SyntaxKind::ImportKeyword as u16 {
        return true;
    }

    let Some(access) = arena.get_access_expr(node) else {
        return false;
    };
    let Some(base_node) = arena.get(access.expression) else {
        return false;
    };
    if base_node.kind != SyntaxKind::ImportKeyword as u16 {
        return false;
    }

    arena
        .get_identifier_at(access.name_or_argument)
        .is_some_and(|ident| ident.escaped_text == "defer")
}

fn import_type_resolution_mode_override(
    arena: &NodeArena,
    call: &tsz_parser::parser::node::CallExprData,
) -> Option<tsz::module_resolver::ImportingModuleKind> {
    use tsz::module_resolver::ImportingModuleKind;
    use tsz::parser::syntax_kind_ext;

    fn object_literal_property_initializer_by_name(
        arena: &NodeArena,
        object_idx: NodeIndex,
        name: &str,
    ) -> Option<NodeIndex> {
        let object_node = arena.get(object_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        for child_idx in arena.get_children(object_idx) {
            let child_node = arena.get(child_idx)?;
            if child_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let prop = arena.get_property_assignment(child_node)?;
            let prop_name = if let Some(name_node) = arena.get(prop.name) {
                if let Some(ident) = arena.get_identifier(name_node) {
                    ident.escaped_text.as_str()
                } else if let Some(text) = arena.get_literal_text(prop.name) {
                    text.trim_matches('"').trim_matches('\'')
                } else {
                    continue;
                }
            } else {
                continue;
            };

            if prop_name == name {
                return Some(prop.initializer);
            }
        }

        None
    }

    let args = call.arguments.as_ref()?.nodes.as_slice();
    let &options_idx = args.get(1)?;
    let with_idx = object_literal_property_initializer_by_name(arena, options_idx, "with")?;
    let resolution_mode_idx =
        object_literal_property_initializer_by_name(arena, with_idx, "resolution-mode")?;
    let value_text = arena.get_literal_text(resolution_mode_idx)?;
    match value_text.trim_matches('"').trim_matches('\'') {
        "import" => Some(ImportingModuleKind::Esm),
        "require" => Some(ImportingModuleKind::CommonJs),
        _ => None,
    }
}

fn import_attributes_resolution_mode(
    arena: &NodeArena,
    attributes_idx: NodeIndex,
) -> Option<tsz::module_resolver::ImportingModuleKind> {
    use tsz::module_resolver::ImportingModuleKind;
    use tsz::parser::syntax_kind_ext;

    let attr_node = arena.get(attributes_idx)?;
    let attrs = arena.get_import_attributes_data(attr_node)?;

    for &elem_idx in &attrs.elements.nodes {
        let elem_node = match arena.get(elem_idx) {
            Some(node) if node.kind == syntax_kind_ext::IMPORT_ATTRIBUTE => node,
            _ => continue,
        };
        let attr = match arena.get_import_attribute_data(elem_node) {
            Some(attr) => attr,
            None => continue,
        };
        let name_node = match arena.get(attr.name) {
            Some(node) => node,
            None => continue,
        };

        let name = if let Some(ident) = arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(lit) = arena.get_literal_text(attr.name) {
            lit.trim_matches('"').trim_matches('\'')
        } else {
            continue;
        };

        if name != "resolution-mode" {
            continue;
        }

        let value_text = arena.get_literal_text(attr.value)?;
        return match value_text.trim_matches('"').trim_matches('\'') {
            "import" => Some(ImportingModuleKind::Esm),
            "require" => Some(ImportingModuleKind::CommonJs),
            _ => None,
        };
    }

    None
}

fn import_attributes_has_type_json(arena: &NodeArena, attributes_idx: NodeIndex) -> bool {
    use tsz::parser::syntax_kind_ext;

    let Some(attr_node) = arena.get(attributes_idx) else {
        return false;
    };
    let Some(attrs) = arena.get_import_attributes_data(attr_node) else {
        return false;
    };

    attrs.elements.nodes.iter().any(|&elem_idx| {
        let Some(elem_node) = arena.get(elem_idx) else {
            return false;
        };
        if elem_node.kind != syntax_kind_ext::IMPORT_ATTRIBUTE {
            return false;
        }
        let Some(attr) = arena.get_import_attribute_data(elem_node) else {
            return false;
        };

        let name_is_type = if let Some(ident) = arena
            .get(attr.name)
            .and_then(|name_node| arena.get_identifier(name_node))
        {
            ident.escaped_text.as_str() == "type"
        } else {
            arena
                .get_literal_text(attr.name)
                .is_some_and(|text| text.trim_matches('"').trim_matches('\'') == "type")
        };
        let value_is_json = arena
            .get_literal_text(attr.value)
            .is_some_and(|text| text.trim_matches('"').trim_matches('\'') == "json");

        name_is_type && value_is_json
    })
}

pub(crate) fn module_specifier_has_type_json_import_attribute(
    arena: &NodeArena,
    specifier_idx: NodeIndex,
) -> bool {
    let Some(parent_idx) = arena.parent_of(specifier_idx) else {
        return false;
    };
    let Some(parent_node) = arena.get(parent_idx) else {
        return false;
    };

    if let Some(import_decl) = arena.get_import_decl(parent_node)
        && import_decl.module_specifier == specifier_idx
    {
        return import_attributes_has_type_json(arena, import_decl.attributes);
    }

    if let Some(export_decl) = arena.get_export_decl(parent_node)
        && export_decl.module_specifier == specifier_idx
    {
        return import_attributes_has_type_json(arena, export_decl.attributes);
    }

    false
}

pub(crate) fn json_type_attribute_enables_json_module(
    options: &ResolvedCompilerOptions,
    containing_file: &Path,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
) -> bool {
    matches!(
        options.checker.module,
        ModuleKind::Node18 | ModuleKind::Node20 | ModuleKind::NodeNext
    ) && implied_resolution_mode_for_file_with_cache(containing_file, base_dir, resolution_cache)
        == "import"
}

/// Extract module specifier from a `require()` call expression
/// e.g., `require('./module')` -> `./module` (without quotes)
fn extract_require_specifier(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    use tsz::parser::syntax_kind_ext;

    let node = arena.get(idx)?;

    // Helper to strip surrounding quotes from a string
    let strip_quotes =
        |s: &str| -> String { s.trim_matches(|c| c == '"' || c == '\'').to_string() };

    // Check if it's a require() call expression
    if node.kind != syntax_kind_ext::CALL_EXPRESSION {
        return None;
    }

    let call = arena.get_call_expr(node)?;

    // Check that the callee is 'require' (an identifier)
    let callee_node = arena.get(call.expression)?;
    if !callee_node.is_identifier() {
        return None;
    }
    let callee_text = arena.get_identifier_text(call.expression)?;
    if callee_text != "require" {
        return None;
    }

    // Get the first argument (the module specifier)
    let args = call.arguments.as_ref()?;
    let arg_idx = args.nodes.first()?;
    if arg_idx.is_none() {
        return None;
    }

    // Get the literal text of the argument (without quotes)
    arena.get_literal_text(*arg_idx).map(strip_quotes)
}

pub(crate) fn collect_import_bindings(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<(String, Vec<String>)> {
    let mut bindings = Vec::new();
    let Some(source) = arena.get_source_file_at(source_file) else {
        return bindings;
    };

    for &stmt_idx in &source.statements.nodes {
        if stmt_idx.is_none() {
            continue;
        }
        let Some(import_decl) = arena.get_import_decl_at(stmt_idx) else {
            continue;
        };
        let Some(specifier) = arena.get_literal_text(import_decl.module_specifier) else {
            continue;
        };
        let local_names = collect_import_local_names(arena, import_decl);
        if !local_names.is_empty() {
            bindings.push((specifier.to_string(), local_names));
        }
    }

    bindings
}

pub(crate) fn collect_export_binding_nodes(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<(String, Vec<NodeIndex>)> {
    let mut bindings = Vec::new();
    let Some(source) = arena.get_source_file_at(source_file) else {
        return bindings;
    };

    for &stmt_idx in &source.statements.nodes {
        if stmt_idx.is_none() {
            continue;
        }
        let Some(export_decl) = arena.get_export_decl_at(stmt_idx) else {
            continue;
        };
        if export_decl.export_clause.is_none() {
            continue;
        }
        let clause_idx = export_decl.export_clause;
        let Some(clause_node) = arena.get(clause_idx) else {
            continue;
        };

        let import_decl = arena.get_import_decl(clause_node);
        let mut specifier = arena
            .get_literal_text(export_decl.module_specifier)
            .map(std::string::ToString::to_string);
        if specifier.is_none()
            && let Some(import_decl) = import_decl
            && let Some(text) = arena.get_literal_text(import_decl.module_specifier)
        {
            specifier = Some(text.to_string());
        }
        let Some(specifier) = specifier else {
            continue;
        };

        let mut nodes = Vec::new();
        if import_decl.is_some() {
            nodes.push(clause_idx);
        } else if let Some(named) = arena.get_named_imports(clause_node) {
            for &spec_idx in &named.elements.nodes {
                if spec_idx.is_some() {
                    nodes.push(spec_idx);
                }
            }
        } else if arena.get_identifier_text(clause_idx).is_some() {
            nodes.push(clause_idx);
        }

        if !nodes.is_empty() {
            bindings.push((specifier.to_string(), nodes));
        }
    }

    bindings
}

pub(crate) fn collect_star_export_specifiers(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<String> {
    let mut specifiers = Vec::new();
    let Some(source) = arena.get_source_file_at(source_file) else {
        return specifiers;
    };

    for &stmt_idx in &source.statements.nodes {
        if stmt_idx.is_none() {
            continue;
        }
        let Some(export_decl) = arena.get_export_decl_at(stmt_idx) else {
            continue;
        };
        if export_decl.export_clause.is_some() {
            continue;
        }
        if let Some(text) = arena.get_literal_text(export_decl.module_specifier) {
            specifiers.push(text.to_string());
        }
    }

    specifiers
}

fn collect_import_local_names(
    arena: &NodeArena,
    import_decl: &tsz::parser::node::ImportDeclData,
) -> Vec<String> {
    let mut names = Vec::new();
    if import_decl.import_clause.is_none() {
        return names;
    }

    let clause_idx = import_decl.import_clause;
    if let Some(clause_node) = arena.get(clause_idx) {
        if let Some(clause) = arena.get_import_clause(clause_node) {
            if clause.name.is_some()
                && let Some(name) = arena.get_identifier_text(clause.name)
            {
                names.push(name.to_string());
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = arena.get(clause.named_bindings)
            {
                if bindings_node.is_identifier() {
                    if let Some(name) = arena.get_identifier_text(clause.named_bindings) {
                        names.push(name.to_string());
                    }
                } else if let Some(named) = arena.get_named_imports(bindings_node) {
                    if named.name.is_some()
                        && let Some(name) = arena.get_identifier_text(named.name)
                    {
                        names.push(name.to_string());
                    }
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec) = arena.get_specifier_at(spec_idx) else {
                            continue;
                        };
                        let local_ident = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        if let Some(name) = arena.get_identifier_text(local_ident) {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        } else if let Some(name) = arena.get_identifier_text(clause_idx) {
            names.push(name.to_string());
        }
    } else if let Some(name) = arena.get_identifier_text(clause_idx) {
        names.push(name.to_string());
    }

    names
}
