# AI Coder Workflow for tsz

This document describes the workflow for AI coders working on conformance fixes.

## Quick Start

```bash
# 1. Build and run conformance
cargo build --release
./scripts/conformance.sh run --max 1000

# 2. Focus on a specific error code
./scripts/conformance.sh run --error-code 2304 --max 200

# 3. Debug a specific test
./scripts/conformance.sh run --filter "testName" --verbose
```

## The Workflow

### Step 1: Identify the Problem

Run conformance and look at the "Top Error Code Mismatches":
```
Top Error Code Mismatches:
  TS2322: missing=54, extra=19   <- missing = we should emit but don't
  TS2304: missing=21, extra=25   <- extra = we emit but shouldn't
```

Pick an error code with high impact. Prefer fixing `missing` errors over `extra`.

### Step 2: Find a Minimal Failing Test

```bash
./scripts/conformance.sh run --error-code XXXX --max 100
```

Pick a simple failing test and compare outputs:
```bash
# Compare tsc vs tsz
npx tsc --noEmit path/to/test.ts
./.target/release/tsz path/to/test.ts --noEmit
```

### Step 3: Ask Gemini Targeted Questions

**CRITICAL**: Always use `scripts/ask-gemini.mjs` with `--include` to provide context:

```bash
# Good - specific files included
./scripts/ask-gemini.mjs \
  --include=src/checker/import_checker.rs \
  --include=src/checker/state_type_resolution.rs \
  "For this test case:
\`\`\`typescript
import m = no;
\`\`\`
tsc emits TS2503 but tsz emits TS2304. Where should TS2503 be emitted?"

# Bad - no context
./scripts/ask-gemini.mjs "How do I fix TS2503?"
```

**Question Templates**:

1. **Finding where to fix**:
   ```
   For this TypeScript code: [code]
   tsc emits [expected error] but tsz emits [actual error].
   1. What AST node type handles this?
   2. Where in the checker should the error be emitted?
   3. Give SPECIFIC file:line locations.
   ```

2. **Understanding existing code**:
   ```
   In [file], the function [name] does [X].
   How does this interact with [Y]?
   What function should I modify to also handle [Z]?
   ```

3. **Code review**:
   ```
   I'm adding this code to [file]:
   [code snippet]
   Does this follow the architecture? Any issues?
   ```

### Step 4: Implement the Fix

1. **Read before writing**: Always read relevant files first
2. **Follow architecture**: Type logic in Solver, diagnostics in Checker
3. **Use existing patterns**: Search for similar error emissions
4. **Small changes**: Make minimal, targeted fixes

### Step 5: Verify

```bash
# Build
cargo build --release

# Test specific case
./.target/release/tsz path/to/test.ts --noEmit

# Run conformance
./scripts/conformance.sh run --max 1000

# Check for regressions
# Pass rate should not drop significantly
```

### Step 6: Commit

Only commit if:
- Pass rate improved or stayed same
- The specific error is fixed
- No new regressions introduced

```bash
git add -A
git commit -m "Fix TS2XXX: Description of what was fixed"
git push origin main
```

## Key Files by Error Type

| Error Category | Key Files |
|----------------|-----------|
| Import errors (TS2307, TS2305, TS1192) | `src/checker/import_checker.rs`, `src/module_resolver.rs` |
| Type resolution (TS2304, TS2503) | `src/checker/state_type_resolution.rs` |
| Type checking (TS2322, TS2345) | `src/checker/state_checking.rs`, `src/checker/type_checking.rs` |
| Parse errors (TS1XXX) | `src/parser/state_*.rs` |
| Function types | `src/checker/function_type.rs`, `src/checker/signature_builder.rs` |

## Common Patterns

### Emitting a Diagnostic

```rust
// In CheckerState methods:
self.error_at_node(node_idx, "Error message", diagnostic_codes::ERROR_CODE);

// Or with formatting:
let message = format_message(diagnostic_messages::TEMPLATE, &[&arg1, &arg2]);
self.error_at_node(node_idx, &message, diagnostic_codes::ERROR_CODE);
```

### Adding a New Diagnostic Code

1. Add to `src/checker/types/diagnostics.rs`:
   ```rust
   pub const NEW_ERROR: u32 = XXXX;
   pub const NEW_ERROR_MESSAGE: &str = "The error message with {0} placeholder";
   ```

2. Use it in checker code

### Checking Symbol Flags

```rust
use crate::binder::symbol_flags;

if symbol.flags & symbol_flags::TYPE != 0 { /* is a type */ }
if symbol.flags & symbol_flags::VALUE != 0 { /* is a value */ }
if symbol.flags & symbol_flags::ALIAS != 0 { /* is an import alias */ }
```

## Debugging Tips

1. **Trace execution**: `RUST_LOG=debug ./.target/release/tsz file.ts`
2. **Compare AST**: Check if parser produces correct AST
3. **Check symbol table**: Verify binder creates correct symbols
4. **Follow the flow**: Checker → Solver → Type resolution

## Don'ts

- Don't make changes without reading existing code first
- Don't add `#[ignore]` to tests
- Don't commit if pass rate drops
- Don't guess - ask Gemini with proper context
- Don't fix multiple unrelated things in one commit

## Current Status

Check conformance results for current pass rate and top issues:
```bash
./scripts/conformance.sh run --max 1000
```

Focus on errors with highest `missing` count first - these have the most impact on conformance.
