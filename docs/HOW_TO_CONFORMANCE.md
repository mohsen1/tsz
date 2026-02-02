# Debugging Conformance Test Failures

This guide walks through a systematic approach to investigating and fixing conformance test failures.

## Prerequisites

Familiarize yourself with the tools:

```bash
./scripts/conformance/run.sh --help
./scripts/ask-gemini.mjs --help
```

## Step 1: Identify the Problem

Start by filtering tests to a specific error code or pattern:

```bash
# Filter by TypeScript error code
./scripts/conformance/run.sh --error-code=2322

# Filter by test name pattern
./scripts/conformance/run.sh --filter="generic"

# Combine both for precision
./scripts/conformance/run.sh --error-code=2322 --filter="conditional"
```

Common error codes to investigate:
- `TS2304` - Cannot find name
- `TS2322` - Type not assignable
- `TS2339` - Property does not exist
- `TS2345` - Argument type mismatch

## Step 2: Understand the Failure

Use `--print-test` to see exactly what's happening:

```bash
./scripts/conformance/run.sh --error-code=2322 --print-test --max=5
```

This shows:
- The test file content
- Directives and compiler options
- Expected vs actual errors
- Where tsz diverges from tsc

**Goal**: Fully understand what tsz produces vs what tsc expects before writing any code.

## Step 3: Research with Gemini

Ask targeted questions using the appropriate preset. Run queries in parallel for faster iteration:

```bash
# Ask about type assignment behavior
./scripts/ask-gemini.mjs --solver "How does assignability work for conditional types?"

# Ask about a specific code path
./scripts/ask-gemini.mjs --checker "Where do we emit TS2322 errors?"

# Include specific files for context
./scripts/ask-gemini.mjs --include=src/solver/assignability.rs "Why might X not be assignable to Y?"
```

**Tips:**
- Use focused presets (`--solver`, `--checker`, `--binder`) for better context
- Ask from multiple angles to triangulate the issue
- Run 2-3 questions in parallel to explore faster

## Step 4: Create a Fix Plan

Once you understand the problem, ask Gemini for a concrete fix:

```bash
./scripts/ask-gemini.mjs --solver \
  --include=src/solver/assignability.rs \
  --include=src/solver/conditional.rs \
  "Given [learnings from step 3], create a fix plan for [specific issue]"
```

Always use `--include` to ensure relevant files are in context.

## Step 5: Apply and Verify

After implementing the fix:

```bash
# Verify the specific error code improved
./scripts/conformance/run.sh --error-code=2322

# CRITICAL: Run full conformance to check for regressions
./scripts/conformance/run.sh
```

**Warning**: Never measure pass rate improvement with `--max`. Conformance is a game of whack-a-mole—fixing one thing can break another. Always run the full suite to get the true pass rate.

## Quick Reference

| Task | Command |
|------|---------|
| Filter by error code | `--error-code=TSXXXX` |
| See test details | `--print-test` |
| Limit test count | `--max=N` |
| Filter by name | `--filter=PATTERN` |
| Full pass rate | `./scripts/conformance/run.sh` (no --max) |

## Workflow Summary

1. **Filter** → Find failing tests by error code or pattern
2. **Understand** → Use `--print-test` to see expected vs actual
3. **Research** → Ask Gemini targeted questions (in parallel)
4. **Plan** → Get a concrete fix plan with relevant files included
5. **Verify** → Run full conformance (no `--max`) to measure real improvement
