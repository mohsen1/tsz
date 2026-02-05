# Session TSZ-5: Template Literal Complexity Management

**Started**: 2026-02-05
**Status**: 🔄 IN PROGRESS
**Focus**: Fix template literal expansion timeout

## Problem Statement

The test `test_template_literal_expansion_limit_widens_to_string` is timing out, indicating an exponential complexity explosion in template literal expansion. This is a critical stability issue that can hang the entire compiler.

## Goal

Implement complexity limits for template literal expansion to prevent compiler timeouts, matching `tsc` behavior by widening to `string` when the Cartesian product of unions exceeds a threshold.

## Why This is Priority

1. **Stability**: Timeouts are "denial of service" bugs for the compiler
2. **Conformance**: TypeScript has a hard limit (~100,000 members) for template literal unions
3. **North Star**: Section 4.4 of NORTH_STAR.md lists MAX_TOTAL_EVALUATIONS and MAX_EVALUATE_DEPTH as critical

## Planned Approach

### Step 1: Locate Expansion Logic
Find the function responsible for Cartesian product expansion of template literals.
- Likely in `src/solver/intern.rs` (template_literal constructor)
- Or `src/solver/evaluate.rs` (meta-type evaluation)

### Step 2: Implement Fuel/Counter
- Introduce a counter to track members generated during expansion
- Check against constant: `MAX_TEMPLATE_EXPANSION_SIZE = 100_000`

### Step 3: Implement Widening
- If limit hit, abort expansion and return `TypeId::STRING`
- Ensure correct fallback for base types (string, number, bigint, boolean)

### Step 4: Visitor Integration
- Use visitor pattern from `src/solver/visitor.rs` if needed
- Calculate potential size before performing allocation

## Files to Modify

- `src/solver/types.rs`: Define limit constants
- `src/solver/intern.rs`: Template literal expansion logic
- `src/solver/evaluate.rs`: If expansion happens during meta-type evaluation

## Potential Pitfalls

1. **Memory Usage**: Check should happen during generation, not after
2. **Recursive Templates**: Nested template literals must accumulate complexity correctly
3. **Early Detection**: Need to estimate size before allocating massive vectors

## Mandatory Pre-Implementation Step

Before modifying any code, MUST ask Gemini Pro Question 1:

```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to fix the timeout in test_template_literal_expansion_limit_widens_to_string.
I plan to add a counter to the Cartesian product expansion in the template literal constructor and return TypeId::STRING if it exceeds 100,000.
Which specific function in src/solver handles this expansion, and is there an existing 'fuel' mechanism I should hook into?"
```

## Next Steps

1. Run failing test with `TSZ_LOG=trace` to see where expansion explodes
2. Ask Gemini Pro Question 1 (MANDATORY - do not skip!)
3. Implement based on Gemini's guidance
4. Ask Gemini Pro Question 2: Review my implementation
5. Test and commit

## Dependencies

- Session tsz-1: Core type relations (coordinate to avoid conflicts)
- Session tsz-2: Complete (circular inference)
- Session tsz-3: Narrowing (different domain)
- Session tsz-4: Emitter (different domain)
