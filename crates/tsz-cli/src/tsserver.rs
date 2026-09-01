use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tsz::CompilerOptions;
use tsz::diagnostics::Diagnostic;
use tsz::service::{
    DefinitionInfo, DocumentHighlights, LanguageService, ReferenceEntry, ReferencedSymbol,
    RenameLocation, RenameResult, ServiceQuery, TextSpan,
};
use tsz::source::SourceText;

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
            let Some(request) = read_framed_message(&mut self.input)? else {
                return Ok(());
            };
            let should_exit = request.get("command").and_then(Value::as_str) == Some("exit");
            let response = self.handle(&request);
            write_framed_message(&mut self.output, &response)?;
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
                let content = open_content(arguments, path)?;
                self.service.open(path, content);
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
                let result = self.service.syntactic_diagnostics(path);
                if !result.syntactic_completion.is_complete() {
                    return Err(format!(
                        "TSZ syntactic diagnostics incomplete: {}",
                        result.syntactic_completion.as_str()
                    ));
                }
                Ok(Some(self.diagnostics_body(
                    path,
                    result.diagnostics,
                    arguments,
                )))
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
                Ok(Some(self.diagnostics_body(
                    path,
                    result.diagnostics,
                    arguments,
                )))
            }
            "quickinfo" => {
                let (path, absolute) = self.location(arguments)?;
                let info = claimed_navigation(command, self.service.quick_info(&path, absolute))?
                    .ok_or_else(|| {
                    "No content available at the requested position.".to_string()
                })?;
                let span = protocol_span(self.source(&path)?, info.text_span);
                Ok(Some(json!({
                    "kind": info.kind,
                    "kindModifiers": "",
                    "start": span["start"],
                    "end": span["end"],
                    "displayString": info.display,
                    "documentation": "",
                    "tags": [],
                })))
            }
            "definitionAndBoundSpan" => {
                let (path, absolute) = self.location(arguments)?;
                let result = claimed_navigation(
                    command,
                    self.service.definition_and_bound_span(&path, absolute),
                )?;
                Ok(Some(self.definition_and_bound_span_body(&path, result)?))
            }
            "definition" => Ok(Some(self.definitions_at(command, arguments, false)?)),
            "typeDefinition" => Ok(Some(self.definitions_at(command, arguments, true)?)),
            "references-full" => {
                let (path, absolute) = self.location(arguments)?;
                let references =
                    claimed_navigation(command, self.service.references(&path, absolute))?;
                Ok(Some(self.full_references_body(references)?))
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
                let highlights = claimed_navigation(
                    command,
                    self.service
                        .document_highlights(&path, absolute, &files_to_search),
                )?;
                Ok(Some(self.document_highlights_body(highlights)?))
            }
            "rename" => {
                let (path, absolute) = self.location(arguments)?;
                let rename = claimed_navigation(command, self.service.rename(&path, absolute))?;
                Ok(Some(self.rename_body(&path, rename)?))
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
        let source = self
            .service
            .source_coordinates(&path)
            .ok_or_else(|| format!("File is not open: {path}"))?;
        let line = number_argument(arguments, "line")?;
        let offset = number_argument(arguments, "offset")?;
        let absolute = source
            .byte_offset(line, offset)
            .ok_or_else(|| "Position is outside the file.".to_string())?;
        Ok((path, absolute))
    }
    fn definitions_at(
        &self,
        command: &str,
        arguments: &Value,
        type_only: bool,
    ) -> Result<Value, String> {
        let (path, absolute) = self.location(arguments)?;
        let definitions = if type_only {
            claimed_navigation(command, self.service.type_definition(&path, absolute))?
        } else {
            claimed_navigation(
                command,
                self.service.definition_and_bound_span(&path, absolute),
            )?
            .map_or_else(Vec::new, |result| result.definitions)
        };
        self.definitions_body(&definitions)
    }
    fn definition_and_bound_span_body(
        &self,
        path: &str,
        result: Option<tsz::service::DefinitionAndBoundSpan>,
    ) -> Result<Value, String> {
        let Some(result) = result else {
            return Ok(json!({"definitions": []}));
        };
        Ok(json!({
            "definitions": self.definitions_body(&result.definitions)?,
            "textSpan": protocol_span(self.source(path)?, result.text_span),
        }))
    }
    fn definitions_body(&self, definitions: &[DefinitionInfo]) -> Result<Value, String> {
        definitions
            .iter()
            .map(|definition| {
                let source = self.source(&definition.file_name)?;
                let metadata = json_value(definition);
                let mut result =
                    protocol_location(source, definition.text_span, definition.context_span);
                for (target, source) in [
                    ("file", "fileName"),
                    ("kind", "kind"),
                    ("name", "name"),
                    ("containerKind", "containerKind"),
                    ("containerName", "containerName"),
                    ("isLocal", "isLocal"),
                    ("isAmbient", "isAmbient"),
                    ("unverified", "unverified"),
                    ("failedAliasResolution", "failedAliasResolution"),
                ] {
                    result[target] = metadata[source].clone();
                }
                Ok(result)
            })
            .collect::<Result<Vec<_>, String>>()
            .map(Value::Array)
    }
    fn source(&self, path: &str) -> Result<&SourceText, String> {
        self.service
            .source_coordinates(path)
            .ok_or_else(|| format!("File is not open: {path}"))
    }
    fn full_references_body(&self, references: Vec<ReferencedSymbol>) -> Result<Value, String> {
        references
            .into_iter()
            .map(|referenced| {
                let source = self.source(&referenced.definition.file_name)?;
                let mut definition = json_value(&referenced.definition);
                definition["textSpan"] = absolute_span(source, referenced.definition.text_span);
                if let Some(context_span) = referenced.definition.context_span {
                    definition["contextSpan"] = absolute_span(source, context_span);
                }
                let entries = referenced
                    .references
                    .into_iter()
                    .map(|entry| self.full_reference_entry(entry))
                    .collect::<Result<Vec<_>, String>>()?;
                Ok(json!({"definition": definition, "references": entries}))
            })
            .collect::<Result<Vec<_>, String>>()
            .map(Value::Array)
    }
    fn full_reference_entry(&self, entry: ReferenceEntry) -> Result<Value, String> {
        let source = self.source(&entry.file_name)?;
        let mut result = json_value(&entry);
        result["textSpan"] = absolute_span(source, entry.text_span);
        if let Some(context_span) = entry.context_span {
            result["contextSpan"] = absolute_span(source, context_span);
        }
        Ok(result)
    }
    fn document_highlights_body(
        &self,
        highlights: Vec<DocumentHighlights>,
    ) -> Result<Value, String> {
        highlights
            .into_iter()
            .map(|document| {
                let source = self.source(&document.file_name)?;
                let spans = document
                    .highlight_spans
                    .into_iter()
                    .map(|highlight| {
                        let mut span =
                            protocol_location(source, highlight.text_span, highlight.context_span);
                        span["kind"] = Value::String(highlight.kind);
                        span
                    })
                    .collect::<Vec<_>>();
                Ok(json!({"file": document.file_name, "highlightSpans": spans}))
            })
            .collect::<Result<Vec<_>, String>>()
            .map(Value::Array)
    }
    fn rename_body(&self, path: &str, rename: RenameResult) -> Result<Value, String> {
        if !rename.info.can_rename {
            return Ok(json!({
                "info": {
                    "canRename": false,
                    "localizedErrorMessage": rename
                        .info
                        .localized_error_message
                        .unwrap_or_else(|| "You cannot rename this element.".to_string()),
                },
                "locs": [],
            }));
        }
        let trigger_span = rename.info.trigger_span.map_or_else(
            || Ok::<_, String>(json!({})),
            |span| Ok(protocol_span_with_length(self.source(path)?, span)),
        )?;
        let locations = self.rename_location_groups(rename.locations)?;
        Ok(json!({
            "info": {
                "canRename": true,
                "displayName": rename.info.display_name.unwrap_or_default(),
                "fullDisplayName": rename.info.full_display_name.unwrap_or_default(),
                "kind": rename.info.kind.unwrap_or_default(),
                "kindModifiers": rename.info.kind_modifiers.unwrap_or_default(),
                "triggerSpan": trigger_span,
            },
            "locs": locations,
        }))
    }
    fn rename_location_groups(&self, locations: Vec<RenameLocation>) -> Result<Vec<Value>, String> {
        let mut groups: std::collections::BTreeMap<String, Vec<Value>> =
            std::collections::BTreeMap::new();
        for location in locations {
            let source = self.source(&location.file_name)?;
            let span = protocol_location(source, location.text_span, location.context_span);
            groups.entry(location.file_name).or_default().push(span);
        }
        Ok(groups
            .into_iter()
            .map(|(file, locs)| json!({"file": file, "locs": locs}))
            .collect())
    }
    fn apply_change(&mut self, path: &str, arguments: &Value) -> Result<(), String> {
        let source = self.source(path)?;
        if let Some(full_text) = arguments.get("fileContent").and_then(Value::as_str) {
            self.service.change(path, Arc::<str>::from(full_text));
            return Ok(());
        }
        let start_line = number_argument(arguments, "line")?;
        let start_offset = number_argument(arguments, "offset")?;
        let end_line = arguments
            .get("endLine")
            .map_or(Ok(start_line), |_| number_argument(arguments, "endLine"))?;
        let end_offset = arguments.get("endOffset").map_or(Ok(start_offset), |_| {
            number_argument(arguments, "endOffset")
        })?;
        let start = source
            .byte_offset(start_line, start_offset)
            .ok_or_else(|| "Change start is outside the file.".to_string())?
            as usize;
        let end = source
            .byte_offset(end_line, end_offset)
            .ok_or_else(|| "Change end is outside the file.".to_string())?
            as usize;
        if start > end {
            return Err("Change range is invalid.".to_string());
        }
        let current = source.text();
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
        let Some(source) = self.service.source_coordinates(path) else {
            return json!([]);
        };
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
                    let message = diagnostic_message_text(&diagnostic);
                    match include_line_position {
                        false => json!({
                            "start": {"line": start.0, "offset": start.1}, "end": {"line": end.0, "offset": end.1},
                            "text": message, "code": diagnostic.code,
                            "category": diagnostic.category.as_str(),
                        }),
                        true => json!({
                            "message": message, "start": diagnostic.start, "length": diagnostic.length,
                            "startLocation": {"line": start.0, "offset": start.1}, "endLocation": {"line": end.0, "offset": end.1},
                            "category": diagnostic.category.as_str(), "code": diagnostic.code,
                        }),
                    }
                })
                .collect(),
        )
    }
}

fn diagnostic_message_text(diagnostic: &Diagnostic) -> String {
    let mut message = diagnostic.message_text.clone();
    for related in &diagnostic.related_information {
        message.push('\n');
        message.push_str(&"  ".repeat(related.depth.max(1) as usize));
        message.push_str(&related.message_text);
    }
    message
}

fn claimed_navigation<T>(command: &str, query: ServiceQuery<T>) -> Result<T, String> {
    match query {
        ServiceQuery::Claimed(value) => Ok(value),
        ServiceQuery::Nonclaimed(nonclaim) => Err(format!(
            "TSZ {command} incomplete: {}",
            nonclaim.completion().as_str()
        )),
    }
}

pub fn read_framed_message(reader: &mut impl BufRead) -> Result<Option<Value>> {
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

pub fn write_framed_message(writer: &mut impl Write, message: &Value) -> Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    Ok(())
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

fn open_content(arguments: &Value, path: &str) -> Result<Arc<str>, String> {
    if let Some(content) = arguments
        .get("fileContent")
        .or_else(|| arguments.get("content"))
    {
        return content
            .as_str()
            .map(Arc::<str>::from)
            .ok_or_else(|| "Open request content must be a string.".to_string());
    }

    let bytes = std::fs::read(path)
        .map_err(|error| format!("Cannot open file '{path}' from disk: {error}"))?;
    Ok(Arc::<str>::from(decode_disk_source(&bytes)))
}

fn decode_disk_source(bytes: &[u8]) -> String {
    if let Some(content) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(content).into_owned();
    }
    let (content, little_endian) = if let Some(content) = bytes.strip_prefix(&[0xff, 0xfe]) {
        (content, true)
    } else if let Some(content) = bytes.strip_prefix(&[0xfe, 0xff]) {
        (content, false)
    } else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    let words = content
        .chunks_exact(2)
        .map(|pair| match little_endian {
            true => u16::from_le_bytes([pair[0], pair[1]]),
            false => u16::from_be_bytes([pair[0], pair[1]]),
        })
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&words)
}

fn number_argument(arguments: &Value, name: &str) -> Result<u32, String> {
    let value = arguments
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Request requires a numeric {name} argument."))?;
    u32::try_from(value).map_err(|_| format!("Request {name} is outside the supported range."))
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

fn protocol_span(source: &SourceText, span: TextSpan) -> Value {
    let start = source
        .position(span.start)
        .expect("valid service span start");
    let end = source
        .position(span.start + span.length)
        .expect("valid service span end");
    json!({
        "start": {"line": start.0, "offset": start.1},
        "end": {"line": end.0, "offset": end.1},
    })
}

fn protocol_location(source: &SourceText, span: TextSpan, context_span: Option<TextSpan>) -> Value {
    let mut result = protocol_span(source, span);
    if let Some(context_span) = context_span {
        let context = protocol_span(source, context_span);
        result["contextStart"] = context["start"].clone();
        result["contextEnd"] = context["end"].clone();
    }
    result
}

fn protocol_span_with_length(source: &SourceText, span: TextSpan) -> Value {
    let mut result = protocol_span(source, span);
    result["length"] = Value::from(
        source
            .utf16_range(span.start, span.length)
            .expect("valid service span")
            .1,
    );
    result
}

fn absolute_span(source: &SourceText, span: TextSpan) -> Value {
    let (start, length) = source
        .utf16_range(span.start, span.length)
        .expect("valid service span");
    json!({"start": start, "length": length})
}

fn json_value(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).expect("service response is serializable")
}

#[cfg(test)]
#[path = "../rewrite-tests/tsserver_coordinates_unit.rs"]
mod tests;
#[cfg(test)]
#[path = "../rewrite-tests/tsserver_type_definition_unit.rs"]
mod type_definition_tests;
