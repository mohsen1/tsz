use std::io::{BufReader, Write};

use anyhow::Result;
use serde_json::{Value, json};
use tsz_cli::tsserver::{read_framed_message, write_framed_message};

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
    while let Some(message) = read_framed_message(&mut input)? {
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
                        "textDocumentSync": 0
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
        write_framed_message(&mut output, &response)?;
        output.flush()?;
    }
    Ok(())
}
