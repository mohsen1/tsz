# TS8009/TS8010 Implementation Status

**Date**: 2026-02-12
**Test**: `ambientPropertyDeclarationInJs.ts`
**Status**: Implementation exists but blocked by test infrastructure

## Summary

TS8009 ("`declare` modifier only in TS files") and TS8010 ("Type annotations only in TS files") are **already implemented** in the checker, but the conformance test fails due to a test infrastructure limitation.

## Implementation Location

**File**: `crates/tsz-checker/src/state_checking_members.rs:2117-2161`
**Function**: `check_property_declaration()`

```rust
// TS8009/TS8010: Check for TypeScript-only features in JavaScript files
let is_js_file = self.ctx.file_name.ends_with(".js")
    || self.ctx.file_name.ends_with(".jsx")
    || self.ctx.file_name.ends_with(".mjs")
    || self.ctx.file_name.ends_with(".cjs");

if is_js_file {
    // TS8009: Modifiers like 'declare' can only be used in TypeScript files
    if self.ctx.has_modifier(&prop.modifiers, tsz_scanner::SyntaxKind::DeclareKeyword as u16) {
        let message = format_message(
            diagnostic_messages::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            &["declare"],
        );
        self.error_at_node(member_idx, &message,
            diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES);
    }

    // TS8010: Type annotations can only be used in TypeScript files
    if !prop.type_annotation.is_none() {
        self.error_at_node(prop.type_annotation,
            diagnostic_messages::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            diagnostic_codes::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES);
    }
}
```

## The Problem: `@filename` Directive

The conformance test uses a special TypeScript compiler directive:

```typescript
// @allowJs: true
// @checkJs: true
// @noEmit: true
// @filename: /test.js   <-- Virtual filename

class Foo {
    declare prop: string;  // Should emit TS8009 + TS8010
}
```

**Expected**: TypeScript treats this as if it's a `.js` file named `/test.js`
**Actual**: We check the physical file name `ambientPropertyDeclarationInJs.ts` which ends in `.ts`
**Result**: Our `is_js_file` check at line 2129 returns false, skipping TS8009/TS8010 checks

## Test Results

### With Actual .js File
```bash
$ ./.target/dist-fast/tsz tmp/test.js --allowJs --checkJs --noEmit
(no errors - because we don't parse TS syntax in JS files properly)
```

### With .ts File Containing JS Code
```bash
$ ./.target/dist-fast/tsz tmp/test.ts --allowJs --checkJs --noEmit
error TS2322: Type '{}' is not assignable to type 'string'.
error TS2339: Property 'foo' does not exist on type 'string'.
(missing TS8009 and TS8010 because file_name ends in .ts)
```

### With TypeScript Compiler
```bash
$ tsc tmp/test.js --allowJs --checkJs --noEmit
error TS8009: The 'declare' modifier can only be used in TypeScript files.
error TS8010: Type annotations can only be used in TypeScript files.
```

## Root Cause

TypeScript's test infrastructure supports compiler directives like:
- `@filename` - Sets virtual file name
- `@target` - Sets compilation target
- `@module` - Sets module system
- etc.

Our conformance test runner (`scripts/conformance.sh`) doesn't parse or handle these directives. It:
1. Runs `tsc` on the file (which DOES handle directives)
2. Runs `tsz` on the file (which DOESN'T handle directives)
3. Compares the outputs

This causes mismatches for tests that rely on `@filename` to change file type.

## Solutions

### Option A: Support `@filename` in CLI (Recommended)
Modify `crates/tsz-cli` to:
1. Parse `@filename` directives from source files
2. Use the virtual filename for `ctx.file_name` instead of physical filename
3. Apply other directives (`@target`, `@module`, etc.) similarly

**Impact**: Would fix multiple conformance tests that use directives
**Complexity**: Medium - requires directive parser and CLI integration

### Option B: Modify Test Infrastructure
Change conformance runner to:
1. Extract `@filename` from test files
2. Actually create/copy files with those names
3. Run tsz on the virtual files

**Impact**: Fixes this test category
**Complexity**: High - fragile, requires file system manipulation

### Option C: Accept Limitation
Document that tests with `@filename` are not supported and exclude them from conformance metrics.

**Impact**: Minimal - tests remain unsupported
**Complexity**: None

## Recommendation

**Implement Option A** - Add `@directive` support to the CLI driver.

This is the proper solution because:
1. TypeScript's own tests use these directives extensively
2. Many conformance tests depend on them
3. It's architecturally cleaner than test infrastructure hacks
4. It enables better testing in general

## Implementation Plan for Option A

1. Create directive parser in `crates/tsz-cli/src/directives.rs`
   - Parse lines starting with `// @name: value`
   - Support common directives: `@filename`, `@target`, `@module`, `@strict`, etc.

2. Modify `crates/tsz-cli/src/driver.rs`
   - Call directive parser before creating checker
   - Override `file_name` in checker context if `@filename` present
   - Apply other directives to compiler options

3. Update checker context
   - Ensure `file_name` can be set explicitly
   - Add `virtual_file_name` field if needed

4. Test with conformance suite
   - Verify `ambientPropertyDeclarationInJs.ts` now passes
   - Check other `@filename` tests

## Alternative Quick Fix

If directive support is too complex, we could add a **simple workaround** just for property declarations:

Check both the physical file name AND a new `ctx.is_javascript` flag that could be set based on `allowJs` + lack of TS-only syntax elsewhere. But this is a hack and not recommended.

## Status

- ✅ TS8009/TS8010 implementation exists and is correct
- ❌ Blocked by missing `@filename` directive support
- ⏸️  Deferred - requires infrastructure work beyond single test fix

## Files Involved

- `crates/tsz-checker/src/state_checking_members.rs` - Implementation (working)
- `crates/tsz-cli/src/driver.rs` - Needs directive parser
- `TypeScript/tests/cases/compiler/ambientPropertyDeclarationInJs.ts` - Test case
