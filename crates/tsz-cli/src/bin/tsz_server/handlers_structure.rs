//! Structural, format, and miscellaneous handlers for tsz-server.
//!
//! Handles formatting, inlay hints, selection ranges, call hierarchy,
//! outlining spans, brace matching, refactoring stubs, and related commands.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::emitter::{ModuleKind, Printer, PrinterOptions};
use tsz::lsp::text_edits::narrow_indentation_only_edit;

struct CompileOnSaveProject {
    config_path: String,
    config_dir: std::path::PathBuf,
    enabled: bool,
    file_names: Vec<String>,
    uses_out_file: bool,
    out_dir: Option<String>,
    module: ModuleKind,
}

impl CompileOnSaveProject {
    fn output_path_for(&self, file: &str) -> std::path::PathBuf {
        let input = std::path::Path::new(file);
        let mut relative = input
            .strip_prefix(&self.config_dir)
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|_| {
                input
                    .file_name()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from(file))
            });
        relative.set_extension("js");
        if let Some(out_dir) = self.out_dir.as_deref() {
            let out_dir = std::path::Path::new(out_dir);
            let out_dir = if out_dir.is_absolute() {
                out_dir.to_path_buf()
            } else {
                self.config_dir.join(out_dir)
            };
            out_dir.join(relative)
        } else {
            input.with_extension("js")
        }
    }
}
use tsz::lsp::code_actions::CodeActionProvider;
use tsz::lsp::editor_decorations::inlay_hints::{InlayHintKind, InlayHintsProvider};
use tsz::lsp::editor_ranges::folding::FoldingRangeProvider;
use tsz::lsp::editor_ranges::selection_range::SelectionRangeProvider;
use tsz::lsp::hierarchy::call_hierarchy::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, CallHierarchyProvider,
    ImportResolutionRequest,
};
use tsz::lsp::highlighting::semantic_tokens::SemanticTokensProvider;
use tsz::lsp::position::{LineMap, Position, Range};
use tsz::lsp::rename::file_rename::FileRenameProvider;
use tsz::lsp::rename::linked_editing::LinkedEditingProvider;
use tsz_solver::construction::TypeInterner;

#[path = "handlers_structure_refactors.rs"]
mod refactors;

impl Server {
    fn tsserver_call_hierarchy_name_kind(name: &str, kind: &str) -> (String, String) {
        if kind == "file" {
            return (name.to_string(), "script".to_string());
        }
        if kind == "property" {
            if let Some(stripped) = name.strip_prefix("get ") {
                return (stripped.to_string(), "getter".to_string());
            }
            if let Some(stripped) = name.strip_prefix("set ") {
                return (stripped.to_string(), "setter".to_string());
            }
        }
        (name.to_string(), kind.to_string())
    }

    fn call_hierarchy_probe_positions(
        line_map: &LineMap,
        source_text: &str,
        position: Position,
    ) -> Vec<Position> {
        let Some(base_offset) = line_map.position_to_offset(position, source_text) else {
            return vec![position];
        };

        let len = source_text.len() as u32;
        let bytes = source_text.as_bytes();
        let mut positions = vec![position];

        // Fourslash call-hierarchy markers are often comment-based (`/**/foo`).
        // Probe just after the comment terminator to resolve the intended token.
        if base_offset + 1 < len
            && bytes[base_offset as usize] == b'/'
            && bytes[(base_offset + 1) as usize] == b'*'
        {
            let mut probe = base_offset + 2;
            while probe + 1 < len {
                if bytes[probe as usize] == b'*' && bytes[(probe + 1) as usize] == b'/' {
                    probe += 2;
                    break;
                }
                probe += 1;
            }
            while probe < len && bytes[probe as usize].is_ascii_whitespace() {
                probe += 1;
            }
            if probe < len {
                positions.push(line_map.offset_to_position(probe, source_text));
            }
        }

        if base_offset < len {
            positions.push(
                line_map.offset_to_position(base_offset.saturating_add(1).min(len), source_text),
            );
        }
        if base_offset > 0 {
            positions.push(line_map.offset_to_position(base_offset - 1, source_text));
        }

        positions
    }

    pub(crate) fn handle_get_supported_code_fixes(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let codes: Vec<String> = tsz::lsp::code_actions::CodeFixRegistry::supported_error_codes()
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self.success_response(seq, request, Some(serde_json::json!(codes)))
    }

    pub(crate) fn handle_apply_code_action_command(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.unsupported_response(
            seq,
            request,
            "tsz code-fix providers emit text edits, not command payloads",
        )
    }

    pub(crate) fn handle_encoded_semantic_classifications_full(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let native_open_files = serde_json::to_value(&self.open_files).ok()?;
            if let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "encodedSemanticClassifications",
                "file": file,
                "start": request.arguments.get("start").and_then(serde_json::Value::as_u64).unwrap_or(0),
                "length": request.arguments.get("length").and_then(serde_json::Value::as_u64).unwrap_or(0),
                "format": request.arguments.get("format").and_then(serde_json::Value::as_str).unwrap_or("original"),
                "openFiles": native_open_files,
            })) {
                return Some(native);
            }
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let mut provider =
                SemanticTokensProvider::new(&arena, &binder, &line_map, &source_text);
            let tokens = provider.get_semantic_tokens(root);
            // Provider emits the LSP 5-tuple delta encoding
            // (deltaLine, deltaChar, length, tokenType, tokenModifiers).
            // tsserver's `encodedSemanticClassifications-full` expects the
            // "2020" format: triples of (absStart, length, classId) with
            // `classId = (modifierBits << 8) | tokenType`. Convert in
            // place so the fourslash harness's span-length assertions
            // match tsc.
            let mut converted: Vec<u32> = Vec::with_capacity(tokens.len() / 5 * 3);
            let mut prev_line: u32 = 0;
            let mut prev_char: u32 = 0;
            let mut i = 0;
            while i + 4 < tokens.len() {
                let delta_line = tokens[i];
                let delta_char = tokens[i + 1];
                let length = tokens[i + 2];
                let token_type = tokens[i + 3];
                let token_modifiers = tokens[i + 4];
                let line = prev_line + delta_line;
                let char = if delta_line == 0 {
                    prev_char + delta_char
                } else {
                    delta_char
                };
                let position = tsz_common::position::Position::new(line, char);
                let abs_start = line_map
                    .position_to_offset(position, &source_text)
                    .unwrap_or(0);
                let class_id = (token_modifiers << 8) | token_type;
                converted.push(abs_start);
                converted.push(length);
                converted.push(class_id);
                prev_line = line;
                prev_char = char;
                i += 5;
            }
            Some(serde_json::json!({
                "spans": converted,
                "endOfLineState": 0,
            }))
        })();
        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"spans": [], "endOfLineState": 0}))),
        )
    }

    /// Implement the `encodedSyntacticClassifications-full` tsserver
    /// command. Walks the source text via the scanner and emits
    /// `(start, length, classificationId)` triples for every non-trivia
    /// token. Classification IDs match tsc's `TokenClass` (0 = punctuation,
    /// 1 = comment, 2 = identifier, 3 = keyword, 4 = numericLiteral,
    /// 5 = operator, 6 = stringLiteral, 7 = regexLiteral, 10 = punctuation).
    ///
    /// `start` and `length` are UTF-16 unit counts (matching tsserver). For
    /// ASCII source, byte == UTF-16 unit; multi-byte content widens
    /// correctly via `len_utf16()`. See #3717.
    pub(crate) fn handle_encoded_syntactic_classifications_full(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let span_start_byte = request
                .arguments
                .get("start")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            let span_length_byte = request
                .arguments
                .get("length")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(u32::MAX as u64) as usize;
            let span_end_byte = span_start_byte.saturating_add(span_length_byte);

            let source_text = self.open_files.get(file)?.clone();

            // UTF-16 prefix counts so each token's byte offset translates to
            // a UTF-16 offset in O(1) after a single pass.
            let mut utf16_prefix: Vec<u32> = Vec::with_capacity(source_text.len() + 1);
            utf16_prefix.push(0);
            let mut count: u32 = 0;
            for ch in source_text.chars() {
                count = count.saturating_add(ch.len_utf16() as u32);
                for _ in 0..ch.len_utf8() {
                    utf16_prefix.push(count);
                }
            }
            let to_utf16 = |byte: usize| -> u32 {
                let idx = byte.min(utf16_prefix.len().saturating_sub(1));
                utf16_prefix[idx]
            };

            let mut scanner = tsz_scanner::scanner_impl::ScannerState::new(source_text, false);
            let mut spans: Vec<u32> = Vec::new();

            loop {
                let token = scanner.scan();
                if token == tsz_scanner::SyntaxKind::EndOfFileToken {
                    break;
                }
                let token_start = scanner.get_token_start();
                let token_end = scanner.get_token_end();

                if token_start >= span_end_byte {
                    break;
                }
                if token_end <= span_start_byte {
                    continue;
                }

                let class_id = match Self::classify_syntactic_token(token) {
                    Some(id) => id,
                    None => continue,
                };

                let utf16_start = to_utf16(token_start);
                let utf16_end = to_utf16(token_end);
                let length = utf16_end.saturating_sub(utf16_start);
                if length == 0 {
                    continue;
                }
                spans.push(utf16_start);
                spans.push(length);
                spans.push(class_id);
            }

            Some(serde_json::json!({
                "spans": spans,
                "endOfLineState": 0,
            }))
        })();
        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"spans": [], "endOfLineState": 0}))),
        )
    }

    /// Map a scanner `SyntaxKind` to tsc's syntactic-classification token
    /// class id. Returns `None` for trivia/EOF that should be skipped.
    fn classify_syntactic_token(token: tsz_scanner::SyntaxKind) -> Option<u32> {
        use tsz_scanner::{SyntaxKind, token_is_keyword};
        // Trivia and EOF: skip.
        if matches!(
            token,
            SyntaxKind::EndOfFileToken
                | SyntaxKind::NewLineTrivia
                | SyntaxKind::WhitespaceTrivia
                | SyntaxKind::ShebangTrivia
                | SyntaxKind::ConflictMarkerTrivia
                | SyntaxKind::NonTextFileMarkerTrivia
        ) {
            return None;
        }
        // Comments → class 1.
        if matches!(
            token,
            SyntaxKind::SingleLineCommentTrivia | SyntaxKind::MultiLineCommentTrivia
        ) {
            return Some(1);
        }
        // Identifiers → class 2.
        if token == SyntaxKind::Identifier
            || token == SyntaxKind::PrivateIdentifier
            || token == SyntaxKind::JsxText
        {
            return Some(2);
        }
        // Keywords → class 3.
        if token_is_keyword(token) {
            return Some(3);
        }
        // Literals.
        match token {
            SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral => return Some(4),
            SyntaxKind::StringLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead
            | SyntaxKind::TemplateMiddle
            | SyntaxKind::TemplateTail => {
                return Some(6);
            }
            SyntaxKind::RegularExpressionLiteral => return Some(7),
            _ => {}
        }
        // Punctuation/operators. tsc folds `=`, `+`, `-`, etc. into class 5
        // (operator) and structural punctuation (`,`, `;`, `(`, `)`, `{`, `}`,
        // `[`, `]`, `.`, `:`, `?`, `=>`) into class 10. The `SyntaxKind`
        // numeric ranges aren't ideal for this distinction, so use a small
        // explicit set for class-10 punctuation; everything else punctuation
        // -shaped becomes class 5.
        if matches!(
            token,
            SyntaxKind::CommaToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::OpenParenToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::DotToken
                | SyntaxKind::DotDotDotToken
                | SyntaxKind::ColonToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::EqualsGreaterThanToken
                | SyntaxKind::AtToken
                | SyntaxKind::HashToken
                | SyntaxKind::BacktickToken
        ) {
            return Some(10);
        }
        Some(5)
    }

    pub(crate) fn handle_emit_output(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;

            // Issue #3784: tsc honors the owning project's `module` and
            // `outDir` for emit-output. Reuse the compile-on-save project
            // lookup (ignoring the `compileOnSave` flag) so the printer
            // module kind and output path match tsserver's behavior.
            let project = self.compile_on_save_project(file);

            let module = project
                .as_ref()
                .map_or_else(|| self.emit_output_module_kind(), |p| p.module);

            let mut printer = Printer::with_source_text_len_and_options(
                &arena,
                source_text.len(),
                PrinterOptions {
                    module,
                    ..Default::default()
                },
            );
            printer.set_source_text(&source_text);
            printer.emit(root);
            let output = printer.take_output();

            let out_name = if let Some(ref project) = project {
                project.output_path_for(file).to_string_lossy().into_owned()
            } else {
                file.strip_suffix(".ts")
                    .or_else(|| file.strip_suffix(".tsx"))
                    .map(|base| format!("{base}.js"))
                    .unwrap_or_else(|| format!("{file}.js"))
            };

            Some(serde_json::json!({
                "outputFiles": [{
                    "name": out_name,
                    "text": output,
                    "writeByteOrderMark": false,
                }],
                "emitSkipped": false,
            }))
        })();
        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"outputFiles": [], "emitSkipped": true}))),
        )
    }

    pub(crate) fn handle_compile_on_save_affected_file_list(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let project = self.compile_on_save_project(file)?;
            if !project.enabled {
                return Some(serde_json::json!([]));
            }
            Some(serde_json::json!([{
                "projectFileName": project.config_path,
                "fileNames": project.file_names,
                "projectUsesOutFile": project.uses_out_file,
            }]))
        })();
        self.success_or_empty_array(seq, request, result)
    }

    pub(crate) fn handle_compile_on_save_emit_file(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let rich_response = request
            .arguments
            .get("richResponse")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let emitted = (|| -> Option<bool> {
            let file = request.arguments.get("file")?.as_str()?;
            let project = self.compile_on_save_project(file)?;
            if !project.enabled {
                return Some(false);
            }
            self.emit_compile_on_save_file(file, &project).ok()?;
            Some(true)
        })()
        .unwrap_or(false);
        let body = if rich_response {
            serde_json::json!({
                "emitSkipped": !emitted,
                "diagnostics": [],
            })
        } else {
            serde_json::json!(emitted)
        };
        self.success_response(seq, request, Some(body))
    }

    fn emit_output_module_kind(&self) -> ModuleKind {
        self.inferred_check_options
            .module
            .as_deref()
            .map(str::to_ascii_lowercase)
            .map(|module| match module.as_str() {
                "none" => ModuleKind::None,
                "commonjs" => ModuleKind::CommonJS,
                "amd" => ModuleKind::AMD,
                "umd" => ModuleKind::UMD,
                "system" => ModuleKind::System,
                "es2015" => ModuleKind::ES2015,
                "es2020" => ModuleKind::ES2020,
                "es2022" => ModuleKind::ES2022,
                "node16" => ModuleKind::Node16,
                "node18" => ModuleKind::Node18,
                "node20" => ModuleKind::Node20,
                "nodenext" => ModuleKind::NodeNext,
                "preserve" => ModuleKind::Preserve,
                _ => ModuleKind::ESNext,
            })
            .unwrap_or(ModuleKind::ESNext)
    }

    fn module_kind_from_config(config_json: &serde_json::Value) -> ModuleKind {
        config_json
            .get("compilerOptions")
            .and_then(|opts| opts.get("module"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_ascii_lowercase)
            .map(|module| match module.as_str() {
                "none" => ModuleKind::None,
                "commonjs" => ModuleKind::CommonJS,
                "amd" => ModuleKind::AMD,
                "umd" => ModuleKind::UMD,
                "system" => ModuleKind::System,
                "es2015" | "es6" => ModuleKind::ES2015,
                "es2020" => ModuleKind::ES2020,
                "es2022" => ModuleKind::ES2022,
                "node16" => ModuleKind::Node16,
                "node18" => ModuleKind::Node18,
                "node20" => ModuleKind::Node20,
                "nodenext" => ModuleKind::NodeNext,
                "preserve" => ModuleKind::Preserve,
                _ => ModuleKind::ESNext,
            })
            .unwrap_or(ModuleKind::ESNext)
    }

    fn compile_on_save_project(&self, file: &str) -> Option<CompileOnSaveProject> {
        let config_path = self.find_project_config_file(file)?;
        let config_json = self.read_config_json(&config_path)?;
        let enabled = config_json
            .get("compileOnSave")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let compiler_options = config_json.get("compilerOptions");
        let uses_out_file = compiler_options
            .and_then(|opts| opts.get("outFile").or_else(|| opts.get("out")))
            .and_then(serde_json::Value::as_str)
            .is_some();
        let out_dir = compiler_options
            .and_then(|opts| opts.get("outDir"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let config_dir = std::path::Path::new(&config_path)
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let (_, _, mut file_names) = self.parse_tsconfig_for_project_info(&config_path);
        if file_names.is_empty() {
            file_names.push(Self::normalize_path_string(std::path::Path::new(file)));
        }
        Some(CompileOnSaveProject {
            config_path,
            config_dir,
            enabled,
            file_names,
            uses_out_file,
            out_dir,
            module: Self::module_kind_from_config(&config_json),
        })
    }

    fn emit_compile_on_save_file(
        &self,
        file: &str,
        project: &CompileOnSaveProject,
    ) -> std::io::Result<()> {
        let (arena, _binder, root, source_text) = self
            .parse_and_bind_file(file)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, file.to_string()))?;
        let mut printer = Printer::with_source_text_len_and_options(
            &arena,
            source_text.len(),
            PrinterOptions {
                module: project.module,
                ..Default::default()
            },
        );
        printer.set_source_text(&source_text);
        printer.emit(root);
        let output = printer.take_output();
        let out_path = project.output_path_for(file);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out_path, output)
    }

    pub(crate) fn handle_prepare_call_hierarchy(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                CallHierarchyProvider::new(&arena, &binder, &line_map, file, &source_text);
            let mut item = None;
            for probe in Self::call_hierarchy_probe_positions(&line_map, &source_text, position) {
                item = provider.prepare(root, probe);
                if item.is_some() {
                    break;
                }
            }
            let item = item?;
            let raw_kind = format!("{:?}", item.kind).to_lowercase();
            let (name, kind) = Self::tsserver_call_hierarchy_name_kind(&item.name, &raw_kind);
            let mut body_item = serde_json::json!({
                "name": name,
                "kind": kind,
                "file": item.uri,
                "span": {
                    "start": Self::lsp_to_tsserver_position(item.range.start),
                    "end": Self::lsp_to_tsserver_position(item.range.end),
                },
                "selectionSpan": {
                    "start": Self::lsp_to_tsserver_position(item.selection_range.start),
                    "end": Self::lsp_to_tsserver_position(item.selection_range.end),
                },
            });
            if let Some(container_name) = item.container_name {
                body_item["containerName"] = serde_json::json!(container_name);
            }
            Some(serde_json::json!([body_item]))
        })();
        self.success_or_empty_array(seq, request, result)
    }

    /// Issue #3753: resolve an outgoing-call import-binding to the actual
    /// exported declaration in the target module.
    ///
    /// Returns `None` when the module specifier can't be resolved (bare
    /// package imports, missing files, parse failures, no matching export).
    /// In that case the caller falls back to the local-import-binding item.
    pub(crate) fn resolve_import_call_hierarchy_target(
        &mut self,
        importer_file: &str,
        request: &ImportResolutionRequest,
    ) -> Option<CallHierarchyItem> {
        // Only handle relative-path specifiers for now. Bare specifiers
        // would need module-resolution machinery the LSP server doesn't
        // wire up yet.
        let spec = &request.module_specifier;
        if !(spec.starts_with("./") || spec.starts_with("../")) {
            return None;
        }

        let importer_path = std::path::Path::new(importer_file);
        let importer_dir = importer_path.parent()?;
        let resolved_path = self.resolve_relative_module_specifier(importer_dir, spec)?;
        let resolved_str = resolved_path.to_string_lossy().into_owned();

        let (arena, binder, _root, source_text) = self.parse_and_bind_file(&resolved_str)?;
        let line_map = LineMap::build(&source_text);
        let provider = CallHierarchyProvider::new(
            &arena,
            &binder,
            &line_map,
            resolved_str.clone(),
            &source_text,
        );

        // Find the exported binding by name. Default imports map to a
        // declaration tagged as default; named imports map to the named
        // export. Namespace imports are not resolved here (no specific
        // export to point at).
        let target_name = request.exported_name.as_deref()?;
        let decl_idx = Self::find_exported_callable(&arena, &binder, target_name)?;

        // Use the provider's prepare-by-position path: locate any identifier
        // at the resolved declaration's position, then build a hierarchy
        // item for it. Falls back to a synthesized item if prepare doesn't
        // recognize the position.
        let decl_node = arena.get(decl_idx)?;
        let pos = line_map.offset_to_position(decl_node.pos, &source_text);
        if let Some(item) = provider.prepare(_root, pos) {
            return Some(CallHierarchyItem {
                uri: resolved_str,
                ..item
            });
        }
        let span_pos = line_map.offset_to_position(decl_node.pos, &source_text);
        let span_end = line_map.offset_to_position(decl_node.end, &source_text);
        Some(CallHierarchyItem {
            name: target_name.to_string(),
            kind: tsz_lsp::SymbolKind::Function,
            uri: resolved_str,
            range: tsz_common::position::Range::new(span_pos, span_end),
            selection_range: tsz_common::position::Range::new(span_pos, span_end),
            container_name: None,
        })
    }

    /// Locate an exported callable (function declaration / class declaration
    /// / variable initializer) by exported name within the bound source
    /// file. Searches `binder.symbols` for symbols tagged as exported with a
    /// matching name and returns the first declaration `NodeIndex`.
    fn find_exported_callable(
        arena: &tsz_parser::parser::node::NodeArena,
        binder: &tsz_binder::BinderState,
        target_name: &str,
    ) -> Option<tsz_parser::NodeIndex> {
        if let Some(sym_id) = binder.file_locals.get(target_name)
            && let Some(symbol) = binder.symbols.get(sym_id)
            && let Some(&decl) = symbol.declarations.first()
        {
            // Skip the symbol if its first declaration is itself an import-binding.
            let kind = arena.get(decl).map(|n| n.kind);
            let is_import_binding = matches!(
                kind,
                Some(k) if k == tsz_parser::syntax_kind_ext::IMPORT_SPECIFIER
                    || k == tsz_parser::syntax_kind_ext::IMPORT_CLAUSE
                    || k == tsz_parser::syntax_kind_ext::NAMESPACE_IMPORT
            );
            if !is_import_binding {
                return Some(decl);
            }
        }
        None
    }

    /// Issue #3753 follow-up: report whether `target_name` (a top-level local
    /// in `target_file`) is also the file's default export — i.e. the same
    /// declaration backs both `module_exports[target_file][target_name]` and
    /// `module_exports[target_file]["default"]`. tsc treats `import x from
    /// "./a"` and `import { x } from "./a"` as both reaching such a function,
    /// so the cross-file caller scan needs to accept default-import bindings
    /// in addition to named-import bindings.
    ///
    /// Returns false when the file can't be parsed/bound, when no `default`
    /// export exists, or when `target_name` and `default` resolve to disjoint
    /// declaration nodes (the typical case for plain `export function`).
    fn target_is_default_export(&self, target_file: &str, target_name: &str) -> bool {
        if target_name == "default" {
            return false;
        }
        let Some((_arena, binder, _root, _src)) = self.parse_and_bind_file(target_file) else {
            return false;
        };
        let Some(file_exports) = binder.module_exports.get(target_file) else {
            return false;
        };
        let Some(default_sid) = file_exports.get("default") else {
            return false;
        };
        let Some(target_sid) = file_exports.get(target_name) else {
            return false;
        };
        if default_sid == target_sid {
            return true;
        }
        let Some(default_sym) = binder.symbols.get(default_sid) else {
            return false;
        };
        let Some(target_sym) = binder.symbols.get(target_sid) else {
            return false;
        };
        // Same declaration node backs both keys: `export default function NAME`,
        // `function NAME() {}; export { NAME as default }`, etc.
        for &decl in &default_sym.declarations {
            if target_sym.declarations.contains(&decl) {
                return true;
            }
        }
        false
    }

    /// Issue #3753: scan the other open files for cross-file callers that
    /// reach `target_item` via an `import` binding. tsc reports those as
    /// incoming calls; without this scan tsz only saw within-file callers
    /// because each `parse_and_bind_file` call only sees one file's
    /// arena/binder.
    ///
    /// For every other open file:
    /// 1. Parse + bind it.
    /// 2. Walk its `IMPORT_DECLARATION` nodes.
    /// 3. For each import whose module specifier resolves to the target's
    ///    file, find the local binding for `target_item.name` (matching by
    ///    exported-name when the spec uses `import { foo as bar }`).
    /// 4. Run that file's `incoming_calls` provider with the local-binding
    ///    position so callers within that file get aggregated correctly.
    pub(crate) fn collect_cross_file_incoming_calls(
        &mut self,
        target_file: &str,
        target_item: &CallHierarchyItem,
    ) -> Vec<CallHierarchyIncomingCall> {
        let mut results: Vec<CallHierarchyIncomingCall> = Vec::new();
        let target_file_canon = Self::canonicalize_path_str(target_file);
        let target_name = target_item.name.clone();
        // Issue #3753 follow-up: a function exported as `export default
        // function NAME` (or `export { NAME as default }`) is reachable in
        // other files via either `import { NAME } from "./a"` or `import
        // <local> from "./a"`. tsc reports both as incoming calls of NAME.
        // Detect whether the target is the file's default export so the
        // default-import / `default`-aliased-named-import branches below also
        // bind to it, not just exported-name matches.
        let target_is_default_export = self.target_is_default_export(target_file, &target_name);
        // Snapshot the keys so we don't iterate while parse_and_bind_file
        // potentially mutates `open_files`.
        let other_files: Vec<String> = self
            .open_files
            .keys()
            .filter(|k| Self::canonicalize_path_str(k) != target_file_canon)
            .cloned()
            .collect();

        for other_file in other_files {
            let Some((arena, binder, root, source_text)) = self.parse_and_bind_file(&other_file)
            else {
                continue;
            };
            let line_map = LineMap::build(&source_text);
            let provider = CallHierarchyProvider::new(
                &arena,
                &binder,
                &line_map,
                other_file.clone(),
                &source_text,
            );

            // Find IMPORT_DECLARATION nodes whose module specifier resolves
            // to the target file, and collect the matching local-binding
            // identifier positions.
            // Collect (binding identifier NodeIndex, local name) for each
            // matching import binding so we can ask the provider which
            // callers reference that local within this file.
            let mut local_bindings: Vec<(tsz_parser::NodeIndex, String)> = Vec::new();
            // Issue #3753 follow-up: collect namespace-import bindings
            // (`import * as ns from "./a"`). For these the import
            // doesn't bind `target_name` directly; we scan for
            // `<ns>.<target_name>(…)` member calls instead.
            let mut namespace_bindings: Vec<tsz_parser::NodeIndex> = Vec::new();
            for node in arena.nodes.iter() {
                if node.kind != tsz_parser::syntax_kind_ext::IMPORT_DECLARATION {
                    continue;
                }
                let Some(import_decl) = arena.get_import_decl(node) else {
                    continue;
                };
                let Some(spec_node) = arena.get(import_decl.module_specifier) else {
                    continue;
                };
                let Some(spec_lit) = arena.get_literal(spec_node) else {
                    continue;
                };
                let spec_text = &spec_lit.text;
                if !(spec_text.starts_with("./") || spec_text.starts_with("../")) {
                    continue;
                }
                let importer_dir = std::path::Path::new(&other_file)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(""));
                let Some(resolved_path) =
                    self.resolve_relative_module_specifier(importer_dir, spec_text)
                else {
                    continue;
                };
                let resolved_canon = Self::canonicalize_path_str(&resolved_path.to_string_lossy());
                if resolved_canon != target_file_canon {
                    continue;
                }

                // Found an import from target_file. Walk its named-imports
                // / default-import / namespace-import bindings to find the
                // local-name identifier whose exported name matches
                // `target_name`. Capture the identifier's position so we
                // can re-run the provider's incoming_calls there.
                if let Some(clause_node) = arena.get(import_decl.import_clause)
                    && clause_node.kind == tsz_parser::syntax_kind_ext::IMPORT_CLAUSE
                {
                    let clause = arena.get_import_clause(clause_node);
                    if let Some(clause) = clause {
                        // Default binding (`import target from "./a"`):
                        // fires when the user asked for incoming calls on a
                        // symbol literally named "default", or when the
                        // resolved target is the file's default export — both
                        // forms are reachable from any default-import binding.
                        if clause.name.is_some()
                            && (target_name == "default" || target_is_default_export)
                            && let Some(name_node) = arena.get(clause.name)
                            && let Some(ident) = arena.get_identifier(name_node)
                        {
                            local_bindings.push((clause.name, ident.escaped_text.clone()));
                        }
                        // Namespace import (`import * as ns from "./a"`):
                        // record the local namespace identifier so the
                        // caller scan can match `<ns>.<target_name>()`
                        // member calls. NamespaceImport reuses
                        // `NamedImportsData` storage with the local name
                        // in the `name` field and an empty elements list.
                        if clause.named_bindings.is_some()
                            && let Some(nb_node) = arena.get(clause.named_bindings)
                            && nb_node.kind == tsz_parser::syntax_kind_ext::NAMESPACE_IMPORT
                            && let Some(ns_import) = arena.get_named_imports(nb_node)
                            && ns_import.name.is_some()
                            && let Some(name_node) = arena.get(ns_import.name)
                            && arena.get_identifier(name_node).is_some()
                        {
                            namespace_bindings.push(ns_import.name);
                        }
                        // Named bindings — walk the named_bindings child for
                        // `NamedImports` (`import { foo } from "./a"`).
                        if clause.named_bindings.is_some()
                            && let Some(nb_node) = arena.get(clause.named_bindings)
                            && nb_node.kind == tsz_parser::syntax_kind_ext::NAMED_IMPORTS
                            && let Some(named) = arena.get_named_imports(nb_node)
                        {
                            {
                                for &spec_idx in &named.elements.nodes {
                                    let Some(specifier_node) = arena.get(spec_idx) else {
                                        continue;
                                    };
                                    let Some(specifier) = arena.get_specifier(specifier_node)
                                    else {
                                        continue;
                                    };
                                    // Matched exported name (property_name when aliased,
                                    // otherwise the binding name).
                                    let exported = if specifier.property_name.is_some()
                                        && let Some(prop_node) = arena.get(specifier.property_name)
                                        && let Some(ident) = arena.get_identifier(prop_node)
                                    {
                                        ident.escaped_text.clone()
                                    } else if let Some(name_node) = arena.get(specifier.name)
                                        && let Some(ident) = arena.get_identifier(name_node)
                                    {
                                        ident.escaped_text.clone()
                                    } else {
                                        continue;
                                    };
                                    let exported_matches = exported == target_name
                                        || (target_is_default_export && exported == "default");
                                    if !exported_matches {
                                        continue;
                                    }
                                    let Some(name_node) = arena.get(specifier.name) else {
                                        continue;
                                    };
                                    let local = arena
                                        .get_identifier(name_node)
                                        .map(|i| i.escaped_text.clone())
                                        .unwrap_or_else(|| target_name.clone());
                                    local_bindings.push((specifier.name, local));
                                }
                            }
                        }
                    }
                }
            }

            let _ = root;
            for (decl_idx, local_name) in local_bindings {
                let calls = provider.incoming_calls_for_decl_in_file(decl_idx, &local_name);
                for call in calls {
                    results.push(call);
                }
            }
            for ns_decl_idx in namespace_bindings {
                let calls = provider.incoming_calls_for_namespace_member(ns_decl_idx, &target_name);
                for call in calls {
                    results.push(call);
                }
            }
        }

        results
    }

    /// Best-effort canonical form for path comparison: prefer
    /// `std::fs::canonicalize`, fall back to the raw normalized string.
    fn canonicalize_path_str(path: &str) -> String {
        let p = std::path::Path::new(path);
        if let Ok(canon) = std::fs::canonicalize(p) {
            return canon.to_string_lossy().into_owned();
        }
        Self::normalize_path(p).to_string_lossy().into_owned()
    }

    /// Strip `.` segments and resolve `..` segments from a path while
    /// preserving the root. Used to normalize the result of
    /// `Path::join("/foo", "./bar")` (which yields `/foo/./bar`) into the
    /// canonical `/foo/bar` so it matches `open_files` keys and `exists()`.
    fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
        use std::path::Component;
        let mut out = std::path::PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        out.push("..");
                    }
                }
                other => out.push(other.as_os_str()),
            }
        }
        if out.as_os_str().is_empty() {
            out.push(".");
        }
        out
    }

    /// Resolve a relative module specifier (e.g. `"./a"`, `"../foo/bar"`)
    /// against the importing file's directory. Tries `.ts`, `.tsx`,
    /// `.d.ts`, `.js`, `.jsx`, `.mts`, `.cts`, then bare path. Returns the
    /// first candidate that exists on disk or matches a key in the
    /// `open_files` map (so unsaved buffers count for resolution).
    fn resolve_relative_module_specifier(
        &self,
        importer_dir: &std::path::Path,
        specifier: &str,
    ) -> Option<std::path::PathBuf> {
        let base = Self::normalize_path(&importer_dir.join(specifier));
        const EXTS: &[&str] = tsz_common::file_extensions::TSC_TS_JS_RESOLUTION_EXTENSIONS_BARE;
        let exists_anywhere = |p: &std::path::Path| -> bool {
            if p.exists() {
                return true;
            }
            let key = p.to_string_lossy().into_owned();
            self.open_files.contains_key(&key)
        };
        if base.extension().is_some() && exists_anywhere(&base) {
            return Some(base);
        }
        for ext in EXTS {
            let candidate = base.with_extension(ext);
            if exists_anywhere(&candidate) {
                return Some(candidate);
            }
        }
        if base.is_dir() {
            for ext in EXTS {
                let candidate = base.join(format!("index.{ext}"));
                if exists_anywhere(&candidate) {
                    return Some(candidate);
                }
            }
        }
        Some(base)
    }

    pub(crate) fn handle_call_hierarchy(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                CallHierarchyProvider::new(&arena, &binder, &line_map, file.clone(), &source_text);

            let is_incoming = request.command == "provideCallHierarchyIncomingCalls";
            // TypeScript treats absolute position 0 as a source-file call hierarchy query.
            // In tsserver protocol this is line:1/offset:1, and should not probe into
            // adjacent offsets to resolve the first identifier token.
            let is_file_start_query = line == 1 && offset == 1;
            let positions = if is_file_start_query {
                vec![position]
            } else {
                Self::call_hierarchy_probe_positions(&line_map, &source_text, position)
            };

            if is_incoming {
                if is_file_start_query {
                    return Some(serde_json::json!([]));
                }
                let mut calls = Vec::new();
                for probe in &positions {
                    calls = provider.incoming_calls(root, *probe);
                    if !calls.is_empty() {
                        break;
                    }
                }

                // Issue #3753: scan the other open files for cross-file
                // callers that reach this target via an `import` binding.
                // tsc reports those as incoming calls from the importing
                // file; tsz only saw within-file callers before.
                let prepared_target = positions
                    .iter()
                    .find_map(|probe| provider.prepare(root, *probe));
                if let Some(target_item) = prepared_target {
                    let cross_calls = self.collect_cross_file_incoming_calls(&file, &target_item);
                    for call in cross_calls {
                        // Avoid duplicates if the caller's local resolution already
                        // produced an entry pointing at the same span.
                        let already_present = calls.iter().any(|existing| {
                            existing.from.uri == call.from.uri
                                && existing.from.selection_range == call.from.selection_range
                        });
                        if !already_present {
                            calls.push(call);
                        }
                    }
                }
                let body: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|call| {
                        let raw_kind = format!("{:?}", call.from.kind).to_lowercase();
                        let (name, kind) =
                            Self::tsserver_call_hierarchy_name_kind(&call.from.name, &raw_kind);
                        let from_ranges: Vec<serde_json::Value> = call
                            .from_ranges
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(r.start),
                                    "end": Self::lsp_to_tsserver_position(r.end),
                                })
                            })
                            .collect();
                        let mut from = serde_json::json!({
                            "from": {
                                "name": name,
                                "kind": kind,
                                "file": call.from.uri,
                                "span": {
                                    "start": Self::lsp_to_tsserver_position(call.from.range.start),
                                    "end": Self::lsp_to_tsserver_position(call.from.range.end),
                                },
                                "selectionSpan": {
                                    "start": Self::lsp_to_tsserver_position(call.from.selection_range.start),
                                    "end": Self::lsp_to_tsserver_position(call.from.selection_range.end),
                                },
                            },
                            "fromSpans": from_ranges,
                        });
                        if let Some(container_name) = &call.from.container_name {
                            from["from"]["containerName"] = serde_json::json!(container_name);
                        }
                        from
                    })
                    .collect();
                Some(serde_json::json!(body))
            } else {
                if is_file_start_query {
                    return Some(serde_json::json!([]));
                }
                // Prefer exact-position outgoing calls; if the cursor sits on a
                // token boundary where prepare fails, probe adjacent offsets to
                // recover the same behavior used by prepare/incoming handlers.
                let mut calls = provider.outgoing_calls(root, position);
                if calls.is_empty() && provider.prepare(root, position).is_none() {
                    for probe in positions.iter().skip(1) {
                        if provider.prepare(root, *probe).is_some() {
                            calls = provider.outgoing_calls(root, *probe);
                            break;
                        }
                    }
                }
                // Issue #3753: when an outgoing callee resolves to an `import`
                // binding, follow it across to the imported module's source
                // file and replace `to` with the actual export's location.
                // tsc points at the exported declaration, not at the
                // local import binding.
                let mut resolved_calls: Vec<tsz_lsp::CallHierarchyOutgoingCall> =
                    Vec::with_capacity(calls.len());
                for call in calls {
                    if let Some(import_req) = call.import_resolution.clone()
                        && let Some(resolved_to) =
                            self.resolve_import_call_hierarchy_target(&file, &import_req)
                    {
                        resolved_calls.push(CallHierarchyOutgoingCall {
                            to: resolved_to,
                            from_ranges: call.from_ranges,
                            import_resolution: None,
                        });
                    } else {
                        resolved_calls.push(call);
                    }
                }
                let calls = resolved_calls;
                let body: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|call| {
                        let raw_kind = format!("{:?}", call.to.kind).to_lowercase();
                        let (name, kind) =
                            Self::tsserver_call_hierarchy_name_kind(&call.to.name, &raw_kind);
                        let from_ranges: Vec<serde_json::Value> = call
                            .from_ranges
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(r.start),
                                    "end": Self::lsp_to_tsserver_position(r.end),
                                })
                            })
                            .collect();
                        let mut to = serde_json::json!({
                            "to": {
                                "name": name,
                                "kind": kind,
                                "file": call.to.uri,
                                "span": {
                                    "start": Self::lsp_to_tsserver_position(call.to.range.start),
                                    "end": Self::lsp_to_tsserver_position(call.to.range.end),
                                },
                                "selectionSpan": {
                                    "start": Self::lsp_to_tsserver_position(call.to.selection_range.start),
                                    "end": Self::lsp_to_tsserver_position(call.to.selection_range.end),
                                },
                            },
                            "fromSpans": from_ranges,
                        });
                        if let Some(container_name) = &call.to.container_name {
                            to["to"]["containerName"] = serde_json::json!(container_name);
                        }
                        to
                    })
                    .collect();
                Some(serde_json::json!(body))
            }
        })();
        self.success_or_empty_array(seq, request, result)
    }

    /// `configurePlugin` — stores plugin configuration for future use.
    pub(crate) fn handle_configure_plugin(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        if let Some(plugin_name) = request.arguments.get("pluginName").and_then(|v| v.as_str()) {
            let config = request
                .arguments
                .get("configuration")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            self.plugin_configs.insert(plugin_name.to_string(), config);
        }
        self.success_response(seq, request, None)
    }

    /// `getMoveToRefactoringFileSuggestions` — suggests files a symbol can be moved to.
    pub(crate) fn handle_get_move_to_refactoring_file_suggestions(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let file_path = std::path::Path::new(file);
            let file_ext = file_path.extension()?.to_str()?;

            // Determine which extensions are compatible
            let compatible_exts: &[&str] = match file_ext {
                "ts" => &["ts"],
                "tsx" => &["tsx", "ts"],
                "js" => &["js"],
                "jsx" => &["jsx", "js"],
                "mts" => &["mts", "ts"],
                "cts" => &["cts", "ts"],
                "mjs" => &["mjs", "js"],
                "cjs" => &["cjs", "js"],
                _ => &[file_ext],
            };

            let push_candidate = |files: &mut Vec<String>,
                                  candidate: &str,
                                  compatible_exts: &[&str]| {
                if candidate == file || files.iter().any(|p| p == candidate) {
                    return;
                }
                if candidate.ends_with(".d.ts")
                    || candidate.ends_with(".d.mts")
                    || candidate.ends_with(".d.cts")
                {
                    return;
                }
                if candidate.contains("/node_modules/") || candidate.contains("\\node_modules\\") {
                    return;
                }
                if let Some(ext) = std::path::Path::new(candidate)
                    .extension()
                    .and_then(|e| e.to_str())
                    && compatible_exts.contains(&ext)
                {
                    files.push(candidate.to_string());
                }
            };

            // Collect candidate files from open files, the configured tsconfig
            // project (issue #3798), and external project lists.
            let mut files: Vec<String> = Vec::new();
            for open_path in self.open_files.keys() {
                push_candidate(&mut files, open_path, compatible_exts);
            }
            // Issue #3798: include files from the owning tsconfig project, not
            // just open files. tsc's language service ranges over the whole
            // project file set when ranking move-to-file targets.
            if let Some(project) = self.compile_on_save_project(file) {
                for pf in &project.file_names {
                    push_candidate(&mut files, pf, compatible_exts);
                }
            }
            for project_files in self.external_project_files.values() {
                for pf in project_files {
                    push_candidate(&mut files, pf, compatible_exts);
                }
            }

            files.sort();

            // Issue #3798: derive the suggested new-file name from the
            // declaration's identifier in the requested range, falling back
            // to "newFile" when no identifier is found. tsc names the
            // suggestion after the moved symbol (e.g. "moveMe.ts").
            let parent = file_path.parent().unwrap_or(std::path::Path::new(""));
            let symbol_stub = Self::move_to_file_symbol_name(self, request)
                .unwrap_or_else(|| "newFile".to_string());
            let new_file_name = parent.join(format!("{symbol_stub}.{file_ext}"));

            Some(serde_json::json!({
                "newFileName": new_file_name.to_string_lossy(),
                "files": files
            }))
        })();

        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"newFileName": "", "files": []}))),
        )
    }

    /// Extract the leading declaration identifier inside the request's
    /// range. Used by `move-to-file` suggestions to name the new file
    /// after the moved symbol (issue #3798). Falls back to None when no
    /// matching declaration is found in the source slice.
    fn move_to_file_symbol_name(&self, request: &TsServerRequest) -> Option<String> {
        let file = request.arguments.get("file")?.as_str()?;
        let source_text = self.open_files.get(file)?;
        let line_map = LineMap::build(source_text);
        let start_line = request.arguments.get("startLine")?.as_u64()? as u32;
        let start_offset = request.arguments.get("startOffset")?.as_u64()? as u32;
        let end_line = request.arguments.get("endLine")?.as_u64()? as u32;
        let end_offset = request.arguments.get("endOffset")?.as_u64()? as u32;
        let start_pos = Self::tsserver_to_lsp_position(start_line, start_offset);
        let end_pos = Self::tsserver_to_lsp_position(end_line, end_offset);
        let start_byte = line_map.position_to_offset(start_pos, source_text)? as usize;
        let end_byte = line_map.position_to_offset(end_pos, source_text)? as usize;
        let slice = source_text.get(start_byte..end_byte.min(source_text.len()))?;
        // Look for the leading exported declaration's name. The text-search
        // approximation matches tsc's typical move-to-file behavior for the
        // common cases (function/class/const/let/var/interface/type/enum).
        let after_export = slice
            .trim_start()
            .strip_prefix("export ")
            .map_or(slice.trim_start(), str::trim_start);
        for keyword in [
            "function ",
            "class ",
            "interface ",
            "enum ",
            "type ",
            "const ",
            "let ",
            "var ",
            "namespace ",
            "module ",
            "abstract class ",
            "default function ",
            "default class ",
        ] {
            if let Some(rest) = after_export.strip_prefix(keyword) {
                let name: String = rest
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
        None
    }

    /// `preparePasteEdits` — checks whether paste-with-imports is available.
    ///
    /// Returns `true` if the pasted content comes from a known source file
    /// (indicating we can potentially add imports).
    pub(crate) fn handle_prepare_paste_edits(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<bool> {
            if let Some(copied_text_span) = request
                .arguments
                .get("copiedTextSpan")
                .and_then(|value| value.as_array())
            {
                let file = request
                    .arguments
                    .get("file")
                    .and_then(|value| value.as_str())?;
                let has_source =
                    self.open_files.contains_key(file) || std::path::Path::new(file).exists();
                let has_non_empty_span = copied_text_span.iter().any(|span| {
                    span.get("length")
                        .and_then(|value| value.as_u64())
                        .is_some_and(|length| length > 0)
                });
                return Some(has_source && has_non_empty_span);
            }

            let copied_from = request
                .arguments
                .get("copiedFromFile")
                .and_then(|v| v.as_str())?;
            if self.open_files.contains_key(copied_from)
                || std::path::Path::new(copied_from).exists()
            {
                return Some(true);
            }
            Some(false)
        })();

        self.success_response(
            seq,
            request,
            Some(serde_json::json!(result.unwrap_or(false))),
        )
    }

    /// `getPasteEdits` — generates import additions for pasted code.
    ///
    /// Parses the pasted text, identifies unresolved identifiers, and generates
    /// import statements from the source file's exports.
    pub(crate) fn handle_get_paste_edits(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let target_file = request.arguments.get("file")?.as_str()?;
            let pasted_text = request
                .arguments
                .get("pastedText")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    let texts = arr
                        .iter()
                        .filter_map(|value| value.as_str())
                        .collect::<Vec<_>>();
                    (!texts.is_empty()).then_some(texts)
                })?;
            let pasted_text_joined = pasted_text.join("\n");
            let paste_locations = request
                .arguments
                .get("pasteLocations")
                .and_then(|value| value.as_array());

            let copied_from = request
                .arguments
                .get("copiedFrom")
                .and_then(|v| v.get("file"))
                .and_then(|v| v.as_str())?;

            // Extract import lines from source file that the pasted code may reference
            let source_content = self
                .open_files
                .get(copied_from)
                .cloned()
                .or_else(|| std::fs::read_to_string(copied_from).ok())?;

            // Find export names from the source file
            let source_exports: Vec<String> = source_content
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("export ") {
                        // Extract exported identifier names
                        let rest = trimmed.strip_prefix("export ")?;
                        // "export function foo" / "export class Foo" / "export const bar"
                        for keyword in &[
                            "function ",
                            "class ",
                            "const ",
                            "let ",
                            "var ",
                            "enum ",
                            "interface ",
                            "type ",
                            "abstract class ",
                            "async function ",
                        ] {
                            if let Some(after) = rest.strip_prefix(keyword) {
                                let name: String = after
                                    .chars()
                                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                                    .collect();
                                if !name.is_empty() {
                                    return Some(name);
                                }
                            }
                        }
                    }
                    None
                })
                .collect();

            if source_exports.is_empty() {
                return None;
            }

            // Check which source exports appear in pasted text but not in target file
            let target_content = self
                .open_files
                .get(target_file)
                .cloned()
                .or_else(|| std::fs::read_to_string(target_file).ok())?;

            let mut names_to_import: Vec<String> = Vec::new();
            for export_name in &source_exports {
                // Check if the export name appears in pasted code
                if !pasted_text_joined.contains(export_name.as_str()) {
                    continue;
                }
                // Check if the target already imports/declares it
                let already_exists = target_content.lines().any(|line| {
                    let t = line.trim();
                    t.contains(export_name.as_str())
                        && (t.starts_with("import ")
                            || t.starts_with("const ")
                            || t.starts_with("let ")
                            || t.starts_with("var ")
                            || t.starts_with("function ")
                            || t.starts_with("class ")
                            || t.starts_with("interface ")
                            || t.starts_with("type ")
                            || t.starts_with("enum "))
                });
                if !already_exists {
                    names_to_import.push(export_name.clone());
                }
            }

            if names_to_import.is_empty() {
                return None;
            }

            names_to_import.sort();
            names_to_import.dedup();

            // Compute relative import path from target to source
            let target_dir = std::path::Path::new(target_file)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            let import_path =
                Self::compute_relative_import(target_dir, std::path::Path::new(copied_from));

            // Find insertion point: after last import line, or at top of file
            let mut insert_line = 0u32;
            for (i, line) in target_content.lines().enumerate() {
                let t = line.trim();
                if t.starts_with("import ") || t.starts_with("import{") {
                    insert_line = i as u32 + 1;
                }
            }

            // Build import statement
            let import_suffix = if insert_line == 0 { "\n\n" } else { "\n" };
            let import_text = format!(
                "import {{ {} }} from \"{}\";{}",
                names_to_import.join(", "),
                import_path,
                import_suffix
            );
            let line_map = LineMap::build(&target_content);
            let import_offset = line_map.position_to_offset(
                Position {
                    line: insert_line,
                    character: 0,
                },
                &target_content,
            )?;
            let mut text_changes = vec![serde_json::json!({
                "span": { "start": import_offset, "length": 0 },
                "newText": import_text
            })];

            if let Some(paste_locations) = paste_locations {
                for (index, location) in paste_locations.iter().enumerate() {
                    let start = location.get("start")?;
                    let start_line = start.get("line")?.as_u64()? as u32;
                    let start_offset = start.get("offset")?.as_u64()? as u32;
                    let start_pos = Position {
                        line: start_line.saturating_sub(1),
                        character: start_offset.saturating_sub(1),
                    };
                    let start_offset = line_map.position_to_offset(start_pos, &target_content)?;
                    let new_text = pasted_text
                        .get(index)
                        .or_else(|| pasted_text.first())
                        .copied()
                        .unwrap_or("");
                    text_changes.push(serde_json::json!({
                        "span": { "start": start_offset, "length": 0 },
                        "newText": new_text
                    }));
                }
            }

            Some(serde_json::json!({
                "edits": [{
                    "fileName": target_file,
                    "textChanges": text_changes
                }],
                "fixId": "providePostPasteEdits"
            }))
        })();

        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"edits": [], "fixId": ""}))),
        )
    }

    /// `mapCode` — maps code snippets to insertion locations in a file.
    ///
    /// Parses code snippets and finds appropriate insertion points based on
    /// the AST structure and optional focus locations.
    pub(crate) fn handle_map_code(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let mapping = request.arguments.get("mapping")?;
            let contents = mapping.get("contents")?.as_array()?;

            if contents.is_empty() {
                return None;
            }

            let file_content = self
                .open_files
                .get(file)
                .cloned()
                .or_else(|| std::fs::read_to_string(file).ok())?;

            // Determine insertion point from focus locations if provided
            let insert_line = if let Some(focus) = mapping
                .get("focusLocations")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.last())
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.last())
            {
                // Focus location gives us a span — insert after it
                focus
                    .get("end")
                    .and_then(|e| e.get("line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0) as u32
            } else {
                // Default: insert at end of file
                file_content.lines().count() as u32
            };

            let mut text_changes = Vec::new();
            for content_val in contents {
                if let Some(snippet) = content_val.as_str() {
                    if snippet.trim().is_empty() {
                        continue;
                    }
                    text_changes.push(serde_json::json!({
                        "start": { "line": insert_line + 1, "offset": 1 },
                        "end": { "line": insert_line + 1, "offset": 1 },
                        "newText": format!("{snippet}\n")
                    }));
                }
            }

            if text_changes.is_empty() {
                return None;
            }

            Some(serde_json::json!([{
                "fileName": file,
                "textChanges": text_changes
            }]))
        })();

        self.success_or_empty_array(seq, request, result)
    }

    pub(crate) fn handle_outlining_spans(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = FoldingRangeProvider::new(&arena, &line_map, &source_text);
            let ranges = provider.get_folding_ranges(root);

            let body: Vec<serde_json::Value> = ranges
                .iter()
                .map(|fr| {
                    // Convert byte offsets to precise line/offset positions
                    let start_pos = line_map.offset_to_position(fr.start_offset, &source_text);
                    let end_pos = line_map.offset_to_position(fr.end_offset, &source_text);
                    let hint_end_pos = line_map
                        .offset_to_position(fr.end_offset.min(fr.start_offset + 200), &source_text);

                    let mut span = serde_json::json!({
                        "textSpan": {
                            "start": Self::lsp_to_tsserver_position(start_pos),
                            "end": Self::lsp_to_tsserver_position(end_pos),
                        },
                        "hintSpan": {
                            "start": Self::lsp_to_tsserver_position(start_pos),
                            "end": Self::lsp_to_tsserver_position(
                                if hint_end_pos.line == start_pos.line {
                                    hint_end_pos
                                } else {
                                    end_pos
                                }
                            ),
                        },
                        "bannerText": "...",
                        "autoCollapse": false,
                    });
                    span["kind"] = serde_json::json!(fr.kind.as_deref().unwrap_or("code"));
                    span
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.success_or_empty_array(seq, request, result)
    }

    pub(crate) fn handle_brace(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, _binder, _root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let byte_offset = line_map.position_to_offset(position, &source_text)? as usize;

            let bytes = source_text.as_bytes();
            if byte_offset >= bytes.len() {
                return Some(serde_json::json!([]));
            }

            let ch = bytes[byte_offset];

            // Build a map of which positions are "in code" (not inside strings/comments)
            let code_map = super::build_code_map(bytes);

            if !code_map[byte_offset] {
                return Some(serde_json::json!([]));
            }

            let match_pos = match ch {
                b'{' => super::scan_forward(bytes, &code_map, byte_offset, b'{', b'}'),
                b'(' => super::scan_forward(bytes, &code_map, byte_offset, b'(', b')'),
                b'[' => super::scan_forward(bytes, &code_map, byte_offset, b'[', b']'),
                b'}' => super::scan_backward(bytes, &code_map, byte_offset, b'}', b'{'),
                b')' => super::scan_backward(bytes, &code_map, byte_offset, b')', b'('),
                b']' => super::scan_backward(bytes, &code_map, byte_offset, b']', b'['),
                b'<' | b'>' => {
                    // For angle brackets, use AST-based matching (not text scanning)
                    // because < and > are also comparison operators
                    super::find_angle_bracket_match(&arena, &source_text, byte_offset)
                }
                _ => None,
            };

            if let Some(match_offset) = match_pos {
                let pos1 = line_map.offset_to_position(byte_offset as u32, &source_text);
                let pos1_end = line_map.offset_to_position((byte_offset + 1) as u32, &source_text);
                let pos2 = line_map.offset_to_position(match_offset as u32, &source_text);
                let pos2_end = line_map.offset_to_position((match_offset + 1) as u32, &source_text);

                let span1 = serde_json::json!({
                    "start": {"line": pos1.line + 1, "offset": pos1.character + 1},
                    "end": {"line": pos1_end.line + 1, "offset": pos1_end.character + 1}
                });
                let span2 = serde_json::json!({
                    "start": {"line": pos2.line + 1, "offset": pos2.character + 1},
                    "end": {"line": pos2_end.line + 1, "offset": pos2_end.character + 1}
                });

                // Return sorted by position
                if byte_offset < match_offset {
                    Some(serde_json::json!([span1, span2]))
                } else {
                    Some(serde_json::json!([span2, span1]))
                }
            } else {
                Some(serde_json::json!([]))
            }
        })();
        self.success_or_empty_array(seq, request, result)
    }
}
