# Explainer: TSZ Fast Composite-Mode Compilation

This document translates the [Project References Performance Investigation RFC](./project-references-performance-investigation.md) into practical, real-world examples. It explains **how** TSZ evaluates project dependencies, invalidates caches, and schedules parallel builds to achieve massive speedups without running out of memory.

## Core Concepts

Before diving into scenarios, keep these three mechanisms in mind:

1. **The Multi-Signal Status Oracle**: TSZ doesn't just look at file modification times (`mtime`) to decide if a file changed. It uses a tiered system (`mtime` -> content hash -> config hash -> upstream API hash) to prevent unnecessary rebuilds.
2. **The API Fingerprint**: A deterministic hash of a project's *public* type surface, calculated exclusively by the **Solver**. It ignores internal implementation details.
3. **Memory-Aware Scheduling**: TSZ builds projects in parallel, but it throttles execution based on system RAM (RSS) to prevent `Arena` allocation out-of-memory (OOM) crashes.

---

## Scenario 1: The Internal Implementation Change

**The Setup:**
You have a monorepo with `Project A` (utilities) and `Project B` (an app that consumes `Project A`).

**The Change:**
You modify a private implementation detail in `Project A`:

```typescript
// packages/project-a/src/math.ts
export function calculateTotal(price: number, tax: number): number {
  // You added a console.log for debugging
  console.log("Calculating total for", price); 
  return price + tax;
}
```

**How TSZ Handles It:**
1. **Status Oracle**: TSZ detects `math.ts` has a new `mtime` and a new content hash. `Project A` is marked as dirty.
2. **Rebuild A**: `Project A` is rebuilt. The AST (`NodeArena`) is updated.
3. **Solver Fingerprinting**: The Checker asks the Solver to generate a new API Fingerprint for `Project A`. The Solver looks at the exported signature: `(price: number, tax: number) => number`. This has *not* changed. The new API Fingerprint matches the cached API Fingerprint.
4. **Project B is Skipped**: When TSZ evaluates `Project B`, it checks its upstream dependencies. `Project A`'s API Fingerprint is identical. `Project B`'s files haven't changed. **`Project B` is skipped entirely.**

*Result: Massive time savings. Changing a function body doesn't trigger a downstream cascade.*

---

## Scenario 2: The Inferred Return Type Leak

**The Setup:**
Same repo (`Project A` and `Project B`). 

**The Change:**
You modify a *private* interface in `Project A` that happens to be returned by a public function.

```typescript
// packages/project-a/src/user.ts
interface PrivateUser {
  name: string;
  // You added a new property to an unexported interface
  age?: number; 
}

// This function is exported and infers PrivateUser as its return type
export function createUser(name: string) {
  return { name, age: 30 } as PrivateUser; 
}
```

**How TSZ Handles It:**
1. **Status Oracle**: `Project A` is marked dirty.
2. **Rebuild A**: `Project A` rebuilds.
3. **Solver Fingerprinting**: The Solver evaluates `createUser`. Because TSZ is strictly **Solver-first**, it fully traverses the `Lazy(DefId)` of the inferred return type. It discovers the `PrivateUser` shape has changed (it now includes `age?: number`).
4. **Fingerprint Mismatch**: The Solver generates a new API Fingerprint for `Project A`.
5. **Project B is Rebuilt**: TSZ evaluates `Project B`. It sees `Project A`'s API Fingerprint has changed. `Project B` is marked dirty and rebuilt to ensure it accommodates the new `age` property.

*Result: 100% correct type safety. TSZ correctly detects semantic changes even if they aren't explicitly exported.*

---

## Scenario 3: The Git Checkout / CI Trap

**The Setup:**
You just ran `git checkout main` or your CI pipeline just cloned the repository. `npm ci` restores your `node_modules` and cache.

**The Problem:**
Git updates the `mtime` (modification time) of *every single file* to the current clock time.

**How TSZ Handles It:**
1. **Tier 1 (mtime)**: TSZ looks at `Project A`. The file `mtime` does not match the `.tsbuildinfo` cache. In naive compilers, this would trigger a full rebuild.
2. **Tier 2 (Content Hash Recovery)**: TSZ pauses. Instead of invalidating the project, it reads the file and hashes the contents.
3. **Cache Rescue**: The content hash *exactly matches* the content hash stored in the `.tsbuildinfo`.
4. **Skip & Update**: TSZ skips the rebuild of `Project A`. It silently updates the `.tsbuildinfo` with the new `mtime` so future checks hit the Tier 1 fast-path again.

*Result: Instantaneous no-op builds in CI and after branch switches.*

---

## Scenario 4: The Wide-Graph Memory Squeeze

**The Setup:**
You have a massive monorepo. `Core` is a base package. `App1` through `App50` all depend on `Core`. 
You are running on a 16-core CI machine with 16GB of RAM.

**The Change:**
You change the public API of `Core`.

**How TSZ Handles It:**
1. **Core Rebuilds**: `Core` builds successfully. Its API Fingerprint changes.
2. **The Fan-Out**: The Scheduler sees that `App1` through `App50` are all "ready" to build.
3. **CPU Dispatching**: The Scheduler dispatches 16 projects (`App1` to `App16`) to the 16 worker threads.
4. **The Memory Trap**: TSZ uses `Arena` allocators (`NodeArena`, `TypeInterner`). These are incredibly fast but consume linear memory. Because 16 projects are type-checking simultaneously, system memory (RSS) spikes to 14GB (87% of capacity).
5. **Memory-Aware Throttling**: A worker finishes `App1`. CPU token is freed. However, the Scheduler checks system RSS. Because RSS is >80%, **it refuses to dispatch `App17`**.
6. **Garbage Collection**: The `App1` thread drops its `Project` state, freeing its Arenas. Memory drops to 12GB (75%).
7. **Resumption**: The Scheduler sees memory is safe again and dispatches `App17`.

*Result: The build finishes as fast as physics allow without ever hitting a catastrophic Out-Of-Memory (OOM) kill.*

---

## Scenario 5: Long-Lived LSP (IDE) Sessions

**The Setup:**
A developer has TSZ running as their language server in VS Code for 4 hours. They are constantly typing, deleting, and changing files across 5 different projects.

**The Problem:**
LSP uses a `global_interner` to share types across files. Because TSZ's Interner is an append-only Arena (for O(1) equality checks), every keystroke adds new `TypeId`s. Over 4 hours, this would normally cause a massive memory leak.

**How TSZ Handles It (The Epoch Swap):**
1. As the developer types, the LSP applies incremental updates.
2. The LSP tracks the size of the `global_interner`.
3. When the interner exceeds a threshold (e.g., 2GB), the LSP triggers an **Epoch Swap** in the background.
4. It creates a brand new `Project` container with an empty `TypeInterner`.
5. It re-evaluates the active workspace state into this new Interner.
6. Once ready, it swaps the pointers and drops the old `Project`. All orphaned `TypeId`s from the last 4 hours of typing are freed instantly.

*Result: Blazing fast IDE responsiveness with flat memory usage, even after hours of editing.*
