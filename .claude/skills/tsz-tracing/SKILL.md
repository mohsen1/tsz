---
name: tsz-tracing
description: Debug tsz compiler issues using the built-in tracing infrastructure. Use when investigating type inference bugs, conformance failures, or understanding runtime behavior. Provides hierarchical trace output of solver, checker, and binder operations.
---

# TSZ Tracing Debug Skill

This skill helps you debug tsz compiler issues using the built-in tracing infrastructure. The tracing system provides detailed runtime information about type inference, narrowing, assignability checks, and more.

## When to Use This Skill

Use this skill when you need to:
- Debug a failing conformance test
- Understand why type inference produces unexpected results
- Trace narrowing and control flow analysis
- Investigate assignability check failures
- Compare tsz behavior to tsc for specific inputs
- Find where in the code path a bug occurs

## Quick Start

```bash
# Tree format - hierarchical, human-readable (recommended for debugging)
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts

# JSON format - machine-readable, good for tooling
TSZ_LOG=debug TSZ_LOG_FORMAT=json cargo run -- file.ts

# Plain text format (default)
TSZ_LOG=debug cargo run -- file.ts
```

## Environment Variables

| Variable | Values | Description |
|----------|--------|-------------|
| `TSZ_LOG` | `trace`, `debug`, `info`, `warn`, `error` | Log level filter (same syntax as RUST_LOG) |
| `TSZ_LOG_FORMAT` | `tree`, `json`, `text` | Output format |
| `RUST_LOG` | Same as TSZ_LOG | Fallback if TSZ_LOG not set |

## Fine-Grained Filtering

Target specific modules for focused debugging:

```bash
# Only solver operations
TSZ_LOG="tsz::solver=debug" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Only checker operations
TSZ_LOG="tsz::checker=debug" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Multiple modules with different levels
TSZ_LOG="tsz::solver=trace,tsz::checker=debug" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Narrowing specifically (very detailed)
TSZ_LOG="tsz::solver::narrowing=trace" TSZ_LOG_FORMAT=tree cargo run -- file.ts
```

## Instrumented Areas

The following areas have tracing instrumentation:

### Solver (`src/solver/`)
- `narrowing.rs` - Type narrowing operations (typeof, instanceof, discriminants)
- `subtype.rs` - Subtype relationship checks
- Type inference and instantiation

### Checker (`src/checker/`)
- `context.rs` - Type context operations
- `state.rs` - Node type caching (`get_type_of_node`)
- `assignability_checker.rs` - Assignability checks
- `class_inheritance.rs` - Class hierarchy analysis
- `dispatch.rs` - Type dispatch operations

### CLI (`src/cli/`)
- `driver.rs` - Compilation spans, file checking
- `build.rs` - Build operations

## Debugging Workflow

### 1. Reproduce with Minimal Input

Create a minimal `.ts` file that reproduces the issue:

```typescript
// test.ts
function example<T>(x: T) {
  if (typeof x === "string") {
    return x.length; // Should narrow T to string
  }
}
```

### 2. Run with Tracing

```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- test.ts 2>&1 | head -100
```

### 3. Compare with tsc

```bash
# Run tsc for expected behavior
npx tsc --noEmit test.ts

# Run tsz with tracing for actual behavior
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- test.ts
```

### 4. Focus on Specific Operations

If output is too verbose, narrow down:

```bash
# Just narrowing operations
TSZ_LOG="tsz::solver::narrowing=trace" TSZ_LOG_FORMAT=tree cargo run -- test.ts

# Just type lookup operations
TSZ_LOG="tsz::checker::state=trace" TSZ_LOG_FORMAT=tree cargo run -- test.ts
```

## Reading Tree Output

The tree format shows hierarchical span relationships:

```
  0ms DEBUG tsz::solver::narrowing narrow_by_typeof
    source_type=42, typeof_result="string"
  │ 0ms TRACE tsz::solver::narrowing Narrowing union type with 3 members
  │ 0ms TRACE tsz::solver::narrowing Found single matching member, returning 15
```

- Indentation shows nesting (parent spans contain child spans)
- `ms` shows elapsed time
- Level (DEBUG, TRACE, etc.) shows severity
- Module path shows source location
- Key-value pairs show span fields

## Common Debugging Scenarios

### Incorrect Type Narrowing

```bash
TSZ_LOG="tsz::solver::narrowing=trace" TSZ_LOG_FORMAT=tree cargo run -- test.ts
```

Look for:
- `narrow_by_typeof` spans for typeof narrowing
- `narrow_by_truthiness` for boolean context narrowing
- Member filtering operations

### Assignability Failures

```bash
TSZ_LOG="tsz::checker::assignability=debug" TSZ_LOG_FORMAT=tree cargo run -- test.ts
```

Look for:
- Subtype check results
- Which types are being compared
- Why assignability fails

### Type Inference Issues

```bash
TSZ_LOG="tsz::solver=debug" TSZ_LOG_FORMAT=tree cargo run -- test.ts
```

Look for:
- Generic instantiation
- Constraint checking
- Type parameter resolution

## Tips

1. **Pipe to file**: Traces can be large. Use `2> trace.log` to capture stderr
2. **Use `head`/`tail`**: Filter output with `| head -200` or `| tail -100`
3. **Use `grep`**: Search for specific type IDs: `| grep "type_id=42"`
4. **Tree is best**: The tree format is easiest to read for debugging
5. **Start broad, then narrow**: Begin with `debug`, then use module filters

## Implementation Details

Tracing is configured in `src/tracing_config.rs`. The subscriber is only initialized when `TSZ_LOG` or `RUST_LOG` is set, so there's zero overhead in normal usage.

Output always goes to stderr to avoid interfering with compiler output (diagnostics, `--showConfig`, or LSP JSON-RPC on stdout).
