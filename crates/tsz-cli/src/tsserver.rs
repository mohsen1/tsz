use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tsz::CompilerOptions;
use tsz::diagnostics::{Diagnostic, DiagnosticCategory};
use tsz::service::LanguageService;
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
            "compilerOptionsForInferredProjects" | "configure" => {
                let options = arguments
                    .get("options")
                    .or_else(|| arguments.get("compilerOptions"))
                    .unwrap_or(arguments);
                self.service.configure(compiler_options(options));
                Ok(Some(json!(true)))
            }
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
                Ok(Some(self.diagnostics_body(path, diagnostics)))
            }
            "semanticDiagnosticsSync" => {
                let path = file_argument(arguments)?;
                let diagnostics = self.service.semantic_diagnostics(path);
                Ok(Some(self.diagnostics_body(path, diagnostics)))
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
                let source = SourceText::new(FileId(0), path.into(), text);
                let start = source.line_and_column(info.text_span.start);
                let end = source.line_and_column(info.text_span.start + info.text_span.length);
                Ok(Some(json!({
                    "kind": info.kind,
                    "kindModifiers": "",
                    "start": {"line": start.0, "offset": start.1},
                    "end": {"line": end.0, "offset": end.1},
                    "displayString": info.display,
                    "documentation": "",
                    "tags": [],
                })))
            }
            "tsz/reset" => {
                self.service.reset();
                Ok(Some(json!({"reset": true})))
            }
            "exit" => Ok(None),
            _ => Err(format!(
                "Command '{command}' is not implemented by the rewrite foundation."
            )),
        }
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

    fn diagnostics_body(&self, path: &str, diagnostics: Vec<Diagnostic>) -> Value {
        let Some(text) = self.service.text(path) else {
            return json!([]);
        };
        let source = SourceText::new(FileId(0), path.into(), text);
        Value::Array(
            diagnostics
                .into_iter()
                .map(|diagnostic| {
                    let start = source.line_and_column(diagnostic.start);
                    let end = source.line_and_column(diagnostic.start + diagnostic.length);
                    json!({
                        "start": {"line": start.0, "offset": start.1},
                        "end": {"line": end.0, "offset": end.1},
                        "text": diagnostic.message_text,
                        "code": diagnostic.code,
                        "category": category_number(diagnostic.category),
                    })
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

fn number_argument(arguments: &Value, name: &str) -> Result<u64, String> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Request requires a numeric {name} argument."))
}

fn compiler_options(value: &Value) -> CompilerOptions {
    let mut options = CompilerOptions::default();
    options.strict = value
        .get("strict")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    options.no_implicit_any = value
        .get("noImplicitAny")
        .and_then(Value::as_bool)
        .unwrap_or(options.strict);
    options.no_emit = true;
    options
}

fn position_to_offset(text: &str, line: u32, offset: u32) -> Option<u32> {
    if line == 0 || offset == 0 {
        return None;
    }
    let mut current_line = 1_u32;
    let mut line_start = 0_usize;
    for (index, byte) in text.bytes().enumerate() {
        if current_line == line {
            line_start = index;
            break;
        }
        if byte == b'\n' {
            current_line += 1;
            line_start = index + 1;
        }
    }
    if current_line != line {
        return None;
    }
    let absolute = line_start.checked_add(offset.saturating_sub(1) as usize)?;
    (absolute <= text.len()).then_some(absolute as u32)
}

const fn category_number(category: DiagnosticCategory) -> u8 {
    match category {
        DiagnosticCategory::Warning => 0,
        DiagnosticCategory::Error => 1,
        DiagnosticCategory::Suggestion => 2,
        DiagnosticCategory::Message => 3,
    }
}

pub fn run_legacy_server(input: impl Read, mut output: impl Write) -> Result<()> {
    let mut service = LanguageService::new(CompilerOptions {
        no_emit: true,
        ..CompilerOptions::default()
    });
    for line in BufReader::new(input).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line)?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let kind = request.get("type").and_then(Value::as_str).unwrap_or("");
        let response = match kind {
            "check" => {
                service.reset();
                if let Some(files) = request.get("files").and_then(Value::as_array) {
                    for file in files {
                        let Some(path) = file
                            .get("path")
                            .or_else(|| file.get("file"))
                            .and_then(Value::as_str)
                        else {
                            continue;
                        };
                        let content = file.get("content").and_then(Value::as_str).unwrap_or("");
                        service.open(path, Arc::<str>::from(content));
                    }
                }
                let started = std::time::Instant::now();
                let output_result = service.compile();
                let codes = output_result
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>();
                json!({
                    "id": id,
                    "codes": codes,
                    "elapsed_ms": started.elapsed().as_secs_f64() * 1_000.0,
                })
            }
            "status" => json!({
                "id": id,
                "memory_bytes": 0,
                "checks": 0,
                "cache_entries": 0,
            }),
            "recycle" => {
                service.reset();
                json!({"id": id, "ok": true})
            }
            "shutdown" => {
                writeln!(output, "{}", json!({"id": id, "ok": true}))?;
                output.flush()?;
                return Ok(());
            }
            _ => json!({"id": id, "error": format!("unsupported request type: {kind}")}),
        };
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    Ok(())
}
