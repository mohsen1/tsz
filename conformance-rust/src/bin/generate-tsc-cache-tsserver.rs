//! TSC Cache Generator using tsserver
//!
//! Uses TypeScript's language server for much faster cache generation.
//! Instead of spawning thousands of tsc processes, we maintain a single
//! tsserver process and query it for diagnostics.

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "generate-tsc-cache-tsserver")]
#[command(about = "Generate TSC cache using tsserver (faster)", long_about = None)]
struct Args {
    /// Test directory path
    #[arg(long, default_value = "./TypeScript/tests/cases/conformance")]
    test_dir: String,

    /// Output cache file path
    #[arg(long, default_value = "./tsc-cache.json")]
    output: String,

    /// Path to tsserver binary (or use npx tsserver)
    #[arg(long, default_value = "npx")]
    tsserver: String,

    /// Maximum number of tests to process (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    max: usize,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TscCacheEntry {
    metadata: FileMetadata,
    error_codes: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct FileMetadata {
    mtime_ms: u64,
    size: u64,
}

/// Sequence number for tsserver requests
static SEQ: AtomicU32 = AtomicU32::new(1);

fn next_seq() -> u32 {
    SEQ.fetch_add(1, Ordering::SeqCst)
}

/// tsserver request
#[derive(Serialize)]
struct TsServerRequest {
    seq: u32,
    #[serde(rename = "type")]
    msg_type: &'static str,
    command: &'static str,
    arguments: serde_json::Value,
}

/// tsserver response
#[derive(Deserialize, Debug)]
struct TsServerResponse {
    #[serde(rename = "type")]
    msg_type: String,
    command: Option<String>,
    request_seq: Option<u32>,
    success: Option<bool>,
    body: Option<serde_json::Value>,
}

/// Diagnostic from tsserver
#[derive(Deserialize, Debug)]
struct TsDiagnostic {
    code: Option<u32>,
}

/// TsServer client for communicating with tsserver
struct TsServerClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    verbose: bool,
}

impl TsServerClient {
    fn new(tsserver_path: &str, verbose: bool) -> Result<Self> {
        let mut cmd = if tsserver_path == "npx" {
            let mut c = Command::new("npx");
            c.arg("tsserver");
            c
        } else {
            Command::new(tsserver_path)
        };

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = BufReader::new(child.stdout.take().expect("Failed to open stdout"));

        Ok(Self {
            child,
            stdin,
            stdout,
            verbose,
        })
    }

    fn send_request(&mut self, command: &'static str, arguments: serde_json::Value) -> Result<u32> {
        let seq = next_seq();
        let request = TsServerRequest {
            seq,
            msg_type: "request",
            command,
            arguments,
        };

        let json = serde_json::to_string(&request)?;
        if self.verbose {
            eprintln!("-> {}", json);
        }

        writeln!(self.stdin, "{}", json)?;
        self.stdin.flush()?;

        Ok(seq)
    }

    fn read_response(&mut self, expected_seq: u32) -> Result<Option<serde_json::Value>> {
        loop {
            let mut line = String::new();
            let bytes_read = self.stdout.read_line(&mut line)?;
            
            if bytes_read == 0 {
                return Ok(None);
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Skip Content-Length headers
            if line.starts_with("Content-Length:") {
                continue;
            }

            if self.verbose {
                eprintln!("<- {}", line);
            }

            // Try to parse as JSON
            if let Ok(response) = serde_json::from_str::<TsServerResponse>(line) {
                // Check if this is the response we're waiting for
                if response.msg_type == "response" {
                    if let Some(req_seq) = response.request_seq {
                        if req_seq == expected_seq {
                            return Ok(response.body);
                        }
                    }
                }
                // Skip events and other responses
            }
        }
    }

    fn open_file(&mut self, file_path: &str, content: &str) -> Result<()> {
        let args = serde_json::json!({
            "file": file_path,
            "fileContent": content,
            "scriptKindName": if file_path.ends_with(".tsx") { "TSX" } 
                             else if file_path.ends_with(".jsx") { "JSX" }
                             else if file_path.ends_with(".js") { "JS" }
                             else { "TS" }
        });

        self.send_request("open", args)?;
        // Open doesn't return a response
        Ok(())
    }

    fn close_file(&mut self, file_path: &str) -> Result<()> {
        let args = serde_json::json!({
            "file": file_path
        });

        self.send_request("close", args)?;
        Ok(())
    }

    fn get_semantic_diagnostics(&mut self, file_path: &str) -> Result<Vec<u32>> {
        let args = serde_json::json!({
            "file": file_path,
            "includeLinePosition": false
        });

        let seq = self.send_request("semanticDiagnosticsSync", args)?;
        
        let body = self.read_response(seq)?;
        
        let mut codes = Vec::new();
        if let Some(diagnostics) = body {
            if let Some(arr) = diagnostics.as_array() {
                for diag in arr {
                    if let Some(code) = diag.get("code").and_then(|c| c.as_u64()) {
                        codes.push(code as u32);
                    }
                }
            }
        }

        codes.sort();
        codes.dedup();
        Ok(codes)
    }

    fn get_syntactic_diagnostics(&mut self, file_path: &str) -> Result<Vec<u32>> {
        let args = serde_json::json!({
            "file": file_path,
            "includeLinePosition": false
        });

        let seq = self.send_request("syntacticDiagnosticsSync", args)?;
        
        let body = self.read_response(seq)?;
        
        let mut codes = Vec::new();
        if let Some(diagnostics) = body {
            if let Some(arr) = diagnostics.as_array() {
                for diag in arr {
                    if let Some(code) = diag.get("code").and_then(|c| c.as_u64()) {
                        codes.push(code as u32);
                    }
                }
            }
        }

        codes.sort();
        codes.dedup();
        Ok(codes)
    }

    fn shutdown(&mut self) -> Result<()> {
        self.send_request("exit", serde_json::json!({}))?;
        self.child.wait()?;
        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("🔍 Discovering test files in: {}", args.test_dir);
    let test_files = discover_tests(&args.test_dir, args.max)?;
    println!("✓ Found {} test files", test_files.len());

    println!("\n🚀 Starting tsserver...");
    let mut client = TsServerClient::new(&args.tsserver, args.verbose)?;
    println!("✓ tsserver started");

    println!("\n🔨 Processing {} tests...", test_files.len());
    let start = Instant::now();

    let mut cache = HashMap::new();
    let mut processed = 0;
    let mut errors = 0;

    for path in &test_files {
        match process_test_file(&mut client, path) {
            Ok(Some((hash, entry))) => {
                cache.insert(hash, entry);
            }
            Ok(None) => {
                // Skipped
            }
            Err(e) => {
                if args.verbose {
                    eprintln!("✗ Error processing {}: {}", path.display(), e);
                }
                errors += 1;
            }
        }

        processed += 1;
        if processed % 100 == 0 {
            print!("\r[{}/{}] processed ({} errors)", processed, test_files.len(), errors);
            std::io::stdout().flush()?;
        }
    }

    println!("\r✓ Completed in {:.1}s ({:.0} tests/sec)", 
        start.elapsed().as_secs_f64(),
        test_files.len() as f64 / start.elapsed().as_secs_f64()
    );

    if errors > 0 {
        println!("  {} errors encountered", errors);
    }

    println!("\n🛑 Shutting down tsserver...");
    client.shutdown()?;

    println!("\n💾 Writing cache to: {}", args.output);
    write_cache(&args.output, &cache)?;
    println!("✓ Cache written with {} entries", cache.len());

    Ok(())
}

fn discover_tests(test_dir: &str, max: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(test_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        if path.extension().map_or(false, |ext| {
            ext == "ts" || ext == "tsx"
        }) {
            // Skip .d.ts files
            if path.to_string_lossy().ends_with(".d.ts") {
                continue;
            }
            files.push(path.to_path_buf());
        }
    }

    files.sort();

    if max > 0 && files.len() > max {
        files.truncate(max);
    }

    Ok(files)
}

fn process_test_file(
    client: &mut TsServerClient,
    path: &Path,
) -> Result<Option<(String, TscCacheEntry)>> {
    use std::fs;

    // Read file content
    let content = fs::read_to_string(path)?;

    // Parse directives
    let parsed = tsz_conformance::test_parser::parse_test_file(&content)?;

    // Check if should skip
    if tsz_conformance::test_parser::should_skip_test(&parsed.directives).is_some() {
        return Ok(None);
    }

    // Get file metadata
    let metadata = fs::metadata(path)?;
    let mtime_ms = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let size = metadata.len() as u64;

    // Calculate hash
    let hash = tsz_conformance::cache::calculate_test_hash(&content, &parsed.directives.options);

    // Get absolute path for tsserver
    let abs_path = path.canonicalize()?.to_string_lossy().to_string();

    // Open file in tsserver
    client.open_file(&abs_path, &content)?;

    // Get diagnostics
    let mut error_codes = client.get_syntactic_diagnostics(&abs_path)?;
    let semantic_codes = client.get_semantic_diagnostics(&abs_path)?;
    error_codes.extend(semantic_codes);
    error_codes.sort();
    error_codes.dedup();

    // Close file
    client.close_file(&abs_path)?;

    Ok(Some((
        hash,
        TscCacheEntry {
            metadata: FileMetadata { mtime_ms, size },
            error_codes,
        },
    )))
}

fn write_cache(path: &str, cache: &HashMap<String, TscCacheEntry>) -> Result<()> {
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, cache)?;
    Ok(())
}
