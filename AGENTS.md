# GOAL

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

Important document: docs/architecture/NORTH_STAR.md

---

## Debugging
- **NEVER use `eprintln!`** — use the `tsz-tracing` skill instead.


## CRITICAL: Use Skills

It's very important to use the available skills frequently to maximize productivity and code quality.

### tsz-gemini skill
Use for:
- codebase questions
- architecture understanding
- code reviews
- implementation strategies
- fixing bugs and failing tests

**This skill wraps ask-gemini.mjs - use it when really really stuck!**

### tsz-tracing skill
**🚫 NEVER use `eprintln!` for debugging - use the tracing skill instead.**

Use for debugging:
- conformance test failures
- type inference issues
- narrowing and control flow analysis
- assignability check problems

Quick start:
```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts
```

**Read the full skill at `.claude/skills/tsz-tracing/SKILL.md` for:**
- Adding `#[tracing::instrument]` and `trace!()` calls
- Filtering by module (`TSZ_LOG="wasm::solver::narrowing=trace"`)
- Reading hierarchical tree output

---

## CRITICAL: Debugging with Tracing (NOT eprintln!)

**🚫 NEVER use `eprintln!` for debugging. We have proper tracing infrastructure.**

### Why Not eprintln!
- `eprintln!` statements get left behind in production code
- No filtering capability - all or nothing
- No hierarchical context - hard to understand call relationships
- No timing information
- Creates noise in CI/test output

### Use the Tracing Crate Instead

We use the `tracing` crate with `tracing-tree` for beautiful hierarchical output.

#### Quick Start
```bash
# Run with debug-level tracing in tree format
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Run tests with tracing (capture stderr)
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo test test_name -- --nocapture 2>&1 | head -200
```

#### Adding Tracing to Code

**For function-level spans (recommended for most functions):**
```rust
use tracing::{trace, debug, span, Level};

#[tracing::instrument(level = "trace", skip(interner), fields(count = types.len()))]
pub fn my_function(interner: &dyn TypeDatabase, types: &[TypeId]) -> TypeId {
    // Function body - automatically creates a span with timing
    trace!("Processing {} types", types.len());
    // ...
}
```

**For inline tracing points:**
```rust
use tracing::{trace, debug};

fn some_function() {
    trace!(type_id = %id.0, "Resolved type");
    debug!(members = members.len(), "Narrowing union");
}
```

**For manual spans (when you need custom scope):**
```rust
use tracing::{trace_span, trace};

fn process_items(items: &[Item]) {
    let _span = trace_span!("process_items", count = items.len()).entered();
    for item in items {
        trace!(item_id = item.id, "Processing item");
    }
}
```

#### Log Levels

| Level | Use For |
|-------|---------|
| `error` | Actual errors, should never happen in normal operation |
| `warn` | Unusual conditions that may indicate problems |
| `info` | High-level milestones (file loaded, check complete) |
| `debug` | Useful debugging info (function entry, major decisions) |
| `trace` | Very detailed info (loop iterations, individual checks) |

#### Filtering Output

```bash
# Only solver module at trace level
TSZ_LOG="wasm::solver=trace" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Multiple modules with different levels
TSZ_LOG="wasm::solver::narrowing=trace,wasm::checker=debug" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Specific submodule for focused debugging
TSZ_LOG="wasm::solver::subtype=trace" TSZ_LOG_FORMAT=tree cargo run -- file.ts
```

#### Performance Testing with Tracing

For performance work, add timing spans:
```rust
#[tracing::instrument(level = "debug", skip(self), fields(count = members.len()))]
fn expensive_operation(&self, members: &[TypeId]) -> TypeId {
    // The span automatically records timing
    // ...
}
```

Then run with:
```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run --release -- file.ts 2>&1 | grep "ms"
```

#### Key Instrumented Areas

Already instrumented (search for `#[tracing::instrument]` or `trace!`):
- `src/solver/narrowing.rs` - Type narrowing operations
- `src/solver/subtype.rs` - Subtype checks
- `src/solver/expression_ops.rs` - Best common type
- `src/solver/intern.rs` - Type interning
- `src/checker/state.rs` - Type caching
- `src/checker/context.rs` - Type context

#### Debugging Workflow

1. **Reproduce the issue** with minimal TypeScript input
2. **Run with tracing**: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- test.ts 2>&1 | head -200`
3. **Narrow the filter** if too verbose: `TSZ_LOG="wasm::solver::narrowing=trace"`
4. **Find the divergence point** - where does the trace show wrong behavior?
5. **Add more tracing** if needed to that specific area
6. **Compare with expected** - what should happen vs what does happen?

---

## Tesing 
- Write unit tests for any new functionality
- It is a good idea to write a failing test first before implementing a feature

## Profiling
- Do NOT bind to port 3000. Disable profiler web UIs (`samply --no-open`, etc).

## Git Workflow
- Commit frequently with clear messages
- Push branches to remote regularly and rebase from main before and after each commit
- Only add files you touched, do not `git add -A`
- Make semantic and short commit headers
- Important: When syncing, also push to remote


Now, make sure repo is setup properly. Run `scripts/setup.sh`
