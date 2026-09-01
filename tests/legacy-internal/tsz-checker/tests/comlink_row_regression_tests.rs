//! Regression tests for the comlink benchmark row false positives.
//!
//! Three structural families:
//! 1. Merged var+generic-interface symbols annotated with an instantiation
//!    of their own interface (`declare var Bag: Bag<string>`) must keep the
//!    instantiated value type (comlink `FinalizationRegistry`, TS2345).
//! 2. `any`-typed object-literal property values stay `any` under contextual
//!    typing; the contextual property type must not overwrite them (comlink
//!    `toWireValue` shorthand against a discriminated union, TS2322).
//! 3. Switch-clause narrowing must work when the case label is a property
//!    access on an imported enum (comlink `fromWireValue`, TS2339).
//! 4. Object-literal method block returns must keep contextual tuple return
//!    types from generic transfer-handler interfaces (comlink proxy handler,
//!    TS2322).
//! 5. Bare generic DOM event annotations must apply declaration defaults
//!    (`MessageEvent<T = any>`), so `ev.data` remains any-like unless an
//!    explicit type argument says otherwise (comlink endpoint listener, TS2339).

use crate::context::CheckerOptions;
use crate::test_utils::{
    check_multi_file_with_libs_stamped, check_source_diagnostics, check_source_with_libs,
    load_lib_files,
};

// ---------------------------------------------------------------------------
// Family 1: merged var + generic interface, self-instantiation annotation
// ---------------------------------------------------------------------------

#[test]
fn merged_var_generic_interface_self_instantiation_uses_type_args() {
    let diags = check_source_diagnostics(
        r#"
interface Keeper<T> {
  new (cb: (heldValue: T) => void): Keeper<T>;
  register(weakItem: object, heldValue: T, unregisterToken?: object | undefined): void;
  unregister(unregisterToken: object): void;
}
declare var Keeper: Keeper<string>;

const keepers =
  "Keeper" in globalThis &&
  new Keeper((held: string) => {
    held.length;
  });

function reg(token: object, held: string) {
  if (keepers) {
    keepers.register(token, held, token);
  }
}
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2345),
        "expected no TS2345 for instantiated merged var+interface, got {diags:#?}"
    );
}

#[test]
fn merged_var_generic_interface_self_instantiation_still_rejects_bad_arg() {
    let diags = check_source_diagnostics(
        r#"
interface Pouch<T> {
  stash(heldValue: T): void;
}
declare var Pouch: Pouch<string>;
Pouch.stash(42);
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2345),
        "expected TS2345 for number against Pouch<string>.stash, got {diags:#?}"
    );
}

#[test]
fn merged_var_interface_bare_self_reference_still_resolves() {
    // The non-generic `declare var X: X` lib pattern must keep working.
    let diags = check_source_diagnostics(
        r#"
interface Gauge {
  read(): number;
}
declare var Gauge: Gauge;
const n: number = Gauge.read();
"#,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics for bare self-referential var pattern, got {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// Family 2: any-typed object literal property values stay any
// ---------------------------------------------------------------------------

#[test]
fn shorthand_any_property_assignable_to_discriminated_union() {
    let diags = check_source_diagnostics(
        r#"
const enum Tag { RAW = "RAW", BOXED = "BOXED" }
interface RawCell { tag: Tag.RAW; payload: {}; }
interface BoxedCell { tag: Tag.BOXED; label: string; payload: unknown; }
type Cell = RawCell | BoxedCell;
export function pack(payload: any): [Cell, number] {
  return [{ tag: Tag.RAW, payload }, 1];
}
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "expected no TS2322 for shorthand any property against union member, got {diags:#?}"
    );
}

#[test]
fn named_any_parameter_property_assignable_to_discriminated_union() {
    let diags = check_source_diagnostics(
        r#"
const enum Mark { A = "A", B = "B" }
interface ArmA { mark: Mark.A; data: {}; }
interface ArmB { mark: Mark.B; name: string; data: unknown; }
type Arm = ArmA | ArmB;
export function build(data: any): Arm {
  return { mark: Mark.A, data: data };
}
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "expected no TS2322 for explicit any-param property against union member, got {diags:#?}"
    );
}

#[test]
fn shorthand_concrete_mismatch_still_rejected() {
    // Negative case: a genuinely incompatible shorthand value must still fail.
    let diags = check_source_diagnostics(
        r#"
interface Slot { width: number; }
const width = "wide";
const s: Slot = { width };
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 for string shorthand against number property, got {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// Family 3: switch narrowing with qualified / imported enum case labels
// ---------------------------------------------------------------------------
//
// The comlink witness imports the enum from another module; the unit harness
// cannot model cross-file import bindings faithfully, so these tests pin the
// same structural path (case label = property access whose base is not a
// local ENUM-flagged identifier, so narrowing must read the recorded case
// expression type) through namespace-qualified enum members.

#[test]
fn switch_narrows_discriminated_union_with_qualified_const_enum_labels() {
    let diags = check_source_diagnostics(
        r#"
namespace Wire {
  export const enum Kind {
    PLAIN = "PLAIN",
    TAGGED = "TAGGED",
  }
}
interface Plain {
  kind: Wire.Kind.PLAIN;
  value: {};
}
interface Tagged {
  kind: Wire.Kind.TAGGED;
  label: string;
}
type Packet = Plain | Tagged;
export function read(pkt: Packet): any {
  switch (pkt.kind) {
    case Wire.Kind.TAGGED:
      return pkt.label;
    case Wire.Kind.PLAIN:
      return pkt.value;
  }
}
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "expected no TS2339 for qualified const enum switch narrowing, got {diags:#?}"
    );
}

#[test]
fn switch_narrows_discriminated_union_with_qualified_regular_enum_labels() {
    let diags = check_source_diagnostics(
        r#"
namespace Geo {
  export enum Form {
    DOT = "DOT",
    LINE = "LINE",
  }
}

interface Dot {
  form: Geo.Form.DOT;
  x: number;
}
interface Line {
  form: Geo.Form.LINE;
  length: number;
}
type Shape = Dot | Line;
export function measure(shape: Shape): number {
  switch (shape.form) {
    case Geo.Form.LINE:
      return shape.length;
    case Geo.Form.DOT:
      return shape.x;
  }
}
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "expected no TS2339 for qualified regular enum switch narrowing, got {diags:#?}"
    );
}

#[test]
fn switch_with_qualified_enum_label_still_rejects_wrong_arm_member() {
    let diags = check_source_diagnostics(
        r#"
namespace Power {
  export const enum Mode {
    ON = "ON",
    OFF = "OFF",
  }
}
interface On {
  mode: Power.Mode.ON;
  brightness: number;
}
interface Off {
  mode: Power.Mode.OFF;
  reason: string;
}
type State = On | Off;
export function bad(state: State): any {
  switch (state.mode) {
    case Power.Mode.ON:
      return state.reason;
  }
}
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2339),
        "expected TS2339 for member from the other arm after narrowing, got {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// Family 4: contextually typed method block returns stay tuple-shaped
// ---------------------------------------------------------------------------

#[test]
fn object_literal_method_block_return_preserves_tuple_context() {
    let diags = check_source_diagnostics(
        r#"
interface Transferable {}
interface MessagePort extends Transferable {
  start(): void;
}
declare class MessageChannel {
  port1: MessagePort;
  port2: MessagePort;
}
declare function expose(value: object, port: MessagePort): void;

interface TransferHandler<T, S> {
  serialize(value: T): [S, Transferable[]];
}

const handler: TransferHandler<object, MessagePort> = {
  serialize(value) {
    const { port1, port2 } = new MessageChannel();
    expose(value, port1);
    return [port2, [port2]];
  },
};
"#,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "expected contextual tuple return for transfer handler method, got {diags:#?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 7006),
        "expected contextual method parameter type, got {diags:#?}"
    );
}

#[test]
fn object_literal_method_block_return_preserves_tuple_context_with_generic_receiver_member() {
    let diags = check_source_diagnostics(
        r#"
interface EventTargetLike<T> {
  onmessage: ((this: T) => any) | null;
}
interface Port extends EventTargetLike<Port> {
  start(): void;
}
declare class Channel {
  port1: Port;
  port2: Port;
}
declare function expose(value: object, port: Port): void;

interface TransferHandler<T, S> {
  serialize(value: T): [S, Port[]];
}

const handler: TransferHandler<object, Port> = {
  serialize(value) {
    const { port1, port2 } = new Channel();
    expose(value, port1);
    return [port2, [port2]];
  },
};
"#,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "expected contextual tuple return through closed generic receiver member, got {diags:#?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 7006),
        "expected contextual method parameter type, got {diags:#?}"
    );
}

#[test]
fn object_literal_method_block_return_preserves_dom_tuple_context() {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    let diags = check_source_with_libs(
        r#"
declare function expose(value: object, port: MessagePort): void;

interface TransferHandler<T, S> {
  serialize(value: T): [S, Transferable[]];
}

const handler: TransferHandler<object, MessagePort> = {
  serialize(value) {
    const { port1, port2 } = new MessageChannel();
    expose(value, port1);
    return [port2, [port2]];
  },
};
export {};
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "expected DOM-backed contextual tuple return for transfer handler method, got {diags:#?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 7006),
        "expected DOM-backed contextual method parameter type, got {diags:#?}"
    );
    assert!(
        !diags.iter().any(|d| matches!(d.code, 2304 | 2583 | 2584)),
        "expected DOM-backed witness globals to resolve, got {diags:#?}"
    );
}

#[test]
fn object_literal_method_block_return_still_rejects_short_tuple() {
    let diags = check_source_diagnostics(
        r#"
interface Transferable {}
interface MessagePort extends Transferable {}
interface TransferHandler<T, S> {
  serialize(value: T): [S, Transferable[]];
}

const handler: TransferHandler<object, MessagePort> = {
  serialize(value) {
    return [value];
  },
};
"#,
    );

    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 for short tuple return, got {diags:#?}"
    );
}

#[test]
fn object_literal_method_block_return_rejects_unresolved_tuple_array_slot() {
    let diags = check_source_diagnostics(
        r#"
interface Handler<T> {
  make(): [T[], number];
}

function create<T>(): Handler<T> {
  return {
    make() {
      return [[123], 0];
    },
  };
}
"#,
    );

    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 for unresolved generic array tuple slot, got {diags:#?}"
    );
}

#[test]
fn object_literal_method_block_return_rejects_unresolved_tuple_wrapper_slot() {
    let diags = check_source_diagnostics(
        r#"
interface Wrapped<T> {
  value: T;
}
declare function wrap<T>(value: T): Wrapped<T>;
interface Handler<T> {
  make(): [Wrapped<T>, number];
}

function create<T>(): Handler<T> {
  return {
    make() {
      return [wrap(123), 0];
    },
  };
}
"#,
    );

    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 for unresolved generic wrapper tuple slot, got {diags:#?}"
    );
}

#[test]
fn object_literal_method_block_return_rejects_unresolved_tuple_object_slot() {
    let diags = check_source_diagnostics(
        r#"
interface Handler<T> {
  make(): [{ value: T }, number];
}

function create<T>(): Handler<T> {
  return {
    make() {
      return [{ value: 123 }, 0];
    },
  };
}
"#,
    );

    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 for unresolved generic object tuple slot, got {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// Family 5: bare `MessageEvent` annotations apply default `T = any`
// ---------------------------------------------------------------------------

#[test]
fn bare_message_event_annotation_defaults_data_to_any() {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    let diags = check_source_with_libs(
        r#"
function handle(ev: MessageEvent) {
  ev.data.argumentList;
  ev.data.value;
  ev.data.id;
}
export {};
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "expected bare MessageEvent to apply T = any for data, got {diags:#?}"
    );
}

#[test]
fn guarded_bare_message_event_data_keeps_default_any() {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    let diags = check_source_with_libs(
        r#"
function handle(ev: MessageEvent) {
  if (!ev || !ev.data) {
    return;
  }
  ev.data.argumentList;
  ev.data.value;
}
export {};
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "expected guarded bare MessageEvent data to keep T = any, got {diags:#?}"
    );
}

#[test]
fn endpoint_listener_callback_annotation_defaults_message_event_data_to_any() {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    let diags = check_source_with_libs(
        r#"
interface EndpointLike {
  addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: {}
  ): void;
}

function expose(ep: EndpointLike) {
  ep.addEventListener("message", function callback(ev: MessageEvent) {
    ev.data.argumentList;
    ev.data.value;
  } as any);
}
export {};
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "expected listener callback's bare MessageEvent annotation to apply T = any, got {diags:#?}"
    );
}

#[test]
fn imported_endpoint_listener_defaults_message_event_data_to_any() {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    let files = [
        (
            "protocol.ts",
            r#"
export interface EventSource {
  addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: {}
  ): void;
}

export interface Endpoint extends EventSource {
  postMessage(message: any, transfer?: Transferable[]): void;
}
"#,
        ),
        (
            "comlink.ts",
            r#"
import { Endpoint } from "./protocol";

export function expose(ep: Endpoint) {
  ep.addEventListener("message", function callback(ev: MessageEvent) {
    ev.data.argumentList;
    ev.data.value;
  } as any);
}
"#,
        ),
    ];
    let diags = check_multi_file_with_libs_stamped(
        &files,
        "comlink.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "expected imported endpoint listener's bare MessageEvent annotation to apply T = any, got {diags:#?}"
    );
}

#[test]
fn explicit_message_event_type_argument_keeps_data_shape() {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    let diags = check_source_with_libs(
        r#"
function handle(ev: MessageEvent<{ id: string }>) {
  ev.data.id.length;
  ev.data.argumentList;
}
export {};
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        diags.iter().any(|d| d.code == 2339),
        "expected explicit MessageEvent payload to reject missing property, got {diags:#?}"
    );
}

#[test]
fn bare_message_event_assertion_defaults_data_to_any() {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    let diags = check_source_with_libs(
        r#"
function handle(ev: Event) {
  const { data } = ev as MessageEvent;
  data.id;
  data.argumentList;
}
export {};
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "expected bare MessageEvent assertion to apply T = any for data, got {diags:#?}"
    );
}

#[test]
fn guarded_message_event_assertion_data_keeps_default_any() {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    let diags = check_source_with_libs(
        r#"
function handle(ev: Event) {
  const { data } = ev as MessageEvent;
  if (!data || !data.id) {
    return;
  }
  data.id;
  data.argumentList;
}
export {};
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "expected guarded bare MessageEvent assertion data to keep T = any, got {diags:#?}"
    );
}
