# Emitter TODO — Skipped / Investigated Issues

## Pattern Analysis (JS+DTS mode, current 9228/13623 = 67.7% JS, 762/2173 = 35.1% DTS)

### Fixed This Session
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

### Investigated but Deferred (this session)

- **Remaining duplicate-export affected tests (~90)**: The overload dedup fix resolves the duplicate `exports.X = X;` issue, but ~90 tests that had this as one of multiple issues remain failing due to other causes (missing helpers, wrong comment placement, module transform gaps). No action needed on the dedup side; remaining failures need other fixes.
- **`exports.default` ordering (~19 tests)**: tsc emits `exports.default = X;` in the preamble (before the function), but tsz emits it after the function body. Requires reordering default export assignment in the CommonJS preamble emission path.
- **Missing `exports.X = X;` for scoped declarations (~42 tests)**: tsc emits `exports.X = X;` for class/function exports declared inside nested scopes (e.g., conditionals, namespaces). The export collector only scans top-level statements, missing these. Would need deeper AST traversal or a different approach.
- `ambientModuleDeclarationWithReservedIdentifierInDottedPath` / `ambientModuleDeclarationWithReservedIdentifierInDottedPath2`: ambient dotted module declarations now still emit wrong declaration shapes when mixed with declaration emit filtering; requires namespace/ambient-module emitter refactor, so deferred for later session.
- `abstractPropertyInitializer` / `abstractPropertyDeclaration`: DTS accessor parity still regresses on mixed abstract/private getter/setter edge cases; we fixed only private setter parameter naming and deferred broader declaration-transform compatibility work.
- `accessor*` and `private*` DTS test filters: remaining failures appear to require cross-module declaration helper/mapping changes, which is outside the smallest emitter-only fix scope for this pass.
- `crates/tsz-emitter/src/declaration_emitter/tests.rs: test_variable_declaration_infers_accessor_object_type_from_initializer_when_type_cache_missing`: this failure predates this pass and is currently blocked by a broader declaration-emitter regression in the same module; skipped to keep this change focused on emitter transform comment ordering.
- `./scripts/emit/run.sh` full run (`JS+DTs`) and `scripts/emit` broader checks: large pre-existing failure set (6,828 failures total for JS+DTS) plus 2 timeouts remain; deferred for dedicated conformance/reporter work outside this smallest parity pass.

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

5. **decorator** (~70 tests): Decorator transform not implemented. Related to
   `missing_helper` — decorators need both the transform and `__decorate` helper.

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

12. **"use strict" for module=preserve** (partially fixed): The `Preserve` module kind
    mapping bug in `args.rs` is now fixed. Remaining failures are in the test runner's
    post-processing logic (`cli-transpiler.ts` lines 422-426).

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

18. **Extra blank line after class static properties** (~2 tests):
    `amdImportNotAsPrimaryExpression` and `classInConvertedLoopES5(target=es2015)` have
    an extra blank line after `ClassName.staticProp = value;` before the next statement.
    This is a different code path from the namespace body fix — it occurs in the class
    declaration's static field initializer emission (`declarations.rs:1010-1051`) where
    `write_line()` after the last static field interacts with the `emit_source_file()`
    loop's newline logic. The AMD variant also involves the module wrapper body path.
    Needs careful investigation of the newline state after class emit completes.

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
