//! Utility functions for the compilation driver's checking phase:
//! export hash computation, tslib helper detection, binder construction,
//! parse diagnostic conversion, and pragma detection.

use super::*;

#[derive(Clone, Copy)]
struct TslibHelperRequirement {
    name: &'static str,
    start: u32,
    length: u32,
    required_parameter_count: Option<usize>,
}

pub(super) fn detect_missing_tslib_helper_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    file_is_esm_map: &rustc_hash::FxHashMap<String, bool>,
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
            return emit_tslib_helper_diagnostics(
                program,
                options,
                &tslib_file.file_name,
                file_is_esm_map,
            );
        }

        // When the file is a `tslib.d.ts` that contains `declare module "tslib" { ... }`,
        // the file-level exports are empty but the module declarations contain the helpers.
        // Check if the tslib module has non-empty ambient exports, OR if the source text
        // of the file contains actual helper function declarations (the binder may not
        // always register ambient module function declarations as module exports).
        if program.declared_modules.contains("tslib") {
            let tslib_ambient_has_exports = program
                .module_exports
                .get("tslib")
                .is_some_and(|exports| !exports.is_empty());
            if tslib_ambient_has_exports {
                return emit_tslib_helper_diagnostics(program, options, "tslib", file_is_esm_map);
            }
            // Scan the source text for helper function declarations.
            // A `declare module "tslib" { export {}; }` with no helpers will not match.
            let source = &tslib_file.arena.source_files.first().map(|sf| &sf.text);
            if let Some(source) = source
                && (source.contains("__importStar")
                    || source.contains("__importDefault")
                    || source.contains("__extends")
                    || source.contains("__rest")
                    || source.contains("__decorate")
                    || source.contains("__metadata")
                    || source.contains("__awaiter")
                    || source.contains("__generator")
                    || source.contains("__spread")
                    || source.contains("__values"))
            {
                return Vec::new();
            }
        }

        if let Some(source) = tslib_file.arena.source_files.first().map(|sf| &*sf.text)
            && let Some(helper_parameter_counts) = source_tslib_helper_parameter_counts(source)
            && !helper_parameter_counts.is_empty()
        {
            return emit_tslib_helper_diagnostics_from_counts(
                program,
                options,
                &helper_parameter_counts,
                file_is_esm_map,
            );
        }

        return emit_tslib_helper_diagnostics(
            program,
            options,
            &tslib_file.file_name,
            file_is_esm_map,
        );
    }

    // Check if tslib is declared as an ambient module (`declare module "tslib" { ... }`).
    // When found, use its module_exports to check for specific helpers.
    if program.declared_modules.contains("tslib") {
        let tslib_exports_empty = program
            .module_exports
            .get("tslib")
            .is_none_or(tsz_binder::SymbolTable::is_empty);

        if !tslib_exports_empty {
            return emit_tslib_helper_diagnostics(program, options, "tslib", file_is_esm_map);
        }

        return emit_tslib_helper_diagnostics(program, options, "tslib", file_is_esm_map);
    }

    // Check the filesystem only when the program appears to be backed by real
    // on-disk files and normal automatic type loading is enabled. Virtual or
    // isolated programs (like conformance harnesses using `@noTypesAndSymbols`)
    // must not inherit tslib availability from the host workspace.
    if !options.checker.no_types_and_symbols
        && program_appears_filesystem_backed(program)
        && let Some(tslib_path) = filesystem_tslib_declaration(base_dir)
    {
        if let Some(helper_parameter_counts) = filesystem_tslib_helper_parameter_counts(&tslib_path)
        {
            return emit_tslib_helper_diagnostics_from_counts(
                program,
                options,
                &helper_parameter_counts,
                file_is_esm_map,
            );
        }
        return Vec::new();
    }

    // tslib truly not found → TS2354 for each file needing helpers
    let mut result = Vec::new();
    for file in &program.files {
        if file.file_name.ends_with(".d.ts") {
            continue;
        }
        let is_esm = file_is_esm_map
            .get(&file.file_name)
            .copied()
            .unwrap_or(false);
        let helpers = required_helpers(
            file,
            options.checker.target,
            options.es_module_interop,
            is_esm,
            options.checker.experimental_decorators,
        );
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

/// Emit helper diagnostics for each file that needs imported tslib helpers.
///
/// - TS2343 when the helper export does not exist in `tslib`
/// - TS2807 when the helper exists but its declaration is too old
fn emit_tslib_helper_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    tslib_key: &str,
    file_is_esm_map: &rustc_hash::FxHashMap<String, bool>,
) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    let tslib_exports = program.module_exports.get(tslib_key);
    for file in &program.files {
        if file.file_name == tslib_key || file.file_name.ends_with(".d.ts") {
            continue;
        }

        let is_esm = file_is_esm_map
            .get(&file.file_name)
            .copied()
            .unwrap_or(false);
        for helper in required_tslib_helpers(
            file,
            options.checker.target,
            options.es_module_interop,
            is_esm,
            options.checker.experimental_decorators,
        ) {
            let export_sym_id = tslib_exports.and_then(|exports| exports.get(helper.name));
            match export_sym_id {
                Some(sym_id) => {
                    let actual_parameter_count =
                        helper_parameter_count_for_symbol(program, sym_id).unwrap_or(usize::MAX);
                    if let Some(required_parameter_count) = helper.required_parameter_count
                        && actual_parameter_count < required_parameter_count
                    {
                        let message = tsz_common::diagnostics::format_message(
                            tsz_common::diagnostics::diagnostic_messages::THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_WITH_PARAMETERS_WHICH_IS_NOT_COMPA,
                            &[
                                "tslib",
                                helper.name,
                                &required_parameter_count.to_string(),
                            ],
                        );
                        result.push(Diagnostic::error(
                            file.file_name.clone(),
                            helper.start,
                            helper.length,
                            message,
                            tsz_common::diagnostics::diagnostic_codes::THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_WITH_PARAMETERS_WHICH_IS_NOT_COMPA,
                        ));
                    }
                }
                None => {
                    result.push(Diagnostic::error(
                        file.file_name.clone(),
                        helper.start,
                        helper.length,
                        format!(
                            "This syntax requires an imported helper named '{}' which does not exist in 'tslib'. Consider upgrading your version of 'tslib'.",
                            helper.name
                        ),
                        2343,
                    ));
                }
            }
        }
    }
    result
}

fn helper_parameter_count_for_symbol(program: &MergedProgram, sym_id: SymbolId) -> Option<usize> {
    let symbol = program.symbols.get(sym_id)?;
    for &decl_idx in &symbol.declarations {
        if let Some(arenas) = program.declaration_arenas.get(&(sym_id, decl_idx)) {
            for arena in arenas {
                let node = arena.get(decl_idx)?;
                if let Some(func) = arena.get_function(node) {
                    return Some(func.parameters.nodes.len());
                }
            }
        }
        if let Some(arena) = program.symbol_arenas.get(&sym_id) {
            let node = arena.get(decl_idx)?;
            if let Some(func) = arena.get_function(node) {
                return Some(func.parameters.nodes.len());
            }
        }
    }
    None
}

fn emit_tslib_helper_diagnostics_from_counts(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    helper_parameter_counts: &rustc_hash::FxHashMap<String, usize>,
    file_is_esm_map: &rustc_hash::FxHashMap<String, bool>,
) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for file in &program.files {
        if file.file_name.ends_with(".d.ts") {
            continue;
        }

        let is_esm = file_is_esm_map
            .get(&file.file_name)
            .copied()
            .unwrap_or(false);
        for helper in required_tslib_helpers(
            file,
            options.checker.target,
            options.es_module_interop,
            is_esm,
            options.checker.experimental_decorators,
        ) {
            match helper_parameter_counts.get(helper.name) {
                Some(&actual_parameter_count) => {
                    if let Some(required_parameter_count) = helper.required_parameter_count
                        && actual_parameter_count < required_parameter_count
                    {
                        let message = tsz_common::diagnostics::format_message(
                            tsz_common::diagnostics::diagnostic_messages::THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_WITH_PARAMETERS_WHICH_IS_NOT_COMPA,
                            &[
                                "tslib",
                                helper.name,
                                &required_parameter_count.to_string(),
                            ],
                        );
                        result.push(Diagnostic::error(
                            file.file_name.clone(),
                            helper.start,
                            helper.length,
                            message,
                            tsz_common::diagnostics::diagnostic_codes::THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_WITH_PARAMETERS_WHICH_IS_NOT_COMPA,
                        ));
                    }
                }
                None => {
                    result.push(Diagnostic::error(
                        file.file_name.clone(),
                        helper.start,
                        helper.length,
                        format!(
                            "This syntax requires an imported helper named '{}' which does not exist in 'tslib'. Consider upgrading your version of 'tslib'.",
                            helper.name
                        ),
                        2343,
                    ));
                }
            }
        }
    }
    result
}

/// Walk up from `base_dir` looking for `node_modules/tslib`.
fn filesystem_tslib_declaration(base_dir: &Path) -> Option<std::path::PathBuf> {
    let mut dir = base_dir;
    loop {
        let candidate = dir.join("node_modules").join("tslib");
        if candidate.is_dir() {
            let tslib_d_ts = candidate.join("tslib.d.ts");
            if tslib_d_ts.is_file() {
                return Some(tslib_d_ts);
            }
            let index_d_ts = candidate.join("index.d.ts");
            if index_d_ts.is_file() {
                return Some(index_d_ts);
            }
            return None;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return None,
        }
    }
}

fn filesystem_tslib_helper_parameter_counts(
    tslib_path: &Path,
) -> Option<rustc_hash::FxHashMap<String, usize>> {
    let source = std::fs::read_to_string(tslib_path).ok()?;
    source_tslib_helper_parameter_counts(&source)
}

fn source_tslib_helper_parameter_counts(
    source: &str,
) -> Option<rustc_hash::FxHashMap<String, usize>> {
    let mut counts = rustc_hash::FxHashMap::default();
    for helper_name in [
        "__extends",
        "__asyncGenerator",
        "__classPrivateFieldGet",
        "__classPrivateFieldSet",
        "__decorate",
        "__param",
        "__metadata",
        "__importStar",
        "__importDefault",
        "__exportStar",
        "__esDecorate",
        "__runInitializers",
        "__setFunctionName",
        "__propKey",
    ] {
        if let Some(param_count) = extract_declared_function_parameter_count(&source, helper_name) {
            counts.insert(helper_name.to_string(), param_count);
        }
    }
    Some(counts)
}

fn extract_declared_function_parameter_count(source: &str, helper_name: &str) -> Option<usize> {
    let marker = format!("function {helper_name}");
    let marker_idx = source.find(&marker)?;
    let mut idx = marker_idx + marker.len();

    while let Some(ch) = source[idx..].chars().next() {
        if ch.is_whitespace() {
            idx += ch.len_utf8();
            continue;
        }
        break;
    }

    if source[idx..].starts_with('<') {
        let mut depth = 0usize;
        for (rel_idx, ch) in source[idx..].char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        idx += rel_idx + ch.len_utf8();
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    while let Some(ch) = source[idx..].chars().next() {
        if ch.is_whitespace() {
            idx += ch.len_utf8();
            continue;
        }
        break;
    }

    if !source[idx..].starts_with('(') {
        return None;
    }
    idx += 1;
    let params_start = idx;
    let mut depth = 1usize;
    for (rel_idx, ch) in source[idx..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let params = &source[params_start..idx + rel_idx];
                    let trimmed = params.trim();
                    if trimmed.is_empty() {
                        return Some(0);
                    }
                    let mut count = 1usize;
                    let mut angle_depth = 0usize;
                    let mut paren_depth = 0usize;
                    let mut bracket_depth = 0usize;
                    let mut brace_depth = 0usize;
                    for ch in trimmed.chars() {
                        match ch {
                            '<' => angle_depth += 1,
                            '>' => angle_depth = angle_depth.saturating_sub(1),
                            '(' => paren_depth += 1,
                            ')' => paren_depth = paren_depth.saturating_sub(1),
                            '[' => bracket_depth += 1,
                            ']' => bracket_depth = bracket_depth.saturating_sub(1),
                            '{' => brace_depth += 1,
                            '}' => brace_depth = brace_depth.saturating_sub(1),
                            ',' if angle_depth == 0
                                && paren_depth == 0
                                && bracket_depth == 0
                                && brace_depth == 0 =>
                            {
                                count += 1;
                            }
                            _ => {}
                        }
                    }
                    return Some(count);
                }
            }
            _ => {}
        }
    }

    None
}

fn program_appears_filesystem_backed(program: &MergedProgram) -> bool {
    program
        .files
        .iter()
        .any(|file| !file.file_name.ends_with(".d.ts") && Path::new(&file.file_name).exists())
}

pub(super) fn required_helpers(
    file: &BoundFile,
    target: tsz_common::ScriptTarget,
    es_module_interop: bool,
    is_esm: bool,
    experimental_decorators: bool,
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
        return decorator_helpers(file, start, length, experimental_decorators);
    }

    if let Some((start, length)) = first_private_id {
        return vec![("__classPrivateFieldSet", start, length)];
    }

    if let (Some((start, length)), Some(_)) = (saw_await, saw_yield) {
        return vec![("__asyncGenerator", start, length)];
    }

    // Module-transform helpers for import/export syntax that lower to tslib
    // helpers in non-ESM output. ESM files don't need these helpers — ESM
    // syntax is native there.
    if !is_esm {
        let helpers = detect_module_transform_helpers(file, es_module_interop);
        if !helpers.is_empty() {
            return helpers;
        }
    }

    Vec::new()
}

fn required_tslib_helpers(
    file: &BoundFile,
    target: tsz_common::ScriptTarget,
    es_module_interop: bool,
    is_esm: bool,
    experimental_decorators: bool,
) -> Vec<TslibHelperRequirement> {
    let mut saw_await: Option<(u32, u32)> = None;
    let mut saw_yield: Option<(u32, u32)> = None;
    let mut first_decorator: Option<(u32, u32)> = None;
    let mut first_private_id: Option<(u32, u32)> = None;
    let mut first_private_get: Option<(u32, u32)> = None;
    let mut first_private_set: Option<(u32, u32)> = None;

    let needs_extends_helper = !target.supports_es2015();
    let needs_private_lowering = !target.supports_es2022();

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
            return vec![TslibHelperRequirement {
                name: "__extends",
                start: node.pos,
                length: node.end.saturating_sub(node.pos),
                required_parameter_count: None,
            }];
        }

        if needs_private_lowering
            && node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = file.arena.get_access_expr(node)
            && file
                .arena
                .get(access.name_or_argument)
                .is_some_and(|name| name.kind == SyntaxKind::PrivateIdentifier as u16)
        {
            let span = (node.pos, node.end.saturating_sub(node.pos));
            let parent_idx = file
                .arena
                .get_extended(node_idx)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
            let parent_node = if parent_idx != NodeIndex::NONE {
                file.arena.get(parent_idx)
            } else {
                None
            };

            let mut is_plain_assignment_lhs = false;
            let mut is_read_modify_write = false;
            if let Some(parent_node) = parent_node
                && parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = file.arena.get_binary_expr(parent_node)
                && binary.left == node_idx
            {
                is_plain_assignment_lhs = binary.operator_token == SyntaxKind::EqualsToken as u16;
                is_read_modify_write = !is_plain_assignment_lhs;
            }

            if is_read_modify_write {
                first_private_get.get_or_insert(span);
                first_private_set.get_or_insert(span);
            } else if is_plain_assignment_lhs {
                first_private_set.get_or_insert(span);
            } else if parent_node.is_some_and(|parent| {
                parent.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || parent.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
            }) {
                first_private_get.get_or_insert(span);
                first_private_set.get_or_insert(span);
            } else {
                first_private_get.get_or_insert(span);
            }
        }

        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            saw_await = Some((node.pos, node.end.saturating_sub(node.pos)));
        }
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            saw_yield = Some((node.pos, node.end.saturating_sub(node.pos)));
        }
    }

    if let Some((start, length)) = first_decorator {
        return decorator_helpers(file, start, length, experimental_decorators)
            .into_iter()
            .map(|(name, start, length)| TslibHelperRequirement {
                name,
                start,
                length,
                required_parameter_count: None,
            })
            .collect();
    }

    if needs_private_lowering && first_private_id.is_some() {
        let mut helpers = Vec::new();
        if let Some((set_start, set_length)) = first_private_set {
            helpers.push(TslibHelperRequirement {
                name: "__classPrivateFieldSet",
                start: set_start,
                length: set_length,
                required_parameter_count: Some(5),
            });
        }
        if let Some((get_start, get_length)) = first_private_get {
            helpers.push(TslibHelperRequirement {
                name: "__classPrivateFieldGet",
                start: get_start,
                length: get_length,
                required_parameter_count: Some(4),
            });
        }
        if !helpers.is_empty() {
            return helpers;
        }
    }

    if let (Some((start, length)), Some(_)) = (saw_await, saw_yield) {
        return vec![TslibHelperRequirement {
            name: "__asyncGenerator",
            start,
            length,
            required_parameter_count: None,
        }];
    }

    if !is_esm {
        let helpers = detect_module_transform_helpers(file, es_module_interop);
        if !helpers.is_empty() {
            return helpers
                .into_iter()
                .map(|(name, start, length)| TslibHelperRequirement {
                    name,
                    start,
                    length,
                    required_parameter_count: None,
                })
                .collect();
        }
    }

    Vec::new()
}

/// Detect all module-transform helpers needed in a file.
///
/// Patterns:
/// - `import * as X from "m"` (non-type-only, esModuleInterop) → `__importStar`
/// - `import { default as X } from "m"` (non-type-only) → `__importDefault`
/// - `export { default } from "m"` or `export { default as X } from "m"` → `__importDefault`
/// - `export * as ns from "m"` (esModuleInterop) → `__importStar`
/// - `export * from "m"` → `__exportStar`
///
/// Note: `import X from "m"` (bare default import) does NOT require __importDefault in tsc.
fn detect_module_transform_helpers(
    file: &BoundFile,
    es_module_interop: bool,
) -> Vec<(&'static str, u32, u32)> {
    let mut helpers = Vec::new();

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
            if es_module_interop && bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                helpers.push(("__importStar", node.pos, node.end.saturating_sub(node.pos)));
                continue;
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
                    if let Some(prop_node) = file.arena.get(specifier.property_name)
                        && (prop_node.kind == SyntaxKind::DefaultKeyword as u16
                            || file
                                .arena
                                .get_identifier(prop_node)
                                .is_some_and(|id| id.escaped_text == "default"))
                    {
                        helpers.push((
                            "__importDefault",
                            prop_node.pos,
                            prop_node.end.saturating_sub(prop_node.pos),
                        ));
                        break;
                    }
                }
            }
            continue;
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

            // `export * from "m"` — no export_clause → `__exportStar`
            let Some(clause_node) = file.arena.get(export_decl.export_clause) else {
                helpers.push(("__exportStar", node.pos, node.end.saturating_sub(node.pos)));
                continue;
            };

            // `export * as ns from "m"` — the export_clause is a plain identifier (not NAMED_EXPORTS)
            if es_module_interop && clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                helpers.push(("__importStar", node.pos, node.end.saturating_sub(node.pos)));
                continue;
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
                    if check_node.kind == SyntaxKind::DefaultKeyword as u16
                        || file
                            .arena
                            .get_identifier(check_node)
                            .is_some_and(|id| id.escaped_text == "default")
                    {
                        helpers.push((
                            "__importDefault",
                            check_node.pos,
                            check_node.end.saturating_sub(check_node.pos),
                        ));
                        break;
                    }
                }
            }
        }
    }

    helpers
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

fn decorator_helpers(
    file: &BoundFile,
    first_dec_start: u32,
    first_dec_length: u32,
    experimental_decorators: bool,
) -> Vec<(&'static str, u32, u32)> {
    if experimental_decorators {
        return vec![("__decorate", first_dec_start, first_dec_length)];
    }

    es_decorator_helpers(file, first_dec_start, first_dec_length)
}

/// Compute the unified export signature for a file from the merged program.
///
/// This uses the same `ExportSignatureInput` → `ExportSignature` pipeline as the
/// LSP, ensuring both systems produce identical hashes for the same public API
/// surface. The signature is binder-level (names, flags, re-exports, augmentations)
/// and does not include checker-inferred types.
pub(super) fn compute_export_signature(
    program: &MergedProgram,
    file: &BoundFile,
    file_idx: usize,
) -> tsz_lsp::export_signature::ExportSignature {
    let input = build_export_signature_input(program, file, file_idx);
    tsz_lsp::export_signature::ExportSignature::from_input(&input)
}

/// Build an `ExportSignatureInput` from the merged program's per-file data.
///
/// This extracts the same data that the LSP's `ExportSignatureInput::from_binder`
/// extracts from a `BinderState`, but reads from the post-merge program structures.
fn build_export_signature_input(
    program: &MergedProgram,
    file: &BoundFile,
    file_idx: usize,
) -> tsz_lsp::export_signature::ExportSignatureInput {
    let mut input = tsz_lsp::export_signature::ExportSignatureInput::default();
    let file_name = &file.file_name;

    // 1. Direct exports from module_exports
    if let Some(exports) = program.module_exports.get(file_name) {
        let mut entries: Vec<_> = exports.iter().collect();
        entries.sort_by_key(|(name, _)| *name);

        for (name, sym_id) in entries {
            if let Some(symbol) = program.symbols.get(*sym_id) {
                input
                    .exports
                    .push((name.clone(), symbol.flags, symbol.is_type_only));
            }
        }
    }

    // 2. Named re-exports
    if let Some(reexports) = program.reexports.get(file_name) {
        let mut entries: Vec<_> = reexports.iter().collect();
        entries.sort_by_key(|(name, _)| *name);

        for (export_name, (source_module, original_name)) in entries {
            input.named_reexports.push((
                export_name.clone(),
                source_module.clone(),
                original_name.clone(),
            ));
        }
    }

    // 3. Wildcard re-exports (with type_only provenance)
    if let Some(wildcards) = program.wildcard_reexports.get(file_name) {
        let type_only_entries = program.wildcard_reexports_type_only.get(file_name);
        let mut entries: Vec<(String, bool)> = wildcards
            .iter()
            .enumerate()
            .map(|(i, module)| {
                let is_type_only = type_only_entries
                    .and_then(|v| v.get(i))
                    .is_some_and(|(_, to)| *to);
                (module.clone(), is_type_only)
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        input.wildcard_reexports = entries;
    }

    // 4. Global augmentations (per-file)
    {
        let mut names: Vec<&String> = file.global_augmentations.keys().collect();
        names.sort();
        for name in names {
            let count = file
                .global_augmentations
                .get(name.as_str())
                .map_or(0, Vec::len);
            input.global_augmentations.push((name.clone(), count));
        }
    }

    // 5. Module augmentations (per-file)
    {
        let mut modules: Vec<&String> = file.module_augmentations.keys().collect();
        modules.sort();
        for module in modules {
            let mut aug_names: Vec<String> = file
                .module_augmentations
                .get(module.as_str())
                .map(|augs| augs.iter().map(|a| a.name.clone()).collect())
                .unwrap_or_default();
            aug_names.sort();
            input.module_augmentations.push((module.clone(), aug_names));
        }
    }

    // 6. Exported file-local symbols
    if let Some(file_locals) = program.file_locals.get(file_idx) {
        let mut exported_locals: Vec<_> = file_locals
            .iter()
            .filter(|(_, sym_id)| program.symbols.get(**sym_id).is_some_and(|s| s.is_exported))
            .collect();
        exported_locals.sort_by_key(|(name, _)| *name);

        for (name, sym_id) in exported_locals {
            if let Some(symbol) = program.symbols.get(*sym_id) {
                input
                    .exported_locals
                    .push((name.clone(), symbol.flags, symbol.is_type_only));
            }
        }
    }

    input
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

    // In tsc, TS1359 for 'await' as a binding identifier in async/static-block
    // contexts is emitted by the checker via grammarErrorOnNode, NOT by the parser.
    // grammarErrorOnNode checks hasParseDiagnostics(sourceFile) and suppresses the
    // error when any parse diagnostic exists. In tsz, this check lives in the parser
    // (check_illegal_binding_identifier). We replicate tsc's suppression by filtering
    // out TS1359-for-await when ANY other non-grammar parse diagnostic is present.
    let has_non_await1359_parse_error = parse_diagnostics.iter().any(|d| {
        // Exclude the special codes and grammar codes from the trigger check
        !(matches!(d.code, 1009 | 1185 | 1214 | 1262)
            || is_parser_grammar_code(d.code)
            // Also exclude TS1359 for 'await' — those are grammar checks in tsc
            || (d.code == 1359 && d.message.contains("'await'")))
    });
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
            // Suppress TS1359 for 'await' when other parse diagnostics exist.
            // In tsc this is a checker grammar check suppressed by hasParseDiagnostics.
            if diagnostic.code == 1359
                && diagnostic.message.contains("'await'")
                && has_non_await1359_parse_error
            {
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
        | 1212 // Identifier expected. '{0}' is a reserved word in strict mode
        | 1213 // Identifier expected. '{0}' is a reserved word in strict mode. Class definitions are automatically in strict mode.
        | 18037 // 'await' expression cannot be used inside a class static block
        | 18041 // A 'return' statement cannot be used inside a class static block
    )
}

/// Parse-error codes that tsc is known to emit for JavaScript files.
/// tsc's parser is lenient with TypeScript-only syntax in JS files and its
/// checker grammar checks (`grammarErrorOnNode`) are suppressed for TS-only
/// constructs. Only these `TS1xxx` codes are legitimately emitted for JS.
pub(super) const fn is_ts1xxx_allowed_in_js(code: u32) -> bool {
    matches!(
        code,
        1002 // Unterminated string literal
        | 1003 // Identifier expected
        | 1005 // "{0}" expected (missing token)
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
        | 17014 // JSX fragment has no corresponding closing tag
        | 17002 // Expected corresponding JSX closing tag for '{0}'
        | 2657 // JSX expressions must have one parent element
        | 17008 // JSX element '{0}' has no corresponding closing tag
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

/// Pre-computed merged augmentation data shared across all per-file binders.
/// Computing this once avoids `O(N_files²)` iteration in [`create_binder_from_bound_file`].
pub(super) struct MergedAugmentations {
    pub module_augmentations: rustc_hash::FxHashMap<String, Vec<tsz::binder::ModuleAugmentation>>,
    pub augmentation_target_modules: rustc_hash::FxHashMap<tsz::binder::SymbolId, String>,
    pub global_augmentations: rustc_hash::FxHashMap<String, Vec<tsz::binder::GlobalAugmentation>>,
}

impl MergedAugmentations {
    /// Build merged augmentations from all files in the program. Call once per compilation.
    pub fn from_program(program: &MergedProgram) -> Self {
        let mut module_augmentations: rustc_hash::FxHashMap<
            String,
            Vec<tsz::binder::ModuleAugmentation>,
        > = rustc_hash::FxHashMap::default();
        let mut augmentation_target_modules: rustc_hash::FxHashMap<tsz::binder::SymbolId, String> =
            rustc_hash::FxHashMap::default();
        let mut global_augmentations: rustc_hash::FxHashMap<
            String,
            Vec<tsz::binder::GlobalAugmentation>,
        > = rustc_hash::FxHashMap::default();

        for file in &program.files {
            for (spec, augs) in &file.module_augmentations {
                module_augmentations
                    .entry(spec.clone())
                    .or_default()
                    .extend(augs.iter().map(|aug| {
                        tsz::binder::ModuleAugmentation::with_arena(
                            aug.name.clone(),
                            aug.node,
                            Arc::clone(&file.arena),
                        )
                    }));
            }
            for (&sym_id, module_spec) in &file.augmentation_target_modules {
                augmentation_target_modules.insert(sym_id, module_spec.clone());
            }
            for (name, decls) in &file.global_augmentations {
                global_augmentations
                    .entry(name.clone())
                    .or_default()
                    .extend(decls.iter().map(|aug| {
                        tsz::binder::GlobalAugmentation::with_arena(
                            aug.node,
                            Arc::clone(&file.arena),
                        )
                    }));
            }
        }

        Self {
            module_augmentations,
            augmentation_target_modules,
            global_augmentations,
        }
    }
}

pub(super) fn create_binder_from_bound_file(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let augmentations = MergedAugmentations::from_program(program);
    create_binder_from_bound_file_with_augmentations(file, program, file_idx, &augmentations)
}

pub(super) fn create_binder_from_bound_file_with_augmentations(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
    augmentations: &MergedAugmentations,
) -> BinderState {
    let declaration_arenas: tsz::binder::state::DeclarationArenaMap = program
        .declaration_arenas
        .iter()
        .filter_map(|(&(sym_id, decl_idx), arenas)| {
            let has_non_local_arena = arenas.iter().any(|arena| !Arc::ptr_eq(arena, &file.arena));
            has_non_local_arena.then(|| ((sym_id, decl_idx), arenas.clone()))
        })
        .collect();

    let symbols_with_non_local_declarations: rustc_hash::FxHashSet<tsz::binder::SymbolId> =
        declaration_arenas
            .keys()
            .map(|&(sym_id, _)| sym_id)
            .collect();

    let symbol_arenas: rustc_hash::FxHashMap<tsz::binder::SymbolId, Arc<tsz_parser::NodeArena>> =
        program
            .symbol_arenas
            .iter()
            .filter_map(|(&sym_id, arena)| {
                let has_non_local_decl = symbols_with_non_local_declarations.contains(&sym_id);
                (has_non_local_decl || !Arc::ptr_eq(arena, &file.arena))
                    .then(|| (sym_id, Arc::clone(arena)))
            })
            .collect();

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

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: augmentations.global_augmentations.clone(),
            module_augmentations: augmentations.module_augmentations.clone(),
            augmentation_target_modules: augmentations.augmentation_target_modules.clone(),
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            wildcard_reexports_type_only: program.wildcard_reexports_type_only.clone(),
            symbol_arenas,
            declaration_arenas,
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
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    // Compose semantic defs from the merged program, then overlay the file-local
    // entries so reconstructed binders preserve the same stable semantic identity
    // map as the core parallel binder path.
    let mut composed_semantic_defs = program.semantic_defs.clone();
    for (sym_id, entry) in &file.semantic_defs {
        composed_semantic_defs.insert(*sym_id, entry.clone());
    }
    binder.semantic_defs = composed_semantic_defs;
    if let Some(root_scope) = binder.scopes.first() {
        binder.current_scope = root_scope.table.clone();
        binder.current_scope_id = tsz::binder::ScopeId(0);
    }
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
        //
        // Note: TS1014 (A rest parameter must be last) is intentionally excluded.
        // It is a grammar check, not a structural parse failure. The AST for
        // `function f(...x, y)` is valid — both parameters are parsed correctly.
        // tsc still emits TS7019/TS7006 alongside TS1014.
        //
        // Note: TS1047 (A rest parameter cannot be optional) is excluded for the
        // same reason — the parameter is syntactically valid and should be type-checked.
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
        // Note: TS1191 (An import declaration cannot have modifiers) is intentionally
        // excluded. It is a grammar constraint error, not a structural parse failure.
        // The AST is fully valid — the import is parsed correctly. tsc still emits
        // semantic errors like TS2323 alongside TS1191.
        | 1313 // 'else' is not allowed after rest element
        | 1351 // An identifier or keyword cannot immediately follow a numeric literal
        | 1357 // A default clause cannot appear more than once
        | 1378 // Top-level 'for await' loops are only allowed...
        | 1432 // 'await' expressions are only allowed within async functions
        | 1434 // Top-level 'await' expressions are only allowed...
        | 1382 // Unexpected token. Did you mean `{'>'}` or `&gt;`? (JSX)
        | 1438 // Interface must be given a name (recovery creates invalid expression statements)
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

/// Parse error codes that should NOT cause `has_syntax_parse_errors` to suppress
/// semantic diagnostics like TS7006/TS7019 (implicit any).
///
/// These are grammar/constraint errors on otherwise well-formed AST nodes:
/// - TS1009: Trailing comma not allowed
/// - TS1014: A rest parameter must be last in a parameter list
/// - TS1047: A rest parameter cannot be optional
/// - TS1048: A rest parameter cannot have an initializer
/// - TS1185: Merge conflict marker encountered
/// - TS1214: Identifier expected (strict mode reserved word)
/// - TS1262: 'await' at top level
/// - TS1359: 'await' in async context
///
/// tsc emits TS7006/TS7019 even in the presence of these errors because
/// the parameter identity (name) is still valid and can be type-checked.
pub(super) const fn is_non_suppressing_parse_error(code: u32) -> bool {
    matches!(
        code,
        1009  // Trailing comma not allowed
        | 1014 // A rest parameter must be last in a parameter list
        | 1047 // A rest parameter cannot be optional
        | 1048 // A rest parameter cannot have an initializer
        | 1185 // Merge conflict marker
        | 1191 // An import declaration cannot have modifiers (grammar constraint, AST is valid)
        | 1214 // Identifier expected (strict mode reserved word)
        | 1262 // 'await' at top level
        | 1359 // 'await' in async context
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
        required_helpers(&file, target, false, false, false)
            .into_iter()
            .map(|(name, _, _)| name)
            .collect()
    }

    fn merged_program(files: &[(&str, &str)]) -> MergedProgram {
        let bind_results = files
            .iter()
            .map(|(name, source)| {
                parallel::parse_and_bind_single((*name).to_string(), (*source).to_string())
            })
            .collect();
        parallel::merge_bind_results(bind_results)
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
    fn default_reexport_requires_import_default_helper_when_interop_enabled() {
        let file = bound_file("export { default } from \"./a\";");
        let helpers: Vec<_> =
            required_helpers(&file, tsz_common::ScriptTarget::ES2017, true, false, false)
                .into_iter()
                .map(|(name, _, _)| name)
                .collect();
        assert_eq!(helpers, vec!["__importDefault"]);
    }

    #[test]
    fn default_named_import_requires_import_default_helper_without_interop() {
        let file = bound_file("import { default as b } from \"./a\";\nvoid b;");
        let helpers: Vec<_> =
            required_helpers(&file, tsz_common::ScriptTarget::ES2017, false, false, false)
                .into_iter()
                .map(|(name, _, _)| name)
                .collect();
        assert_eq!(helpers, vec!["__importDefault"]);
    }

    #[test]
    fn virtual_program_missing_tslib_reports_ts2354() {
        let program = merged_program(&[
            ("__virtual__/a.ts", "export default class { }"),
            ("__virtual__/b.ts", "export { default } from \"./a\";"),
        ]);
        let mut options = ResolvedCompilerOptions::default();
        options.import_helpers = true;
        options.es_module_interop = true;
        options.checker.target = tsz_common::ScriptTarget::ES2017;

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            Path::new("/__virtual__"),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == 2354
                    && diag.file == "__virtual__/b.ts"
                    && diag.message_text
                        == "This syntax requires an imported helper but module 'tslib' cannot be found."
            }),
            "Expected TS2354 for virtual program without tslib. Got: {diagnostics:#?}"
        );
    }

    #[test]
    fn in_program_tslib_index_helpers_satisfy_legacy_decorator_requirements() {
        let program = merged_program(&[
            (
                "/app/a.ts",
                "declare var dec: any;\n@dec export class A {}\n",
            ),
            (
                "/app/node_modules/tslib/index.d.ts",
                "export declare function __decorate(decorators: Function[], target: any, key?: string | symbol, desc?: any): any;\n",
            ),
        ]);
        let mut options = ResolvedCompilerOptions::default();
        options.import_helpers = true;
        options.checker.target = tsz_common::ScriptTarget::ES2015;
        options.checker.experimental_decorators = true;

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            Path::new("/app"),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2343 || diag.code == 2354),
            "Did not expect tslib helper diagnostics when index.d.ts declares __decorate. Got: {diagnostics:#?}"
        );
    }

    #[test]
    fn old_tslib_private_instance_helpers_report_ts2807_for_get_and_set() {
        let program = merged_program(&[
            (
                "main.ts",
                r#"
export class C {
    #a = 1;
    #b() { this.#c = 42; }
    set #c(v: number) { this.#a += v; }
}
"#,
            ),
            (
                "node_modules/tslib/index.d.ts",
                r#"
export declare function __classPrivateFieldGet<T extends object, V>(receiver: T, state: any): V;
export declare function __classPrivateFieldSet<T extends object, V>(receiver: T, state: any, value: V): V;
"#,
            ),
        ]);
        let mut options = ResolvedCompilerOptions::default();
        options.import_helpers = true;
        options.checker.target = tsz_common::ScriptTarget::ES2015;

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            Path::new("/"),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == 2807
                    && diag.file == "main.ts"
                    && diag.message_text.contains("__classPrivateFieldSet")
                    && diag.message_text.contains("5 parameters")
            }),
            "Expected TS2807 for old __classPrivateFieldSet helper. Got: {diagnostics:#?}"
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == 2807
                    && diag.file == "main.ts"
                    && diag.message_text.contains("__classPrivateFieldGet")
                    && diag.message_text.contains("4 parameters")
            }),
            "Expected TS2807 for old __classPrivateFieldGet helper. Got: {diagnostics:#?}"
        );
    }

    #[test]
    fn old_tslib_private_static_helpers_report_ts2807_for_get_and_set() {
        let program = merged_program(&[
            (
                "main.ts",
                r#"
export class S {
    static #a = 1;
    static #b() { this.#a = 42; }
    static get #c() { return S.#b(); }
}
"#,
            ),
            (
                "node_modules/tslib/index.d.ts",
                r#"
export declare function __classPrivateFieldGet<T extends object, V>(receiver: T, state: any): V;
export declare function __classPrivateFieldSet<T extends object, V>(receiver: T, state: any, value: V): V;
"#,
            ),
        ]);
        let mut options = ResolvedCompilerOptions::default();
        options.import_helpers = true;
        options.checker.target = tsz_common::ScriptTarget::ES2015;

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            Path::new("/"),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == 2807
                    && diag.file == "main.ts"
                    && diag.message_text.contains("__classPrivateFieldSet")
                    && diag.message_text.contains("5 parameters")
            }),
            "Expected TS2807 for old static __classPrivateFieldSet helper. Got: {diagnostics:#?}"
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == 2807
                    && diag.file == "main.ts"
                    && diag.message_text.contains("__classPrivateFieldGet")
                    && diag.message_text.contains("4 parameters")
            }),
            "Expected TS2807 for old static __classPrivateFieldGet helper. Got: {diagnostics:#?}"
        );
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

    #[test]
    fn filtered_parse_diagnostics_suppresses_await_ts1359_when_ts1109_present() {
        use tsz::parser::ParseDiagnostic;

        let diagnostics = vec![
            ParseDiagnostic {
                start: 100,
                length: 5,
                message:
                    "Identifier expected. 'await' is a reserved word that cannot be used here."
                        .to_string(),
                code: 1359,
            },
            ParseDiagnostic {
                start: 200,
                length: 1,
                message: "Expression expected.".to_string(),
                code: 1109,
            },
        ];

        let filtered = filtered_parse_diagnostics(&diagnostics);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&1359),
            "TS1359 for 'await' should be suppressed when TS1109 is present, got: {codes:?}"
        );
        assert!(
            codes.contains(&1109),
            "TS1109 should still be present, got: {codes:?}"
        );
    }

    #[test]
    fn filtered_parse_diagnostics_keeps_await_ts1359_when_alone() {
        use tsz::parser::ParseDiagnostic;

        let diagnostics = vec![ParseDiagnostic {
            start: 100,
            length: 5,
            message: "Identifier expected. 'await' is a reserved word that cannot be used here."
                .to_string(),
            code: 1359,
        }];

        let filtered = filtered_parse_diagnostics(&diagnostics);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1359),
            "TS1359 for 'await' should be kept when it's the only diagnostic, got: {codes:?}"
        );
    }

    #[test]
    fn js_parse_allowlist_keeps_ts2657() {
        assert!(
            is_ts1xxx_allowed_in_js(2657),
            "TS2657 should be preserved for JS JSX recovery diagnostics"
        );
    }

    #[test]
    fn js_parse_allowlist_keeps_ts17002() {
        assert!(
            is_ts1xxx_allowed_in_js(17002),
            "TS17002 should be preserved for JS JSX closing-tag mismatch diagnostics"
        );
    }

    #[test]
    fn js_parse_allowlist_keeps_ts17014() {
        assert!(
            is_ts1xxx_allowed_in_js(17014),
            "TS17014 should be preserved for JS JSX fragment recovery diagnostics"
        );
    }

    // ---------------------------------------------------------------
    // Export signature tests: CLI path via build_export_signature_input
    // ---------------------------------------------------------------

    /// Helper: compute export signature from source via the CLI pipeline
    /// (`parse_and_bind_single` → merge → `build_export_signature_input` → `from_input`).
    fn cli_export_signature(source: &str) -> tsz_lsp::export_signature::ExportSignature {
        let bind_result =
            parallel::parse_and_bind_single("test.ts".to_string(), source.to_string());
        let program = parallel::merge_bind_results(vec![bind_result]);
        let file = &program.files[0];
        compute_export_signature(&program, file, 0)
    }

    /// Helper: compute CLI export signature input (for structural inspection).
    fn cli_export_input(source: &str) -> tsz_lsp::export_signature::ExportSignatureInput {
        let bind_result =
            parallel::parse_and_bind_single("test.ts".to_string(), source.to_string());
        let program = parallel::merge_bind_results(vec![bind_result]);
        let file = &program.files[0];
        build_export_signature_input(&program, file, 0)
    }

    #[test]
    fn body_only_edit_preserves_signature() {
        let before = "export function foo() { return 1; }";
        let after = "export function foo() { return 42; }";
        assert_eq!(
            cli_export_signature(before),
            cli_export_signature(after),
            "body-only edit must not change export signature"
        );
    }

    #[test]
    fn comment_only_edit_preserves_signature() {
        let before = "// original comment\nexport const x = 1;";
        let after = "// modified comment with extra words\nexport const x = 1;";
        assert_eq!(
            cli_export_signature(before),
            cli_export_signature(after),
            "comment-only edit must not change export signature"
        );
    }

    #[test]
    fn private_symbol_edit_preserves_signature() {
        let before = "const priv = 1;\nexport const pub_val = priv;";
        let after = "const priv = 999;\nconst priv2 = 2;\nexport const pub_val = priv;";
        assert_eq!(
            cli_export_signature(before),
            cli_export_signature(after),
            "private symbol additions/edits must not change export signature"
        );
    }

    #[test]
    fn adding_export_changes_signature() {
        let before = "export const x = 1;";
        let after = "export const x = 1;\nexport const y = 2;";
        assert_ne!(
            cli_export_signature(before),
            cli_export_signature(after),
            "adding a new export must change the signature"
        );
    }

    #[test]
    fn removing_export_changes_signature() {
        let before = "export const x = 1;\nexport const y = 2;";
        let after = "export const x = 1;";
        assert_ne!(
            cli_export_signature(before),
            cli_export_signature(after),
            "removing an export must change the signature"
        );
    }

    #[test]
    fn re_export_edit_changes_signature() {
        let before = "export { foo } from './other';";
        let after = "export { foo, bar } from './other';";
        assert_ne!(
            cli_export_signature(before),
            cli_export_signature(after),
            "adding a named re-export must change the signature"
        );
    }

    #[test]
    fn wildcard_re_export_changes_signature() {
        let before = "export const x = 1;";
        let after = "export const x = 1;\nexport * from './other';";
        assert_ne!(
            cli_export_signature(before),
            cli_export_signature(after),
            "adding a wildcard re-export must change the signature"
        );
    }

    #[test]
    fn augmentation_edit_changes_signature() {
        let before = "export const x = 1;";
        let after = "export const x = 1;\ndeclare global { interface Window { foo: string; } }";
        assert_ne!(
            cli_export_signature(before),
            cli_export_signature(after),
            "adding a global augmentation must change the signature"
        );
    }

    #[test]
    fn export_input_captures_exports() {
        let input = cli_export_input("export const x = 1;\nexport function foo() {}");
        let names: Vec<&str> = input.exports.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(names.contains(&"x"), "should contain x export: {names:?}");
        assert!(
            names.contains(&"foo"),
            "should contain foo export: {names:?}"
        );
    }

    #[test]
    fn export_input_captures_re_exports() {
        let input = cli_export_input("export { bar } from './other';");
        let re_names: Vec<&str> = input
            .named_reexports
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect();
        assert!(
            re_names.contains(&"bar"),
            "should contain bar re-export: {re_names:?}"
        );
    }

    #[test]
    fn export_input_captures_wildcard_re_exports() {
        let input = cli_export_input("export * from './other';");
        assert_eq!(
            input.wildcard_reexports.len(),
            1,
            "should have one wildcard re-export"
        );
        assert_eq!(input.wildcard_reexports[0].0, "./other");
    }

    #[test]
    fn export_input_ignores_private_symbols() {
        let input = cli_export_input("const priv = 1;\nexport const pub_val = priv;");
        let names: Vec<&str> = input.exports.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(
            !names.contains(&"priv"),
            "private symbols must not appear in export input"
        );
        assert!(names.contains(&"pub_val"));
    }
}
