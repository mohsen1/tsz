# TSC Directive Reference

This documents ALL directives supported by TypeScript's conformance test infrastructure, extracted directly from the TypeScript compiler source code (`src/harness/harnessIO.ts` and `src/compiler/commandLineParser.ts`).

### Directive Syntax

Directives are parsed using the regex pattern:
```
/^\/{2}\s*@(\w+)\s*:\s*([^\r\n]*)/gm
```

This matches comments like:
- `// @strict: true`
- `// @target: ES2020`
- `// @filename: lib.ts`

**Note:** Directive names are **case-insensitive** (e.g., `@FileName`, `@filename`, and `@FILENAME` are equivalent).

---

### Test Harness-Specific Directives

These directives are specific to the test infrastructure and NOT part of `tsconfig.json`:

| Directive | Type | Description |
|-----------|------|-------------|
| `@filename` | string | Starts a new file section in multi-file tests |
| `@allowNonTsExtensions` | boolean | Allow files without .ts/.tsx extensions |
| `@useCaseSensitiveFileNames` | boolean | Use case-sensitive file name matching |
| `@baselineFile` | string | Specify custom baseline file name |
| `@noErrorTruncation` | boolean | Don't truncate error messages in baselines |
| `@suppressOutputPathCheck` | boolean | Skip output path validation |
| `@noImplicitReferences` | boolean | Don't auto-include referenced files |
| `@currentDirectory` | string | Set virtual current working directory |
| `@symlink` | string | Create a symlink (format: `target -> link`) |
| `@link` | string | Alias for `@symlink` |
| `@noTypesAndSymbols` | boolean | Skip type/symbol baselines |
| `@fullEmitPaths` | boolean | Show full paths in emit baselines |
| `@noCheck` | boolean | Disable semantic checking (parse only) |
| `@reportDiagnostics` | boolean | Enable diagnostics in transpile baselines |
| `@captureSuggestions` | boolean | Include suggestions in error baselines |
| `@typeScriptVersion` | string | Specify minimum TypeScript version for test |

---

### Type Checking Options (Strict Family)

| Directive | Type | Default | Description |
|-----------|------|---------|-------------|
| `@strict` | boolean | false | Enable all strict type-checking options |
| `@noImplicitAny` | boolean | false (true if strict) | Error on expressions with implied `any` type |
| `@strictNullChecks` | boolean | false (true if strict) | Include `null` and `undefined` in type checking |
| `@strictFunctionTypes` | boolean | false (true if strict) | Stricter function type checking |
| `@strictBindCallApply` | boolean | false (true if strict) | Check `bind`, `call`, `apply` arguments |
| `@strictPropertyInitialization` | boolean | false (true if strict) | Check class property initialization |
| `@strictBuiltinIteratorReturn` | boolean | false (true if strict) | Built-in iterators return `undefined` not `any` |
| `@noImplicitThis` | boolean | false (true if strict) | Error on `this` with implied `any` type |
| `@useUnknownInCatchVariables` | boolean | false (true if strict) | Catch variables are `unknown` not `any` |
| `@alwaysStrict` | boolean | false (true if strict) | Emit `"use strict"` in output |

---

### Additional Type Checking Options

| Directive | Type | Default | Description |
|-----------|------|---------|-------------|
| `@noUnusedLocals` | boolean | false | Error on unused local variables |
| `@noUnusedParameters` | boolean | false | Error on unused function parameters |
| `@exactOptionalPropertyTypes` | boolean | false | Don't add `undefined` to optional properties |
| `@noImplicitReturns` | boolean | false | Error when not all paths return a value |
| `@noFallthroughCasesInSwitch` | boolean | false | Error on fallthrough cases in switch |
| `@noUncheckedIndexedAccess` | boolean | false | Add `undefined` to index signature results |
| `@noImplicitOverride` | boolean | false | Require `override` modifier for overrides |
| `@noPropertyAccessFromIndexSignature` | boolean | false | Require bracket notation for index signatures |
| `@allowUnusedLabels` | boolean | undefined | Disable error for unused labels |
| `@allowUnreachableCode` | boolean | undefined | Disable error for unreachable code |

---

### Target and Language Options

| Directive | Type | Values | Description |
|-----------|------|--------|-------------|
| `@target` | enum | ES3, ES5, ES2015-ES2024, ESNext | ECMAScript target version |
| `@lib` | list | es5, es6, es2015-es2024, esnext, dom, dom.iterable, dom.asynciterable, webworker, webworker.importscripts, webworker.iterable, webworker.asynciterable, scripthost, decorators, decorators.legacy, + many granular libs | Library files to include |
| `@noLib` | boolean | false | Don't include default lib.d.ts |
| `@jsx` | enum | preserve, react, react-native, react-jsx, react-jsxdev | JSX code generation mode |
| `@jsxFactory` | string | "React.createElement" | JSX factory function |
| `@jsxFragmentFactory` | string | "React.Fragment" | JSX fragment reference |
| `@jsxImportSource` | string | "react" | Module for JSX factory imports |
| `@useDefineForClassFields` | boolean | true for ES2022+ | Use `define` for class fields |
| `@experimentalDecorators` | boolean | false | Enable legacy decorator support |
| `@emitDecoratorMetadata` | boolean | false | Emit decorator metadata |
| `@moduleDetection` | enum | auto, legacy, force | How to detect module files |

---

### Module Options

| Directive | Type | Values | Description |
|-----------|------|--------|-------------|
| `@module` | enum | none, commonjs, amd, system, umd, es6, es2015, es2020, es2022, esnext, node16, node18, node20, nodenext, preserve | Module code generation |
| `@moduleResolution` | enum | node10, node, classic, node16, nodenext, bundler | Module resolution strategy |
| `@baseUrl` | string | | Base directory for non-relative imports |
| `@paths` | object | | Path mapping for module imports |
| `@rootDirs` | list | | Virtual root directories |
| `@typeRoots` | list | | Directories for type definitions |
| `@types` | list | | Type packages to include |
| `@allowSyntheticDefaultImports` | boolean | true | Allow `import x from 'y'` without default export |
| `@esModuleInterop` | boolean | true | Emit helpers for CommonJS/ESM interop |
| `@preserveSymlinks` | boolean | false | Don't resolve symlinks to real paths |
| `@allowUmdGlobalAccess` | boolean | false | Allow UMD globals from modules |
| `@moduleSuffixes` | list | | Suffixes for module resolution |
| `@allowImportingTsExtensions` | boolean | false | Allow imports with .ts/.tsx extensions |
| `@rewriteRelativeImportExtensions` | boolean | false | Rewrite .ts extensions in output |
| `@resolvePackageJsonExports` | boolean | varies | Use package.json exports field |
| `@resolvePackageJsonImports` | boolean | varies | Use package.json imports field |
| `@customConditions` | list | | Custom conditions for package.json resolution |
| `@noUncheckedSideEffectImports` | boolean | true | Check side effect imports |
| `@resolveJsonModule` | boolean | false | Allow importing .json files |
| `@allowArbitraryExtensions` | boolean | false | Allow any extension with .d.ts |
| `@noResolve` | boolean | false | Don't resolve triple-slash references |

---

### JavaScript Support Options

| Directive | Type | Default | Description |
|-----------|------|---------|-------------|
| `@allowJs` | boolean | false | Allow JavaScript files |
| `@checkJs` | boolean | false | Type-check JavaScript files |
| `@maxNodeModuleJsDepth` | number | 0 | Max depth for checking node_modules JS |

---

### Emit Options

| Directive | Type | Default | Description |
|-----------|------|---------|-------------|
| `@noEmit` | boolean | false | Don't emit output files |
| `@declaration` | boolean | false | Generate .d.ts files |
| `@declarationMap` | boolean | false | Generate sourcemaps for .d.ts |
| `@emitDeclarationOnly` | boolean | false | Only emit .d.ts files |
| `@sourceMap` | boolean | false | Generate .js.map files |
| `@inlineSourceMap` | boolean | false | Include sourcemap in .js files |
| `@inlineSources` | boolean | false | Include source in sourcemaps |
| `@outFile` | string | | Concatenate output to single file |
| `@outDir` | string | | Output directory |
| `@rootDir` | string | | Root directory of source files |
| `@declarationDir` | string | | Output directory for .d.ts files |
| `@removeComments` | boolean | false | Remove comments from output |
| `@importHelpers` | boolean | false | Import helpers from tslib |
| `@downlevelIteration` | boolean | false | Emit ES5-compliant iteration |
| `@preserveConstEnums` | boolean | false | Keep const enums in output |
| `@stripInternal` | boolean | false | Don't emit `@internal` members |
| `@noEmitHelpers` | boolean | false | Don't emit helper functions |
| `@noEmitOnError` | boolean | false | Don't emit if errors exist |
| `@emitBOM` | boolean | false | Emit UTF-8 BOM |
| `@newLine` | enum | crlf, lf | Line ending style |
| `@sourceRoot` | string | | Root path for debugger sources |
| `@mapRoot` | string | | Root path for sourcemaps |

---

### Project Options

| Directive | Type | Default | Description |
|-----------|------|---------|-------------|
| `@incremental` | boolean | false | Enable incremental compilation |
| `@composite` | boolean | false | Enable project references |
| `@tsBuildInfoFile` | string | ".tsbuildinfo" | Build info file path |
| `@disableSourceOfProjectReferenceRedirect` | boolean | false | Don't prefer source files |
| `@disableSolutionSearching` | boolean | false | Opt out of multi-project checking |
| `@disableReferencedProjectLoad` | boolean | false | Reduce auto-loaded projects |

---

### Interop Constraints

| Directive | Type | Default | Description |
|-----------|------|---------|-------------|
| `@isolatedModules` | boolean | false | Ensure each file is independently transpilable |
| `@verbatimModuleSyntax` | boolean | false | Don't transform import/export syntax |
| `@isolatedDeclarations` | boolean | false | Require explicit type annotations on exports |
| `@erasableSyntaxOnly` | boolean | false | Disallow non-erasable TypeScript constructs |
| `@forceConsistentCasingInFileNames` | boolean | true | Enforce consistent import casing |

---

### Library and Completeness Options

| Directive | Type | Default | Description |
|-----------|------|---------|-------------|
| `@skipLibCheck` | boolean | false | Skip type checking all .d.ts files |
| `@skipDefaultLibCheck` | boolean | false | Skip type checking default lib files |
| `@libReplacement` | boolean | false | Enable lib replacement |
| `@disableSizeLimit` | boolean | false | Remove 20MB cap for JS files |

---

### Backwards Compatibility Options

| Directive | Type | Default | Description |
|-----------|------|---------|-------------|
| `@suppressExcessPropertyErrors` | boolean | false | Don't report excess properties |
| `@suppressImplicitAnyIndexErrors` | boolean | false | Suppress index signature errors |
| `@noImplicitUseStrict` | boolean | false | Don't emit `"use strict"` |
| `@noStrictGenericChecks` | boolean | false | Disable strict generic checking |
| `@keyofStringsOnly` | boolean | false | `keyof` returns only strings |
| `@preserveValueImports` | boolean | false | Keep unused value imports |
| `@importsNotUsedAsValues` | enum | remove, preserve, error | Handling for type-only imports |
| `@charset` | string | "utf8" | (deprecated) File encoding |
| `@out` | string | | (deprecated) Use `outFile` instead |
| `@reactNamespace` | string | "React" | (deprecated) JSX factory object |

---

### Available @lib Values

The `@lib` directive accepts comma-separated values from this complete list:

**ECMAScript Versions:**
- `es5`, `es6`/`es2015`, `es7`/`es2016`, `es2017`, `es2018`, `es2019`, `es2020`, `es2021`, `es2022`, `es2023`, `es2024`, `esnext`

**Host Environments:**
- `dom`, `dom.iterable`, `dom.asynciterable`
- `webworker`, `webworker.importscripts`, `webworker.iterable`, `webworker.asynciterable`
- `scripthost`

**Decorators:**
- `decorators`, `decorators.legacy`

**Granular ES2015+ Features:**
- `es2015.core`, `es2015.collection`, `es2015.generator`, `es2015.iterable`, `es2015.promise`, `es2015.proxy`, `es2015.reflect`, `es2015.symbol`, `es2015.symbol.wellknown`
- `es2016.array.include`, `es2016.intl`
- `es2017.arraybuffer`, `es2017.date`, `es2017.object`, `es2017.sharedmemory`, `es2017.string`, `es2017.intl`, `es2017.typedarrays`
- `es2018.asyncgenerator`, `es2018.asynciterable`, `es2018.intl`, `es2018.promise`, `es2018.regexp`
- `es2019.array`, `es2019.object`, `es2019.string`, `es2019.symbol`, `es2019.intl`
- `es2020.bigint`, `es2020.date`, `es2020.promise`, `es2020.sharedmemory`, `es2020.string`, `es2020.symbol.wellknown`, `es2020.intl`, `es2020.number`
- `es2021.promise`, `es2021.string`, `es2021.weakref`, `es2021.intl`
- `es2022.array`, `es2022.error`, `es2022.intl`, `es2022.object`, `es2022.string`, `es2022.regexp`
- `es2023.array`, `es2023.collection`, `es2023.intl`
- `es2024.arraybuffer`, `es2024.collection`, `es2024.object`, `es2024.promise`, `es2024.regexp`, `es2024.sharedmemory`, `es2024.string`
- `esnext.array`, `esnext.collection`, `esnext.symbol`, `esnext.asynciterable`, `esnext.intl`, `esnext.disposable`, `esnext.bigint`, `esnext.string`, `esnext.promise`, `esnext.weakref`, `esnext.decorators`, `esnext.object`, `esnext.regexp`, `esnext.iterator`, `esnext.float16`, `esnext.typedarrays`, `esnext.error`, `esnext.sharedmemory`

---

### Multi-File Test Syntax

Multi-file tests use `@filename` directives to separate files:

```typescript
// @strict: true
// @target: ES2020

// @filename: types.ts
export interface User {
    name: string;
    age: number;
}

// @filename: main.ts
import { User } from './types';

const user: User = { name: "Alice", age: 30 };
console.log(user.email);  // Error: Property 'email' does not exist
```

---

### Symlink Syntax

The `@link` directive uses a special syntax with `->`:

```typescript
// @link: /actual/path -> /symlink/path
```

This is parsed with the regex:
```
/^\/{2}\s*@link\s*:\s*([^\r\n]*)\s*->\s*([^\r\n]*)/gm
```

---
