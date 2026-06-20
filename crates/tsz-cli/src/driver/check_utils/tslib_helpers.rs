//! Imported tslib helper requirement detection.

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
    let mut max_parameter_count: Option<usize> = None;
    for &decl_idx in &symbol.declarations {
        if let Some(arenas) = program.declaration_arenas.get(&(sym_id, decl_idx)) {
            for arena in arenas {
                let node = arena.get(decl_idx)?;
                if let Some(func) = arena.get_function(node) {
                    let parameter_count = func.parameters.nodes.len();
                    max_parameter_count = Some(
                        max_parameter_count.map_or(parameter_count, |max| max.max(parameter_count)),
                    );
                }
            }
        }
        if let Some(arena) = program.symbol_arenas.get(&sym_id) {
            let node = arena.get(decl_idx)?;
            if let Some(func) = arena.get_function(node) {
                let parameter_count = func.parameters.nodes.len();
                max_parameter_count = Some(
                    max_parameter_count.map_or(parameter_count, |max| max.max(parameter_count)),
                );
            }
        }
    }
    max_parameter_count
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
    let mut search_start = 0usize;
    let mut max_parameter_count: Option<usize> = None;
    while let Some(marker_idx) =
        find_source_marker_outside_trivia_from(source, &marker, search_start)
    {
        search_start = marker_idx + marker.len();
        if let Some(parameter_count) =
            extract_declared_function_parameter_count_at(source, search_start)
        {
            max_parameter_count =
                Some(max_parameter_count.map_or(parameter_count, |max| max.max(parameter_count)));
        }
    }
    max_parameter_count
}

fn extract_declared_function_parameter_count_at(source: &str, mut idx: usize) -> Option<usize> {
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
                    if idx
                        .checked_add(rel_idx)
                        .and_then(|byte_idx| source.as_bytes().get(byte_idx.saturating_sub(1)))
                        == Some(&b'=')
                    {
                        continue;
                    }
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

fn find_source_marker_outside_trivia_from(
    source: &str,
    marker: &str,
    mut search_start: usize,
) -> Option<usize> {
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
