use anyhow::{Context, Result};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::config::{JsxEmit, ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
use crate::fs::is_valid_module_file;
use tsz::declaration_emitter::DeclarationEmitter;
use tsz::emitter::{ModuleKind, NewLineKind, Printer};
use tsz::parallel::MergedProgram;
use tsz::parser::NodeIndex;
use tsz::parser::ParserState;
use tsz::parser::node::{NodeAccess, NodeArena};
use tsz::scanner::SyntaxKind;

#[derive(Debug, Clone)]
pub(crate) struct OutputFile {
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageType {
    Module,
    CommonJs,
}

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
    let candidates = type_package_candidates(name);
    if candidates.is_empty() {
        return None;
    }

    for root in roots {
        for candidate in &candidates {
            let package_root = root.join(candidate);
            if !package_root.is_dir() {
                continue;
            }
            if let Some(entry) = resolve_type_package_entry(&package_root, options) {
                return Some(entry);
            }
        }
    }

    None
}

/// Public wrapper for `type_package_candidates`.
pub(crate) fn type_package_candidates_pub(name: &str) -> Vec<String> {
    type_package_candidates(name)
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
                    return Some(canonicalize_or_owned(&candidate));
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
        if is_declaration_file(&resolved) {
            Some(resolved)
        } else {
            None
        }
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
    if let Some(exports) = &package_json.exports {
        if let Some(target) = resolve_exports_subpath(exports, ".", &conditions) {
            let target_path = package_root.join(target.trim_start_matches("./"));
            // Try to find a declaration file at the target
            let package_type = package_type_from_json(Some(package_json));
            for candidate in expand_module_path_candidates(&target_path, options, package_type) {
                if candidate.is_file() && is_declaration_file(&candidate) {
                    return Some(canonicalize_or_owned(&candidate));
                }
            }
            // Try exact path
            if target_path.is_file() && is_declaration_file(&target_path) {
                return Some(canonicalize_or_owned(&target_path));
            }
        }
    }

    None
}

pub(crate) fn default_type_roots(base_dir: &Path) -> Vec<PathBuf> {
    let candidate = base_dir.join("node_modules").join("@types");
    if candidate.is_dir() {
        vec![canonicalize_or_owned(&candidate)]
    } else {
        Vec::new()
    }
}

pub(crate) fn collect_module_specifiers_from_text(path: &Path, text: &str) -> Vec<String> {
    let file_name = path.to_string_lossy().into_owned();
    let mut parser = ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    collect_module_specifiers(&arena, source_file)
        .into_iter()
        .map(|(specifier, _, _)| specifier)
        .collect()
}

pub(crate) fn collect_module_specifiers(
    arena: &NodeArena,
    source_file: NodeIndex,
) -> Vec<(String, NodeIndex, tsz::module_resolver::ImportKind)> {
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
                specifiers.push((strip_quotes(text), import_decl.module_specifier, kind));
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
                ));
            } else if !export_decl.export_clause.is_none()
                && let Some(import_decl) = arena.get_import_decl_at(export_decl.export_clause)
                && let Some(text) = arena.get_literal_text(import_decl.module_specifier)
            {
                specifiers.push((
                    strip_quotes(text),
                    import_decl.module_specifier,
                    ImportKind::EsmReExport,
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
            if has_declare {
                if let Some(text) = arena.get_literal_text(module_decl.name) {
                    specifiers.push((strip_quotes(text), module_decl.name, ImportKind::EsmImport));
                }
            }
        }
    }

    // Also collect dynamic imports from expression statements
    collect_dynamic_imports(arena, source_file, &strip_quotes, &mut specifiers);

    specifiers
}

/// Collect dynamic import() expressions from the AST
fn collect_dynamic_imports(
    arena: &NodeArena,
    _source_file: NodeIndex,
    strip_quotes: &dyn Fn(&str) -> String,
    specifiers: &mut Vec<(String, NodeIndex, tsz::module_resolver::ImportKind)>,
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
            ));
        }
    }
}

/// Extract module specifier from a require() call expression
/// e.g., `require('./module')` -> `./module` (without quotes)
fn extract_require_specifier(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    use tsz::parser::syntax_kind_ext;
    use tsz::scanner::SyntaxKind;

    let node = arena.get(idx)?;

    // Helper to strip surrounding quotes from a string
    let strip_quotes =
        |s: &str| -> String { s.trim_matches(|c| c == '"' || c == '\'').to_string() };

    // If it's directly a string literal, return it (without quotes)
    if let Some(text) = arena.get_literal_text(idx) {
        return Some(strip_quotes(text));
    }

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
    arena.get_literal_text(*arg_idx).map(|s| strip_quotes(s))
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
            .map(|text| text.to_string());
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
                if !spec_idx.is_none() {
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
        if !export_decl.export_clause.is_none() {
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
            if !clause.name.is_none()
                && let Some(name) = arena.get_identifier_text(clause.name)
            {
                names.push(name.to_string());
            }

            if !clause.named_bindings.is_none()
                && let Some(bindings_node) = arena.get(clause.named_bindings)
            {
                if bindings_node.kind == SyntaxKind::Identifier as u16 {
                    if let Some(name) = arena.get_identifier_text(clause.named_bindings) {
                        names.push(name.to_string());
                    }
                } else if let Some(named) = arena.get_named_imports(bindings_node) {
                    if !named.name.is_none()
                        && let Some(name) = arena.get_identifier_text(named.name)
                    {
                        names.push(name.to_string());
                    }
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec) = arena.get_specifier_at(spec_idx) else {
                            continue;
                        };
                        let local_ident = if !spec.name.is_none() {
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
    let specifier = module_specifier.trim();
    if specifier.is_empty() {
        return None;
    }
    let specifier = specifier.replace('\\', "/");
    if specifier.starts_with('#') {
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
    } else if specifier.starts_with('.') {
        let joined = from_dir.join(&specifier);
        candidates.extend(expand_module_path_candidates(
            &joined,
            options,
            package_type,
        ));
    } else if matches!(resolution, ModuleResolutionKind::Classic) {
        if let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) = select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            let base = options.base_url.as_deref().unwrap_or(from_dir);
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

        if candidates.is_empty() {
            // Classic resolution walks up the directory tree from the containing
            // file's directory, probing for <specifier>.ts, .d.ts, etc. at each level.
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
            || (candidate.is_file() && is_valid_module_file(&candidate));

        if exists {
            return Some(canonicalize_or_owned(&candidate));
        }
    }

    // If path mapping was attempted but no file was found, return None early
    // to emit TS2307 rather than falling through to node_modules resolution
    if path_mapping_attempted {
        return None;
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

fn expand_export_path_candidates(
    path: &Path,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let base = normalize_path(path);
    let suffixes = &options.module_suffixes;
    if let Some((base_no_ext, extension)) = split_path_extension(&base) {
        return candidates_with_suffixes_and_extension(&base_no_ext, extension, suffixes);
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

fn extension_candidates_for_resolution(
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

fn normalize_path(path: &Path) -> PathBuf {
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
    const ZERO: SemVer = SemVer {
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

fn default_types_versions_compiler_version() -> SemVer {
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
        } else if subpath.is_none() {
            // Try resolving as a file directly in node_modules
            // e.g., node_modules/foo.d.ts for bare specifier "foo"
            let node_modules_dir = current.join("node_modules");
            if node_modules_dir.is_dir() {
                let candidates = expand_module_path_candidates(&package_root, options, None);
                for candidate in candidates {
                    if candidate.is_file() && is_valid_module_file(&candidate) {
                        return Some(canonicalize_or_owned(&candidate));
                    }
                }
            }
        }

        // 2. Look for @types package (if not already looking for one)
        // TypeScript looks up @types/foo for 'foo', and @types/scope__pkg for '@scope/pkg'
        if !package_name.starts_with("@types/") {
            let types_package_name = if let Some(scope_pkg) = package_name.strip_prefix('@') {
                // Scoped package: @scope/pkg -> @types/scope__pkg
                // Skip the '@' (1 char) and replace '/' with '__'
                format!("@types/{}", scope_pkg.replace('/', "__"))
            } else {
                format!("@types/{}", package_name)
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

fn resolve_package_specifier(
    package_root: &Path,
    subpath: Option<&str>,
    package_json: Option<&PackageJson>,
    conditions: &[&str],
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let package_type = package_type_from_json(package_json);
    if let Some(package_json) = package_json {
        if options.resolve_package_json_exports
            && let Some(exports) = package_json.exports.as_ref()
        {
            let subpath_key = match subpath {
                Some(value) => format!("./{}", value),
                None => ".".to_string(),
            };
            if let Some(target) = resolve_exports_subpath(exports, &subpath_key, conditions)
                && let Some(resolved) =
                    resolve_export_entry(package_root, &target, options, package_type)
            {
                return Some(resolved);
            }
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
    let mut candidates = Vec::new();

    if let Some(package_json) = package_json {
        candidates = collect_package_entry_candidates(package_json);
    }

    if !candidates
        .iter()
        .any(|entry| entry == "index" || entry == "./index")
    {
        candidates.push("index".to_string());
    }

    for entry in candidates {
        if let Some(resolved) = resolve_package_entry(package_root, &entry, options, package_type) {
            return Some(resolved);
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

    for candidate in expand_module_path_candidates(&path, options, package_type) {
        if candidate.is_file() && is_valid_module_file(&candidate) {
            return Some(canonicalize_or_owned(&candidate));
        }
    }

    // Check subpath's package.json for types/main fields
    if path.is_dir() {
        if let Some(pj) = read_package_json(&path.join("package.json")) {
            let sub_type = package_type_from_json(Some(&pj));
            // Try types/typings field
            if let Some(types) = pj.types.or(pj.typings) {
                let types_path = path.join(&types);
                for candidate in expand_module_path_candidates(&types_path, options, sub_type) {
                    if candidate.is_file() && is_valid_module_file(&candidate) {
                        return Some(canonicalize_or_owned(&candidate));
                    }
                }
                if types_path.is_file() {
                    return Some(canonicalize_or_owned(&types_path));
                }
            }
            // Try main field
            if let Some(main) = &pj.main {
                let main_path = path.join(main);
                for candidate in expand_module_path_candidates(&main_path, options, sub_type) {
                    if candidate.is_file() && is_valid_module_file(&candidate) {
                        return Some(canonicalize_or_owned(&candidate));
                    }
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
            return Some(canonicalize_or_owned(&candidate));
        }
    }

    None
}

fn package_type_from_json(package_json: Option<&PackageJson>) -> Option<PackageType> {
    let package_json = package_json?;

    match package_json.package_type.as_deref() {
        Some("module") => Some(PackageType::Module),
        Some("commonjs") => Some(PackageType::CommonJs),
        Some(_) => None,
        None => Some(PackageType::CommonJs),
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
        return if pattern == subpath {
            Some(String::new())
        } else {
            None
        };
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
        serde_json::Value::String(value) => {
            if subpath_key == "." {
                Some(value.clone())
            } else {
                None
            }
        }
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
    match target {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(resolved) = resolve_exports_target(entry, conditions) {
                    return Some(resolved);
                }
            }
            None
        }
        serde_json::Value::Object(map) => {
            for condition in conditions {
                if let Some(value) = map.get(*condition)
                    && let Some(resolved) = resolve_exports_target(value, conditions)
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
    if !pattern.contains('*') {
        return None;
    }
    let pattern = pattern.strip_prefix("./")?;
    let subpath = subpath_key.strip_prefix("./")?;

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
    } else {
        target.to_string()
    }
}

pub(crate) fn emit_outputs(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    declaration_dir: Option<&Path>,
    dirty_paths: Option<&FxHashSet<PathBuf>>,
    type_caches: &FxHashMap<std::path::PathBuf, tsz::checker::TypeCache>,
) -> Result<Vec<OutputFile>> {
    let mut outputs = Vec::new();
    let new_line = new_line_str(options.printer.new_line);

    // Build mapping from arena address to file path for module resolution
    let arena_to_path: rustc_hash::FxHashMap<usize, String> = program
        .files
        .iter()
        .map(|file| {
            let arena_addr = std::sync::Arc::as_ptr(&file.arena) as usize;
            (arena_addr, file.file_name.clone())
        })
        .collect();

    for (file_idx, file) in program.files.iter().enumerate() {
        let input_path = PathBuf::from(&file.file_name);
        if let Some(dirty_paths) = dirty_paths
            && !dirty_paths.contains(&input_path)
        {
            continue;
        }

        if let Some(js_path) = js_output_path(base_dir, root_dir, out_dir, options.jsx, &input_path)
        {
            // Get type_only_nodes from the type cache (if available)
            let type_only_nodes = type_caches
                .get(&input_path)
                .map(|cache| std::sync::Arc::new(cache.type_only_nodes.clone()))
                .unwrap_or_else(|| std::sync::Arc::new(rustc_hash::FxHashSet::default()));

            // Clone and update printer options with type_only_nodes
            let mut printer_options = options.printer.clone();
            printer_options.type_only_nodes = type_only_nodes;

            // Run the lowering pass to generate transform directives
            let mut ctx = tsz::emit_context::EmitContext::with_options(printer_options.clone());
            // Enable auto-detect module: when module is None and file has imports/exports,
            // the emitter should switch to CommonJS (matching tsc behavior)
            ctx.auto_detect_module = true;
            let transforms =
                tsz::lowering_pass::LoweringPass::new(&file.arena, &ctx).run(file.source_file);

            let mut printer =
                Printer::with_transforms_and_options(&file.arena, transforms, printer_options);
            printer.set_auto_detect_module(true);
            // Always set source text for comment preservation and single-line detection
            if let Some(source_text) = file
                .arena
                .get(file.source_file)
                .and_then(|node| file.arena.get_source_file(node))
                .map(|source| source.text.as_ref())
            {
                printer.set_source_text(source_text);
            }

            let map_info = if options.source_map {
                map_output_info(&js_path)
            } else {
                None
            };

            // Always set source text for formatting decisions (single-line vs multi-line)
            // This is needed even when source maps are disabled
            if let Some(source_text) = file
                .arena
                .get(file.source_file)
                .and_then(|node| file.arena.get_source_file(node))
                .map(|source| source.text.as_ref())
            {
                printer.set_source_map_text(source_text);
            }

            if let Some((_, _, output_name)) = map_info.as_ref() {
                printer.enable_source_map(output_name, &file.file_name);
            }

            printer.emit(file.source_file);
            let map_json = map_info
                .as_ref()
                .and_then(|_| printer.generate_source_map_json());
            let mut contents = printer.take_output();
            let mut map_output = None;

            if let Some((map_path, map_name, _)) = map_info
                && let Some(map_json) = map_json
            {
                append_source_mapping_url(&mut contents, &map_name, new_line);
                map_output = Some(OutputFile {
                    path: map_path,
                    contents: map_json,
                });
            }

            outputs.push(OutputFile {
                path: js_path,
                contents,
            });
            if let Some(map_output) = map_output {
                outputs.push(map_output);
            }
        }

        if options.emit_declarations {
            let decl_base = declaration_dir.or(out_dir);
            if let Some(dts_path) =
                declaration_output_path(base_dir, root_dir, decl_base, &input_path)
            {
                // Get type cache for this file if available
                let file_path = PathBuf::from(&file.file_name);
                let type_cache = type_caches.get(&file_path).cloned();

                // Reconstruct BinderState for this file to enable usage analysis
                let binder = tsz::parallel::create_binder_from_bound_file(file, program, file_idx);

                // Create emitter with type information and binder
                let mut emitter = if let Some(ref cache) = type_cache {
                    use tsz_emitter::type_cache_view::TypeCacheView;
                    let cache_view = TypeCacheView {
                        node_types: cache.node_types.clone(),
                        def_to_symbol: cache.def_to_symbol.clone(),
                    };
                    let mut emitter = DeclarationEmitter::with_type_info(
                        &file.arena,
                        cache_view,
                        &program.type_interner,
                        &binder,
                    );
                    // Set current arena and file path for foreign symbol tracking
                    emitter.set_current_arena(file.arena.clone(), file.file_name.clone());
                    // Set arena to path mapping for module resolution
                    emitter.set_arena_to_path(arena_to_path.clone());
                    emitter
                } else {
                    let mut emitter = DeclarationEmitter::new(&file.arena);
                    // Still set binder even without cache for consistency
                    emitter.set_binder(Some(&binder));
                    emitter.set_arena_to_path(arena_to_path.clone());
                    emitter
                };
                let map_info = if options.declaration_map {
                    map_output_info(&dts_path)
                } else {
                    None
                };

                if let Some((_, _, output_name)) = map_info.as_ref() {
                    if let Some(source_text) = file
                        .arena
                        .get(file.source_file)
                        .and_then(|node| file.arena.get_source_file(node))
                        .map(|source| source.text.as_ref())
                    {
                        emitter.set_source_map_text(source_text);
                    }
                    emitter.enable_source_map(output_name, &file.file_name);
                }

                // Run usage analysis and calculate required imports if we have type cache
                if let Some(ref cache) = type_cache {
                    use rustc_hash::FxHashMap;
                    use tsz::declaration_emitter::usage_analyzer::UsageAnalyzer;
                    use tsz_emitter::type_cache_view::TypeCacheView;

                    // Empty import_name_map for this usage (not needed for auto-import calculation)
                    let import_name_map = FxHashMap::default();
                    let cache_view = TypeCacheView {
                        node_types: cache.node_types.clone(),
                        def_to_symbol: cache.def_to_symbol.clone(),
                    };

                    let mut analyzer = UsageAnalyzer::new(
                        &file.arena,
                        &binder,
                        &cache_view,
                        &program.type_interner,
                        file.arena.clone(),
                        &import_name_map,
                    );

                    // Clone used_symbols before calling another method on analyzer
                    let used_symbols = analyzer.analyze(file.source_file).clone();
                    let foreign_symbols = analyzer.get_foreign_symbols().clone();

                    // Set used symbols and foreign symbols on emitter
                    emitter.set_used_symbols(used_symbols);
                    emitter.set_foreign_symbols(foreign_symbols);
                }

                let mut contents = emitter.emit(file.source_file);
                let map_json = map_info
                    .as_ref()
                    .and_then(|_| emitter.generate_source_map_json());
                let mut map_output = None;

                if let Some((map_path, map_name, _)) = map_info
                    && let Some(map_json) = map_json
                {
                    append_source_mapping_url(&mut contents, &map_name, new_line);
                    map_output = Some(OutputFile {
                        path: map_path,
                        contents: map_json,
                    });
                }

                outputs.push(OutputFile {
                    path: dts_path,
                    contents,
                });
                if let Some(map_output) = map_output {
                    outputs.push(map_output);
                }
            }
        }
    }

    Ok(outputs)
}

fn map_output_info(output_path: &Path) -> Option<(PathBuf, String, String)> {
    let output_name = output_path.file_name()?.to_string_lossy().into_owned();
    let map_name = format!("{output_name}.map");
    let map_path = output_path.with_file_name(&map_name);
    Some((map_path, map_name, output_name))
}

fn append_source_mapping_url(contents: &mut String, map_name: &str, new_line: &str) {
    if !contents.is_empty() && !contents.ends_with(new_line) {
        contents.push_str(new_line);
    }
    contents.push_str("//# sourceMappingURL=");
    contents.push_str(map_name);
}

fn new_line_str(kind: NewLineKind) -> &'static str {
    match kind {
        NewLineKind::LineFeed => "\n",
        NewLineKind::CarriageReturnLineFeed => "\r\n",
    }
}

pub(crate) fn write_outputs(outputs: &[OutputFile]) -> Result<Vec<PathBuf>> {
    outputs.par_iter().try_for_each(|output| -> Result<()> {
        if let Some(parent) = output.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        std::fs::write(&output.path, &output.contents)
            .with_context(|| format!("failed to write {}", output.path.display()))?;
        Ok(())
    })?;

    Ok(outputs.iter().map(|output| output.path.clone()).collect())
}

fn js_output_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    jsx: Option<JsxEmit>,
    input_path: &Path,
) -> Option<PathBuf> {
    if is_declaration_file(input_path) {
        return None;
    }

    let extension = js_extension_for(input_path, jsx)?;
    let relative = output_relative_path(base_dir, root_dir, input_path);
    let mut output = match out_dir {
        Some(out_dir) => out_dir.join(relative),
        None => input_path.to_path_buf(),
    };
    output.set_extension(extension);
    Some(output)
}

fn declaration_output_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    input_path: &Path,
) -> Option<PathBuf> {
    if is_declaration_file(input_path) {
        return None;
    }

    let relative = output_relative_path(base_dir, root_dir, input_path);
    let file_name = relative.file_name()?.to_str()?;
    let new_name = declaration_file_name(file_name)?;

    let mut output = match out_dir {
        Some(out_dir) => out_dir.join(relative),
        None => input_path.to_path_buf(),
    };
    output.set_file_name(new_name);
    Some(output)
}

fn output_relative_path(base_dir: &Path, root_dir: Option<&Path>, input_path: &Path) -> PathBuf {
    if let Some(root_dir) = root_dir
        && let Ok(relative) = input_path.strip_prefix(root_dir)
    {
        return relative.to_path_buf();
    }

    input_path
        .strip_prefix(base_dir)
        .unwrap_or(input_path)
        .to_path_buf()
}

fn declaration_file_name(file_name: &str) -> Option<String> {
    if file_name.ends_with(".mts") {
        return Some(file_name.trim_end_matches(".mts").to_string() + ".d.mts");
    }
    if file_name.ends_with(".cts") {
        return Some(file_name.trim_end_matches(".cts").to_string() + ".d.cts");
    }
    if file_name.ends_with(".tsx") {
        return Some(file_name.trim_end_matches(".tsx").to_string() + ".d.ts");
    }
    if file_name.ends_with(".ts") {
        return Some(file_name.trim_end_matches(".ts").to_string() + ".d.ts");
    }

    None
}

fn is_declaration_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
}

fn js_extension_for(path: &Path, jsx: Option<JsxEmit>) -> Option<&'static str> {
    let name = path.file_name().and_then(|name| name.to_str())?;
    if name.ends_with(".mts") {
        return Some("mjs");
    }
    if name.ends_with(".cts") {
        return Some("cjs");
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("ts") => Some("js"),
        Some("tsx") => match jsx {
            Some(JsxEmit::Preserve) => Some("jsx"),
            Some(JsxEmit::React)
            | Some(JsxEmit::ReactJsx)
            | Some(JsxEmit::ReactJsxDev)
            | Some(JsxEmit::ReactNative)
            | None => Some("js"),
        },
        _ => None,
    }
}

pub(crate) fn normalize_base_url(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        let resolved = if dir.is_absolute() {
            dir
        } else {
            base_dir.join(dir)
        };
        canonicalize_or_owned(&resolved)
    })
}

pub(crate) fn normalize_output_dir(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        if dir.is_absolute() {
            dir
        } else {
            base_dir.join(dir)
        }
    })
}

pub(crate) fn normalize_root_dir(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        let resolved = if dir.is_absolute() {
            dir
        } else {
            base_dir.join(dir)
        };
        canonicalize_or_owned(&resolved)
    })
}

pub(crate) fn normalize_type_roots(
    base_dir: &Path,
    roots: Option<Vec<PathBuf>>,
) -> Option<Vec<PathBuf>> {
    let roots = roots?;
    let mut normalized = Vec::new();
    for root in roots {
        let resolved = if root.is_absolute() {
            root
        } else {
            base_dir.join(root)
        };
        let resolved = canonicalize_or_owned(&resolved);
        if resolved.is_dir() {
            normalized.push(resolved);
        }
    }
    Some(normalized)
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
#[path = "driver_resolution_tests.rs"]
mod tests;
