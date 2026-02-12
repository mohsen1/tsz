# Slice 2 Conformance Test Fixes - Implementation Guide

## Current Status

**Slice**: offset 3146, max 3146
**Failing Tests**: 10 tests + 2 timeouts

## Issue Analysis Complete

### 1. Export= Import Error Codes (7 tests) - READY TO IMPLEMENT

**Problem**: When a module uses `export = Foo`, TypeScript prohibits named imports like `import { Foo }`. We currently emit generic TS2305 but should emit specific error codes based on configuration.

**Tests Affected**:
- importNonExportedMember6.ts - Expected [TS2497, TS2596], got TS2305
- importNonExportedMember10.ts - Expected [TS2497, TS2596], got TS2305
- importNonExportedMember11.ts - Expected [TS2497, TS2595], got TS2305
- importNonExportedMember4.ts - Expected [TS2497, TS2617], got TS2305
- importNonExportedMember5.ts - Expected [TS2497, TS2616], got TS2305

**Implementation** (in `crates/tsz-checker/src/import_checker.rs` around line 323):

```rust
// Replace the TS2305 emission block (around line 323-334) with:
} else {
    // Check if the module uses `export =` syntax before emitting TS2305
    let uses_export_equals = if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        arena.source_files.iter().any(|source_file| {
            source_file.statements.as_ref().map_or(false, |stmts| {
                stmts.nodes.iter().any(|&stmt_idx| {
                    arena.get(stmt_idx)
                        .map_or(false, |node| node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT)
                })
            })
        })
    } else {
        false
    };

    if uses_export_equals {
        // Module uses `export =`, emit appropriate error
        use tsz_common::common::ModuleKind;
        let is_es_module = matches!(
            self.ctx.compiler_options.module,
            ModuleKind::ES6 | ModuleKind::ES2015 | ModuleKind::ES2020 | ModuleKind::ES2022 | ModuleKind::ESNext | ModuleKind::Preserve
        );
        let es_module_interop = self.ctx.compiler_options.es_module_interop.unwrap_or(false);

        if is_es_module {
            if es_module_interop {
                // TS2497: Suggest turning on esModuleInterop and using default import
                let message = format_message(
                    diagnostic_messages::THIS_MODULE_CAN_ONLY_BE_REFERENCED_WITH_ECMASCRIPT_IMPORTS_EXPORTS_BY_TURNING_ON,
                    &[module_name, "esModuleInterop"],
                );
                self.error_at_node(
                    specifier.name,
                    &message,
                    diagnostic_codes::THIS_MODULE_CAN_ONLY_BE_REFERENCED_WITH_ECMASCRIPT_IMPORTS_EXPORTS_BY_TURNING_ON,
                );
            } else {
                // TS2596: Suggest enabling esModuleInterop
                let message = format_message(
                    diagnostic_messages::CAN_ONLY_BE_IMPORTED_BY_TURNING_ON_THE_ESMODULEINTEROP_FLAG_AND_USING_A_DEFAULT_IMP,
                    &[module_name],
                );
                self.error_at_node(
                    specifier.name,
                    &message,
                    diagnostic_codes::CAN_ONLY_BE_IMPORTED_BY_TURNING_ON_THE_ESMODULEINTEROP_FLAG_AND_USING_A_DEFAULT_IMP,
                );
            }
        } else {
            // CommonJS module
            if es_module_interop {
                // TS2617: Suggest import = require or default import with esModuleInterop
                let message = format_message(
                    diagnostic_messages::CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_BY_TURNING_ON_THE_ESMODULEINTEROP,
                    &[module_name, import_name, &format!("\"{}\"", module_name)],
                );
                self.error_at_node(
                    specifier.name,
                    &message,
                    diagnostic_codes::CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_BY_TURNING_ON_THE_ESMODULEINTEROP,
                );
            } else {
                // TS2616: Suggest import = require or default import
                let message = format_message(
                    diagnostic_messages::CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_A_DEFAULT_IMPORT,
                    &[module_name, import_name, &format!("\"{}\"", module_name)],
                );
                self.error_at_node(
                    specifier.name,
                    &message,
                    diagnostic_codes::CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_A_DEFAULT_IMPORT,
                );
            }
        }
    } else {
        // TS2305: Symbol doesn't exist in the module at all
        let message = format_message(
            diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
            &[module_name, import_name],
        );
        self.error_at_node(
            specifier.name,
            &message,
            diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
        );
    }
}
```

**Location**: Search for `// TS2305: Symbol doesn't exist in the module at all` around line 324 in `import_checker.rs`. There are two occurrences - modify the FIRST one (which has TS2459 context above it).

### 2. Import Helpers Checking (3 tests) - NEEDS INVESTIGATION

**Tests**:
- importHelpersNoModule.ts - Expected TS2354, got no errors
- importHelpersInAmbientContext.ts - Expected no errors, got TS1182
- importHelpersVerbatimModuleSyntax.ts - Expected no errors, got TS1203

**Problem**: When `--importHelpers` is enabled and features like decorators/async-await are used, tsz should:
1. Check if 'tslib' module exists
2. Emit TS2354 if not found
3. NOT check import helpers in ambient contexts

**Next Steps**: Search for where TS1182 and TS1203 are emitted. These are being incorrectly triggered in import helper contexts.

### 3. Import Non-Exported Member (1 test) - NEEDS FIX

**Test**: importNonExportedMember.ts
**Expected**: TS2460 (module declares X locally but exports as Y)
**Actual**: No errors

**Problem**: This test has:
```typescript
// a.ts
export { foo, bar as baz };
// b.ts
import { foo, bar } from "./a";  // bar is exported as 'baz'
```

The checker isn't detecting that `bar` exists locally but is exported under a different name (`baz`).

**Fix Location**: Same area as issue #1, in the `check_local_symbol_and_renamed_export` logic.

### 4. False TS2580 Suggestion (1 test)

**Test**: importNonExportedMember12.ts
**Expected**: No errors
**Actual**: TS2580 (suggests installing @types/node)

**Problem**: False positive - suggesting @types/node when it's not relevant.

**Fix**: Find where TS2580 is emitted and add logic to avoid suggesting @types packages in module contexts.

### 5. Timeouts (2 tests) - PERFORMANCE ISSUE

**Tests**:
- resolvingClassDeclarationWhenInBaseTypeResolution.ts (>5s)
- silentNeverPropagation.ts (>5s)

**Problem**: Infinite loops or exponential complexity.

**Next Steps**:
1. Run with tracing to identify hot loops
2. Add cycle detection or depth limits
3. Consider memoization

## Implementation Notes

- **CRITICAL**: Kill all background cargo/rustfmt processes before editing:
  ```bash
  killall -9 cargo rustc; sleep 2
  ```

- Background watchers were reverting file changes during this session. Recommend using git directly:
  ```bash
  # Make edits, then immediately:
  git add file.rs
  git commit -m "fix: description"
  git push
  ```

## Testing

After implementing fixes:

```bash
# Build
cargo build --profile dist-fast -p tsz-cli -p tsz-conformance

# Test slice
./scripts/conformance.sh run --offset 3146 --max 3146

# Test specific failing test
./scripts/conformance.sh run-test TypeScript/tests/cases/compiler/importNonExportedMember6.ts
```

## Priority Order

1. **Export= errors** (7 tests) - Implementation ready, just needs file edit to stick
2. **importNonExportedMember** (1 test) - Small fix in same file
3. **Import helpers** (3 tests) - Needs investigation
4. **False TS2580** (1 test) - Should be quick
5. **Timeouts** (2 tests) - May require deeper investigation

**Estimated**: Issues #1-2 should fix 8/12 failures quickly if file edits can be applied.
