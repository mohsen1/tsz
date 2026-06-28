//! Regression tests for issue #14740.
//!
//! A function whose declared return type is a lib distributive-conditional
//! utility alias (`Exclude<T | undefined, undefined>`) — or the `NonNullable`
//! variant — must resolve to its reduced body when the consumer module is
//! type-checked BEFORE (or instead of) the producer module that declares the
//! function. Before the fix, the cross-arena `Application(Lazy(lib_alias_def),
//! args)` lost its conditional body and degraded to `unknown`, so a member
//! access on the call result reported a false `TS2571` (`Object is of type
//! 'unknown'`).
//!
//! Binder names are deliberately varied away from the issue's `getWidget`/
//! `Widget` so the fix cannot key on any user identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check_consumer_first(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    tsz_checker::test_utils::check_multi_file_with_libs(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

/// `Exclude<Shape | undefined, undefined>` return type, consumer checked first.
#[test]
fn exclude_undefined_return_member_access_consumer_first_is_clean() {
    let producer = r#"
export type Gadget = { ping(n: number): void };
export function pickGadget(): Exclude<Gadget | undefined, undefined> {
    return undefined as any;
}
"#;
    let consumer = r#"
import { pickGadget } from "../zservices/gadgets";
export function runner() {
    pickGadget().ping(1);
}
"#;

    let diagnostics = check_consumer_first(
        &[
            ("acore/runner.ts", consumer),
            ("zservices/gadgets.ts", producer),
        ],
        "acore/runner.ts",
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        !codes.contains(&2571),
        "Exclude<...> return type must reduce to its conditional body cross-module \
         (no TS2571 'Object is of type unknown'); got {diagnostics:#?}"
    );
    assert!(
        !codes.contains(&2322) && !codes.contains(&7006),
        "no downstream degradation diagnostics expected; got {diagnostics:#?}"
    );
}

/// `NonNullable<Shape | undefined>` variant, consumer checked first.
#[test]
fn non_nullable_return_member_access_consumer_first_is_clean() {
    let producer = r#"
export type Lever = { yank(n: number): void };
export function grabLever(): NonNullable<Lever | undefined> {
    return undefined as any;
}
"#;
    let consumer = r#"
import { grabLever } from "../zservices/levers";
export function operate() {
    grabLever().yank(2);
}
"#;

    let diagnostics = check_consumer_first(
        &[
            ("acore/operate.ts", consumer),
            ("zservices/levers.ts", producer),
        ],
        "acore/operate.ts",
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        !codes.contains(&2571),
        "NonNullable<...> return type must reduce cross-module (no TS2571); got {diagnostics:#?}"
    );
}

/// Control: a user-defined distributive conditional with identical semantics
/// is already order-independent and must stay clean (guards against the fix
/// regressing the working path).
#[test]
fn user_conditional_return_member_access_consumer_first_is_clean() {
    let producer = r#"
type DropUndef<T> = T extends undefined ? never : T;
export type Hatch = { open(n: number): void };
export function findHatch(): DropUndef<Hatch | undefined> {
    return undefined as any;
}
"#;
    let consumer = r#"
import { findHatch } from "../zservices/hatches";
export function unlock() {
    findHatch().open(3);
}
"#;

    let diagnostics = check_consumer_first(
        &[
            ("acore/unlock.ts", consumer),
            ("zservices/hatches.ts", producer),
        ],
        "acore/unlock.ts",
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        !codes.contains(&2571),
        "user distributive conditional control must stay clean; got {diagnostics:#?}"
    );
}
