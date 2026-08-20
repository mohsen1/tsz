# Rewrite Work Session

This is a compact routine for a focused TSZ rewrite session. It replaces the
retired persona rotation, shared scheduling board, and time-boxed compatibility
workflow.

## 1. Orient

1. Read `docs/plan/ROADMAP.md` and `docs/architecture/RESET.md`.
2. Run the worktree intake and inspect local changes before editing.
3. Fetch `origin/main`; inspect open PRs, recent merges, and relevant issues.
4. Drain work you already own before starting unrelated work.
5. Use the disk guard before a new worktree or heavy build.

Do not mutate or discard another contributor's work. Git history is the archive
for the deleted compiler; it is not an implementation source to restore.

## 2. Choose A Bounded Structural Slice

Map the slice to one roadmap milestone and one PR goal:

- `green`: make a supported result match TypeScript 7;
- `hold`: protect an exact capability already declared supported;
- `grow`: add a real dependency-complete project after its prerequisites are
  green;
- `fast`: improve a green result without changing it.

Prefer one grammar or semantic family whose preconditions and outputs can be
reviewed together. Do not select work merely because a retained test references
an old internal API.

## 3. Establish Oracle Truth

Before coding:

1. Minimize the witness.
2. Run the pinned TypeScript 7.0.2 oracle.
3. Record options, root order, diagnostics, exit status, and emit.
4. Locate the TypeScript operation and preserve its ordering constraints.
5. State:

```text
When <structural condition>, TypeScript 7 does X; TSZ does X through <module/API>.
```

Plan adjacent cases: renamed binders, wrappers/nesting, generic and concrete
forms, positive behavior, and fallback behavior when relevant.

## 4. Implement Through The Replacement Boundary

- Put syntax, program, checker semantics, emit, or service behavior in its one
  owning `tsz-core` module.
- Keep `tsz-cli` a process/protocol adapter and `tsz-conformance` an
  external-process harness.
- Preserve symbolic deferred forms and explicit completion states.
- Keep type handles session-local and diagnostics downstream of structured
  semantic facts.
- Start uncached; add a cache only with a typed key, dependency/reset rule,
  residency bound, and uncached agreement test.
- Keep the deterministic single-checker path authoritative.

Do not restore deleted crates, introduce behavior flags for alternate
semantics, add browser/WASM bindings before R4, or hardcode fixture/user text.

## 5. Verify The Claim

Run focused tests while iterating, then the strict rewrite gate appropriate to
the change:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
python3 scripts/arch/arch_guard.py
```

Launch narrow conformance, emit, or fourslash filters when the change touches
their process contract. Treat broad legacy suites as observations until their
families are declared supported. Never hide unsupported or crashed cases.

For deterministic or cached behavior, compare repeated, reversed-root-order,
cold/warm, and applicable thread-count outputs. Wrap long commands with
`scripts/safe-run.sh`; never run concurrent conformance commands in one
worktree.

## 6. Ship Truthfully

The reset PR opens only after the roadmap's complete R0 conviction gate passes
on its exact head. Every later PR still needs a bounded claim and exact oracle
evidence.

Include:

- `Goal: <green|fast|grow|hold>`;
- the structural rule and owning module/API;
- supported adjacent cases and known unsupported surface;
- `## Verification` with exact commands and results;
- `## Provenance` with actual Machine, Assistant, Model, and Effort values.

Do not imply broad compatibility from a seed result. Push the branch, verify
the remote PR body and exact head, then continue useful work instead of polling
CI. Never merge draft, WIP, blocked, stale-head, or someone else's work.

## 7. Close The Session

1. Run `git diff --check` and inspect the full branch diff.
2. Commit and push every intended change; leave unrelated files untouched.
3. Record the exact next action and any failed or unsupported observation.
4. If instructions or startup hooks changed, run
   `scripts/agents/llm-context-audit.py`.
5. Queue only an exact reviewed head whose required checks have passed.
