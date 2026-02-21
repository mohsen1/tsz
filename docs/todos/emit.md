# Emitter TODO — Skipped / Investigated Issues

## Pattern Analysis (JS-only mode, baseline 9118/13623 = 66.9%)

### Fixed This Session
- **Orphaned comments at end of class body** (+27 tests): Comments after erased members
  leaked past the closing `}`. Fixed by advancing `comment_emit_idx` past remaining
  comments inside the class body boundary after the member loop.

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
