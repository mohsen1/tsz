# Research Team 10: LSP Testing Infrastructure

**Research Date:** 2026-01-30
**Focus:** LSP testing gaps and comprehensive testing strategy
**Tools Used:** Manual code analysis + Gemini LSP analysis via `ask-gemini.mjs`

---

## Executive Summary

The tsz LSP implementation has **solid unit test coverage** for individual features but lacks critical **integration testing, protocol-level validation, and performance benchmarking**. This report identifies gaps and provides a roadmap for comprehensive testing infrastructure.

---

## Current Unit Test Coverage

### Existing Test Structure

**Location:** `src/lsp/tests/`

**Dedicated LSP Test Files:**
- `project_tests.rs` - Multi-file project operations
- `code_actions_tests.rs` - Code actions and quick fixes (1,948 lines)
- `signature_help_tests.rs` - Signature help functionality (749 lines)
- `tests.rs` - Integration workflow tests (137 lines)

**Inline Module Tests:**
Each LSP feature module contains `#[cfg(test)]` tests:
- `completions.rs` - Member completion, scope shadowing, keyword suggestions
- `definition.rs` - Go-to-Definition for variables, classes, cross-file imports
- `rename.rs` - Renaming variables, functions, identifier validation
- `hover.rs` - Hover content generation, JSDoc extraction
- `semantic_tokens.rs` - Token generation, delta encoding
- `signature_help.rs` - Active parameter detection, signature extraction
- `code_lens.rs`, `folding.rs`, `selection_range.rs`, `formatting.rs`

### Test Coverage Quality

**Strengths:**
- **Comprehensive Code Action Testing:** 1,948 lines covering extract variable, import organization, quick fixes, cross-file scenarios
- **Project-Level Testing:** Multi-file scenarios including cross-file references, incremental text updates, file addition/removal
- **Signature Help Edge Cases:** Trailing commas, comments in parameter lists, overload selection, JSDoc integration

**Test Methodology:**
Tests typically instantiate `ParserState`, `BinderState`, and `LineMap`, then invoke providers directly:

```rust
let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
let root = parser.parse_source_file();
let mut binder = BinderState::new();
binder.bind_source_file(arena, root);
let provider = HoverProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
let hover = provider.get_hover(root, position);
```

---

## Critical Integration Test Gaps

### 1. JSON-RPC Protocol Layer - **MISSING**

**Current State:** Tests operate on Rust structs without validating JSON serialization/deserialization.

**Missing Tests:**
- Content-Length framing (`src/bin/tsz_lsp.rs:846-849`)
- JSON-RPC message parsing (`jsonrpc: "2.0"`, `id`, `method`, `params`)
- Error response formatting (invalid requests, method not found)
- Notification vs request handling (no `id` field)

**Impact:** Protocol bugs could go undetected until real editor integration fails.

### 2. Server Lifecycle - **MISSING**

**Required Test Scenarios:**
```bash
initialize → initialized → shutdown → exit
```

**Missing Validation:**
- Server rejects requests before `initialize`
- Server accepts requests only after `initialized` notification
- `shutdown` flag is respected (exit code 0 vs 1 on `exit`)
- Document state persists across lifecycle changes

### 3. Concurrency & Cancellation - **MISSING**

**Missing Tests:**
- Simultaneous document edits during completion requests
- Request cancellation (LSP `$/cancelRequest` notification)
- Race conditions: file closed while hover request in-flight
- Parallel multi-file diagnostics

### 4. Configuration Changes - **MISSING**

**Missing Tests:**
- `workspace/didChangeConfiguration` handling
- `tsconfig.json` hot-reloading (strict mode toggles, lib changes)
- Per-file override settings

### 5. File System Watching - **MISSING**

**Current State:** `cli/watch.rs` exists but no integration tests verify it triggers LSP updates.

**Missing:**
- File created event → `Project::set_file` called
- File deleted event → `Project::remove_file` called
- File modified event → incremental update triggered
- Debouncing behavior (multiple rapid changes)

---

## Binary Testing Strategy

### Current Binaries

**`tsz-lsp`** (`src/bin/tsz_lsp.rs`)
- Standard LSP server (JSON-RPC over stdio)
- **Tested:** None (manual editor integration only)

**`tsz-server`** (`src/bin/tsz_server.rs`)
- tsserver-compatible protocol
- **Tested:** Via TypeScript conformance tests

### Real Editor Testing Approach

#### A. VS Code Extension Testing

**Create minimal test extension:**

**`package.json`:**
```json
{
  "name": "tsz-lsp-test",
  "activationEvents": ["onLanguage:typescript"],
  "contributes": {
    "languages": [{ "id": "typescript", "extensions": [".ts"] }],
    "languageServers": [
      {
        "id": "tsz",
        "label": "TSZ Language Server",
        "server": {
          "command": "/path/to/target/release/tsz",
          "args": ["lsp"],
          "transport": "stdio"
        }
      }
    ]
  }
}
```

**Test Scenarios:**
1. Open a TypeScript file → Verify `initialize` handshake
2. Trigger hover → Assert response structure
3. Request completion → Validate items
4. Invoke code action → Check edits

**Automation:** Use `vscode-test` npm package for programmatic testing:

```typescript
import { runTests } from 'vscode-test';

async function run() {
  await runTests({ extensionDevelopmentPath, extensionTestsPath });
}
```

#### B. Neovim Native LSP

**Configure `init.lua`:**
```lua
vim.lsp.config.tsz = {
  cmd = { "/path/to/target/release/tsz", "lsp" },
  filetypes = { "typescript", "typescriptreact" },
  root_dir = vim.lsp.util.root_pattern("tsconfig.json", ".git"),
}
require('lspconfig').tsz.setup{}
```

**Test Scenarios:**
- `:lua vim.lsp.buf.hover()` - Verify hover response
- `:lua vim.lsp.buf.definition()` - Check go-to-definition
- `:lua vim.lsp.buf.references()` - Validate find-references

#### C. E2E Automated Testing Framework

**Recommended Tool:** `langserver-rs` or custom test harness

**Example Test Flow:**
```rust
#[tokio::test]
async fn test_completion_in_typescript_file() {
  let mut client = LspClient::spawn("./target/release/tsz lsp").await;

  // Initialize
  client.send_request("initialize", json!({})).await;
  client.send_notification("initialized", json!({})).await;

  // Open file
  client.send_notification("textDocument/didOpen", json!({
    "textDocument": {
      "uri": "file:///test.ts",
      "text": "const x = 1;\nx.toStr|",
      "version": 1
    }
  })).await;

  // Trigger completion
  let response = client.send_request("textDocument/completion", json!({
    "textDocument": { "uri": "file:///test.ts" },
    "position": { "line": 1, "character": 7 }
  })).await;

  // Assert
  assert!(response["items"].as_array().unwrap().len() > 0);
}
```

---

## Performance Benchmarking Strategy

### Existing Infrastructure

**`ProjectPerformance`** (`src/lsp/project.rs:959-981`):

```rust
pub struct ProjectPerformance {
  timings: FxHashMap<ProjectRequestKind, ProjectRequestTiming>,
}

pub struct ProjectRequestTiming {
  pub duration: Duration,
  pub scope_hits: u32,
  pub scope_misses: u32,
}
```

**Tracked Operations:**
- Definition, References, Hover, Rename, Completions, Diagnostics

**Current State:** Metrics are collected but **not exposed** outside the `Project` struct.

### Benchmarking Recommendations

#### A. LSP-Specific Benchmark Suite

**Create `benches/lsp_bench.rs`:**

```rust
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn bench_go_to_definition_large_file(c: &mut Criterion) {
  let source = generate_large_typescript_file(1000); // 1000 lines
  let mut project = Project::new();
  project.set_file("test.ts", source);

  c.bench_function("goto_def_large_file", |b| {
    b.iter(|| {
      black_box(project.get_definition("test.ts", Position::new(500, 10)))
    })
  });
}

fn bench_completion_global_scope(c: &mut Criterion) {
  let source = "export const a = 1; export const b = 2; /* ... 100 exports */";
  let mut project = Project::new();
  project.set_file("test.ts", source.to_string());

  c.bench_function("completion_global_scope", |b| {
    b.iter(|| {
      black_box(project.get_completions("test.ts", Position::new(0, 50)))
    })
  });
}
```

#### B. Real-World Scenarios

**1. Cold Start (Project Load)**
```rust
fn bench_cold_start_project_with_libs(c: &mut Criterion) {
  c.bench_function("cold_start_with_libs", |b| {
    b.iter(|| {
      let mut project = Project::new();
      project.load_lib("lib.es5.d.ts");
      project.load_lib("lib.dom.d.ts");
      project.set_file("index.ts", source.clone());
      black_box(project.get_diagnostics("index.ts"))
    })
  });
}
```

**2. Incremental Edit Latency**
```rust
fn bench_incremental_edit_typing(c: &mut Criterion) {
  let mut project = Project::new();
  let initial_source = "const x: string = \"hello\";\nx";
  project.set_file("test.ts", initial_source.to_string());

  c.bench_function("incremental_edit_typing", |b| {
    b.iter(|| {
      let edit = TextEdit {
        range: Range {
          start: Position::new(1, 1),
          end: Position::new(1, 1)
        },
        new_text: "+".to_string()
      };
      project.update_file_with_edit("test.ts", vec![edit]);
      black_box(project.get_diagnostics("test.ts"))
    })
  });
}
```

**3. Large Project Navigation**
```rust
fn bench_find_references_monorepo(c: &mut Criterion) {
  let mut project = Project::new();
  // Simulate 100 files using the same export
  for i in 0..100 {
    let source = format!("import {{ foo }} from './common'; foo;");
    project.set_file(format!("file{}.ts", i), source);
  }
  project.set_file("common.ts", "export const foo = 1;");

  c.bench_function("find_refs_monorepo", |b| {
    b.iter(|| {
      black_box(project.find_references("common.ts", Position::new(0, 15)))
    })
  });
}
```

#### C. Metrics Export

**Add LSP command for performance data:**
```rust
// In tsz-lsp.rs
fn handle_custom_performance(&self) -> Result<Value> {
  let perf = self.documents.values()
    .map(|doc| doc.project.performance())
    .collect::<Vec<_>>();

  Ok(serde_json::json!({
    "files": perf,
    "timestamp": SystemTime::now()
  }))
}
```

---

## Test Infrastructure Recommendations

### Immediate Actions (High Priority)

**1. Add Protocol Validation Tests**
```rust
#[test]
fn test_json_rpc_content_length_framing() {
  let message = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
  let framed = format!("Content-Length: {}\r\n\r\n{}", message.len(), message);

  let mut reader = BufReader::new(framed.as_bytes());
  let parsed = read_content_length_message(&mut reader).unwrap();
  assert_eq!(parsed, message);
}
```

**2. Add Server Lifecycle Tests**
```rust
#[test]
fn test_rejects_requests_before_initialize() {
  let mut server = LspServer::new();
  let response = server.handle_message(JsonRpcMessage {
    jsonrpc: "2.0".to_string(),
    id: Some(Value::Number(1.into())),
    method: Some("textDocument/hover".to_string()),
    params: None,
  });

  assert!(response.unwrap().error.is_some());
}
```

**3. Expose Performance Metrics**
```rust
// Add to Project
pub fn get_performance_stats(&self) -> serde_json::Value {
  serde_json::to_value(self.performance).unwrap()
}
```

### Medium-Term Improvements

**1. Concurrency Tests**
```rust
#[tokio::test]
async fn test_concurrent_edits_and_queries() {
  let mut project = Arc::new(Mutex::new(Project::new()));
  let project_clone = project.clone();

  // Spawn background editor task
  let edit_task = tokio::spawn(async move {
    for i in 0..100 {
      let mut p = project_clone.lock().unwrap();
      p.set_file("test.ts", format!("const x = {};", i));
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
  });

  // Spawn query task
  let query_task = tokio::spawn(async move {
    for _ in 0..100 {
      let p = project.lock().unwrap();
      p.get_definition("test.ts", Position::new(0, 10));
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
  });

  tokio::try_join!(edit_task, query_task).unwrap();
}
```

**2. Configuration Change Tests**
```rust
#[test]
fn test_workspace_did_change_configuration() {
  let mut server = LspServer::new();
  server.handle_message(initialize_request());
  server.handle_message(initialized_notification());

  // Change strict mode
  server.handle_notification("workspace/didChangeConfiguration", json!({
    "settings": { "strict": true }
  }));

  // Verify Project.strict updated
  assert!(server.project.strict);
}
```

**3. Fourslash Test Integration**

**Extend existing infrastructure:**

Create `lsp.fourslash` test files:
```typescript
// @filename: test.ts
function foo() { }
// @showHover: 1, 8
foo^();
// @expect: {
//   "contents": ["function foo()"]
// }
```

---

## Summary of Findings

| Category | Current State | Gaps | Priority |
|----------|---------------|------|----------|
| **Unit Tests** | Strong (inline + dedicated files) | None | - |
| **Protocol Tests** | None | JSON-RPC validation, framing | **High** |
| **Lifecycle Tests** | None | init/initialized/shutdown/exit | **High** |
| **Concurrency** | None | Cancellation, parallel edits | Medium |
| **Configuration** | None | tsconfig hot-reload | Medium |
| **File Watch** | None | FS event integration | Low |
| **Editor E2E** | Manual only | VS Code, Neovim automation | **High** |
| **Benchmarks** | General compiler only | LSP-specific metrics | Medium |
| **Fourslash** | Checker exists | LSP features missing | Low |

---

## Recommended Action Plan

### Phase 1: Foundation (1-2 weeks)
1. Create `src/lsp/tests/protocol_tests.rs` - JSON-RPC validation
2. Create `src/lsp/tests/lifecycle_tests.rs` - Server state machine
3. Add LSP-specific benchmark suite `benches/lsp_bench.rs`
4. Expose `ProjectPerformance` metrics via custom LSP command

### Phase 2: Editor Integration (2-3 weeks)
1. Build VS Code test extension
2. Create Neovim test harness
3. Add E2E test framework using `langserver-rs`
4. Automate editor tests in CI

### Phase 3: Advanced Scenarios (3-4 weeks)
1. Concurrency and cancellation tests
2. Configuration change handling tests
3. File system watcher integration tests
4. Fourslash LSP test support

### Phase 4: Performance & Reliability (Ongoing)
1. Continuous benchmarking in CI
2. Memory leak detection
3. Fuzz testing for protocol robustness
4. Performance regression alerts

---

## Appendix: File Inventory

**Test Files:**
- `src/lsp/tests/project_tests.rs`
- `src/lsp/tests/code_actions_tests.rs`
- `src/lsp/tests/signature_help_tests.rs`
- `src/lsp/tests/tests.rs`

**Binaries:**
- `src/bin/tsz_lsp.rs` (LSP server)
- `src/bin/tsz_server.rs` (tsserver-compatible)

**Performance Infrastructure:**
- `src/lsp/project.rs` (lines 959-981: `ProjectPerformance`)
- `benches/` (7 benchmark files, zero LSP-specific)

---

**Report Prepared By:** Research Team 10
**Tools Used:** Direct code analysis, Gemini AI via `ask-gemini.mjs`
**Confidence:** High - Clear testing strategy with proven tools
