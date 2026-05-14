# Claim: Resolve simple-object missing-interface rows through lib metadata

Date: 2026-05-14
Branch: `codex/perf-simple-object-missing-interface-lib-20260514`
Status: claimed

## Claim

The simple local-interface object shortcut still records seven
`reject_missing_interface_decl` rows on regenerated monorepo-006 after the
declaration/provenance residue naming slice. This work proves whether those
exact rows can reuse existing lib metadata before the shortcut records a
missing-interface reject.

## Scope

- Admit only the named missing-interface residue family:
  `Iterable`, `IteratorReturnResult`, `IteratorYieldResult`,
  `PropertyDescriptor`, `PropertyDescriptorMap`, `RegExpIndicesArray`, and
  `RegExpStringIterator`.
- Reuse `resolve_lib_type_by_name`; do not lower declaration arenas manually.
- Leave `reject_out_of_arena_decl` rows and all non-allowlisted symbols on the
  current fallback path.
- Record regenerated monorepo-006 attribution counters before making a timing
  claim.

## Validation

- Pending.
