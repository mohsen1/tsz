## Architecture checklist (required)

- [ ] This change is classified as `WHAT` (type algorithm) or `WHERE` (diagnostic/orchestration), and I stated which one in the PR description.
- [ ] If `WHAT`, implementation is in solver/query helpers (not checker-local type semantics).
- [ ] If `WHERE`, checker uses existing query boundaries/solver APIs (no duplicate semantic logic).
- [ ] I listed which solver/query boundary functions are used or added.
- [ ] I listed parity impact (tests/fixtures changed, expected behavior impact, or "no parity changes").

## TS2322 checklist (if applicable)

- [ ] Assignability behavior changes go through relation/query boundary helpers.
- [ ] Diagnostic text/path comes from solver failure reasons, not checker-local heuristics.
