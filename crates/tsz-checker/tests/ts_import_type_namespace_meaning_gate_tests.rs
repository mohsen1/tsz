//! A qualified TS-syntax type-position reference (`import(...).A.B[.C…]`)
//! requires every segment but the last to resolve in *namespace meaning* — a
//! real module/namespace/enum symbol whose own export table (not its
//! instance member table) supplies the next segment. `tsc` rejects a
//! class/interface/type-alias head as an ineligible qualifier and fails **at
//! that segment**, blamed against the namespace formed by the segments
//! already validated before it (never including the rejected segment
//! itself) — it never walks into the head's own member table looking for
//! the tail. Oracle-verified against pinned `typescript@7.0.2`:
//!
//!   // mod.ts: export class C { `s()`: void {} }
//!   type Y = import('./mod').C.Inner;
//!   // tsc: Namespace '"/abs/path/to/mod"' has no exported member 'C'.
//!   //      (not `Namespace '"mod".C' has no exported member 'Inner'`)
//!
//! An enum head is the interesting negative case: unlike class/interface/
//! type-alias, `SymbolFlags.Enum` is part of tsc's own `SymbolFlags.Namespace`
//! grouping, so an enum head walks like a namespace (qualified correctly by
//! itself when its own member is missing, not rejected at the enum segment).
//!
//! Owner: `crates/tsz-checker/src/state/type_resolution/import_type.rs`
//! (`resolve_import_type_reference`, `import_type_missing_member_context`,
//! `resolve_import_type_target_symbol` — three duplicated walks over the
//! same qualified-segment shape, all gated by `symbol_flags::NAMESPACE` here).
//! Mirrors the identical JSDoc-path structural fix for #17181.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS2694: u32 = 2694;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn only_ts2694(diags: &[(u32, String)]) -> &str {
    let matches: Vec<_> = diags.iter().filter(|(c, _)| *c == TS2694).collect();
    assert_eq!(matches.len(), 1, "expected exactly one TS2694: {diags:?}");
    &matches[0].1
}

/// Like `check`, but also returns each diagnostic's byte-offset `start` so the
/// TS2694 *anchor* can be asserted, not just its message. `tsc` anchors TS2694
/// on the segment it blames; the anchor and the message must agree.
fn check_with_start(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String, u32)> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text, d.start))
    .collect()
}

/// The single TS2694 diagnostic's `(message, start_byte_offset)`.
fn only_ts2694_with_start(diags: &[(u32, String, u32)]) -> (&str, u32) {
    let matches: Vec<_> = diags.iter().filter(|(c, _, _)| *c == TS2694).collect();
    assert_eq!(matches.len(), 1, "expected exactly one TS2694: {diags:?}");
    (matches[0].1.as_str(), matches[0].2)
}

/// Class head: `C` is not a legal qualifier for `.Inner`. tsc blames `C`
/// itself, unqualified — not `Inner` qualified as `"mod".C`.
#[test]
fn class_head_rejected_as_qualifier_blames_class_segment_unqualified() {
    let diags = check(
        &[
            ("mod.ts", "export class C {\n  s(): void {}\n}\n"),
            ("test.ts", "type Y = import(\"./mod\").C.Inner;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"mod\"' has no exported member 'C'."
    );
}

/// False-negative regression guard: a class whose own instance member table
/// happens to contain a name matching the tail segment must NOT silently
/// resolve into it — tsc rejects the class head regardless of what it
/// contains, since a class is never namespace-meaning-eligible.
#[test]
fn class_head_member_name_collision_with_tail_segment_still_rejected() {
    let diags = check(
        &[
            ("mod.ts", "export class C {\n  Inner(): void {}\n}\n"),
            (
                "test.ts",
                "type Y = import(\"./mod\").C.Inner;\ndeclare const y: Y;\n",
            ),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"mod\"' has no exported member 'C'."
    );
}

/// Interface head: same rejection as a class head.
#[test]
fn interface_head_rejected_as_qualifier_blames_interface_segment_unqualified() {
    let diags = check(
        &[
            ("mod.ts", "export interface I {\n  s(): void;\n}\n"),
            ("test.ts", "type Y = import(\"./mod\").I.Missing;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"mod\"' has no exported member 'I'."
    );
}

/// Type-alias head: same rejection.
#[test]
fn type_alias_head_rejected_as_qualifier_blames_alias_segment_unqualified() {
    let diags = check(
        &[
            ("mod.ts", "export type Alias = { s(): void };\n"),
            ("test.ts", "type Y = import(\"./mod\").Alias.Missing;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"mod\"' has no exported member 'Alias'."
    );
}

/// Enum head is namespace-meaning-eligible in tsc (`SymbolFlags.Enum` is
/// part of `SymbolFlags.Namespace`): the walk proceeds past `E` and blames
/// the missing tail segment, qualified as `"mod".E` — the class/interface/
/// alias rejection above must NOT apply to enums.
#[test]
fn enum_head_is_namespace_meaning_eligible_blames_missing_tail_qualified() {
    let diags = check(
        &[
            ("mod.ts", "export enum E {\n  A,\n  B,\n}\n"),
            ("test.ts", "type Y = import(\"./mod\").E.Missing;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"mod\".E' has no exported member 'Missing'."
    );
}

/// Namespace head with a real nested member: positive control, must resolve
/// cleanly with no TS2694 at all.
#[test]
fn namespace_head_with_real_nested_member_resolves_cleanly() {
    let diags = check(
        &[
            (
                "mod.ts",
                "export namespace NS {\n  export interface Inner {}\n}\n",
            ),
            ("test.ts", "type Y = import(\"./mod\").NS.Inner;\n"),
        ],
        "test.ts",
    );
    assert!(
        diags.iter().all(|(c, _)| *c != TS2694),
        "expected no TS2694: {diags:?}"
    );
}

/// Namespace head with a missing nested member: the walk proceeds past `NS`
/// (it is namespace-meaning-eligible) and blames the missing tail, qualified
/// as `"mod".NS` — unaffected by this fix, kept as a regression guard.
#[test]
fn namespace_head_with_missing_nested_member_blames_tail_qualified() {
    let diags = check(
        &[
            (
                "mod.ts",
                "export namespace NS {\n  export interface Inner {}\n}\n",
            ),
            ("test.ts", "type Y = import(\"./mod\").NS.Missing;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"mod\".NS' has no exported member 'Missing'."
    );
}

// =========================================================================
// Anchor parity (#17191 / #17192): `tsc` anchors TS2694 on the segment it
// blames. `type Y = import("./mod").X.Y;` lays the qualifier out as:
//   byte 25 = `X` (head)   byte 27 = the tail after a single-char head+`.`
// so a head-blamed reference must anchor at 25 and a tail-blamed one past it.
// A rejected non-namespace head (class/interface/type-alias) blames AND
// anchors the head; enum/namespace heads walk through and blame/anchor the
// tail. Regression guard: before the fix the anchor was always the tail.
// =========================================================================

/// Class head: message blames `C`, anchor lands on `C` (byte 25), not `Inner`.
#[test]
fn class_head_anchors_at_head_segment_matching_the_blamed_message() {
    let diags = check_with_start(
        &[
            ("mod.ts", "export class C {\n  s(): void {}\n}\n"),
            ("test.ts", "type Y = import(\"./mod\").C.Inner;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694_with_start(&diags),
        ("Namespace '\"mod\"' has no exported member 'C'.", 25),
    );
}

/// Interface head: same — blame and anchor are both the head segment.
#[test]
fn interface_head_anchors_at_head_segment() {
    let diags = check_with_start(
        &[
            ("mod.ts", "export interface I {\n  s(): void;\n}\n"),
            ("test.ts", "type Y = import(\"./mod\").I.Missing;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694_with_start(&diags),
        ("Namespace '\"mod\"' has no exported member 'I'.", 25),
    );
}

/// Type-alias head: blame and anchor are both the head segment.
#[test]
fn type_alias_head_anchors_at_head_segment() {
    let diags = check_with_start(
        &[
            ("mod.ts", "export type Alias = { s(): void };\n"),
            ("test.ts", "type Y = import(\"./mod\").Alias.Missing;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694_with_start(&diags),
        ("Namespace '\"mod\"' has no exported member 'Alias'.", 25),
    );
}

/// Enum head is namespace-meaning-eligible: the blamed segment genuinely is
/// the tail, so the anchor stays on the tail (byte 27). Regression guard that
/// the head-anchoring fix does not drag enum/namespace anchors to the head.
#[test]
fn enum_head_anchors_at_tail_segment() {
    let diags = check_with_start(
        &[
            ("mod.ts", "export enum E {\n  A,\n  B,\n}\n"),
            ("test.ts", "type Y = import(\"./mod\").E.Missing;\n"),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694_with_start(&diags),
        (
            "Namespace '\"mod\".E' has no exported member 'Missing'.",
            27
        ),
    );
}

/// Namespace head with a missing nested member: blame and anchor are the tail.
#[test]
fn namespace_head_anchors_at_tail_segment() {
    let diags = check_with_start(
        &[
            (
                "mod.ts",
                "export namespace NS {\n  export interface Inner {}\n}\n",
            ),
            ("test.ts", "type Y = import(\"./mod\").NS.Missing;\n"),
        ],
        "test.ts",
    );
    // `NS` occupies bytes 25-26, `.` 27, `Missing` starts at byte 28.
    assert_eq!(
        only_ts2694_with_start(&diags),
        (
            "Namespace '\"mod\".NS' has no exported member 'Missing'.",
            28
        ),
    );
}

/// 3-segment chain rejected in the MIDDLE: `NS` (namespace) walks through, but
/// `NS.Cls` is a class and so is an ineligible qualifier for `.Inner`. tsc
/// blames `Cls` qualified as `"mod".NS` and anchors on `Cls` — the middle
/// segment, neither the head nor the tail.
#[test]
fn middle_segment_rejection_blames_and_anchors_the_middle_segment() {
    let diags = check_with_start(
        &[
            (
                "mod.ts",
                "export namespace NS {\n  export class Cls {\n    m(): void {}\n  }\n}\n",
            ),
            ("test.ts", "type Y = import(\"./mod\").NS.Cls.Inner;\n"),
        ],
        "test.ts",
    );
    // `NS` bytes 25-26, `.` 27, `Cls` starts at byte 28.
    assert_eq!(
        only_ts2694_with_start(&diags),
        ("Namespace '\"mod\".NS' has no exported member 'Cls'.", 28),
    );
}

/// Renamed binders: the class-head rejection is a structural symbol-flags
/// check, not a name-string check — must hold under a renamed export and a
/// renamed local qualifier reference.
#[test]
fn class_head_rejection_holds_under_renamed_binders() {
    let diags = check(
        &[
            (
                "mod.ts",
                "class Widget {\n  render(): void {}\n}\nexport { Widget as ExportedWidget };\n",
            ),
            (
                "test.ts",
                "type Y = import(\"./mod\").ExportedWidget.Sub;\n",
            ),
        ],
        "test.ts",
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"mod\"' has no exported member 'ExportedWidget'."
    );
}
