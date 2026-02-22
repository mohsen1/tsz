# Emitter TODO — Skipped / Investigated Issues

## Pattern Analysis (JS+DTS mode, current 5063/7163 = 70.7% JS, 416/1036 = 40.2% DTS)

### Fixed This Session
- **Numeric separator downleveling for hex/octal/binary literals** (+7 JS):
  When numeric literals contain separators (`_`, an ES2021 feature) and the target is < ES2021,
  tsc converts all prefixed forms (`0b`, `0o`, `0x`) to their decimal equivalents. tsz was
  stripping the underscores but not converting the base prefix to decimal — so `0x00_11` became
  `0x0011` instead of `17`. Fix: added a `had_separators` flag to `emit_numeric_literal` and
  extended `convert_numeric_literal_downlevel` with a `needs_separator_downlevel` condition that
  fires when `had_separators && target < ES2021`. Added a new `0x`/`0X` match arm for hex-to-decimal
  conversion. Four unit tests added.
  Tests fixed: `numericUnderscoredSeparator(target=es5/es2015/es2016/es2017)`,
  `parser.numericSeparators.hex`, `parser.numericSeparators.octal`,
  `parser.numericSeparators.octalNegative`.
  JS: 5056 → 5063, DTS unchanged, zero regressions.

### Previously Fixed
- **Hoisted CJS exports for `export { f }` referencing function declarations** (+2 JS):
  When `export { f }` re-exports a locally-declared function, tsc emits `exports.f = f;` in
  the CJS preamble (before the function body) because JS function declarations are hoisted.
  tsz was categorizing these as non-function exports, producing `exports.f = void 0;` plus
  a duplicate `exports.f = f;` after. Three changes: (1) `collect_export_names_categorized`
  now resolves named export specifiers back to their declarations — if the local name matches
  a function declaration, it's categorized as hoisted. (2) `ModuleTransformState` gains
  `hoisted_func_exports` to track preamble-emitted names. (3) The `NAMED_EXPORTS` inline
  handler skips specifiers in `hoisted_func_exports` to prevent duplicate emission.
  Two unit tests added.
  Tests fixed: `assertionFunctionWildcardImport2`, `exportRedeclarationTypeAliases`.
  JS: 5054 → 5056, DTS unchanged, zero regressions.
  Note: ~46 other baselines have this hoisted function export pattern but still fail due
  to overlapping issues (missing helpers, other export ordering issues, ambient re-exports).

### Previously Fixed
- **Unterminated template literal emission preserves source verbatim** (+5 JS):
  tsc preserves unterminated template literals verbatim in emitted JS — no closing backtick
  is added when the source doesn't have one (error recovery). tsz was always writing a closing
  backtick in `emit_no_substitution_template`. Two changes: (1) added
  `no_substitution_template_has_closing_backtick()` which checks whether the source byte at
  `node.end - 1` is an unescaped backtick (counting preceding backslashes to detect escaped
  backticks like `\``). (2) Changed `get_raw_template_part_text()` to return the raw source
  slice from after the opening backtick to `node.end` when no closing delimiter is found
  (unterminated case), instead of falling back to the scanner's cooked value which loses
  escape sequences. Five unit tests added.
  Tests fixed: `templateStringUnterminated1_ES6`, `templateStringUnterminated2`,
  `templateStringUnterminated3`, `templateStringUnterminated5_ES6`,
  `taggedTemplatesWithIncompleteNoSubstitutionTemplate1`.
  JS: 871 → 876, DTS unchanged, zero regressions.

### Previously Fixed
- **Unicode escape sequences in identifiers not preserved** (+10 JS):
  tsc preserves unicode escape sequences (`\u0041`, `\u{102A7}`) verbatim in emitted identifiers,
  but tsz resolved them to their Unicode characters (`A`, `𐊧`). Root cause: the scanner resolves
  escapes during tokenization for semantic analysis (atom interning), but the parser never captured
  the original source text. Fix: when the scanner sets `TokenFlags::UnicodeEscape`, capture the
  source slice (`source_text[token_start..token_end]`) into `IdentifierData.original_text`. The
  emitter then uses `original_text` when available instead of the resolved `escaped_text`.
  Three parser call sites updated: `parse_identifier`, `parse_identifier_name`, and the default
  arm of `parse_property_name` in `state_expressions_literals.rs`. Two unit tests added.
  Tests fixed: `duplicateObjectLiteralProperty(es2015/es5)`, `scannerS7.6_A4.2_T1`,
  `parserClassDeclaration23`, `unicodeEscapesInNames01/02(es2015)`,
  `extendedUnicodeEscapeSequenceIdentifiers`, and others.
  JS: 5039 → 5049, DTS unchanged, zero regressions.
  Note: `unicodeEscapesInNames01/02(es5)` still fail because ES5 class lowering doesn't
  carry the `original_text` through the transform IR. `enumWithUnicodeEscape1` fails because
  the escape is in a string literal (enum member name), not an identifier — separate fix needed.

### Previously Fixed
- **Erased member comment over-consumption past closing brace** (+6 JS, +1 DTS):
  `skip_comments_for_erased_node()` in `comment_helpers.rs` consumed all same-line comments up to
  the end of the line, even when a code token like `}` separated the erased node from the comment.
  For `class C extends E { foo: string; } // error`, erasing `foo: string;` also consumed
  `// error` which logically belongs to the closing `}`. Root cause: `find_token_end_before_trivia`'s
  suffix recovery scan overshot `node.end` (finding the parent `}`), making `actual_end` = 34
  (right after `}`), so the gap between the erased node and the comment appeared to be just a space.
  Fix: use `node.end` (not the overshot `actual_end`) as the gap-check anchor. If any non-whitespace
  code exists between `node.end` and the comment start, the comment is not consumed.
  Two unit tests added. JS: 9570 → 9576, DTS: 775 → 776, zero regressions.
  Note: the initial analysis predicted 57 sole-fix tests, but most of those tests have additional
  overlapping issues (comment placement on lowered field initializers, extra comments from other
  paths) — the 6 net gains are tests where THIS was the only remaining mismatch.

### Previously Fixed
- **Hoisted `exports.default` for function declarations in CJS** (+14 JS):
  tsc emits `exports.default = f;` BEFORE the function declaration for
  `export default function f()` in CommonJS mode. JS function declarations are hoisted, so
  the binding exists at the top of the scope. tsz was emitting the export assignment AFTER
  the function body. Fix: add `emit_commonjs_export_with_hoisting()` with an
  `is_hoisted_declaration` flag. Three code paths updated: ExportDeclaration wrapping in
  `module_emission.rs`, transform directive dispatch, and chained directive dispatch.
  Two unit tests added.
  JS: 9557 → 9571, DTS unchanged, zero regressions.

### Previously Fixed
- **Wrong AST accessors for method/constructor overload erasure** (+3 JS, +1 DTS):
  The `is_erased` check in the class member emission loop (line ~870 in `declarations.rs`) used
  `self.arena.get_function(member_node)` for `METHOD_DECLARATION` and `CONSTRUCTOR` kinds. But
  `get_function()` only matches `FUNCTION_DECLARATION | FUNCTION_EXPRESSION | ARROW_FUNCTION` —
  it always returns `None` for methods and constructors. This meant bodyless overload signatures
  were never detected as erased, so their leading comments (JSDoc blocks, `// error` annotations)
  leaked into the JS output. Fix: use `get_method_decl()` for methods and `get_constructor()` for
  constructors. Two unit tests added.
  This fix also removes the extra-comment component from ~66 multi-issue failures, making future
  fixes in that area easier.
  JS: 5030 → 5033, DTS: 414 → 415, zero regressions.

### Previously Fixed
- **Private fields (#name) with initializers dropped at ES2022+ when useDefineForClassFields=false** (+8 JS, +1 DTS):
  Private class fields use native syntax at ES2022+ and are unaffected by
  `useDefineForClassFields` (which only controls public field semantics). The class field
  lowering skip logic (line ~761 in `declarations.rs`) unconditionally skipped all property
  declarations with initializers when `needs_class_field_lowering` was true. For private fields,
  the collection phase (lines 620-653) failed to collect them because `identifier_text()`
  returns empty for `PrivateIdentifier` nodes, so they were silently dropped — neither collected
  for post-class lowered emission NOR emitted in the class body. Fix: add a guard to the skip
  condition that preserves private fields when the target natively supports them (>= ES2022).
  Two unit tests added.
  Tests fixed: `privateNameStaticFieldInitializer(es2022/esnext)`,
  `privateNameStaticFieldDestructuredBinding(es2022/esnext)`,
  `privateNameStaticAndStaticInitializer(es2022/esnext)`, and others.
  JS: 5028 → 5036, DTS: 415 → 416, zero regressions.

### Previously Fixed
- **Missing parentheses in `extends` clause for lowered optional chains** (+1 JS, +1 DTS):
  When `class C extends A?.B {}` is emitted with target < ES2020, the optional chain is lowered
  to `A === null || A === void 0 ? void 0 : A.B`. This conditional expression needs parens in
  the `extends` position because JavaScript grammar requires a `LeftHandSideExpression` there.
  The parser stores the heritage expression directly as a `PropertyAccessExpression` (not wrapped
  in `ExpressionWithTypeArguments`) when there are no type arguments, so the fix checks
  `question_dot_token` on `AccessExprData` in both branches of `emit_heritage_expression()`.
  A `heritage_expr_needs_optional_chain_parens()` helper handles property access, element access,
  and call expression optional chains. Unit test added.
  Test fixed: `classExtendingOptionalChain`.
  JS: 9541 → 9542, DTS: 776 → 777, zero regressions.

### Previously Fixed
- **`/// <reference>` directives emitted inside AMD/UMD/System wrapper body** (+10 JS tests):
  tsc places `/// <reference path="..." />` directives at file top level, BEFORE the module
  wrapper (`define()`, UMD IIFE, `System.register()`). tsz was emitting them inside the wrapper
  body with indentation because `emit_source_file` deferred them as header comments within the
  already-indented wrapper context. Fixed by: (1) adding `extract_reference_directives()` to
  extract `/// <reference` lines from source headers, (2) emitting them before the wrapper in
  `emit_amd_wrapper`, `emit_umd_wrapper`, and `emit_system_wrapper`, (3) filtering them from
  the comment stream inside `emit_source_file` when `original_module_kind` is set to prevent
  duplication. Three unit tests added.
  Tests fixed: `tsxSfcReturnNull`, `tsxSfcReturnNullStrictNullChecks`,
  `tsxStatelessFunctionComponentOverload1/2`,
  `tsxStatelessFunctionComponentWithDefaultTypeParameter1/2`,
  `tsxStatelessFunctionComponentsWithTypeArguments1/2/3/5`.
  JS: 9531 → 9541, DTS unchanged, zero regressions.

### Previously Fixed
- **Redundant `public` modifier in DTS class members** (+6 DTS tests):
  `emit_member_modifiers` in `declaration_emitter/exports.rs` was emitting `public ` for
  class properties, methods, and accessors. tsc omits `public` in `.d.ts` output because
  it's the default accessibility — only `protected` and `private` are meaningful. The
  parameter-property promotion path (line ~1028 in `mod.rs`) already correctly skipped
  `public`; this aligns the general member modifier path. Unit test added.
  DTS: 207 → 213 passed, JS unchanged at 2793.

### Previously Fixed
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
- **Unicode escape in string literals / enum member names (~3 remaining tests)**: The identifier-level fix preserves
  `\u0041` in identifiers, but unicode escapes inside string literals (e.g., `'gold \u2730'` as an enum member name)
  are still resolved to their Unicode characters by the scanner. The scanner's string literal processing path
  resolves all escapes into `token_value`. Fixing this would require changes to the scanner's string literal
  handling or the emitter's string literal emission to use source slices. Affects `enumWithUnicodeEscape1`,
  `constEnumSyntheticNodesComments`, `templateLiteralEscapeSequence`.
- ~~**Numeric literal not downleveled for ES5 (~13 tests)**~~ — **FIXED**: Legacy octal conversion works for all targets, and numeric separator downleveling now converts `0b`/`0o`/`0x` prefixed literals to decimal at targets < ES2021 when the original had separators. Remaining: enum IIFE initializer paths still use un-converted source text (affects `es5-oldStyleOctalLiteralInEnums`, `strictModeOctalLiterals`).
- **Missing hoisted temp var declarations (~8 tests)**: When lowering optional chaining, nullish coalescing, or element access chains, tsc hoists temporary variable declarations (`var _a, _b`) to the top of the enclosing scope. tsz omits these declarations. Affects `elementAccessChain.2`, `nullishCoalescingOperator10/12`, `spreadUnionPropOverride`.
- **Parenthesization mismatches (~12 tests)**: Various parenthesization differences — extra parens around cast results, missing parens in extends clauses, extra parens around yield, missing parens on async arrow params. Multiple sub-issues.
- ~~**Missing `Object.defineProperty(exports, "__esModule")` for moduleDetection=force (~10 tests)**~~ — **FIXED** (see "Fixed This Session").
- **Remaining duplicate-export affected tests (~90)**: The overload dedup fix resolves the duplicate `exports.X = X;` issue, but ~90 tests that had this as one of multiple issues remain failing due to other causes (missing helpers, wrong comment placement, module transform gaps). No action needed on the dedup side; remaining failures need other fixes.
- **`exports.default` ordering (~19 remaining tests, was ~36)**: For named default function exports, tsc emits
  `exports.default = f;` before the function — now fixed (+14 tests). Remaining cases involve: (1) anonymous
  default functions where `default_1` naming isn't applied in the transform path, (2) `exports.default` emitted
  in the preamble before non-hoisted variable statements (e.g., `var before = func();` appearing after
  `exports.default = func;`), and (3) system module export ordering. These remaining cases need different
  approaches (preamble-level default export for non-hoisted declarations, anonymous name synthesis fix).
- **Missing `exports.X = X;` for scoped declarations (~42 tests)**: tsc emits `exports.X = X;` for class/function exports declared inside nested scopes (e.g., conditionals, namespaces). The export collector only scans top-level statements, missing these. Would need deeper AST traversal or a different approach.
- `ambientModuleDeclarationWithReservedIdentifierInDottedPath` / `ambientModuleDeclarationWithReservedIdentifierInDottedPath2`: ambient dotted module declarations now still emit wrong declaration shapes when mixed with declaration emit filtering; requires namespace/ambient-module emitter refactor, so deferred for later session.
- `abstractPropertyInitializer` / `abstractPropertyDeclaration`: DTS accessor parity still regresses on mixed abstract/private getter/setter edge cases; we fixed only private setter parameter naming and deferred broader declaration-transform compatibility work.
- `accessor*` and `private*` DTS test filters: remaining failures appear to require cross-module declaration helper/mapping changes, which is outside the smallest emitter-only fix scope for this pass.
- `crates/tsz-emitter/src/declaration_emitter/tests.rs: test_variable_declaration_infers_accessor_object_type_from_initializer_when_type_cache_missing`: this failure predates this pass and is currently blocked by a broader declaration-emitter regression in the same module; skipped to keep this change focused on emitter transform comment ordering.
- `./scripts/emit/run.sh` full run (`JS+DTs`) and `scripts/emit` broader checks: large pre-existing failure set (6,828 failures total for JS+DTS) plus 2 timeouts remain; deferred for dedicated conformance/reporter work outside this smallest parity pass.
- **Class field initializer lowering drops trailing comments (~30-40 multi-issue tests)**: When class
  fields like `a = z; // error` are lowered into the constructor body at targets < ES2022
  (`this.a = z;`), the trailing comment is not carried into the constructor. The lowering path
  in `emit_constructor_body_with_prologue` (functions.rs) collects field name and initializer but
  does not capture or re-emit trailing comments from the original field declaration's source position.
  This is separate from the "erased member comment over-consumption" fix above — it affects fields
  WITH initializers that are lowered (not erased). Fixing would require scanning for trailing comments
  after each field initializer's source position and emitting them after the lowered `this.x = value;`.
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
