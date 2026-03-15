//! Utility functions for the compilation driver's checking phase:
//! export hash computation, tslib helper detection, binder construction,
//! parse diagnostic conversion, and pragma detection.

use super::*;

pub(super) fn detect_missing_tslib_helper_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
) -> Vec<Diagnostic> {
    if !options.import_helpers {
        return Vec::new();
    }

    let tslib_file = program.files.iter().find(|file| {
        let path = file.file_name.replace('\\', "/");
        // Match tslib by directory or filename: the package's main declaration
        // file may be `tslib.d.ts` or `index.d.ts` inside a `tslib/` directory.
        path.contains("/tslib/")
            || Path::new(&file.file_name)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("tslib.d.ts"))
    });

    // Resolved via program files — check exports directly.
    if let Some(tslib_file) = tslib_file {
        let tslib_exports_empty = program
            .module_exports
            .get(&tslib_file.file_name)
            .is_none_or(tsz_binder::SymbolTable::is_empty);

        if !tslib_exports_empty {
            return Vec::new();
        }

        return emit_ts2343_for_missing_helpers(program, options, &tslib_file.file_name);
    }

    // Check if tslib is declared as an ambient module (`declare module "tslib" { ... }`).
    // When found, use its module_exports to check for specific helpers.
    if program.declared_modules.contains("tslib") {
        let tslib_exports_empty = program
            .module_exports
            .get("tslib")
            .is_none_or(tsz_binder::SymbolTable::is_empty);

        if !tslib_exports_empty {
            return Vec::new();
        }

        return emit_ts2343_for_missing_helpers(program, options, "tslib");
    }

    // Check the filesystem: tslib may exist in node_modules but not be part of
    // the compiled program (since tsconfig `include` typically excludes node_modules).
    if tslib_exists_on_filesystem(base_dir) {
        return Vec::new();
    }

    // tslib truly not found → TS2354 for each file needing helpers
    let mut result = Vec::new();
    for file in &program.files {
        if file.file_name.ends_with(".d.ts") {
            continue;
        }
        let helpers = required_helpers(file, options.checker.target, options.es_module_interop);
        if let Some((_helper_name, start, length)) = helpers.first() {
            result.push(Diagnostic::error(
                file.file_name.clone(),
                *start,
                *length,
                "This syntax requires an imported helper but module 'tslib' cannot be found."
                    .to_string(),
                2354,
            ));
        }
    }
    result
}

/// Emit TS2343 for each file that needs helpers but tslib lacks them.
fn emit_ts2343_for_missing_helpers(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    tslib_key: &str,
) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for file in &program.files {
        if file.file_name == tslib_key || file.file_name.ends_with(".d.ts") {
            continue;
        }

        for (helper_name, start, length) in required_helpers(file, options.checker.target, false) {
            result.push(Diagnostic::error(
                file.file_name.clone(),
                start,
                length,
                format!(
                    "This syntax requires an imported helper named '{helper_name}' which does not exist in 'tslib'. Consider upgrading your version of 'tslib'."
                ),
                2343,
            ));
        }
    }
    result
}

/// Walk up from `base_dir` looking for `node_modules/tslib`.
fn tslib_exists_on_filesystem(base_dir: &Path) -> bool {
    let mut dir = base_dir;
    loop {
        let candidate = dir.join("node_modules").join("tslib");
        if candidate.is_dir() {
            return true;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return false,
        }
    }
}

pub(super) fn required_helpers(
    file: &BoundFile,
    target: tsz_common::ScriptTarget,
    es_module_interop: bool,
) -> Vec<(&'static str, u32, u32)> {
    let mut saw_await: Option<(u32, u32)> = None;
    let mut saw_yield: Option<(u32, u32)> = None;
    let mut first_decorator: Option<(u32, u32)> = None;
    let mut first_private_id: Option<(u32, u32)> = None;

    // At ES2015+, class syntax is native — no __extends helper needed.
    let needs_extends_helper = !target.supports_es2015();

    for node_idx_raw in 0..file.arena.len() {
        let node_idx = NodeIndex(node_idx_raw as u32);
        let Some(node) = file.arena.get(node_idx) else {
            continue;
        };

        if node.kind == SyntaxKind::PrivateIdentifier as u16 && first_private_id.is_none() {
            first_private_id = Some((node.pos, node.end.saturating_sub(node.pos)));
        }

        if node.kind == syntax_kind_ext::DECORATOR && first_decorator.is_none() {
            first_decorator = Some((node.pos, node.end.saturating_sub(node.pos)));
        }

        if needs_extends_helper
            && node.kind == syntax_kind_ext::CLASS_DECLARATION
            && let Some(class_data) = file.arena.get_class(node)
            && class_data.heritage_clauses.is_some()
            && first_decorator.is_none()
            && first_private_id.is_none()
        {
            return vec![("__extends", node.pos, node.end.saturating_sub(node.pos))];
        }

        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            saw_await = Some((node.pos, node.end.saturating_sub(node.pos)));
        }
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            saw_yield = Some((node.pos, node.end.saturating_sub(node.pos)));
        }
    }

    // Decorators take priority (ES decorators handle private fields internally)
    if let Some((start, length)) = first_decorator {
        return es_decorator_helpers(file, start, length);
    }

    if let Some((start, length)) = first_private_id {
        return vec![("__classPrivateFieldSet", start, length)];
    }

    if let (Some((start, length)), Some(_)) = (saw_await, saw_yield) {
        return vec![("__asyncGenerator", start, length)];
    }

    // esModuleInterop helpers: __importStar for namespace imports/re-exports,
    // __importDefault for default named imports/re-exports.
    if es_module_interop && let Some(helper) = detect_es_module_interop_helper(file) {
        return vec![helper];
    }

    Vec::new()
}

/// Detect esModuleInterop helpers needed in a file.
///
/// Patterns:
/// - `import * as X from "m"` (non-type-only) → `__importStar` at import statement
/// - `import { default as X } from "m"` (non-type-only) → `__importDefault` at `default` keyword
/// - `export { default } from "m"` or `export { default as X } from "m"` → `__importDefault` at `default` keyword
/// - `export * as ns from "m"` → `__importStar` at export statement
///
/// Note: `import X from "m"` (bare default import) does NOT require __importDefault in tsc.
fn detect_es_module_interop_helper(file: &BoundFile) -> Option<(&'static str, u32, u32)> {
    for node_idx_raw in 0..file.arena.len() {
        let node_idx = NodeIndex(node_idx_raw as u32);
        let Some(node) = file.arena.get(node_idx) else {
            continue;
        };

        // Check import declarations: `import * as X from "m"`
        if let Some(import_decl) = file.arena.get_import_decl(node) {
            if import_decl.is_type_only {
                continue;
            }
            let Some(clause_node) = file.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = file.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only {
                continue;
            }
            let Some(bindings_node) = file.arena.get(clause.named_bindings) else {
                continue;
            };

            // `import * as X from "m"` → NAMESPACE_IMPORT
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                return Some(("__importStar", node.pos, node.end.saturating_sub(node.pos)));
            }

            // `import { ..., default as X, ... } from "m"` → NAMED_IMPORTS with a `default` specifier
            if let Some(named_imports) = file.arena.get_named_imports(bindings_node) {
                for &elem_idx in &named_imports.elements.nodes {
                    let Some(elem_node) = file.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(specifier) = file.arena.get_specifier(elem_node) else {
                        continue;
                    };
                    if specifier.is_type_only {
                        continue;
                    }
                    // Check if property_name (the original name) is "default"
                    if let Some(prop_node) = file.arena.get(specifier.property_name) {
                        if prop_node.kind == SyntaxKind::DefaultKeyword as u16 {
                            return Some((
                                "__importDefault",
                                prop_node.pos,
                                prop_node.end.saturating_sub(prop_node.pos),
                            ));
                        }
                        if let Some(ident) = file.arena.get_identifier(prop_node)
                            && ident.escaped_text == "default"
                        {
                            return Some((
                                "__importDefault",
                                prop_node.pos,
                                prop_node.end.saturating_sub(prop_node.pos),
                            ));
                        }
                    }
                }
            }
        }

        // Check export declarations
        if let Some(export_decl) = file.arena.get_export_decl(node) {
            if export_decl.is_type_only {
                continue;
            }
            // Must have a module_specifier (re-export from another module)
            if file.arena.get(export_decl.module_specifier).is_none() {
                continue;
            }

            let Some(clause_node) = file.arena.get(export_decl.export_clause) else {
                continue;
            };

            // `export * as ns from "m"` — the export_clause is a plain identifier (not NAMED_EXPORTS)
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                return Some(("__importStar", node.pos, node.end.saturating_sub(node.pos)));
            }

            // `export { default } from "m"` or `export { default as X } from "m"` → NAMED_EXPORTS
            if let Some(named_exports) = file.arena.get_named_imports(clause_node) {
                for &elem_idx in &named_exports.elements.nodes {
                    let Some(elem_node) = file.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(specifier) = file.arena.get_specifier(elem_node) else {
                        continue;
                    };
                    if specifier.is_type_only {
                        continue;
                    }
                    // For export specifiers, check property_name first (original name),
                    // then fall back to name (when there's no rename, name IS the original)
                    let check_node_idx = if file.arena.get(specifier.property_name).is_some() {
                        specifier.property_name
                    } else {
                        specifier.name
                    };
                    let Some(check_node) = file.arena.get(check_node_idx) else {
                        continue;
                    };
                    if check_node.kind == SyntaxKind::DefaultKeyword as u16 {
                        return Some((
                            "__importDefault",
                            check_node.pos,
                            check_node.end.saturating_sub(check_node.pos),
                        ));
                    }
                    if let Some(ident) = file.arena.get_identifier(check_node)
                        && ident.escaped_text == "default"
                    {
                        return Some((
                            "__importDefault",
                            check_node.pos,
                            check_node.end.saturating_sub(check_node.pos),
                        ));
                    }
                }
            }
        }
    }

    None
}

/// Determine which ES decorator helpers are needed for a file.
///
/// tsc emits all needed helper diagnostics at the position of the first decorated
/// node. The helpers depend on the class structure:
/// - `__esDecorate` + `__runInitializers`: always needed
/// - `__setFunctionName`: needed when class is anonymous, has private members
///   (static or non-static) with decorators, or is a default export
/// - `__propKey`: needed when a decorated member has a static computed property name
fn es_decorator_helpers(
    file: &BoundFile,
    first_dec_start: u32,
    first_dec_length: u32,
) -> Vec<(&'static str, u32, u32)> {
    let mut needs_set_function_name = false;
    let mut needs_prop_key = false;

    // Helper: check if a modifiers list contains a DECORATOR node
    let has_decorator_in_modifiers = |modifiers: &Option<tsz::parser::NodeList>| -> bool {
        modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&idx| {
                file.arena
                    .get(idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
            })
        })
    };

    // Helper: check if modifiers contain DefaultKeyword
    let has_default_keyword = |modifiers: &Option<tsz::parser::NodeList>| -> bool {
        modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&idx| {
                file.arena
                    .get(idx)
                    .is_some_and(|n| n.kind == SyntaxKind::DefaultKeyword as u16)
            })
        })
    };

    for node_idx_raw in 0..file.arena.len() {
        let node_idx = NodeIndex(node_idx_raw as u32);
        let Some(node) = file.arena.get(node_idx) else {
            continue;
        };

        let is_class = node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION;
        if !is_class {
            continue;
        }
        let Some(class_data) = file.arena.get_class(node) else {
            continue;
        };

        let class_has_decorator = has_decorator_in_modifiers(&class_data.modifiers);

        // Anonymous class expression → needs __setFunctionName
        let name_node = file.arena.get(class_data.name);
        let class_is_anonymous = name_node.is_none()
            || name_node.is_some_and(|n| n.kind == SyntaxKind::Unknown as u16 || n.pos == n.end);

        // export default @dec class → needs __setFunctionName
        let is_default_export = class_has_decorator && has_default_keyword(&class_data.modifiers);

        if class_is_anonymous || is_default_export {
            needs_set_function_name = true;
        }

        // Walk class members for private identifiers or static computed property names
        for &member_idx in &class_data.members.nodes {
            let Some(member) = file.arena.get(member_idx) else {
                continue;
            };

            // Scan all arena nodes within the member's span for relevant node kinds.
            // Arena stores nodes bottom-up (children before parents), so we scan all nodes.
            let is_field = member.kind == syntax_kind_ext::PROPERTY_DECLARATION;
            let mut member_has_decorator = false;
            let mut member_is_static = false;

            for child_idx_raw in 0..file.arena.len() {
                let child_idx = NodeIndex(child_idx_raw as u32);
                let Some(child) = file.arena.get(child_idx) else {
                    continue;
                };
                // Only consider nodes within the member's span
                if child.pos < member.pos || child.pos >= member.end {
                    continue;
                }
                if child.kind == syntax_kind_ext::DECORATOR {
                    member_has_decorator = true;
                }
                if child.kind == SyntaxKind::StaticKeyword as u16 {
                    member_is_static = true;
                }
                if child.kind == SyntaxKind::PrivateIdentifier as u16
                    && !is_field
                    && (member_has_decorator || class_has_decorator)
                {
                    needs_set_function_name = true;
                }
                if child.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    && member_has_decorator
                    && member_is_static
                {
                    needs_prop_key = true;
                }
            }
        }
    }

    // Build helper list in alphabetical order (matching tsc output)
    let mut helpers = vec![("__esDecorate", first_dec_start, first_dec_length)];
    if needs_prop_key {
        helpers.push(("__propKey", first_dec_start, first_dec_length));
    }
    helpers.push(("__runInitializers", first_dec_start, first_dec_length));
    if needs_set_function_name {
        helpers.push(("__setFunctionName", first_dec_start, first_dec_length));
    }
    helpers
}

pub(super) fn compute_export_hash(
    program: &MergedProgram,
    file: &BoundFile,
    file_idx: usize,
    checker: &mut CheckerState,
) -> u64 {
    let mut formatter = TypeFormatter::with_symbols(&program.type_interner, &program.symbols);
    let mut hasher = FxHasher::default();
    let mut type_str_cache: FxHashMap<TypeId, std::borrow::Cow<'static, str>> =
        FxHashMap::default();

    if let Some(file_locals) = program.file_locals.get(file_idx) {
        let mut exports: Vec<(&String, SymbolId)> = file_locals
            .iter()
            .filter_map(|(name, &sym_id)| {
                is_exported_symbol(&program.symbols, sym_id).then_some((name, sym_id))
            })
            .collect();
        exports.sort_by(|left, right| left.0.cmp(right.0));

        for (name, sym_id) in exports {
            name.hash(&mut hasher);
            let type_id = checker.get_type_of_symbol(sym_id);
            let type_str = type_str_cache
                .entry(type_id)
                .or_insert_with(|| formatter.format(type_id));
            type_str.hash(&mut hasher);
        }
    }

    let mut export_signatures = Vec::new();
    collect_export_signatures(file, checker, &mut formatter, &mut export_signatures);
    export_signatures.sort();
    for signature in export_signatures {
        signature.hash(&mut hasher);
    }

    hasher.finish()
}

pub(super) fn js_file_has_ts_check_pragma(file: &BoundFile) -> bool {
    let Some(source) = file.arena.get_source_file_at(file.source_file) else {
        return false;
    };
    let text = source.text.as_ref().to_ascii_lowercase();
    let ts_check = text.rfind("@ts-check");
    let ts_no_check = text.rfind("@ts-nocheck");
    match (ts_check, ts_no_check) {
        (Some(check_idx), Some(no_check_idx)) => check_idx > no_check_idx,
        (Some(_), None) => true,
        _ => false,
    }
}

pub(super) fn js_file_has_ts_nocheck_pragma(file: &BoundFile) -> bool {
    let Some(source) = file.arena.get_source_file_at(file.source_file) else {
        return false;
    };
    source
        .text
        .as_ref()
        .to_ascii_lowercase()
        .contains("@ts-nocheck")
}

pub(super) fn is_exported_symbol(symbols: &tsz::binder::SymbolArena, sym_id: SymbolId) -> bool {
    let Some(symbol) = symbols.get(sym_id) else {
        return false;
    };
    symbol.is_exported || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0
}

pub(super) fn collect_export_signatures(
    file: &BoundFile,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    signatures: &mut Vec<String>,
) {
    let arena = &file.arena;
    let Some(source) = arena.get_source_file_at(file.source_file) else {
        return;
    };

    for &stmt_idx in &source.statements.nodes {
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };

        if let Some(export_decl) = arena.get_export_decl(stmt) {
            if export_decl.is_default_export {
                if let Some(signature) =
                    export_default_signature(export_decl.export_clause, checker, formatter)
                {
                    signatures.push(signature);
                }
                continue;
            }

            if export_decl.module_specifier.is_none() {
                if export_decl.export_clause.is_some() {
                    let clause_node = export_decl.export_clause;
                    if arena.get_named_imports_at(clause_node).is_some() {
                        collect_local_named_export_signatures(
                            arena,
                            file.source_file,
                            clause_node,
                            checker,
                            formatter,
                            export_type_prefix(export_decl.is_type_only),
                            signatures,
                        );
                    } else {
                        collect_exported_declaration_signatures(
                            arena,
                            clause_node,
                            checker,
                            formatter,
                            export_type_prefix(export_decl.is_type_only),
                            signatures,
                        );
                    }
                }
                continue;
            }

            let module_spec = arena
                .get_literal_text(export_decl.module_specifier)
                .unwrap_or("")
                .to_string();
            if export_decl.export_clause.is_none() {
                signatures.push(format!(
                    "{}*|{}",
                    export_type_prefix(export_decl.is_type_only),
                    module_spec
                ));
                continue;
            }

            let clause_node = export_decl.export_clause;
            if let Some(named) = arena.get_named_imports_at(clause_node) {
                let mut specifiers = Vec::new();
                for &spec_idx in &named.elements.nodes {
                    let Some(spec) = arena.get_specifier_at(spec_idx) else {
                        continue;
                    };
                    let name = arena.get_identifier_text(spec.name).unwrap_or("");
                    if spec.property_name.is_none() {
                        specifiers.push(name.to_string());
                    } else {
                        let property = arena.get_identifier_text(spec.property_name).unwrap_or("");
                        specifiers.push(format!("{property} as {name}"));
                    }
                }
                specifiers.sort();
                signatures.push(format!(
                    "{}{{{}}}|{}",
                    export_type_prefix(export_decl.is_type_only),
                    specifiers.join(","),
                    module_spec
                ));
            } else if let Some(name) = arena.get_identifier_text(clause_node) {
                signatures.push(format!(
                    "{}* as {}|{}",
                    export_type_prefix(export_decl.is_type_only),
                    name,
                    module_spec
                ));
            }

            continue;
        }

        if let Some(export_assignment) = arena.get_export_assignment(stmt)
            && export_assignment.expression.is_some()
        {
            let type_id = checker.get_type_of_node(export_assignment.expression);
            let type_str = formatter.format(type_id);
            signatures.push(format!("export=:{type_str}"));
        }
    }
}

fn collect_local_named_export_signatures(
    arena: &NodeArena,
    source_file: NodeIndex,
    named_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let Some(named) = arena.get_named_imports_at(named_idx) else {
        return;
    };

    for &spec_idx in &named.elements.nodes {
        let Some(spec) = arena.get_specifier_at(spec_idx) else {
            continue;
        };
        let exported_name = if spec.name.is_some() {
            arena.get_identifier_text(spec.name).unwrap_or("")
        } else {
            arena.get_identifier_text(spec.property_name).unwrap_or("")
        };
        if exported_name.is_empty() {
            continue;
        }
        let local_name = if spec.property_name.is_some() {
            arena.get_identifier_text(spec.property_name).unwrap_or("")
        } else {
            exported_name
        };
        let type_id = find_local_declaration(arena, source_file, local_name)
            .map_or(TypeId::ANY, |decl_idx| checker.get_type_of_node(decl_idx));
        let type_str = formatter.format(type_id);
        signatures.push(format!("{type_prefix}{exported_name}:{type_str}"));
    }
}

fn collect_exported_declaration_signatures(
    arena: &NodeArena,
    decl_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let Some(node) = arena.get(decl_idx) else {
        return;
    };

    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
        if let Some(var_stmt) = arena.get_variable(node) {
            for &list_idx in &var_stmt.declarations.nodes {
                collect_exported_declaration_signatures(
                    arena,
                    list_idx,
                    checker,
                    formatter,
                    type_prefix,
                    signatures,
                );
            }
        }
        return;
    }

    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(list) = arena.get_variable(node) {
            for &decl_idx in &list.declarations.nodes {
                collect_exported_declaration_signatures(
                    arena,
                    decl_idx,
                    checker,
                    formatter,
                    type_prefix,
                    signatures,
                );
            }
        }
        return;
    }

    if let Some(var_decl) = arena.get_variable_declaration(node) {
        if let Some(name) = arena.get_identifier_text(var_decl.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(func) = arena.get_function(node) {
        if let Some(name) = arena.get_identifier_text(func.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(class) = arena.get_class(node) {
        if let Some(name) = arena.get_identifier_text(class.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(interface) = arena.get_interface(node) {
        if let Some(name) = arena.get_identifier_text(interface.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(type_alias) = arena.get_type_alias(node) {
        if let Some(name) = arena.get_identifier_text(type_alias.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(enum_decl) = arena.get_enum(node) {
        if let Some(name) = arena.get_identifier_text(enum_decl.name) {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
        return;
    }

    if let Some(module_decl) = arena.get_module(node) {
        let name = arena
            .get_identifier_text(module_decl.name)
            .or_else(|| arena.get_literal_text(module_decl.name));
        if let Some(name) = name {
            push_exported_signature(name, decl_idx, checker, formatter, type_prefix, signatures);
        }
    }
}

fn push_exported_signature(
    name: &str,
    decl_idx: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
    type_prefix: &str,
    signatures: &mut Vec<String>,
) {
    let type_id = checker.get_type_of_node(decl_idx);
    let type_str = formatter.format(type_id);
    signatures.push(format!("{type_prefix}{name}:{type_str}"));
}

pub(super) fn find_local_declaration(
    arena: &NodeArena,
    source_file: NodeIndex,
    name: &str,
) -> Option<NodeIndex> {
    let source = arena.get_source_file_at(source_file)?;

    for &stmt_idx in &source.statements.nodes {
        let Some(stmt) = arena.get(stmt_idx) else {
            continue;
        };
        if let Some(export_decl) = arena.get_export_decl(stmt) {
            if export_decl.export_clause.is_none() {
                continue;
            }
            let clause_idx = export_decl.export_clause;
            if arena.get_named_imports_at(clause_idx).is_some() {
                continue;
            }
            if let Some(found) = find_local_declaration_in_node(arena, clause_idx, name) {
                return Some(found);
            }
            continue;
        }

        if let Some(found) = find_local_declaration_in_node(arena, stmt_idx, name) {
            return Some(found);
        }
    }

    None
}

fn find_local_declaration_in_node(
    arena: &NodeArena,
    node_idx: NodeIndex,
    name: &str,
) -> Option<NodeIndex> {
    let node = arena.get(node_idx)?;

    if let Some(var_decl) = arena.get_variable_declaration(node) {
        if let Some(decl_name) = arena.get_identifier_text(var_decl.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
        if let Some(var_stmt) = arena.get_variable(node) {
            for &list_idx in &var_stmt.declarations.nodes {
                if let Some(found) = find_local_declaration_in_node(arena, list_idx, name) {
                    return Some(found);
                }
            }
        }
        return None;
    }

    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(list) = arena.get_variable(node) {
            for &decl_idx in &list.declarations.nodes {
                if let Some(found) = find_local_declaration_in_node(arena, decl_idx, name) {
                    return Some(found);
                }
            }
        }
        return None;
    }

    if let Some(func) = arena.get_function(node) {
        if let Some(decl_name) = arena.get_identifier_text(func.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(class) = arena.get_class(node) {
        if let Some(decl_name) = arena.get_identifier_text(class.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(interface) = arena.get_interface(node) {
        if let Some(decl_name) = arena.get_identifier_text(interface.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(type_alias) = arena.get_type_alias(node) {
        if let Some(decl_name) = arena.get_identifier_text(type_alias.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(enum_decl) = arena.get_enum(node) {
        if let Some(decl_name) = arena.get_identifier_text(enum_decl.name)
            && decl_name == name
        {
            return Some(node_idx);
        }
        return None;
    }

    if let Some(module_decl) = arena.get_module(node) {
        let decl_name = arena
            .get_identifier_text(module_decl.name)
            .or_else(|| arena.get_literal_text(module_decl.name));
        if let Some(decl_name) = decl_name
            && decl_name == name
        {
            return Some(node_idx);
        }
    }

    None
}

fn export_default_signature(
    export_clause: NodeIndex,
    checker: &mut CheckerState,
    formatter: &mut TypeFormatter,
) -> Option<String> {
    if export_clause.is_none() {
        return None;
    }
    let type_id = if let Some(sym_id) = checker.ctx.binder.get_node_symbol(export_clause) {
        checker.get_type_of_symbol(sym_id)
    } else {
        checker.get_type_of_node(export_clause)
    };
    let type_str = formatter.format(type_id);
    Some(format!("default:{type_str}"))
}

pub(super) const fn export_type_prefix(is_type_only: bool) -> &'static str {
    if is_type_only { "type:" } else { "" }
}

/// Convert specific parser diagnostics to `TS8xxx` equivalents for JS files.
/// tsc's parser is lenient with TypeScript-only syntax in JS files, so some
/// parser errors should be converted to `TS8xxx` checker equivalents rather
/// than being suppressed entirely.
pub(super) fn convert_js_parse_diagnostics_to_ts8xxx(
    parse_diagnostics: &[ParseDiagnostic],
    file_name: &str,
    out: &mut Vec<Diagnostic>,
    source_text: Option<&str>,
) {
    for diag in parse_diagnostics {
        // TS1162 ("An object member cannot be declared optional.") ->
        // TS8009 ("The '?' modifier can only be used in TypeScript files.")
        // tsc's parser accepts `?` on object members in JS files; the checker
        // emits TS8009 only for method-like optionals (e.g., `m?()`), not for
        // property optionals (e.g., `prop?: val`). We distinguish by checking
        // if `(` follows the `?`.
        if diag.code == 1162 {
            let is_method_optional = source_text.is_some_and(|src| {
                let after_q = (diag.start + diag.length) as usize;
                // Skip whitespace after `?` and check for `(`
                src.get(after_q..)
                    .map(|s| s.trim_start().starts_with('(') || s.trim_start().starts_with('<'))
                    .unwrap_or(false)
            });
            if is_method_optional {
                out.push(Diagnostic::error(
                    file_name.to_string(),
                    diag.start,
                    diag.length,
                    "The '?' modifier can only be used in TypeScript files.".to_string(),
                    8009,
                ));
            }
        }
        // All other parser diagnostics are suppressed for JS files.
    }
}

pub(super) fn parse_diagnostic_to_checker(
    file_name: &str,
    diagnostic: &ParseDiagnostic,
) -> Diagnostic {
    Diagnostic::error(
        file_name.to_string(),
        diagnostic.start,
        diagnostic.length,
        diagnostic.message.clone(),
        diagnostic.code,
    )
}

pub(super) fn filtered_parse_diagnostics(
    parse_diagnostics: &[ParseDiagnostic],
) -> Vec<&ParseDiagnostic> {
    let has_real_syntax_error = parse_diagnostics
        .iter()
        .any(|diagnostic| is_real_syntax_error(diagnostic.code));

    // tsc emits these codes via grammarErrorOnNode in the checker, which checks
    // hasParseDiagnostics(sourceFile) and suppresses when any parse error exists.
    // In tsz, these are emitted by the parser. We post-filter them here to match
    // tsc's suppression behavior. We only suppress grammar codes when there's a
    // non-grammar parse error present (e.g., TS1005, TS1109) to avoid suppressing
    // grammar codes that are the file's only diagnostic.
    let has_non_grammar_parse_error = parse_diagnostics
        .iter()
        .any(|d| !matches!(d.code, 1009 | 1185 | 1214 | 1262) && !is_parser_grammar_code(d.code));

    parse_diagnostics
        .iter()
        .filter(|diagnostic| {
            // Existing: suppress TS1184 when real syntax errors exist
            if has_real_syntax_error && diagnostic.code == 1184 {
                return false;
            }
            // Suppress parser-emitted grammar codes that tsc would emit via
            // grammarErrorOnNode (checker-side, suppressed by hasParseDiagnostics).
            if has_non_grammar_parse_error && is_parser_grammar_code(diagnostic.code) {
                return false;
            }
            true
        })
        .collect()
}

/// Parser-emitted codes that tsc emits via grammarErrorOnNode in the checker.
/// These should be suppressed when the file has parse errors, matching tsc behavior.
/// Only includes codes confirmed to be checker-side grammar checks in tsc that
/// our parser emits instead.
const fn is_parser_grammar_code(code: u32) -> bool {
    matches!(
        code,
        1014 // A rest parameter must be last in a parameter list
        | 1017 // An index signature cannot have a rest parameter
        | 1019 // An index signature parameter cannot have a question mark
        | 1021 // An index signature must have a type annotation
        | 1031 // '{0}' modifier cannot appear on class elements of this kind
        | 1042 // 'async' modifier cannot be used here
        | 1044 // '{0}' modifier cannot appear on a module or namespace element
        | 1054 // A 'get' accessor cannot have parameters
        | 1070 // '{0}' modifier cannot appear on a type member
        | 1071 // An accessor must have a body (interface/ambient)
        | 1089 // '{0}' modifier cannot appear on a constructor declaration
        | 1090 // '{0}' modifier cannot appear on a parameter
        | 1093 // Type annotation cannot appear on a constructor declaration
        | 1095 // A 'set' accessor cannot have a return type annotation
        | 1096 // An index signature must have exactly one parameter
        | 1097 // '{0}' list cannot be empty
        | 1110 // Type expected
        | 1113 // A 'default' clause cannot appear more than once in a 'switch' statement
        | 1114 // Duplicate label
        | 1123 // Variable declaration list cannot be empty
        | 1162 // An object member cannot be declared optional
        | 1163 // A 'yield' expression is only allowed in a generator body
        | 1171 // A comma expression is not allowed in a computed property name
        | 1172 // extends clause already seen
        | 1174 // Classes can only extend a single class
        | 1182 // A destructuring declaration must have an initializer
        | 1184 // Modifiers cannot appear here
        | 1191 // An import declaration cannot have modifiers
        | 1197 // Catch clause variable cannot have an initializer
        | 1200 // Line terminator not permitted before arrow
        | 1206 // Decorators are not valid here
        | 1210 // Code contained in a class is evaluated in strict mode
        | 18037 // 'await' expression cannot be used inside a class static block
        | 18041 // A 'return' statement cannot be used inside a class static block
    )
}

/// `TS1xxx` codes that tsc is known to emit for JavaScript files.
/// tsc's parser is lenient with TypeScript-only syntax in JS files and its
/// checker grammar checks (`grammarErrorOnNode`) are suppressed for TS-only
/// constructs. Only these `TS1xxx` codes are legitimately emitted for JS.
pub(super) const fn is_ts1xxx_allowed_in_js(code: u32) -> bool {
    matches!(
        code,
        1003 // Identifier expected
        | 1014 // A rest parameter must be last in a parameter list
        | 1016 // A required parameter cannot follow an optional parameter
        | 1064 // The return type of an async function must be 'void' or 'Promise<T>'
        | 1069 // Unexpected token; expected type parameter
        | 1092 // Type parameters cannot appear on a constructor declaration
        | 1093 // Type annotation cannot appear on a constructor declaration
        | 1098 // Type parameter list cannot be empty
        | 1100 // Invalid use of 'arguments' in strict mode
        | 1101 // 'with' statements are not allowed in strict mode
        | 1102 // SyntaxError (strict mode binding)
        | 1104 // A 'continue' statement can only be used within an enclosing iteration statement
        | 1105 // A 'break' statement can only be used within an enclosing iteration statement
        | 1107 // Jump target cannot cross function boundary
        | 1109 // Expression expected
        | 1110 // Type expected
        | 1111 // Private field must be declared in an enclosing class
        | 1139 // Can not use 'JSDoc' type in TS
        | 1196 // Catch clause variable type annotation
        | 1206 // Decorators are not valid here
        | 1210 // Code contained in a class is evaluated in strict mode
        | 1215 // Identifier expected; 'await' is a reserved word
        | 1223 // Constructor implementation is missing
        | 1228 // A type predicate is only allowed in return type position
        | 1262 // Identifier expected; 'await' at top level
        | 1273 // '@typedef' tag should either have a type annotation or be followed by '@property' or '@member' tags
        | 1274 // JSDoc '@typedef' tag should either have a type annotation or be followed by '@property' or '@member' tags
        | 1277 // 'JSDoc' types may only appear in type positions
        | 1308 // 'await' expressions are only allowed within async functions
        | 1344 // Not all code paths return a value / unreachable code
        | 1359 // Identifier expected; 'await' is reserved in async
        | 1360 // '@satisfies' types can only be used in type positions
        | 1382 // Unexpected token
        | 1464 // Import assertion/attribute
        | 1470 // 'import.meta' outside module
        | 1473 // Module declaration names
        | 1479 // This syntax is only allowed when 'allowImportingTsExtensions'
        | 1489 // Duplicate identifier
    )
}

/// Checker-emitted grammar codes outside the `TS1xxx` range that should be
/// suppressed for JS files. tsc doesn't emit these for JavaScript because
/// its parser handles the constructs leniently.
pub(super) const fn is_checker_grammar_code_suppressed_in_js(code: u32) -> bool {
    matches!(
        code,
        17012 // '{0}' is not a valid meta-property for keyword '{1}'
        | 18016 // Private identifiers are not allowed outside class bodies
    )
}

pub(super) fn create_binder_from_bound_file(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let mut file_locals = SymbolTable::new();

    if file_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }

    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    // Merge module augmentations from all files
    // When checking a file, we need access to augmentations from all other files
    let mut merged_module_augmentations: rustc_hash::FxHashMap<
        String,
        Vec<tsz::binder::ModuleAugmentation>,
    > = rustc_hash::FxHashMap::default();

    for other_file in &program.files {
        for (spec, augs) in &other_file.module_augmentations {
            merged_module_augmentations
                .entry(spec.clone())
                .or_default()
                .extend(augs.iter().map(|aug| {
                    tsz::binder::ModuleAugmentation::with_arena(
                        aug.name.clone(),
                        aug.node,
                        Arc::clone(&other_file.arena),
                    )
                }));
        }
    }

    // Merge augmentation target module mappings from all files
    let mut merged_augmentation_target_modules: rustc_hash::FxHashMap<
        tsz::binder::SymbolId,
        String,
    > = rustc_hash::FxHashMap::default();
    for other_file in &program.files {
        for (&sym_id, module_spec) in &other_file.augmentation_target_modules {
            merged_augmentation_target_modules.insert(sym_id, module_spec.clone());
        }
    }

    // Merge global augmentations from all files
    // Each augmentation is tagged with its source arena for cross-file resolution.
    let mut merged_global_augmentations: rustc_hash::FxHashMap<
        String,
        Vec<tsz::binder::GlobalAugmentation>,
    > = rustc_hash::FxHashMap::default();

    for other_file in &program.files {
        for (name, decls) in &other_file.global_augmentations {
            merged_global_augmentations
                .entry(name.clone())
                .or_default()
                .extend(decls.iter().map(|aug| {
                    tsz::binder::GlobalAugmentation::with_arena(
                        aug.node,
                        Arc::clone(&other_file.arena),
                    )
                }));
        }
    }

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: merged_global_augmentations,
            module_augmentations: merged_module_augmentations,
            augmentation_target_modules: merged_augmentation_target_modules,
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            wildcard_reexports_type_only: program.wildcard_reexports_type_only.clone(),
            symbol_arenas: program.symbol_arenas.clone(),
            declaration_arenas: program.declaration_arenas.clone(),
            cross_file_node_symbols: program.cross_file_node_symbols.clone(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            modules_with_export_equals: Default::default(),
            flow_nodes: file.flow_nodes.clone(),
            node_flow: file.node_flow.clone(),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            alias_partners: program.alias_partners.clone(),
        },
    );

    binder.declared_modules = program.declared_modules.clone();
    // Restore is_external_module from BoundFile to preserve per-file state
    binder.is_external_module = file.is_external_module;
    // Reconstructed program binders already contain lib symbols remapped into the
    // unified symbol arena, so preserve that invariant instead of falling back to
    // legacy raw-lib lookup paths.
    binder.set_lib_symbols_merged(true);
    binder.lib_binders = program.lib_binders.clone();
    // Track lib-originating symbols so unused checking can skip them
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();

    binder
}

/// Build a binder for cross-file symbol and type resolution.
///
/// Cross-file delegation can use entries from `CheckerContext::all_binders` for
/// full semantic type computation, not just export-table lookups. Reuse the same
/// binder construction path as a normal file check so delegated child checkers
/// have access to the owning file's symbols, declaration arenas, and augmentations.
pub(super) fn create_cross_file_lookup_binder(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    create_binder_from_bound_file(file, program, file_idx)
}

// --- TS directive suppression ---
/// Build a line-start table: `line_starts[i]` is the byte offset of the first char on line `i`.
fn build_line_starts(text: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    starts
}

/// Get the 0-based line number for a byte offset.
fn line_of_offset(line_starts: &[u32], offset: u32) -> u32 {
    match line_starts.binary_search(&offset) {
        Ok(exact) => exact as u32,
        Err(insert) => insert.saturating_sub(1) as u32,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectiveKind {
    ExpectError,
    Ignore,
}

/// Characters that can follow `@ts-expect-error` / `@ts-ignore` in a valid directive.
const fn is_directive_separator(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b':' | b'*' | b'/')
}

/// Check if a comment text contains `@ts-expect-error` or `@ts-ignore`.
fn find_directive_in_text(comment: &str) -> Option<(DirectiveKind, usize)> {
    if let Some(pos) = comment.find("@ts-expect-error") {
        let after = pos + "@ts-expect-error".len();
        if after >= comment.len() || is_directive_separator(comment.as_bytes()[after]) {
            return Some((DirectiveKind::ExpectError, pos));
        }
    }
    if let Some(pos) = comment.find("@ts-ignore") {
        let after = pos + "@ts-ignore".len();
        if after >= comment.len() || is_directive_separator(comment.as_bytes()[after]) {
            return Some((DirectiveKind::Ignore, pos));
        }
    }
    None
}

/// A `@ts-expect-error` or `@ts-ignore` directive found in a source file comment.
struct TsDirective {
    /// True for `@ts-expect-error`, false for `@ts-ignore`.
    is_expect_error: bool,
    /// The 0-based line number that this directive suppresses (the line after the comment).
    suppressed_line: u32,
    /// Byte offset of the start of the comment containing the directive.
    comment_start: u32,
    /// Byte length of the comment containing the directive.
    comment_length: u32,
    /// Byte offset of the `@ts-expect-error` text within the comment.
    directive_text_start: u32,
}

/// Scan source text for `@ts-expect-error` and `@ts-ignore` directives in comments.
fn find_ts_directives(text: &str) -> Vec<TsDirective> {
    let line_starts = build_line_starts(text);
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut directives = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'/' && i + 1 < len {
            if bytes[i + 1] == b'/' {
                // Single-line comment
                let comment_start = i as u32;
                let line_end = bytes[i..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map(|offset| i + offset)
                    .unwrap_or(len);
                let comment_text = &text[i..line_end];
                let comment_length = (line_end - i) as u32;

                if let Some((kind, rel_offset)) = find_directive_in_text(comment_text) {
                    let comment_line = line_of_offset(&line_starts, comment_start);
                    directives.push(TsDirective {
                        is_expect_error: kind == DirectiveKind::ExpectError,
                        suppressed_line: comment_line + 1,
                        comment_start,
                        comment_length,
                        directive_text_start: comment_start + rel_offset as u32,
                    });
                }
                i = line_end;
                continue;
            } else if bytes[i + 1] == b'*' {
                // Multi-line comment
                let comment_start = i as u32;
                let close = text[i + 2..]
                    .find("*/")
                    .map(|offset| i + 2 + offset + 2)
                    .unwrap_or(len);
                let comment_text = &text[i..close];
                let comment_length = (close - i) as u32;

                if let Some((kind, rel_offset)) = find_directive_in_text(comment_text) {
                    let close_line = line_of_offset(&line_starts, (close.saturating_sub(1)) as u32);
                    directives.push(TsDirective {
                        is_expect_error: kind == DirectiveKind::ExpectError,
                        suppressed_line: close_line + 1,
                        comment_start,
                        comment_length,
                        directive_text_start: comment_start + rel_offset as u32,
                    });
                }
                i = close;
                continue;
            }
        }

        // Skip string literals to avoid false positives
        if bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'`' {
            let quote = bytes[i];
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        i += 1;
    }

    directives
}

/// Apply `@ts-expect-error` and `@ts-ignore` directive suppression to diagnostics.
///
/// 1. Finds all directive comments in the source text
/// 2. Suppresses diagnostics on the line following each directive
/// 3. Emits TS2578 for unused `@ts-expect-error` directives
pub(super) fn apply_ts_directive_suppression(
    file_name: &str,
    source_text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let directives = find_ts_directives(source_text);
    if directives.is_empty() {
        return;
    }

    let line_starts = build_line_starts(source_text);

    // Check for @ts-nocheck — suppresses TS2578 for unused directives.
    let has_ts_nocheck = source_text.to_ascii_lowercase().contains("@ts-nocheck");

    let mut directive_used = vec![false; directives.len()];

    // Suppress diagnostics on directive-targeted lines
    diagnostics.retain(|diag| {
        let diag_line = line_of_offset(&line_starts, diag.start);
        for (idx, directive) in directives.iter().enumerate() {
            if diag_line == directive.suppressed_line {
                directive_used[idx] = true;
                return false;
            }
        }
        true
    });

    // Emit TS2578 for unused @ts-expect-error directives
    if !has_ts_nocheck {
        for (idx, directive) in directives.iter().enumerate() {
            if directive.is_expect_error && !directive_used[idx] {
                let directive_line = line_of_offset(&line_starts, directive.directive_text_start);
                let line_start_offset = line_starts[directive_line as usize];
                let comment_end = directive.comment_start + directive.comment_length;
                let length = comment_end.saturating_sub(line_start_offset);
                diagnostics.push(Diagnostic::error(
                    file_name.to_string(),
                    line_start_offset,
                    length,
                    "Unused '@ts-expect-error' directive.".to_string(),
                    2578,
                ));
            }
        }
    }
}

/// Classify a parse diagnostic code as a "real" syntax error (actual parse failure)
/// vs a grammar/semantic check emitted during parsing.
///
/// Real syntax errors indicate that the parser couldn't parse the source normally
/// and had to recover. tsc propagates `ThisNodeHasError` flags from these errors
/// to suppress cascading semantic errors like TS2304.
///
/// Grammar checks (e.g., strict mode violations, decorator errors) are emitted
/// during parsing but don't indicate parse failure — tsc still emits TS2304 for
/// undeclared names in these files.
pub(super) const fn is_real_syntax_error(code: u32) -> bool {
    matches!(
        code,
        1005  // '{0}' expected
        // Note: TS1009 (Trailing comma not allowed) is intentionally excluded.
        // It does not corrupt the AST enough to suppress semantic errors like
        // TS2304. Files with only TS1009 parse errors (e.g., `extends A,`)
        // still have valid identifiers that need name resolution.
        | 1014 // A rest parameter must be last in a parameter list
        | 1036 // Statements are not allowed in ambient contexts
        | 1109 // Expression expected
        | 1110 // Type expected
        | 1126 // Unexpected end of text
        | 1127 // Invalid character
        | 1128 // Declaration or statement expected
        | 1129 // '{' or ';' expected
        | 1130 // '}' expected
        | 1131 // Property assignment expected
        | 1134 // Variable declaration expected
        | 1135 // Argument expression expected
        | 1136 // Property or signature expected
        | 1137 // Expression or comma expected
        | 1138 // Parameter declaration expected
        | 1141 // Type parameter declaration expected
        | 1146 // Declaration expected
        | 1155 // 'const' declarations must be initialized
        | 1160 // Unterminated template literal
        | 1161 // Unterminated regular expression literal
        | 1002 // Unterminated string literal
        | 1003 // Identifier expected
        | 1006 // A file cannot have a reference to itself
        | 1007 // The parser expected to find a '}'
        | 1010 // 'while' expected
        | 1011 // '(' or '<' expected
        | 1012 // '{' expected
        | 1035 // Only ambient modules can use quoted names
        | 1101 // 'with' statements are not allowed in strict mode
        | 1103 // A character literal must contain exactly one character
        | 1121 // Octal literals are not allowed in strict mode
        | 1124 // Digit expected
        | 1144 // '{' or ';' expected
        | 1145 // '{' or JSX element expected
        | 1147 // Import declarations in a namespace cannot reference a module
        | 1164 // Computed property names are not allowed in enums
        | 1185 // Merge conflict marker encountered
        | 1191 // An import declaration cannot have modifiers
        | 1313 // 'else' is not allowed after rest element
        | 1351 // An identifier or keyword cannot immediately follow a numeric literal
        | 1357 // A default clause cannot appear more than once
        | 1378 // Top-level 'for await' loops are only allowed...
        | 1432 // 'await' expressions are only allowed within async functions
        | 1434 // Top-level 'await' expressions are only allowed...
        | 1382 // Unexpected token. Did you mean `{'>'}` or `&gt;`? (JSX)
        | 1442 // Identifier or expression expected (TS-only construct in JS)
        | 1477 // Member must have an initializer
    )
}

/// Classify a parse diagnostic as a **structural** parse error — one that causes
/// actual AST malformation and error recovery, leading to cascading semantic errors.
///
/// This is a more restrictive subset of `is_real_syntax_error`. It excludes:
/// - Grammar checks that don't affect AST structure (strict mode, trailing commas)
/// - Contextual restrictions that don't cause parse recovery (import modifiers, etc.)
///
/// Used for the cascading suppression heuristic: semantic errors near structural
/// parse failures are likely artifacts of error recovery and should be suppressed.
pub(super) const fn is_structural_parse_error(code: u32) -> bool {
    matches!(
        code,
        1002  // Unterminated string literal
        | 1003 // Identifier expected
        | 1005 // '{0}' expected (missing token)
        | 1007 // The parser expected to find a '}'
        | 1010 // 'while' expected
        | 1011 // '(' or '<' expected
        | 1012 // '{' expected
        | 1109 // Expression expected
        | 1110 // Type expected
        | 1124 // Digit expected
        | 1126 // Unexpected end of text
        | 1127 // Invalid character
        | 1128 // Declaration or statement expected
        | 1129 // '{' or ';' expected
        | 1130 // '}' expected
        | 1131 // Property assignment expected
        | 1134 // Variable declaration expected
        | 1135 // Argument expression expected
        | 1136 // Property or signature expected
        | 1137 // Expression or comma expected
        | 1138 // Parameter declaration expected
        | 1141 // Type parameter declaration expected
        | 1144 // '{' or ';' expected
        | 1145 // '{' or JSX element expected
        | 1146 // Declaration expected
        | 1155 // 'const' declarations must be initialized
        | 1160 // Unterminated template literal
        | 1161 // Unterminated regular expression literal
        | 1185 // Merge conflict marker encountered
        | 1313 // 'else' is not allowed after rest element
        | 1351 // An identifier or keyword cannot immediately follow a numeric literal
        | 1382 // Unexpected token in JSX
        | 1442 // Identifier or expression expected
    )
}

/// Semantic diagnostic codes (>= 2000) that tsc allows through for plain JS files.
/// Mirrors tsc's `plainJSErrors` set from `program.ts`.
pub(super) const fn is_plain_js_allowed_code(code: u32) -> bool {
    matches!(
        code,
        2451  // Cannot redeclare block-scoped variable '{0}'
        | 2492 // Cannot redeclare identifier '{0}' in catch clause
        | 2528 // A module cannot have multiple default exports
        | 2752 // The first export default is here
        | 2753 // Another export default is here
        | 2774 // This condition will always return true since this function is always defined
        | 2801 // This condition will always return true since this '{0}' is always defined
        | 2803 // Cannot assign to private method '{0}'. Private methods are not writable
        | 2839 // This condition will always return '{0}' since JS compares objects by reference
        | 2845 // This condition will always return '{0}'
        | 18013 // Property '{0}' is not accessible outside class '{1}' (private identifier)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse source text and return the `BoundFile` from a merged program.
    fn bound_file(source: &str) -> BoundFile {
        let bind_result =
            parallel::parse_and_bind_single("test.ts".to_string(), source.to_string());
        let program = parallel::merge_bind_results(vec![bind_result]);
        program.files.into_iter().next().unwrap()
    }

    /// Extract helper names from `required_helpers` (at ES5 target by default).
    fn helper_names(source: &str) -> Vec<&'static str> {
        helper_names_at(source, tsz_common::ScriptTarget::ES5)
    }

    fn helper_names_at(source: &str, target: tsz_common::ScriptTarget) -> Vec<&'static str> {
        let file = bound_file(source);
        required_helpers(&file, target, false)
            .into_iter()
            .map(|(name, _, _)| name)
            .collect()
    }

    #[test]
    fn plain_class_needs_no_helpers() {
        assert!(helper_names("class C { method() {} }").is_empty());
    }

    #[test]
    fn private_field_emits_class_private_field_set() {
        let helpers = helper_names("class C { #foo = 1; }");
        assert_eq!(helpers, vec!["__classPrivateFieldSet"]);
    }

    #[test]
    fn decorated_class_emits_es_decorate_and_run_initializers() {
        let helpers = helper_names("declare var dec: any;\n@dec class C { method() {} }");
        assert!(helpers.contains(&"__esDecorate"), "got: {helpers:?}");
        assert!(helpers.contains(&"__runInitializers"), "got: {helpers:?}");
        // Named non-default class should not need __setFunctionName
        assert!(!helpers.contains(&"__setFunctionName"), "got: {helpers:?}");
    }

    #[test]
    fn decorated_class_with_private_method_emits_set_function_name() {
        let helpers = helper_names("declare var dec: any;\n@dec class C { #privateMethod() {} }");
        assert!(helpers.contains(&"__esDecorate"), "got: {helpers:?}");
        assert!(helpers.contains(&"__runInitializers"), "got: {helpers:?}");
        assert!(
            helpers.contains(&"__setFunctionName"),
            "private method should trigger __setFunctionName, got: {helpers:?}"
        );
    }

    #[test]
    fn decorator_takes_priority_over_private_field() {
        let helpers = helper_names("declare var dec: any;\n@dec class C { #foo = 1; }");
        // ES decorators handle private fields internally
        assert!(helpers.contains(&"__esDecorate"), "got: {helpers:?}");
        assert!(
            !helpers.contains(&"__classPrivateFieldSet"),
            "decorator should take priority, got: {helpers:?}"
        );
    }

    #[test]
    fn class_with_extends_emits_extends_helper() {
        let helpers = helper_names("class Base {} class Derived extends Base {}");
        assert_eq!(helpers, vec!["__extends"]);
    }

    #[test]
    fn class_with_extends_no_helper_at_es2015() {
        // At ES2015+, class syntax is native — __extends is not needed
        let helpers = helper_names_at(
            "class Base {} class Derived extends Base {}",
            tsz_common::ScriptTarget::ES2015,
        );
        assert!(
            !helpers.contains(&"__extends"),
            "ES2015 target should not need __extends, got: {helpers:?}"
        );
    }
}
