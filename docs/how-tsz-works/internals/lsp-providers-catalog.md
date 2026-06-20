# The LSP Provider Catalog: Rename, References, Code Actions, Semantic Tokens, Formatting, Signature Help

## Orientation

The sibling doc [`lsp-and-wasm-surfaces.md`](lsp-and-wasm-surfaces.md) names
the `tsz-lsp` provider families, draws the `Owns / Must not own` boundary for
the whole crate, and explains the three-tier `define_lsp_provider!` macro and
the `Project` container. It does *not* walk into the providers themselves. This
page fills that long-tail gap by going one level deeper into the *read layer* —
the concrete provider structs in
[`crates/tsz-lsp/src`](../../../crates/tsz-lsp/src) that turn a cursor position
into a rename `WorkspaceEdit`, a find-all-references list, a code-action menu, a
delta-encoded semantic-token stream, a formatting `TextEdit` set, or a
signature-help popup. The thesis is the same as its parent's, restated per
provider: **every provider here is a projection of binder/checker/solver data;
none owns a type algorithm.** Rename, references, highlighting, hierarchy,
semantic tokens, document links, selection/folding ranges, and the bulk of the
code-action refactors run entirely off the *binder* (symbols, scopes, the
`ScopeWalker`) and the *parser arena* (AST shapes). Only signature help — the
one `full`-tier provider in this catalog — borrows the `&TypeInterner` and a
per-request `CheckerState` to read call/construct signatures, and even it asks
the checker rather than running inference itself.

The dependency budget is encoded in the tier of the macro that generates each
provider's fields, so the cheapest way to read this catalog is "which tier?"
first. `minimal` providers (document links, folding, selection ranges, linked
editing, file rename) see only `arena`/`line_map`/`source_text`. `binder`
providers (rename, references, highlighting, code actions, call/type hierarchy)
add `binder` + `file_name` and resolve *usages* to a `SymbolId` through the
`ScopeWalker`. The single `full` provider (`SignatureHelpProvider`) adds
`interner`, `strict`, `sound_mode`, `checker_options`, and `lib_contexts`. See
[`checker-context-and-state.md`](checker-context-and-state.md) for what a
`CheckerState` actually is, and
[`solver-call-evaluator-and-inference-kernel.md`](solver-call-evaluator-and-inference-kernel.md)
for the signature machinery signature help reads.

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Cursor → offset → `NodeIndex` lookup (`find_node_at_offset`, `find_node_at_or_before_offset`) and symbol-query gating (`is_symbol_query_node`) | The type universe — `binder`-tier providers never touch `TypeId`; the one `full` provider holds a borrowed `&TypeInterner` and mutates nothing |
| Usage→`SymbolId` resolution via the ephemeral `ScopeWalker` and its `ScopeCache` | Relation/inference/instantiation/evaluation/narrowing — those live in the solver behind `CheckerState` (only signature help reaches them) |
| Wire-format DTOs (`WorkspaceEdit`, `RenameTextEdit`, `ReferenceInfo`, `CodeAction`, semantic-token deltas, `SignatureHelp`) | Diagnostic production — code actions *consume* `LspDiagnostic` codes; they never invent or suppress a diagnostic code |
| Presentation policy: edit shaping, shorthand/import expansion, token classification, `kindModifiers` | The flag-priority cascade itself — that has one owner, `classify::classify_symbol_flags` / `kind_modifiers` |
| The refactor/quick-fix edit *geometry* (where to insert, brace handling, indentation) | Re-binding or re-typing semantics — providers read symbols/types, never create them |
| Refusing unsafe work (formatting fallback is whitespace-only; rename refuses built-ins) | Structural code rewriting without a real parser — internal formatting delegates to Prettier/ESLint or does nothing |

## Module map

| Module path | Provider / type | Tier | Role |
| --- | --- | --- | --- |
| `provider_macro/mod.rs` | `define_lsp_provider!`, `FullProviderOptions` | — | field-and-constructor template; `full` arm owns `checker_with_cache` |
| `resolver/core.rs` | `ScopeWalker`, `ScopeCache`, `ScopeCacheStats` | — | usage→`SymbolId` resolution; the shared backbone of rename/references/highlighting |
| `classify/mod.rs` | `classify_symbol_flags`, `LspSymbolClass`, `kind_modifiers`, `variable_decl_kind` | — | one owner for symbol→presentation mapping |
| `rename/mod.rs`, `rename/core.rs` | `RenameProvider`, `WorkspaceEdit`, `RenameTextEdit`, `PrepareRenameResult` | `binder` | prepare/provide rename, shorthand & specifier expansion |
| `rename/linked_editing.rs` | `LinkedEditingProvider`, `LinkedEditingRanges` | `minimal` | JSX open/close tag sync |
| `rename/file_rename.rs` | `FileRenameProvider`, `ImportLocation` | `minimal` | collect import specifiers referencing a renamed file |
| `navigation/references.rs` | `FindReferences`, `ReferenceInfo`, `RenameLocation` | `binder` | find-all-references, write/definition flags, rename locations |
| `navigation/{definition,declaration,type_definition,implementation,source_definition}.rs` | go-to providers | mostly `binder` | (covered by [`lsp-and-wasm-surfaces.md`](lsp-and-wasm-surfaces.md)) |
| `highlighting/document.rs` | `DocumentHighlightProvider`, `DocumentHighlightKind` | manual | "highlight all occurrences", read/write/keyword |
| `highlighting/semantic_tokens.rs` | `SemanticTokensProvider`, `SemanticTokenType`, `SemanticTokensBuilder` | manual | delta-encoded semantic highlighting |
| `formatting.rs` | `DocumentFormattingProvider`, `FormattingOptions`, `FallbackFormattingMode` | — | Prettier/ESLint shell-out + whitespace-only fallback |
| `signature_help/` (`mod.rs`, `trigger.rs`, `phases.rs`, `shapes.rs`, `selection.rs`, `display.rs`, `docs.rs`, `contextual.rs`) | `SignatureHelpProvider`, `SignatureHelp`, `SignatureInformation` | `full` | active call site, overload candidates, active parameter |
| `code_actions/` (≈27 `code_action_*.rs`) | `CodeActionProvider`, `CodeFixRegistry`, `CodeActionContext` | mixed | quick fixes, refactors, import management, source actions |
| `document_links/mod.rs` | `DocumentLinkProvider`, `DocumentLink` | `minimal` | clickable module specifiers |
| `editor_ranges/{folding,selection_range}.rs` | `FoldingRangeProvider`, `SelectionRangeProvider` | `minimal` | structural editor ranges |
| `hierarchy/{call_hierarchy,type_hierarchy}.rs` | `CallHierarchyProvider`, `TypeHierarchyProvider` | `binder` | incoming/outgoing calls, super/sub types |
| `project/file_context.rs` | `LspProviderContext`, `ProjectFile::provider_context` | — | borrowed five-field view for `from_context` constructors |
| `project/operations.rs` | `Project::get_rename_edits`, `Project::find_references` | — | cross-file orchestration over single-file providers |

## The shared backbone: `ScopeWalker` and the symbol-query gate

Every symbol-oriented provider in this catalog (rename, references,
highlighting, hierarchy, the member-aware code fixes) shares one resolution
backbone, because the binder maps *declaration nodes* to symbols but the editor
needs to resolve *usages* — an identifier on the right-hand side of a `.`, a
type reference, a callee. That backbone is the `ScopeWalker` in
[`crates/tsz-lsp/src/resolver/core.rs`](../../../crates/tsz-lsp/src/resolver/core.rs).
It is *not* the binder; it is a lightweight re-walk that reconstructs the scope
chain on demand and resolves names to existing `SymbolId`s, creating nothing
(see the module doc-comment, "rather than creating new symbols").

`ScopeWalker::resolve_node(root, target)` runs a fast path then a slow path
(`fn resolve_node`):

```
resolve_node(root, target)
  ├─ binder.node_symbols.get(target.0)          → declaration node? return its SymbolId
  ├─ resolve_module_namespace_string_symbol     → string in import/export specifier?
  └─ walk_to_node(root, target, …)              → rebuild scope chain, resolve the name
```

The slow path (`fn walk_to_node` → `walk_to_node_inner`) descends the AST from
`root`, pushing/popping `SymbolTable` scopes (`fn push_scope` / `fn pop_scope`,
tracking function-scope indices for `var` hoisting) until it reaches `target`,
then resolves the identifier text against the assembled scope stack. The walker
starts seeded with `binder.file_locals` (`ScopeWalker::new`). The cached variant
`resolve_node_cached` keys a `ScopeCache` (`type ScopeCache =
FxHashMap<u32, Vec<SymbolTable>>`) by the *target node id*, so repeated cursor
queries against the same node reuse the scope chain; misses fall back to
`get_scope_chain` and then to `binder.resolve_identifier`. `ScopeCacheStats`
records hits/misses for residency telemetry.

Three recursive walks share one explicit depth counter because
`stacker::remaining_stack()` reports only the current segment's headroom inside
a `maybe_grow` closure and therefore "never detects runaway chaining" (see the
field comment on `tree_walk_depth`). `walk_to_node`, `walk_for_scope`, and
`collect_references` all increment `tree_walk_depth` on entry, trip
`ref_walk_stack_tripped` when it exceeds `TREE_WALK_MAX_DEPTH = 4096`, and
short-circuit every subsequent recursive call. The walker is ephemeral (one per
operation) so the trip flag needs no reset. This is the guard that keeps a
pathological or cyclic AST from blowing the OS stack during rename/references.

## Find references

`FindReferences` (`navigation/references.rs`, a `binder`-tier provider) is the
hub the rename and highlight providers both call into. The public entry
`find_references(root, position)` (`fn find_references_internal`) runs:

```
position ──LineMap.position_to_offset──▶ offset
offset   ──find_node_at_offset────────▶ NodeIndex (tightest node containing offset)
node     ──is_symbol_query_node───────▶ gate: identifier / private-id / specifier
                                          string / template part / keyword token
node     ──ScopeWalker.resolve_*──────▶ SymbolId
symbol   ──reference_nodes_for_symbol─▶ Vec<NodeIndex>
nodes    ──location_for_node──────────▶ Vec<Location>
```

`is_symbol_query_node` (`utils/mod.rs`) is the gate: it accepts `Identifier`,
`PrivateIdentifier`, string literals that are import/export specifier text,
no-substitution/template-head/middle/tail tokens (so a tagged template can fall
back to its tag symbol), and the keyword-token range `BreakKeyword..=DeferKeyword`
(so a cursor on `class`/`function` resolves to the declaration).

`reference_nodes_for_symbol_declarations` is where the node set is assembled
(`fn reference_nodes_for_symbol_declarations`): it runs
`ScopeWalker::find_references` (which collects every *usage* node via
`collect_references`), appends the symbol's *declaration* nodes, then calls
`collect_member_access_reference_nodes` for class/interface members. That last
step is the type-directed widener: for a `METHOD`/`PROPERTY`/`GET_ACCESSOR`/
`SET_ACCESSOR` whose parent is a `CLASS`/`INTERFACE`, it scans every
`PROPERTY_ACCESS_EXPRESSION` in the arena, matches the accessed name, and keeps
those whose receiver `expression_has_named_type` resolves to the owning
class/interface — covering `obj.method()` references the lexical scope walk
alone cannot reach. The receiver check handles both `this` (via
`enclosing_class_name`) and a typed variable/parameter (via
`declaration_has_named_type` → `type_node_matches_name` /
`new_expression_matches_name`). All of this is *AST/symbol* reasoning — it reads
type *annotations* by name, never the solver's `TypeId`.

The rich variants layer presentation on top of the node set:

- `find_references_detailed` / `find_references_with_symbol` produce
  `ReferenceInfo { location, is_write_access, is_definition, line_text }`.
  `is_definition_node` tests membership in the symbol's declaration set (or the
  name child of a declaration); `is_write_access_node` is a large AST
  parent-walk recognizing assignment LHS, declaration names, `++`/`--`,
  import/binding/`for-in`/`for-of`/`catch` targets, and class members.
- `find_rename_locations` emits `RenameLocation { file_path, range, line_text }`
  for the `findRenameLocations` protocol.

`resolve_symbol_internal` adds three fallbacks past the plain walker resolve:
`try_keyword_declaration_fallback` (cursor on a declaration keyword →
`node_symbols` of the enclosing declaration), `try_resolve_member_access`
(cursor on the name side of `obj.member` → look the member up in the receiver
symbol's `members`/`exports` tables, or follow a type annotation through
`find_class_member_via_type_annotation`), and `tagged_template_tag` (cursor in a
template → resolve the tag).

## Rename

`RenameProvider` (`rename/core.rs`, `binder` tier) reuses `FindReferences` for
the *where*; its own job is the *what to write*. The two-stage protocol mirrors
tsserver:

`prepare_rename_info(root, position)` returns a `PrepareRenameResult` with
`can_rename`, `display_name`, qualified `full_display_name`, a
`RenameSymbolKind`, `kind_modifiers`, and the `trigger_span`. The renamability
gate is the parity-critical part:

- `rename_target_node` accepts only `Identifier` / `PrivateIdentifier`, plus
  `StringLiteral` when its parent is element access / property assignment /
  import or export specifier.
- `is_non_renamable_builtin` rejects `undefined`, `NaN`, `Infinity`,
  `globalThis`, `arguments`, and the primitive type keywords (`any`, `string`,
  `number`, …) — matching TypeScript's `isKnownIntrinsicTypeSymbol`.
- `import.meta` / `new.target`: the RHS contextual keyword is rejected, covering
  both the real `META_PROPERTY` shape and the tsz-specific
  `PROPERTY_ACCESS_EXPRESSION` lowering of `import.meta`.
- `default` is rejected as a declaration name but allowed as an object-literal
  property name.
- Any path containing `node_modules` is rejected as `ExternalModule`.

The kind/modifiers come from the shared `classify` module:
`classify_symbol_flags` gives the `LspSymbolClass`, and the block-scoped
(`let_or_const_kind`) and function-scoped (`is_parameter`) refinements that need
arena access stay in the provider. `kind_modifiers_for_symbol` delegates to
`classify::kind_modifiers`. `full_display_name` walks parent symbols (bounded to
10 hops) and, for top-level exported values, prefixes the quoted module path
(`"/path/to/module".SymbolName`) to match tsserver display names.

`provide_rich_rename_edits_internal` is the edit pipeline:

```
rename_target_node → old_name (get_identifier_text or source-slice fallback)
normalize_rename_name → validate new identifier (keyword/private/string rules)
FindReferences.find_references → Vec<Location>
dedup_locations → unique (file,line,char,…)
each location → build_rename_edit → RenameTextEdit
```

`build_rename_edit` is where rename earns its parity reputation. A reference
location's span can over-reach the identifier (destructuring binding elements and
shorthand assignments carry the trailing delimiter), so the edit first tightens
the replaced range to exactly `old_name.len()` verified against the source.
Then it detects structural-expansion contexts and emits `prefix_text` /
`suffix_text` metadata instead of a plain replacement:

| Context | Result | Edit |
| --- | --- | --- |
| `SHORTHAND_PROPERTY_ASSIGNMENT` `{ x }` | `{ x: y }` | `with_prefix("x: ")` |
| destructuring `BINDING_ELEMENT` `{ x }` (no `property_name`) | `{ x: y }` | `with_prefix("x: ")` |
| `IMPORT_SPECIFIER` `import { foo }` (no `property_name`) | `import { foo as bar }` | `with_prefix("foo as ")` |
| `EXPORT_SPECIFIER` `export { foo }` (no `property_name`) | `export { bar as foo }` | `with_suffix(" as foo")` |

The `expand_specifiers` flag distinguishes the position-based local rename
(which expands import/export specifiers to keep the public name stable) from the
symbol-based cross-file path (`provide_rename_edits_for_symbol`), which passes
`false` because the [`crate::project`] cross-file machinery rewrites specifiers
directly. `RenameTextEdit::to_text_edit` folds prefix/suffix into a plain edit
for clients that do not consume the rich metadata.

Cross-file rename is orchestrated in `project/operations.rs`
(`Project::get_rename_edits`), not in the provider: it normalizes the name,
resolves the symbol with the file's `scope_cache`, branches to
`get_heritage_rename_edits` for class/interface members, and otherwise computes
`import_targets_for_local` / `exported_names_for_symbol`, resolves module
specifiers, and re-applies the single-file `RenameProvider` to each cross-target
file. The provider stays single-file; the `Project` owns the file fan-out.

The two sibling rename providers are `minimal` tier: `LinkedEditingProvider`
(JSX open/close tag sync, AST-only) and `FileRenameProvider` (collect
`ImportLocation`s referencing a renamed file by walking import/export decls and
`require()`/`import()` calls).

## Code actions

`CodeActionProvider` (`code_actions/code_action_provider.rs`) is a `binder`-tier
struct (it carries `arena`, `binder`, `line_map`, `file_name`, `source`, plus
organize-imports options) split across ≈27 `code_action_*.rs` modules. The
single dispatch point is `provide_code_actions(root, range, context)`; it gates
each family on the client's `only` filter (`request_quickfix`, `request_source`,
`request_refactor`) and then runs the family generators, each of which returns
`Option<CodeAction>` or `Vec<CodeAction>`. The `CodeActionContext` carries the
`Vec<LspDiagnostic>` at the position, the `only` kinds, and project-supplied
`import_candidates`.

```
provide_code_actions
 ├─ quick fixes (per diagnostic): unused_import, unused_declaration,
 │    missing_property, add_missing_const, missing_import (project-aware),
 │    add_missing_await, convert_require_to_import, add_override_modifier,
 │    fix_spelling, prefix_unused_with_underscore
 ├─ source actions: organize_imports, source_actions (remove-unused / sort)
 ├─ refactors (selection): extract_variable / extract_function /
 │    extract_type_alias / surround_with
 ├─ refactors (point): convert_to_arrow / named_function, inline_variable,
 │    generate_accessors, namespace↔named, sort_import_specifiers,
 │    template-string↔concat, arrow braces, optional chaining,
 │    nullish coalescing, move_to_new_file, extract_interface_from_class,
 │    convert_params_to_destructured, add_return_type, async/await,
 │    default↔named export
 └─ deeper quick fixes: implement_interface, override_methods,
      add_missing_switch_cases, fix_all_actions
```

The defining property is that diagnostic-driven quick fixes are keyed on the
*diagnostic code*, never on rendered message text. `unused_import_quickfix`
matches `ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED`, `missing_property_quickfix`
matches `PROPERTY_DOES_NOT_EXIST_ON_TYPE`, `add_missing_const_quickfix` matches
`2304`/`18004` — all from `tsz_checker::diagnostics::diagnostic_codes`. The
provider then locates the AST node at the diagnostic's start offset
(`find_node_at_offset`) and synthesizes the edit geometry: e.g.
`object_literal_property_edits` and `class_property_edits` handle single-line vs
multi-line insertion, trailing-comma detection, and indentation inference
(`indent_at_offset`, `indent_unit_from`) so the inserted `prop: undefined` /
`prop: any;` lands correctly without re-formatting the surrounding code. This is
purely *edit shaping*: the provider never re-checks the type, it just produces
text the user can accept.

`CodeFixRegistry` (`code_action_fixes.rs`) is the static map from error code to
`(fix_name, fix_id, description, fix_all_description)` tuples used to populate
the tsserver `fixName`/`fixId` metadata (e.g. `2663`/`2662` → `spelling` +
`fixForgottenThisPropertyAccess`). The DTOs `CodeFixFileChange` /
`CodeFixTextChange` / `CodeFixPosition` carry the tsserver protocol's 1-based
line/offset shape. Import management (`code_action_imports.rs`) is the
project-aware family: it builds `ImportCandidate`s, finds insertion points,
merges into existing import declarations (`module_specifier_match_for_merge`),
and respects the provider's `new_line_override`. `code_action_editor_features.rs`
adds the file-level `source_actions`, `SourceActionKind`, `LspCommands`,
`PasteAnalysis` (auto-import-on-paste), and `handle_file_deleted`.

## Semantic tokens

`SemanticTokensProvider` (`highlighting/semantic_tokens.rs`) is built by hand
rather than via the macro (it carries `arena`, `binder`, `line_map`,
`source_text`, a `SemanticTokensBuilder`, an `in_decorator` flag, and an
optional `range_filter`). `get_semantic_tokens(root)` does a single document-
order AST walk (`visit_node` → `visit_children` over `arena.get_children`) and
emits one token per classified node into the delta encoder. The wire encoding is
the LSP `[deltaLine, deltaStartChar, length, tokenType, tokenModifiers]` quintet;
`SemanticTokensBuilder::push` computes each token relative to the previous one
(absolute column on a new line, delta column otherwise), which is why tokens
*must* be pushed in document order.

Classification is symbol-flag-driven, not text-driven:

- Modifier keywords (`public`/`private`/`static`/`readonly`/`abstract`/`async`/
  …, via `is_modifier`) emit `Modifier`.
- `DECORATOR` nodes set `in_decorator` so nested identifiers emit `Decorator`.
- For an identifier, `handle_identifier` tries, in order: type-parameter *name*
  (`is_type_parameter_name`) → declaration symbol (`find_declaration_symbol`,
  which confirms the identifier is the *name child* of its declaration parent and
  reads `binder.get_node_symbol`) → reference symbol
  (`binder.resolve_identifier`) → type-parameter *reference*
  (`is_type_parameter_reference`, walking enclosing generic scopes). Unresolved
  identifiers emit *no* token, deferring to the editor's lexical highlighting.

`map_symbol_to_token` is the flag cascade (`CLASS` → `Class`, `INTERFACE` →
`Interface`, `ENUM`/`ENUM_MEMBER`, `TYPE_ALIAS` → `Type`, `TYPE_PARAMETER`,
`FUNCTION`, `METHOD`, accessor/property → `Property`, function-scoped variable
that is a `PARAMETER` → `Parameter` else `Variable`, block-scoped → `Variable`,
modules → `Namespace`, `ALIAS` → `Variable`). Modifiers are layered on:
`STATIC`/`ABSTRACT` from symbol flags; `READONLY` for `const` bindings (via the
shared `classify::is_const_decl` parent walk); `ASYNC`/`DEPRECATED` from the
declaration's `modifier_flags` and node flags; `DECLARATION` for declaration
sites. `get_semantic_tokens_range` reuses the same walk but installs a
`range_filter`, and `emit_token_for_node` skips any token whose span falls
entirely outside the requested range. `DocumentHighlightProvider`
(`highlighting/document.rs`) is a thinner sibling: it routes through
`FindReferences::find_references` and tags each location read/write/text via
`detect_access_kind_ast`, or falls back to control-flow keyword matching.

## Formatting

`DocumentFormattingProvider` (`formatting.rs`) is the deliberate non-provider of
the catalog: it owns *no* AST reasoning and runs *no* type queries. Its policy,
stated in the module doc-comment, is to delegate real formatting to an external
parser-backed tool and otherwise do the least dangerous thing possible:

```
format_document
 ├─ has_prettier()       → format_with_prettier (stdin, --stdin-filepath)
 ├─ has_eslint_fix()     → format_with_eslint   (--fix-dry-run via stdin)
 └─ neither / wasm32     → apply_safe_whitespace_formatting
```

The internal fallback is bounded by the `FallbackFormattingMode` enum:
`WhitespaceOnly` operations (trim trailing whitespace via
`trim_end_matches([' ', '\t'])`, normalize the final newline) are permitted;
anything structural — re-indentation, semicolon insertion/removal, brace
spacing, `as` spacing — is `UnsupportedForStructuralFormatting` and the fallback
returns *no edits* rather than risk corrupting template literals, regex, JSX,
generics, or conditional types. `FormattingOptions` exposes `tabSize`,
`insertSpaces`, `trimTrailingWhitespace`, `insertFinalNewline`,
`trimFinalNewlines`, and a `semicolons` preference that only external formatters
honor. `format_range` and `format_on_key` follow the same whitespace-only floor.
On `wasm32` the external paths are compiled out entirely (no process spawning),
so the browser surface always takes the whitespace-only fallback. This is the
clearest statement of "the emitter/printer owns code shape, the LSP does not":
correct structural formatting needs a real parser, which lives in Prettier/
ESLint, not here.

## Signature help

`SignatureHelpProvider` (`signature_help/mod.rs`) is the only `full`-tier
provider in this catalog, and the only one that reads the type system. It is
split into phase modules — `trigger` (locate the call site, resolve the callee
name, find the active parameter), `phases` (orchestration contexts), `shapes`
(turn solver types into `SignatureCandidate`s), `selection` (overload scoring),
`display` (applicable span + type-param substitution), `docs` (JSDoc
enrichment), and `contextual` (textual fallbacks). The entry point
`get_signature_help(root, position, type_cache)` threads a reusable
`Option<TypeCache>` so repeated keystrokes amortize checker setup.

The data flow through `get_signature_help_internal`:

```
position ─▶ signature_help_trigger_context ─▶ leaf node + offset
         ├─ textual type-argument trigger? (incomplete `foo(bar<` ) → handle, return
         ├─ find_containing_call(leaf, offset) ─▶ (call node, CallSite, CallKind)
         │     │  CallKind ∈ { Call, New, TaggedTemplate }
         │     └─ none → contextual / textual fallbacks
         ├─ type_argument_context_for_call → active param (type-arg vs value list)
         ├─ ScopeWalker.resolve_node(callee_expr) → SymbolId (super() resolves base)
         ├─ checker = self.checker_with_cache(type_cache)        ← macro-generated
         ├─ callee_type = checker.get_type_of_symbol | get_type_of_node
         │               then resolve_lazy_type
         ├─ collect_signature_candidates_for_call → Vec<SignatureCandidate>
         ├─ apply_signature_docs / apply_source_signature_type_overrides (JSDoc, source text)
         ├─ infer_type_param_substitutions_from_arguments → apply_type_param_substitution
         └─ select_signature_help_display → active_signature, span, counts
```

The boundary discipline is exactly the same as the checker's, one level out: the
provider asks `CheckerState` for the callee's type (`get_type_of_symbol` /
`get_type_of_node`) and resolves `TypeData::Lazy` refs
(`resolve_lazy_type`), but the *shape extraction* in `signature_help/shapes.rs`
reads only solver-published structure — `visitor::function_shape_id` /
`callable_shape_id`, then `interner.function_shape` / `callable_shape`, yielding
`FunctionShape` / `ParamInfo` with `TypeId`s. It selects call vs construct
signatures by `CallKind` (`include_call` for `Call`/`TaggedTemplate`,
`include_construct` for `New`). It never runs an inference round itself; even
the type-parameter substitution is textual display polish over solver-provided
default/constraint/`unknown` substitutions. Parity edge cases handled inline:
interfaces/type aliases used as call targets get no help; `new` on private or
protected constructors out of scope returns nothing
(`checker.is_private_ctor` / `is_protected_ctor`); `super()` resolves the base
class's construct signatures via `find_base_class_expression`. When the solver
yields nothing, `signature_help_for_textual_call` /
`_textual_type_arguments` provide JS-friendly fallbacks. After the request the
checker's cache is extracted back into `*type_cache` so the next keystroke
reuses it.

For built-in primitive methods the provider supplements solver shapes with the
hand-authored `intrinsic_params` table (`string_intrinsic_method_params`,
`number_intrinsic_method_params`, …) so `"x".slice(` shows parameter names the
lib `.d.ts` would otherwise only give as positional types.

## Document links, ranges, hierarchy

These are the `minimal`/`binder` long tail:

- `DocumentLinkProvider` (`document_links/mod.rs`, `minimal`) walks the AST
  (`collect_links`) for `IMPORT_DECLARATION`/`EXPORT_DECLARATION` module
  specifiers and dynamic `import()`/`require()` calls, emitting a `DocumentLink`
  range over the specifier text (quotes excluded). Pure syntax.
- `FoldingRangeProvider` and `SelectionRangeProvider` (`editor_ranges/`,
  `minimal`) compute structural ranges (collapsible blocks/imports/comments/
  `#region`; expand-by-semantic-boundary) straight from the arena.
- `CallHierarchyProvider` and `TypeHierarchyProvider` (`hierarchy/`, `binder`)
  navigate incoming/outgoing calls and super/sub types. They resolve symbols
  through the same backbone and, for cross-file links, are re-invoked per
  `ProjectFile` by `project/features.rs` (`from_context(other_file.provider_context())`).

## Caches and invariants

| Cache / invariant | Owner | Key | Invalidation |
| --- | --- | --- | --- |
| `ScopeCache` (`FxHashMap<u32, Vec<SymbolTable>>`) | `resolver/core.rs`, held per `ProjectFile` | target `NodeIndex.0` | dropped/rebuilt when the file is re-parsed; ephemeral `ScopeWalker` per call |
| `ScopeCacheStats` | `resolver/core.rs` | — | telemetry only (`hits`/`misses`), no correctness effect |
| per-file `TypeCache` (signature help) | `provider_macro` `checker_with_cache`, stored as `Option<TypeCache>` | checker-internal | `take()`n into the request's `CheckerState`, `extract_cache()`d back after |
| `tree_walk_depth` / `ref_walk_stack_tripped` | `ScopeWalker` | — | reset by walker being ephemeral; trips at `TREE_WALK_MAX_DEPTH = 4096` |
| `dedup_locations` / `sort_dedup_nodes` | rename / references | (file, line, char) tuple or `NodeIndex.0` | per request; guarantees one edit per textual location |

Invariants worth restating: providers create no symbols and intern no types
(only `SignatureHelpProvider` even holds `&TypeInterner`, and read-only).
Diagnostic codes are inputs to code actions, never outputs. Marker/fourslash
awareness is forbidden inside any production provider — they must behave
identically for user-typed and marker-annotated text (the fourslash harness in
`fourslash.rs` is test-only). The presentation cascade has exactly one owner
(`classify`); a provider may *refine* a class with arena access (const/let,
parameter/var, getter/setter) but may not reorder the cascade.

## Edge cases and tsc parity

- **Rename span tightening.** Destructuring binding elements and shorthand
  property assignments carry a node span that includes the trailing `,`/`}`;
  `build_rename_edit` verifies `old_name` against the source slice and shrinks
  the replaced range so the expansion does not eat the delimiter.
- **Shorthand vs full property.** `{ x }` renamed to `y` expands to `{ x: y }`
  (a `prefix_text`), never `{ y }`, which would silently bind a different
  property. Symbol-based cross-file rename disables specifier expansion because
  the project layer rewrites specifiers directly.
- **Non-renamable intrinsics.** `any`/`string`/`number`/… parse as identifiers
  in type positions but are rejected by `is_non_renamable_builtin`, matching
  TypeScript's `isKnownIntrinsicTypeSymbol`. `import.meta`/`new.target` RHS and
  declaration-position `default` are likewise rejected.
- **Member references via type.** `collect_member_access_reference_nodes` finds
  `obj.method()` references that the lexical scope walk misses, by matching the
  receiver's *declared type name* — keeping find-references and rename in sync
  with tsc's symbol-based search without running the type checker.
- **Semantic tokens for unresolved identifiers.** No token is emitted, so the
  editor's TextMate grammar provides the color; tsz does not guess.
- **Signature help on type-only / inaccessible callees.** Interfaces and type
  aliases as call targets, and `new` on private/protected constructors from
  out-of-scope, produce no help — matching tsserver's suppression.
- **Formatting refuses structural edits.** With no external formatter (always
  the case on `wasm32`), the fallback only trims whitespace and normalizes the
  final newline; it never re-indents or touches semicolons, preferring "no
  edits" to "risky edits".

## See also

- [`lsp-and-wasm-surfaces.md`](lsp-and-wasm-surfaces.md) — the crate-wide
  boundary, the `define_lsp_provider!` tiers, hover/completions, the `Project`
  container, and the WASM `wasm_api` shim this catalog sits beneath.
- [`checker-context-and-state.md`](checker-context-and-state.md) and
  [`checker-type-of-symbol-and-symbol-types.md`](checker-type-of-symbol-and-symbol-types.md)
  — what `CheckerState::get_type_of_symbol` / `get_type_of_node` return to
  signature help.
- [`solver-call-evaluator-and-inference-kernel.md`](solver-call-evaluator-and-inference-kernel.md)
  and [`checker-calls-signatures-generics.md`](checker-calls-signatures-generics.md)
  — the call/construct signature machinery whose `FunctionShape`s signature help
  formats.
- [`binder.md`](binder.md) — the symbol tables, `node_symbols`, and
  `file_locals` the `ScopeWalker` re-walks.
- [`checker-error-reporter-diagnostics.md`](checker-error-reporter-diagnostics.md)
  — the diagnostic codes code actions key their quick fixes on.
- [`cli-surface-and-diagnostic-reporting.md`](cli-surface-and-diagnostic-reporting.md)
  — the other consumer of `tsz_checker::diagnostics`.
