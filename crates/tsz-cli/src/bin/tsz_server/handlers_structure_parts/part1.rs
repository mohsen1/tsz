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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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

    pub(crate) fn handle_get_applicable_refactors(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            // Issue #3718: tsserver accepts FileLocationOrRangeRequestArgs.
            // The position-only form sends `{ line, offset }` and the range
            // form sends `{ startLine, startOffset, endLine, endOffset }`.
            // Treat a position as a zero-length range that anchors both
            // ends at the same coordinate.
            let (start_line, start_offset, end_line, end_offset) =
                Self::parse_refactor_request_range(request)?;

            let (arena, binder, root, content) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&content);

            let range = Range {
                start: Position {
                    line: start_line.saturating_sub(1),
                    character: start_offset.saturating_sub(1),
                },
                end: Position {
                    line: end_line.saturating_sub(1),
                    character: end_offset.saturating_sub(1),
                },
            };

            let provider =
                CodeActionProvider::new(&arena, &binder, &line_map, file.to_string(), &content);

            let mut refactors = Vec::new();

            // Check if extract variable is applicable
            if provider.extract_variable(root, range).is_some() {
                // Issue #3803: tsc emits one extract action per *applicable*
                // scope and attaches a range. Approximate "applicable scopes"
                // by detecting whether the request's expression has an
                // enclosing function in its ancestor chain.
                let action_range = serde_json::json!({
                    "start": { "line": start_line, "offset": start_offset },
                    "end": { "line": end_line, "offset": end_offset },
                });
                let inside_function =
                    Self::request_is_inside_function(&arena, &line_map, &content, range);
                let function_actions: Vec<serde_json::Value> = if inside_function {
                    vec![
                        serde_json::json!({
                            "name": "function_scope_0",
                            "description": "Extract to function in enclosing scope",
                            "kind": "refactor.extract.function",
                            "range": action_range,
                        }),
                        serde_json::json!({
                            "name": "function_scope_1",
                            "description": "Extract to function in global scope",
                            "kind": "refactor.extract.function",
                            "range": action_range,
                        }),
                    ]
                } else {
                    vec![serde_json::json!({
                        "name": "function_scope_0",
                        "description": "Extract to function in global scope",
                        "kind": "refactor.extract.function",
                        "range": action_range,
                    })]
                };
                let constant_actions: Vec<serde_json::Value> = if inside_function {
                    vec![
                        serde_json::json!({
                            "name": "constant_scope_0",
                            "description": "Extract to constant in enclosing scope",
                            "kind": "refactor.extract.constant",
                            "range": action_range,
                        }),
                        serde_json::json!({
                            "name": "constant_scope_1",
                            "description": "Extract to constant in global scope",
                            "kind": "refactor.extract.constant",
                            "range": action_range,
                        }),
                    ]
                } else {
                    vec![serde_json::json!({
                        "name": "constant_scope_0",
                        "description": "Extract to constant in enclosing scope",
                        "kind": "refactor.extract.constant",
                        "range": action_range,
                    })]
                };
                refactors.push(serde_json::json!({
                    "name": "Extract Symbol",
                    "description": "Extract function",
                    "actions": function_actions,
                }));
                refactors.push(serde_json::json!({
                    "name": "Extract Symbol",
                    "description": "Extract constant",
                    "actions": constant_actions,
                }));
            }

            Some(serde_json::json!(refactors))
        })();

        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    /// Parse the request's range fields, falling back to a position
    /// (`line`/`offset`) when the explicit range fields are absent. tsserver
    /// accepts `FileLocationOrRangeRequestArgs` for refactor commands; a
    /// position is treated as a zero-length range. Issue #3718.
    pub(super) fn parse_refactor_request_range(
        request: &TsServerRequest,
    ) -> Option<(u32, u32, u32, u32)> {
        let line_only = request
            .arguments
            .get("line")
            .and_then(serde_json::Value::as_u64)
            .map(|line| line as u32);
        let offset_only = request
            .arguments
            .get("offset")
            .and_then(serde_json::Value::as_u64)
            .map(|offset| offset as u32);

        let pick = |range_key: &str, position: Option<u32>| -> Option<u32> {
            request
                .arguments
                .get(range_key)
                .and_then(serde_json::Value::as_u64)
                .map(|n| n as u32)
                .or(position)
        };

        let start_line = pick("startLine", line_only)?;
        let start_offset = pick("startOffset", offset_only)?;
        let end_line = pick("endLine", line_only)?;
        let end_offset = pick("endOffset", offset_only)?;
        Some((start_line, start_offset, end_line, end_offset))
    }

    /// Walk the AST upward from the request range looking for an
    /// enclosing function-like node (function/method/arrow/constructor/
    /// accessor). Returns `true` when one is found, `false` when the
    /// request range is at module level. Used by
    /// `handle_get_applicable_refactors` to decide which extract scopes
    /// to advertise. Issue #3803.
    fn request_is_inside_function(
        arena: &tsz::parser::node::NodeArena,
        line_map: &LineMap,
        source_text: &str,
        range: Range,
    ) -> bool {
        let Some(start_offset) = line_map.position_to_offset(range.start, source_text) else {
            return false;
        };
        let mut current = tsz::lsp::utils::find_node_at_offset(arena, start_offset);
        while current.is_some() {
            let Some(node) = arena.get(current) else {
                return false;
            };
            if node.is_function_like() {
                return true;
            }
            let Some(ext) = arena.get_extended(current) else {
                return false;
            };
            current = ext.parent;
        }
        false
    }

    pub(crate) fn handle_get_edits_for_refactor(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let refactor = request.arguments.get("refactor")?.as_str()?;
            // Issue #3718: accept either the range form (startLine etc.) or
            // a position-only form ({ line, offset }) per
            // FileLocationOrRangeRequestArgs.
            let (start_line, start_offset, end_line, end_offset) =
                Self::parse_refactor_request_range(request)?;

            let (arena, binder, root, content) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&content);

            let range = Range {
                start: Position {
                    line: start_line.saturating_sub(1),
                    character: start_offset.saturating_sub(1),
                },
                end: Position {
                    line: end_line.saturating_sub(1),
                    character: end_offset.saturating_sub(1),
                },
            };

            let provider =
                CodeActionProvider::new(&arena, &binder, &line_map, file.to_string(), &content);

            if refactor == "Extract Symbol" {
                let action = provider.extract_variable(root, range)?;
                let edit = action.edit?;
                let mut file_edits = Vec::new();
                for (fname, edits) in edit.changes {
                    let mut text_changes = Vec::new();
                    for e in edits {
                        text_changes.push(serde_json::json!({
                            "start": {
                                "line": e.range.start.line + 1,
                                "offset": e.range.start.character + 1
                            },
                            "end": {
                                "line": e.range.end.line + 1,
                                "offset": e.range.end.character + 1
                            },
                            "newText": e.new_text
                        }));
                    }
                    file_edits.push(serde_json::json!({
                        "fileName": fname,
                        "textChanges": text_changes
                    }));
                }
                return Some(serde_json::json!({ "edits": file_edits }));
            }

            None
        })();

        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"edits": []}))),
        )
    }

    pub(crate) fn handle_organize_imports(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request
                .arguments
                .get("scope")
                .and_then(|s| s.get("args"))
                .and_then(|a| a.get("file"))
                .and_then(|v| v.as_str())
                .or_else(|| request.arguments.get("file").and_then(|v| v.as_str()))?;

            let (arena, binder, root, content) = self.parse_and_bind_file(file)?;

            let parse_organize_imports_ignore_case = |value: &serde_json::Value| {
                value
                    .as_bool()
                    .or_else(|| value.as_str().and_then(|s| (s == "auto").then_some(true)))
            };
            let organize_imports_ignore_case = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("organizeImportsIgnoreCase"))
                .and_then(parse_organize_imports_ignore_case)
                .or_else(|| {
                    request
                        .arguments
                        .get("organizeImportsIgnoreCase")
                        .and_then(parse_organize_imports_ignore_case)
                })
                .unwrap_or(self.organize_imports_ignore_case);
            let organize_imports_type_order = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("organizeImportsTypeOrder"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    request
                        .arguments
                        .get("organizeImportsTypeOrder")
                        .and_then(serde_json::Value::as_str)
                })
                .map(ToOwned::to_owned)
                .or_else(|| self.organize_imports_type_order.clone());

            let line_map = LineMap::build(&content);
            let provider =
                CodeActionProvider::new(&arena, &binder, &line_map, file.to_string(), &content)
                    .with_organize_imports_ignore_case(organize_imports_ignore_case)
                    .with_organize_imports_type_order(organize_imports_type_order);

            let action = provider.organize_imports(root)?;

            let mut text_changes = Vec::new();
            if let Some(edit) = action.edit {
                for (_fname, edits) in edit.changes {
                    for e in edits {
                        text_changes.push(serde_json::json!({
                            "start": {
                                "line": e.range.start.line + 1,
                                "offset": e.range.start.character + 1
                            },
                            "end": {
                                "line": e.range.end.line + 1,
                                "offset": e.range.end.character + 1
                            },
                            "newText": e.new_text
                        }));
                    }
                }
            }

            Some(serde_json::json!([{
                "fileName": file,
                "textChanges": text_changes
            }]))
        })();

        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_get_edits_for_file_rename(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let old_file = request.arguments.get("oldFilePath")?.as_str()?;
            let new_file = request.arguments.get("newFilePath")?.as_str()?;

            let old_path = std::path::Path::new(old_file);
            let new_path = std::path::Path::new(new_file);

            let mut file_changes: Vec<serde_json::Value> = Vec::new();

            // Scan all open files for imports that reference the renamed file
            let open_files: Vec<(String, String)> = self
                .open_files
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            for (dep_file, source_text) in &open_files {
                let (arena, _binder, root, _) = self.parse_and_bind_file(dep_file)?;
                let line_map = LineMap::build(source_text);
                let provider = FileRenameProvider::new(&arena, &line_map, source_text);
                let imports = provider.find_import_specifier_nodes(root);

                let dep_dir = std::path::Path::new(dep_file.as_str()).parent()?;
                let mut text_changes: Vec<serde_json::Value> = Vec::new();

                for import in &imports {
                    // Check if this import points to the old file
                    let spec = &import.current_specifier;
                    if !spec.starts_with('.') {
                        continue; // Only relative imports
                    }
                    let resolved = dep_dir.join(spec);
                    let resolved_normalized = Self::normalize_module_path(&resolved);
                    let old_normalized = Self::normalize_module_path(old_path);

                    if resolved_normalized != old_normalized {
                        continue;
                    }

                    // Compute new relative path
                    let new_rel = Self::compute_relative_import(dep_dir, new_path);
                    let quote_char = source_text
                        .get(import.range.start.character as usize..)
                        .and_then(|s| s.chars().next())
                        .unwrap_or('"');

                    text_changes.push(serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(import.range.start),
                        "end": Self::lsp_to_tsserver_position(import.range.end),
                        "newText": format!("{quote_char}{new_rel}{quote_char}"),
                    }));
                }

                if !text_changes.is_empty() {
                    file_changes.push(serde_json::json!({
                        "fileName": dep_file,
                        "textChanges": text_changes,
                    }));
                }
            }

            Some(serde_json::json!(file_changes))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn normalize_module_path(path: &std::path::Path) -> String {
        let normalized = Self::normalize_path_string(path);
        let s = normalized.as_str();
        let s = s
            .strip_suffix(".ts")
            .or_else(|| s.strip_suffix(".tsx"))
            .or_else(|| s.strip_suffix(".js"))
            .or_else(|| s.strip_suffix(".jsx"))
            .unwrap_or(s);
        s.to_string()
    }

    fn compute_relative_import(from_dir: &std::path::Path, to_file: &std::path::Path) -> String {
        let to_stem = to_file.with_extension("");

        // Compute relative path components
        let from_parts: Vec<_> = from_dir.components().collect();
        let to_parts: Vec<_> = to_stem.components().collect();

        let mut common = 0;
        while common < from_parts.len().min(to_parts.len())
            && from_parts[common] == to_parts[common]
        {
            common += 1;
        }

        let ups = from_parts.len() - common;
        let mut parts: Vec<String> = Vec::new();
        for _ in 0..ups {
            parts.push("..".to_string());
        }
        for &comp in &to_parts[common..] {
            parts.push(comp.as_os_str().to_string_lossy().to_string());
        }

        let rel = parts.join("/");
        if rel.starts_with('.') {
            rel
        } else {
            format!("./{rel}")
        }
    }

    pub(crate) fn handle_format(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self
                .open_files
                .get(file)
                .cloned()
                .or_else(|| std::fs::read_to_string(file).ok())?;
            let request_options = request
                .arguments
                .get("options")
                .cloned()
                .unwrap_or_default();
            let mut native_open_map = serde_json::Map::new();
            native_open_map.insert(
                file.to_string(),
                serde_json::Value::String(source_text.clone()),
            );
            if let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "format",
                "file": file,
                "line": request.arguments.get("line").cloned().unwrap_or(serde_json::Value::Null),
                "offset": request.arguments.get("offset").cloned().unwrap_or(serde_json::Value::Null),
                "endLine": request.arguments.get("endLine").cloned().unwrap_or(serde_json::Value::Null),
                "endOffset": request.arguments.get("endOffset").cloned().unwrap_or(serde_json::Value::Null),
                "options": request_options,
                "openFiles": serde_json::Value::Object(native_open_map),
            })) {
                return Some(native);
            }

            let options = tsz::lsp::formatting::FormattingOptions {
                tab_size: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("tabSize"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4) as u32,
                insert_spaces: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("insertSpaces"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                ..Default::default()
            };

            let range = request
                .arguments
                .get("line")
                .and_then(serde_json::Value::as_u64)
                .zip(
                    request
                        .arguments
                        .get("offset")
                        .and_then(serde_json::Value::as_u64),
                )
                .zip(
                    request
                        .arguments
                        .get("endLine")
                        .and_then(serde_json::Value::as_u64)
                        .zip(
                            request
                                .arguments
                                .get("endOffset")
                                .and_then(serde_json::Value::as_u64),
                        ),
                )
                .map(|((line, offset), (end_line, end_offset))| {
                    Range::new(
                        Position::new(
                            line.saturating_sub(1) as u32,
                            offset.saturating_sub(1) as u32,
                        ),
                        Position::new(
                            end_line.saturating_sub(1) as u32,
                            end_offset.saturating_sub(1) as u32,
                        ),
                    )
                });

            let edits_result = if let Some(range) = range {
                tsz::lsp::formatting::DocumentFormattingProvider::format_range(
                    &source_text,
                    range,
                    &options,
                )
            } else {
                tsz::lsp::formatting::DocumentFormattingProvider::format_document(
                    file,
                    &source_text,
                    &options,
                )
            };

            match edits_result {
                Ok(edits) => {
                    let line_map = LineMap::build(&source_text);
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            let normalized =
                                narrow_indentation_only_edit(&source_text, &line_map, edit);
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(normalized.range.start),
                                "end": Self::lsp_to_tsserver_position(normalized.range.end),
                                "newText": normalized.new_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_format_on_key(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self
                .open_files
                .get(file)
                .cloned()
                .or_else(|| std::fs::read_to_string(file).ok())?;
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64()? as u32;
            let key = request.arguments.get("key")?.as_str()?;
            let request_options = request
                .arguments
                .get("options")
                .cloned()
                .unwrap_or_default();
            let mut native_open_map = serde_json::Map::new();
            native_open_map.insert(
                file.to_string(),
                serde_json::Value::String(source_text.clone()),
            );
            if let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "formatOnKey",
                "file": file,
                "line": line,
                "offset": offset,
                "key": key,
                "options": request_options,
                "openFiles": serde_json::Value::Object(native_open_map),
            })) {
                return Some(native);
            }

            let options = tsz::lsp::formatting::FormattingOptions {
                tab_size: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("tabSize"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4) as u32,
                insert_spaces: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("insertSpaces"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                ..Default::default()
            };

            // tsserver protocol uses 1-based line/offset, convert to 0-based
            let lsp_line = line.saturating_sub(1);
            let lsp_offset = offset.saturating_sub(1);

            match tsz::lsp::formatting::DocumentFormattingProvider::format_on_key(
                &source_text,
                lsp_line,
                lsp_offset,
                key,
                &options,
            ) {
                Ok(edits) => {
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(edit.range.start),
                                "end": Self::lsp_to_tsserver_position(edit.range.end),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(super) fn find_nearest_tsconfig(file: &str) -> Option<String> {
        let mut current = std::path::Path::new(file).parent();
        while let Some(dir) = current {
            for name in ["tsconfig.json", "jsconfig.json"] {
                let config_path = dir.join(name);
                if config_path.exists() {
                    return Some(config_path.to_string_lossy().to_string());
                }
            }
            current = dir.parent();
        }
        None
    }

    pub(crate) fn handle_reload(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // Clear caches so next request re-parses everything
        self.lib_cache.clear();
        self.unified_lib_cache = None;

        let reload_finished = if let Some(file) = request
            .arguments
            .get("file")
            .and_then(|value| value.as_str())
        {
            let source_path = request
                .arguments
                .get("tmpfile")
                .and_then(|value| value.as_str())
                .unwrap_or(file);
            if let Ok(content) = std::fs::read_to_string(source_path) {
                self.open_files.insert(file.to_string(), content);
                true
            } else {
                false
            }
        } else {
            // Re-read all open files for reload-project style requests.
            let paths: Vec<String> = self.open_files.keys().cloned().collect();
            for path in &paths {
                if let Ok(content) = std::fs::read_to_string(path) {
                    self.open_files.insert(path.clone(), content);
                }
            }
            true
        };

        self.success_response(
            seq,
            request,
            Some(serde_json::json!({ "reloadFinished": reload_finished })),
        )
    }

    pub(crate) fn handle_reload_projects(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.lib_cache.clear();
        self.unified_lib_cache = None;

        let paths: Vec<String> = self.open_files.keys().cloned().collect();
        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                self.open_files.insert(path.clone(), content);
            }
        }

        self.success_response(seq, request, None)
    }

    pub(crate) fn handle_compiler_options_for_inferred(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let options = request
            .arguments
            .get("options")
            .filter(|value| value.is_object())
            .or_else(|| {
                request
                    .arguments
                    .get("compilerOptions")
                    .filter(|value| value.is_object())
            })
            .or_else(|| request.arguments.is_object().then_some(&request.arguments));
        self.apply_inferred_project_options(options);
        self.success_response(seq, request, Some(serde_json::json!(true)))
    }

    pub(crate) fn handle_external_project(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        match request.command.as_str() {
            "openExternalProject" => {
                self.apply_inferred_project_options(request.arguments.get("options"));
                let project_name = request
                    .arguments
                    .get("projectFileName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();

                let mut tracked_files = Vec::new();
                if let Some(root_files) = request
                    .arguments
                    .get("rootFiles")
                    .and_then(serde_json::Value::as_array)
                {
                    for entry in root_files {
                        let Some(file_name) = entry.get("fileName").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        let content = entry
                            .get("content")
                            .and_then(serde_json::Value::as_str)
                            .map(std::string::ToString::to_string)
                            .or_else(|| std::fs::read_to_string(file_name).ok());
                        if let Some(content) = content {
                            self.open_files.insert(file_name.to_string(), content);
                        }
                        tracked_files.push(file_name.to_string());
                    }
                }
                if !project_name.is_empty() {
                    self.external_project_files
                        .insert(project_name, tracked_files);
                }
            }
            "openExternalProjects" => {
                if let Some(projects) = request
                    .arguments
                    .get("projects")
                    .and_then(serde_json::Value::as_array)
                {
                    for project in projects {
                        self.apply_inferred_project_options(project.get("options"));
                        let project_name = project
                            .get("projectFileName")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("")
                            .to_string();

                        let mut tracked_files = Vec::new();
                        if let Some(root_files) = project
                            .get("rootFiles")
                            .and_then(serde_json::Value::as_array)
                        {
                            for entry in root_files {
                                let Some(file_name) =
                                    entry.get("fileName").and_then(|v| v.as_str())
                                else {
                                    continue;
                                };
                                let content = entry
                                    .get("content")
                                    .and_then(serde_json::Value::as_str)
                                    .map(std::string::ToString::to_string)
                                    .or_else(|| std::fs::read_to_string(file_name).ok());
                                if let Some(content) = content {
                                    self.open_files.insert(file_name.to_string(), content);
                                }
                                tracked_files.push(file_name.to_string());
                            }
                        }
                        if !project_name.is_empty() {
                            self.external_project_files
                                .insert(project_name, tracked_files);
                        }
                    }
                }
            }
            "closeExternalProject" => {
                if let Some(project_name) = request
                    .arguments
                    .get("projectFileName")
                    .and_then(serde_json::Value::as_str)
                    && let Some(files) = self.external_project_files.remove(project_name)
                {
                    for file in files {
                        let still_owned_elsewhere = self
                            .external_project_files
                            .values()
                            .any(|other_files| other_files.iter().any(|p| p == &file));
                        if !still_owned_elsewhere {
                            self.open_files.remove(&file);
                        }
                    }
                }
            }
            _ => {}
        }

        let body = match request.command.as_str() {
            "openExternalProject" | "openExternalProjects" => Some(serde_json::json!(true)),
            _ => None,
        };
        self.success_response(seq, request, body)
    }

    pub(crate) fn handle_synchronize_project_list(
        &self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let include_redirect_info = request
            .arguments
            .get("includeProjectReferenceRedirectInfo")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let mut body: Vec<serde_json::Value> = Vec::new();

        let mut projects: Vec<(&String, &Vec<String>)> =
            self.external_project_files.iter().collect();
        projects.sort_by_key(|(left, _)| *left);

        for (project_name, files) in projects {
            body.push(Self::synchronize_project_list_entry(
                project_name,
                false,
                serde_json::json!({}),
                files.clone(),
                include_redirect_info,
            ));
        }

        let external_files: rustc_hash::FxHashSet<String> = self
            .external_project_files
            .values()
            .flat_map(|files| files.iter().cloned())
            .collect();
        let mut configured_projects: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        let mut inferred_roots: Vec<String> = Vec::new();

        let mut open_files: Vec<&String> = self.open_files.keys().collect();
        open_files.sort();
        for file in open_files {
            if external_files.contains(file) || !Self::is_supported_project_source_file(file) {
                continue;
            }
            match self.find_project_config_file(file) {
                Some(config_path) => {
                    configured_projects
                        .entry(config_path.clone())
                        .or_insert_with(|| {
                            let options = self
                                .read_config_json(&config_path)
                                .and_then(|config| config.get("compilerOptions").cloned())
                                .unwrap_or_else(|| serde_json::json!({}));
                            let (_, file_names) = self.compute_project_info(file);
                            Self::synchronize_project_list_entry(
                                &config_path,
                                false,
                                options,
                                file_names,
                                include_redirect_info,
                            )
                        });
                }
                None => inferred_roots.push(file.clone()),
            }
        }

        body.extend(configured_projects.into_values());

        if !inferred_roots.is_empty() {
            let mut file_names: Vec<String> = Vec::new();
            let (lib_names, no_lib, _) = self.inferred_project_info(&inferred_roots[0]);
            if !no_lib {
                file_names
                    .extend(self.resolve_virtual_lib_files(&lib_names, Some(&inferred_roots[0])));
            }

            let mut visited: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
            let mut project_files = Vec::new();
            for root in inferred_roots {
                self.collect_reachable_files(&root, &mut visited, &mut project_files);
            }
            project_files.sort();
            project_files.dedup();
            file_names.extend(project_files);

            body.push(Self::synchronize_project_list_entry(
                "/dev/null/inferredProject1*",
                true,
                self.inferred_project_options_json(),
                file_names,
                include_redirect_info,
            ));
        }

        self.success_response(seq, request, Some(serde_json::json!(body)))
    }
}
