# RelationRequest Ownership

`RelationRequest` is the checker-side policy descriptor for assignability
queries that need structured failure information. It lives in
`crates/tsz-checker/src/query_boundaries/relation_request.rs`, is re-exported
through `query_boundaries::assignability`, and is executed by `execute_relation`.

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
| `source` | `assign`, `for_in_lhs`, `call_arg`, `return_stmt`, `jsx_props`, `jsx_children`, `satisfies`, `destructuring`, `rest_parameter`, `import_attributes`, `computed_enum_member`, `type_parameter_default`, `index_signature`, `decorator_callee`, `jsdoc_type_constraint`, `property_index_key`, `nullish_error_target`, `duplicate_identifier`, `variable_initializer`, `object_literal_computed_key`, `contextual_symbol_index_value`, `in_operator_key`, `in_operator_primitive_constraint`, `compound_assignment`, `generic_element_write` | `execute_relation`, failure analysis, weak-union analysis, property classification, checker-only post-check | Semantic solver input, diagnostic input, and classification input |
| `target` | Same constructors as `source` | Same consumers as `source` | Semantic solver input, diagnostic input, and classification input |
| `kind` | Same constructors as `source` | `execute_relation` debug span | Diagnostic/tracing context only; no solver or cache policy change today |
| `excess_property_mode` | Defaults to `Skip`; `with_fresh_source`, `with_spread_source`, `with_excess_property_mode` | No direct `execute_relation` branch today | Advisory request descriptor; caller-side EPC logic still emits or suppresses diagnostics |
| `missing_property_mode` | Defaults to `Report`; `with_missing_property_mode` | No direct `execute_relation` branch today | Advisory request descriptor; failure rendering and caller-side paths still own presentation |
| `source_is_fresh` | Defaults to `false`; `with_fresh_source` | No direct `execute_relation` branch today | Advisory request descriptor; fresh object literal EPC is still handled before or around the relation call |
| `allow_erased_generic_signature_retry` | Defaults to `false`; `with_erased_generic_signature_retry` | `execute_relation` | Semantic relation flag; translated to `RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY` |

## Current Call Sites

`assignability_diagnostics.rs` builds `RelationRequest::assign` for TS2322
diagnostics and `RelationRequest::call_arg` for TS2345 call-argument
diagnostics. Call diagnostic anchor helpers also use the call-argument request
when probing argument, object-literal property, or array-element mismatch
anchors. Call context helpers use the call-argument request when probing
explicit callback parameter conflicts that decide whether to surface an outer
argument mismatch, and render-failure helpers use it when probing nested strict
callback parameter mismatches. Dynamic `import(...)` validation uses the
call-argument request for specifier and options argument probes while retaining
checker-owned anchors and diagnostic codes. Call diagnostic elaboration helpers
use the call-argument request for parameter-derived object/array member probes,
including polymorphic-`this` object-literal property probes, and the return
request when drilling into callback return expressions or return-source
conditional branches. These callers reuse `RelationOutcome` to avoid separately
recomputing weak-union and property-classification analysis.
Call diagnostic display/recovery helpers use the env-aware call-argument
request for contextual-signature, generator callback, variadic tuple parameter,
polymorphic-`this` rest-target, and aggregate/fresh/rest argument recovery
probes. Generator callback recovery also uses the call-argument request for
`TNext` because that component is the value accepted by `next(value)`.
Call-result recovery probes use the call-argument request when comparing actual
argument types against parameter unions or polymorphic-`this` parameter targets.
Round-2 generic call argument rechecks and inference-refinement adoption guards
also use the env-aware call-argument request when comparing refreshed,
checker-refined, or synthetic argument-derived types against instantiated
parameter/rest types.
Generic `new` expression recovery uses the call-argument request when a
contextually typed object or array literal argument is compared against the
constructor parameter that supplied that context.
Awaited thenable validation also uses the call-argument request when probing
whether the awaited receiver satisfies a `then` signature's `this` type.
Iterator `next(value)` compatibility diagnostics use the call-argument request
when probing whether the value sent by `for...of`, spread, destructuring, or
`yield*` is accepted by the iterator's `next` parameter type.
`instanceof` `[Symbol.hasInstance]` validation uses the return request for the
hook return type and the call-argument request for the left operand passed to
the hook's first parameter.

`assignability/assignment_checker/destructuring.rs` builds
`RelationRequest::destructuring` for rest/default/property destructuring
assignment diagnostics.

`state/variable_checking/for_loop.rs` builds `RelationRequest::for_in_lhs`
for TS2405 `for...in` initializer target checks, where the source key type
must be assignable to the valid LHS target type.

`checkers/parameter_checker.rs` builds `RelationRequest::rest_parameter` for
TS2370 rest-parameter array checks, where a declared, resolved, or initializer
type must be assignable to readonly `any[]`.

`declarations/import/declaration_attributes.rs` builds
`RelationRequest::import_attributes` for TS2322 import-attribute object-shape
checks, where the synthesized attribute object must be assignable to the global
`ImportAttributes` target while checker code owns the import-attribute anchor.

`state/state_checking_members/statement_helpers.rs` builds
`RelationRequest::computed_enum_member` for computed enum-member validation,
where checker-owned enum evaluation fallback and TS18033 anchoring need
number/string compatibility probes.

`state/type_analysis/type_param_defaults.rs` builds
`RelationRequest::type_parameter_default` for type-parameter default constraint
validation, where checker code owns the type-parameter default diagnostic and
uses relation outcomes for raw, evaluated, and syntax-instantiated forms.

Index-signature TS2411/TS2413 diagnostics build
`RelationRequest::index_signature` through `index_signature_relation_outcome`
for template-pattern index compatibility, number-to-string index compatibility,
property/member-to-index value checks, and union index-signature value probes.

Decorator callee validation builds `RelationRequest::decorator_callee` through
`decorator_callee_relation_outcome` when probing whether a non-callable
decorator type is structurally assignable to the global `Function` interface.

JSDoc generic constraint diagnostics build
`RelationRequest::jsdoc_type_constraint` through
`jsdoc_type_constraint_relation_outcome` for direct JSDoc type references and
import-type member references, where checker code owns comment anchoring and
TS2344-style constraint diagnostic text.

Excess-property diagnostics build `RelationRequest::property_index_key` through
`property_index_key_relation_outcome` when checking whether a source
property-name literal is accepted by a target string index-signature key type.
The checker keeps the property/source-span walk while the request names the
relation role.

Nullish nested-target diagnostics build `RelationRequest::nullish_error_target`
through `nullish_error_target_relation_outcome` when a `null` or `undefined`
source is compared against the nullable portion of a structured target that
contains nested error types. The checker owns the cascade decision while the
request names the relation role.

Duplicate declaration diagnostics build
`RelationRequest::duplicate_identifier` through
`duplicate_identifier_relation_outcome` when probing whether duplicate
property, method, accessor, or index declarations are mutually compatible. The
checker owns TS2300/TS2717 diagnostic selection while the request names the
relation role.

Variable initializer diagnostics build
`RelationRequest::variable_initializer` through
`variable_initializer_relation_outcome` when probing initializer assignment
failure before object-literal/property elaboration or generic TS2322 fallback.
The checker owns variable-declaration anchoring and elaboration order while the
request names the relation role.

Object-literal computed-key routing builds
`RelationRequest::object_literal_computed_key` through
`object_literal_computed_key_relation_outcome` when deciding whether a computed
property key flows into the inferred number, symbol, or string index-signature
bucket. The checker owns bucket selection while the request names the relation
role.

Contextual symbol-index diagnostics build
`RelationRequest::contextual_symbol_index_value` through
`contextual_symbol_index_value_relation_outcome` when checking a computed
property value against a contextual symbol index signature. The checker owns
the computed-name diagnostic anchor while the request names the relation role.

`in`-operator diagnostics build `RelationRequest::in_operator_key` through
`in_operator_key_relation_outcome` when checking whether the left operand is
assignable to the property-key space. The checker owns nullish splitting,
TS2322 anchoring, and diagnostic display while the request names the relation
role.

`in`-operator RHS primitive-shape diagnostics build
`RelationRequest::in_operator_primitive_constraint` through
`in_operator_primitive_constraint_relation_outcome` when probing whether a type
parameter constraint could still admit primitive values for TS2638. The checker
owns the recursive RHS-shape walk while the request names the relation role.

Assignment-operation diagnostics build `RelationRequest::compound_assignment`
through `compound_assignment_relation_outcome` when probing whether a
compound-like RHS is compatible with the widened LHS type before suppressing a
generic TS2322 fallback. Deferred generic element writes build
`RelationRequest::generic_element_write` through
`generic_element_write_relation_outcome` before choosing the generic
write-target error. The checker owns assignment syntax and diagnostic anchoring
while the requests name the relation roles.

`assignability_diagnostics.rs` builds `RelationRequest::satisfies` for
`expr satisfies T` diagnostics.

Return diagnostics in `types/type_checking`, JSX component return checks,
decorator checks, contextual return utilities, function-type helpers, and call
checker callback-return recovery build `RelationRequest::return_stmt` through
`return_relation_outcome` or `return_relation_outcome_with_env`. Contextual
generic call retry and callback-return retyping use the env-aware return request
when comparing inferred or callback returns to contextual return targets.
Method/accessor and property decorator return validation uses the return request
when comparing decorator function returns to the decorator ABI return target.
Async JSDoc return suppression uses the return request when comparing a
promise-unwrapped initializer return to the declared function return type.
Weak-type TS2560 suggestions also use the return request when probing whether a
call or construct result would satisfy the weak target.

JSX props and attribute validation paths build `RelationRequest::jsx_props`
through `jsx_props_relation_outcome` in the JSX props resolution, validation,
spread, overload, generic-spread, union-props, and intrinsic tag-resolution
checkers, generic managed-attributes final assignability, plus React props
display-alias storage when checking whether the candidate alias and props
surface are mutually compatible.

JSX children and text-child validation paths build
`RelationRequest::jsx_children` through `jsx_children_relation_outcome` in the
JSX children and diagnostics checkers.

`query_boundaries/class.rs` builds `RelationRequest::assign` with
`with_erased_generic_signature_retry` for class/interface member compatibility
where erased generic signature retry is allowed.

No current production call site uses `with_excess_property_mode` or
`with_missing_property_mode`. They are retained as explicit policy shapes and
are covered by architecture tests so follow-up work can centralize one policy
decision at a time.

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
