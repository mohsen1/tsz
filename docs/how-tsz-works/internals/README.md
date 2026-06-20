# Internals: Deep-Dive Tier

This is the deep-dive tier of the `tsz` guide. It sits between the narrative map
chapters one level up (such as [`../05-checker.md`](../05-checker.md) and
[`../06-solver.md`](../06-solver.md)), which give the wide architectural shape,
and the generated [`../file-inventory/`](../file-inventory/README.md), which
lists every repository file path. The map chapters tell you which layer owns a
truth; these pages walk the actual modules, data shapes, entry functions,
caches, fuel limits, and `tsc` parity edge cases that implement that truth. Each
doc opens with an "Owns / Must not own" table so the boundary stays explicit,
then traces real call paths with grep-verified identifiers.

Read a map chapter first for orientation, then drop into the matching internals
doc below when you need the exact mechanism.

## Checker

The checker orchestrates whole-file checking, builds context and diagnostics,
and asks the solver for semantic answers.

- [Checker Context, Session State, and the Whole-File Checking Lifecycle](checker-context-and-state.md) - how `CheckerState`/`CheckerContext`, the two `TypeEnvironment`s, `DefId` resolution, and the per-file lifecycle and caches fit together.
- [Computing the Type of a Symbol: get_type_of_symbol and the type_resolution Engine](checker-type-of-symbol-and-symbol-types.md) - the value-position vs type-position entry points, the cache and re-entrancy kernel, `DefId` minting, and cross-arena delegation.
- [Flow Graph Construction and Narrowing Orchestration in the Checker](checker-flow-and-narrowing.md) - the binder-built flow graph, `get_flow_type`, loop fixed points, and the checker-to-solver narrowing boundary.
- [Reachability and Control-Flow Completeness](checker-reachability-and-cfa-completeness.md) - the structural reachability walk behind TS7027/TS7029/TS7030/TS2355/TS2366, never-returning calls, switch exhaustiveness, and TS2454 definite assignment.
- [Class Checking: Declarations, Inheritance, Implements, and Members](checker-classes.md) - member comparison, extends/implements paths, abstract completeness, visibility brands, and strict initialization.
- [Building Class Instance and Constructor Types](checker-class-shape-construction.md) - the `ClassInstanceBuilder` phase pipeline, the static/constructor side, and the `Lazy(DefId)` handoff that names both shapes.
- [Call Resolution, Overloads, and Generic Checking on the Checker Side](checker-calls-signatures-generics.md) - argument collection, contextual typing, overload resolution, and the checker side of generic inference dispatch.
- [Declarations: Imports, Exports, Namespaces, Modules, and Ambient](checker-declarations-modules.md) - module-resolution integration, re-exports, namespace merging, ambient handling, and verbatim/isolated-module rules.
- [JSX, Property Access, Accessors, Enums, Iterables, and Promises](checker-jsx-properties-accessors-enums.md) - the property-access chokepoint plus accessors, enums, `for...of` iterables, `await`, and JSX checking.
- [Const Enums and Compile-Time Literal Evaluation](checker-const-enum-and-literal-evaluation.md) - the two checker constant folders, the ECMAScript-wrapping arithmetic kernel, auto-increment, cross-enum resolution, and TS2474/TS2477/TS2478.
- [The Assignability Gateway and Query Boundaries](checker-assignability-gateway.md) - the relation -> reason -> diagnostic gateway shared by TS2322/TS2345/TS2416, its cache namespaces, and acceptance gates.
- [Diagnostic Rendering, Elaboration, Priority, Suppression, and Recovery](checker-error-reporter-diagnostics.md) - anchor resolution, elaboration into related chains, dedup/priority/suppression, the printer boundary, and recovery semantics.

## Solver

The solver owns relations, evaluation, inference, instantiation, narrowing,
operations, the type universe, caches, and the compatibility policy.

- [The Relation Engine: Subtype, Assignability, Identity, Comparable, Variance](solver-relations.md) - the Judge (structural subtype kernel) vs the Lawyer (`CompatChecker`), variance, weak/freshness quirks, and failure reasons.
- [Type Inference: Candidate Collection, Contextual Inference, and Priorities](solver-inference.md) - `infer_from_types`, the `constrain_types` walker, inference priorities, two-round fixing, and default fallbacks.
- [The Call-Evaluation Kernel: Arguments, Contextual Typing, and Reverse/Mapped Inference](solver-call-evaluator-and-inference-kernel.md) - the `CallEvaluator` driver, overload dispatch, the two-round generic schedule, and reverse-mapped/keyof inference.
- [Contextual Typing and Reverse Inference](solver-contextual-typing-and-reverse-inference.md) - the `ContextualTypeContext` descent, parameter/property/element extractors, the merge rule, and the two reverse-inference machineries.
- [Instantiation, Type Mappers, and Instantiated-Type Caching](solver-instantiation.md) - `TypeSubstitution`, the `TypeInstantiator`, meta-type arms, alpha-canonical caching, and the DefId/Lazy/Application interaction.
- [Evaluation of Conditional, Mapped, Template, Infer, keyof, and Index Types](solver-evaluation.md) - the evaluation driver loop, conditional distribution, `infer` extraction, mapped/keyof/indexed/template evaluation, and recursion fuel.
- [Mapped-Type and Tuple Shards: Homomorphic Mapping, Key Remapping, Tuple Rebinding](solver-mapped-and-tuple-shards.md) - the homomorphic deferral cascade, `as`-clause key remapping, the four-case tuple rebinding switch, and modifier arithmetic.
- [Narrowing and Type Guards in the Solver](solver-narrowing.md) - the narrowing dispatcher, typeof/discriminant/predicate/instanceof/in guards, exclusion narrowing, and the work budget.
- [Operations: Binary, Unary, and Index/Property Access Type Computation](solver-operations.md) - the `BinaryOpEvaluator` kernel, nullish-coalescing diagnostics, unary and compound assignment, and property/element access.
- [The Type Universe: TypeData, Interning, DefId/Lazy Resolution, Canonicalization](solver-types-intern-def.md) - the handle family, `TypeId` layout and sentinels, the content-addressed `TypeInterner`, DefId/Lazy indirection, and canonicalization.
- [Caches, Object Types, Contextual Typing, and the Compatibility Model](solver-caches-objects-contextual-compat.md) - the cache database trait stack, `QueryCache`/`SharedQueryCache`, object/interface representation, contextual reverse inference, and the compat layer.

## Front end and emitter

The bookends of the pipeline: text becomes an AST and symbol graph, then a
checked program becomes JS and `.d.ts` output.

- [Front End: Scanner and Parser](front-end-scanner-parser.md) - lexing, token rescanning, interning, the AST arena and node pools, and parser speculation/recovery.
- [Binder: Symbols, Scopes, Hoisting, Flow Skeleton, and Module Graph](binder.md) - the symbol model, scopes and the container tree, hoisting, `declare_symbol`, the flow skeleton, modules, and the DefId on-ramp.
- [Emitter: JS Emit, Transforms, Lowering, Declaration Emit, and Source Maps](emitter.md) - the lowering and print passes, per-feature transforms, module-format output, helper/temp planning, `.d.ts` emit, and source maps.
- [Async/Generator State Machines, Decorators, and Module-Format Wrapping in Emit](emitter-async-generator-decorators-modules.md) - the `__generator` opcode protocol, legacy vs TC39 decorator lowering, and the CommonJS/AMD/UMD/System wrappers with live exports.
- [Emit Helpers and Decorator Metadata Serialization](emitter-helpers-and-decorator-metadata.md) - the emit-helper library and scheduling, plus `design:type`/`design:paramtypes`/`design:returntype` decorator metadata serialization.
- [Source-Map Generation and the Mapped Output Pipeline](emitter-source-maps.md) - VLQ-encoded mappings, name and source tables, inline vs external maps, and how mapped positions thread through the print pipeline.

## Driver, editor, and the full timeline

The layers that drive whole programs through the pipeline, resolve modules,
reuse work across builds, and expose the compiler to editors, the browser, and
the command line.

- [The Module-Resolution Engine and Re-Export Validation](module-resolution-engine.md) - the resolution ladder, ESM vs CJS classification, `exports`/`imports` subpaths, the probing caches, and re-export/import-attribute validation.
- [Incremental Build and Watch Mode](driver-incremental-and-watch.md) - the `.tsbuildinfo` on-disk format, the live `CompilationCache`, watch debouncing and invalidation, and file-session reuse.
- [Project References and Build Mode](driver-project-references-and-build-mode.md) - the reference graph, Kahn build order, composite fan-out, the up-to-date check, and the `extends` timeline.
- [The Parallelism and Determinism Model](driver-parallelism-and-determinism.md) - the work-distribution policy, where parallel execution is allowed, and how deterministic ordering is preserved across threads.
- [Editor and Browser Surfaces: the LSP Language Service and the WASM API](lsp-and-wasm-surfaces.md) - the provider tier macro, the `ScopeWalker` cursor resolution, hover/completion/signature-help, pull-model diagnostics, and the WASM surface.
- [The LSP Provider Catalog: Rename, References, Code Actions, Semantic Tokens, Formatting, Signature Help](lsp-providers-catalog.md) - the per-provider catalog covering rename, find-references, code actions, semantic tokens, formatting, and signature help.
- [The CLI Surface and Diagnostic Reporting](cli-surface-and-diagnostic-reporting.md) - the command-line entry surface, flag parsing, the program driver, and how diagnostics are formatted and reported to the terminal.
- [End-to-End Timeline: One File from Text to Output, and the Driving Layer](end-to-end-timeline.md) - `compile_inner` and the driving layer, single-file and cross-file walk-throughs, parallelism policy, and LSP/WASM/`--build` reuse.

## How these were written and how to extend them

Each page was written from source read directly in the tree, with every
CamelCase identifier, file name, and dotted path grep-verified before use. They
intentionally cite real function names, constant values, and diagnostic codes so
a reader can jump straight to the implementation. When you extend or correct a
page, follow the same discipline: ground every claim in the code, keep the
"Owns / Must not own" boundary honest, and back parity claims with a structural
rule rather than a fixture name.

For where these pages fit in the larger guide and how to keep the file inventory
in sync after adding or moving files, see
[`../12-maintaining-this-guide.md`](../12-maintaining-this-guide.md).
