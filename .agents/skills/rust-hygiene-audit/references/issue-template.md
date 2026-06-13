# Tech-debt issue body template

Match the repo's existing `techdebt(scope):` issues. The workflow authors
children in this shape; epics summarize the theme and link children.

## Title

`techdebt(<crate-or-repo>): <imperative summary>` — e.g.
`techdebt(solver): split over-cap god-files def/core.rs along concern axes`.

## Child body

```markdown
## Summary
<one paragraph + the structural rule:
"When <structural condition>, idiomatic Rust does X; tsz does Y at <owner>.">

## Evidence
- `path/to/file.rs:LINE` — short quote of the duplicated/bloated code
- `path/to/other.rs:LINE` — the second site (DRY claims need >=2)

## Why it matters
<concrete maintenance/correctness/perf cost; cite counts; name any parity bug
the duplication has already produced — that is the highest-value evidence>

## Proposed fix
<concrete idiomatic Rust: declarative macro / derive_more / newtype / module
split / lint promotion. Sized S / M / L; if L, sketch the multi-PR sequence.
Keep files under the repo line cap.>

## Risks / coordination
<behavior-preservation argument, ordering subtleties, overlap with other open
issues or active campaigns, and the exact verification commands / CI gates>
```

## Epic body

```markdown
## Summary
<the theme + the structural rule it enforces across the codebase>

## Measured evidence (this audit)
<hard numbers: clippy warning counts + triage for the lint epic; derive-site
counts; over-cap file table; etc.>

## Why it matters
<the compounding cost of leaving the whole family unaddressed>

## Proposed approach
<staged plan; one invariant per child PR>

## Risks / coordination
<cross-epic ordering, scope boundaries vs active campaigns>

## Child issue map
<appended automatically by create_issues.py>
```

## Senior framing that makes issues land

- Lead with the **structural rule**, not the symptom. "The reported repro is one
  witness, not the scope."
- For derive bloat, name the idiomatic Rust fix precisely: a declarative
  `define_id!` macro for repeated `u32` newtypes, `derive_more` for
  `From`/`Deref`/`Display`, `#[derive(Default)]` for derivable manual impls.
- Flag duplication that already drifted into divergent behavior as a **parity
  bug inside the dedup**, and propose fixing it as a separate reviewed commit on
  top of the mechanical consolidation.
