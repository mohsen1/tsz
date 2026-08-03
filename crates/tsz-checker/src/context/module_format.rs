//! Per-file CommonJS/ESM output format for the current file.
//!
//! Under the Node module kinds (`node16`, `node18`, `node20`, `nodenext`)
//! `tsc` decides several grammar questions per *file* rather than per
//! program: the file's implied format — from its extension, or from the
//! nearest `package.json`'s `"type"` field when the extension is ambiguous —
//! selects the diagnostic. `import.meta` (TS1470) and top-level `await`
//! (TS1309) both ask this same question, so the predicate lives here rather
//! than being restated at each site.

use super::CheckerContext;

impl CheckerContext<'_> {
    /// Whether the current file builds into `CommonJS` output.
    ///
    /// `.cts`/`.cjs` force `CommonJS` and `.mts`/`.mjs` force ESM regardless of
    /// any `package.json`; an ambiguous `.ts`/`.js` extension defers to the
    /// driver-resolved format (`file_is_esm`), which mirrors `tsc`'s
    /// `impliedNodeFormat`. An unresolved format (`file_is_esm == None`, the
    /// single-file case with no project context) is not treated as `CommonJS`:
    /// `tsc` only reaches the `CommonJS` arm when the program actually
    /// resolved the file to that format.
    ///
    /// Callers are responsible for checking that the module kind is a Node
    /// one — this answers the format question alone.
    pub(crate) fn current_file_builds_to_commonjs(&self) -> bool {
        let file_name = &self.file_name;
        if file_name.ends_with(".cts") || file_name.ends_with(".cjs") {
            return true;
        }
        if file_name.ends_with(".mts") || file_name.ends_with(".mjs") {
            return false;
        }
        self.file_is_esm == Some(false)
    }
}
