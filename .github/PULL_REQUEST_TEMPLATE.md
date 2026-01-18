## Description

<!-- Provide a brief description of the changes in this PR -->

## Code Review Checklist

- [ ] No test-aware code (no `file_name.contains()` for behavior changes)
- [ ] No whack-a-mole error suppression
- [ ] Fixed root cause, not symptoms
- [ ] All tests pass (`cargo test` or `./test.sh`)
- [ ] Conformance tests reviewed
- [ ] Reviewed [agents.md](../agents.md) architecture rules

## Testing

### Tests Run
<!-- Describe what tests were run -->
- [ ] Unit tests: `./test.sh`
- [ ] Conformance tests: `./conformance/run-conformance.sh`

### Conformance Test Results
<!--
Compare before/after results if applicable:
- Exact match: X%
- Missing errors: X
- Extra errors: X
-->

### Test File Changes
<!-- List any changes to test files or test infrastructure -->

## Architecture

### Architectural Fit
<!-- How does this change align with the project architecture? -->

### New Patterns
<!-- Are any new patterns or abstractions introduced? -->

### Root Cause vs Symptom
<!-- Explain why this is a root cause fix, not a symptom patch -->

## References

- [agents.md](../agents.md) - Development guidelines and workflow
- [PROJECT_DIRECTION.md](../PROJECT_DIRECTION.md) - Architecture rules and anti-patterns
- [specs/WASM_ARCHITECTURE.md](../specs/WASM_ARCHITECTURE.md) - WASM architecture
- [specs/SOLVER.md](../specs/SOLVER.md) - Type solver design (if relevant)

<!-- Link to related issues, specs, or documentation -->
