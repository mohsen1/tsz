# Continue Prompt for AI Coders

Copy this prompt to start a new AI coding session:

---

## Prompt

You are continuing work on **tsz** - a TypeScript compiler written in Rust. The goal is to match `tsc` behavior exactly.

### Your Workflow

1. **Read `docs/AI_WORKFLOW.md`** for the full workflow guide
2. **Run conformance** to see current status:
   ```bash
   ./scripts/conformance.sh run --max 1000
   ```
3. **Pick an error code** from "Top Error Code Mismatches" (prefer high `missing` count)
4. **Use Gemini** for questions - ALWAYS include relevant files:
   ```bash
   ./scripts/ask-gemini.mjs --include=src/relevant/file.rs "Your specific question"
   ```
5. **Make small, targeted fixes** - one error type at a time
6. **Verify** with conformance before committing
7. **Commit and push** when pass rate improves

### Current Focus Areas

Based on recent conformance results:
- TS2322 (type not assignable) - highest missing count
- TS2304 (cannot find name) - balance of missing/extra
- TS2300 (duplicate identifier) - many missing

### Key Commands

```bash
# Build
cargo build --release

# Conformance
./scripts/conformance.sh run --max 1000
./scripts/conformance.sh run --error-code 2322 --max 200

# Compare with tsc
npx tsc --noEmit file.ts
./.target/release/tsz file.ts --noEmit

# Ask Gemini (ALWAYS use --include)
./scripts/ask-gemini.mjs --include=src/checker/file.rs "Question"
```

### Rules

1. **Always read before writing** - understand existing code
2. **Use Gemini** - don't guess, ask with proper file context
3. **Small commits** - one fix per commit
4. **No regressions** - pass rate must not drop
5. **Follow architecture** - type logic in Solver, diagnostics in Checker

Start by running conformance to see the current state, then pick an impactful error to fix.
