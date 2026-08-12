//! TS1xxx/#16279 self-suppression list: parser-emitted codes that tsc's
//! checker would emit via `grammarErrorOnNode` instead. Split out of
//! `check_utils.rs` to stay under the CLI boundary's 2000-line cap; this is
//! a single, self-contained classification table, not a shared dependency of
//! the rest of that file.

/// Parser-emitted codes that tsc emits via grammarErrorOnNode in the checker.
/// These should be suppressed when the file has parse errors, matching tsc behavior.
/// Only includes codes confirmed to be checker-side grammar checks in tsc that
/// our parser emits instead.
///
/// Membership is load-bearing in BOTH directions. A listed code is suppressed
/// when a real parse error exists. And every listed code is folded into
/// `is_non_suppressing_parse_error` by construction (that predicate delegates
/// to this one), so a parser-emitted grammar code that is *omitted* here is
/// classified as a suppressing "real parse error" by
/// `has_non_grammar_parse_error` and silently deletes every listed sibling in
/// its file. (Before the filter-trigger unification `has_non_grammar_parse_error`
/// was the literal complement of this list; it is now computed from
/// `is_non_suppressing_parse_error`, but the containment above keeps the hazard
/// — and the fix — identical.) So omitting one member of a `grammarError`-family
/// silently deletes the rest of its family: TS1049/TS1051 were missing here,
/// and any file whose setter tripped one of them lost the getter's TS1054
/// entirely.
///
/// The accessor family (tsc's single `checkGrammarAccessor`) is therefore all
/// in or all out: TS1049, TS1051, TS1054, TS1095. TS1052/TS1053 belong to the
/// same tsc function but are checker-emitted in tsz, so they never reach this
/// parse-diagnostic filter.
///
/// Same shape for tsc's `checkGrammarParameterList`, which reports TS1014 (a
/// rest parameter must be last), TS1047 (a rest parameter cannot be
/// optional), and TS1048 (a rest parameter cannot have an initializer) from
/// one function. TS1014 was listed, TS1047/1048 were not: a file whose rest
/// parameter tripped 1047 or 1048 silently deleted an unrelated function's
/// TS1014 elsewhere in the same file. TS1015/1016 belong to the same tsc
/// function but are checker-emitted in tsz (`parameter_checker.rs`), so they
/// never reach this filter either.
///
/// #16279 audit round: 11 more parser-emitted codes confirmed
/// checker-suppressible against a real `typescript@7.0.2` oracle (a genuine
/// unrelated syntax error in the same file drops each of these, matching the
/// families already above) and added here: TS1079/1120 (modifier-on-a-
/// declaration-that-cannot-have-modifiers, siblings of TS1191/1193 above),
/// TS1092/1094 (type parameters where none are allowed, siblings of
/// TS1093/1054 above), TS1098/1099 (list-cannot-be-empty, sibling of TS1097
/// above), TS1242 (modifier-can-only-appear-on-X, sibling of TS1275 below),
/// TS1246/1247 (property initializer not allowed in a type position), and
/// TS1491/1495 (modifier-on-a-using-declaration). Two adjacent candidates
/// with the same "modifier/decorator in the wrong place" shape, TS1433 and
/// TS1436, were oracle-tested and rejected: tsc keeps both alongside an
/// unrelated syntax error in the same file, so they are real parser
/// diagnostics in tsc too and must NOT be added here.
///
/// #16279 audit round 2: tsc's `checkGrammarForInOrForOfStatement` reports
/// TS1091 (`for...in`) and TS1188 (`for...of`) — "only a single variable
/// declaration is allowed" — for a multi-declarator loop head. Both are
/// parser-emitted in tsz (`state_declarations_exports.rs`) and were entirely
/// absent from this list, so neither was ever suppressed alongside a real
/// syntax error the way tsc suppresses both (oracle-confirmed against
/// `typescript@7.0.2`: a `let a: = 1;` syntax error elsewhere in the file
/// drops the TS1091/TS1188 that would otherwise fire), and each counted as a
/// "real" parse error that could silently delete an unrelated listed
/// sibling from the same file.
///
/// #16279 audit round 3: two more parser-emitted, unrelated-family codes
/// confirmed checker-suppressible against `typescript@7.0.2` (Direction B: a
/// genuine unrelated syntax error elsewhere in the file drops each of these
/// on the real compiler, AND the real `tsz` binary was rebuilt and rerun on
/// each witness end-to-end, not just the synthetic-diagnostic unit test).
/// TS1156 (`'{0}' declarations can only be declared inside a block`,
/// `state/recovery.rs`) and TS1358 (`tagged template expressions are not
/// permitted in an optional chain`, `state_expressions_call_member.rs`;
/// reachable via the immediate "optional-chain token directly followed by a
/// template literal" shape — a chain that opens earlier and continues into
/// a tagged template is a separate, unclaimed detection gap, not this
/// list's concern), and TS18024 (`an enum member cannot be named with a
/// private identifier`, `state_declarations.rs`).
///
/// #16279 audit round 4: `checkGrammarForInOrForOfStatement` also reports
/// TS1493/TS1494 (`for...in` left-hand side cannot be a `using`/`await
/// using` declaration) from the same tsc function as the already-listed
/// TS1091/TS1188, and both are parser-emitted in tsz
/// (`state_declarations_exports.rs`). Both were entirely absent from this
/// list — oracle-confirmed (`typescript@7.0.2`): a `let a: = 1;` syntax
/// error elsewhere in the file drops TS1493/TS1494 on the real compiler,
/// and, mirroring TS1091/TS1188's own discovery, an unlisted TS1493/TS1494
/// counted as a "real" non-grammar parse error and could silently delete an
/// unrelated *listed* sibling (e.g. TS1091 itself) from the same file.
///
/// Two adjacent candidates from the same scan were investigated and
/// deliberately left OUT of this round, each for a different reason —
/// worth reading before extending this list, since neither failure mode is
/// obvious from a synthetic-diagnostic unit test alone:
/// - **TS2499** (`an interface can only extend an identifier/qualified-name
///   with optional type arguments`) is Direction-B-confirmed suppressible,
///   but has a pre-existing, unrelated double-emission bug: both the parser
///   (`state_statements_class_declarations.rs`) and the checker
///   (`state/state_checking/heritage.rs`, its own independent TS2499 walk)
///   report it for the same node, so `interface I extends (1 + 2) {}`
///   reports TS2499 twice today regardless of this list. Adding it here
///   would fold a real emission-site bug into this suppression-only audit;
///   left for its own fix.
/// - **TS2427/TS2457** (interface/type-alias reserved names) are resolved by
///   emission site rather than by this list (#16279): the parser owns the
///   hard-keyword `void`/`null` (and numeric) interface-name TS2427, the checker
///   owns the soft predefined-type names, and the CLI keep-gate
///   (`checker_diagnostics.rs`) deduplicates and suppresses per tsc's
///   `hasParseDiagnostics` — so neither code belongs in this parser-grammar
///   list. TS2819 (namespace reserved names, the third member of this family)
///   was oracle-tested and rejected: tsc keeps it alongside an unrelated syntax
///   error in the same file, unlike its TS2427/TS2457 siblings — so family
///   membership must not be assumed from a sibling's membership.
///
/// #16279 audit round 5: TS1492 (`'{0}' declarations may not have binding
/// patterns.`, `state_variable_declarations.rs` — `using {a} = x` / `await
/// using [a] = x`) is the direct-declaration sibling that round 4's
/// `for...in`/`for...of` TS1493/TS1494 left out of the same "using
/// declaration" grammar family. Parser-emitted and, like its siblings, was
/// entirely absent from this list. Oracle-confirmed against
/// `typescript@7.0.2` (Direction B): an unrelated `let x: = 1;` syntax error
/// elsewhere in the file drops the TS1492 that would otherwise fire. End-to-
/// end witness against the rebuilt `tsz` binary: `using {a} = null as any;`
/// next to `export declare import x = require("y");` in the same file —
/// tsc reports both TS1492 and TS1079 (the `declare`-on-an-import-declaration
/// modifier error, already listed above); tsz's `main` before this entry
/// reported only TS1492, silently dropping TS1079 because the unlisted TS1492
/// counted as a "real" parse error.
///
/// #16279 audit round 6: full re-derivation of the grep-and-diff recipe (every
/// `diagnostic_codes::*` name emitted from `crates/tsz-parser/src`, resolved
/// to codes, diffed against this list) surfaced 132 unlisted candidates.
/// Batch Direction-A/B oracle testing against `typescript@7.0.2` found one
/// genuine addition: TS1326 (see its own comment below) — and several
/// look-alikes that were tested and correctly rejected because they survive
/// Direction B (tsc does **not** suppress them alongside an unrelated real
/// syntax error, so they must stay unlisted): TS1034 (`'super' must be
/// followed by an argument list or member access`), TS1477 (`an
/// instantiation expression cannot be followed by a property access`),
/// TS2754 (`'super' may not use type arguments`), TS18009 (`private
/// identifiers cannot be used as parameters`), TS18029 (`private identifiers
/// are not allowed in variable declarations`), and TS18030 (`an optional
/// chain cannot contain private identifiers` — already handled by a
/// different mechanism, see `is_real_syntax_error` below).
///
/// **Not pursued, flagged for a future slice**: TS18016 (`private
/// identifiers are not allowed outside class bodies`) is also Direction-B
/// suppressible, but unlike TS1326 it has four checker-side emission sites
/// (`types/type_checking/core.rs`, `assignability/assignment_checker/
/// assignment_ops.rs`, `state/type_analysis/computed_helpers_private.rs`)
/// alongside its five parser sites — the same double-emission shape that
/// blocked TS2499 in round 3. Adding it here only patches the parser-emitted
/// occurrences and needs the checker-side sites audited first, same caveat.
///
/// #16279 audit round 10: TS1313 (`the body of an 'if' statement cannot be
/// the empty statement`, `state_declarations_exports.rs`) was round 3's
/// deferred "adjacent candidate" — Direction-B-confirmed suppressible, but
/// adding it here previously made it suppress *itself*, because it was
/// simultaneously a member of `is_real_syntax_error`/`is_structural_parse_error`
/// (`check_utils.rs`) under an unrelated, non-existent message ("'else' is
/// not allowed after rest element" matches no tsc diagnostic anywhere in
/// `crates/tsz-common/src/diagnostics`) — a stale mislabel, not a deliberate
/// classification. The `then_statement` node TS1313 is reported on is a
/// well-formed `EMPTY_STATEMENT`, not an error-recovery placeholder, so it
/// was never a structural parse failure. Oracle-confirmed against
/// `typescript@7.0.2`: `if (true);` alone reports TS1313 (Direction A);
/// `if (true); let x: = 1;` drops TS1313 and keeps only the real error
/// (Direction B); `if (true); undeclaredName;` reports BOTH TS1313 and
/// TS2304 (TS1313 does not itself suppress cascading semantic diagnostics,
/// unlike a genuine structural failure). Fix removed 1313 from the two
/// structural-error lists and added it here instead.
///
/// #17253: TS1155 (`'{0}' declarations must be initialized.`,
/// `state_variable_declarations.rs`/`state_statements.rs` via the shared
/// `report_const_or_using_uninitialized`/`report_for_header_const_using_uninitialized`
/// owners) was wired by #17251 as a parser diagnostic but landed in
/// `is_real_syntax_error`/`is_structural_parse_error` instead of here — the
/// same self-suppression trap TS1313 hit at round 10. tsc's
/// `checkGrammarVariableDeclaration` reports TS1155 from the checker over a
/// syntactically valid AST (a `const`/`using`/`await using` declarator with no
/// initializer parses cleanly), so it never suppresses a file's other checker
/// diagnostics. Being misclassified as a structural/real syntax error instead
/// set `has_real_syntax_errors` for the whole file, which broadly suppresses
/// checker diagnostics — dropping TS2588/TS7005 companions the real compiler
/// keeps (`constDeclarations-errors.ts`, `for-of2.ts`,
/// `downlevelLetConst2.ts`; oracle-confirmed against `typescript@7.0.2`,
/// `const x; y();` reports both TS1155 and TS2304). It also went unsuppressed
/// itself alongside an unrelated real syntax error in the same file
/// (`decoratorOnUsing.ts`, `commonMissingSemicolons.ts`), the mirror-image
/// half of the same membership gap. Fix: removed from the two structural-error
/// lists in `check_utils.rs`, added here instead — the TS1313 fix, replayed.
///
/// #16279 audit round 11: re-derived the grep-and-diff recipe (every
/// `diagnostic_codes::*` name emitted from `crates/tsz-parser/src`, resolved
/// to numeric codes, diffed against this list AND
/// `is_non_suppressing_parse_error`'s extra codes plus the 1499-1538 regex
/// band — round 6's diff undercounted coverage by treating those as
/// unlisted) surfaced 80 remaining candidates. Message-text triage found the
/// overwhelming majority are genuine "X expected"/"unterminated X" structural
/// syntax errors that must stay unlisted. One real addition: **TS8020**
/// (`JSDoc types can only be used inside documentation comments.`, a bare `*`
/// in type position outside a doc comment — `state_types.rs`,
/// `state_types_jsx.rs`). `check_utils.rs`'s own `is_js_only_syntactic_diagnostic`
/// doc comment already states "JSDoc-related `TS8xxx` codes (TS8020-TS8039 save
/// for TS8038) come from the checker" in tsc, corroborating the classification
/// independently of the oracle run. Oracle-confirmed against
/// `typescript@7.0.2`: Direction A, `let x: *;` alone reports TS8020;
/// Direction B, the same line plus an unrelated real syntax error
/// (`let y: = 1;`) drops TS8020 entirely, leaving only TS1110; self-suppression
/// witness, `class C { get x(a: number) { return a; } }` next to `let y: *;`
/// reports BOTH TS1054 and TS8020 on tsc, confirming the already-listed
/// TS1054 would have been silently deleted by the unlisted TS8020. No
/// checker-side emission site in tsz (parser-only), so no double-emission
/// risk. Sole rejected candidate this round: **TS6189** (`Multiple
/// consecutive numeric separators are not permitted.`) survives Direction B
/// (kept alongside an unrelated real syntax error on the real compiler), so —
/// like its already-rejected sibling TS6188 — it is a genuine parser
/// diagnostic in tsc too and must NOT be added; tested independently rather
/// than assumed from TS6188's membership, per round 4's own caution.
pub(super) const fn is_parser_grammar_code(code: u32) -> bool {
    matches!(
        code,
        1013 // A rest parameter or binding pattern may not have a trailing
             // comma. tsc's checkGrammarParameterList/checkGrammarAccessor/
             // checkGrammarMethod report this from the checker for a rest
             // parameter or destructuring binding pattern; tsz emits it from
             // the parser at four sites (state_expressions_literals.rs,
             // state_types_jsx.rs, state_statements_class.rs). A distinct
             // checker-emitted site (assignment_ops.rs) also reports this code
             // for a destructuring-*assignment* target's trailing comma, but
             // that is a `CheckerDiagnostic`, never a `ParseDiagnostic`, so it
             // never reaches this filter and is unaffected by this entry.
        | 1014 // A rest parameter must be last in a parameter list
        | 2462 // A rest element must be last in a destructuring pattern. tsc's
               // checkGrammarBindingElement reports this from the checker;
               // #16989 moved tsz's binding-pattern check into the parser
               // (report_rest_element_not_last) to cover every binding-pattern
               // position uniformly, which made it a ParseDiagnostic and so
               // subject to this filter. Unlisted it behaved as a real parse
               // error: it survived alongside a genuine syntax error (tsc drops
               // it), and it deleted listed siblings — a file with both a
               // misplaced binding-pattern rest and a misplaced rest parameter
               // lost its TS1014 entirely. Sibling of TS1014 in both tsc's
               // grammar family and this list. The distinct checker-emitted
               // site for destructuring-*assignment* targets (#16966,
               // assignment_ops.rs) is a CheckerDiagnostic, never a
               // ParseDiagnostic, so it never reaches this filter.
        | 1017 // An index signature cannot have a rest parameter
        | 1018 // An index signature parameter cannot have an accessibility modifier
        | 1101 // 'with' statements are not allowed in strict mode. tsc's
                // checkStrictModeWithStatement is a binder check
                // (file.bindDiagnostics); tsz emits it eagerly from the parser
                // for the syntactically-auto-strict cases (class body, ES
                // module top level) since that context is known without the
                // checker. Route it through the same hasParseDiagnostics-style
                // suppression as its checker-emitted binder-check siblings.
        | 1019 // An index signature parameter cannot have a question mark
        | 1020 // An index signature parameter cannot have an initializer
        | 1021 // An index signature must have a type annotation
        | 1025 // An index signature cannot have a trailing comma
        | 1028 // Accessibility modifier already seen
        | 1029 // '{0}' modifier must precede '{1}' modifier
        | 1030 // '{0}' modifier already seen
        | 1031 // '{0}' modifier cannot appear on class elements of this kind
        | 1040 // '{0}' modifier cannot be used in an ambient context
        | 1042 // 'async' modifier cannot be used here
        | 1044 // '{0}' modifier cannot appear on a module or namespace element
        | 1047 // A rest parameter cannot be optional
        | 1048 // A rest parameter cannot have an initializer
        | 1049 // A 'set' accessor must have exactly one parameter
        | 1051 // A 'set' accessor cannot have an optional parameter
        | 1054 // A 'get' accessor cannot have parameters
        | 1070 // '{0}' modifier cannot appear on a type member
        | 1071 // An accessor must have a body (interface/ambient)
        | 1079 // A '{0}' modifier cannot be used with an import declaration
        | 1089 // '{0}' modifier cannot appear on a constructor declaration
        | 1090 // '{0}' modifier cannot appear on a parameter
        | 1091 // Only a single variable declaration is allowed in a 'for...in' statement
        | 1092 // Type parameters cannot appear on a constructor declaration
        | 1093 // Type annotation cannot appear on a constructor declaration
        | 1094 // An accessor cannot have type parameters
        | 1095 // A 'set' accessor cannot have a return type annotation
        | 1096 // An index signature must have exactly one parameter
        | 1097 // '{0}' list cannot be empty
        | 1098 // Type parameter list cannot be empty
        | 1099 // Type argument list cannot be empty
        | 1113 // A 'default' clause cannot appear more than once in a 'switch' statement
        | 1114 // Duplicate label
        | 1120 // An export assignment cannot have modifiers
        | 1123 // Variable declaration list cannot be empty
        | 1162 // An object member cannot be declared optional
        | 1163 // A 'yield' expression is only allowed in a generator body
        | 1171 // A comma expression is not allowed in a computed property name
        | 1172 // extends clause already seen
        | 1173 // extends clause must precede implements clause
        | 1174 // Classes can only extend a single class
        | 1175 // implements clause already seen
        | 1176 // Interface declaration cannot have an implements clause
        | 1182 // A destructuring declaration must have an initializer
        | 1184 // Modifiers cannot appear here
        | 1188 // Only a single variable declaration is allowed in a 'for...of' statement
        | 1492 // '{0}' declarations may not have binding patterns
        | 1493 // The left-hand side of a 'for...in' statement cannot be a 'using' declaration
        | 1494 // The left-hand side of a 'for...in' statement cannot be an 'await using' declaration
        | 1191 // An import declaration cannot have modifiers
        | 1193 // An export declaration cannot have modifiers
        | 1197 // Catch clause variable cannot have an initializer
        | 1200 // Line terminator not permitted before arrow
        | 1206 // Decorators are not valid here
        | 1210 // Code contained in a class is evaluated in strict mode
        | 1212 // Identifier expected. '{0}' is a reserved word in strict mode
        | 1213 // Identifier expected. '{0}' is a reserved word in strict mode. Class definitions are automatically in strict mode.
        | 1024 // 'readonly' modifier can only appear on a property declaration or index signature
        | 1242 // 'abstract' modifier can only appear on a class, method, or property declaration
        | 1274 // '{0}' modifier can only appear on a type parameter of a class,
                // interface or type alias. tsc's checkGrammarModifiers reports
                // this for `in`/`out` used as a PARAMETER modifier (e.g.
                // `function f(in x: number) {}`); tsz emits that shape from
                // the parser's `parameter_modifier_grammar_error`
                // (`state_statements_class.rs`). #16279 audit round 9:
                // oracle-confirmed against `typescript@7.0.2` — Direction A,
                // `function f(in x: number) {}` (and the `out` sibling)
                // alone reports TS1274 exactly once; Direction B, the same
                // line plus an unrelated real syntax error (`let x: = 1;`)
                // elsewhere in the file drops TS1274 entirely on the real
                // compiler, which tsz's parser-emitted copy did not.
                //
                // TS1274 has a SECOND, independent emission shape this list
                // does not cover: `in`/`out` used as a class member's own
                // modifier (`class C { in x }`) is checker-emitted, from
                // `check_variance_modifier_not_on_class_member_node`
                // (`class_type_param_checks.rs`) — a `CheckerDiagnostic`,
                // never a `ParseDiagnostic`, so it never reaches this list or
                // `filtered_parse_diagnostics`. That shape's suppression gap
                // is fixed separately via `is_checker_routed_ts1xxx_grammar`
                // below. No double-emission between the two: a parameter and
                // a class member are disjoint node kinds.
        | 1243 // '{0}' modifier cannot be used with '{1}' modifier
        | 1246 // An interface property cannot have an initializer
        | 1247 // A type literal property cannot have an initializer
        | 1491 // '{0}' modifier cannot appear on a 'using' declaration
        | 1495 // '{0}' modifier cannot appear on an 'await using' declaration
        | 1275 // 'accessor' modifier can only appear on a property declaration
        | 1276 // An 'accessor' property cannot be declared optional
        | 1155 // '{0}' declarations must be initialized
        | 1156 // '{0}' declarations can only be declared inside a block
        | 1313 // The body of an 'if' statement cannot be the empty statement
        | 1358 // Tagged template expressions are not permitted in an optional chain
        | 18024 // An enum member cannot be named with a private identifier
        | 18016 // Private identifiers are not allowed outside class bodies.
                // tsc's checkGrammarPrivateIdentifierExpression reports this
                // via grammarErrorOnNode (checker-side); tsz's parser emits it
                // directly for a private-identifier-keyed interface/type-literal
                // member (`state_declarations.rs`) and object-literal member
                // (`state_expressions_literals/object_members.rs`). #16279
                // audit round 8: oracle-confirmed against `typescript@7.0.2` —
                // Direction A, `interface I { #foo: number }` alone reports
                // TS18016 exactly once; Direction B, the same line plus an
                // unrelated real syntax error (`let x: = 1;`) elsewhere in the
                // file drops TS18016 entirely on the real compiler, which
                // tsz's parser-emitted copy did not. Unlisted, it also
                // silently deleted a listed sibling's diagnostics in the same
                // file (verified with `interface I { #foo: number }` next to
                // a class with a parameterless `set` accessor: tsc keeps both
                // TS18016 and TS1049/TS7032; tsz kept only TS18016).
        | 8038 // Decorators may not appear after 'export' or 'export default' if they also appear before 'export'
        | 18037 // 'await' expression cannot be used inside a class static block
        | 18041 // A 'return' statement cannot be used inside a class static block
        | 18054 // 'await using' statements cannot be used inside a class static block
        // The invalid-meta-property-name family, reported from tsc's single
        // `checkMetaProperty` (checker-side `grammarErrorOnNode`) and emitted
        // from tsz's parser instead (`state_expressions_literals.rs`, the
        // `new.<name>` / `import.<name>` sites, which pick between the two codes
        // on whether a `(` follows). It is all-in-or-all-out: TS17012 fires for
        // the non-call form (`import.foo` / `new.foo`) and TS18061 for the call
        // form (`import.foo()`), and a file can carry both at once
        // (`importDefer/importMetaPropertyInvalidInCall.ts`), so listing only
        // one lets the *other* — still counted as a suppressing "real parse
        // error" — delete the listed one. The checker's own meta-property
        // access path (`property_access_type/helpers.rs`) deliberately does NOT
        // emit either ("A separate grammar check is expected to emit TS17012"),
        // so there is no double-emission — the trap that blocked TS2499/TS18016.
        // #16279 meta-property round: oracle-confirmed against `typescript@7.0.2`
        // — Direction A, `const y = import.foo;` reports TS17012 and
        // `import.foo();` reports TS18061; Direction B, either construct plus an
        // unrelated real syntax error (`let zzz: = 1;`) drops the meta-property
        // code entirely, which tsz's parser-emitted copies did not — and,
        // unlisted, TS17012 deleted a co-occurring listed TS1054 (`get` accessor
        // with parameters) from the same file.
        | 17012 // '{0}' is not a valid meta-property for keyword '{1}'. Did you mean '{2}'?
        | 18061 // '{0}' is not a valid meta-property for keyword 'import'. Did you mean 'meta' or 'defer'?
        // #16279 audit round: JSX comma-operator family. tsc's checker
        // (`checkGrammarJsxExpression`, via `grammarErrorOnNode`) reports this
        // for a comma expression inside a JSX expression container
        // (`<div className={a, b}/>`); tsz emits it from the parser instead
        // (`state_types_jsx_elements.rs`), with no checker-side counterpart, so
        // there is no double-emission risk. Oracle-confirmed against
        // `typescript@7.0.2` — Direction A, `<div>{a, b}</div>` alone reports
        // TS18007 (alongside unrelated JSX/comma diagnostics); Direction B, the
        // same line plus an unrelated real syntax error (`let zzz: = 1;`)
        // elsewhere in the file drops TS18007 entirely on the real compiler,
        // which tsz's parser-emitted copy did not.
        | 18007 // JSX expressions may not use the comma operator. Did you mean to write an array?
        | 1326 // This use of 'import' is invalid. 'import()' calls can be written,
               // but they must have parentheses and cannot have type arguments.
               // tsc's checkGrammarImportCallExpression reports this from the
               // checker (`import<T>("m")`); tsz emits it from the parser
               // (`state_expressions_literals.rs`). #16279 audit round 6:
               // oracle-confirmed against `typescript@7.0.2` — Direction A,
               // `import<number>("mod")` alone reports TS1326 (plus the
               // unrelated TS2307 for the unresolved module specifier);
               // Direction B, the same line plus an unrelated real syntax
               // error (`let x: = 1;`) elsewhere in the file drops TS1326
               // entirely on the real compiler. Sole emission site, no
               // checker-side counterpart, so no double-emission risk.
        | 8020 // JSDoc types can only be used inside documentation comments.
               // tsc's checker reports this `TS8xxx` code via
               // `grammarErrorOnNode` for a bare `*` (JSDoc "any type") in
               // ordinary type position; tsz's parser emits it directly
               // (`state_types.rs`, `state_types_jsx.rs`), sole emission
               // site, no checker-side counterpart. #16279 audit round 11.
        | 2499 // An interface can only extend an identifier/qualified-name
               // with optional type arguments. tsc's checker rejects a
               // parenthesized or bracketed `extends` operand
               // (`interface I extends (1 + 2) {}`); tsz's parser
               // (`parse_interface_heritage_type_reference`,
               // `state_statements_class_declarations.rs`) already
               // special-cases that shape and reports TS2499 itself, at the
               // same position the checker's independent generic heritage
               // walk (`heritage.rs`) also reports it — a genuine
               // double-emission this list alone cannot fix (see
               // `post_process_checker_diagnostics`'s TS2499 position-match
               // filter for that half). Oracle-confirmed
               // (`typescript@7.0.2`) — Direction A, `interface I extends
               // (1 + 2) {}` alone reports TS2499 exactly once; Direction B,
               // the same line plus an unrelated real syntax error
               // (`let x: = 1;`) elsewhere in the file drops TS2499
               // entirely on the real compiler, which tsz's parser-emitted
               // copy did not (round 7 of the #16279 audit).
    )
}
