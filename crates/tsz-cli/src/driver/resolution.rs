use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::config::{ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
use crate::fs::{is_valid_module_file, is_valid_module_or_js_file};
use tsz::emitter::ModuleKind;
use tsz::module_resolver::{PackageType, is_path_relative};
use tsz::parser::NodeIndex;
use tsz::parser::ParserState;
use tsz::parser::node::{NodeAccess, NodeArena};
use tsz::scanner::SyntaxKind;

#[derive(Default)]
pub(crate) struct ModuleResolutionCache {
    package_type_by_dir: FxHashMap<PathBuf, Option<PackageType>>,
}

impl ModuleResolutionCache {
    fn package_type_for_dir(&mut self, dir: &Path, base_dir: &Path) -> Option<PackageType> {
        let mut current = dir;
        let mut visited = Vec::new();

        loop {
            if let Some(value) = self.package_type_by_dir.get(current).copied() {
                for path in visited {
                    self.package_type_by_dir.insert(path, value);
                }
                return value;
            }

            visited.push(current.to_path_buf());

            if let Some(package_json) = read_package_json(&current.join("package.json")) {
                let value = package_type_from_json(Some(&package_json));
                for path in visited {
                    self.package_type_by_dir.insert(path, value);
                }
                return value;
            }

            if current == base_dir {
                for path in visited {
                    self.package_type_by_dir.insert(path, None);
                }
                return None;
            }

            let Some(parent) = current.parent() else {
                for path in visited {
                    self.package_type_by_dir.insert(path, None);
                }
                return None;
            };
            current = parent;
        }
    }
}

pub(crate) fn resolve_type_package_from_roots(
    name: &str,
    roots: &[PathBuf],
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    for root in roots {
        let candidates = type_package_candidates_for_root(name, root);
        if candidates.is_empty() {
            continue;
        }
        for candidate in &candidates {
            let package_root = root.join(candidate);
            if package_root.is_dir()
                && let Some(entry) = resolve_type_package_entry(&package_root, options)
            {
                return Some(entry);
            }

            if let Some(entry) = resolve_declaration_package_entry(root, candidate, options, None) {
                return Some(entry);
            }
        }
    }

    None
}

/// Resolve a `/// <reference types="..." />` directive by searching `node_modules/`
/// directories walking up from the source file. This is the fallback used in
/// Node16/NodeNext/Bundler module resolution when type roots don't contain the package.
///
/// In tsc, `resolveTypeReferenceDirective` uses the regular module resolution algorithm
/// as a fallback after checking type roots. This means packages in `node_modules/`
/// (not just `node_modules/@types/`) can be found via triple-slash type references.
///
/// The resolution mode is determined by either:
/// - The explicit `resolution-mode` attribute (if present)
/// - The source file's module format (CJS → `require`, ESM → `import`)
pub(crate) fn resolve_type_reference_from_node_modules(
    name: &str,
    from_file: &Path,
    base_dir: &Path,
    resolution_mode: Option<&str>,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    // Determine effective resolution mode from explicit attribute or file format
    let effective_mode = resolution_mode
        .map(String::from)
        .unwrap_or_else(|| implied_resolution_mode_for_file(from_file, base_dir));

    // Generate all candidate package names (original + @types mangled form)
    let candidates = type_package_candidates(name);

    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        let node_modules = current.join("node_modules");
        for candidate in &candidates {
            let package_root = node_modules.join(candidate);
            if package_root.is_dir() {
                let resolved =
                    resolve_type_package_entry_with_mode(&package_root, &effective_mode, options);
                if resolved.is_some() {
                    return resolved;
                }
                // Fall back to non-conditional resolution (types/typings/main/index)
                let resolved = resolve_type_package_entry(&package_root, options);
                if resolved.is_some() {
                    return resolved;
                }
            }
        }

        if current == base_dir {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    None
}

/// Determine the implied resolution mode ("import" or "require") for a file
/// based on its extension and nearest `package.json` `type` field.
///
/// In Node16/NodeNext:
/// - `.mts`/`.mjs` files → ESM → "import"
/// - `.cts`/`.cjs` files → CJS → "require"
/// - `.ts`/`.tsx`/`.js`/`.jsx` files → depends on nearest `package.json`:
///   - `"type": "module"` → "import"
///   - otherwise → "require"
pub(super) fn implied_resolution_mode_for_file(file: &Path, base_dir: &Path) -> String {
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "mts" | "mjs" => return "import".to_string(),
        "cts" | "cjs" => return "require".to_string(),
        _ => {}
    }

    // Walk up from the file to find the nearest package.json with "type" field
    let mut current = file.parent().unwrap_or(base_dir);
    loop {
        let pkg_json_path = current.join("package.json");
        if pkg_json_path.is_file()
            && let Some(pj) = read_package_json(&pkg_json_path)
        {
            if pj.package_type.as_deref() == Some("module") {
                return "import".to_string();
            }
            // Found a package.json without "type": "module" → CJS
            return "require".to_string();
        }
        if current == base_dir {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    // Default to require (CJS) when no package.json is found
    "require".to_string()
}

/// Public wrapper for `type_package_candidates`.
pub(crate) fn type_package_candidates_pub(name: &str) -> Vec<String> {
    type_package_candidates(name)
}

fn type_package_candidates_for_root(name: &str, root: &Path) -> Vec<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let normalized = trimmed.replace('\\', "/");
    let mut candidates = Vec::new();
    let is_at_types_root = root.file_name().and_then(|name| name.to_str()) == Some("@types");

    if let Some(stripped) = normalized.strip_prefix("@types/")
        && !stripped.is_empty()
    {
        candidates.push(stripped.to_string());
    }

    if let Some(stripped) = normalized.strip_prefix('@')
        && !normalized.starts_with("@types/")
        && let Some((scope, pkg)) = stripped.split_once('/')
        && !scope.is_empty()
        && !pkg.is_empty()
    {
        let mangled = format!("{scope}__{pkg}");
        if is_at_types_root {
            candidates.push(mangled);
        } else {
            candidates.push(normalized.clone());
        }
        return candidates;
    }

    if !normalized.starts_with('@') && !normalized.contains('/') {
        let at_types = format!("@types/{normalized}");
        if !candidates.iter().any(|v| v == &at_types) {
            candidates.push(at_types);
        }
    }

    if !candidates.iter().any(|value| value == &normalized) {
        candidates.push(normalized);
    }

    candidates
}

fn type_package_candidates(name: &str) -> Vec<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let normalized = trimmed.replace('\\', "/");
    let mut candidates = Vec::new();

    if let Some(stripped) = normalized.strip_prefix("@types/")
        && !stripped.is_empty()
    {
        candidates.push(stripped.to_string());
    }

    // Scoped package mangling: @scope/name → @types/scope__name
    // tsc resolves `/// <reference types="@scope/name" />` by checking both
    // the original scoped path and the @types-mangled equivalent.
    if let Some(stripped) = normalized.strip_prefix('@')
        && !normalized.starts_with("@types/")
        && let Some((scope, pkg)) = stripped.split_once('/')
        && !scope.is_empty()
        && !pkg.is_empty()
    {
        let plain_mangled = format!("{scope}__{pkg}");
        candidates.push(plain_mangled);
        candidates.push(format!("@types/@{scope}/{pkg}"));
        let mangled = format!("@types/{scope}__{pkg}");
        candidates.push(mangled);
    }

    // For bare (non-scoped) package names, also check @types/<name>.
    // tsc's resolveTypeReferenceDirective checks both node_modules/<name>/
    // and node_modules/@types/<name>/ during the walk-up.
    if !normalized.starts_with('@') && !normalized.contains('/') {
        let at_types = format!("@types/{normalized}");
        if !candidates.iter().any(|v| v == &at_types) {
            candidates.push(at_types);
        }
    }

    if !candidates.iter().any(|value| value == &normalized) {
        candidates.push(normalized);
    }

    candidates
}

pub(crate) fn collect_type_packages_from_root(root: &Path) -> Vec<PathBuf> {
    let mut packages = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return packages,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if name.starts_with('@') {
            if let Ok(scope_entries) = std::fs::read_dir(&path) {
                for scope_entry in scope_entries.flatten() {
                    let scope_path = scope_entry.path();
                    if scope_path.is_dir() {
                        packages.push(scope_path);
                    }
                }
            }
            continue;
        }
        packages.push(path);
    }

    packages
}

pub(crate) fn resolve_type_package_entry(
    package_root: &Path,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let package_json = read_package_json(&package_root.join("package.json"));

    // In node10/classic module resolution, type package fallback resolution
    // should NOT try .d.mts/.d.cts extensions (those require exports map).
    // Only bundler/node16/nodenext try the full extension set.
    let use_restricted_extensions = matches!(
        options.effective_module_resolution(),
        ModuleResolutionKind::Node | ModuleResolutionKind::Classic
    );

    if use_restricted_extensions {
        // Use restricted resolution: only types/typings/main + index.d.ts fallback
        let mut candidates = Vec::new();
        if let Some(ref pj) = package_json {
            candidates = collect_package_entry_candidates(pj);
        }
        if !candidates
            .iter()
            .any(|entry| entry == "index" || entry == "./index")
        {
            candidates.push("index".to_string());
        }
        // Only try .ts, .tsx, .d.ts extensions (no .d.mts/.d.cts)
        let restricted_extensions = &["ts", "tsx", "d.ts"];
        for entry_name in candidates {
            let entry_name = entry_name.trim().trim_start_matches("./");
            let path = package_root.join(entry_name);
            for ext in restricted_extensions {
                let candidate = path.with_extension(ext);
                if candidate.is_file() && is_declaration_file(&candidate) {
                    return Some(normalize_resolved_path(&candidate, options));
                }
            }
        }
        None
    } else {
        // For bundler/node16/nodenext, use resolve_package_specifier which respects
        // the exports map. This is needed for type packages that use conditional exports
        // (e.g. `"exports": { ".": { "import": "./index.d.mts", "require": "./index.d.cts" } }`)
        let conditions = export_conditions(options);
        let resolved = resolve_package_specifier(
            package_root,
            None,
            package_json.as_ref(),
            &conditions,
            options,
        )?;
        is_declaration_file(&resolved).then_some(resolved)
    }
}

/// Resolve a type package entry using a specific resolution-mode condition.
///
/// When `resolution_mode` is "import" or "require", the exports map is consulted
/// with the corresponding condition. This implements the `resolution-mode` attribute
/// of `/// <reference types="..." resolution-mode="..." />` directives.
pub(crate) fn resolve_type_package_entry_with_mode(
    package_root: &Path,
    resolution_mode: &str,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let package_json = read_package_json(&package_root.join("package.json"));
    let package_json = package_json.as_ref()?;

    // Build conditions based on resolution mode
    let conditions: Vec<&str> = match resolution_mode {
        "require" => vec!["require", "types", "default"],
        "import" => vec!["import", "types", "default"],
        _ => return None,
    };

    // Try the exports map first
    if let Some(exports) = &package_json.exports
        && let Some(target) = resolve_exports_subpath(exports, ".", &conditions)
    {
        let target_path = package_root.join(target.trim_start_matches("./"));
        // Try to find a declaration file at the target
        let package_type = package_type_from_json(Some(package_json));
        for candidate in expand_module_path_candidates(&target_path, options, package_type) {
            if candidate.is_file() && is_declaration_file(&candidate) {
                return Some(normalize_resolved_path(&candidate, options));
            }
        }
        // Try exact path
        if target_path.is_file() && is_declaration_file(&target_path) {
            return Some(normalize_resolved_path(&target_path, options));
        }
    }

    None
}

pub(crate) fn default_type_roots(base_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = FxHashSet::default();
    let mut current = Some(base_dir.to_path_buf());

    while let Some(dir) = current {
        let candidate = dir.join("node_modules").join("@types");
        if candidate.is_dir() {
            let canonical = canonicalize_or_owned(&candidate);
            if seen.insert(canonical.clone()) {
                roots.push(canonical);
            }
        }
        current = dir.parent().map(Path::to_path_buf);
    }

    roots
}

pub(crate) fn collect_module_specifiers_from_text(path: &Path, text: &str) -> Vec<String> {
    // Fast path: skip the full parse if the text cannot contain any module specifiers.
    // This avoids a redundant parse for files that will be parsed again in build_program.
    if !text_may_contain_module_specifiers(text) {
        return Vec::new();
    }
    let file_name = path.to_string_lossy().into_owned();
    let mut parser = ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    collect_module_specifiers(&arena, source_file)
        .into_iter()
        .map(|(specifier, _, _, _)| specifier)
        .collect()
}

/// Quick text scan to determine if a source file might contain module specifiers.
/// Returns false only when we can guarantee there are no imports/exports/requires.
fn text_may_contain_module_specifiers(text: &str) -> bool {
    // All module specifier patterns require at least one of these keywords:
    // - `import` for ES imports and dynamic import()
    // - `require(` for CommonJS require calls
    // - `from '` or `from "` for re-exports like `export { x } from 'y'`
    // - `declare module` for ambient module declarations
    text.contains("import")
        || text.contains("require(")
        || text.contains("from '")
        || text.contains("from \"")
        || text.contains("declare module")
}

pub(crate) fn collect_module_specifiers(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<(
    String,
    NodeIndex,
    tsz::module_resolver::ImportKind,
    Option<tsz::module_resolver::ImportingModuleKind>,
)> {
    use tsz::module_resolver::ImportKind;
    let mut specifiers = Vec::new();

    let Some(source) = arena.get_source_file_at(source_file) else {
        return specifiers;
    };

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
            if let Some(text) = arena.get_literal_text(export_decl.module_specifier) {
                specifiers.push((
                    strip_quotes(text),
                    export_decl.module_specifier,
                    ImportKind::EsmReExport,
                    import_attributes_resolution_mode(arena, export_decl.attributes),
                ));
            } else if export_decl.export_clause.is_some()
                && let Some(import_decl) = arena.get_import_decl_at(export_decl.export_clause)
                && let Some(text) = arena.get_literal_text(import_decl.module_specifier)
            {
                specifiers.push((
                    strip_quotes(text),
                    import_decl.module_specifier,
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
                specifiers.push((
                    strip_quotes(text),
                    module_decl.name,
                    ImportKind::EsmImport,
                    None,
                ));
            }
        }
    }

    // Also collect dynamic imports and plain CommonJS require() calls from
    // expression/call sites so dependency discovery follows the same module
    // graph that checker-side call typing uses.
    // Skip for declaration files (.d.ts) — they cannot contain runtime
    // expressions like import() or require(), and scanning all nodes in
    // large lib files (e.g. dom.d.ts with ~40K nodes) is wasted work.
    if !source.is_declaration_file {
        collect_dynamic_imports(arena, source_file, &strip_quotes, &mut specifiers);
        collect_commonjs_requires(arena, &mut specifiers);
    }

    collect_import_type_specifiers(arena, &strip_quotes, &mut specifiers);

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

fn collect_import_type_specifiers(
    arena: &NodeArena,
    strip_quotes: &dyn Fn(&str) -> String,
    specifiers: &mut Vec<(
        String,
        NodeIndex,
        tsz::module_resolver::ImportKind,
        Option<tsz::module_resolver::ImportingModuleKind>,
    )>,
) {
    use tsz::module_resolver::ImportKind;
    use tsz::parser::syntax_kind_ext;

    for i in 0..arena.nodes.len() {
        let node = &arena.nodes[i];
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            continue;
        }
        let Some(type_ref) = arena.get_type_ref(node) else {
            continue;
        };
        let Some(call_idx) = leftmost_import_type_call(arena, type_ref.type_name) else {
            continue;
        };
        let Some(call_node) = arena.get(call_idx) else {
            continue;
        };
        let Some(call) = arena.get_call_expr(call_node) else {
            continue;
        };
        let Some(args) = call.arguments.as_ref() else {
            continue;
        };
        let Some(&arg_idx) = args.nodes.first() else {
            continue;
        };
        if let Some(text) = arena.get_literal_text(arg_idx) {
            specifiers.push((strip_quotes(text), arg_idx, ImportKind::EsmImport, None));
        }
    }
}

/// Collect dynamic `import()` expressions from the AST
fn collect_dynamic_imports(
    arena: &NodeArena,
    _source_file: NodeIndex,
    strip_quotes: &dyn Fn(&str) -> String,
    specifiers: &mut Vec<(
        String,
        NodeIndex,
        tsz::module_resolver::ImportKind,
        Option<tsz::module_resolver::ImportingModuleKind>,
    )>,
) {
    use tsz::parser::syntax_kind_ext;
    use tsz::scanner::SyntaxKind;

    // Iterate all nodes looking for CallExpression with ImportKeyword callee
    for i in 0..arena.nodes.len() {
        let node = &arena.nodes[i];
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            continue;
        }
        let Some(call) = arena.get_call_expr(node) else {
            continue;
        };
        // Check if the callee is an ImportKeyword (dynamic import)
        let Some(callee) = arena.get(call.expression) else {
            continue;
        };
        if callee.kind != SyntaxKind::ImportKeyword as u16 {
            continue;
        }
        // Get the first argument (the module specifier)
        let Some(args) = call.arguments.as_ref() else {
            continue;
        };
        let Some(&arg_idx) = args.nodes.first() else {
            continue;
        };
        if arg_idx.is_none() {
            continue;
        }
        if let Some(text) = arena.get_literal_text(arg_idx) {
            specifiers.push((
                strip_quotes(text),
                arg_idx,
                tsz::module_resolver::ImportKind::DynamicImport,
                None,
            ));
        }
    }
}

fn collect_commonjs_requires(
    arena: &NodeArena,
    specifiers: &mut Vec<(
        String,
        NodeIndex,
        tsz::module_resolver::ImportKind,
        Option<tsz::module_resolver::ImportingModuleKind>,
    )>,
) {
    use tsz::parser::syntax_kind_ext;
    for i in 0..arena.nodes.len() {
        // Only check call expressions — skip all other node kinds to avoid
        // incorrectly treating numeric/string literals as module specifiers.
        if arena.nodes[i].kind != syntax_kind_ext::CALL_EXPRESSION {
            continue;
        }
        let idx = NodeIndex(i as u32);
        if let Some(specifier) = extract_require_specifier(arena, idx) {
            specifiers.push((
                specifier,
                idx,
                tsz::module_resolver::ImportKind::CjsRequire,
                None,
            ));
        }
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

/// Extract module specifier from a `require()` call expression
/// e.g., `require('./module')` -> `./module` (without quotes)
fn extract_require_specifier(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    use tsz::parser::syntax_kind_ext;
    use tsz::scanner::SyntaxKind;

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
    if callee_node.kind != SyntaxKind::Identifier as u16 {
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
                if bindings_node.kind == SyntaxKind::Identifier as u16 {
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

pub(crate) fn resolve_module_specifier(
    from_file: &Path,
    module_specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    known_files: &FxHashSet<PathBuf>,
) -> Option<PathBuf> {
    let debug = std::env::var_os("TSZ_DEBUG_RESOLVE").is_some();
    if debug {
        tracing::debug!(
            "resolve_module_specifier: from_file={from_file:?}, specifier={module_specifier:?}, resolution={:?}, base_url={:?}",
            options.effective_module_resolution(),
            options.base_url
        );
    }
    let specifier = module_specifier.trim();
    if specifier.is_empty() {
        return None;
    }
    let specifier = specifier.replace('\\', "/");
    if specifier.starts_with('#') {
        if is_invalid_package_import_specifier(&specifier) {
            return None;
        }
        if options.resolve_package_json_imports {
            return resolve_package_imports_specifier(from_file, &specifier, base_dir, options);
        }
        return None;
    }
    let resolution = options.effective_module_resolution();
    let mut candidates = Vec::new();

    let from_dir = from_file.parent().unwrap_or(base_dir);
    let package_type = match resolution {
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
            resolution_cache.package_type_for_dir(from_dir, base_dir)
        }
        _ => None,
    };

    let mut allow_node_modules = false;
    let mut path_mapping_attempted = false;

    if Path::new(&specifier).is_absolute() {
        candidates.extend(expand_module_path_candidates(
            &PathBuf::from(specifier.as_str()),
            options,
            package_type,
        ));
    } else if is_path_relative(&specifier) {
        let joined = from_dir.join(&specifier);
        candidates.extend(expand_module_path_candidates(
            &joined,
            options,
            package_type,
        ));
    } else if matches!(resolution, ModuleResolutionKind::Classic) {
        if options.base_url.is_some()
            && let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) = select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            let base = options.base_url.as_ref().expect("baseUrl present");
            for target in &mapping.targets {
                let substituted = substitute_path_target(target, &wildcard);
                let path = if Path::new(&substituted).is_absolute() {
                    PathBuf::from(substituted)
                } else {
                    base.join(substituted)
                };
                candidates.extend(expand_module_path_candidates(&path, options, package_type));
            }
        }

        // Classic resolution always walks up the directory tree from the containing
        // file's directory, probing for <specifier>.ts/.tsx/.d.ts and related candidates.
        // This runs even when baseUrl/path-mapping candidates were generated, matching
        // TypeScript behavior where classic resolution falls back to relative ancestor checks.
        // Unlike Node resolution, Classic resolution walks up for all specifiers including
        // bare module specifiers (e.g., "module3") since it has no node_modules concept.
        {
            let mut current = from_dir.to_path_buf();
            loop {
                candidates.extend(expand_module_path_candidates(
                    &current.join(&specifier),
                    options,
                    package_type,
                ));

                match current.parent() {
                    Some(parent) if parent != current => current = parent.to_path_buf(),
                    _ => break,
                }
            }
        }
    } else if let Some(base_url) = options.base_url.as_ref() {
        allow_node_modules = true;
        if let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) = select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            for target in &mapping.targets {
                let substituted = substitute_path_target(target, &wildcard);
                let path = if Path::new(&substituted).is_absolute() {
                    PathBuf::from(substituted)
                } else {
                    base_url.join(substituted)
                };
                candidates.extend(expand_module_path_candidates(&path, options, package_type));
            }
        }

        if candidates.is_empty() {
            candidates.extend(expand_module_path_candidates(
                &base_url.join(&specifier),
                options,
                package_type,
            ));
        }
    } else {
        allow_node_modules = true;
    }

    for candidate in candidates {
        // Check if candidate exists in known files (for virtual test files) or on filesystem
        let exists = known_files.contains(&candidate)
            || (candidate.is_file() && is_valid_module_or_js_file(&candidate));
        if debug {
            tracing::debug!("candidate={candidate:?} exists={exists}");
        }

        if exists {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    // TypeScript falls through to Classic-style directory walking when path mappings
    // were attempted but did not resolve. This matches behavior where path mapping
    // misses are not treated as terminal failures in classic mode.
    if path_mapping_attempted && matches!(resolution, ModuleResolutionKind::Classic) {
        let mut current = from_dir.to_path_buf();
        loop {
            for candidate in
                expand_module_path_candidates(&current.join(&specifier), options, package_type)
            {
                let exists = known_files.contains(&candidate)
                    || (candidate.is_file() && is_valid_module_or_js_file(&candidate));
                if debug {
                    tracing::debug!("classic-fallback candidate={candidate:?} exists={exists}");
                }
                if exists {
                    return Some(normalize_resolved_path(&candidate, options));
                }
            }

            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }
    }

    if allow_node_modules {
        return resolve_node_module_specifier(from_file, &specifier, base_dir, options);
    }

    None
}

fn select_path_mapping<'a>(
    mappings: &'a [PathMapping],
    specifier: &str,
) -> Option<(&'a PathMapping, String)> {
    let mut best: Option<(&PathMapping, String)> = None;
    let mut best_score = 0usize;
    let mut best_pattern_len = 0usize;

    for mapping in mappings {
        let Some(wildcard) = mapping.match_specifier(specifier) else {
            continue;
        };
        let score = mapping.specificity();
        let pattern_len = mapping.pattern.len();

        let is_better = match &best {
            None => true,
            Some((current, _)) => {
                score > best_score
                    || (score == best_score && pattern_len > best_pattern_len)
                    || (score == best_score
                        && pattern_len == best_pattern_len
                        && mapping.pattern < current.pattern)
            }
        };

        if is_better {
            best_score = score;
            best_pattern_len = pattern_len;
            best = Some((mapping, wildcard));
        }
    }

    best
}

fn substitute_path_target(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

fn expand_module_path_candidates(
    path: &Path,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let base = normalize_path(path);
    let mut default_suffixes: Vec<String> = Vec::new();
    let suffixes = if options.module_suffixes.is_empty() {
        default_suffixes.push(String::new());
        &default_suffixes
    } else {
        &options.module_suffixes
    };
    if let Some((base_no_ext, extension)) = split_path_extension(&base) {
        // Try extension substitution (.js → .ts/.tsx/.d.ts) for all resolution modes.
        // TypeScript resolves `.js` imports to `.ts` sources in all modes.
        let mut candidates = Vec::new();
        if let Some(rewritten) = node16_extension_substitution(&base, extension) {
            for candidate in rewritten {
                candidates.extend(candidates_with_suffixes(&candidate, suffixes));
            }
        }
        // Also include the original extension as fallback
        candidates.extend(candidates_with_suffixes_and_extension(
            &base_no_ext,
            extension,
            suffixes,
        ));
        return candidates;
    }

    let extensions = extension_candidates_for_resolution(options, package_type);
    let mut candidates = Vec::new();
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(&base, ext, suffixes));
    }
    let index = base.join("index");
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, ext, suffixes,
        ));
    }
    candidates
}

fn expand_export_path_candidates(
    path: &Path,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let base = normalize_path(path);
    let suffixes = &options.module_suffixes;
    if let Some((base_no_ext, extension)) = split_path_extension(&base) {
        // Package `exports` targets participate in declaration-sidecar lookup
        // during program discovery. This keeps the driver aligned with the
        // checker `ModuleResolver`, which resolves `./entry.js` to adjacent
        // `./entry.d.ts` / `./entry.d.mts` / `./entry.d.cts` files when those
        // are the type-bearing program inputs.
        let mut candidates = Vec::new();
        if let Some(rewritten) = node16_extension_substitution(&base, extension) {
            for candidate in rewritten {
                candidates.extend(candidates_with_suffixes(&candidate, suffixes));
            }
        }
        candidates.extend(candidates_with_suffixes_and_extension(
            &base_no_ext,
            extension,
            suffixes,
        ));
        return candidates;
    }

    let extensions = extension_candidates_for_resolution(options, package_type);
    let mut candidates = Vec::new();
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(&base, ext, suffixes));
    }
    if options.resolve_json_module {
        candidates.extend(candidates_with_suffixes_and_extension(
            &base, "json", suffixes,
        ));
    }
    let index = base.join("index");
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, ext, suffixes,
        ));
    }
    if options.resolve_json_module {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, "json", suffixes,
        ));
    }
    candidates
}

fn split_path_extension(path: &Path) -> Option<(PathBuf, &'static str)> {
    let path_str = path.to_string_lossy();
    for ext in KNOWN_EXTENSIONS {
        if path_str.ends_with(ext) {
            let base = &path_str[..path_str.len().saturating_sub(ext.len())];
            if base.is_empty() {
                return None;
            }
            return Some((PathBuf::from(base), ext.trim_start_matches('.')));
        }
    }
    None
}

fn candidates_with_suffixes(path: &Path, suffixes: &[String]) -> Vec<PathBuf> {
    let Some((base, extension)) = split_path_extension(path) else {
        return Vec::new();
    };
    candidates_with_suffixes_and_extension(&base, extension, suffixes)
}

fn candidates_with_suffixes_and_extension(
    base: &Path,
    extension: &str,
    suffixes: &[String],
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for suffix in suffixes {
        if let Some(candidate) = path_with_suffix_and_extension(base, suffix, extension) {
            candidates.push(candidate);
        }
    }
    candidates
}

fn path_with_suffix_and_extension(base: &Path, suffix: &str, extension: &str) -> Option<PathBuf> {
    let file_name = base.file_name()?.to_string_lossy();
    let mut candidate = base.to_path_buf();
    let mut new_name = String::with_capacity(file_name.len() + suffix.len() + extension.len() + 1);
    new_name.push_str(&file_name);
    new_name.push_str(suffix);
    new_name.push('.');
    new_name.push_str(extension);
    candidate.set_file_name(new_name);
    Some(candidate)
}

fn node16_extension_substitution(path: &Path, extension: &str) -> Option<Vec<PathBuf>> {
    let replacements: &[&str] = match extension {
        "js" => &["ts", "tsx", "d.ts"],
        "jsx" => &["tsx", "d.ts"],
        "mjs" => &["mts", "d.mts"],
        "cjs" => &["cts", "d.cts"],
        _ => return None,
    };

    Some(
        replacements
            .iter()
            .map(|ext| path.with_extension(ext))
            .collect(),
    )
}

const fn extension_candidates_for_resolution(
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> &'static [&'static str] {
    match options.effective_module_resolution() {
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => match package_type {
            Some(PackageType::Module) => &NODE16_MODULE_EXTENSION_CANDIDATES,
            Some(PackageType::CommonJs) => &NODE16_COMMONJS_EXTENSION_CANDIDATES,
            None => &TS_EXTENSION_CANDIDATES,
        },
        _ => &TS_EXTENSION_CANDIDATES,
    }
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::RootDir
            | std::path::Component::Normal(_)
            | std::path::Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

pub(crate) fn normalize_resolved_path(path: &Path, options: &ResolvedCompilerOptions) -> PathBuf {
    if options.preserve_symlinks {
        normalize_path(path)
    } else {
        canonicalize_or_owned(path)
    }
}

const KNOWN_EXTENSIONS: [&str; 12] = [
    ".d.mts", ".d.cts", ".d.ts", ".mts", ".cts", ".tsx", ".ts", ".mjs", ".cjs", ".jsx", ".js",
    ".json",
];
const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];

const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];

#[derive(Debug, Deserialize)]
struct PackageJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    types: Option<String>,
    #[serde(default)]
    typings: Option<String>,
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default, rename = "type")]
    package_type: Option<String>,
    #[serde(default)]
    exports: Option<serde_json::Value>,
    #[serde(default)]
    imports: Option<serde_json::Value>,
    #[serde(default, rename = "typesVersions")]
    types_versions: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
}

impl SemVer {
    const ZERO: Self = Self {
        major: 0,
        minor: 0,
        patch: 0,
    };
}

// NOTE: Keep this in sync with the TypeScript version this compiler targets.
// TODO: Make this configurable once CLI plumbing is available.
const TYPES_VERSIONS_COMPILER_VERSION_FALLBACK: SemVer = SemVer {
    major: 6,
    minor: 0,
    patch: 0,
};

fn types_versions_compiler_version(options: &ResolvedCompilerOptions) -> SemVer {
    options
        .types_versions_compiler_version
        .as_deref()
        .and_then(parse_semver)
        .unwrap_or_else(default_types_versions_compiler_version)
}

const fn default_types_versions_compiler_version() -> SemVer {
    // Use the fallback version directly since the project's package.json version
    // is not a TypeScript version. The fallback represents the TypeScript version
    // that this compiler is compatible with for typesVersions resolution.
    TYPES_VERSIONS_COMPILER_VERSION_FALLBACK
}

fn export_conditions(options: &ResolvedCompilerOptions) -> Vec<&'static str> {
    let resolution = options.effective_module_resolution();
    let mut conditions = Vec::new();
    push_condition(&mut conditions, "types");

    match resolution {
        ModuleResolutionKind::Bundler => push_condition(&mut conditions, "browser"),
        ModuleResolutionKind::Classic
        | ModuleResolutionKind::Node
        | ModuleResolutionKind::Node16
        | ModuleResolutionKind::NodeNext => {
            push_condition(&mut conditions, "node");
        }
    }

    match options.printer.module {
        ModuleKind::CommonJS | ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System => {
            push_condition(&mut conditions, "require");
        }
        ModuleKind::ES2015
        | ModuleKind::ES2020
        | ModuleKind::ES2022
        | ModuleKind::ESNext
        | ModuleKind::Node16
        | ModuleKind::Node18
        | ModuleKind::Node20
        | ModuleKind::NodeNext => {
            push_condition(&mut conditions, "import");
        }
        _ => {}
    }

    push_condition(&mut conditions, "default");
    match resolution {
        ModuleResolutionKind::Bundler => {
            push_condition(&mut conditions, "import");
            push_condition(&mut conditions, "require");
            push_condition(&mut conditions, "node");
        }
        ModuleResolutionKind::Classic
        | ModuleResolutionKind::Node
        | ModuleResolutionKind::Node16
        | ModuleResolutionKind::NodeNext => {
            push_condition(&mut conditions, "import");
            push_condition(&mut conditions, "require");
            push_condition(&mut conditions, "browser");
        }
    }

    conditions
}

fn push_condition(conditions: &mut Vec<&'static str>, condition: &'static str) {
    if !conditions.contains(&condition) {
        conditions.push(condition);
    }
}

fn resolve_node_module_specifier(
    from_file: &Path,
    module_specifier: &str,
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let (package_name, subpath) = split_package_specifier(module_specifier)?;
    let conditions = export_conditions(options);

    // Self-reference: check if any ancestor package.json has a "name" matching
    // the import specifier. Node.js supports importing your own package by name
    // using the "exports" field in package.json.
    {
        let mut dir = from_file.parent().unwrap_or(base_dir);
        loop {
            let pj_path = dir.join("package.json");
            if pj_path.is_file()
                && let Some(pj) = read_package_json(&pj_path)
            {
                if pj.name.as_deref() == Some(&package_name) {
                    let resolved = resolve_package_specifier(
                        dir,
                        subpath.as_deref(),
                        Some(&pj),
                        &conditions,
                        options,
                    );
                    if resolved.is_some() {
                        return resolved;
                    }

                    // Output-to-source remapping for self-reference imports.
                    // When outDir/declarationDir is set, export map targets point
                    // to the output directory (e.g., "./dist/index.js"). tsc
                    // remaps these back to source files by stripping the output
                    // prefix and substituting output extensions with source
                    // extensions (tryLoadInputFileForPath).
                    if let Some(ref exports) = pj.exports {
                        let subpath_key = match &subpath {
                            Some(value) => format!("./{value}"),
                            None => ".".to_string(),
                        };
                        if let Some(target) =
                            resolve_exports_subpath(exports, &subpath_key, &conditions)
                            && let Some(resolved) =
                                try_remap_output_to_source(dir, &target, from_file, options)
                        {
                            return Some(resolved);
                        }
                    }
                }
                // Stop at the first package.json with a name (that's the package boundary)
                if pj.name.is_some() {
                    break;
                }
            }
            if dir == base_dir {
                break;
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }

    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        // 1. Look for the package itself in node_modules
        let package_root = current.join("node_modules").join(&package_name);
        if package_root.is_dir() {
            let package_json = read_package_json(&package_root.join("package.json"));
            let resolved = resolve_package_specifier(
                &package_root,
                subpath.as_deref(),
                package_json.as_ref(),
                &conditions,
                options,
            );
            if resolved.is_some() {
                return resolved;
            }
        } else if subpath.is_none()
            && options.effective_module_resolution() == ModuleResolutionKind::Bundler
        {
            let candidates = expand_module_path_candidates(&package_root, options, None);
            for candidate in candidates {
                if candidate.is_file() && is_valid_module_or_js_file(&candidate) {
                    return Some(normalize_resolved_path(&candidate, options));
                }
            }
        }

        // 2. Look for @types package (if not already looking for one)
        // TypeScript looks up @types/foo for 'foo', and @types/scope__pkg for '@scope/pkg'
        if !options.checker.no_types_and_symbols && !package_name.starts_with("@types/") {
            let types_package_name = if let Some(scope_pkg) = package_name.strip_prefix('@') {
                // Scoped package: @scope/pkg -> @types/scope__pkg
                // Skip the '@' (1 char) and replace '/' with '__'
                format!("@types/{}", scope_pkg.replace('/', "__"))
            } else {
                format!("@types/{package_name}")
            };

            let types_root = current.join("node_modules").join(&types_package_name);
            if types_root.is_dir() {
                let package_json = read_package_json(&types_root.join("package.json"));
                let resolved = resolve_package_specifier(
                    &types_root,
                    subpath.as_deref(),
                    package_json.as_ref(),
                    &conditions,
                    options,
                );
                if resolved.is_some() {
                    return resolved;
                }
            }
        }

        if current == base_dir {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    // When a package was loaded through `types`/`typeRoots`, TypeScript still
    // treats bare imports from that package as resolved. Mirror that here by
    // consulting the configured type roots for package entrypoints after the
    // normal node_modules walk-up fails.
    if !options.checker.no_types_and_symbols && subpath.is_none() {
        let type_roots = options
            .type_roots
            .clone()
            .unwrap_or_else(|| default_type_roots(base_dir));
        if let Some(resolved) = resolve_type_package_from_roots(&package_name, &type_roots, options)
        {
            return Some(resolved);
        }
    }

    None
}

fn resolve_package_imports_specifier(
    from_file: &Path,
    module_specifier: &str,
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let conditions = export_conditions(options);
    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        let package_json_path = current.join("package.json");
        if package_json_path.is_file()
            && let Some(package_json) = read_package_json(&package_json_path)
            && let Some(imports) = package_json.imports.as_ref()
            && let Some(target) = resolve_imports_subpath(imports, module_specifier, &conditions)
        {
            let package_type = package_type_from_json(Some(&package_json));
            if let Some(resolved) = resolve_package_entry(current, &target, options, package_type) {
                return Some(resolved);
            }
            // Output-to-source remapping for package imports.
            // When outDir/declarationDir is set, import targets like "./dist/index.js"
            // point to the output directory which doesn't exist at compile time.
            // Remap back to source files (e.g., "./index.ts").
            if let Some(resolved) = try_remap_output_to_source(current, &target, from_file, options)
            {
                return Some(resolved);
            }
        }

        if current == base_dir {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    None
}

fn is_invalid_package_import_specifier(specifier: &str) -> bool {
    specifier == "#" || specifier.starts_with("#/")
}

fn resolve_package_specifier(
    package_root: &Path,
    subpath: Option<&str>,
    package_json: Option<&PackageJson>,
    conditions: &[&str],
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let package_type = package_type_from_json(package_json);
    if let Some(package_json) = package_json {
        let has_exports = options.resolve_package_json_exports && package_json.exports.is_some();

        if has_exports {
            let exports = package_json
                .exports
                .as_ref()
                .expect("has_exports guard ensures exports is Some");
            let subpath_key = match subpath {
                Some(value) => format!("./{value}"),
                None => ".".to_string(),
            };
            if let Some(target) = resolve_exports_subpath(exports, &subpath_key, conditions)
                && let Some(resolved) =
                    resolve_export_entry(package_root, &target, options, package_type)
            {
                return Some(resolved);
            }
            // When an "exports" field exists, subpaths not listed in the exports
            // map are blocked — do not fall through to file-system resolution.
            // This matches Node.js package encapsulation semantics (TS2307).
            return None;
        }

        if let Some(types_versions) = package_json.types_versions.as_ref() {
            let types_subpath = subpath.unwrap_or("index");
            if let Some(resolved) = resolve_types_versions(
                package_root,
                types_subpath,
                types_versions,
                options,
                package_type,
            ) {
                return Some(resolved);
            }
        }
    }

    if let Some(subpath) = subpath {
        return resolve_package_entry(package_root, subpath, options, package_type);
    }

    resolve_package_root(package_root, package_json, options, package_type)
}

fn split_package_specifier(specifier: &str) -> Option<(String, Option<String>)> {
    let mut parts = specifier.split('/');
    let first = parts.next()?;

    if first.starts_with('@') {
        let second = parts.next()?;
        let package = format!("{first}/{second}");
        let rest = parts.collect::<Vec<_>>().join("/");
        let subpath = if rest.is_empty() { None } else { Some(rest) };
        return Some((package, subpath));
    }

    let rest = parts.collect::<Vec<_>>().join("/");
    let subpath = if rest.is_empty() { None } else { Some(rest) };
    Some((first.to_string(), subpath))
}

fn resolve_package_root(
    package_root: &Path,
    package_json: Option<&PackageJson>,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    if let Some(package_json) = package_json {
        for entry in [package_json.types.as_ref(), package_json.typings.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(resolved) =
                resolve_declaration_package_entry(package_root, entry, options, package_type)
            {
                return Some(resolved);
            }
        }

        for entry in [package_json.module.as_ref(), package_json.main.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(resolved) =
                resolve_package_entry(package_root, entry, options, package_type)
            {
                return Some(resolved);
            }
        }
    }

    if let Some(resolved) = resolve_package_entry(package_root, "index", options, package_type) {
        return Some(resolved);
    }

    None
}

fn resolve_declaration_package_entry(
    package_root: &Path,
    entry: &str,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let entry = entry.trim_start_matches("./");
    let path = if Path::new(entry).is_absolute() {
        PathBuf::from(entry)
    } else {
        package_root.join(entry)
    };

    for candidate in expand_module_path_candidates(&path, options, package_type) {
        if candidate.is_file() && is_declaration_file(&candidate) {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    if path.is_file() && is_declaration_file(&path) {
        return Some(normalize_resolved_path(&path, options));
    }

    if path.is_dir()
        && let Some(pj) = read_package_json(&path.join("package.json"))
    {
        let sub_type = package_type_from_json(Some(&pj));
        for entry in [pj.types.as_ref(), pj.typings.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(resolved) =
                resolve_declaration_package_entry(&path, entry, options, sub_type)
            {
                return Some(resolved);
            }
        }
    }

    None
}

fn resolve_package_entry(
    package_root: &Path,
    entry: &str,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let entry = entry.trim_start_matches("./");
    let path = if Path::new(entry).is_absolute() {
        PathBuf::from(entry)
    } else {
        package_root.join(entry)
    };

    // resolve_package_entry is used for `imports` field targets and `main` field
    // resolution — contexts where tsc accepts JS files as valid resolution targets
    // (they get added to the program via import-following). This differs from
    // resolve_export_entry which uses is_valid_module_file (TS/JSON only).
    //
    // In Node16/NodeNext with ESM packages (type: "module"), Node.js does not
    // perform directory index resolution. Skip index candidates for ESM packages.
    let is_esm_no_index = matches!(package_type, Some(PackageType::Module))
        && matches!(
            options.effective_module_resolution(),
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        );
    for candidate in expand_module_path_candidates(&path, options, package_type) {
        // Skip directory index candidates (path/index.{ext}) for ESM packages
        if is_esm_no_index
            && candidate.parent() == Some(&path)
            && let Some(name) = candidate.file_name().and_then(|n| n.to_str())
            && name.starts_with("index.")
        {
            continue;
        }
        if candidate.is_file() && is_valid_module_or_js_file(&candidate) {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    // Check subpath's package.json for types/main fields
    if !is_esm_no_index
        && path.is_dir()
        && let Some(pj) = read_package_json(&path.join("package.json"))
    {
        let sub_type = package_type_from_json(Some(&pj));
        // Try types/typings field
        for types in [pj.types.as_ref(), pj.typings.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(resolved) =
                resolve_declaration_package_entry(&path, types, options, sub_type)
            {
                return Some(resolved);
            }
        }
        // Try main field
        if let Some(main) = &pj.main {
            let main_path = path.join(main);
            for candidate in expand_module_path_candidates(&main_path, options, sub_type) {
                if candidate.is_file() && is_valid_module_or_js_file(&candidate) {
                    return Some(normalize_resolved_path(&candidate, options));
                }
            }
        }
    }

    None
}

fn resolve_export_entry(
    package_root: &Path,
    entry: &str,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let entry = entry.trim_start_matches("./");
    let path = if Path::new(entry).is_absolute() {
        PathBuf::from(entry)
    } else {
        package_root.join(entry)
    };

    for candidate in expand_export_path_candidates(&path, options, package_type) {
        if candidate.is_file() && is_valid_module_file(&candidate) {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    None
}

/// Remap an export map target from the output directory to the source directory.
///
/// When `outDir` or `declarationDir` is set, export targets like `./dist/index.js`
/// point to the output directory which doesn't exist at compile time. tsc's
/// `tryLoadInputFileForPath` handles this by stripping the output directory prefix
/// and substituting output extensions (.js, .d.ts) with source extensions (.ts, .tsx).
///
/// Example: outDir="./dist", target="./dist/index.js"
///   → strip "./dist" → "index.js" → try "index.ts" → found!
fn try_remap_output_to_source(
    package_root: &Path,
    target: &str,
    _from_file: &Path,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    fn resolve_configured_path_against_package_root(
        configured: &Path,
        package_root: &Path,
        canon_package_root: &Path,
        _from_file: &Path,
        options: &ResolvedCompilerOptions,
    ) -> PathBuf {
        if configured.is_absolute() {
            if let Ok(relative) = configured.strip_prefix(package_root) {
                return canon_package_root.join(relative);
            }

            let canonical = normalize_resolved_path(configured, options);
            if canonical.exists() {
                return canonical;
            }

            // Conformance tests use virtual absolute paths like `/pkg/src`
            // while writing files under `<tmpdir>/pkg/src`. Re-anchor those
            // option paths to the temporary project root when the host-absolute
            // path doesn't exist.
            if let Some(project_root) = canon_package_root.parent()
                && let Ok(relative) = configured.strip_prefix(Path::new("/"))
            {
                let matches_package_root =
                    relative
                        .components()
                        .next()
                        .and_then(|component| match component {
                            std::path::Component::Normal(name) => Some(name),
                            _ => None,
                        })
                        == package_root.file_name();

                if matches_package_root {
                    return project_root.join(relative);
                }
            }

            return canonical;
        }

        canon_package_root.join(configured)
    }

    let target = target.trim_start_matches("./");
    // Canonicalize package_root first (it exists) so that symlinks are resolved
    // before joining the target (which may not exist on disk).
    let canon_root = normalize_resolved_path(package_root, options);
    let target_path = canon_root.join(target);

    // Compute the source directory: the root from which source files are organized.
    // Use rootDir if set (already canonicalized), otherwise fall back to the
    // package root (where package.json lives). tsc uses getCommonSourceDirectory()
    // which defaults to the requesting file's directory for single-file projects,
    // but for self-reference resolution the package root is the correct fallback
    // since export targets are relative to it.
    let source_dir_owned;
    let source_dir = if let Some(ref root_dir) = options.root_dir {
        source_dir_owned = resolve_configured_path_against_package_root(
            root_dir,
            package_root,
            &canon_root,
            _from_file,
            options,
        );
        source_dir_owned.as_path()
    } else {
        source_dir_owned = canon_root.clone();
        source_dir_owned.as_path()
    };

    let out_dirs: Vec<&Path> = [
        options.out_dir.as_deref(),
        options.declaration_dir.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();

    if out_dirs.is_empty() {
        return None;
    }

    for out_dir in &out_dirs {
        let resolved_out_dir = resolve_configured_path_against_package_root(
            out_dir,
            package_root,
            &canon_root,
            _from_file,
            options,
        );

        // Check if the target path falls inside the output directory.
        let target_canon = normalize_path(&target_path);
        let out_canon = normalize_path(&resolved_out_dir);

        if let Ok(relative) = target_canon.strip_prefix(&out_canon) {
            // Target is inside the output dir. Build the source path.
            let source_base = source_dir.join(relative);

            // Try substituting output extensions with source extensions
            let source_exts: &[(&str, &[&str])] = &[
                (".js", &[".ts", ".tsx"]),
                (".jsx", &[".tsx", ".ts"]),
                (".mjs", &[".mts"]),
                (".cjs", &[".cts"]),
                (".d.ts", &[".ts", ".tsx"]),
                (".d.mts", &[".mts"]),
                (".d.cts", &[".cts"]),
            ];

            let source_str = source_base.to_string_lossy();
            for (out_ext, src_exts) in source_exts {
                if let Some(base) = source_str.strip_suffix(out_ext) {
                    for src_ext in *src_exts {
                        let candidate = PathBuf::from(format!("{base}{src_ext}"));
                        if candidate.is_file() {
                            return Some(normalize_resolved_path(&candidate, options));
                        }
                    }
                }
            }

            // Also try the path as-is (it might be a .ts file already)
            if source_base.is_file() {
                return Some(normalize_resolved_path(&source_base, options));
            }
        }
    }

    None
}

fn package_type_from_json(package_json: Option<&PackageJson>) -> Option<PackageType> {
    let package_json = package_json?;

    match package_json.package_type.as_deref() {
        Some("module") => Some(PackageType::Module),
        Some("commonjs") | None => Some(PackageType::CommonJs),
        Some(_) => None,
    }
}

fn read_package_json(path: &Path) -> Option<PackageJson> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn collect_package_entry_candidates(package_json: &PackageJson) -> Vec<String> {
    let mut seen = FxHashSet::default();
    let mut candidates = Vec::new();

    for value in [package_json.types.as_ref(), package_json.typings.as_ref()]
        .into_iter()
        .flatten()
    {
        if seen.insert(value.clone()) {
            candidates.push(value.clone());
        }
    }

    for value in [package_json.module.as_ref(), package_json.main.as_ref()]
        .into_iter()
        .flatten()
    {
        if seen.insert(value.clone()) {
            candidates.push(value.clone());
        }
    }

    candidates
}

fn resolve_types_versions(
    package_root: &Path,
    subpath: &str,
    types_versions: &serde_json::Value,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    let compiler_version = types_versions_compiler_version(options);
    let paths = select_types_versions_paths(types_versions, compiler_version)?;
    let mut best_pattern: Option<&String> = None;
    let mut best_value: Option<&serde_json::Value> = None;
    let mut best_wildcard = String::new();
    let mut best_specificity = 0usize;
    let mut best_len = 0usize;

    for (pattern, value) in paths {
        let Some(wildcard) = match_types_versions_pattern(pattern, subpath) else {
            continue;
        };
        let specificity = types_versions_specificity(pattern);
        let pattern_len = pattern.len();
        let is_better = match best_pattern {
            None => true,
            Some(current) => {
                specificity > best_specificity
                    || (specificity == best_specificity && pattern_len > best_len)
                    || (specificity == best_specificity
                        && pattern_len == best_len
                        && pattern < current)
            }
        };

        if is_better {
            best_specificity = specificity;
            best_len = pattern_len;
            best_pattern = Some(pattern);
            best_value = Some(value);
            best_wildcard = wildcard;
        }
    }

    let value = best_value?;

    let mut targets = Vec::new();
    match value {
        serde_json::Value::String(value) => targets.push(value.as_str()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(value) = entry.as_str() {
                    targets.push(value);
                }
            }
        }
        _ => {}
    }

    for target in targets {
        let substituted = substitute_path_target(target, &best_wildcard);
        if let Some(resolved) =
            resolve_package_entry(package_root, &substituted, options, package_type)
        {
            return Some(resolved);
        }
    }

    None
}

fn select_types_versions_paths(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    select_types_versions_paths_for_version(types_versions, compiler_version)
}

fn select_types_versions_paths_for_version(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let map = types_versions.as_object()?;
    let mut best_score: Option<RangeScore> = None;
    let mut best_key: Option<&str> = None;
    let mut best_value: Option<&serde_json::Map<String, serde_json::Value>> = None;

    for (key, value) in map {
        let Some(value_map) = value.as_object() else {
            continue;
        };
        let Some(score) = match_types_versions_range(key, compiler_version) else {
            continue;
        };
        let is_better = match best_score {
            None => true,
            Some(best) => {
                score > best
                    || (score == best && best_key.is_none_or(|best_key| key.as_str() < best_key))
            }
        };

        if is_better {
            best_score = Some(score);
            best_key = Some(key);
            best_value = Some(value_map);
        }
    }

    best_value
}

fn match_types_versions_pattern(pattern: &str, subpath: &str) -> Option<String> {
    if !pattern.contains('*') {
        return (pattern == subpath).then(String::new);
    }

    let star = pattern.find('*')?;
    let (prefix, suffix) = pattern.split_at(star);
    let suffix = &suffix[1..];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());
    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

fn types_versions_specificity(pattern: &str) -> usize {
    if let Some(star) = pattern.find('*') {
        star + (pattern.len() - star - 1)
    } else {
        pattern.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct RangeScore {
    constraints: usize,
    min_version: SemVer,
    key_len: usize,
}

fn match_types_versions_range(range: &str, compiler_version: SemVer) -> Option<RangeScore> {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len: range.len(),
        });
    }

    let mut best: Option<RangeScore> = None;
    for segment in range.split("||") {
        let segment = segment.trim();
        let Some(score) =
            match_types_versions_range_segment(segment, compiler_version, range.len())
        else {
            continue;
        };
        if best.is_none_or(|current| score > current) {
            best = Some(score);
        }
    }

    best
}

fn match_types_versions_range_segment(
    segment: &str,
    compiler_version: SemVer,
    key_len: usize,
) -> Option<RangeScore> {
    if segment.is_empty() {
        return None;
    }
    if segment == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len,
        });
    }

    let mut min_version = SemVer::ZERO;
    let mut constraints = 0usize;

    for token in segment.split_whitespace() {
        if token.is_empty() || token == "*" {
            continue;
        }
        let (op, version) = parse_range_token(token)?;
        if !compare_range(compiler_version, op, version) {
            return None;
        }
        constraints += 1;
        if matches!(op, RangeOp::Gt | RangeOp::Gte | RangeOp::Eq) && version > min_version {
            min_version = version;
        }
    }

    Some(RangeScore {
        constraints,
        min_version,
        key_len,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RangeOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

fn parse_range_token(token: &str) -> Option<(RangeOp, SemVer)> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    let (op, rest) = if let Some(rest) = token.strip_prefix(">=") {
        (RangeOp::Gte, rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        (RangeOp::Lte, rest)
    } else if let Some(rest) = token.strip_prefix('>') {
        (RangeOp::Gt, rest)
    } else if let Some(rest) = token.strip_prefix('<') {
        (RangeOp::Lt, rest)
    } else if let Some(rest) = token.strip_prefix('=') {
        (RangeOp::Eq, rest)
    } else {
        (RangeOp::Eq, token)
    };

    parse_semver(rest).map(|version| (op, version))
}

fn compare_range(version: SemVer, op: RangeOp, bound: SemVer) -> bool {
    match op {
        RangeOp::Gt => version > bound,
        RangeOp::Gte => version >= bound,
        RangeOp::Lt => version < bound,
        RangeOp::Lte => version <= bound,
        RangeOp::Eq => version == bound,
    }
}

fn parse_semver(value: &str) -> Option<SemVer> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let core = value.split(['-', '+']).next().unwrap_or(value);
    let mut parts = core.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().unwrap_or("0").parse().ok()?;
    let patch: u32 = parts.next().unwrap_or("0").parse().ok()?;
    Some(SemVer {
        major,
        minor,
        patch,
    })
}

fn resolve_exports_subpath(
    exports: &serde_json::Value,
    subpath_key: &str,
    conditions: &[&str],
) -> Option<String> {
    match exports {
        serde_json::Value::String(value) => (subpath_key == ".").then(|| value.clone()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(resolved) = resolve_exports_subpath(entry, subpath_key, conditions) {
                    return Some(resolved);
                }
            }
            None
        }
        serde_json::Value::Object(map) => {
            let has_subpath_keys = map.keys().any(|key| key.starts_with('.'));
            if has_subpath_keys {
                if let Some(value) = map.get(subpath_key)
                    && let Some(target) = resolve_exports_target(value, conditions)
                {
                    return Some(target);
                }

                let mut best_match: Option<(usize, String, &serde_json::Value)> = None;
                for (key, value) in map {
                    let Some(wildcard) = match_exports_subpath(key, subpath_key) else {
                        continue;
                    };
                    let specificity = key.len();
                    let is_better = match &best_match {
                        None => true,
                        Some((best_len, _, _)) => specificity > *best_len,
                    };
                    if is_better {
                        best_match = Some((specificity, wildcard, value));
                    }
                }

                if let Some((_, wildcard, value)) = best_match
                    && let Some(target) = resolve_exports_target(value, conditions)
                {
                    return Some(apply_exports_subpath(&target, &wildcard));
                }

                None
            } else if subpath_key == "." {
                resolve_exports_target(exports, conditions)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn resolve_exports_target(target: &serde_json::Value, conditions: &[&str]) -> Option<String> {
    resolve_exports_target_versioned(
        target,
        conditions,
        default_types_versions_compiler_version(),
    )
}

fn resolve_exports_target_versioned(
    target: &serde_json::Value,
    conditions: &[&str],
    compiler_version: SemVer,
) -> Option<String> {
    match target {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(resolved) =
                    resolve_exports_target_versioned(entry, conditions, compiler_version)
                {
                    return Some(resolved);
                }
            }
            None
        }
        serde_json::Value::Object(map) => {
            // Process keys in insertion order (Node.js spec). For each key:
            // 1. Check if it's a plain condition match
            // 2. Check if it's a versioned condition like "types@>=1"
            for (key, value) in map {
                // Check for versioned condition (e.g., "types@>=1")
                if let Some(at_pos) = key.find('@') {
                    let base_condition = &key[..at_pos];
                    let version_range = &key[at_pos + 1..];
                    if conditions.contains(&base_condition)
                        && match_types_versions_range(version_range, compiler_version).is_some()
                        && let Some(resolved) =
                            resolve_exports_target_versioned(value, conditions, compiler_version)
                    {
                        return Some(resolved);
                    }
                } else if conditions.contains(&key.as_str())
                    && let Some(resolved) =
                        resolve_exports_target_versioned(value, conditions, compiler_version)
                {
                    return Some(resolved);
                }
            }
            None
        }
        _ => None,
    }
}

fn resolve_imports_subpath(
    imports: &serde_json::Value,
    subpath_key: &str,
    conditions: &[&str],
) -> Option<String> {
    let serde_json::Value::Object(map) = imports else {
        return None;
    };

    let has_subpath_keys = map.keys().any(|key| key.starts_with('#'));
    if !has_subpath_keys {
        return None;
    }

    if let Some(value) = map.get(subpath_key) {
        return resolve_exports_target(value, conditions);
    }

    let mut best_match: Option<(usize, String, &serde_json::Value)> = None;
    for (key, value) in map {
        let Some(wildcard) = match_imports_subpath(key, subpath_key) else {
            continue;
        };
        let specificity = key.len();
        let is_better = match &best_match {
            None => true,
            Some((best_len, _, _)) => specificity > *best_len,
        };
        if is_better {
            best_match = Some((specificity, wildcard, value));
        }
    }

    if let Some((_, wildcard, value)) = best_match
        && let Some(target) = resolve_exports_target(value, conditions)
    {
        return Some(apply_exports_subpath(&target, &wildcard));
    }

    None
}

fn match_exports_subpath(pattern: &str, subpath_key: &str) -> Option<String> {
    let pattern_inner = pattern.strip_prefix("./")?;
    let subpath = subpath_key.strip_prefix("./")?;

    // A bare "./" exports entry only exposes explicit file-like subpaths such as
    // "./index.js". It should not manufacture extensionless package subpaths like
    // "inner/other" that tsc still rejects with TS2307.
    if pattern == "./" {
        let has_explicit_extension = Path::new(subpath)
            .extension()
            .is_some_and(|ext| !ext.is_empty());
        return has_explicit_extension.then(|| subpath.to_string());
    }

    // Handle deprecated trailing-slash directory patterns like "./dir/".
    if !pattern_inner.is_empty() && pattern_inner.ends_with('/') && !pattern.contains('*') {
        if let Some(rest) = subpath.strip_prefix(pattern_inner) {
            return Some(rest.to_string());
        }
        return None;
    }

    if !pattern.contains('*') {
        return None;
    }

    let star = pattern_inner.find('*')?;
    let (prefix, suffix) = pattern_inner.split_at(star);
    let suffix = &suffix[1..];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());
    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

fn match_imports_subpath(pattern: &str, subpath_key: &str) -> Option<String> {
    if !pattern.contains('*') {
        return None;
    }
    let pattern = pattern.strip_prefix('#')?;
    let subpath = subpath_key.strip_prefix('#')?;

    let star = pattern.find('*')?;
    let (prefix, suffix) = pattern.split_at(star);
    let suffix = &suffix[1..];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());
    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

fn apply_exports_subpath(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else if target.ends_with('/') {
        // Trailing-slash directory pattern: append the matched portion
        format!("{target}{wildcard}")
    } else {
        target.to_string()
    }
}

pub(crate) fn is_declaration_file(path: &Path) -> bool {
    tsz::module_resolver::ModuleExtension::from_path(path).is_declaration()
}

pub(crate) fn canonicalize_with_missing_tail(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }

    let mut tail = Vec::new();
    let mut current = path;
    while !current.exists() {
        let Some(name) = current.file_name() else {
            return path.to_path_buf();
        };
        tail.push(name.to_os_string());
        let Some(parent) = current.parent() else {
            return path.to_path_buf();
        };
        current = parent;
    }

    let Ok(mut canonical) = std::fs::canonicalize(current) else {
        return path.to_path_buf();
    };
    for component in tail.iter().rev() {
        canonical.push(component);
    }
    canonical
}

pub(crate) fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
}

#[cfg(test)]
#[path = "resolution_tests.rs"]
mod resolution_tests;
