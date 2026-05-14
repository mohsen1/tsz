# RelationRequest Ownership

`RelationRequest` is the checker-side policy descriptor for assignability
queries that need structured failure information. It lives in
`crates/tsz-checker/src/query_boundaries/assignability.rs` and is executed by
`execute_relation`.

This document records current behavior. It does not claim every request field is
fully centralized yet; several fields are intentionally tracked here as follow-up
work under the assignability tech-debt parent.

## Execution Path

The active boundary path is:

1. Callers prepare source and target types with
   `AssignabilityChecker::prepare_assignability_inputs`.
2. Callers construct a `RelationRequest`.
3. `AssignabilityChecker::execute_relation_request` passes checker relation
   flags, the checker context, the inheritance graph, and the override provider
   to `query_boundaries::assignability::execute_relation`.
4. `execute_relation` calls `is_assignable_with_overrides`, records
   `depth_exceeded`, and returns a `RelationOutcome`.
5. On failure, `execute_relation` also collects structured failure analysis,
   weak-union classification, and canonical object property classification.

`execute_relation_request` can still downgrade a solver-related result through
checker-only assignability failure analysis. That post-check is intentionally
outside the solver boundary today because it depends on checker-only state.

Legacy diagnostic callers that still collect raw
`AssignabilityFailureAnalysis` through `check_assignable_gate_with_overrides`
route `ExcessProperty` suppression through
`suppress_raw_excess_property_failure_if_needed`. The caller supplies
checker-specific member normalization, but the decision about which target
shapes suppress EPC now lives in the assignability boundary.

## Field Map

| Field | Constructors / builders | Current consumers | Effect today |
| --- | --- | --- | --- |
| `source` | `assign`, `call_arg`, `return_stmt`, `satisfies`, `destructuring` | `execute_relation`, failure analysis, weak-union analysis, property classification, checker-only post-check | Semantic solver input, diagnostic input, and classification input |
| `target` | Same constructors as `source` | Same consumers as `source` | Semantic solver input, diagnostic input, and classification input |
| `kind` | Same constructors as `source` | `execute_relation` debug span | Diagnostic/tracing context only; no solver or cache policy change today |
| `excess_property_mode` | Defaults to `Skip`; `with_fresh_source`, `with_spread_source`, `with_excess_property_mode` | No direct `execute_relation` branch today | Advisory request descriptor; caller-side EPC logic still emits or suppresses diagnostics |
| `missing_property_mode` | Defaults to `Report`; `with_missing_property_mode` | No direct `execute_relation` branch today | Advisory request descriptor; failure rendering and caller-side paths still own presentation |
| `source_is_fresh` | Defaults to `false`; `with_fresh_source` | No direct `execute_relation` branch today | Advisory request descriptor; fresh object literal EPC is still handled before or around the relation call |
| `allow_erased_generic_signature_retry` | Defaults to `false`; `with_erased_generic_signature_retry` | `execute_relation` | Semantic relation flag; translated to `RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY` |

## Current Call Sites

`assignability_diagnostics.rs` builds `RelationRequest::assign` for TS2322
diagnostics and `RelationRequest::call_arg` for TS2345 call-argument
diagnostics. These callers reuse `RelationOutcome` to avoid separately
recomputing weak-union and property-classification analysis.

`assignment_checker/destructuring.rs` builds `RelationRequest::assign` for rest
destructuring assignment diagnostics.

`query_boundaries/class.rs` builds `RelationRequest::assign` with
`with_erased_generic_signature_retry` for class/interface member compatibility
where erased generic signature retry is allowed.

No current production call site uses `return_stmt`, `satisfies`, `destructuring`,
`with_excess_property_mode`, or `with_missing_property_mode`. They are retained
as explicit policy shapes and are covered by architecture tests so follow-up
work can centralize one policy decision at a time.

## Boundary Responsibilities

`execute_relation` currently owns:

- applying checker relation flags plus erased-generic retry;
- invoking the solver relation through `is_assignable_with_overrides`;
- carrying relation depth overflow back through `RelationOutcome`;
- collecting structured solver failure reasons;
- detecting weak-union violations;
- computing canonical object property classification for failed relations;
- suppressing excess-property failure reasons when the target shape makes EPC
  inapplicable.

The boundary also exposes `suppress_raw_excess_property_failure_if_needed` for
the remaining raw-analysis path, so callers do not duplicate the target-shape
EPC suppression policy while that path is being migrated.

It does not yet own:

- deciding whether a fresh object literal should run full EPC;
- deciding whether a spread source should run explicit-only EPC;
- suppressing missing-property presentation from `missing_property_mode`;
- changing relation cache keys based on `RelationKind`.

Those are the remaining policy-centralization surfaces for later slices.
