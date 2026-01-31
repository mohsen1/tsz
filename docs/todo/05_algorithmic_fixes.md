# O(N²) Algorithmic Fixes (Post-Salsa)

**Reference**: `docs/architecture/SOLVER_REFACTORING_PROPOSAL.md`
**Benchmarks**: `scripts/bench-vs-tsgo.sh` — "O(N²) Algorithmic Pattern Tests" section
**Status**: IN PROGRESS — Pattern 1 (BCT) complete, Patterns 2-3 remaining
**Priority**: High — these are the primary blockers to reaching 5x+ at 200-entity scale

---

## Context

Salsa memoization eliminates redundant recomputation but does **not** fix
algorithms that are inherently O(N²). Three patterns in the solver have
quadratic (or worse) complexity that will remain after Salsa lands:

| # | Pattern | Location | Complexity | Benchmark |
|---|---------|----------|------------|-----------|
| 1 | Best Common Type | `src/solver/infer.rs:1060` | O(N²) — N candidates × N subtype checks | `BCT candidates=N` |
| 2 | Constraint Conflict Detection | `src/solver/infer.rs:135` | O(N²) + O(M×N) — bound pairs | `Constraint conflicts N=N` |
| 3 | Mapped Type Complex Templates | `src/solver/evaluate_rules/mapped.rs:157` | O(N × template_cost) | `Mapped complex template keys=N` |

**Projected combined impact**: Fixing all three + Salsa could push 200-entity
scaling benchmarks from ~1-2x → 4-5x vs tsgo.

---

## Pattern 1: Best Common Type (BCT) — ✅ DONE

Tournament reduction implemented (commit `8c7807d`). Replaced O(N²) candidate scan with O(N) single-pass "king of the hill" approach plus final O(N) validation pass.

---

## Pattern 2: Constraint Conflict Detection

### Current Code

```rust
// infer.rs:137-142 — O(N²) upper bound pairs
for (i, &u1) in self.upper_bounds.iter().enumerate() {
    for &u2 in &self.upper_bounds[i + 1..] {
        if are_disjoint(interner, u1, u2) { ... }
    }
}

// infer.rs:146-159 — O(M×N) lower×upper cross-check
for &lower in &self.lower_bounds {
    for &upper in &self.upper_bounds {
        if !is_subtype_of(interner, lower, upper) { ... }
    }
}
```

### Recommended Fix: Incremental Single-Representative — O(N) amortized

Instead of batch-checking all pairs at the end, maintain a running
representative as bounds are added:

1. When adding upper bound `U_new`: check only against representative `U_rep`
2. If `is_subtype(U_new, U_rep)`, new bound is tighter — becomes representative
3. If `are_disjoint(U_new, U_rep)`, report conflict immediately
4. For lower bounds: only check against the representative upper bound

This is O(1) per bound addition, O(N) total.

### Alternative Options

- **Lattice-based narrowing**: Eagerly compute `effective_upper = U1 & U2 & ... & Un`
  as an intersection type. Check lower bounds against that. More principled but
  requires intersection type creation which has its own costs.
- **Early-exit with sorted bounds**: Sort by specificity, check until first
  compatible pair. Doesn't change worst-case but helps average case.

### Acceptance Criteria

- [ ] `Constraint conflicts N=200` benchmark shows linear scaling
- [ ] Conflict detection still catches all genuine conflicts
- [ ] No false positives introduced

---

## Pattern 3: Mapped Type Complex Templates

### Current Code

```rust
// evaluate_rules/mapped.rs:157-202
for key_name in key_set.string_literals {               // O(N) properties
    let property_type = self.evaluate(
        instantiate_type(self.interner(), mapped.template, &subst)  // O(template_size)
    );
}
```

For each of N properties, instantiates and evaluates the template. With
complex conditional templates (e.g., `FormFields<T>`), each evaluation is
expensive.

### Recommended Fix: Template Pre-Analysis + Lazy Evaluation

**Phase A — Template Classification** (high impact, low risk):

Analyze the template once to classify it:
- **Identity** (`T[K]`): copy properties directly, O(N) total
- **Modifier-only** (`+?`, `readonly`): copy with modifier change, O(N)
- **Simple transform** (template doesn't recurse): batch process
- **Complex**: fall back to per-property evaluation

This matches tsc's `isSimpleMappedType` optimization.

**Phase B — Lazy Property Evaluation** (medium impact, medium risk):

Represent mapped types as virtual objects that compute properties on demand.
If a consumer only accesses K properties out of N, only K evaluations happen.

**Phase C — Parallel Evaluation** (low impact, future):

With Salsa, per-property evaluations are pure and can run in parallel via
rayon. Doesn't reduce total work but divides wall-clock time.

### Acceptance Criteria

- [ ] `Mapped complex template keys=200` benchmark shows < 3x slowdown vs keys=100
- [ ] Simple mapped types (Partial, Readonly, Pick) are O(N) not O(N × template)
- [ ] No conformance regressions on mapped type tests

---

## Benchmark Usage

Run the O(N²) benchmarks specifically:

```bash
# Full suite (tests N = 25, 50, 100, 200 for each pattern)
./scripts/bench-vs-tsgo.sh

# Quick smoke test (one size per pattern)
./scripts/bench-vs-tsgo.sh --quick
```

Look for the **"O(N²) Algorithmic Pattern Tests"** section in the output.
Compare scaling ratios across N values — linear algorithms should show
roughly constant time-per-entity as N grows.

### How to Read Results

For BCT with tournament fix, expect:
```
BCT candidates=25    →  X ms
BCT candidates=50    →  ~2X ms    (linear: 2x entities = 2x time)
BCT candidates=100   →  ~4X ms
BCT candidates=200   →  ~8X ms
```

Without fix (current O(N²)):
```
BCT candidates=25    →  X ms
BCT candidates=50    →  ~4X ms    (quadratic: 2x entities = 4x time)
BCT candidates=100   →  ~16X ms
BCT candidates=200   →  ~64X ms
```

---

## Implementation Order

1. ~~**BCT Tournament**~~ — ✅ DONE (commit `8c7807d`)
2. **Constraint Incremental Representative** — medium impact, also localized
3. **Mapped Template Pre-Analysis** — requires more design, broader changes

All three are independent of each other and independent of Salsa (they can be
done before or after Salsa lands). However, their impact is most visible when
combined with Salsa memoization, since Salsa removes the redundant-recomputation
noise and makes the algorithmic scaling pattern clear.
