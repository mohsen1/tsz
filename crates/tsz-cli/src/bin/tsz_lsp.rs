use std::io::{BufRead, BufReader, Write};

use anyhow::{Context, Result};
use serde_json::{Value, json};

fn main() {
    if std::env::args()
        .skip(1)
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        println!(
            "tsz-lsp {}\n\nUsage: tsz-lsp\n\nSpeaks the Language Server Protocol over stdio.",
            env!("CARGO_PKG_VERSION")
        );
        return;
    }
    if run().is_err() {
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut output = std::io::stdout().lock();
    while let Some(message) = read_message(&mut input)? {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        if method == "exit" {
            return Ok(());
        }
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "capabilities": {
                        "textDocumentSync": 1
                    },
                    "serverInfo": {
                        "name": "tsz-lsp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }),
            "shutdown" => json!({"jsonrpc": "2.0", "id": id, "result": null}),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": "Method not implemented by the rewrite foundation."}
            }),
        };
        write_message(&mut output, &response)?;
        output.flush()?;
    }
    Ok(())
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.trim().strip_prefix("Content-Length:") {
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let length = length.context("missing Content-Length header")?;
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
