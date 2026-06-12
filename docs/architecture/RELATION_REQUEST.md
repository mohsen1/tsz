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

### Failure-analysis memo (single pass per key)

Reason-collecting executions are shared through the stamp-guarded
`AssignabilityFailureMemo` (issue #13243). `execute_relation_request` and
`analyze_assignability_failure` both key the raw solver analysis on
`(prepared source, prepared target, solver flags, sound mode)` under the same
session stamp as the `evaluate_type_for_assignability` memo, so a failing
TS2322/TS2345 pair walks the relation engine once: the gateway's captured
analysis is replayed when the error reporter re-analyzes the same pair.
Decision-only requests, the overload subtype pass, non-`Assignable` solver
kinds, and env-resolver executions (`relation_outcome_with_env`) never consult
or populate the memo. Entries are dropped whenever the stamp moves and are
never written for depth/iteration/fuel-degraded passes. Counters:
`relation_failure_reason_walks` / `relation_failure_memo_hits`.

## Field Map

| Field | Constructors / builders | Current consumers | Effect today |
| --- | --- | --- | --- |
| `source` | `assign`, `assignability_reason`, `for_in_lhs`, `call_arg`, `return_stmt`, `jsx_props`, `jsx_children`, `jsx_element_type`, `satisfies`, `destructuring`, `rest_parameter`, `import_attributes`, `computed_enum_member`, `numeric_enum_assignment`, `type_parameter_default`, `index_signature`, `decorator_callee`, `jsdoc_type_constraint`, `explicit_alias_constraint`, `array_like_constraint_element`, `merged_interface_constraint`, `recursive_heritage_property`, `union_constraint_member`, `syntax_instantiated_constraint`, `type_arg_constraint`, `mapped_key_constraint`, `indexed_access_constraint_key`, `indexed_access_key_space`, `conditional_constraint_component`, `conditional_true_base_constraint`, `conditional_true_branch_constraint`, `required_mapped_constraint`, `infer_result_constraint`, `generic_constraint_property`, `property_index_key`, `nullish_error_target`, `duplicate_identifier`, `variable_initializer`, `identifier_binding_default`, `keyof_diagnostic_suppression`, `diagnostic_source_narrowing`, `diagnostic_overlap`, `polymorphic_this_receiver`, `class_extends_index_value`, `class_implements_index_value`, `class_implements_whole_type`, `class_static_side`, `interface_heritage_index_value`, `interface_heritage_generic_method`, `interface_heritage_property_index`, `jsdoc_heritage_constraint`, `missing_property_read`, `missing_property_write`, `concrete_remapped_mapped_missing_property`, `exact_optional_source_filter`, `union_excess_required_property`, `array_literal_contextual_collapse`, `jsx_render_fallback`, `object_literal_mapped_contextual_key`, `object_literal_computed_key`, `object_literal_jsdoc_declared_property`, `contextual_symbol_index_value`, `in_operator_key`, `in_operator_primitive_constraint`, `compound_assignment`, `generic_element_write`, `property_receiver_element_display`, `property_receiver_index_value_display`, `element_access_number_index`, `element_access_method_suggestion`, `call_elaboration_mutual`, `call_display_overlap`, `call_generator_yield`, `round2_contextual_substitution`, `constructor_inference_constraint`, `call_adapter_compatibility`, `call_adapter_identity`, `overload_implementation_parameter`, `binary_arithmetic_number`, `private_member_access`, `function_type_compatibility` | `execute_relation`, failure analysis, weak-union analysis, property classification, checker-only post-check | Semantic solver input, diagnostic input, and classification input |
| `target` | Same constructors as `source` | Same consumers as `source` | Semantic solver input, diagnostic input, and classification input |
| `kind` | Same constructors as `source` | `execute_relation` debug span | Diagnostic/tracing context only; no solver or cache policy change today |
| `excess_property_mode` | Defaults to `Skip`; `with_fresh_source`, `with_spread_source`, `with_excess_property_mode` | No direct `execute_relation` branch today | Advisory request descriptor; caller-side EPC logic still emits or suppresses diagnostics |
| `missing_property_mode` | Defaults to `Report`; `with_missing_property_mode` | No direct `execute_relation` branch today | Advisory request descriptor; failure rendering and caller-side paths still own presentation |
| `source_is_fresh` | Defaults to `false`; `with_fresh_source` | No direct `execute_relation` branch today | Advisory request descriptor; fresh object literal EPC is still handled before or around the relation call |
| `allow_erased_generic_signature_retry` | Defaults to `false`; `with_erased_generic_signature_retry` | `execute_relation` | Semantic relation flag; translated to `RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY` |

Additional active request builders are tracked as they land. The checker-only
`iterator_result_value` request names the `IteratorResult` value-property probe
used by TS2322 failure analysis. The checker type-overlap helpers use
`type_comparability` for bidirectional comparability probes.

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

Checker type-overlap diagnostics build `RelationRequest::type_comparability`
through `type_comparability_relation_outcome` for bidirectional comparability
fast paths, union/intersection member overlap probes, and common-property
comparability. Type-parameter comparability uses the same request when probing
constraint-chain overlap. The checker owns apparent-type selection and
diagnostic orchestration while the request names the relation role.

Call diagnostic display/recovery helpers use the env-aware call-argument
request for contextual-signature, generator callback, variadic tuple parameter,
polymorphic-`this` rest-target, and aggregate/fresh/rest argument recovery
probes. Generator callback recovery also uses the call-argument request for
`TNext` because that component is the value accepted by `next(value)`.
Callable-source to union-arm compatibility builds
`RelationRequest::callable_union_return` and
`RelationRequest::callable_union_parameter` through their matching
`RelationOutcome` helpers when checking whether a callable source satisfies at
least one callable union member. The checker owns signature extraction and
union-arm selection while the requests name the return and contravariant
parameter relation roles.
Type-predicate validation builds `RelationRequest::type_predicate_parameter`
through `type_predicate_parameter_relation_outcome` when checking whether a
predicate's narrowed type is compatible with its parameter type. The
type-predicate boundary owns predicate-shape recursion while the request names
the checker relation role.
Generic argument suppression builds
`RelationRequest::generic_argument_suppression` through
`generic_argument_suppression_relation_outcome_with_env` when checking whether a
self-referential mapped or contextual generic argument should suppress an outer
mismatch. The checker owns suppression-shape recognition while the request names
the env-aware relation role.
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

Numeric enum assignment diagnostics build
`RelationRequest::numeric_enum_assignment` through
`numeric_enum_assignment_relation_outcome` when probing whether a numeric
literal initializer satisfies the structural target member before reporting a
TS2322-style enum assignment mismatch. The checker owns numeric-enum discovery,
initializer-literal recovery, and diagnostic anchoring while the request names
the relation role.

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
import-type member references, including type-reference argument checks and
their base/indexed-access fallback probes, where checker code owns comment
anchoring and TS2344-style constraint diagnostic text.

Explicit alias generic constraint diagnostics build
`RelationRequest::explicit_alias_constraint` through
`explicit_alias_constraint_relation_outcome` when probing whether an explicit
alias base satisfies an instantiated type-argument constraint before trying
union-member and array-like fallbacks. The checker owns the explicit-AST witness
and fallback order while the request names the relation role.

Array-like generic constraint diagnostics build
`RelationRequest::array_like_constraint_element` through
`array_like_constraint_element_relation_outcome` when probing whether the source
element type satisfies the target element type before recursive array-like
fallback. The checker owns array-surface classification and fallback order while
the request names the relation role.

Merged-interface generic constraint diagnostics build
`RelationRequest::merged_interface_constraint` through
`merged_interface_constraint_relation_outcome` when probing whether a sibling
interface declaration's type-parameter constraint satisfies the required
constraint before array-like fallback. The checker owns sibling declaration
discovery, callable shortcutting, and fallback order while the request names the
relation role.

Recursive heritage property conflict diagnostics build
`RelationRequest::recursive_heritage_property` through
`recursive_heritage_property_relation_outcome` when probing both directions of
inherited member compatibility after structural identity has failed. The checker
owns recursive heritage discovery, identity prefiltering, and conflict polarity
while the request names the relation role.

Base-union generic constraint diagnostics build
`RelationRequest::union_constraint_member` through
`union_constraint_member_relation_outcome` when probing whether each evaluated
union member satisfies the required constraint before array-like fallback. The
checker owns union member iteration, heritage shortcutting, and fallback order
while the request names the relation role.

Syntax-instantiated generic constraint diagnostics build
`RelationRequest::syntax_instantiated_constraint` through
`syntax_instantiated_constraint_relation_outcome` when probing whether a
checker-synthesized type argument satisfies an instantiated constraint before
base-union or array-like fallback. The checker owns syntax reconstruction,
callable shortcutting, and fallback order while the request names the relation
role.

Generic type-argument constraint diagnostics build
`RelationRequest::type_arg_constraint` through
`type_arg_constraint_relation_outcome` or
`type_arg_constraint_relation_outcome_with_env` when probing whether a
substituted, evaluated, base-constraint, inferred substitution, or primitive-key
witness satisfies an instantiated TS2344 constraint. Evaluated witness,
diagnostic-suppression, and non-all-optional fallback probes use
`type_arg_constraint_no_weak_relation_outcome` to preserve the
`isTypeAssignableTo`-style no-weak policy while still naming the generic
type-argument constraint role. The checker owns type-argument evaluation,
scoped-parameter substitution, source-constraint fallback selection, base
fallback selection, primitive key witness selection, weak-type fallback choice,
and diagnostic anchoring while the request names the relation role.

Mapped-key constraint diagnostics build
`RelationRequest::mapped_key_constraint` through
`mapped_key_constraint_relation_outcome` when probing whether a deferred or
pre-evaluation indexed-access constraint, evaluated current-object constraint,
constraint-chain member, or conditional key candidate is accepted by the mapped
object's key space before accepting the mapped type key, filtering current
object keys, or reporting the invalid key constraint. The checker owns
mapped-key validity, evaluation ordering, circular constraint checks,
current-object key filtering, and diagnostic anchoring while the request names
the relation role.

Indexed-access generic constraint diagnostics build
`RelationRequest::indexed_access_constraint_key` through
`indexed_access_constraint_key_relation_outcome` when probing whether an
effective index type is accepted by a concrete object's key space before
returning the union of property value types. The checker owns indexed-access
normalization, keyed-object recovery, and property collection while the request
names the relation role.

Indexed-access key-space diagnostics build
`RelationRequest::indexed_access_key_space` through
`indexed_access_key_space_relation_outcome` when probing whether an effective
index, constraint, or union member is accepted by a computed key space before
choosing a fallback result or suppressing TS2536. The checker owns
indexed-access normalization, access-computation object-shape recovery,
type-literal fallback ordering, string-index coercion checks, and mapped-value
recovery while the request names the relation role.

Conditional generic constraint diagnostics build
`RelationRequest::conditional_constraint_component` through
`conditional_constraint_component_relation_outcome` when probing whether a
conditional result branch, indexed-object-map branch value, or conditional
extends fallback satisfies the required constraint. The checker owns
conditional component discovery, alias expansion, infer/never guards, and
fallback ordering while the request names the relation role.

Conditional true-base generic constraint diagnostics build
`RelationRequest::conditional_true_base_constraint` through
`conditional_true_base_constraint_relation_outcome` when probing whether the
base constraint of a true-branch type parameter satisfies the required
constraint. The checker owns the exact true/check identity guard, bare
type-parameter check, and base extraction while the request names the relation
role.

Conditional true-branch generic constraint diagnostics build
`RelationRequest::conditional_true_branch_constraint` through
`conditional_true_branch_constraint_relation_outcome` when probing whether an
enclosing conditional's `extends` type, resolved/evaluated `extends` type, or
accumulated extends-type intersection satisfies the required constraint. The
checker owns conditional AST ancestry, true-branch containment, callable
shortcutting, and branch accumulation while the request names the relation role.

Required mapped generic constraint diagnostics build
`RelationRequest::required_mapped_constraint` through
`required_mapped_constraint_relation_outcome` when probing whether a type
argument or matching required property value satisfies a required mapped source.
The checker owns required-source discovery, substitution, property collection,
optional/name checks, and alias-property node lookup while the request names the
relation role.

Infer-result generic constraint diagnostics build
`RelationRequest::infer_result_constraint` through
`infer_result_constraint_relation_outcome` when probing whether a restricted
infer-result type satisfies an instantiated constraint. Inferred alias-body and
conditional array-element fallback witnesses use
`infer_result_constraint_no_weak_relation_outcome` to preserve the
`isTypeAssignableTo`-style no-weak policy while still naming the infer-result
constraint role. The checker owns check-constraint, application-argument,
referenced-constraint, inferred-base, evaluated-result, positional,
hidden-infer substitution, alias-body witness extraction, conditional
array-element witness extraction, and fallback ordering while the request names
the relation role.

Generic constraint diagnostics build
`RelationRequest::generic_constraint_property` through
`generic_constraint_property_relation_outcome` when probing whether every
property in a closed object-map indexed-access target can satisfy a constraint
before suppressing TS2344. The checker owns the closed-shape carve-out while
the request names the relation role.

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

Contextual binding-default identifier preservation builds
`RelationRequest::identifier_binding_default` through
`identifier_binding_default_relation_outcome` when deciding whether a flow type
from a contextual binding default should preserve the declared identifier type.
The checker owns identifier result selection while the request names the
relation role.

`keyof` diagnostic suppression builds
`RelationRequest::keyof_diagnostic_suppression` through
`keyof_diagnostic_suppression_relation_outcome` when probing whether a source
can satisfy an evaluated or interface-augmented `keyof` target before suppressing
a TS2322-style diagnostic. The checker owns the suppression shape and
augmentation walk while the request names the relation role.

Assignability reason entrypoints build `RelationRequest::assignability_reason`
through `assignability_reason_relation_outcome` when deciding whether a
TS2322-style failure still needs detailed elaboration. The checker owns
diagnostic anchoring and nested suppression order while the request names the
relation role.

Diagnostic-source display selection builds
`RelationRequest::diagnostic_source_narrowing` through
`diagnostic_source_narrowing_relation_outcome` when probing whether an
expression display type is strictly narrower than its declared type. The
checker owns source-span and display selection while the request names the
relation role.

Diagnostic overlap/comparability helpers build
`RelationRequest::diagnostic_overlap` through
`diagnostic_overlap_relation_outcome` when probing whether candidate types or
signature components are related enough for overlap diagnostics. The checker
owns signature arity pairing, rest-parameter element extraction, generic
signature permissiveness, and direction suppression while the request names the
relation role.

Polymorphic `this` receiver diagnostics build
`RelationRequest::polymorphic_this_receiver` through
`polymorphic_this_receiver_relation_outcome` when probing whether a receiver or
intersection member satisfies the target before choosing the concrete receiver
type to display. The checker owns receiver/member selection while the request
names the relation role.

Class/interface heritage diagnostics build
`RelationRequest::class_extends_index_value`,
`RelationRequest::class_implements_index_value`,
`RelationRequest::class_implements_whole_type`,
`RelationRequest::class_static_side`,
`RelationRequest::interface_heritage_index_value`,
`RelationRequest::interface_heritage_generic_method`,
`RelationRequest::interface_heritage_property_index`, or
`RelationRequest::jsdoc_heritage_constraint` through dedicated relation outcome
helpers when probing class-extends index-signature values, namespace-merged
static-side compatibility, inherited-base index conflicts, whole
class-implements surfaces including own-member mismatch fallback suppression,
generic method specialization and fresh generic trailing-overload retries,
type-alias property compatibility with inherited string indexes, or JSDoc
heritage object constraints. The checker owns heritage diagnostic anchoring,
retry eligibility, and suppression order while the requests name the relation
roles.

Assignability reporter presentation builds
`RelationRequest::missing_property_read`,
`RelationRequest::missing_property_write`,
`RelationRequest::concrete_remapped_mapped_missing_property`, or
`RelationRequest::exact_optional_source_filter` through dedicated relation
outcome helpers when probing whether a source property satisfies a missing
target property, when deciding whether an evaluated concrete remapped mapped
source still fails before missing-property reporting, or when filtering source
union members for exact-optional diagnostic display. The checker owns
missing-property and exact-optional message selection while the requests name
the relation roles.

Union excess-property fallback filtering builds
`RelationRequest::union_excess_required_property` through
`union_excess_required_property_relation_outcome` when probing whether an
explicit source property satisfies a required target property before choosing
the effective unresolved union members for TS2353 checking. The checker owns
union member filtering and excess-property anchoring while the request names the
relation role.

Array-literal contextual collapse builds
`RelationRequest::array_literal_contextual_collapse` through
`array_literal_contextual_collapse_relation_outcome` for construct-signature or
abstract-class override probes, and routes the normal structural-subtype
fallback through `array_literal_contextual_collapse_subtype_outcome` before
collapsing an array literal to its contextual element type. The checker owns
contextual element discovery, structural-subtype fallback selection, and
excess-property timing while the named helpers keep raw relation calls out of
type-computation code.

JSX render fallback selection builds `RelationRequest::jsx_render_fallback`
through `jsx_render_fallback_relation_outcome` when probing whether a construct
return member satisfies a required target member before allowing the fallback
render extraction path. The checker owns the JSX fallback decision while the
request names the relation role.

Object-literal mapped contextual property lookup builds
`RelationRequest::object_literal_mapped_contextual_key` through
`object_literal_mapped_contextual_key_relation_outcome` when probing whether a
literal or numeric property key is accepted by the contextual mapped type's key
constraint before instantiating the mapped template. The checker owns
property-name synthesis, mapped type lookup, and template instantiation while
the request names the relation role.

Object-literal computed-key routing builds
`RelationRequest::object_literal_computed_key` through
`object_literal_computed_key_relation_outcome` when deciding whether a computed
property key flows into the inferred number, symbol, or string index-signature
bucket. The checker owns bucket selection while the request names the relation
role.

Object-literal JSDoc declared-property diagnostics build
`RelationRequest::object_literal_jsdoc_declared_property` through
`object_literal_jsdoc_declared_property_relation_outcome` when prechecking
whether a property initializer satisfies the property type declared by JSDoc
`@type` before emitting TS2322 at the initializer/name anchor. The checker owns
JSDoc declaration discovery, anchor choice, and declared-type fallback while
the request names the relation role.

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

Element-access receiver diagnostics build
`RelationRequest::property_receiver_element_display`,
`RelationRequest::property_receiver_index_value_display`,
`RelationRequest::element_access_number_index`, or
`RelationRequest::element_access_method_suggestion` through dedicated relation
outcome helpers when probing declared receiver element displays, declared index
value displays, TS7015 numeric index compatibility, or `get`/`set` method
suggestion parameters. The checker owns receiver syntax, display selection, and
diagnostic anchoring while the requests name the relation roles.

Call diagnostic elaboration builds `RelationRequest::call_elaboration_mutual`
through `call_elaboration_mutual_relation_outcome` when probing whether two
types share the same key space for richer contextual `keyof` display. The
checker owns display selection while the request names the relation role.

Call diagnostic display formatting builds `RelationRequest::call_display_overlap`
through `call_display_overlap_relation_outcome` when probing whether two types
overlap enough for a more useful diagnostic display. The checker owns display
selection while the request names the relation role.

Call checker generator recovery builds `RelationRequest::call_generator_yield`
through `call_generator_yield_relation_outcome` when probing whether actual and
expected generator yield components are mutually compatible before forcing a
callback return mismatch diagnostic. The checker owns diagnostic filtering while
the request names the relation role.

Checker-only `IteratorResult` value diagnostics build
`RelationRequest::iterator_result_value` through
`iterator_result_value_relation_outcome` when probing whether `undefined`
satisfies a required `value` property before choosing an iterator-result
TS2322 failure reason. The checker owns `IteratorResult` recognition,
target-shape recovery, and diagnostic reason selection while the request names
the relation role.

Round-2 contextual call inference builds
`RelationRequest::round2_contextual_substitution` through
`round2_contextual_substitution_relation_outcome` or
`round2_contextual_substitution_relation_outcome_with_env` when probing
widened/current substitutions against evaluated contextual constraints before
preserving a literal substitution, or when checking whether a probed
instantiated parameter still matches the solver parameter during refinement.
The checker owns widening, literal-preservation policy, substitution, solver
default detection, and constraint evaluation while the request names the
relation role.

Generic constructor inference builds
`RelationRequest::constructor_inference_constraint` through
`constructor_inference_constraint_relation_outcome` or
`constructor_inference_constraint_relation_outcome_with_env` when probing
whether concrete inferred type arguments or actual primitive argument parts
satisfy type-parameter constraints before falling back to
constraint-substituted constructor returns. The checker owns concrete
type-argument filtering, primitive-part extraction, type-parameter discovery,
constraint evaluation, and fallback timing while the request names the relation
role.

The call checker assignability adapter builds
`RelationRequest::call_adapter_compatibility` and
`RelationRequest::call_adapter_identity` through dedicated relation outcome
helpers when call-resolution asks the checker for default compatibility truth or
lazy-resolution identity fallback truth. The adapter owns checker-only
fallbacks such as temporal rounding options while the requests name the relation
roles.

Overload implementation compatibility builds
`RelationRequest::overload_implementation_parameter` through
`overload_implementation_parameter_relation_outcome` when comparing the
implementation signature's parameter surface against an overload after return
compatibility has already been checked and return types are replaced with
`any`. The checker owns overload diagnostic eligibility while the request names
the relation role.

Indexed-access arithmetic diagnostics build
`RelationRequest::binary_arithmetic_number` through
`binary_arithmetic_number_relation_outcome` when probing whether a computed
operand can flow to `number` before accepting an arithmetic fallback. The
checker owns operator recovery and operand anchoring while the request names
the relation role.

Private member access diagnostics build `RelationRequest::private_member_access`
through `private_member_access_relation_outcome` when probing whether the object
type is compatible with the declaring private-member type before choosing
accessibility or shadowing diagnostics. The checker owns private-name scoping,
brand shortcuts, and diagnostic anchoring while the request names the relation
role.

Function type diagnostics build `RelationRequest::function_type_compatibility`
through `function_type_compatibility_relation_outcome` when probing contextual
type-parameter constraints, class coinductive return-cycle parameter
compatibility, or JS constructor return union members before choosing a
function-type diagnostic/recovery shape. The checker owns function syntax,
contextual extraction, and recovery selection while the request names the
relation role.

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

JSX component validation builds `RelationRequest::jsx_element_type` through
`jsx_element_type_relation_outcome` when probing whether a component type is
compatible with a user-defined `JSX.ElementType` after callable-return
shortcuts fail. The checker owns JSX namespace lookup, callable-return
shortcuts, and invalid-component diagnostic anchoring while the request names
the relation role.

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
