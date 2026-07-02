//! Parity guard: an interface whose heritage base is a *generic* mapped-type
//! application (`Pick<T, K>`, `Omit<T, K>`, `Record<K, V>` with a type-parameter
//! key) is not a valid interface base type. `tsc` rejects it with TS2312
//! ("An interface can only extend an object type ... with statically known
//! members") and, crucially, does NOT additionally run the structural TS2430
//! "incorrectly extends" comparison against it — `getBaseTypes` excludes an
//! invalid base before heritage assignability runs (`isValidBaseType`).
//!
//! tsz reported TS2312 from `state_checking::heritage` but reached the TS2430
//! comparison in `class_checker_compat` through a *different* evaluation entry,
//! so it spuriously emitted TS2430 on top of (or instead of) TS2312 whenever the
//! deriving interface declared its own members. This is one witness of the
//! spurious-TS2430 heritage-variance false-positive family (io-ts, kysely).
//!
//! The fix gates the interface-heritage TS2430 comparison on the same validity
//! predicate the TS2312 site uses, so an invalid mapped base is owned by TS2312
//! alone. These cases pin: (1) no spurious TS2430 on generic mapped bases,
//! (2) legitimate TS2430 for real member/index incompatibilities is preserved,
//! (3) valid heritage stays clean. Binder names are varied across cases per the
//! anti-hardcoding contract so the guard stays structural, not name-scoped.

use std::sync::Arc;
use std::sync::OnceLock;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_source_with_libs, diagnostic_codes, has_diagnostic_code, load_default_lib_files,
};
use tsz_common::ModuleKind;

fn libs() -> &'static [Arc<LibFile>] {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(load_default_lib_files)
}

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2020,
        module: ModuleKind::CommonJS,
        no_lib: false,
        ..Default::default()
    }
}

fn check(src: &str) -> Vec<Diagnostic> {
    check_source_with_libs(src, "main.ts", opts(), libs())
}

fn assert_no_code(diags: &[Diagnostic], code: u32, context: &str) {
    assert!(
        !has_diagnostic_code(diags, code),
        "{context}: expected NO TS{code}, got {:?}\n{}",
        diagnostic_codes(diags),
        diags
            .iter()
            .map(|d| format!("  TS{} {}", d.code, d.message_text))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

fn assert_has_code(diags: &[Diagnostic], code: u32, context: &str) {
    assert!(
        has_diagnostic_code(diags, code),
        "{context}: expected TS{code}, got {:?}",
        diagnostic_codes(diags),
    );
}

// ─── Generic mapped bases: TS2312 owns them, no spurious TS2430 ──────────────

/// `Pick<Shape, P>` with a type-parameter key `P` is an invalid base: TS2312,
/// never TS2430 — even though the deriving interface adds its own member.
#[test]
fn pick_generic_key_base_reports_ts2312_not_ts2430() {
    let diags = check(
        r#"
interface Shape { width: number; height: number; depth: boolean }
interface Slice<P extends keyof Shape> extends Pick<Shape, P> { label: number }
"#,
    );
    assert_no_code(&diags, 2430, "Pick<Shape, P> heritage base");
    assert_has_code(&diags, 2312, "Pick<Shape, P> heritage base");
}

/// `Omit<Entry, Q>` (a nested alias application over a mapped type) must not
/// produce the spurious TS2430 the structural comparison used to emit.
#[test]
fn omit_generic_key_base_does_not_report_ts2430() {
    let diags = check(
        r#"
interface Entry { id: number; name: string; active: boolean }
interface Trimmed<Q extends keyof Entry> extends Omit<Entry, Q> { extra: number }
"#,
    );
    assert_no_code(&diags, 2430, "Omit<Entry, Q> heritage base");
}

/// `Record<Key, number>` with a type-parameter key domain is invalid: TS2312,
/// never TS2430.
#[test]
fn record_generic_key_base_reports_ts2312_not_ts2430() {
    let diags = check(
        r#"
interface Bag<Key extends string> extends Record<Key, number> { count: boolean }
"#,
    );
    assert_no_code(&diags, 2430, "Record<Key, number> heritage base");
    assert_has_code(&diags, 2312, "Record<Key, number> heritage base");
}

/// `Partial<T>` over a bare parameter is a generic mapped base: TS2312, no
/// TS2430.
#[test]
fn partial_generic_base_reports_ts2312_not_ts2430() {
    let diags = check(
        r#"
interface Wrapper<T> extends Partial<T> { own: number }
"#,
    );
    assert_no_code(&diags, 2430, "Partial<T> heritage base");
    assert_has_code(&diags, 2312, "Partial<T> heritage base");
}

/// A generic mapped base combined with a valid object base: the invalid mapped
/// base contributes only TS2312, and the valid base does not trigger a spurious
/// TS2430 either.
#[test]
fn generic_mapped_base_alongside_valid_base_reports_ts2312_not_ts2430() {
    let diags = check(
        r#"
interface Fields { alpha: number; beta: string }
interface Marker { mark: symbol }
interface Combined<F extends keyof Fields> extends Pick<Fields, F>, Marker { own: number }
"#,
    );
    assert_no_code(&diags, 2430, "Pick<Fields, F> + Marker heritage");
    assert_has_code(&diags, 2312, "Pick<Fields, F> + Marker heritage");
}

// ─── Legitimate TS2430 must be preserved ─────────────────────────────────────

/// A concrete base whose property the derived interface re-declares
/// incompatibly still produces TS2430.
#[test]
fn incompatible_property_override_still_reports_ts2430() {
    let diags = check(
        r#"
interface Holder { amount: number }
interface BadHolder extends Holder { amount: string }
"#,
    );
    assert_has_code(&diags, 2430, "incompatible property override");
}

/// A generic interface base applied with a *concrete* argument is a valid base,
/// so an incompatible member override still produces TS2430.
#[test]
fn incompatible_override_of_concrete_generic_base_still_reports_ts2430() {
    let diags = check(
        r#"
interface Cell<V> { content: V }
interface BadCell extends Cell<number> { content: string }
"#,
    );
    assert_has_code(&diags, 2430, "incompatible override of Cell<number>");
}

/// Incompatible index signatures between a valid base and the derived interface
/// still produce TS2430.
#[test]
fn incompatible_index_signature_still_reports_ts2430() {
    let diags = check(
        r#"
interface StrMap { [k: string]: string }
interface NumMap extends StrMap { [k: string]: number }
"#,
    );
    assert_has_code(&diags, 2430, "incompatible index signature");
}

// ─── Valid heritage stays clean ──────────────────────────────────────────────

/// `Pick<Book, "title">` with a *concrete* key is a valid base with statically
/// known members: no diagnostic.
#[test]
fn concrete_pick_base_is_valid_and_clean() {
    let diags = check(
        r#"
interface Book { title: string; pages: number }
interface TitleOnly extends Pick<Book, "title"> { note: number }
"#,
    );
    assert_no_code(&diags, 2312, "Pick<Book, \"title\"> concrete base");
    assert_no_code(&diags, 2430, "Pick<Book, \"title\"> concrete base");
}

/// A generic interface base parameterised by the deriving interface's own type
/// parameter is valid: no TS2312/TS2430.
#[test]
fn generic_interface_base_is_valid_and_clean() {
    let diags = check(
        r#"
interface Box<T> { item: T }
interface LabeledBox<T> extends Box<T> { label: number }
"#,
    );
    assert_no_code(&diags, 2312, "Box<T> generic interface base");
    assert_no_code(&diags, 2430, "Box<T> generic interface base");
}

/// A plain compatible extension adds a new member with no conflict: clean.
#[test]
fn plain_compatible_extension_is_clean() {
    let diags = check(
        r#"
interface Alpha { x: number }
interface Beta extends Alpha { y: string }
"#,
    );
    assert_no_code(&diags, 2312, "plain compatible extension");
    assert_no_code(&diags, 2430, "plain compatible extension");
}
