/// Scan forward from `start` (exclusive) to find the matching closing brace.
/// Returns the byte offset of the matching close brace, or None.
fn scan_forward(
    bytes: &[u8],
    code_map: &[bool],
    start: usize,
    open: u8,
    close: u8,
) -> Option<usize> {
    let mut depth = 1i32;
    let mut i = start + 1;
    while i < bytes.len() {
        if code_map[i] {
            if bytes[i] == open {
                depth += 1;
            } else if bytes[i] == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// Scan backward from `start` (exclusive) to find the matching opening brace.
/// Returns the byte offset of the matching open brace, or None.
fn scan_backward(
    bytes: &[u8],
    code_map: &[bool],
    start: usize,
    close: u8,
    open: u8,
) -> Option<usize> {
    let mut depth = 1i32;
    let mut i = start;
    while i > 0 {
        i -= 1;
        if code_map[i] {
            if bytes[i] == close {
                depth += 1;
            } else if bytes[i] == open {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Find matching angle bracket using AST-based analysis.
/// Returns the byte offset of the matching bracket, or None.
fn find_angle_bracket_match(arena: &NodeArena, source: &str, pos: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut pairs: Vec<(usize, usize)> = Vec::new();

    // Derive angle bracket positions from NodeList children.
    // The NodeList pos/end may be 0/0 (unset), but we can find the `<` and `>`
    // by looking at the first/last child nodes:
    //   `<` is at first_child.pos - 1
    //   `>` is at last_child.end - 1 (if parser includes `>` in range)
    //        or last_child.end (if parser excludes `>` from range)
    let check_list_nodes = |list: &Option<tsz::parser::base::NodeList>| -> Option<(usize, usize)> {
        let list = list.as_ref()?;
        if list.nodes.is_empty() {
            return None;
        }
        let first = arena.nodes.get(list.nodes.first()?.0 as usize)?;
        let last = arena.nodes.get(list.nodes.last()?.0 as usize)?;

        let open_pos = (first.pos as usize).checked_sub(1)?;
        if bytes.get(open_pos) != Some(&b'<') {
            return None;
        }

        // Try last_child.end - 1 first (parser includes `>` in range)
        let close_candidate1 = last.end as usize;
        if close_candidate1 > 0 && bytes.get(close_candidate1 - 1) == Some(&b'>') {
            return Some((open_pos, close_candidate1 - 1));
        }
        // Try last_child.end (parser excludes `>` from range)
        if bytes.get(close_candidate1) == Some(&b'>') {
            return Some((open_pos, close_candidate1));
        }
        None
    };

    // Collect from all data pools that have type_parameters or type_arguments
    for f in &arena.functions {
        if let Some(pair) = check_list_nodes(&f.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.classes {
        if let Some(pair) = check_list_nodes(&c.type_parameters) {
            pairs.push(pair);
        }
    }
    for iface in &arena.interfaces {
        if let Some(pair) = check_list_nodes(&iface.type_parameters) {
            pairs.push(pair);
        }
    }
    for t in &arena.type_aliases {
        if let Some(pair) = check_list_nodes(&t.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.call_exprs {
        if let Some(pair) = check_list_nodes(&c.type_arguments) {
            pairs.push(pair);
        }
    }
    for t in &arena.type_refs {
        if let Some(pair) = check_list_nodes(&t.type_arguments) {
            pairs.push(pair);
        }
    }
    for s in &arena.signatures {
        if let Some(pair) = check_list_nodes(&s.type_parameters) {
            pairs.push(pair);
        }
    }
    for m in &arena.method_decls {
        if let Some(pair) = check_list_nodes(&m.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.constructors {
        if let Some(pair) = check_list_nodes(&c.type_parameters) {
            pairs.push(pair);
        }
    }
    for ft in &arena.function_types {
        if let Some(pair) = check_list_nodes(&ft.type_parameters) {
            pairs.push(pair);
        }
    }
    for e in &arena.expr_with_type_args {
        if let Some(pair) = check_list_nodes(&e.type_arguments) {
            pairs.push(pair);
        }
    }

    // Type assertions: <type>expr
    for node in &arena.nodes {
        if node.kind == tsz::parser::syntax_kind_ext::TYPE_ASSERTION
            && let Some(ta) = arena.type_assertions.get(node.data_index as usize)
        {
            let open_pos = node.pos as usize;
            if bytes.get(open_pos) != Some(&b'<') {
                continue;
            }
            if let Some(type_node) = arena.nodes.get(ta.type_node.0 as usize) {
                // `>` might be at type_node.end - 1 or type_node.end
                let end = type_node.end as usize;
                if end > 0 && bytes.get(end - 1) == Some(&b'>') {
                    pairs.push((open_pos, end - 1));
                } else if bytes.get(end) == Some(&b'>') {
                    pairs.push((open_pos, end));
                }
            }
        }
    }

    // Search for the position in collected pairs
    for (open, close) in pairs {
        if pos == open {
            return Some(close);
        } else if pos == close {
            return Some(open);
        }
    }

    None
}

/// Read a Content-Length framed message from stdin (tsserver protocol)
fn read_content_length_message<R: BufRead>(reader: &mut R) -> Result<Option<String>> {
    let mut header_line = String::new();
    let bytes_read = reader.read_line(&mut header_line)?;
    if bytes_read == 0 {
        return Ok(None); // EOF
    }

    let header = header_line.trim();
    if header.is_empty() {
        // Skip empty lines (can happen between messages)
        return read_content_length_message(reader);
    }

    // Parse Content-Length header
    let content_length = if let Some(len_str) = header.strip_prefix("Content-Length:") {
        len_str
            .trim()
            .parse::<usize>()
            .with_context(|| format!("invalid Content-Length: {}", len_str.trim()))?
    } else {
        // Not a Content-Length header - try to parse as raw JSON (for compatibility)
        return Ok(Some(header.to_string()));
    };

    // Read the blank line separator
    let mut blank_line = String::new();
    reader.read_line(&mut blank_line)?;

    // Read the message body
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    String::from_utf8(body)
        .map(Some)
        .context("invalid UTF-8 in message body")
}

/// Write a Content-Length framed message to stdout (tsserver protocol)
fn write_content_length_message<W: Write>(stdout: &mut W, message: &str) -> Result<()> {
    write!(
        stdout,
        "Content-Length: {}\r\n\r\n{}",
        message.len(),
        message
    )?;
    stdout.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    // Initialize tracing (always stderr so it doesn't interfere with protocol).
    // Supports TSZ_LOG_FORMAT=tree|json|text (see src/tracing_config.rs).
    tsz_cli::tracing_config::init_tracing();

    let args = ServerArgs::parse();

    // Run on a large stack to prevent overflows in recursive AST traversals
    // (document highlights, find-references, narrowing) on deeply-nested code.
    // Matches the 128 MiB stack used by the tsz CLI for project-sized workloads.
    std::thread::Builder::new()
        .stack_size(limits::THREAD_STACK_SIZE_BYTES)
        .spawn(move || server_main(args))
        .expect("failed to spawn server thread")
        .join()
        .expect("server thread panicked")
}

fn server_main(args: ServerArgs) -> Result<()> {
    let mut server = Server::new(&args).context("failed to initialize server")?;

    info!("tsz-server ready (protocol: {:?})", args.protocol);

    match args.protocol {
        Protocol::Tsserver => run_tsserver_protocol(&mut server)?,
        Protocol::Legacy => run_legacy_protocol(&mut server)?,
    }

    Ok(())
}

fn run_tsserver_protocol(server: &mut Server) -> Result<()> {
    let mut stdin = BufReader::new(std::io::stdin());
    let mut stdout = std::io::stdout();

    run_tsserver_protocol_with_io(server, &mut stdin, &mut stdout)
}

fn run_tsserver_protocol_with_io<R: BufRead, W: Write>(
    server: &mut Server,
    stdin: &mut R,
    stdout: &mut W,
) -> Result<()> {
    loop {
        let message = match read_content_length_message(stdin)? {
            Some(msg) => msg,
            None => break, // EOF
        };

        if message.trim().is_empty() {
            continue;
        }

        let request: TsServerRequest = match serde_json::from_str(&message) {
            Ok(req) => req,
            Err(e) => {
                let error_response = TsServerResponse {
                    seq: server.next_seq(),
                    msg_type: "response".to_string(),
                    command: "unknown".to_string(),
                    request_seq: 0,
                    success: false,
                    message: Some(format!("invalid request: {e}")),
                    body: None,
                };
                let json = serde_json::to_string(&error_response)?;
                write_content_length_message(stdout, &json)?;
                continue;
            }
        };

        if request.command == "exit" {
            break;
        }

        let response = server.handle_tsserver_request(request);
        let json = serde_json::to_string(&response)?;
        write_content_length_message(stdout, &json)?;

        // Async events queued by the handler (e.g. `geterr` → `syntaxDiag`
        // / `semanticDiag` / `suggestionDiag` / `requestCompleted`) write
        // after the originating response. See #3544.
        for event in server.drain_pending_events() {
            let json = serde_json::to_string(&event)?;
            write_content_length_message(stdout, &json)?;
        }
    }

    Ok(())
}

fn run_legacy_protocol(server: &mut Server) -> Result<()> {
    let stdin = BufReader::new(std::io::stdin());
    let mut stdout = std::io::stdout();

    for line in stdin.lines() {
        let line = line.context("failed to read from stdin")?;
        if line.trim().is_empty() {
            continue;
        }

        let request: LegacyRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = LegacyResponse::Error(ErrorResponse {
                    id: 0,
                    error: format!("invalid request: {e}"),
                });
                writeln!(stdout, "{}", serde_json::to_string(&error_response)?)?;
                stdout.flush()?;
                continue;
            }
        };

        let is_shutdown = matches!(request, LegacyRequest::Shutdown { .. });
        let response = server.handle_legacy_request(request);
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;

        if is_shutdown {
            break;
        }
    }

    Ok(())
}
