# Claim: fix F-bounded recursive alias corruption (#6557)

Status: claim
Owner: Codex
Branch: codex/fbounded-recursive-alias-6557-20260513
PR: TBD
Issue: #6557

## Scope
- Investigate and fix the TS2420 false positive where recursive interface + recursive alias context corrupts F-bounded `implements Comparable<MyNumber>` checking.
- Add focused regression coverage for the minimal repro.

## Validation plan
- Targeted checker regression test for #6557.
- Focused conformance/checker validation as warranted by touched solver/checker surface.
