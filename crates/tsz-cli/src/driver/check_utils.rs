//! Utility functions for the compilation driver's checking phase:
//! export hash computation, tslib helper detection, binder construction,
//! parse diagnostic conversion, and pragma detection.

use super::*;
use tsz_common::file_extensions::is_ts_declaration_file;

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

    let tslib_file = {
        // Prefer `.d.ts` over `.d.mts`/`.d.cts` re-export stubs.  In nodenext
        // with conditional exports the ESM entry (`.d.mts`) may only contain
        // `export * from "./index.js"` without actual helper declarations.
        let mut candidates: Vec<_> = program
            .files
            .iter()
            .filter(|file| {
                let path = file.file_name.replace('\\', "/");
                path.contains("/tslib/")
                    || Path::new(&file.file_name)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.eq_ignore_ascii_case("tslib.d.ts"))
            })
            .collect();
        candidates.sort_by_key(|f| {
            if f.file_name.ends_with(".d.mts") || f.file_name.ends_with(".d.cts") {
                1
            } else {
                0
            }
        });
        candidates.into_iter().next()
    };

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
        // Check if the tslib module has non-empty ambient exports. If not, fall through
        // to the declaration scan below; raw helper-name mentions in comments or strings
        // must not satisfy tslib helper requirements.
        if program.declared_modules.contains("tslib") {
            let tslib_ambient_has_exports = program
                .module_exports
                .get("tslib")
                .is_some_and(|exports| !exports.is_empty());
            if tslib_ambient_has_exports {
                return emit_tslib_helper_diagnostics(program, options, "tslib", file_is_esm_map);
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

    // Always honor a project-local `node_modules/tslib` directly under the
    // compilation base directory. Conformance tests often materialize tslib in
    // a temp project while also excluding `node_modules` from the synthetic
    // tsconfig and enabling `@noTypesAndSymbols`, so the binder never sees the
    // file even though it intentionally exists for this project.
    if let Some(tslib_path) = local_filesystem_tslib_declaration(base_dir) {
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

    // Check parent directories only when the program appears to be backed by
    // real on-disk files and normal automatic type loading is enabled. Virtual
    // or isolated programs (like conformance harnesses using
    // `@noTypesAndSymbols`) must not inherit tslib availability from the host
    // workspace.
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
        if is_ts_declaration_file(Path::new(&file.file_name)) {
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
        if file.file_name == tslib_key || is_ts_declaration_file(Path::new(&file.file_name)) {
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
        if is_ts_declaration_file(Path::new(&file.file_name)) {
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

fn tslib_declaration_in_dir(dir: &Path) -> Option<std::path::PathBuf> {
    let candidate = dir.join("node_modules").join("tslib");
    if !candidate.is_dir() {
        return None;
    }

    let tslib_d_ts = candidate.join("tslib.d.ts");
    if tslib_d_ts.is_file() {
        return Some(tslib_d_ts);
    }

    let index_d_ts = candidate.join("index.d.ts");
    if index_d_ts.is_file() {
        return Some(index_d_ts);
    }

    None
}

fn local_filesystem_tslib_declaration(base_dir: &Path) -> Option<std::path::PathBuf> {
    tslib_declaration_in_dir(base_dir)
}

/// Walk up from `base_dir` looking for `node_modules/tslib`.
fn filesystem_tslib_declaration(base_dir: &Path) -> Option<std::path::PathBuf> {
    let mut dir = base_dir;
    loop {
        if let Some(tslib_path) = tslib_declaration_in_dir(dir) {
            return Some(tslib_path);
        }
        dir = dir.parent()?;
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
        "__awaiter",
        "__generator",
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
        if let Some(param_count) = extract_declared_function_parameter_count(source, helper_name) {
            counts.insert(helper_name.to_string(), param_count);
        }
    }
    Some(counts)
}

fn extract_declared_function_parameter_count(source: &str, helper_name: &str) -> Option<usize> {
    let marker = format!("function {helper_name}");
    let marker_idx = find_source_marker_outside_trivia(source, &marker)?;
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

fn find_source_marker_outside_trivia(source: &str, marker: &str) -> Option<usize> {
    let mut search_start = 0usize;
    loop {
        let rel_idx = source[search_start..].find(marker)?;
        let marker_idx = search_start + rel_idx;
        if !source_offset_is_in_comment_or_string(source, marker_idx) {
            return Some(marker_idx);
        }
        search_start = marker_idx + marker.len();
    }
}

fn source_offset_is_in_comment_or_string(source: &str, target: usize) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Code,
        LineComment,
        BlockComment,
        SingleQuote,
        DoubleQuote,
        Template,
    }

    let bytes = source.as_bytes();
    let mut idx = 0usize;
    let mut state = State::Code;
    while idx < target && idx < bytes.len() {
        let byte = bytes[idx];
        let next = bytes.get(idx + 1).copied();
        match state {
            State::Code => match (byte, next) {
                (b'/', Some(b'/')) => {
                    state = State::LineComment;
                    idx += 2;
                    continue;
                }
                (b'/', Some(b'*')) => {
                    state = State::BlockComment;
                    idx += 2;
                    continue;
                }
                (b'\'', _) => state = State::SingleQuote,
                (b'"', _) => state = State::DoubleQuote,
                (b'`', _) => state = State::Template,
                _ => {}
            },
            State::LineComment => {
                if byte == b'\n' || byte == b'\r' {
                    state = State::Code;
                }
            }
            State::BlockComment => {
                if byte == b'*' && next == Some(b'/') {
                    state = State::Code;
                    idx += 2;
                    continue;
                }
            }
            State::SingleQuote | State::DoubleQuote | State::Template => {
                if byte == b'\\' {
                    idx += 2;
                    continue;
                }
                let terminator = match state {
                    State::SingleQuote => b'\'',
                    State::DoubleQuote => b'"',
                    State::Template => b'`',
                    _ => unreachable!(),
                };
                if byte == terminator {
                    state = State::Code;
                }
            }
        }
        idx += 1;
    }
    state != State::Code
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
    let mut first_async_function: Option<(u32, u32)> = None;
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
        if first_async_function.is_none()
            && let Some(func) = file.arena.get_function(node)
            && func.is_async
            && !func.asterisk_token
        {
            if let Some(name_node) = file.arena.get(func.name) {
                first_async_function =
                    Some((name_node.pos, name_node.end.saturating_sub(name_node.pos)));
            } else {
                first_async_function = Some((node.pos, node.end.saturating_sub(node.pos)));
            }
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
    if let Some((start, length)) = first_async_function.or(saw_await) {
        return vec![("__awaiter", start, length)];
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
    let mut first_async_function: Option<(u32, u32)> = None;
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
        if first_async_function.is_none()
            && let Some(func) = file.arena.get_function(node)
            && func.is_async
            && !func.asterisk_token
        {
            if let Some(name_node) = file.arena.get(func.name) {
                first_async_function =
                    Some((name_node.pos, name_node.end.saturating_sub(name_node.pos)));
            } else {
                first_async_function = Some((node.pos, node.end.saturating_sub(node.pos)));
            }
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
    if let Some((start, length)) = first_async_function.or(saw_await) {
        let mut helpers = vec![TslibHelperRequirement {
            name: "__awaiter",
            start,
            length,
            required_parameter_count: None,
        }];
        if !target.supports_es2015() {
            helpers.push(TslibHelperRequirement {
                name: "__generator",
                start,
                length,
                required_parameter_count: None,
            });
        }
        return helpers;
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
    let text: &str = source.text.as_ref();
    // When both directives are present in leading trivia, the last one wins.
    let ts_check_pos =
        tsz_common::comments::last_ts_directive_offset_in_leading_trivia(text, "@ts-check");
    let ts_nocheck_pos =
        tsz_common::comments::last_ts_directive_offset_in_leading_trivia(text, "@ts-nocheck");
    match (ts_check_pos, ts_nocheck_pos) {
        (Some(check), Some(nocheck)) => check > nocheck,
        (Some(_), None) => true,
        _ => false,
    }
}

pub(super) fn js_file_has_ts_nocheck_pragma(file: &BoundFile) -> bool {
    let Some(source) = file.arena.get_source_file_at(file.source_file) else {
        return false;
    };
    let text: &str = source.text.as_ref();
    tsz_common::comments::source_has_ts_nocheck_directive(text)
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

pub(super) fn collect_no_check_parse_diagnostics_for_file(
    file_name: &str,
    arena: &NodeArena,
    source_file: NodeIndex,
    parse_diagnostics: &[ParseDiagnostic],
    options: &ResolvedCompilerOptions,
    program_has_real_syntax_errors: bool,
) -> Vec<Diagnostic> {
    let filtered_parse_diagnostics =
        filtered_parse_diagnostics(parse_diagnostics, program_has_real_syntax_errors);
    let is_js = is_js_file(Path::new(file_name));

    let mut file_diagnostics: Vec<Diagnostic> = if is_js {
        let source_text = arena
            .get_source_file_at(source_file)
            .map(|sf| sf.text.as_ref());
        let mut diags = Vec::new();
        convert_js_parse_diagnostics_to_ts8xxx(
            parse_diagnostics,
            file_name,
            &mut diags,
            source_text,
        );
        for parse_diagnostic in &filtered_parse_diagnostics {
            if is_ts1xxx_allowed_in_js(parse_diagnostic.code) {
                diags.push(parse_diagnostic_to_checker(file_name, parse_diagnostic));
            }
        }
        // tsc reports the JS-only TS8xxx grammar diagnostics from its parser,
        // so they must surface even in `--noCheck` mode where tsz otherwise
        // skips the regular checker pass (#3692). Run a minimal binder + checker
        // grammar-only walk for each JS source so type annotations, modifiers,
        // and other TypeScript-only constructs still produce TS8xxx errors.
        diags.extend(collect_js_grammar_diagnostics(
            file_name,
            arena,
            source_file,
            options,
        ));
        diags
    } else {
        filtered_parse_diagnostics
            .into_iter()
            .map(|d| parse_diagnostic_to_checker(file_name, d))
            .collect()
    };

    if is_js {
        file_diagnostics.retain(|d| !is_checker_grammar_code_suppressed_in_js(d.code));
    }

    file_diagnostics
}

/// Run the checker's JS grammar pass on a parsed JS source file. The pass
/// surfaces the `TS8xxx` diagnostics tsc emits for TypeScript-only constructs in
/// JS files. Used by the `--noCheck` parse-only path to align with tsc, which
/// reports these from its parser regardless of `--noCheck`.
fn collect_js_grammar_diagnostics(
    file_name: &str,
    arena: &NodeArena,
    source_file: NodeIndex,
    options: &ResolvedCompilerOptions,
) -> Vec<Diagnostic> {
    let mut binder = tsz_binder::state::BinderState::new();
    binder.bind_source_file(arena, source_file);
    tsz_checker::run_js_grammar_pass(
        arena,
        &binder,
        source_file,
        file_name.to_string(),
        options.checker.clone(),
    )
}

pub(super) fn filtered_parse_diagnostics(
    parse_diagnostics: &[ParseDiagnostic],
    program_has_real_syntax_errors: bool,
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

    // TS1359 for `await` is parser-emitted in tsz. Keep it alongside unrelated
    // parse diagnostics (tsc does this in plain JS binder errors), but suppress
    // it for expression-recovery cases where TS1109 is the primary diagnostic.
    let has_expression_expected_parse_error = parse_diagnostics.iter().any(|d| d.code == 1109);
    let has_hard_keyword_interface_ts2427 = parse_diagnostics
        .iter()
        .any(is_hard_keyword_interface_name_2427_parse_diagnostic);
    parse_diagnostics
        .iter()
        .filter(|diagnostic| {
            // Existing: suppress TS1184 when real syntax errors exist
            if has_real_syntax_error && diagnostic.code == 1184 {
                return false;
            }
            // Suppress parser-emitted grammar codes that tsc would emit via
            // grammarErrorOnNode (checker-side, suppressed by hasParseDiagnostics).
            // This applies both per-file (when the current file has non-grammar errors)
            // and program-wide (when any file in the program has real syntax errors).
            // tsc's grammarErrorOnNode calls hasParseDiagnostics(sourceFile) which
            // covers program-level parse errors; we mirror that behavior here.
            if (has_non_grammar_parse_error || program_has_real_syntax_errors)
                && is_parser_grammar_code(diagnostic.code)
            {
                return false;
            }
            // Suppress TS1359 for 'await' when expression recovery already
            // reported TS1109 at the construct.
            if diagnostic.code == 1359
                && diagnostic.message.contains("'await'")
                && has_expression_expected_parse_error
            {
                return false;
            }
            if has_hard_keyword_interface_ts2427
                && diagnostic.code == 2427
                && !is_hard_keyword_interface_name_2427_parse_diagnostic(diagnostic)
            {
                return false;
            }
            true
        })
        .collect()
}

fn is_hard_keyword_interface_name_2427_parse_diagnostic(diagnostic: &ParseDiagnostic) -> bool {
    diagnostic.code == 2427
        && (diagnostic.message == "Interface name cannot be 'void'."
            || diagnostic.message == "Interface name cannot be 'null'.")
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
        | 1029 // '{0}' modifier must precede '{1}' modifier
        | 1030 // '{0}' modifier already seen
        | 1031 // '{0}' modifier cannot appear on class elements of this kind
        | 1040 // '{0}' modifier cannot be used in an ambient context
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
        | 1243 // '{0}' modifier cannot be used with '{1}' modifier
        | 1275 // 'accessor' modifier can only appear on a property declaration
        | 1276 // An 'accessor' property cannot be declared optional
        | 8038 // Decorators may not appear after 'export' or 'export default' if they also appear before 'export'
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
        | 1141 // String literal expected
        | 1163 // A 'yield' expression is only allowed in a generator body
        // Note: TS1192 ("Module has no default export") is intentionally
        // excluded — it is a semantic checker diagnostic that tsc routes
        // through getSemanticDiagnostics, so unchecked JS files never see
        // it (issue #3693).
        | 1196 // Catch clause variable type annotation
        | 1206 // Decorators are not valid here
        | 8038 // Decorators may not appear after 'export' if they also appear before 'export'
        | 1210 // Code contained in a class is evaluated in strict mode
        | 1214 // Identifier expected; 'yield' is reserved in module strict mode
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
        | 18030 // An optional chain cannot contain private identifiers
        | 18012 // '#constructor' is a reserved word
    )
}

/// Checker-emitted grammar codes outside the `TS1xxx` range that should be
/// suppressed for JS files. tsc doesn't emit these for JavaScript because
/// its parser handles the constructs leniently.
pub(super) const fn is_checker_grammar_code_suppressed_in_js(code: u32) -> bool {
    matches!(
        code, 17012 // '{0}' is not a valid meta-property for keyword '{1}'
    )
}

/// JS-only-syntactic diagnostic codes — those `TS8xxx` codes that tsc emits
/// from `getJSSyntacticDiagnosticsForFile` (see `program.ts`) for TypeScript
/// syntax appearing inside JavaScript source files. tsc routes these through
/// `getSyntacticDiagnostics` and uses them to short-circuit
/// `getSemanticDiagnostics` across the whole program in
/// `emitFilesAndReportErrors`.
///
/// This list is a stricter subset of `is_js_grammar_diagnostic`. JSDoc-related
/// `TS8xxx` codes (`TS8020`–`TS8039` save for `TS8038`) come from the checker
/// and do **not** participate in the syntactic-skip-semantic gate.
pub(super) const fn is_js_only_syntactic_diagnostic(code: u32) -> bool {
    matches!(
        code,
        8002  // 'import ... =' can only be used in TypeScript files
        | 8003  // 'export =' can only be used in TypeScript files
        | 8004  // Type parameter declarations
        | 8005  // 'implements' clauses
        | 8006  // '{0}' declarations (interface, namespace, enum, import/export type)
        | 8008  // Type aliases
        | 8009  // The '{0}' modifier
        | 8010  // Type annotations
        | 8011  // Type arguments
        | 8012  // Parameter modifiers
        | 8013  // Non-null assertions
        | 8016  // Type assertion expressions
        | 8017  // Signature declarations
        | 8037  // Type satisfaction expressions
        | 8038 // Decorators may not appear after 'export'
    )
}

/// True when a diagnostic should be retained even though the program contains
/// a JS-only-syntactic diagnostic.
///
/// In tsc, `getSyntacticDiagnostics` (which contains the JS-only-syntactic
/// codes for JS files) short-circuits `getSemanticDiagnostics` program-wide
/// in `emitFilesAndReportErrors`. The only diagnostics that survive are the
/// ones tsc routes through `getSyntacticDiagnostics` itself: structural parse
/// failures, plus the codes contributed by `getJSSyntacticDiagnosticsForFile`.
///
/// tsz's emission map straddles parser and checker — many `TS1xxx` codes that
/// `is_ts1xxx_allowed_in_js` legitimately accepts in JS files are nonetheless
/// emitted from the *checker*'s grammar phase, so tsc would route them through
/// `getSemanticDiagnostics` and drop them here. We honour that by keeping the
/// broad `TS1xxx` allow-list and then explicitly excluding the checker/binder
/// grammar checks tsc treats as semantic — break/continue (`TS1104`/`TS1105`)
/// and the cross-function jump-target check (`TS1107`).
pub(super) const fn keep_diagnostic_when_js_only_syntactic_skips_semantic(code: u32) -> bool {
    if is_post_js_gate_suppressed_checker_grammar(code) {
        return false;
    }
    is_real_syntax_error(code)
        || is_ts1xxx_allowed_in_js(code)
        || (code >= 8000 && code < 9000)
        || matches!(code, 2427 | 2457)
}

/// Checker/binder grammar codes that tsc routes through `getSemanticDiagnostics`
/// rather than `getSyntacticDiagnostics` — so when the JS-only-syntactic gate
/// fires, tsc drops them program-wide. These codes appear in
/// `is_ts1xxx_allowed_in_js` because tsc legitimately emits them for plain JS
/// files when no syntactic gate-trigger is present, but once a gate-trigger
/// fires they must be suppressed even though they are `TS1xxx`.
const fn is_post_js_gate_suppressed_checker_grammar(code: u32) -> bool {
    matches!(
        code,
        // The break/continue family — tsc's `checkBreakOrContinueStatement`
        // emits these from the type checker.
        1104 // A 'continue' statement can only be used within an enclosing iteration statement.
        | 1105 // A 'break' statement can only be used within an enclosing iteration or switch statement.
        | 1107 // Jump target cannot cross function boundary.
    )
}

/// Pre-computed merged augmentation data shared across all per-file binders.
/// Computing this once avoids `O(N_files²)` iteration in [`create_binder_from_bound_file`].
pub(super) struct MergedAugmentations {
    /// Cross-file merged module augmentations.
    ///
    /// Wrapped in `Arc` so per-file binders can share the merged map via
    /// `Arc::clone` instead of deep-cloning the entire map into each binder.
    pub module_augmentations:
        std::sync::Arc<rustc_hash::FxHashMap<String, Vec<tsz::binder::ModuleAugmentation>>>,
    /// Cross-file merged augmentation target modules.
    ///
    /// Wrapped in `Arc` so per-file binders can share the merged map via
    /// `Arc::clone` instead of deep-cloning the entire map into each binder.
    pub augmentation_target_modules:
        std::sync::Arc<rustc_hash::FxHashMap<tsz::binder::SymbolId, String>>,
    /// Cross-file merged global augmentations.
    ///
    /// Wrapped in `Arc` so per-file binders can share the merged map via
    /// `Arc::clone` instead of deep-cloning the entire map into each binder.
    pub global_augmentations:
        std::sync::Arc<rustc_hash::FxHashMap<String, Vec<tsz::binder::GlobalAugmentation>>>,
}

impl MergedAugmentations {
    /// Build merged augmentations from all files in the program. Call once per compilation.
    pub fn from_program(program: &MergedProgram) -> Self {
        let module_augmentation_keys = program
            .files
            .iter()
            .map(|file| file.module_augmentations.len())
            .sum();
        let augmentation_target_count = program
            .files
            .iter()
            .map(|file| file.augmentation_target_modules.len())
            .sum();
        let global_augmentation_keys = program
            .files
            .iter()
            .map(|file| file.global_augmentations.len())
            .sum();

        let mut module_augmentations: rustc_hash::FxHashMap<
            String,
            Vec<tsz::binder::ModuleAugmentation>,
        > = rustc_hash::FxHashMap::with_capacity_and_hasher(
            module_augmentation_keys,
            Default::default(),
        );
        let mut augmentation_target_modules: rustc_hash::FxHashMap<tsz::binder::SymbolId, String> =
            rustc_hash::FxHashMap::with_capacity_and_hasher(
                augmentation_target_count,
                Default::default(),
            );
        let mut global_augmentations: rustc_hash::FxHashMap<
            String,
            Vec<tsz::binder::GlobalAugmentation>,
        > = rustc_hash::FxHashMap::with_capacity_and_hasher(
            global_augmentation_keys,
            Default::default(),
        );

        for file in &program.files {
            for (spec, augs) in file.module_augmentations.iter() {
                module_augmentations
                    .entry(spec.clone())
                    .or_insert_with(|| Vec::with_capacity(augs.len()))
                    .extend(augs.iter().map(|aug| {
                        tsz::binder::ModuleAugmentation::with_arena(
                            aug.name.clone(),
                            aug.node,
                            Arc::clone(&file.arena),
                        )
                    }));
            }
            for (&sym_id, module_spec) in file.augmentation_target_modules.iter() {
                augmentation_target_modules.insert(sym_id, module_spec.clone());
            }
            for (name, decls) in file.global_augmentations.iter() {
                global_augmentations
                    .entry(name.clone())
                    .or_insert_with(|| Vec::with_capacity(decls.len()))
                    .extend(decls.iter().map(|aug| {
                        tsz::binder::GlobalAugmentation::with_arena(
                            aug.node,
                            Arc::clone(&file.arena),
                            aug.flags,
                        )
                    }));
            }
        }

        Self {
            module_augmentations: std::sync::Arc::new(module_augmentations),
            augmentation_target_modules: std::sync::Arc::new(augmentation_target_modules),
            global_augmentations: std::sync::Arc::new(global_augmentations),
        }
    }
}

#[allow(dead_code)]
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
    // Share the program-wide `declaration_arenas` map via `Arc::clone` — O(1)
    // instead of iterating the entire program-wide map per file and cloning
    // matching entries. The previous filter kept ~99% of entries on large
    // projects, so the per-file filtering was almost entirely wasted work:
    // on a 6086-file project with ~100K declarations this was ~600M entry
    // visits × a `SmallVec<[Arc<NodeArena>; 1]>` clone each.
    //
    // Consumers doing point lookups (~30 call sites) see the same data via
    // `binder.declaration_arenas.get(&(sym_id, decl_idx))`. The three iter
    // consumers that needed to enumerate every `NodeIndex` for a given
    // `SymbolId` were rewritten to use the `sym_to_decl_indices` secondary
    // index (point lookup) instead of a full `declaration_arenas` scan.
    let declaration_arenas = Arc::clone(&program.declaration_arenas);
    let sym_to_decl_indices = Arc::clone(&program.sym_to_decl_indices);

    // Share the program-wide symbol_arenas via Arc::clone — O(1) instead of
    // building a per-file filtered map. The previous filter dropped entries
    // where the symbol was already local (arena pointer equal to file.arena
    // and no cross-file decl), but keeping them is harmless: consumers do
    // point lookups (`binder.symbol_arenas.get(&sym_id)`), and the checker
    // has no iter consumers of this map. Drops ~O(N_files × N_symbols)
    // iteration on large repos.
    let symbol_arenas = Arc::clone(&program.symbol_arenas);

    // Merge per-file locals with program globals via the shared helper,
    // which short-circuits to an O(1) `Arc::clone` when one side is empty
    // (common for trivial declaration files with no top-level locals).
    // The slow path pre-sizes to (locals + globals) to avoid rehashing
    // during inserts.
    let file_locals = program.build_merged_file_locals(file_idx);

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        // Arc::clone is O(1) (atomic refcount bump) instead of deep-cloning the
        // underlying `FxHashMap<u32, SymbolId>`. Per-file binders consume this
        // map read-only after construction (binder mutations during checking
        // are gated by `Arc::make_mut`, which copy-on-writes safely if a
        // mutation ever does fire); sharing is safe.
        Arc::clone(&file.node_symbols),
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
            sym_to_decl_indices,
            // Per-binder cross_file_node_symbols left empty intentionally.
            // The program-wide outer map is stored once on ProgramContext and
            // read via `ctx.cross_file_node_symbols_for_arena`. Cloning
            // it into every per-file binder scales outer-map allocation
            // with N² — several hundred MB on large-ts-repo.
            cross_file_node_symbols: Default::default(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            // Per-binder `flow_nodes` is an Arc clone (atomic increment)
            // instead of a deep clone of the underlying `Vec<FlowNode>`.
            // Each `FlowNode` owns a `Vec<FlowNodeId>` antecedents, so
            // the previous deep clone was allocation-heavy; on large
            // repos it was paid ~2× per file (cross-file lookup +
            // per-file checking binder).
            flow_nodes: Arc::clone(&file.flow_nodes),
            // Arc::clone is O(1); per-file binders share the same `node_flow`
            // map as the `BoundFile` instead of deep-cloning the underlying
            // `FxHashMap<u32, FlowNodeId>`. Per-file binders consume this map
            // read-only after construction (binder mutations during checking
            // are gated by `Arc::make_mut`, which copy-on-writes safely if a
            // mutation ever does fire); sharing is safe.
            node_flow: Arc::clone(&file.node_flow),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            // Per-binder alias_partners left empty: every checker consumer
            // routes through `ctx.alias_partner_for` /
            // `alias_partners_contains`, which prefers the project-wide
            // `program_alias_partners` Arc installed by ProgramContext::apply_to.
            alias_partners: Default::default(),
        },
    );

    // Per-binder declared_modules left empty: every checker consumer
    // routes through `ctx.declared_modules_contains`, which prefers the
    // project-wide `global_declared_modules` index built from the skeleton.
    binder.declared_modules = Default::default();
    // Restore is_external_module from BoundFile to preserve per-file state
    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    // Only the file-local semantic_defs are stored on the reconstructed
    // binder. The cross-file / program-wide entries live in the shared
    // `DefinitionStore` installed by `ProgramContext::apply_to`, which gates
    // every consumer of `binder.semantic_defs` (`pre_populate_def_ids_*`,
    // `resolve_cross_batch_heritage`) behind
    // `!ctx.definition_store.is_fully_populated()`. In the parallel CLI
    // path the shared store IS fully populated, so those consumers never
    // read the binder's map — copying `program.semantic_defs` into each
    // per-file binder was pure O(N · program_defs) waste (6%+ of total
    // CPU on ts-toolbelt subsets, all of it in `SemanticDefEntry::drop`).
    // Arc::clone is O(1) (atomic refcount bump) instead of deep-cloning the
    // underlying `FxHashMap<SymbolId, SemanticDefEntry>`. The previous deep
    // clone was the largest single source of memory pressure on multi-file
    // builds (e.g., 50-70 GB total virtual on the 6086-file large-ts-repo
    // benchmark, multiplied across rayon worker threads). Cross-file lookup
    // binders only read this map (post-construction), so sharing is safe.
    binder.semantic_defs = Arc::clone(&file.semantic_defs);
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
pub(super) fn create_cross_file_lookup_binder_with_augmentations(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
    augmentations: &MergedAugmentations,
) -> BinderState {
    // Cross-file lookup binders never merge program-wide globals into their
    // `file_locals`; consumers (e.g. `resolve_in_all_binders`) only walk the
    // per-file local entries. Since #1535 made `SymbolTable` internally
    // `Arc<FxHashMap<String, SymbolId>>`, plain `.clone()` is an O(1)
    // atomic refcount bump — no fresh map allocation, no per-entry
    // `String` clones. The previous manual rebuild paid `local_count` per
    // file, multiplied by the rayon-parallel per-file binder build.
    let file_locals = program
        .file_locals
        .get(file_idx)
        .cloned()
        .unwrap_or_default();

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        // Arc::clone is O(1) (atomic refcount bump) instead of deep-cloning the
        // underlying `FxHashMap<u32, SymbolId>`. Per-file binders consume this
        // map read-only after construction (binder mutations during checking
        // are gated by `Arc::make_mut`, which copy-on-writes safely if a
        // mutation ever does fire); sharing is safe.
        Arc::clone(&file.node_symbols),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: augmentations.global_augmentations.clone(),
            module_augmentations: augmentations.module_augmentations.clone(),
            augmentation_target_modules: augmentations.augmentation_target_modules.clone(),
            // Per-binder `module_exports` is left empty intentionally.
            // The program-wide merged `module_exports` lives once on
            // `ProgramContext` as `program_module_exports` and is read via
            // `ctx.module_exports_for_module`. Cross-file lookup binders
            // used to deep-clone the entire merged map (thousands of
            // entries on large repos) into every one of N per-file
            // binders.
            module_exports: Default::default(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            // Per-binder re-export maps left empty intentionally. The
            // program-wide merged re-export maps are stored once on
            // `ProgramContext` and read via `ctx.reexports_for_file` /
            // `wildcard_reexports_for_file`. Cloning them into every one
            // of N cross-file lookup binders scales the per-file setup
            // cost with total re-exports across the whole project —
            // several GB on the large-ts-repo benchmark fixture.
            reexports: Default::default(),
            wildcard_reexports: Default::default(),
            wildcard_reexports_type_only: Default::default(),
            // Cross-file lookup binders only need local scopes/symbol ownership plus the
            // merged export/augmentation tables. Cloning the full cross-program arena maps
            // into every file binder makes all_binders setup scale with total declarations.
            symbol_arenas: Default::default(),
            declaration_arenas: Default::default(),
            sym_to_decl_indices: Default::default(),
            // See `create_binder_from_bound_file_with_augmentations` for
            // the rationale: the program-wide map lives on ProgramContext.
            cross_file_node_symbols: Default::default(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            // Per-binder `flow_nodes` is an Arc clone; see
            // `create_binder_from_bound_file_with_augmentations` for
            // the rationale.
            flow_nodes: Arc::clone(&file.flow_nodes),
            // Arc::clone is O(1); cross-file lookup binders share the per-file
            // `node_flow` map instead of deep-cloning the underlying
            // `FxHashMap<u32, FlowNodeId>`. Per-file binders consume this map
            // read-only after construction (binder mutations during checking
            // are gated by `Arc::make_mut`, which copy-on-writes safely if a
            // mutation ever does fire); sharing is safe.
            node_flow: Arc::clone(&file.node_flow),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            // See `create_binder_from_bound_file_with_augmentations`:
            // consumers go through the project-wide accessor.
            alias_partners: Default::default(),
        },
    );

    // See `create_binder_from_bound_file_with_augmentations` for rationale.
    binder.declared_modules = Default::default();
    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    // See `create_binder_from_bound_file_with_augmentations` for the
    // rationale: the cross-file semantic_defs live in the shared
    // `DefinitionStore`, not here.
    // Arc::clone is O(1) (atomic refcount bump) instead of deep-cloning the
    // underlying `FxHashMap<SymbolId, SemanticDefEntry>`. The previous deep
    // clone was the largest single source of memory pressure on multi-file
    // builds (e.g., 50-70 GB total virtual on the 6086-file large-ts-repo
    // benchmark, multiplied across rayon worker threads). Cross-file lookup
    // binders only read this map (post-construction), so sharing is safe.
    binder.semantic_defs = Arc::clone(&file.semantic_defs);
    if let Some(root_scope) = binder.scopes.first() {
        binder.current_scope = root_scope.table.clone();
        binder.current_scope_id = tsz::binder::ScopeId(0);
    }
    binder.set_lib_symbols_merged(true);
    binder.lib_binders = program.lib_binders.clone();
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();

    binder
}

// --- TS directive suppression ---
/// Length in bytes of a line break starting at `bytes[i]`, or `0` if there is
/// no line break at that position. Recognizes `\n`, `\r`, `\r\n`, and the
/// UTF-8 encodings of U+2028 (LINE SEPARATOR) and U+2029 (PARAGRAPH
/// SEPARATOR), matching `tsz-scanner::is_line_break` and tsc's own line
/// break recognition.
fn line_break_len_at(bytes: &[u8], i: usize) -> usize {
    match bytes.get(i) {
        Some(&b'\n') => 1,
        Some(&b'\r') => {
            if bytes.get(i + 1) == Some(&b'\n') {
                2
            } else {
                1
            }
        }
        Some(&0xE2)
            if bytes.get(i + 1) == Some(&0x80)
                && matches!(bytes.get(i + 2), Some(&0xA8) | Some(&0xA9)) =>
        {
            3
        }
        _ => 0,
    }
}

/// Build a line-start table: `line_starts[i]` is the byte offset of the first char on line `i`.
fn build_line_starts(text: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        let lb = line_break_len_at(bytes, i);
        if lb > 0 {
            starts.push((i + lb) as u32);
            i += lb;
        } else {
            i += 1;
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
    matches!(
        b,
        b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C | b':' | b'*' | b'/'
    )
}

const fn is_directive_leading_trivia_byte(b: u8) -> bool {
    matches!(b, b'/' | b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C | b'*')
}

/// Check if a comment text contains `@ts-expect-error` or `@ts-ignore`.
/// Returns the directive kind and the byte offset of the directive marker
/// within the comment text.
fn find_directive_in_text(comment: &str) -> Option<(DirectiveKind, u32)> {
    let bytes = comment.as_bytes();
    let mut pos = if comment.starts_with("//") || comment.starts_with("/*") {
        2
    } else {
        0
    };

    while pos < bytes.len() && is_directive_leading_trivia_byte(bytes[pos]) {
        pos += 1;
    }

    for (kind, text) in [
        (DirectiveKind::ExpectError, "@ts-expect-error"),
        (DirectiveKind::Ignore, "@ts-ignore"),
    ] {
        if !comment[pos..].starts_with(text) {
            continue;
        }
        let after = pos + text.len();
        if after >= comment.len() || is_directive_separator(comment.as_bytes()[after]) {
            return Some((kind, pos as u32));
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
    /// Byte offset where an unused `@ts-expect-error` diagnostic should start.
    unused_diagnostic_start: u32,
    /// Byte length for an unused `@ts-expect-error` diagnostic.
    unused_diagnostic_length: u32,
}

/// Scan source text for `@ts-expect-error` and `@ts-ignore` directives in comments.
fn find_ts_directives(text: &str) -> Vec<TsDirective> {
    let line_starts = build_line_starts(text);
    let mut directives = Vec::new();

    for comment in tsz_common::comments::get_comment_ranges(text) {
        let comment_text = comment.get_text(text);
        let Some((kind, directive_offset)) = find_directive_in_text(comment_text) else {
            continue;
        };

        let suppressed_line = if comment.is_multi_line {
            let close_line = line_of_offset(&line_starts, comment.end.saturating_sub(1));
            close_line + 1
        } else {
            let comment_line = line_of_offset(&line_starts, comment.pos);
            comment_line + 1
        };
        let directive_start = comment.pos.saturating_add(directive_offset);
        let directive_line = line_of_offset(&line_starts, directive_start) as usize;
        let directive_line_start = line_starts
            .get(directive_line)
            .copied()
            .unwrap_or(comment.pos);
        let unused_diagnostic_start = if comment.is_multi_line && directive_line_start > comment.pos
        {
            directive_line_start
        } else {
            comment.pos
        };

        directives.push(TsDirective {
            is_expect_error: kind == DirectiveKind::ExpectError,
            suppressed_line,
            unused_diagnostic_start,
            unused_diagnostic_length: comment.end.saturating_sub(unused_diagnostic_start),
        });
    }

    directives
}

/// Apply `@ts-expect-error` and `@ts-ignore` directive suppression to diagnostics.
///
/// 1. Finds all directive comments in the source text
/// 2. Suppresses diagnostics on the line following each directive
/// 3. Emits TS2578 for unused `@ts-expect-error` directives
#[cfg(test)]
pub(super) fn apply_ts_directive_suppression(
    file_name: &str,
    source_text: &str,
    diagnostics: &mut Vec<Diagnostic>,
    preserve_declaration_jsdoc_name_diagnostics: bool,
) {
    apply_ts_directive_suppression_with_unused_reporting(
        file_name,
        source_text,
        diagnostics,
        true,
        preserve_declaration_jsdoc_name_diagnostics,
    );
}

pub(super) fn apply_ts_directive_suppression_with_unused_reporting(
    file_name: &str,
    source_text: &str,
    diagnostics: &mut Vec<Diagnostic>,
    report_unused_expect_error: bool,
    preserve_declaration_jsdoc_name_diagnostics: bool,
) {
    let directives = find_ts_directives(source_text);
    if directives.is_empty() {
        return;
    }

    let line_starts = build_line_starts(source_text);

    // Check for @ts-nocheck — suppresses TS2578 for unused directives.
    let has_ts_nocheck =
        tsz_common::comments::has_ts_directive_in_leading_trivia(source_text, "@ts-nocheck");

    let mut directive_used = vec![false; directives.len()];

    // Suppress diagnostics on directive-targeted lines.
    //
    // tsc applies `@ts-ignore` and `@ts-expect-error` uniformly across the
    // checking pipeline, including the JSDoc `@type` lookup that runs during
    // checked-JS declaration emit. An earlier carve-out kept TS2304/TS2552
    // alive on lines containing `@type {` to align a different fingerprint,
    // but issue #3996 confirmed tsc actually suppresses those diagnostics.
    // The `preserve_declaration_jsdoc_name_diagnostics` flag is now unused
    // here; callers still pass it so the public signature stays stable while
    // any deeper revisit of declaration-emit fingerprints lands.
    let _ = preserve_declaration_jsdoc_name_diagnostics;
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

    // Emit TS2578 for unused @ts-expect-error directives.
    //
    // tsc anchors this diagnostic at the directive comment text, not at the
    // enclosing line start. Same-line directives start at the `//` or `/*`
    // opener, while directives inside multiline block comments start at the
    // line containing the directive text.
    if report_unused_expect_error && !has_ts_nocheck {
        for (idx, directive) in directives.iter().enumerate() {
            if directive.is_expect_error && !directive_used[idx] {
                diagnostics.push(Diagnostic::error(
                    file_name.to_string(),
                    directive.unused_diagnostic_start,
                    directive.unused_diagnostic_length,
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
        // Note: TS1101 ('with' statements are not allowed in strict mode) is intentionally
        // excluded. It is a grammar check, not a structural parse failure. The parser
        // accepts the with-statement and produces a valid AST; tsc still emits semantic
        // errors like TS2410 alongside TS1101.
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
        | 1389 // '{0}' is not allowed as a variable declaration name
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
        | 1441 // Cannot start a function call in a type annotation
        | 1442 // Identifier or expression expected
        | 1068 // Unexpected token. A constructor, method, accessor, or property was expected.
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
        | 1492 // 'using' declarations may not have binding patterns (grammar constraint, AST is valid)
        | 1499 // Unknown regular expression flag (grammar check in tsc's checker, not a parse failure)
        | 1500 // Duplicate regular expression flag (grammar check, AST is valid)
        | 1502 // The Unicode 'u' and 'v' flags cannot be set simultaneously (grammar check, AST is valid)
        | 17019 // '?' at end of type is not valid TS syntax (parser recovers valid AST)
        | 17020 // '?' at start of type is not valid TS syntax (parser recovers valid AST)
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

    fn check_merged_program_file(files: &[(&str, &str)], entry_file: &str) -> Vec<Diagnostic> {
        let program = merged_program(files);
        let entry_idx = program
            .files
            .iter()
            .position(|file| file.file_name == entry_file)
            .expect("entry file should exist");
        let augmentations = MergedAugmentations::from_program(&program);
        let all_arenas = Arc::new(
            program
                .files
                .iter()
                .map(|file| Arc::clone(&file.arena))
                .collect::<Vec<_>>(),
        );
        let all_binders = Arc::new(
            program
                .files
                .iter()
                .enumerate()
                .map(|(file_idx, file)| {
                    Arc::new(create_binder_from_bound_file_with_augmentations(
                        file,
                        &program,
                        file_idx,
                        &augmentations,
                    ))
                })
                .collect::<Vec<_>>(),
        );
        let file_names = program
            .files
            .iter()
            .map(|file| file.file_name.clone())
            .collect::<Vec<_>>();
        let (resolved_module_paths, resolved_modules) =
            tsz::checker::module_resolution::build_module_resolution_maps(&file_names);
        let opts = tsz_common::checker_options::CheckerOptions {
            jsx_mode: tsz_common::checker_options::JsxMode::React,
            no_unused_locals: true,
            no_lib: true,
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let interner = tsz_solver::TypeInterner::new();
        let mut checker = CheckerState::new(
            all_arenas[entry_idx].as_ref(),
            all_binders[entry_idx].as_ref(),
            &interner,
            file_names[entry_idx].clone(),
            opts,
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(entry_idx);
        checker.ctx.set_lib_contexts(Vec::new());
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);

        checker.check_source_file(program.files[entry_idx].source_file);
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|diag| diag.code != 2318)
            .cloned()
            .collect()
    }

    #[test]
    fn plain_class_needs_no_helpers() {
        assert!(helper_names("class C { method() {} }").is_empty());
    }

    #[test]
    fn jsx_fragment_factory_scope_ignores_external_module_globals() {
        let diagnostics = check_merged_program_file(
            &[
                (
                    "/renderer.d.ts",
                    r#"
declare global {
    namespace JSX {
        interface IntrinsicElements { [e: string]: any; }
        interface Element {}
    }
}
export function h(): void;
export function Fragment(): void;
"#,
                ),
                (
                    "/entry.tsx",
                    r#"/** @jsx h
 * @jsxFrag Fragment
 */
import { Fragment } from "./renderer";
const _frag = <></>;
"#,
                ),
            ],
            "/entry.tsx",
        );

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == 2874 && diag.message_text.contains("'h'")),
            "Expected TS2874 for missing JSX factory `h`, got: {diagnostics:#?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.code == 2879 && diag.message_text.contains("Fragment")),
            "Expected imported fragment factory to remain in scope, got: {diagnostics:#?}"
        );
    }

    /// TS1101 ('with' statements not allowed in strict mode) is a grammar
    /// check, not a structural parse failure. The parser produces a valid AST
    /// for the with-statement; tsc still emits semantic errors like TS2410
    /// alongside it. Including TS1101 in `is_real_syntax_error` would cause
    /// the CLI's `program_has_real_syntax_errors` filter to drop every
    /// semantic diagnostic in any module file containing a `with` statement.
    #[test]
    fn ts1101_is_not_treated_as_real_syntax_error() {
        assert!(
            !is_real_syntax_error(1101),
            "TS1101 must NOT be classified as a real syntax error \
             — it is a strict-mode grammar check that does not malform the AST"
        );
    }

    /// Sanity-check that genuinely structural parse failures remain classified
    /// as real syntax errors so the regression of TS1101's removal does not
    /// accidentally weaken the broader filter.
    #[test]
    fn structural_parse_failures_remain_real_syntax_errors() {
        for code in [1005u32, 1109, 1128] {
            assert!(
                is_real_syntax_error(code),
                "TS{code} should still be classified as a real syntax error"
            );
        }
    }

    #[test]
    fn ts_directive_scan_ignores_jsdoc_example_mentions() {
        let source = r#"/**
Example:
```
// @ts-expect-error
foo.bar;
```
*/
const value = 1;
"#;
        assert!(
            find_ts_directives(source).is_empty(),
            "directives embedded in documentation examples must not target source lines"
        );
    }

    #[test]
    fn ts_directive_scan_keeps_real_line_directives() {
        let directives =
            find_ts_directives("// @ts-expect-error: intentional\nconst x: string = 1;");
        assert_eq!(directives.len(), 1);
        assert!(directives[0].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 1);
    }

    #[test]
    fn ts_directive_scan_accepts_form_feed_before_directive() {
        let directives = find_ts_directives("//\x0C@ts-ignore\nconst x: string = 1;");
        assert_eq!(directives.len(), 1);
        assert!(!directives[0].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 1);
    }

    #[test]
    fn ts_directive_scan_accepts_vertical_tab_before_directive() {
        let directives = find_ts_directives("//\x0B@ts-ignore\nconst x: string = 1;");
        assert_eq!(directives.len(), 1);
        assert!(!directives[0].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 1);
    }

    #[test]
    fn ts_directive_suppresses_next_line_with_form_feed_spacing() {
        let source = "//\x0C@ts-ignore\nlet x: string = 1;\n";
        let mut diagnostics = vec![Diagnostic::error(
            "repro.ts".to_string(),
            21,
            1,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            2322,
        )];

        apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);

        assert!(
            diagnostics.is_empty(),
            "Expected form-feed @ts-ignore to suppress the next-line diagnostic, got: {diagnostics:?}"
        );
    }

    #[test]
    fn ts_directive_scan_keeps_template_substitution_directives() {
        let directives =
            find_ts_directives("const value = `${/* @ts-ignore */ 0}`;\nconst x: string = 1;");
        assert_eq!(directives.len(), 1);
        assert!(!directives[0].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 1);

        let directives = find_ts_directives(
            "const value = `${/* @ts-expect-error */ 0}`;\nconst x: string = 1;",
        );
        assert_eq!(directives.len(), 1);
        assert!(directives[0].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 1);
    }

    #[test]
    fn ts_directive_suppresses_next_line_from_template_substitution() {
        let source = "const value = `${/* @ts-ignore */ 0}`;\nconst x: string = 1;\n";
        let mut diagnostics = vec![Diagnostic::error(
            "repro.ts".to_string(),
            45,
            1,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            2322,
        )];

        apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);

        assert!(
            diagnostics.is_empty(),
            "Expected template-substitution @ts-ignore to suppress the next-line diagnostic, got: {diagnostics:?}"
        );
    }

    #[test]
    fn ts_directive_scan_ignores_plain_template_text() {
        assert!(
            find_ts_directives("const value = `// @ts-ignore`;\nconst x: string = 1;").is_empty(),
            "directives in template text must not target source lines"
        );
    }

    #[test]
    fn ts_directive_line_starts_treat_cr_as_line_break() {
        assert_eq!(
            build_line_starts("// @ts-ignore\rlet x: string = 1;\r"),
            vec![0, 14, 33],
        );
        assert_eq!(
            build_line_starts("// @ts-ignore\r\nlet x: string = 1;\n"),
            vec![0, 15, 34],
        );
    }

    #[test]
    fn ts_ignore_suppresses_jsdoc_at_type_ts2304_in_declaration_emit() {
        // Issue #3996: a `// @ts-ignore` followed by a JSDoc `@type` annotation
        // referencing a missing name was incorrectly preserved during checked-JS
        // declaration emit because of a `line_text.contains("@type {")`
        // carve-out. tsc 6.0.3 suppresses the diagnostic regardless of which
        // checking surface (source-file vs declaration-emit) raised it.
        let source = "// @ts-ignore\n/** @type {Missing} */\nexport const x = 1;\n";
        let mut diagnostics = vec![Diagnostic::error(
            "repro.js".to_string(),
            22,
            7,
            "Cannot find name 'Missing'.".to_string(),
            2304,
        )];
        apply_ts_directive_suppression("repro.js", source, &mut diagnostics, true);
        assert!(
            diagnostics.is_empty(),
            "Expected @ts-ignore to suppress JSDoc @type TS2304 even during declaration emit, got: {diagnostics:?}"
        );
    }

    #[test]
    fn ts_ignore_suppresses_next_line_with_cr_only_line_endings() {
        let source = "// @ts-ignore\rlet x: string = 1;\r";
        let mut diagnostics = vec![Diagnostic::error(
            "repro.ts".to_string(),
            18,
            1,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            2322,
        )];

        apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);

        assert!(
            diagnostics.is_empty(),
            "Expected CR-only @ts-ignore to suppress the next-line diagnostic, got: {diagnostics:?}"
        );
    }

    #[test]
    fn ts_expect_error_uses_next_line_with_cr_only_line_endings() {
        let source = "// @ts-expect-error\rlet x: string = 1;\r";
        let mut diagnostics = vec![Diagnostic::error(
            "repro.ts".to_string(),
            24,
            1,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            2322,
        )];

        apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);

        assert!(
            diagnostics.is_empty(),
            "Expected CR-only @ts-expect-error to suppress the next-line diagnostic, got: {diagnostics:?}"
        );
    }

    /// Anchor regression for TS2578.
    ///
    /// tsc 6.0.3 emits TS2578 at the comment range — the `/` of the `//`
    /// or `/*` opener — not at the enclosing line start. For an indented
    /// `  // @ts-expect-error` that means the diagnostic span starts at
    /// the comment's first character (here byte 2, column 3), not at
    /// column 1.
    ///
    /// Source: type-challenges 00004-easy-pick (issue #4902).
    #[test]
    fn unused_expect_error_anchors_at_indented_comment_start() {
        let source = "const a = 1;\n  // @ts-expect-error\nconst x = 1;\n";
        let mut diagnostics = Vec::new();
        apply_ts_directive_suppression("anchor.ts", source, &mut diagnostics, false);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, 2578);
        // The `//` of the indented comment starts at byte offset
        // `"const a = 1;\n  ".len() == 15`. tsc anchors at this position
        // (column 3 on the comment line), not at the line start (column 1).
        assert_eq!(diagnostics[0].start, 15);
        // Span covers the entire comment, including the `//` opener.
        assert_eq!(diagnostics[0].length, "// @ts-expect-error".len() as u32);
    }

    /// Same rule for block comments: anchor at `/*`, span the whole comment.
    /// Anti-hardcoding cover: a different comment opener and a different
    /// indent — the fix must key on the structural comment range, not on
    /// `//` specifically or on a fixed offset.
    #[test]
    fn unused_expect_error_anchors_at_indented_block_comment_start() {
        let source = "    /* @ts-expect-error */\nconst y = 1;\n";
        let mut diagnostics = Vec::new();
        apply_ts_directive_suppression("anchor-block.ts", source, &mut diagnostics, false);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, 2578);
        // 4 spaces of indent, then `/*` starts at byte 4 (column 5).
        assert_eq!(diagnostics[0].start, 4);
        assert_eq!(diagnostics[0].length, "/* @ts-expect-error */".len() as u32);
    }

    #[test]
    fn unused_expect_error_in_multiline_block_anchors_at_directive_line_start() {
        let source = "    /*\n   @ts-expect-error */\nconst y = 1;\n";
        let mut diagnostics = Vec::new();
        apply_ts_directive_suppression(
            "anchor-multiline-block.ts",
            source,
            &mut diagnostics,
            false,
        );

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, 2578);
        assert_eq!(diagnostics[0].start, "    /*\n".len() as u32);
        assert_eq!(diagnostics[0].length, "   @ts-expect-error */".len() as u32);
    }

    #[test]
    fn raw_ts_nocheck_text_does_not_suppress_unused_expect_error() {
        let source = r#"const marker = "@ts-nocheck";

// @ts-expect-error
const stringValue = 1;

marker;
stringValue;
"#;
        let mut diagnostics = Vec::new();

        apply_ts_directive_suppression("string.ts", source, &mut diagnostics, false);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, 2578);
    }

    #[test]
    fn late_ts_nocheck_comment_does_not_suppress_unused_expect_error() {
        let source = r#"const before = 0;

// @ts-nocheck
// @ts-expect-error
const lateValue = 1;

before;
lateValue;
"#;
        let mut diagnostics = Vec::new();

        apply_ts_directive_suppression("late-comment.ts", source, &mut diagnostics, false);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, 2578);
    }

    #[test]
    fn leading_ts_nocheck_suppresses_unused_expect_error() {
        let source = r#"// @ts-nocheck

// @ts-expect-error
const unchecked = 1;

unchecked;
"#;
        let mut diagnostics = Vec::new();

        apply_ts_directive_suppression(
            "actual-nocheck-control.ts",
            source,
            &mut diagnostics,
            false,
        );

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn ts_directive_scan_keeps_triple_slash_directives() {
        let directives = find_ts_directives("/// @ts-ignore\nx();");
        assert_eq!(directives.len(), 1);
        assert!(!directives[0].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 1);
    }

    #[test]
    fn build_line_starts_handles_cr_and_crlf() {
        // \n only
        assert_eq!(build_line_starts("a\nb\nc"), vec![0, 2, 4]);
        // \r only (classic Mac line endings)
        assert_eq!(build_line_starts("a\rb\rc"), vec![0, 2, 4]);
        // \r\n (Windows): one line break, not two
        assert_eq!(build_line_starts("a\r\nb\r\nc"), vec![0, 3, 6]);
        // Mixed
        assert_eq!(build_line_starts("a\nb\rc\r\nd"), vec![0, 2, 4, 7]);
        // U+2028 LINE SEPARATOR
        assert_eq!(build_line_starts("a\u{2028}b"), vec![0, 4]);
        // U+2029 PARAGRAPH SEPARATOR
        assert_eq!(build_line_starts("a\u{2029}b"), vec![0, 4]);
    }

    #[test]
    fn ts_directive_scan_handles_cr_only_line_endings() {
        // CR-only file: directive on line 0 must suppress line 1, and the
        // single-line comment must not swallow the rest of the file.
        let directives = find_ts_directives("// @ts-ignore\rlet x: string = 1;\r");
        assert_eq!(directives.len(), 1);
        assert!(!directives[0].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 1);
        // Comment span must stop at the CR, not run to end-of-file.
        assert_eq!(
            directives[0].unused_diagnostic_length,
            "// @ts-ignore".len() as u32
        );
    }

    #[test]
    fn ts_directive_scan_handles_crlf_line_endings() {
        let directives = find_ts_directives("// @ts-expect-error\r\nconst x: string = 1;\r\n");
        assert_eq!(directives.len(), 1);
        assert!(directives[0].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 1);
        // The CR must be excluded from the comment span (matches the
        // existing behaviour of the LF-only path).
        assert_eq!(
            directives[0].unused_diagnostic_length,
            "// @ts-expect-error".len() as u32
        );
    }

    #[test]
    fn ts_directive_suppresses_diagnostic_with_cr_line_endings() {
        let source = "// @ts-ignore\rlet x: string = 1;\r";
        // The bad assignment is on line 1 (0-based) at the byte offset of
        // the literal `1` after the CR.
        let bad_offset = source.find('1').unwrap() as u32;
        let mut diagnostics = vec![Diagnostic::error(
            "repro.ts".to_string(),
            bad_offset,
            1,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            2322,
        )];
        apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);
        assert!(
            diagnostics.is_empty(),
            "@ts-ignore must suppress the next-line diagnostic with CR-only endings: {diagnostics:?}"
        );
    }

    #[test]
    fn ts_directive_scan_keeps_block_comment_directives() {
        let directives = find_ts_directives(
            r#"/**
 @ts-expect-error */
texts.push(100);

{/*@ts-ignore*/}
<MyComponent foo={100} />"#,
        );
        assert_eq!(directives.len(), 2);
        assert!(directives[0].is_expect_error);
        assert!(!directives[1].is_expect_error);
        assert_eq!(directives[0].suppressed_line, 2);
        assert_eq!(directives[1].suppressed_line, 5);
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
        let mut options = ResolvedCompilerOptions {
            import_helpers: true,
            es_module_interop: true,
            ..Default::default()
        };
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
    fn declaration_extension_variants_do_not_require_imported_tslib_helpers() {
        let program = merged_program(&[
            (
                "__virtual__/index.d.mts",
                "declare class Base {}\ndeclare class Derived extends Base {}",
            ),
            (
                "__virtual__/index.d.cts",
                "declare class CjsBase {}\ndeclare class CjsDerived extends CjsBase {}",
            ),
        ]);
        let mut options = ResolvedCompilerOptions {
            import_helpers: true,
            ..Default::default()
        };
        options.checker.target = tsz_common::ScriptTarget::ES5;

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            Path::new("/__virtual__"),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            diagnostics.iter().all(|diag| diag.code != 2354),
            "Did not expect TS2354 for declaration-file variants. Got: {diagnostics:#?}"
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
        let options = ResolvedCompilerOptions {
            import_helpers: true,
            checker: tsz_common::checker_options::CheckerOptions {
                target: tsz_common::ScriptTarget::ES2015,
                experimental_decorators: true,
                ..Default::default()
            },
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            Path::new("/app"),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.code == 2343 || diag.code == 2354),
            "Did not expect tslib helper diagnostics when index.d.ts declares __decorate. Got: {diagnostics:#?}"
        );
    }

    #[test]
    fn ambient_tslib_helper_comments_do_not_satisfy_missing_helpers() {
        let program = merged_program(&[
            (
                "main.ts",
                "export async function load(): Promise<number> {\n    await Promise.resolve();\n    return 1;\n}\n",
            ),
            (
                "node_modules/tslib/tslib.d.ts",
                r#"declare module "tslib" {
  // Mentioning __importStar in a comment should not provide any helper export.
  // export declare function __awaiter(thisArg: any, _arguments: any, P: any, generator: any): any;
  export {};
}
"#,
            ),
        ]);
        let mut options = ResolvedCompilerOptions {
            import_helpers: true,
            ..Default::default()
        };
        options.checker.target = tsz_common::ScriptTarget::ES5;

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            Path::new("/"),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == 2343
                    && diag.file == "main.ts"
                    && diag.message_text.contains("__awaiter")
            }),
            "Expected TS2343 for missing __awaiter. Got: {diagnostics:#?}"
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == 2343
                    && diag.file == "main.ts"
                    && diag.message_text.contains("__generator")
            }),
            "Expected TS2343 for missing __generator. Got: {diagnostics:#?}"
        );
    }

    #[test]
    fn ambient_tslib_helper_declarations_satisfy_async_helpers() {
        let program = merged_program(&[
            (
                "main.ts",
                "export async function load(): Promise<number> {\n    await Promise.resolve();\n    return 1;\n}\n",
            ),
            (
                "node_modules/tslib/tslib.d.ts",
                r#"declare module "tslib" {
  export declare function __awaiter(thisArg: any, _arguments: any, P: any, generator: any): any;
  export declare function __generator(thisArg: any, body: any): any;
}
"#,
            ),
        ]);
        let mut options = ResolvedCompilerOptions {
            import_helpers: true,
            ..Default::default()
        };
        options.checker.target = tsz_common::ScriptTarget::ES5;

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            Path::new("/"),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2343),
            "Did not expect missing-helper diagnostics when ambient tslib declares async helpers. Got: {diagnostics:#?}"
        );
    }

    #[test]
    fn no_types_and_symbols_still_honors_project_local_tslib() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let tslib_dir = temp_dir.path().join("node_modules").join("tslib");
        std::fs::create_dir_all(&tslib_dir).unwrap();
        std::fs::write(
            tslib_dir.join("index.d.ts"),
            "export declare function __decorate(decorators: Function[], target: any, key?: string | symbol, desc?: any): any;\n",
        )
        .unwrap();

        let program = merged_program(&[(
            "/app/a.ts",
            "declare var dec: any, __decorate: any;\n@dec export class A {}\n",
        )]);
        let options = ResolvedCompilerOptions {
            import_helpers: true,
            checker: tsz_common::checker_options::CheckerOptions {
                target: tsz_common::ScriptTarget::ES2015,
                experimental_decorators: true,
                no_types_and_symbols: true,
                ..Default::default()
            },
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = detect_missing_tslib_helper_diagnostics(
            &program,
            &options,
            temp_dir.path(),
            &rustc_hash::FxHashMap::default(),
        );

        assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.code == 2343 || diag.code == 2354),
            "Did not expect tslib helper diagnostics when a project-local tslib exists. Got: {diagnostics:#?}"
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
        let mut options = ResolvedCompilerOptions {
            import_helpers: true,
            ..Default::default()
        };
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
        let mut options = ResolvedCompilerOptions {
            import_helpers: true,
            ..Default::default()
        };
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

        let filtered = filtered_parse_diagnostics(&diagnostics, false);
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
    fn filtered_parse_diagnostics_keeps_await_ts1359_with_unrelated_parse_errors() {
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
                start: 10,
                length: 6,
                message: "A module cannot have multiple default exports.".to_string(),
                code: 2528,
            },
        ];

        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1359),
            "TS1359 for 'await' should survive unrelated parse diagnostics, got: {codes:?}"
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

        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1359),
            "TS1359 for 'await' should be kept when it's the only diagnostic, got: {codes:?}"
        );
    }

    #[test]
    fn js_parse_allowlist_keeps_plain_js_binder_strict_codes() {
        for code in [1214, 18012] {
            assert!(
                is_ts1xxx_allowed_in_js(code),
                "plain JS binder parse diagnostic TS{code} should be reported in JavaScript files"
            );
        }
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

    #[test]
    fn js_parse_allowlist_keeps_ts1163() {
        assert!(
            is_ts1xxx_allowed_in_js(1163),
            "TS1163 should be preserved for JS yield-outside-generator diagnostics"
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

    #[test]
    fn regex_flag_errors_do_not_suppress_semantic_diagnostics() {
        // TS1499 (unknown regex flag) should not set has_syntax_parse_errors,
        // so TS2339 (property does not exist) should still be emitted.
        assert!(
            is_non_suppressing_parse_error(1499),
            "TS1499 (Unknown regex flag) should be non-suppressing"
        );
        assert!(
            is_non_suppressing_parse_error(1500),
            "TS1500 (Duplicate regex flag) should be non-suppressing"
        );
        assert!(
            is_non_suppressing_parse_error(1502),
            "TS1502 (Incompatible u/v flags) should be non-suppressing"
        );
    }

    /// Helper: parse a single file and collect noCheck path diagnostics.
    fn collect_no_check_diags(file_name: &str, source: &str) -> Vec<Diagnostic> {
        let mut parse_results =
            parallel::parse_files_parallel(vec![(file_name.to_string(), source.to_string())]);
        let result = parse_results.remove(0);
        let options = ResolvedCompilerOptions::default();
        let program_has_real_syntax_errors = result
            .parse_diagnostics
            .iter()
            .any(|d| is_real_syntax_error(d.code));
        collect_no_check_parse_diagnostics_for_file(
            &result.file_name,
            &result.arena,
            result.source_file,
            &result.parse_diagnostics,
            &options,
            program_has_real_syntax_errors,
        )
    }

    #[test]
    fn no_check_path_emits_ts8010_for_js_parameter_type_annotation() {
        // Issue #3692: `--noCheck` previously skipped TS8xxx grammar
        // diagnostics that tsc reports from its parser. Confirm that a
        // type-annotated JS parameter still produces TS8010 here.
        let diagnostics = collect_no_check_diags("a.js", "function f(x: number) {}\n");
        assert!(
            diagnostics.iter().any(|d| d.code == 8010),
            "expected TS8010 in JS noCheck output, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn no_check_path_emits_ts8010_for_js_variable_type_annotation() {
        // Variable declarations with TS-only type annotations also surface.
        let diagnostics = collect_no_check_diags("a.js", "let x: number;\n");
        assert!(
            diagnostics.iter().any(|d| d.code == 8010),
            "expected TS8010 in JS noCheck output for `let x: number`, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn no_check_path_does_not_emit_ts8010_for_typescript_files() {
        // The grammar walker must not fire on TypeScript files.
        let diagnostics = collect_no_check_diags("a.ts", "function f(x: number) {}\n");
        assert!(
            !diagnostics.iter().any(|d| d.code == 8010),
            "TS8010 must not fire on TypeScript files, got: {diagnostics:#?}"
        );
    }
}
