# TypeScript API Surface Mapping

**Generated:** January 2026  
**TypeScript Version:** 5.8.2  
**Purpose:** Guide implementation of `@tsz/typescript` compatibility shim

---

## Executive Summary

TypeScript exports **2,244 symbols** from its main module. For test harness compatibility, we need to implement a subset focused on:

- **P0 (Critical):** Program creation, diagnostics, emit - ~25 functions
- **P1 (Important):** AST traversal, TypeChecker, factory - ~200 methods
- **P2 (Utility):** Node type guards, syntax helpers - ~300 functions
- **P3 (Completeness):** Full API parity - remaining ~1,700 symbols

---

## Priority 0: Critical APIs

These APIs are required for basic test harness functionality.

### Program Creation

| Function | Type | Priority | Notes |
|----------|------|----------|-------|
| `createProgram` | function | P0 | Core entry point - creates TypeScript program |
| `createCompilerHost` | function | P0 | File system abstraction |
| `createSourceFile` | function | P0 | Parse single file to AST |
| `parseConfigFileTextToJson` | function | P0 | Parse tsconfig.json |
| `parseJsonConfigFileContent` | function | P0 | Convert JSON to CompilerOptions |
| `readConfigFile` | function | P0 | Read and parse config file |
| `getParsedCommandLineOfConfigFile` | function | P1 | Full config parsing |

### Diagnostics

| Function | Type | Priority | Notes |
|----------|------|----------|-------|
| `getPreEmitDiagnostics` | function | P0 | Get all diagnostics from program |
| `formatDiagnostic` | function | P0 | Format diagnostic to string |
| `formatDiagnosticsWithColorAndContext` | function | P1 | Pretty format with colors |
| `flattenDiagnosticMessageText` | function | P0 | Flatten chained messages |

### Emit

| Function | Type | Priority | Notes |
|----------|------|----------|-------|
| `transpileModule` | function | P0 | Single-file transpilation |
| `transpile` | function | P1 | Legacy transpile API |

---

## Priority 1: Program Interface (72 methods)

The `Program` object returned by `createProgram` has 72 methods.

### Critical Methods (must implement first)

```typescript
interface Program {
  // Diagnostics
  getSyntacticDiagnostics(sourceFile?: SourceFile): readonly Diagnostic[];
  getSemanticDiagnostics(sourceFile?: SourceFile): readonly Diagnostic[];
  getGlobalDiagnostics(): readonly Diagnostic[];
  getDeclarationDiagnostics(sourceFile?: SourceFile): readonly Diagnostic[];
  
  // Source files
  getSourceFiles(): readonly SourceFile[];
  getSourceFile(fileName: string): SourceFile | undefined;
  getRootFileNames(): readonly string[];
  
  // Type checking
  getTypeChecker(): TypeChecker;
  
  // Emit
  emit(targetSourceFile?: SourceFile, writeFile?: WriteFileCallback): EmitResult;
  
  // Options
  getCompilerOptions(): CompilerOptions;
  getCurrentDirectory(): string;
}
```

### Secondary Methods

```typescript
interface Program {
  // Config diagnostics
  getConfigFileParsingDiagnostics(): readonly Diagnostic[];
  getOptionsDiagnostics(): readonly Diagnostic[];
  
  // Resolution
  getResolvedModule(file: SourceFile, moduleName: string): ResolvedModule | undefined;
  getProjectReferences(): readonly ProjectReference[] | undefined;
  
  // Statistics
  getNodeCount(): number;
  getIdentifierCount(): number;
  getSymbolCount(): number;
  getTypeCount(): number;
}
```

---

## Priority 1: TypeChecker Interface (172 methods)

The `TypeChecker` is the most complex interface. Methods are grouped by category.

### Type Queries (114 get* methods)

**Most Important (implement first):**

| Method | Returns | Used By |
|--------|---------|---------|
| `getTypeAtLocation(node)` | Type | Hover, diagnostics |
| `getSymbolAtLocation(node)` | Symbol? | Go-to-definition |
| `getTypeOfSymbol(symbol)` | Type | Type inference |
| `getDeclaredTypeOfSymbol(symbol)` | Type | Interface/class types |
| `getPropertiesOfType(type)` | Symbol[] | Object members |
| `getSignaturesOfType(type, kind)` | Signature[] | Function types |
| `getReturnTypeOfSignature(sig)` | Type | Return types |
| `getBaseTypes(type)` | Type[] | Inheritance |
| `getApparentType(type)` | Type | Widened types |

**String Conversion:**

| Method | Returns | Notes |
|--------|---------|-------|
| `typeToString(type, node?, flags?)` | string | Format type for display |
| `symbolToString(symbol, node?, meaning?)` | string | Format symbol name |
| `signatureToString(sig, node?, flags?)` | string | Format signature |

**Intrinsic Types:**

| Method | Returns | Notes |
|--------|---------|-------|
| `getAnyType()` | Type | `any` |
| `getStringType()` | Type | `string` |
| `getNumberType()` | Type | `number` |
| `getBooleanType()` | Type | `boolean` |
| `getVoidType()` | Type | `void` |
| `getUndefinedType()` | Type | `undefined` |
| `getNullType()` | Type | `null` |
| `getNeverType()` | Type | `never` |
| `getUnknownType()` | Type | `unknown` |

### Type Predicates (19 is* methods)

| Method | Checks | Notes |
|--------|--------|-------|
| `isTypeAssignableTo(source, target)` | Assignability | Core type relation |
| `isArrayType(type)` | Array | Array check |
| `isTupleType(type)` | Tuple | Tuple check |
| `isArrayLikeType(type)` | Array-like | Includes tuples |
| `isNullableType(type)` | null/undefined | Nullable check |
| `isOptionalParameter(node)` | Optional | Parameter optionality |
| `isValidPropertyAccess(node, name)` | Access | Property accessibility |

### Resolution (3 resolve* methods)

| Method | Purpose |
|--------|---------|
| `resolveName(name, location, meaning, exclude)` | Symbol lookup |
| `resolveExternalModuleName(location, spec)` | Module resolution |
| `resolveExternalModuleSymbol(symbol)` | Module symbol |

### Creation (36 other methods)

| Method | Creates | Notes |
|--------|---------|-------|
| `createAnonymousType(symbol, members, ...)` | Type | Anonymous object type |
| `createArrayType(elementType)` | Type | Array type |
| `createPromiseType(type)` | Type | Promise<T> |
| `createSignature(...)` | Signature | Function signature |
| `createSymbol(flags, name)` | Symbol | Symbol object |

---

## Priority 2: AST Node Type Guards (~300 functions)

TypeScript exports extensive `is*` functions for AST node type checking.

### Categories

| Category | Count | Examples |
|----------|-------|----------|
| Declarations | 45 | `isFunctionDeclaration`, `isClassDeclaration`, `isVariableDeclaration` |
| Expressions | 40 | `isCallExpression`, `isBinaryExpression`, `isIdentifier` |
| Statements | 25 | `isIfStatement`, `isForStatement`, `isReturnStatement` |
| Types | 30 | `isTypeReferenceNode`, `isArrayTypeNode`, `isUnionTypeNode` |
| Modifiers | 15 | `isPublicKeyword`, `isStaticKeyword`, `isAsyncKeyword` |
| Other | 145 | Various utility predicates |

### Key Type Guards

```typescript
// Most commonly used in test harness
function isIdentifier(node: Node): node is Identifier;
function isFunctionDeclaration(node: Node): node is FunctionDeclaration;
function isClassDeclaration(node: Node): node is ClassDeclaration;
function isVariableDeclaration(node: Node): node is VariableDeclaration;
function isCallExpression(node: Node): node is CallExpression;
function isPropertyAccessExpression(node: Node): node is PropertyAccessExpression;
function isTypeReferenceNode(node: Node): node is TypeReferenceNode;
function isInterfaceDeclaration(node: Node): node is InterfaceDeclaration;
function isTypeAliasDeclaration(node: Node): node is TypeAliasDeclaration;
```

---

## Priority 2: AST Traversal

### Core Traversal Functions

| Function | Purpose | Priority |
|----------|---------|----------|
| `forEachChild(node, cb)` | Iterate direct children | P0 |
| `visitNode(node, visitor)` | Transform single node | P1 |
| `visitNodes(nodes, visitor)` | Transform node array | P1 |
| `visitEachChild(node, visitor, ctx)` | Deep transform | P1 |

### Node Factory

The `factory` object provides methods to create AST nodes:

```typescript
const factory = {
  // Identifiers
  createIdentifier(text: string): Identifier;
  
  // Literals
  createStringLiteral(text: string): StringLiteral;
  createNumericLiteral(value: string | number): NumericLiteral;
  
  // Declarations
  createFunctionDeclaration(...): FunctionDeclaration;
  createVariableDeclaration(...): VariableDeclaration;
  createClassDeclaration(...): ClassDeclaration;
  
  // Expressions
  createCallExpression(...): CallExpression;
  createPropertyAccessExpression(...): PropertyAccessExpression;
  
  // Types
  createTypeReferenceNode(...): TypeReferenceNode;
  createArrayTypeNode(...): ArrayTypeNode;
  
  // ... ~200 more methods
};
```

---

## Priority 3: Enums (142 total)

### Critical Enums

| Enum | Values | Notes |
|------|--------|-------|
| `SyntaxKind` | ~400 | Node type identifiers |
| `TypeFlags` | ~30 | Type classification |
| `SymbolFlags` | ~30 | Symbol classification |
| `ModifierFlags` | ~20 | Modifier bits |
| `NodeFlags` | ~20 | Node metadata |
| `DiagnosticCategory` | 4 | Error/Warning/Suggestion/Message |
| `ScriptTarget` | 12 | ES3-ESNext |
| `ModuleKind` | 12 | None/CommonJS/ES6/etc |

### Implementation Note

Enums can be directly re-exported from a constants module since they're just objects with numeric values.

---

## Priority 3: System Utilities

### `sys` Object

```typescript
const sys: System = {
  args: string[];
  newLine: string;
  useCaseSensitiveFileNames: boolean;
  write(s: string): void;
  readFile(path: string, encoding?: string): string | undefined;
  writeFile(path: string, data: string, writeByteOrderMark?: boolean): void;
  fileExists(fileName: string): boolean;
  directoryExists(directoryName: string): boolean;
  createDirectory(directoryName: string): void;
  getExecutingFilePath(): string;
  getCurrentDirectory(): string;
  getDirectories(path: string): string[];
  readDirectory(path: string, ...): string[];
  exit(exitCode?: number): void;
};
```

### Path Utilities

| Function | Purpose |
|----------|---------|
| `normalizePath(path)` | Normalize separators |
| `normalizeSlashes(path)` | Convert \\ to / |
| `getDirectoryPath(path)` | Get parent directory |
| `resolvePath(path, ...paths)` | Resolve relative paths |
| `combinePaths(path, ...paths)` | Join path segments |
| `hasExtension(path)` | Check for extension |
| `getBaseFileName(path)` | Get file name |
| `removeExtension(path, ext)` | Strip extension |

---

## Implementation Mapping to tsz

### Already Implemented in tsz (WASM)

| TypeScript API | tsz Equivalent | Status |
|----------------|----------------|--------|
| `createProgram` | `WasmProgram` | ✅ Partial |
| `Parser` | `Parser` class | ✅ Complete |
| `getSemanticDiagnostics` | `checkSourceFile()` | ✅ Complete |
| `getSyntacticDiagnostics` | `getDiagnosticsJson()` | ✅ Complete |
| `emit` | `emit()`, `emitModern()` | ✅ Complete |
| `transpileModule` | N/A | ❌ Not exposed |
| String comparison | Various | ✅ Complete |
| Path utilities | Various | ✅ Complete |

### Needs WASM Enhancement

| TypeScript API | Required Change |
|----------------|-----------------|
| `getTypeChecker()` | Expose `WasmTypeChecker` with full methods |
| `getSourceFiles()` | Return AST structure, not just file list |
| `forEachChild` | AST traversal from WASM |
| Type/Symbol objects | Serialize type data to JS |

### Shim-Only (No WASM Change)

| TypeScript API | Implementation |
|----------------|----------------|
| `SyntaxKind` enum | Re-export constants |
| `factory` | Build from primitives |
| `is*` type guards | Check SyntaxKind |
| `sys` | Use Node.js fs/path |

---

## Conformance Test Usage Analysis

From `conformance/src/worker.ts`, the harness uses:

```typescript
// Program creation
ts.createProgram(fileNames, compilerOptions, host);
ts.createCompilerHost(compilerOptions);
ts.createSourceFile(name, content, target, setParentNodes, scriptKind);

// Diagnostics
program.getSyntacticDiagnostics(sourceFile);
program.getSemanticDiagnostics(sourceFile);
program.getGlobalDiagnostics();

// Enums
ts.ScriptTarget.ES2020
ts.ModuleKind.ESNext
ts.ScriptKind.TS

// Constants
ts.ModuleResolutionKind.NodeJs
ts.JsxEmit.React
```

This confirms P0 APIs are sufficient for conformance testing.

---

## Next Steps

1. **Phase 1:** Implement P0 APIs (Program, diagnostics, emit)
2. **Phase 2:** Implement SourceFile AST serialization
3. **Phase 3:** Implement TypeChecker interface (top 20 methods first)
4. **Phase 4:** Implement remaining P1 APIs
5. **Ongoing:** Add P2/P3 APIs as needed for specific tests
