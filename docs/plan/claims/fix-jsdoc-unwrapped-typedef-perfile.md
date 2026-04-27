**2026-04-27 03:11:26** — `fix/jsdoc-unwrapped-typedef-perfile-20260427-0311`

Scope: TS1110 unwrapped-multiline-typedef check (`check_jsdoc_unwrapped_multiline_typedefs`)
in `crates/tsz-checker/src/jsdoc/diagnostics.rs` walks every arena's comments
but emits diagnostics through `error_at_position`, which always attributes to
the current file's name. In multi-file JS suites, this mis-routes a TS1110
emitted for a comment in `modN.js` onto `mod0.js` and rebases its byte-offset
against `mod0.js`'s text. The fix limits the check to the current file's
arena only (the same pattern used by sibling JSDoc checks), so each file
contributes its own TS1110 diagnostics with positions resolved against its
own source text.

Target failure: `tests/cases/conformance/jsdoc/typedefTagWrapping.ts` (fingerprint-only).
