//! Standard-library aliases reached through lazy interface members must expose
//! their structural bodies to relation and operation consumers.
//!
//! The custom lib keeps binder names unrelated to the project witness and
//! covers primitive aliases, alias chains, generic applications, aliases to
//! interfaces, and a module-local shadow. The producer/consumer order is
//! reversed to exercise both cross-file `DefId` layouts.

use std::sync::Arc;

use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};
use tsz_common::common::ModuleKind;

const LIB_SOURCE: &str = r#"
type ClockTick = NumericTick;
type NumericTick = number;
type CaptionTick = string;
type Parcel<T> = { value: T };

interface DisplayFace {
    label: string;
}
type DisplayAlias = DisplayFace;

interface Chronometer {
    now(): ClockTick;
    caption(): CaptionTick;
    parcel<T>(value: T): Parcel<T>;
    display(): DisplayAlias;
}

declare const chronometer: Chronometer;
"#;

const PRODUCER: &str = r#"
export const marker = 1;
"#;

const CONSUMER: &str = r#"
import { marker } from "./producer";

declare const selectClock: boolean;
declare function needsNumber(value: number): void;

void marker;
const clockValue = selectClock ? chronometer.now() : 0;
needsNumber(clockValue);
const elapsed = clockValue - 1;
const clockWrong: string = chronometer.now();

const captionOk: string = chronometer.caption();
const captionWrong: number = chronometer.caption();
const captionMath = chronometer.caption() - 1;

const boxedOk: number = chronometer.parcel(1).value;
const boxedWrong: string = chronometer.parcel(1).value;

const labelOk: string = chronometer.display().label;
const labelWrong: number = chronometer.display().label;

type ClockTick = string;
const localTick: ClockTick = "local";
const localTickWrong: number = localTick;
"#;

fn options() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::ESNext,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn diagnostics(files: &[(&str, &str)]) -> Vec<(u32, String, u32, String)> {
    let mut libs = load_lib_files(&["es5.d.ts"]);
    assert_eq!(
        libs.len(),
        1,
        "es5 lib is required for this regression test"
    );
    libs.push(Arc::new(LibFile::from_source(
        "lib.clockwork.d.ts".to_string(),
        LIB_SOURCE.to_string(),
    )));
    check_multi_file_with_libs_stamped(files, "consumer.ts", options(), &libs)
        .into_iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.file,
                diagnostic.start,
                diagnostic.message_text,
            )
        })
        .collect()
}

fn assert_expected_diagnostics(files: &[(&str, &str)], order: &str) {
    let diagnostics = diagnostics(files);
    let codes: Vec<u32> = diagnostics.iter().map(|(code, ..)| *code).collect();
    assert_eq!(
        codes,
        vec![2322, 2322, 2362, 2322, 2322, 2322],
        "primitive/generic/lib-interface aliases must materialize while genuine string and shadow mismatches remain ({order}); got {diagnostics:#?}",
    );
}

#[test]
fn lazy_lib_alias_bodies_are_materialized_in_both_root_orders() {
    assert_expected_diagnostics(
        &[("producer.ts", PRODUCER), ("consumer.ts", CONSUMER)],
        "producer first",
    );
    assert_expected_diagnostics(
        &[("consumer.ts", CONSUMER), ("producer.ts", PRODUCER)],
        "consumer first",
    );
}
