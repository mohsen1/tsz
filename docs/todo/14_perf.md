# Performance Analysis Summary

1. Generic Functions Scaling (3.84x → 1.31x degradation)

Root Causes:

- Eager Structural Interning: Every generic instantiation requires hashing the entire type structure for deduplication in TypeInterner. For 200 generics, this means hashing all 200 TypeIds in every combination.
- "Lawyer" Variance Checks: Linear scan through all type arguments for variance rules (Covariant/Contravariant/Bivariant) during unification.
- Coinductive Cycle Detection: Deep generic nesting increases cycle_stack scans, approaching the MAX_SUBTYPE_DEPTH limit.

File: src/solver/instantiate.rs, src/solver/lawyer.rs

---

2. BCT Algorithm (2.54x → 1.27x degradation)

Root Cause - O(N·D²) Complexity:

The find_common_base_class function in src/solver/infer.rs (lines 1060-1062):

for &ty in types.iter().skip(1) { // O(N)
if let Some(ty_bases) = self.get_class_hierarchy(ty) {
// O(D) \* O(D) = O(D²) operation
base_candidates.retain(|&base| ty_bases.contains(&base));

- N = number of candidates (50, 100, 200)
- D = depth of inheritance hierarchy
- Each retain call performs contains() which is O(D)
- Nested inside the loop makes it O(N·D²)

---

3. Constraint Conflicts (2.77x → 1.57x degradation)

Root Causes:

A. Quadratic Conflict Detection (src/solver/infer.rs):

for (i, &u1) in self.upper_bounds.iter().enumerate() {
for &u2 in &self.upper_bounds[i + 1..] { // O(N²)
if are_disjoint(interner, u1, u2) { ... }
}
}
for &lower in &self.lower_bounds {
for &upper in &self.upper_bounds { // O(M\*N)
if !is_subtype_of(interner, lower, upper) { ... }
}
}

B. Constraint Propagation Growth:

- strengthen_constraints iterates type_params.len() times
- unify_values appends without deduplication
- Constraint lists grow significantly during propagation

---

4. Mapped Type Templates (2.67x → 1.63x degradation)

Root Causes:

A. Allocation Overhead (src/checker/state_type_analysis.rs):

- Creates a new TypeSubstitution map for every key in the iteration loop
- For 200+ union keys, this allocation becomes significant

B. Missing Key Remapping:

- Currently ignores the name_type field (the as clause)
- Key remapping (e.g., as Exclude<K, "id">) is not applied

---

5. Why Parallelization Benefits Diminish

A. Interner Contention (src/solver/intern.rs):

- TypeInterner uses DashMap with 64 shards
- Complex types hash to the same shard (statistically)
- Threads lock on hot shards instead of computing

B. Memory Bandwidth Saturation:

- High generic counts create massive TypeKey data
- CPU cache thrashes from random access into DashMap
- Bottleneck moves from CPU cycles to RAM latency

C. Global Cache Locking (src/solver/db.rs):

- QueryCache uses RwLock<FxHashMap>
- Inference is write-heavy on the interner
- Threads block on write lock when caching new results

---

Recommended Solutions

Short-term (Quick Wins):

1. BCT Optimization: Implement heuristic to bail out of O(N²) checks for large arrays
2. Constraint Deduplication: Add deduplication to unify_values
3. Reuse Substitutions: Allocate single TypeSubstitution for mapped types

Medium-term (Architecture):

1. Thread-Local Interning: Use local interner for temporary inference types, merge to global only when resolved
2. Fix Mapped Type Remapping: Implement proper as clause handling

Long-term (North Star):

1. Full Salsa Integration: Migrate to salsa_db.rs for fine-grained dependency tracking
2. Profile-Guided Optimization: Use cargo flamegraph to identify exact hotspots
