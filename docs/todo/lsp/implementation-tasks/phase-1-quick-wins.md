# Phase 1: Quick Wins - Implementation Tasks

**Duration:** 2 weeks
**Team Size:** 2-3 developers
**Goal:** Complete partially implemented features for maximum user value
**Expected Outcome:** 85% LSP feature parity

---

## Task 1: Wire Up Completions to LSP Server

**Effort:** 2-4 hours
**Impact:** ⭐⭐⭐⭐⭐ (Highest)
**Research:** [Team 4 Report](../research-findings/team-4-completions.md)
**Status:** Ready to implement

### Background

**MAJOR DISCOVERY:** The type-aware completion system is **ALREADY FULLY IMPLEMENTED** in `src/lsp/completions.rs`! The current LSP server just returns basic keywords because it hasn't been wired up.

### Implementation Steps

1. **Add `Project` field to `LspServer`** (5 lines)
   ```rust
   // src/bin/tsz_lsp.rs
   pub struct LspServer {
       project: Project,  // ADD THIS
       // ... existing fields
   }
   ```

2. **Update document handlers** (10 lines)
   ```rust
   fn handle_did_open(&mut self, params: DidOpenTextDocumentParams) {
       let uri = params.text_document.uri;
       let text = params.text_document.text;
       self.project.set_file(uri_to_path(&uri), text);
   }
   ```

3. **Replace `handle_completion`** (30 lines)
   ```rust
   fn handle_completion(&mut self, params: CompletionParams) -> Result<Value> {
       let uri = params.text_document_position.text_document.uri;
       let pos = params.text_document_position.position;

       let completions = self.project.get_completions(&uri_to_path(&uri), pos)?;

       Ok(serde_json::to_value(completions)?)
   }
   ```

4. **Add helper functions** (20 lines)
   ```rust
   fn uri_to_path(uri: &Url) -> String {
       uri.to_file_path().unwrap().to_str().unwrap().to_string()
   }
   ```

### Total Code Changes
- **Lines Added:** ~65
- **Lines Modified:** ~20
- **Files Changed:** 1 (`src/bin/tsz_lsp.rs`)

### Testing

1. Open TypeScript file in VS Code
2. Type `console.` - should see methods like `log`, `warn`, `error`
3. Type `arr.` on array - should see `map`, `filter`, `forEach`
4. Type import statement - should see auto-import suggestions

### Acceptance Criteria
- ✅ Member completions work (`obj.prop`)
- ✅ Identifier completions work (scope-based)
- ✅ Keyword completions work
- ✅ JSDoc documentation shown in completion detail
- ✅ All existing tests pass

---

## Task 2: Implement Inlay Hints Type Hints

**Effort:** 6-10 hours
**Impact:** ⭐⭐⭐⭐
**Research:** [Team 2 Report](../research-findings/team-2-inlay-hints.md)
**Status:** Clear implementation path

### Background

Parameter name hints work perfectly. Type hints for variables like `let x = 1` → `let x: number = 1` need TypeInterner integration.

### Implementation Steps

1. **Update `InlayHintsProvider` struct** (5 lines)
   ```rust
   // src/lsp/inlay_hints.rs
   pub struct InlayHintsProvider<'a> {
       arena: &'a NodeArena,
       binder: &'a BinderState,
       checker: &'a CheckerState,  // ADD THIS
       type_interner: &'a TypeInterner,  // ADD THIS
       line_map: &'a LineMap,
       file_name: String,
       source: &'a str,
   }
   ```

2. **Implement `collect_type_hints()`** (100 lines)
   ```rust
   fn collect_type_hints(&self, range: Range) -> Vec<InlayHint> {
       // For each variable declaration:
       // 1. Check if it has type annotation → Skip if yes
       // 2. Check if it has initializer → Skip if no
       // 3. Get inferred type: checker.get_type_of_node(decl)
       // 4. Format type: checker.format_type(type_id)
       // 5. Filter unhelpful types (any, unknown)
       // 6. Add hint: ": <type>"
   }
   ```

3. **Wire up in `provide_inlay_hints()`** (10 lines)
   ```rust
   pub fn provide_inlay_hints(&mut self, params: InlayHintParams) -> Result<Vec<InlayHint>> {
       // ... existing code for parameter hints ...

       let mut checker = CheckerState::new(/* ... */);
       let type_interner = checker.type_interner();

       let provider = InlayHintsProvider::new_with_types(
           arena, &binder, &checker, &type_interner, &line_map, file_name, source
       );

       Ok(provider.provide_inlay_hints(params.text_document.range))
   }
   ```

4. **Add `get_inlay_hints()` to `Project`** (15 lines)
   ```rust
   pub fn get_inlay_hints(&mut self, file_name: &str, range: Range) -> Option<Vec<InlayHint>> {
       let file = self.files.get(file_name)?;
       let type_cache = file.type_cache.clone();  // REUSE

       let hints = file.get_inlay_hints(range, &mut type_cache)?;

       // Update cache
       let file = self.files.get_mut(file_name)?;
       file.type_cache = type_cache;

       Some(hints)
   }
   ```

### Total Code Changes
- **Lines Added:** ~150
- **Lines Modified:** ~30
- **Files Changed:** 2 (`src/lsp/inlay_hints.rs`, `src/lsp/project.rs`)

### Testing

1. `const x = 1` → `const x: number = 1`
2. `const s = "hello"` → `const s: string = "hello"`
3. `const arr = [1, 2]` → `const arr: number[] = [1, 2]`
4. `let fn = (x: number) => x` → `let fn: (x: number) => number = (x: number) => x`
5. No hint if type annotation present
6. No hint for `any` or `unknown` types

### Acceptance Criteria
- ✅ Type hints shown for `const` and `let` without explicit types
- ✅ Object types shown: `{ name: string, age: number }`
- ✅ Array types shown: `number[]`, `string[][]`
- ✅ Function types shown: `(arg: Type) => ReturnType`
- ✅ Generic types shown: `Array<T>`, `Promise<T>`
- ✅ No hint when type is `any` or `unknown`
- ✅ No hint when type annotation present
- ✅ No hint when no initializer
- ✅ Parameter name hints still work
- ✅ Performance: <50ms cold, <10ms warm per file

---

## Task 3: Wire Up Hover with TypeInterner

**Effort:** 1-2 days
**Impact:** ⭐⭐⭐⭐⭐
**Research:** [Team 3 Report](../research-findings/team-3-hover.md)
**Status:** Infrastructure complete, needs wiring

### Background

`HoverProvider` is fully implemented and tested. LSP server stub returns null. Just needs TypeInterner integration (proven pattern via `Project`).

### Implementation Steps

**Option A: Extend `DocumentState`** (RECOMMENDED)

1. **Add TypeInterner to `DocumentState`** (10 lines)
   ```rust
   // src/bin/tsz_lsp.rs
   struct DocumentState {
       content: String,
       line_map: LineMap,
       type_interner: TypeInterner,  // ADD THIS
       type_cache: Option<TypeCache>,  // ADD THIS
       version: i32,
   }
   ```

2. **Initialize TypeInterner in `handle_did_open`** (15 lines)
   ```rust
   fn handle_did_open(&mut self, params: DidOpenTextDocumentParams) {
       let uri = params.text_document.uri;
       let text = params.text_document.text;
       let line_map = LineMap::new(&text);

       let mut binder = BinderState::new();
       // ... bind AST ...

       let mut checker = CheckerState::new();
       // ... type check ...

       let type_interner = checker.extract_interner();
       let type_cache = Some(checker.extract_cache());

       self.documents.insert(uri, DocumentState {
           content: text,
           line_map,
           type_interner,
           type_cache,
           version: 1,
       });
   }
   ```

3. **Implement `handle_hover`** (30 lines)
   ```rust
   fn handle_hover(&mut self, params: HoverParams) -> Result<Value> {
       let uri = params.text_document_position.text_document.uri;
       let doc = self.documents.get(&uri).ok_or_else(|| error("document not found"))?;

       let pos = params.text_document_position.position;

       let provider = HoverProvider::new(
           &arena,
           &binder,
           &doc.type_interner,
           &doc.line_map,
           uri_to_path(&uri),
           &doc.content,
       );

       let hover = provider.get_hover(root, pos)?;

       Ok(serde_json::to_value(hover)?)
   }
   ```

4. **Update caches in `handle_did_change`** (10 lines)
   ```rust
   fn handle_did_change(&mut self, params: DidChangeTextDocumentParams) {
       // ... apply edits ...
       doc.type_cache = None;  // Invalidate
       // ... re-type-check ...
       doc.type_cache = Some(checker.extract_cache());
   }
   ```

### Total Code Changes
- **Lines Added:** ~80
- **Lines Modified:** ~40
- **Files Changed:** 1 (`src/bin/tsz_lsp.rs`)

### Testing

1. Hover over `const x = 1` → shows `const x: number = 1`
2. Hover over function name → shows signature
3. Hover over variable → shows type and JSDoc
4. Hover over import → shows resolved type
5. Hover over class method → shows signature

### Acceptance Criteria
- ✅ Hover shows type information
- ✅ Hover shows function signatures
- ✅ Hover shows JSDoc documentation
- ✅ Works with incremental file updates
- ✅ Performance: <100ms for typical files
- ✅ All existing tests pass
- ✅ No regressions in other LSP features

---

## Task 4: Fix Signature Help Incomplete Member Calls

**Effort:** 2-3 hours
**Impact:** ⭐⭐⭐⭐
**Research:** [Team 1 Report](../research-findings/team-1-signature-help.md)
**Status:** Fallback infrastructure exists

### Background

Signature help fails for `obj.method(|` because type checking can't handle incomplete property access. JSDoc extraction infrastructure already exists, just needs to be used as fallback.

### Implementation Steps

1. **Modify `get_signature_help_internal`** (20 lines)
   ```rust
   // src/lsp/signature_help.rs (lines 218-233)
   fn get_signature_help_internal(&mut self, call_expr: NodeIndex) -> Option<SignatureHelp> {
       // ... existing code to get call type ...

       let call_type = self.checker.get_type_of_node(call_expr.expression)?;

       if call_type == TypeId::ERROR || call_type == TypeId::UNKNOWN {
           // NEW: Fallback for incomplete member calls
           if let Some(property_access) = self.get_property_access(call_expr.expression) {
               return self.signature_help_for_property_access_fallback(property_access);
           }
       }

       // ... existing type-based resolution ...
   }
   ```

2. **Add fallback method** (50 lines)
   ```rust
   fn signature_help_for_property_access_fallback(&mut self, prop_access: NodeIndex) -> Option<SignatureHelp> {
       // 1. Extract property name from `obj.method`
       let method_name = extract_property_name(prop_access)?;

       // 2. Find object type
       let object_type = self.checker.get_type_of_node(get_object_expression(prop_access))?;

       // 3. Find method declaration in type
       let method_decl = self.find_method_declaration(object_type, &method_name)?;

       // 4. Extract signature from JSDoc
       let signature = self.signature_documentation_for_property_access(method_decl)?;

       Some(SignatureHelp {
           signatures: vec![signature],
           active_signature: 0,
           active_parameter: self.detect_active_parameter(prop_access),
       })
   }
   ```

3. **Add helper methods** (30 lines)
   ```rust
   fn get_property_access(&self, node: NodeIndex) -> Option<NodeIndex> {
       // Return node if it's PropertyAccessExpression
   }

   fn extract_property_name(&self, prop_access: NodeIndex) -> Option<String> {
       // Extract method name from `obj.method`
   }

   fn find_method_declaration(&self, type_id: TypeId, method_name: &str) -> Option<NodeIndex> {
       // Search class/interface for method
   }
   ```

### Total Code Changes
- **Lines Added:** ~100
- **Lines Modified:** ~20
- **Files Changed:** 1 (`src/lsp/signature_help.rs`)

### Testing

1. Test ignored test case: `obj.method(|` should show signature
2. Complete calls still work: `obj.method(arg1, |`
3. Direct calls still work: `method(|`
4. Overload resolution still works

### Acceptance Criteria
- ✅ `obj.method(|` shows signature help
- ✅ `obj.method(arg1, |` shows signature help with active parameter
- ✅ Complete calls still work correctly
- ✅ Direct calls still work correctly
- ✅ Overload resolution still works
- ✅ Ignored test now passes

---

## Task 5: Add Workspace Symbols

**Effort:** 2 days
**Impact:** ⭐⭐⭐⭐⭐
**Research:** [Team 7 Report](../research-findings/team-7-missing-features.md)
**Status:** Infrastructure 90% complete

### Background

`SymbolIndex` exists and indexes all symbols. Just need to expose via LSP `workspace/symbol` request.

### Implementation Steps

1. **Add symbol indexing to `Project`** (20 lines)
   ```rust
   // src/lsp/project.rs
   impl Project {
       pub fn new_with_index() -> Self {
           let mut project = Self::new();
           project.symbol_index = Some(SymbolIndex::new());
           project
       }

       pub fn index_file_symbols(&mut self, file_name: &str) {
           if let Some(index) = &mut self.symbol_index {
               let file = self.files.get(file_name)?;
               index.update_file(file_name, &file.binder);
           }
       }
   }
   ```

2. **Implement `workspace/symbol` handler** (40 lines)
   ```rust
   // src/bin/tsz_lsp.rs
   fn handle_workspace_symbol(&mut self, params: WorkspaceSymbolParams) -> Result<Value> {
       let query = params.query.to_lowercase();

       let symbols = self.project.symbol_index.as_ref()
           .ok_or_else(|| error("symbol index not initialized"))?
           .search_symbols(&query)?;

       let symbol_infos = symbols.into_iter()
           .map(|(name, kind, location)| SymbolInformation {
               name,
               kind,
               location,
               ..Default::default()
           })
           .collect::<Vec<_>>();

       Ok(serde_json::to_value(symbol_infos)?)
   }
   ```

3. **Register capability in `initialize`** (5 lines)
   ```rust
   fn server_capabilities() -> ServerCapabilities {
       ServerCapabilities {
           workspace_symbol: Some(WorkspaceSymbolServerCapabilities {
               symbol_kind: Some(SymbolKindCapability {
                   value_set: Kind::all().to_vec(),
               }),
               ..Default::default()
           }),
           // ... existing capabilities ...
       }
   }
   ```

### Total Code Changes
- **Lines Added:** ~80
- **Lines Modified:** ~10
- **Files Changed:** 2 (`src/lsp/project.rs`, `src/bin/tsz_lsp.rs`)

### Testing

1. Open multi-file project
2. Execute "Go to Symbol in Workspace" (Ctrl+T in VS Code)
3. Type query: "foo" - shows all symbols named "foo"
4. Type query: "bar" - shows all symbols named "bar"
5. Click result - navigates to location

### Acceptance Criteria
- ✅ Search across all project files
- ✅ Fuzzy matching support
- ✅ Symbol kinds shown (function, class, variable, etc.)
- ✅ Click navigates to definition
- ✅ Performance: <100ms for 1000 symbols
- ✅ Handles re-exports correctly

---

## Task 6: Add Document Links

**Effort:** 1 day
**Impact:** ⭐⭐
**Research:** [Team 7 Report](../research-findings/team-7-missing-features.md)
**Status:** Trivial implementation

### Background

Module resolution already exists. Just need to extract import/export paths and return as clickable links.

### Implementation Steps

1. **Implement `DocumentLinksProvider`** (80 lines)
   ```rust
   // src/lsp/document_links.rs (new file)
   pub struct DocumentLinksProvider<'a> {
       arena: &'a NodeArena,
       line_map: &'a LineMap,
       file_name: String,
       source: &'a str,
   }

   impl<'a> DocumentLinksProvider<'a> {
       pub fn get_document_links(&self, root: NodeIndex) -> Vec<DocumentLink> {
           let mut links = Vec::new();

           // Find all ImportDeclaration nodes
           for import_decl in self.find_imports(root) {
               let module_specifier = self.extract_module_specifier(import_decl);
               let range = self.get_module_specifier_range(import_decl);

               let target = self.resolve_module(&module_specifier);

               if let Some(target_uri) = target {
                   links.push(DocumentLink {
                       range,
                       target: target_uri,
                       tooltip: Some("Click to navigate".to_string()),
                   });
               }
           }

           links
       }

       fn resolve_module(&self, specifier: &str) -> Option<String> {
           // Use existing module resolution from project_operations.rs
           let resolved = resolve_module_specifier(self.file_name, specifier)?;
           Some(format!("file://{}", resolved))
       }
   }
   ```

2. **Wire up in LSP server** (20 lines)
   ```rust
   // src/bin/tsz_lsp.rs
   fn handle_document_link(&mut self, params: DocumentLinkParams) -> Result<Value> {
       let uri = params.text_document.uri;
       let doc = self.documents.get(&uri).ok_or_else(|| error("not found"))?;

       let provider = DocumentLinksProvider::new(
           &arena,
           &doc.line_map,
           uri_to_path(&uri),
           &doc.content,
       );

       let links = provider.get_document_links(root)?;

       Ok(serde_json::to_value(links)?)
   }
   ```

3. **Register capability** (5 lines)
   ```rust
   fn server_capabilities() -> ServerCapabilities {
       ServerCapabilities {
           document_link_provider: Some(DocumentLinkOptions {
               resolve_provider: Some(false),
               ..Default::default()
           }),
           // ... existing ...
       }
   }
   ```

### Total Code Changes
- **Lines Added:** ~120
- **Lines Modified:** ~10
- **Files Changed:** 2 (new `src/lsp/document_links.rs`, `src/bin/tsz_lsp.rs`)

### Testing

1. Open file with imports:
   ```typescript
   import { foo } from './utils';
   import * as fs from 'fs';
   ```
2. Execute "Open Link" on `./utils`
3. Should navigate to `utils.ts`
4. Execute "Open Link" on `fs`
5. Should navigate to `fs.d.ts` (or show error if not found)

### Acceptance Criteria
- ✅ Import specifiers shown as clickable links
- ✅ Export specifiers shown as clickable links
- ✅ Click navigates to target file
- ✅ Command+Click works (VS Code)
- ✅ Relative paths resolved correctly
- ✅ Node modules resolved correctly
- ✅ Shows tooltip: "Click to navigate"
- ✅ Performance: <50ms per file

---

## Task 7: Add Standard Library Symbols to Global Scope

**Effort:** 2-3 days
**Impact:** ⭐⭐⭐⭐
**Research:** [Team 6 Report](../research-findings/team-6-fourslash-tests.md)
**Status:** Infrastructure ready

### Background

Standard library globals (`console`, `Array`, `Promise`, etc.) not in global scope. Breaks 15% of fourslash tests. Need to populate global scope.

### Implementation Steps

1. **Create global symbol definitions** (200 lines)
   ```rust
   // src/binder/globals.rs (new file)
   pub fn create_global_symbols(binder: &mut BinderState) {
       // DOM globals
       bind_global(binder, "console", create_console_type());
       bind_global(binder, "window", create_window_type());
       bind_global(binder, "document", create_document_type());

       // ES6 globals
       bind_global(binder, "Array", create_array_type());
       bind_global(binder, "Object", create_object_type());
       bind_global(binder, "Promise", create_promise_type());
       bind_global(binder, "Map", create_map_type());
       bind_global(binder, "Set", create_set_type());
       bind_global(binder, "JSON", create_json_type());

       // Node.js globals
       bind_global(binder, "process", create_process_type());
       bind_global(binder, "Buffer", create_buffer_type());
       bind_global(binder, "require", create_require_type());

       // TypeScript globals
       bind_global(binder, "undefined", TypeId::UNDEFINED);
       bind_global(binder, "NaN", TypeId::NAN);
       bind_global(binder, "Infinity", TypeId::INFINITY);
   }
   ```

2. **Integrate into binding** (10 lines)
   ```rust
   // src/binder/mod.rs
   impl BinderState {
       pub fn new() -> Self {
           let mut binder = Self {
               // ... existing fields ...
           };

           // NEW: Populate global scope
           create_global_symbols(&mut binder);

           binder
       }
   }
   ```

3. **Load type definitions** (100 lines)
   ```rust
   // src/binder/lib_loader.rs (new file)
   pub fn load_lib_d_ts(project: &mut Project, lib_name: &str) {
       let content = get_lib_definition(lib_name);
       project.set_file(format!("lib.{}", lib_name), content);
   }

   fn get_lib_definition(lib_name: &str) -> String {
       match lib_name {
           "es5" => include_str!("../../lib/es5.d.ts").to_string(),
           "es6" => include_str!("../../lib/es6.d.ts").to_string(),
           "dom" => include_str!("../../lib/dom.d.ts").to_string(),
           _ => String::new(),
       }
   }
   ```

### Total Code Changes
- **Lines Added:** ~350
- **Lines Modified:** ~20
- **Files Changed:** 3 (new `src/binder/globals.rs`, new `src/binder/lib_loader.rs`, `src/binder/mod.rs`)

### Testing

1. `console.log("hello")` - no "undefined" error
2. `const arr = new Array()` - works
3. `const prom = new Promise()` - works
4. `process.exit()` - works (in Node context)
5. Fourslash tests: +10% pass rate

### Acceptance Criteria
- ✅ DOM globals available (`console`, `window`, `document`)
- ✅ ES6 globals available (`Array`, `Object`, `Promise`, `Map`, `Set`)
- ✅ Node.js globals available (`process`, `Buffer`, `require`)
- ✅ TypeScript globals available (`undefined`, `NaN`, `Infinity`)
- ✅ Type information available for all globals
- ✅ JSDoc available for common methods
- ✅ Fourslash pass rate increases by 10%
- ✅ No performance regression in binding

---

## Summary

**Total Effort:** ~10 days
**Total Code Changes:** ~1,000 lines
**Features Completed:** 7
**LSP Parity:** 75% → 85%

### Quick Wins by Day

| Day | Tasks | Impact |
|-----|-------|--------|
| 1 | Wire up completions (2-4 hours) | ⭐⭐⭐⭐⭐ |
| 1-2 | Fix signature help (2-3 hours) | ⭐⭐⭐⭐ |
| 3-5 | Implement inlay hints (6-10 hours) | ⭐⭐⭐⭐ |
| 6 | Add document links (1 day) | ⭐⭐ |
| 7-8 | Wire up hover (1-2 days) | ⭐⭐⭐⭐⭐ |
| 9-10 | Add workspace symbols (2 days) | ⭐⭐⭐⭐⭐ |
| 11-13 | Add standard library symbols (2-3 days) | ⭐⭐⭐⭐ |

### Success Metrics

- ✅ All 7 features working in VS Code
- ✅ User-visible improvements in daily editing
- ✅ All tests passing
- ✅ No regressions
- ✅ Performance targets met

### Next Phase

After completing these quick wins, proceed to **Phase 2: Type System Foundation** to fix the underlying type checker gaps that limit LSP accuracy.
