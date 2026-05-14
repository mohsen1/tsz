# Claim: Reject name-only simple-object missing-interface lib resolution

Date: 2026-05-14
Branch: `codex/perf-simple-object-missing-interface-lib-20260514`
PR: #6917
Status: rejected behavior probe

## Claim

The simple local-interface object shortcut still records seven
`reject_missing_interface_decl` rows on regenerated monorepo-006 after the
declaration/provenance residue naming slice. A behavior probe tried to admit
those rows through existing lib metadata before the shortcut records a
missing-interface reject. CI showed that this is not safe: both the broad
iterator-inclusive allowlist and the narrowed non-iterator allowlist regressed
conformance and DTS emit.

## Scope

- Do not merge the behavior path that admits missing-interface rows by name.
- Keep `reject_missing_interface_decl` and `reject_out_of_arena_decl` on the
  current fallback path.
- Keep the regenerated monorepo-006 counter artifacts as evidence that local
  attribution alone was insufficient to prove safety.
- Use this result to constrain the next design: lib shape reuse needs proven
  symbol provenance/identity and type-parameter instantiation, not a string
  allowlist.

## Validation

- Broad CI run `25849166412`: failed emit and conformance aggregate.
- Narrowed CI run `25850104213`: failed emit and conformance aggregate.
- Narrowed failure details: DTS emit `1477 < 1527`, conformance aggregate
  `12575/12585 < 12581/12585`.
- Newly failing conformance cases included deep mapped/indexing cases,
  generic-return inference cases, modularized lib cases, and
  `strictOptionalProperties1`.
