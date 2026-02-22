# Emitter TODO — Skipped / Investigated Issues

## Pattern Analysis (JS+DTS mode, current 9531/13623 = 70.0% JS, 766/1990 = 38.5% DTS)

### Fixed This Session
- **DTS double semicolons on constructor/template-literal/infer types** (+2 JS, +5 DTS tests):
  The declaration emitter's `emit_type()` method was missing handler arms for `CONSTRUCTOR_TYPE`,
  `TEMPLATE_LITERAL_TYPE`, and `INFER_TYPE`. These fell through to `emit_node()` which used
  `get_source_slice(node.pos, node.end)` — but the parser sets `node.end` past the trailing `;`
  (due to `parse_template_literal_head()` calling `self.next_token()` then `self.token_end()`).
  The source slice included the `;`, and then the type alias/declare statement emitter added its
  own `;`, producing `;;`. Fixed by adding proper `emit_type` handlers:
  - `CONSTRUCTOR_TYPE`: emits `[abstract] new <Params>(...) => ReturnType`
  - `TEMPLATE_LITERAL_TYPE`: emits `` `head${Type}middle${Type}tail` `` by walking head + spans
  - `INFER_TYPE`: emits `infer TypeParam`
  Five unit tests added in `declaration_emitter/tests.rs`.

### Previously Fixed
- **Single-line constructor body formatting** (+4 JS tests):
  `emit_constructor_body_with_prologue` always expanded constructor bodies to multiline,
  even when the source was single-line (e.g., `constructor(x) { this.a = x; }`). tsc
  preserves single-line format for constructors just like methods. Added an early-return
  path in `emit_constructor_body_with_prologue` that checks: single statement, no param
  properties, no field inits, no auto accessor inits, no hoisted temps, and `is_single_line`
  returns true. When all conditions hold, emits `{ stmt }` on one line instead of expanding.
  Tests fixed: `derivedClassWithoutExplicitConstructor3`,
  `emitClassDeclarationWithTypeArgumentAndOverloadInES6`,
  `objectTypesIdentityWithGenericConstructSignaturesDifferingTypeParameterCounts`,
  `objectTypesIdentityWithGenericConstructSignaturesOptionalParams3`.

- **`// @ts-check` and `// @ts-nocheck` directives stripped from JS output** (+3 JS tests):
  The header comment filter stripped all `// @` prefixed comments (designed for test harness
  directives like `// @target: esnext`), but this also caught `// @ts-check` and
  `// @ts-nocheck` — runtime directives that tsc preserves in output. Fixed by checking
  if the text after `@` starts with `ts-check` or `ts-nocheck` and preserving those.
  Tests fixed: `checkJsdocParamOnVariableDeclaredFunctionExpression`, `checkJsdocParamTag1`,
  `checkJsdocTypedefOnlySourceFile`.
  **UPDATE**: Generalized this fix — now ALL `// @` comments are preserved (see below).

- **All `// @` source comments stripped from JS output** (+5 JS tests, 9522 → 9527):
  The previous fix was too conservative — it only preserved `@ts-check` and `@ts-nocheck`
  while still stripping other `// @` comments like `@ts-ignore`, `@ts-expect-error`,
  `@noErrorTruncation`, `@strict`, `@internal`, etc. But tsc preserves ALL source-level
  comments in JS output. The test harness strips actual test directives (e.g. `@target`,
  `@module`) from the baseline source BEFORE the emitter sees them, so any `// @` comment
  remaining in the source is a legitimate source comment that should be preserved.
  Fix: removed the `// @` stripping logic entirely from `emit_source_file` in `mod.rs`.
  Added 3 unit tests in `statements.rs`: `test_at_directive_comments_preserved`,
  `test_ts_ignore_directive_preserved`, `test_ts_expect_error_directive_preserved`.
  Affected comment categories: `@ts-ignore` (39 occurrences across baselines),
  `@ts-expect-error` (13), `@strict` (6), `@internal` (4), `@readonly` (4), others.
  Tests fixed (sole-fix): 5 tests where the only mismatch was missing `// @` comments.

### Previously Fixed
- **Duplicate leading comments in ES5 class IIFE lowering** (+5 JS, +1 DTS tests):
  When classes were lowered to ES5 IIFEs (`var C = /** @class */ (function () { ... })()`),
  leading comments before the class (e.g., `// No errors`) were emitted twice:
  1. By the statement-level comment handler (`emit_comments_before_pos` in the block loop)
  2. By the IR printer via `ES5ClassIIFE.leading_comment`
  Fix: pass `None` for `leading_comment` in the IIFE IR construction and remove the
  `extract_leading_class_comment` method. The statement loop already handles leading
  comments for all statement types including class declarations. Two unit tests added.
  Tests fixed: `ES5For-ofTypeCheck10(target=es5)`, `accessibilityModifiers(target=es5)`,
  `accessorAccidentalCallDiagnostic(target=es5)`, and 2 others.

### Previously Fixed
- **Legacy octal literal conversion** (+20 JS tests):
  tsc always converts legacy octal literals (`01`, `076`, `009`) to their decimal equivalents
  in emitted JS, regardless of target. tsz was emitting them verbatim. The fix adds a
  `b'0'..=b'9'` match arm to `convert_numeric_literal_downlevel()` in `literals.rs` that:
  1. If all digits after leading `0` are `0-7`: parse as octal and emit decimal form
  2. If any digit is `8` or `9`: parse entire literal as decimal and emit decimal form
  This is distinct from ES2015 `0o`/`0b` downleveling (which is target-gated); legacy octal
  conversion fires for ALL targets, matching tsc behavior. Three unit tests added.
  Tests fixed: `scannerNumericLiteral2/3/8/9` (both targets), `scannerES3NumericLiteral2/3`,
  `octalLiteralInStrictModeES3`, `octalLiteralAndEscapeSequence`, and 12 additional tests
  where legacy octals appeared incidentally.
  Note: `es5-oldStyleOctalLiteralInEnums` still fails because the enum IIFE generator uses
  evaluated enum values from a separate code path (not `emit_numeric_literal`). Similarly,
  `strictModeOctalLiterals` still fails due to const enum value folding (expression `12 + 01`
  is not evaluated to `13`).

### Previously Fixed
- **Enum IIFE `var` → `let` for block-scoped enums at ES2015+** (+7 JS tests):
  tsc uses `var` for top-level enums but `let` for enums inside block scopes (functions,
  methods, constructors, namespaces) when targeting ES2015+. Two code paths needed fixing:
  1. **IR printer path** (`ir_printer.rs`): `EnumIIFE` handler and `emit_namespace_bound_enum_iife`
     now check `in_namespace_iife_body` flag to choose `let` vs `var`.
  2. **Direct emit path** (`declarations.rs`): Added `should_use_let_for_enum(idx)` helper that
     walks the AST parent chain to detect if the enum is inside a function/method/namespace rather
     than at source file top level. The string replacement `var E;` → `let E;` only fires when
     this returns true AND target is ES2015+.
  Tests fixed: `unusedLocalsInMethod4(target=es2015)`, `internalAliasEnumInsideLocalModuleWithoutExport`,
  `internalAliasEnumInsideLocalModuleWithoutExportAccessError`, and 4 others.
  Note: `localTypes1(target=es2015)` still fails due to a pre-existing `declared_namespace_names`
  scoping bug — the set is file-global, so repeated enum names across different function scopes
  cause the first branch (strip `var E;\n`) to match instead of the let-replacement branch.

- **ImportKeyword emission in emit_node_by_kind dispatch** (+13 JS tests):
  `SyntaxKind::ImportKeyword` was missing from the emitter's `emit_node_by_kind` match table.
  Dynamic `import('path')` calls have a `CallExpression` with an `ImportKeyword` token node as
  the callee (not an `Identifier`). Similarly, `import.meta` is a `PropertyAccessExpression` with
  `ImportKeyword` as the base expression. Without the dispatch entry, the keyword fell through to
  the `_ => {}` default arm and was silently dropped, producing `('path')` instead of
  `import('path')` and `.meta.url` instead of `import.meta.url`. Fixed by adding
  `k if k == SyntaxKind::ImportKeyword as u16 => self.write("import")` alongside the existing
  `ThisKeyword` and `SuperKeyword` handlers. Tests fixed (sole-fix): `importCallExpressionAsyncES2020`,
  `importCallExpressionShouldNotGetParen`, `moduleNoneDynamicImport(target=es2020)`,
  `nodeModulesDynamicImport(module=node16/node18/node20/nodenext)` (4 tests),
  `nodeModulesImportMeta(module=node16/node18/node20/nodenext)` (4 tests),
  plus 2 additional multi-issue tests partially fixed.

### Previously Fixed
- **moduleDetection=force support** (+31 JS, +1 DTS tests):
  `moduleDetection=force` was parsed by the CLI (`args.rs`) but never threaded to the emitter.
  Files without import/export syntax were not treated as modules, so the CJS `__esModule` marker,
  `"use strict"`, and export preamble were missing. Fixed by:
  1. Adding `module_detection_force: bool` to `PrinterOptions`.
  2. Threading from CLI (`apply_cli_overrides`) and tsconfig (`resolve_compiler_options`).
  3. Adding early-return `true` guards in all three module-detection functions:
     `file_is_module` (emitter), `file_is_module` (lowering pass), `should_emit_es_module_marker`.
  Also added `module_detection` field to `CompilerOptions` struct and `merge_options` macro.

- **Async arrow param parenthesization** (+2 JS tests):
  tsc always parenthesizes async arrow function parameters (`async (x) => ...`), even when
  the source omits parens (`async x => ...`). The native emit path in `emit_arrow_function_native`
  only checked `source_had_parens || !is_simple` but missed `func.is_async`. The lowered path
  (`emit_arrow_function_async_lowered`) already always parenthesized. Fixed by adding
  `|| func.is_async` to the `needs_parens` condition.
  Tests fixed: `asyncUnParenthesizedArrowFunction_es2017`, `modularizeLibrary_Worker.asynciterable`.

### Previously Fixed
- **JS input file compilation (allowJs parity)** (+33 JS tests):
  Two bugs prevented `.js`/`.jsx`/`.mjs`/`.cjs` input files from being emitted:
  1. `js_extension_for()` in `driver_resolution.rs` returned `None` for JS input extensions,
     so no output file was produced. Added `.js→.js`, `.jsx→.jsx`, `.mjs→.mjs`, `.cjs→.cjs`
     mappings to match tsc behavior where allowJs files are emitted.
  2. `discover_ts_files()` in `fs.rs` required `allow_js` to be true even for explicitly
     listed files (CLI positional args, tsconfig `"files"` array). tsc always compiles
     explicitly listed files regardless of `allowJs`; the setting only controls
     pattern-matched discovery (`include`/`exclude`). Removed the `allow_js` guard for
     the explicit file loop.
  Both fixes were required together — either alone had no effect. The 33 tests fixed
  are `.js` files that tsc emits with `"use strict"` (via `alwaysStrict`) but tsz
  previously skipped entirely.

### Previously Fixed
- **CLI-transpiler spurious "use strict" injection for AMD/UMD/Preserve** (+30 JS tests):
  The `cli-transpiler.ts` post-processing hack injected `"use strict"` at the top of output
  for AMD (module=2), UMD (module=3), and Preserve (module=200) module kinds. This was wrong:
  AMD/UMD modules add `"use strict"` inside their wrapper functions (not at the top level),
  and Preserve keeps ESM as ESM (implicitly strict, no preamble needed). Only CJS (module=1)
  needs the top-level compensation. Fixed by restricting `commonJsLikeModule` in
  `scripts/emit/src/cli-transpiler.ts` from `module === 1 || module === 2 || module === 3 || module === 200`
  to `module === 1`.

- **ES decorator emission for esnext target** (+36 JS tests):
  `emit_method_modifiers_js` and `emit_class_member_modifiers_js` in
  `declarations_class_members.rs` silently skipped DECORATOR nodes, causing all ES (non-legacy)
  decorators to be dropped from output at esnext target. tsc emits these decorators verbatim
  when not using `--experimentalDecorators` (legacy mode). Fixed by adding conditional emission
  of decorator nodes when `!self.ctx.options.legacy_decorators`. Affects method decorators,
  static method decorators, getter/setter decorators, and property decorators that survive
  inside the class body.

### Previously Fixed
- **JSX text whitespace/newline preservation** (+32 JS tests):
  JSX text nodes between opening and closing elements were losing leading whitespace
  and newlines. For example, `<Comp>\n        hi hi hi!\n    </Comp>` was collapsed
  to `<Comp>hi</Comp>`. Two bugs in the scanner's JSX rescan path:
  1. `re_scan_jsx_token` reset `pos` to `token_start` (after trivia) instead of
     `full_start_pos` (before trivia), matching tsc's `pos = tokenStart = fullStartPos`.
  2. `scan_jsx_token` did not clear `token_atom`, so `get_token_value_ref()` returned
     the stale interned identifier from the prior regular scan instead of the JSX text.
  Tests fixed (sole-fix): `checkJsxChildrenProperty1`-`16`, `checkJsxChildrenCanBeProps`,
  `checkJsxNamespaceNamesQuestionableForms`, and 14+ others with multiline JSX content.

### Previously Fixed
- **Case clause same-line non-block statement formatting** (+20 JS, +1 DTS tests):
  When a case/default clause has a single statement on the same source line as the label
  (e.g., `case true: return "true";`), tsc emits it on one line. tsz was splitting it
  across two lines because `emit_case_clause_body()` only allowed same-line formatting
  for BLOCK statements (`{ ... }`), not for other statements like `return`, `break`, etc.
  Fixed by removing the `stmt_node.kind == syntax_kind_ext::BLOCK` restriction in
  `statements.rs`, allowing any single statement on the same source line to stay on one line.
  Added `!self.writer.is_at_line_start()` guard to prevent double newlines for blocks.
  Tests fixed (sole-fix): `booleanLiteralTypes1`, `booleanLiteralTypes2`,
  `discriminatedUnionTypes1`, `enumLiteralTypes1`, `enumLiteralTypes2`,
  `numericLiteralTypes1`, `numericLiteralTypes2`, and 13 others.

### Previously Fixed
- **Extra blank line after lowered static class properties** (~70 affected tests, 6+ sole-fix):
  When class declarations have static fields lowered to `ClassName.field = value;` (for
  targets < ES2022), the `write_line()` at the end of each static field init left the
  writer at line start. Subsequent newline writes in the calling code (source file loop,
  block loop, CJS export handler, namespace body, decorator handler) added a duplicate
  newline, producing an unwanted blank line between the last `ClassName.field = value;`
  and the next statement (`exports.default = ...;`, `D = __decorate(...)`, or the next
  block statement). Fixed by adding `!self.writer.is_at_line_start()` guards in 6 places:
  1. `statements.rs`: block-level statement loop
  2. `module_emission.rs`: CJS `emit_commonjs_export` after inner emission
  3. `module_emission.rs`: CJS export class declaration handler
  4. `declarations.rs`: namespace body class emission
  5. `declarations.rs`: namespace body exported class branch
  6. `declarations.rs`: decorated class `write_line` after `emit_class_es6_with_options`
  Tests fixed (sole-fix): `classImplementsImportedInterface`, `classInConvertedLoopES5(target=es2015)`,
  `declarationEmitInvalidExport`, `es6ModuleClassDeclaration`, `es6modulekindWithES5Target(target=es2015)`,
  `esnextmodulekindWithES5Target(target=es2015)`. Also resolved the blank-line component of
  `defaultDeclarationEmitNamedCorrectly` (still fails for `let` vs `var` in namespace).

### Previously Fixed
- **JSX self-closing element space before `/>`** (+16 JS tests):
  Self-closing JSX elements (`<Tag />`) were emitted without the space before `/>`,
  producing `<Tag/>`. tsc always calls `writeSpace()` after the tag name in
  `emitJsxSelfClosingElement`, so when there are no attributes the space appears
  before `/>`. When attributes are present, `emit_jsx_attributes` already prepends
  a space before each attribute, so no extra space is needed. Fixed by checking
  `has_attributes` in `emit_jsx_self_closing_element` and writing `" "` only when
  there are no attributes. Results: JS 4878→4894, DTS unchanged, zero regressions.

### Previously Fixed
- **Deduplicate overloaded function exports in CommonJS** (+4 JS tests):
  Overloaded functions produce multiple `FUNCTION_DECLARATION` AST nodes (one per
  overload signature + implementation). `collect_export_names` and
  `collect_export_names_categorized` in `module_commonjs.rs` pushed the name for
  each declaration without dedup, causing repeated `exports.X = X;` lines.
  Fixed by adding `!exports.contains(&name)` / `!func_exports.contains(&name)` guards
  in all three collection paths (direct function, wrapped export-declaration function,
  and `collect_export_name_from_declaration` helper).
  Results: JS 9224→9228, DTS unchanged, zero regressions.

### Previously Fixed
- **"use strict" emission fixes + CLI Preserve mapping** (+17 JS, +1 DTS tests):
  Three targeted fixes:
  1. `args.rs`: `Self::Preserve` was incorrectly mapped to `ModuleKind::ESNext` instead of
     `ModuleKind::Preserve`. This caused `--module preserve` to behave as ESNext.
  2. `emitter/mod.rs`: AMD/UMD "use strict" condition was too broad — it fired for
     non-module scripts (no import/export). Added `is_file_module` guard. AMD module files
     are already handled by `emit_module_wrapper()` (line ~647) which adds "use strict"
     inside the `define()` callback.
  3. `emitter/mod.rs`: The `alwaysStrict` "use strict" path incorrectly excluded AMD/UMD
     (`&& !is_amd_or_umd`). Since AMD module files never reach `emit_source_file()` (they're
     redirected to `emit_module_wrapper()`), this exclusion was wrong for AMD non-module
     scripts that have alwaysStrict enabled.
  Results: JS 9222→9239, DTS 744→745 (zero regressions).

### Previously Fixed
- **Trailing comments on lowered static class fields** (+2 tests):
  When static class fields are lowered to `ClassName.field = value;` for targets < ES2022,
  trailing comments (e.g. `static intance = new C3(); // ok`) were consumed by
  `comment_emit_idx` advancement but never saved for re-emission. Fixed by collecting
  trailing comments alongside leading comments during class body member processing and
  emitting them after the lowered `ClassName.field = value;` statement.
  Tests fixed: `classDeclarationCheckUsedBeforeDefinitionInItself`, `classMemberInitializerScoping`.

### Previously Fixed
- **Namespace body phantom blank line for zero-output statements** (+4 tests):
  `emit_namespace_body_statements()` in `declarations.rs` unconditionally called
  `write_line()` in the `else` branch for non-erased statements, even when `emit()`
  produced no output. Type-only import-equals aliases like `import T = M1.I;` pass
  `is_erased_statement()` (they might have runtime value), but `emit_import_equals_declaration_inner()`
  returns early without writing anything when `import_decl_has_runtime_value()` is false.
  Fixed by wrapping the trailing-comment + write_line logic in a `before_len` guard,
  matching the pattern used in `emit_block()` and the EXPORT_DECLARATION branch.
  Tests fixed: `classImplementsImportedInterface` and 3 others with similar patterns.

### Previously Fixed (Prior Session)
- **esModuleInterop gating for CJS import helpers** (architectural fix, +0 net tests):
  `__importStar`, `__importDefault`, and `__createBinding` helpers were emitted
  unconditionally for all CJS imports. Now properly gated on `esModuleInterop` flag:
  without it, namespace imports emit `const ns = require("mod")` and default imports
  emit `const m = require("mod")` (no helper wrapper). Also fixed test runner to
  default `esModuleInterop: true` matching TS6 semantics. Files changed:
  `lowering_pass.rs`, `module_emission.rs`, `module_commonjs.rs`, `emitter/mod.rs`,
  `config.rs`, `driver.rs`, `runner.ts`.

### Previously Fixed
- **Template literal closing brace off-by-one** (+73 tests): `template_span_has_closing_brace`
  scanned `text[expr_end..lit_pos]` but Rust's half-open range excluded `lit_pos` itself,
  which is where `}` sits. When whitespace padded `${ expr }`, the range contained only
  spaces and returned false, dropping the `}`. Similarly, `template_tail_has_backtick` had
  an analogous issue. Fixed both in `template_literals.rs` to check `lit_node.pos` and
  `node.end - 1` directly.

### Previously Fixed
- **Orphaned comments at end of class body** (+27 tests): Comments after erased members
  leaked past the closing `}`. Fixed by advancing `comment_emit_idx` past remaining
  comments inside the class body boundary after the member loop.

- **Semicolons in class bodies** (+4 tests): `SEMICOLON_CLASS_ELEMENT` nodes were
  incorrectly marked as erased in the emitter (declarations.rs). Additionally, the
  parser's `parse_class_members()` consumed trailing semicolons unconditionally via
  `parse_optional(SemicolonToken)` after each member, which ate the second `;` when
  consecutive semicolons appeared. Fixed in both emitter (stop erasing) and parser
  (skip trailing-semicolon consumption when member is itself a `SEMICOLON_CLASS_ELEMENT`).

### Investigated but Deferred

- **JS-side double semicolons on IIFEs/yield/await (~29 tests)**: The `})();;` pattern on static
  block IIFE lowering, `yield;;`/`await ...;;` in async downlevel transforms, and
  `_this.memberClass = class { };;` in class expression lowering are all caused by the same root
  issue: both the inner expression/block emitter and the outer statement/wrapper emitter add `;`.
  The yield/await `;;` cases are part of the existing deferred "async_transform" pattern. The IIFE
  `;;` cases require careful audit of the static block lowering path in `declarations.rs` line 1240
  and the expression statement semicolon logic. The `for (var x;;)` cases in the cache are actually
  valid JS infinite loops — not bugs.

- **`declared_namespace_names` set is file-global, not scope-local (~5-10 tests)**:
  The `declared_namespace_names` set in the Printer tracks enum names for namespace/enum merges,
  but it's never cleared or scoped when entering/exiting function bodies. If two different functions
  both declare `enum E`, the second one's `var E;\n` prefix gets stripped entirely (the
  namespace-merge branch fires instead of the let-replacement branch). Affects `localTypes1(target=es2015)`
  and similar tests with repeated enum names across different function scopes. Fix would require
  scoping the set by function/block context, similar to how temp name scoping works.

- **"use strict" deduplication is context-dependent (~145 remaining tests, ~29 sole-fix)**:
  Investigated removing `dedupeUseStrictPreamble()` from the cli-transpiler and fixing the
  emitter's `should_emit_use_strict` logic to handle alwaysStrict separately from CJS module
  "use strict". This caused 56 regressions because tsc's behavior is inconsistent:
  `alwaysStrictAlreadyUseStrict` expects ONE `"use strict"` (dedup when source has prologue),
  but `localClassesInLoop` expects TWO (source prologue + alwaysStrict output), both with
  `alwaysStrict=true`. The difference appears to depend on whether alwaysStrict is set
  explicitly via `@alwaysStrict: true` comment vs. inferred from other options. Reverted.
  Affects `amdDeclarationEmitNoExtraDeclare`, `impliedNodeFormatEmit1(module=amd)`,
  `emitBundleWithPrologueDirectives1`.
- **Triple-slash reference directives in JS output (~21 tests, caution: complex)**: `/// <reference path="..." />` comments are emitted in JS output where tsc strips them in SOME contexts. Investigated blanket stripping of `/// <reference` directives in the comment filter — this caused 136 regressions because tsc PRESERVES them in many test baselines. The stripping behavior is context-dependent (file-header-only vs inline references, or possibly depends on module format / bundling). Needs careful analysis of tsc's exact rules before fixing. Affects `augmentExportEquals3_1`, `doNotemitTripleSlashComments`, `jsxEmptyExpressionNotCountedAsChild`.
- **Const enum value folding (~33 tests)**: Const enum member property accesses (`E.A`) are not replaced with their literal values (`0 /* E.A */`). Requires solver integration. Affects `constEnumPropertyAccess*`.
- **Node modules CJS/ESM format comment (~21 tests)**: tsz emits `// cjs format file` while tsc emits `// esm format file` for `.js` files inside `node_modules` when the containing package has `"type": "module"` in `package.json`. Module format detection is wrong for these files under `node16`/`node18`/`node20`/`nodenext` module modes. Affects `nodeModulesAllowJs*` family.
- **Extra source comments on transformed constructs (~37 tests)**: tsz emits source-level comments (like `// error`, `// no error`, `// should not error`) that tsc strips during transformation. Broader than item #13 which only covers erased constructs — these comments are attached to parameter lists, expressions, and statement-level constructs that tsc transforms.
- ~~**Numeric literal not downleveled for ES5 (~13 tests)**~~ — **PARTIALLY FIXED** (see "Fixed This Session"): Legacy octal conversion now works for all targets. Remaining: ES2015 octal (`0o`) with numeric separators at ES2015+ targets may need additional handling (affects `parser.numericSeparators.octal`/`octalNegative`). Enum IIFE initializer paths still use un-converted source text.
- **Missing hoisted temp var declarations (~8 tests)**: When lowering optional chaining, nullish coalescing, or element access chains, tsc hoists temporary variable declarations (`var _a, _b`) to the top of the enclosing scope. tsz omits these declarations. Affects `elementAccessChain.2`, `nullishCoalescingOperator10/12`, `spreadUnionPropOverride`.
- **Parenthesization mismatches (~12 tests)**: Various parenthesization differences — extra parens around cast results, missing parens in extends clauses, extra parens around yield, missing parens on async arrow params. Multiple sub-issues.
- ~~**Missing `Object.defineProperty(exports, "__esModule")` for moduleDetection=force (~10 tests)**~~ — **FIXED** (see "Fixed This Session").
- **Remaining duplicate-export affected tests (~90)**: The overload dedup fix resolves the duplicate `exports.X = X;` issue, but ~90 tests that had this as one of multiple issues remain failing due to other causes (missing helpers, wrong comment placement, module transform gaps). No action needed on the dedup side; remaining failures need other fixes.
- **`exports.default` ordering (~19 tests)**: tsc emits `exports.default = X;` in the preamble (before the function), but tsz emits it after the function body. Requires reordering default export assignment in the CommonJS preamble emission path.
- **Missing `exports.X = X;` for scoped declarations (~42 tests)**: tsc emits `exports.X = X;` for class/function exports declared inside nested scopes (e.g., conditionals, namespaces). The export collector only scans top-level statements, missing these. Would need deeper AST traversal or a different approach.
- `ambientModuleDeclarationWithReservedIdentifierInDottedPath` / `ambientModuleDeclarationWithReservedIdentifierInDottedPath2`: ambient dotted module declarations now still emit wrong declaration shapes when mixed with declaration emit filtering; requires namespace/ambient-module emitter refactor, so deferred for later session.
- `abstractPropertyInitializer` / `abstractPropertyDeclaration`: DTS accessor parity still regresses on mixed abstract/private getter/setter edge cases; we fixed only private setter parameter naming and deferred broader declaration-transform compatibility work.
- `accessor*` and `private*` DTS test filters: remaining failures appear to require cross-module declaration helper/mapping changes, which is outside the smallest emitter-only fix scope for this pass.
- `crates/tsz-emitter/src/declaration_emitter/tests.rs: test_variable_declaration_infers_accessor_object_type_from_initializer_when_type_cache_missing`: this failure predates this pass and is currently blocked by a broader declaration-emitter regression in the same module; skipped to keep this change focused on emitter transform comment ordering.
- `./scripts/emit/run.sh` full run (`JS+DTs`) and `scripts/emit` broader checks: large pre-existing failure set (6,828 failures total for JS+DTS) plus 2 timeouts remain; deferred for dedicated conformance/reporter work outside this smallest parity pass.
- **Extra blank lines between JSDoc blocks in .js files (~18 tests)**: When tsz processes `.js` source files, it inserts extra blank lines between JSDoc comment blocks and following declarations. tsc does not add these. Affects `checkJsdocTypeTag1`, `checkJsdocTypeTag2`, `checkJsdocSatisfiesTag15`. Likely a newline-after-statement issue in the emission loop for files where type annotations are erased.
- **JSDoc comments on object literal properties run together (~1 test)**: `checkJsdocTypeTagOnObjectProperty1` has `// @ts-check` preserved now (fixed this session), but JSDoc comments on object properties (`/** @type {string} */`) are emitted without the preceding newline, concatenating them with the previous property's trailing comma. Likely a comment-before-pos issue in the object literal property emission loop.
- **Duplicate `//# sourceMappingURL` in multi-file concatenation (~10 tests, runner issue)**: When tests
  use `@outFile` with a non-`out.js` name (e.g., `testfiles/fooResult.js`), the runner doesn't pass
  `--outFile` to tsz (only does so for `out.js`). tsz ignores `--outFile` anyway (bundling not implemented).
  The baseline parser also includes the expected output JS file as a source input to tsz, causing tsz to
  re-emit the bundled output file and append a second `//# sourceMappingURL`. Two-pronged issue:
  1. Runner should exclude expected output files from source inputs
  2. tsz's `--outFile` bundling is not yet implemented
  Affects `declarationMapsWithSourceMap`, `sourceMapWithCaseSensitiveFileNames`, `out-flag2/3`, etc.
  The runner's persistent `emit-cache.json` can mask changes — clear `.cache/emit-cache.json` when debugging.
- **Class expression static property comma-expression lowering (~12 tests)**: tsc emits a comma-expression pattern with a temp variable (`var _a; var v = (_a = class C {}, _a.a = 1, _a);`) for class expressions with static properties. tsz emits separate statements. Affects `classExpressionWithStaticProperties1`-`ES64`, `classBlockScoping`.
- **`super(...arguments)` in `extends null` classes (~5 tests)**: tsz inserts `super(...arguments)` for classes that `extends null`, which tsc does not. Auto-super injection logic doesn't account for `extends null`. Affects `classExtendingNull`.
- **JSX text whitespace collapsing in React transform mode (~remaining JSX tests)**: The `re_scan_jsx_token` fix resolves preserve-mode JSX text, but some JSX tests using React transform may have different whitespace-handling requirements where tsc strips certain whitespace-only text nodes.

### High-Impact Patterns (Not Yet Fixed)

1. **class_iife** (~205 tests, ~123 unique): Classes with downlevel transforms expected
   to emit IIFE wrappers (e.g. `var Foo = (function() { ... })();`). tsz emits ES6
   class syntax instead. Requires implementing the ES5 class transform pipeline.

2. **extra_comment (between-member)** (~90 remaining tests): Comments between erased
   and non-erased class members still leak through `emit_comments_before_pos()`. The
   aggressive fix (skipping to next member pos) regressed 15 tests because it ate
   leading comments of subsequent non-erased members. Needs a smarter heuristic that
   distinguishes "trailing comment of erased member" from "leading comment of next
   member" — possibly using line gap or blank-line detection.

3. **export_pattern** (~101 tests): Various export rewriting mismatches — missing
   `Object.defineProperty(exports, ...)`, incorrect `exports.X = ...` patterns,
   module system transform issues.

4. **missing_helper** (~99 tests): Missing runtime helper functions like `__decorate`,
   `__extends`, `__awaiter`, `__generator`, `__spreadArray` etc. Requires implementing
   the helper injection system.

5. **decorator** (~34 remaining tests): ES decorator verbatim emission at esnext is
   now fixed (+36 tests). Remaining failures are legacy (experimental) decorator tests
   that need `__decorate` helper and the full transform pipeline.

6. **let_var** (~49 tests): `let`/`const` → `var` downlevel transform not applied
   when targeting ES5.

7. **enum_iife** (~35 tests): Enum declarations not emitted as IIFEs
   (`var Color; (function(Color) { ... })(Color || (Color = {}));`).

8. **namespace/module IIFE** (~30 tests): Similar to enum — namespace/module blocks
   need IIFE wrapping.

9. **async_transform** (~25 tests): `async`/`await` downlevel transform to
   `__awaiter`/`__generator` pattern.

10. **computed_property** (~20 tests): Computed property names in class/object
    downlevel transform.

11. **"use strict" for AMD/outFile modules** (partially fixed): AMD module "use strict"
    inside `define()` callback now works correctly. Remaining failures are outFile-specific
    bundling scenarios where the test runner interaction is complex.

12. ~~**"use strict" for module=preserve**~~ — **FIXED**: The `Preserve` module kind
    mapping bug in `args.rs` and the cli-transpiler's spurious `"use strict"` injection
    for AMD/UMD/Preserve are both now fixed.

13. **Comment preservation on erased constructs** (~13 tests): Comments like
    `// error` and `// no error` attached to type-only declarations are emitted
    even when the declaration is erased. The emitter's `skip_comments_for_erased_node`
    doesn't fully suppress comments that are interleaved between erased and
    non-erased members.

14. **accessor keyword transform** (~34 tests): The `accessor` keyword on class
    fields requires a downlevel transform to getter/setter pairs. Not yet implemented.

15. **`using` statement disposal helpers** (~26 tests): The `using` declaration
    requires `__addDisposableResource` and related helpers. Not yet implemented.

16. **Import elision for unused value imports** (~11 tests): `import {x} from "foo"`
    where `x` is never used at runtime should be stripped, with `export {};` emitted to
    preserve module status. See `cachedModuleResolution1..9`, `bundlerConditionsExcludesNode`,
    `bundlerNodeModules1`. Requires checker-emitter coordination to track used imports.

17. **Enum constant folding/inlining** (~5 tests): `foo(E.A)` should emit `foo(0 /* E.A */)`
    when `E.A` is a const-evaluable enum member. See `assignmentNonObjectTypeConstraints`,
    `blockScopedEnumVariablesUseBeforeDef*`. Requires solver integration for enum evaluation.

18. ~~**Extra blank line after class static properties**~~ — **FIXED** (see "Fixed This Session").

19. **Extra `"use strict"` for AMD/outFile modules** (mostly fixed): The AMD `"use strict"`
    condition in `emit_source_file()` now correctly requires `is_file_module`, preventing
    spurious top-level emission for non-module scripts. AMD module files are handled by
    `emit_module_wrapper()`. Remaining failures are outFile-specific bundling edge cases.

20. **Trailing comments in ES5-lowered method bodies** (~5 tests): When class methods
    are lowered to ES5 `Object.defineProperty` patterns, trailing comments on statements
    inside the method body (e.g. `return A._a; // is possibly null`) are dropped.
    See `classStaticPropertyTypeGuard(target=es5)`. The ES5 class transform IR pipeline
    doesn't carry trailing comment information through the IR → output path.

21. **Comment misplacement between closing brace and else/next-construct** (~3 tests):
    Comments before closing `}` can be misplaced to after the `else` keyword instead.
    See `commentLeadingCloseBrace`. The block emission's trailing comment scanner
    attaches the comment to the wrong boundary when `}` is followed by `else`.
