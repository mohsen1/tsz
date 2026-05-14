# Claim: issue 6840 abstract mixin applied to concrete base

Status: claim
Owner: codex
Issue: #6840
Branch: codex/issue-6840-mixin-concrete-base-20260514

## Scope

Fix the TS2511 false positive where an abstract mixin class returned from a function remains marked abstract after the mixin is applied to a concrete base constructor.

## Plan

- Add a focused TS2511 regression for the issue repro.
- Patch checker class/mixin return refinement so the instantiated result is concrete when the supplied base constructor is concrete and no abstract members remain.
- Run the smallest targeted checker test covering the regression.

## Status

Claimed 2026-05-14.
