Task: Workstream 5 large-repo fixture - fix non-total sort comparator panic
Status: ready
Branch: `fix/large-repo-sort-total-order`
PR: #2141

Plan:
- Reproduce the `large-ts-repo` runtime panic with a backtrace.
- Identify the non-total comparator and add deterministic tie-breakers without changing diagnostic semantics.
- Verify with focused tests plus a guarded large-repo retry to confirm the panic is gone or moved.
