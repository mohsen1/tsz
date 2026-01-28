# TypeScript API Shim Architecture

**Status:** Phase 0 - Implementation Started  
**Created:** January 2026  
**Updated:** January 2026 (switched to Rust-native approach)

---

## Overview

TypeScript API compatibility is achieved through a **Rust-native approach** using `wasm-bindgen`. Instead of a TypeScript shim layer, we expose TypeScript-compatible APIs directly from Rust, compiled to WASM.

**Key benefits:**
- No JS-WASM boundary overhead for internal operations
- Type safety enforced by Rust
- Single source of truth (Rust code)
- Better performance (no serialization between JS shim and WASM)

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│  TypeScript Test Harness / User Code                                │
│  import { TsProgram, createTsProgram } from 'tsz-wasm';            │
└─────────────────────────────┬───────────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────────┐
│  tsz WASM Module (src/wasm_api/)                                    │
│  TypeScript-compatible APIs exposed via wasm-bindgen                │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  wasm_api/program.rs      - TsProgram, createTsProgram      │   │
│  │  wasm_api/type_checker.rs - TsTypeChecker                   │   │
│  │  wasm_api/source_file.rs  - TsSourceFile                    │   │
│  │  wasm_api/types.rs        - TsType, TsSymbol, TsSignature   │   │
│  │  wasm_api/diagnostics.rs  - TsDiagnostic, formatDiagnostic  │   │
│  │  wasm_api/enums.rs        - SyntaxKind, TypeFlags, etc.     │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                      │
│  ┌───────────────────────────▼─────────────────────────────────┐   │
│  │  Core Compiler (Existing - src/)                            │   │
│  │                                                              │   │
│  │  ParserState → BinderState → CheckerState → Emitter         │   │
│  │       ↓            ↓             ↓                          │   │
│  │  NodeArena    SymbolTable   TypeInterner                    │   │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

## Module Structure (Rust)

```
src/wasm_api/
├── mod.rs            # Module exports
├── program.rs        # TsProgram - main compilation unit
├── type_checker.rs   # TsTypeChecker - type query interface
├── source_file.rs    # TsSourceFile - AST access
├── types.rs          # TsType, TsSymbol, TsSignature
├── diagnostics.rs    # TsDiagnostic, formatting utilities
└── enums.rs          # SyntaxKind, TypeFlags, SymbolFlags, etc.
```

---

## Core Design Principles

### 1. Handle-Based Object Identity

TypeScript code assumes object identity: `sourceFile === program.getSourceFile(name)` must return `true`. We achieve this through handle management:

```typescript
// Internal handle registry
class HandleRegistry {
  private handles = new Map<number, WeakRef<object>>();
  private reverseMap = new WeakMap<object, number>();
  
  getOrCreate<T extends object>(handle: number, factory: () => T): T {
    const existing = this.handles.get(handle)?.deref();
    if (existing) return existing as T;
    
    const obj = factory();
    this.handles.set(handle, new WeakRef(obj));
    this.reverseMap.set(obj, handle);
    return obj;
  }
  
  getHandle(obj: object): number | undefined {
    return this.reverseMap.get(obj);
  }
}
```

### 2. Lazy Evaluation

TypeChecker methods are expensive. We defer WASM calls until actually needed:

```typescript
class TypeImpl implements Type {
  private _properties?: Symbol[];
  
  constructor(
    private handle: number,
    private checker: WasmTypeChecker,
  ) {}
  
  getProperties(): Symbol[] {
    if (!this._properties) {
      const raw = this.checker.getPropertiesOfType(this.handle);
      this._properties = raw.map(h => this.checker.registry.getSymbol(h));
    }
    return this._properties;
  }
}
```

### 3. Minimal Serialization

Only serialize data that crosses the WASM boundary. Keep complex structures in WASM memory, pass handles (u32) to JS.

**Serialized:**
- Diagnostic messages (strings)
- Type/symbol names (for display)
- Source positions (numbers)

**Handle-based:**
- AST nodes
- Types
- Symbols
- Signatures

---

## Module Structure

```
pkg/typescript-shim/
├── src/
│   ├── index.ts              # Main export (re-exports everything)
│   │
│   ├── core/
│   │   ├── wasm-bridge.ts    # WASM loading and low-level calls
│   │   ├── handles.ts        # Handle registry for object identity
│   │   ├── serialization.ts  # Type/AST serialization helpers
│   │   └── memory.ts         # WASM memory management
│   │
│   ├── compiler/
│   │   ├── program.ts        # Program implementation
│   │   ├── compiler-host.ts  # CompilerHost interface
│   │   ├── source-file.ts    # SourceFile implementation
│   │   └── emit.ts           # Emit implementation
│   │
│   ├── checker/
│   │   ├── type-checker.ts   # TypeChecker implementation
│   │   ├── type.ts           # Type interface implementation
│   │   ├── symbol.ts         # Symbol interface implementation
│   │   └── signature.ts      # Signature interface implementation
│   │
│   ├── ast/
│   │   ├── node.ts           # Node base implementation
│   │   ├── traversal.ts      # forEachChild, visitNode, etc.
│   │   ├── factory.ts        # Node factory
│   │   └── guards.ts         # is* type guards
│   │
│   ├── diagnostics/
│   │   ├── diagnostic.ts     # Diagnostic interfaces
│   │   └── formatter.ts      # Diagnostic formatting
│   │
│   ├── services/
│   │   ├── language-service.ts  # LanguageService (future)
│   │   └── document-registry.ts # DocumentRegistry (future)
│   │
│   ├── utilities/
│   │   ├── sys.ts            # System utilities
│   │   ├── path.ts           # Path utilities
│   │   └── strings.ts        # String comparison
│   │
│   └── types/
│       ├── enums.ts          # SyntaxKind, TypeFlags, etc.
│       ├── compiler-options.ts # CompilerOptions interface
│       └── public-api.d.ts   # TypeScript-compatible type definitions
│
├── wasm/
│   ├── tsz_wasm.js           # Generated WASM bindings
│   ├── tsz_wasm.d.ts         # TypeScript definitions for WASM
│   └── tsz_wasm_bg.wasm      # WASM binary
│
├── package.json
├── tsconfig.json
└── README.md
```

---

## WASM API Extensions

The current tsz WASM API needs extensions. Here's what we need to add to `src/lib.rs`:

### New WASM Exports

```rust
// src/wasm_api/mod.rs (new module)

/// Extended Program handle with AST access
#[wasm_bindgen]
pub struct WasmProgramHandle {
    program: Arc<MergedProgram>,
    checker: Option<WasmTypeCheckerHandle>,
}

#[wasm_bindgen]
impl WasmProgramHandle {
    /// Get all source file handles
    #[wasm_bindgen(js_name = getSourceFileHandles)]
    pub fn get_source_file_handles(&self) -> Vec<u32> {
        self.program.files.iter()
            .map(|f| f.handle as u32)
            .collect()
    }
    
    /// Get source file by name
    #[wasm_bindgen(js_name = getSourceFileHandle)]
    pub fn get_source_file_handle(&self, name: &str) -> Option<u32> {
        self.program.files.iter()
            .find(|f| f.file_name == name)
            .map(|f| f.handle as u32)
    }
    
    /// Get type checker handle (lazy creation)
    #[wasm_bindgen(js_name = getTypeChecker)]
    pub fn get_type_checker(&mut self) -> WasmTypeCheckerHandle {
        if self.checker.is_none() {
            self.checker = Some(WasmTypeCheckerHandle::new(&self.program));
        }
        self.checker.as_ref().unwrap().clone()
    }
}

/// TypeChecker with handle-based API
#[wasm_bindgen]
pub struct WasmTypeCheckerHandle {
    inner: Arc<CheckerState>,
}

#[wasm_bindgen]
impl WasmTypeCheckerHandle {
    /// Get type at location, returns type handle
    #[wasm_bindgen(js_name = getTypeAtLocation)]
    pub fn get_type_at_location(&self, node_handle: u32) -> u32 {
        let type_id = self.inner.get_type_of_node(NodeIndex(node_handle));
        type_id.0
    }
    
    /// Get symbol at location, returns symbol handle or u32::MAX
    #[wasm_bindgen(js_name = getSymbolAtLocation)]
    pub fn get_symbol_at_location(&self, node_handle: u32) -> u32 {
        self.inner.get_symbol_at_node(NodeIndex(node_handle))
            .map(|s| s.0)
            .unwrap_or(u32::MAX)
    }
    
    /// Format type as string
    #[wasm_bindgen(js_name = typeToString)]
    pub fn type_to_string(&self, type_handle: u32) -> String {
        self.inner.format_type(TypeId(type_handle))
    }
    
    /// Check if source type is assignable to target type
    #[wasm_bindgen(js_name = isTypeAssignableTo)]
    pub fn is_type_assignable_to(&self, source: u32, target: u32) -> bool {
        self.inner.is_assignable_to(TypeId(source), TypeId(target))
    }
    
    /// Get properties of type, returns array of symbol handles
    #[wasm_bindgen(js_name = getPropertiesOfType)]
    pub fn get_properties_of_type(&self, type_handle: u32) -> Vec<u32> {
        self.inner.get_properties_of_type(TypeId(type_handle))
            .into_iter()
            .map(|s| s.0)
            .collect()
    }
}

/// Source file with AST access
#[wasm_bindgen]
pub struct WasmSourceFileHandle {
    arena: Arc<NodeArena>,
    root: NodeIndex,
    file_name: String,
}

#[wasm_bindgen]
impl WasmSourceFileHandle {
    #[wasm_bindgen(getter)]
    pub fn fileName(&self) -> String {
        self.file_name.clone()
    }
    
    /// Get statement handles
    #[wasm_bindgen(js_name = getStatementHandles)]
    pub fn get_statement_handles(&self) -> Vec<u32> {
        let sf = self.arena.get_source_file(self.root);
        sf.statements.iter().map(|n| n.0).collect()
    }
    
    /// Get node kind
    #[wasm_bindgen(js_name = getNodeKind)]
    pub fn get_node_kind(&self, handle: u32) -> u16 {
        self.arena.get(NodeIndex(handle))
            .map(|n| n.kind)
            .unwrap_or(0)
    }
    
    /// Get node position
    #[wasm_bindgen(js_name = getNodePos)]
    pub fn get_node_pos(&self, handle: u32) -> u32 {
        self.arena.get(NodeIndex(handle))
            .map(|n| n.pos)
            .unwrap_or(0)
    }
    
    /// Get node end
    #[wasm_bindgen(js_name = getNodeEnd)]
    pub fn get_node_end(&self, handle: u32) -> u32 {
        self.arena.get(NodeIndex(handle))
            .map(|n| n.end)
            .unwrap_or(0)
    }
    
    /// Get children of node
    #[wasm_bindgen(js_name = getChildren)]
    pub fn get_children(&self, handle: u32) -> Vec<u32> {
        let mut children = Vec::new();
        if let Some(node) = self.arena.get(NodeIndex(handle)) {
            self.collect_children(node, &mut children);
        }
        children
    }
}
```

---

## Implementation Strategy

### Phase 1: Core Program API

**Week 1-2: Basic Infrastructure**

```typescript
// pkg/typescript-shim/src/core/wasm-bridge.ts
let wasmModule: any = null;

export async function initWasm(): Promise<void> {
  if (!wasmModule) {
    wasmModule = await import('../wasm/tsz_wasm.js');
  }
}

export function getWasm() {
  if (!wasmModule) throw new Error('WASM not initialized');
  return wasmModule;
}

// pkg/typescript-shim/src/compiler/program.ts
import { getWasm } from '../core/wasm-bridge';
import { HandleRegistry } from '../core/handles';

const registry = new HandleRegistry();

export function createProgram(
  rootNames: readonly string[] | CreateProgramOptions,
  options?: CompilerOptions,
  host?: CompilerHost,
  oldProgram?: Program,
): Program {
  const wasm = getWasm();
  
  // Normalize arguments
  let fileNames: readonly string[];
  let compilerOptions: CompilerOptions;
  let compilerHost: CompilerHost;
  
  if (Array.isArray(rootNames)) {
    fileNames = rootNames;
    compilerOptions = options!;
    compilerHost = host || createCompilerHost(compilerOptions);
  } else {
    fileNames = rootNames.rootNames;
    compilerOptions = rootNames.options;
    compilerHost = rootNames.host || createCompilerHost(compilerOptions);
  }
  
  // Create WASM program
  const wasmProgram = new wasm.WasmProgram();
  
  // Set compiler options
  wasmProgram.setCompilerOptions(JSON.stringify(compilerOptions));
  
  // Add files
  for (const fileName of fileNames) {
    const content = compilerHost.readFile(fileName);
    if (content !== undefined) {
      wasmProgram.addFile(fileName, content);
    }
  }
  
  // Create handle
  const handle = wasmProgram.getHandle();
  
  return registry.getOrCreate(handle, () => new ProgramImpl(
    wasmProgram,
    compilerOptions,
    compilerHost,
    registry,
  ));
}
```

### Phase 2: AST Serialization

**Week 3-4: Node Implementation**

```typescript
// pkg/typescript-shim/src/ast/node.ts
import { SyntaxKind } from '../types/enums';

export class NodeImpl implements Node {
  readonly kind: SyntaxKind;
  readonly pos: number;
  readonly end: number;
  readonly flags: NodeFlags;
  
  private _parent?: Node;
  private _children?: Node[];
  
  constructor(
    private handle: number,
    private sourceFile: SourceFileImpl,
    private wasm: WasmSourceFileHandle,
  ) {
    this.kind = wasm.getNodeKind(handle);
    this.pos = wasm.getNodePos(handle);
    this.end = wasm.getNodeEnd(handle);
    this.flags = wasm.getNodeFlags(handle);
  }
  
  get parent(): Node {
    if (!this._parent) {
      const parentHandle = this.wasm.getParent(this.handle);
      if (parentHandle !== 0xFFFFFFFF) {
        this._parent = this.sourceFile.getNode(parentHandle);
      }
    }
    return this._parent!;
  }
  
  getChildren(): Node[] {
    if (!this._children) {
      const handles = this.wasm.getChildren(this.handle);
      this._children = handles.map(h => this.sourceFile.getNode(h));
    }
    return this._children;
  }
  
  getText(): string {
    return this.sourceFile.text.substring(this.pos, this.end);
  }
}

// pkg/typescript-shim/src/ast/traversal.ts
export function forEachChild<T>(
  node: Node,
  cbNode: (node: Node) => T | undefined,
): T | undefined {
  const children = node.getChildren();
  for (const child of children) {
    const result = cbNode(child);
    if (result !== undefined) return result;
  }
  return undefined;
}
```

### Phase 3: TypeChecker

**Week 5-7: Type System Bridge**

```typescript
// pkg/typescript-shim/src/checker/type-checker.ts
export class TypeCheckerImpl implements TypeChecker {
  private typeCache = new Map<number, Type>();
  private symbolCache = new Map<number, Symbol>();
  
  constructor(
    private wasmChecker: WasmTypeCheckerHandle,
    private registry: HandleRegistry,
  ) {}
  
  getTypeAtLocation(node: Node): Type {
    const nodeHandle = this.registry.getHandle(node);
    if (nodeHandle === undefined) {
      throw new Error('Node not from this program');
    }
    
    const typeHandle = this.wasmChecker.getTypeAtLocation(nodeHandle);
    return this.getOrCreateType(typeHandle);
  }
  
  private getOrCreateType(handle: number): Type {
    let type = this.typeCache.get(handle);
    if (!type) {
      type = new TypeImpl(handle, this.wasmChecker, this);
      this.typeCache.set(handle, type);
    }
    return type;
  }
  
  typeToString(type: Type, enclosingDeclaration?: Node, flags?: TypeFormatFlags): string {
    const handle = this.registry.getHandle(type);
    if (handle === undefined) {
      return '<unknown>';
    }
    return this.wasmChecker.typeToString(handle);
  }
  
  isTypeAssignableTo(source: Type, target: Type): boolean {
    const sourceHandle = this.registry.getHandle(source);
    const targetHandle = this.registry.getHandle(target);
    if (sourceHandle === undefined || targetHandle === undefined) {
      return false;
    }
    return this.wasmChecker.isTypeAssignableTo(sourceHandle, targetHandle);
  }
}
```

---

## Memory Management

### WASM Memory Growth

tsz WASM memory can grow unbounded for large programs. We implement cleanup:

```typescript
// pkg/typescript-shim/src/core/memory.ts
export class ProgramManager {
  private programs = new Map<number, WeakRef<ProgramImpl>>();
  private cleanupQueue: number[] = [];
  
  register(handle: number, program: ProgramImpl): void {
    this.programs.set(handle, new WeakRef(program));
    
    // Set up finalizer
    const cleanup = () => this.cleanupQueue.push(handle);
    new FinalizationRegistry(cleanup).register(program, handle);
  }
  
  // Call periodically
  runCleanup(): void {
    const wasm = getWasm();
    for (const handle of this.cleanupQueue) {
      wasm.releaseProgram(handle);
    }
    this.cleanupQueue = [];
  }
}
```

### Explicit Disposal

For long-running processes (LSP), provide explicit cleanup:

```typescript
interface Disposable {
  dispose(): void;
}

class ProgramImpl implements Program, Disposable {
  dispose(): void {
    this.wasmProgram.free();
    this.registry.remove(this.handle);
  }
}
```

---

## Testing Strategy

### Unit Tests

```typescript
// pkg/typescript-shim/test/program.test.ts
import * as ts from '../src/index';

describe('createProgram', () => {
  it('creates a program from files', () => {
    const program = ts.createProgram(['test.ts'], {}, createTestHost({
      'test.ts': 'const x: number = 1;'
    }));
    
    expect(program.getSourceFiles()).toHaveLength(1);
    expect(program.getSourceFile('test.ts')).toBeDefined();
  });
  
  it('reports semantic diagnostics', () => {
    const program = ts.createProgram(['test.ts'], {}, createTestHost({
      'test.ts': 'const x: number = "hello";'
    }));
    
    const diags = program.getSemanticDiagnostics();
    expect(diags.length).toBeGreaterThan(0);
    expect(diags[0].code).toBe(2322); // Type not assignable
  });
});
```

### Conformance Testing

Run TypeScript's test suite against the shim:

```bash
# In TypeScript repo
npm install @tsz/typescript
npm test -- --use-tsz
```

### Differential Testing

Compare outputs against real TypeScript:

```typescript
function compareOutput(code: string, options: CompilerOptions) {
  const realTs = require('typescript');
  const tszTs = require('@tsz/typescript');
  
  const realDiags = getDiagnostics(realTs, code, options);
  const tszDiags = getDiagnostics(tszTs, code, options);
  
  expect(tszDiags.map(d => d.code)).toEqual(realDiags.map(d => d.code));
}
```

---

## Success Criteria

| Metric | Target | Timeline |
|--------|--------|----------|
| P0 APIs implemented | 100% | Week 4 |
| TypeChecker methods | 50+ | Week 7 |
| Conformance pass rate | 50% | Week 8 |
| Conformance pass rate | 80% | Week 12 |
| API coverage | 90% | Week 16 |

---

## Risk Mitigation

### Risk: TypeChecker Complexity

**Mitigation:** Implement methods on-demand based on test failures. Start with the 20 most-used methods.

### Risk: AST Identity Issues

**Mitigation:** Strict handle management with identity tests in CI.

### Risk: Performance Overhead

**Mitigation:** Profile early, cache aggressively, minimize WASM boundary crossings.

### Risk: WASM Memory Leaks

**Mitigation:** Implement FinalizationRegistry cleanup, add memory monitoring in tests.
