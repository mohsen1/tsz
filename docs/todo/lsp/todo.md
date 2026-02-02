# LSP Postponed Tasks

Items identified during LSP test improvement work that require significant architectural changes or are otherwise deferred for later.

## High Impact (100+ tests)

### goToDefinition cross-file resolution
- **Tests**: 0/175 passing
- **Issue**: Needs cross-file symbol resolution, project-wide name lookup, and proper metadata for identifier spans
- **Blocked on**: Multi-file project support in the checker/binder

### quickInfo displayParts structured tokens
- **Tests**: ~114 failing due to displayParts format
- **Issue**: Server returns flat text `[{"text": "...", "kind": "text"}]` but tests expect structured tokens with separate `keyword`, `punctuation`, `className`, `parameterName` etc. parts
- **Blocked on**: Refactoring the type display to emit structured display parts instead of plain strings

## Medium Impact (10-30 tests)

### signatureHelp unterminated templates
- **Tests**: ~10 failing (signatureHelpTaggedTemplates2, signatureHelpTaggedTemplates7, signatureHelpTaggedTemplatesIncomplete2-9)
- **Issue**: Parser does not produce valid `TAGGED_TEMPLATE_EXPRESSION` nodes for unterminated template literals (missing closing backtick)
- **Blocked on**: Parser error recovery for incomplete templates

### signatureHelp overloaded tag functions
- **Tests**: 9 failing (signatureHelpTaggedTemplatesWithOverloadedTags1-9)
- **Issue**: Needs function overload resolution - detecting multiple `function f(...)` signatures and returning all as separate signature items
- **Blocked on**: Overload resolution in the checker

### indentation calculation
- **Tests**: 3/18 passing
- **Issue**: Indentation values are systematically wrong (e.g., expected 0 actual 4, expected 8 actual 4). The nesting depth calculation doesn't account for all node types correctly
- **Blocked on**: Understanding and fixing the SmartIndenter AST walk

## Lower Impact (<10 tests)

### JSX-aware toggle line comment
- **Tests**: ~6 failing
- **Issue**: JSX regions need `{/* */}` style comments instead of `//`
- **Blocked on**: JSX node detection in the comment toggle handler

### toggleMultilineComment complex overlaps
- **Tests**: 5 failing (tests 2, 3, 4, 5, 6)
- **Issue**: Complex cases where selection partially overlaps existing `/* */` comments, or involves JSX `{/* */}` syntax
- **Blocked on**: More sophisticated comment boundary detection and JSX awareness

### getEmitOutput
- **Tests**: 0/30 passing
- **Issue**: All tests create new baselines, suggesting the emit output format doesn't match expectations
- **Blocked on**: Emit/transpile pipeline implementation

## Notes

- Current test pass rate: 757/6563 (11.5%) as of last run
- Branch: `claude/lsp-fix-tests-5yvoE`
- docComment tests: 19/33 passing (57.6%) - some template insertion failures remain
- format tests: 41/234 passing (17.5%) - mostly formatting output mismatches
- completionList tests: 87/307 passing (28.3%)
- signatureHelp tests: 43/121 passing (35.5%)
