use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};
use tsz::CompilerOptions;
use tsz::diagnostics::Diagnostic;
use tsz::service::{
    DefinitionInfo, DocumentHighlights, LanguageService, ReferenceEntry, ReferencedSymbol,
    RenameLocation, RenameResult, TextSpan,
};
use tsz::source::{FileId, SourceText};

pub fn run_tsserver(input: impl Read, output: impl Write) -> Result<()> {
    Server::new(input, output).run()
}

struct Server<R, W> {
    input: BufReader<R>,
    output: W,
    service: LanguageService,
    sequence: u64,
}

impl<R: Read, W: Write> Server<R, W> {
    fn new(input: R, output: W) -> Self {
        Self {
            input: BufReader::new(input),
            output,
            service: LanguageService::new(CompilerOptions::default()),
            sequence: 0,
        }
    }

    fn run(mut self) -> Result<()> {
        loop {
            let Some(request) = read_message(&mut self.input)? else {
                return Ok(());
            };
            let should_exit = request.get("command").and_then(Value::as_str) == Some("exit");
            let response = self.handle(&request);
            write_message(&mut self.output, &response)?;
            self.output.flush()?;
            if should_exit {
                return Ok(());
            }
        }
    }

    fn handle(&mut self, request: &Value) -> Value {
        self.sequence += 1;
        let request_seq = request.get("seq").and_then(Value::as_u64).unwrap_or(0);
        let command = request.get("command").and_then(Value::as_str).unwrap_or("");
        let arguments = request.get("arguments").unwrap_or(&Value::Null);
        match self.dispatch(command, arguments) {
            Ok(body) => response(self.sequence, request_seq, command, true, body, None),
            Err(message) => response(
                self.sequence,
                request_seq,
                command,
                false,
                None,
                Some(message),
            ),
        }
    }

    fn dispatch(&mut self, command: &str, arguments: &Value) -> Result<Option<Value>, String> {
        match command {
            "compilerOptionsForInferredProjects" => {
                let options = arguments
                    .get("options")
                    .or_else(|| arguments.get("compilerOptions"))
                    .unwrap_or(arguments);
                self.service.configure(compiler_options(options));
                Ok(Some(json!(true)))
            }
            // `configure` carries formatting and language-service preferences,
            // not compiler options. Like `exit`, its response body must be
            // absent; the outer loop owns the actual exit transition.
            "configure" | "exit" => Ok(None),
            "open" => {
                let path = file_argument(arguments)?;
                let content = arguments
                    .get("fileContent")
                    .or_else(|| arguments.get("content"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                self.service.open(path, Arc::<str>::from(content));
                Ok(None)
            }
            "close" => {
                let path = file_argument(arguments)?;
                self.service.close(path);
                Ok(None)
            }
            "change" => {
                let path = file_argument(arguments)?;
                self.apply_change(path, arguments)?;
                Ok(None)
            }
            "syntacticDiagnosticsSync" => {
                let path = file_argument(arguments)?;
                let diagnostics = self.service.syntactic_diagnostics(path);
                Ok(Some(self.diagnostics_body(path, diagnostics, arguments)))
            }
            "semanticDiagnosticsSync" => {
                let path = file_argument(arguments)?;
                let result = self.service.semantic_diagnostics(path);
                if !result.semantic_completion.is_complete() {
                    return Err(format!(
                        "TSZ semantic diagnostics incomplete: {}",
                        result.semantic_completion.as_str()
                    ));
                }
                let body = self.diagnostics_body(path, result.diagnostics, arguments);
                Ok(Some(body))
            }
            "quickinfo" => {
                let path = file_argument(arguments)?;
                let text = self
                    .service
                    .text(path)
                    .ok_or_else(|| format!("File is not open: {path}"))?;
                let line = number_argument(arguments, "line")? as u32;
                let offset = number_argument(arguments, "offset")? as u32;
                let absolute = position_to_offset(&text, line, offset)
                    .ok_or_else(|| "Position is outside the file.".to_string())?;
                let info = self
                    .service
                    .quick_info(path, absolute)
                    .ok_or_else(|| "No content available at the requested position.".to_string())?;
                let start = offset_to_position(&text, info.text_span.start);
                let end = offset_to_position(
                    &text,
                    info.text_span.start.saturating_add(info.text_span.length),
                );
                Ok(Some(json!({
                    "kind": info.kind,
                    "kindModifiers": "",
                    "start": start,
                    "end": end,
                    "displayString": info.display,
                    "documentation": "",
                    "tags": [],
                })))
            }
            "definitionAndBoundSpan" => {
                let (path, absolute) = self.location(arguments)?;
                let result = self.service.definition_and_bound_span(&path, absolute);
                Ok(Some(self.definition_and_bound_span_body(&path, result)))
            }
            "definition" => {
                let (path, absolute) = self.location(arguments)?;
                let definitions = self
                    .service
                    .definition_and_bound_span(&path, absolute)
                    .map_or_else(Vec::new, |result| result.definitions);
                Ok(Some(self.definitions_body(&definitions)))
            }
            "typeDefinition" => {
                let (path, absolute) = self.location(arguments)?;
                let definitions = self
                    .service
                    .definition_and_bound_span(&path, absolute)
                    .map_or_else(Vec::new, |result| {
                        result
                            .definitions
                            .into_iter()
                            .filter(|definition| {
                                matches!(definition.kind.as_str(), "class" | "interface" | "type")
                            })
                            .collect()
                    });
                Ok(Some(self.definitions_body(&definitions)))
            }
            "references-full" => {
                let (path, absolute) = self.location(arguments)?;
                let references = self.service.references(&path, absolute);
                Ok(Some(self.full_references_body(references)))
            }
            "documentHighlights" => {
                let (path, absolute) = self.location(arguments)?;
                let files_to_search = arguments
                    .get("filesToSearch")
                    .and_then(Value::as_array)
                    .map(|files| {
                        files
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![path.clone()]);
                let highlights =
                    self.service
                        .document_highlights(&path, absolute, &files_to_search);
                Ok(Some(self.document_highlights_body(highlights)))
            }
            "rename" => {
                let (path, absolute) = self.location(arguments)?;
                let rename = self.service.rename(&path, absolute);
                Ok(Some(self.rename_body(&path, rename)))
            }
            // Harness state probe for adapter consistency; exposes text, never semantics.
            "tsz/text" => {
                let path = file_argument(arguments)?;
                self.service
                    .text(path)
                    .map(|text| Some(Value::String(text.to_string())))
                    .ok_or_else(|| format!("File is not open: {path}"))
            }
            "tsz/reset" => {
                self.service.reset();
                Ok(Some(json!({"reset": true})))
            }
            _ => Err(format!(
                "Command '{command}' is not implemented by the rewrite foundation."
            )),
        }
    }

    fn location(&self, arguments: &Value) -> Result<(String, u32), String> {
        let path = file_argument(arguments)?.to_string();
        let text = self
            .service
            .text(&path)
            .ok_or_else(|| format!("File is not open: {path}"))?;
        let line = number_argument(arguments, "line")? as u32;
        let offset = number_argument(arguments, "offset")? as u32;
        let absolute = position_to_offset(&text, line, offset)
            .ok_or_else(|| "Position is outside the file.".to_string())?;
        Ok((path, absolute))
    }

    fn definition_and_bound_span_body(
        &self,
        path: &str,
        result: Option<tsz::service::DefinitionAndBoundSpan>,
    ) -> Value {
        let Some(result) = result else {
            return json!({"definitions": []});
        };
        json!({
            "definitions": self.definitions_body(&result.definitions),
            "textSpan": self.protocol_span_for_open_file(path, result.text_span),
        })
    }

    fn definitions_body(&self, definitions: &[DefinitionInfo]) -> Value {
        Value::Array(
            definitions
                .iter()
                .filter_map(|definition| {
                    let text = self.service.text(&definition.file_name)?;
                    let mut result = protocol_span(&text, definition.text_span);
                    result["file"] = Value::String(definition.file_name.clone());
                    result["kind"] = Value::String(definition.kind.clone());
                    result["name"] = Value::String(definition.name.clone());
                    result["containerName"] = Value::String(definition.container_name.clone());
                    result["isLocal"] = Value::Bool(definition.is_local);
                    result["isAmbient"] = Value::Bool(definition.is_ambient);
                    result["unverified"] = Value::Bool(definition.unverified);
                    result["failedAliasResolution"] =
                        Value::Bool(definition.failed_alias_resolution);
                    if let Some(context_span) = definition.context_span {
                        let context = protocol_span(&text, context_span);
                        result["contextStart"] = context["start"].clone();
                        result["contextEnd"] = context["end"].clone();
                    }
                    Some(result)
                })
                .collect(),
        )
    }

    fn protocol_span_for_open_file(&self, path: &str, span: TextSpan) -> Value {
        self.service
            .text(path)
            .map_or_else(|| json!({}), |text| protocol_span(&text, span))
    }

    fn full_references_body(&self, references: Vec<ReferencedSymbol>) -> Value {
        Value::Array(
            references
                .into_iter()
                .filter_map(|referenced| {
                    let definition_text = self.service.text(&referenced.definition.file_name)?;
                    let mut definition = json!({
                        "containerKind": referenced.definition.container_kind,
                        "containerName": referenced.definition.container_name,
                        "fileName": referenced.definition.file_name,
                        "kind": referenced.definition.kind,
                        "name": referenced.definition.name,
                        "textSpan": absolute_span(&definition_text, referenced.definition.text_span),
                        "displayParts": referenced.definition.display_parts,
                    });
                    if let Some(context_span) = referenced.definition.context_span {
                        definition["contextSpan"] = absolute_span(&definition_text, context_span);
                    }
                    let entries = referenced
                        .references
                        .into_iter()
                        .filter_map(|entry| self.full_reference_entry(entry))
                        .collect::<Vec<_>>();
                    Some(json!({"definition": definition, "references": entries}))
                })
                .collect(),
        )
    }

    fn full_reference_entry(&self, entry: ReferenceEntry) -> Option<Value> {
        let text = self.service.text(&entry.file_name)?;
        let mut result = json!({
            "textSpan": absolute_span(&text, entry.text_span),
            "fileName": entry.file_name,
            "isWriteAccess": entry.is_write_access,
        });
        if let Some(context_span) = entry.context_span {
            result["contextSpan"] = absolute_span(&text, context_span);
        }
        if let Some(is_definition) = entry.is_definition {
            result["isDefinition"] = Value::Bool(is_definition);
        }
        Some(result)
    }

    fn document_highlights_body(&self, highlights: Vec<DocumentHighlights>) -> Value {
        Value::Array(
            highlights
                .into_iter()
                .filter_map(|document| {
                    let text = self.service.text(&document.file_name)?;
                    let spans = document
                        .highlight_spans
                        .into_iter()
                        .map(|highlight| {
                            let mut span = protocol_span(&text, highlight.text_span);
                            span["kind"] = Value::String(highlight.kind);
                            if let Some(context_span) = highlight.context_span {
                                let context = protocol_span(&text, context_span);
                                span["contextStart"] = context["start"].clone();
                                span["contextEnd"] = context["end"].clone();
                            }
                            span
                        })
                        .collect::<Vec<_>>();
                    Some(json!({"file": document.file_name, "highlightSpans": spans}))
                })
                .collect(),
        )
    }

    fn rename_body(&self, path: &str, rename: RenameResult) -> Value {
        if !rename.info.can_rename {
            return json!({
                "info": {
                    "canRename": false,
                    "localizedErrorMessage": rename
                        .info
                        .localized_error_message
                        .unwrap_or_else(|| "You cannot rename this element.".to_string()),
                },
                "locs": [],
            });
        }
        let trigger_span = rename.info.trigger_span.map_or_else(
            || json!({}),
            |span| {
                self.service
                    .text(path)
                    .map_or_else(|| json!({}), |text| protocol_span_with_length(&text, span))
            },
        );
        let locations = self.rename_location_groups(rename.locations);
        json!({
            "info": {
                "canRename": true,
                "displayName": rename.info.display_name.unwrap_or_default(),
                "fullDisplayName": rename.info.full_display_name.unwrap_or_default(),
                "kind": rename.info.kind.unwrap_or_default(),
                "kindModifiers": rename.info.kind_modifiers.unwrap_or_default(),
                "triggerSpan": trigger_span,
            },
            "locs": locations,
        })
    }

    fn rename_location_groups(&self, locations: Vec<RenameLocation>) -> Vec<Value> {
        let mut groups: std::collections::BTreeMap<String, Vec<Value>> =
            std::collections::BTreeMap::new();
        for location in locations {
            let Some(text) = self.service.text(&location.file_name) else {
                continue;
            };
            let mut span = protocol_span(&text, location.text_span);
            if let Some(context_span) = location.context_span {
                let context = protocol_span(&text, context_span);
                span["contextStart"] = context["start"].clone();
                span["contextEnd"] = context["end"].clone();
            }
            groups.entry(location.file_name).or_default().push(span);
        }
        groups
            .into_iter()
            .map(|(file, locs)| json!({"file": file, "locs": locs}))
            .collect()
    }

    fn apply_change(&mut self, path: &str, arguments: &Value) -> Result<(), String> {
        let current = self
            .service
            .text(path)
            .ok_or_else(|| format!("File is not open: {path}"))?;
        if let Some(full_text) = arguments.get("fileContent").and_then(Value::as_str) {
            self.service.change(path, Arc::<str>::from(full_text));
            return Ok(());
        }
        let start_line = number_argument(arguments, "line")? as u32;
        let start_offset = number_argument(arguments, "offset")? as u32;
        let end_line = arguments
            .get("endLine")
            .and_then(Value::as_u64)
            .unwrap_or(u64::from(start_line)) as u32;
        let end_offset = arguments
            .get("endOffset")
            .and_then(Value::as_u64)
            .unwrap_or(u64::from(start_offset)) as u32;
        let start = position_to_offset(&current, start_line, start_offset)
            .ok_or_else(|| "Change start is outside the file.".to_string())?
            as usize;
        let end = position_to_offset(&current, end_line, end_offset)
            .ok_or_else(|| "Change end is outside the file.".to_string())?
            as usize;
        if start > end || !current.is_char_boundary(start) || !current.is_char_boundary(end) {
            return Err("Change range is invalid.".to_string());
        }
        let inserted = arguments
            .get("insertString")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut changed = String::with_capacity(current.len() + inserted.len());
        changed.push_str(&current[..start]);
        changed.push_str(inserted);
        changed.push_str(&current[end..]);
        self.service.change(path, Arc::<str>::from(changed));
        Ok(())
    }

    fn diagnostics_body(&self, path: &str, diagnostics: Vec<Diagnostic>, args: &Value) -> Value {
        let Some(text) = self.service.text(path) else {
            return json!([]);
        };
        let source = SourceText::new(FileId(0), path.into(), text);
        let include_line_position = args.get("includeLinePosition").is_some_and(|value| {
            !matches!(value, Value::Null | Value::Bool(false))
                && value.as_f64() != Some(0.0)
                && value.as_str() != Some("")
        });
        Value::Array(
            diagnostics
                .into_iter()
                .map(|diagnostic| {
                    let start = source.line_and_column(diagnostic.start);
                    let end = source.line_and_column(diagnostic.start + diagnostic.length);
                    match include_line_position {
                        false => json!({
                            "start": {"line": start.0, "offset": start.1}, "end": {"line": end.0, "offset": end.1},
                            "text": diagnostic.message_text, "code": diagnostic.code,
                            "category": diagnostic.category.as_str(),
                        }),
                        true => json!({
                            "message": diagnostic.message_text, "start": diagnostic.start, "length": diagnostic.length,
                            "startLocation": {"line": start.0, "offset": start.1}, "endLocation": {"line": end.0, "offset": end.1},
                            "category": diagnostic.category.as_str(), "code": diagnostic.code,
                        }),
                    }
                })
                .collect(),
        )
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.trim().strip_prefix("Content-Length:").map(str::trim) {
            content_length = Some(value.parse::<usize>()?);
        }
    }
    let length = content_length.context("missing Content-Length header")?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

fn write_message(writer: &mut impl Write, message: &Value) -> Result<()> {
    let body = if message.get("command").and_then(Value::as_str) == Some("references-full") {
        serde_json::to_vec(&OrderedReferencesJson(message))?
    } else {
        serde_json::to_vec(message)?
    };
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    Ok(())
}

/// `references-full` is consumed directly by the TypeScript service harness,
/// whose baselines preserve JavaScript insertion order. `serde_json::Value`
/// stores maps in lexical order in this workspace, so serialize the protocol's
/// structurally ordered fields without changing their values.
struct OrderedReferencesJson<'a>(&'a Value);

impl Serialize for OrderedReferencesJson<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            Value::Array(values) => serializer.collect_seq(values.iter().map(Self)),
            Value::Object(values) => {
                let mut keys = values.keys().map(String::as_str).collect::<Vec<_>>();
                let order = references_field_order(values);
                keys.sort_by_key(|key| {
                    order
                        .iter()
                        .position(|candidate| candidate == key)
                        .unwrap_or(order.len())
                });
                serializer.collect_map(keys.into_iter().map(|key| (key, Self(&values[key]))))
            }
            value => value.serialize(serializer),
        }
    }
}

fn references_field_order(values: &serde_json::Map<String, Value>) -> &'static [&'static str] {
    if values.contains_key("containerKind") && values.contains_key("displayParts") {
        &[
            "containerKind",
            "containerName",
            "fileName",
            "kind",
            "name",
            "textSpan",
            "displayParts",
            "contextSpan",
        ]
    } else if values.len() == 2 && values.contains_key("text") && values.contains_key("kind") {
        &["text", "kind"]
    } else if values.len() == 2 && values.contains_key("start") && values.contains_key("length") {
        &["start", "length"]
    } else if values.contains_key("isWriteAccess") && values.contains_key("fileName") {
        &[
            "textSpan",
            "fileName",
            "contextSpan",
            "isWriteAccess",
            "isDefinition",
        ]
    } else {
        &[]
    }
}

fn response(
    seq: u64,
    request_seq: u64,
    command: &str,
    success: bool,
    body: Option<Value>,
    message: Option<String>,
) -> Value {
    let mut response = json!({
        "seq": seq,
        "type": "response",
        "command": command,
        "request_seq": request_seq,
        "success": success,
    });
    if let Some(body) = body {
        response["body"] = body;
    }
    if let Some(message) = message {
        response["message"] = Value::String(message);
    }
    response
}

fn file_argument(arguments: &Value) -> Result<&str, String> {
    arguments
        .get("file")
        .or_else(|| arguments.get("fileName"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Request requires a file argument.".to_string())
}

fn number_argument(arguments: &Value, name: &str) -> Result<u64, String> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Request requires a numeric {name} argument."))
}

fn compiler_options(value: &Value) -> CompilerOptions {
    let defaults = CompilerOptions::default();
    let boolean = |name| value.get(name).and_then(Value::as_bool);
    let check_js = boolean("checkJs");
    let allow_js = boolean("allowJs").unwrap_or(check_js == Some(true));
    let target = value
        .get("target")
        .and_then(Value::as_str)
        .map_or_else(|| defaults.target.clone(), str::to_string);
    let lib = value.get("lib").and_then(Value::as_array).map(|libraries| {
        libraries
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    });
    CompilerOptions {
        strict: boolean("strict").unwrap_or(false),
        strict_null_checks: boolean("strictNullChecks"),
        strict_property_initialization: boolean("strictPropertyInitialization"),
        no_implicit_any: boolean("noImplicitAny"),
        use_define_for_class_fields: boolean("useDefineForClassFields"),
        no_unused_locals: boolean("noUnusedLocals").unwrap_or(false),
        no_unused_parameters: boolean("noUnusedParameters").unwrap_or(false),
        no_lib: boolean("noLib").unwrap_or(false),
        allow_js,
        check_js,
        lib,
        target,
        no_emit: true,
        ..defaults
    }
}

fn position_to_offset(text: &str, line: u32, offset: u32) -> Option<u32> {
    if line == 0 || offset == 0 {
        return None;
    }
    let line_start = match line {
        1 => 0,
        _ => text.match_indices('\n').nth(line as usize - 2)?.0 + 1,
    };
    let target_units = offset - 1;
    let line_text = text.get(line_start..)?.split(['\r', '\n']).next()?;
    let mut units = 0_u32;
    for (relative, character) in line_text.char_indices() {
        if units == target_units {
            return Some((line_start + relative) as u32);
        }
        units += character.len_utf16() as u32;
        if units > target_units {
            return None;
        }
    }
    (units == target_units).then_some((line_start + line_text.len()) as u32)
}

fn protocol_span(text: &str, span: TextSpan) -> Value {
    json!({
        "start": offset_to_position(text, span.start),
        "end": offset_to_position(text, span.start.saturating_add(span.length)),
    })
}

fn protocol_span_with_length(text: &str, span: TextSpan) -> Value {
    let mut result = protocol_span(text, span);
    result["length"] = Value::from(
        byte_to_utf16_offset(text, span.start.saturating_add(span.length))
            .saturating_sub(byte_to_utf16_offset(text, span.start)),
    );
    result
}

fn absolute_span(text: &str, span: TextSpan) -> Value {
    let start = byte_to_utf16_offset(text, span.start);
    let end = byte_to_utf16_offset(text, span.start.saturating_add(span.length));
    json!({"start": start, "length": end.saturating_sub(start)})
}

fn offset_to_position(text: &str, offset: u32) -> Value {
    let end = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(text.len());
    let (line, line_text) = text[..end].split('\n').enumerate().last().unwrap();
    json!({"line": line as u32 + 1, "offset": line_text.encode_utf16().count() as u32 + 1})
}

fn byte_to_utf16_offset(text: &str, offset: u32) -> u32 {
    let end = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(text.len());
    text[..end].encode_utf16().count() as u32
}
