//! Overload-resolution argument collection must SEE caller-computed node
//! types (issue #13184).
//!
//! Rule under test:
//!
//! > When `obj[kSym] = rhs` (with `kSym` a `unique symbol` const) — or any
//! > assignment whose RHS type was already computed by the surrounding
//! > statement check — is followed by a read of the same reference in
//! > argument position of an overloaded call, tsc narrows the read to the
//! > assigned type while resolving the overload. tsz does the same through
//! > `FlowAnalyzer` assignment narrowing, which requires the speculative
//! > overload argument-collection pass to read the caller's `node_types`
//! > entries (an overlay layer) instead of an empty scratch map.
//!
//! Without the overlay, the flow walk for the argument cannot resolve the
//! assignment RHS (`handler.bind(this)` below), silently degrades to the
//! declared type `EventHandler<Event> | null`, and every candidate rejects
//! `null` — a false `TS2769`. Witness: msw `src/core/sse.ts` 526/537/548.
//!
//! The matrix below varies binder names, symbol-keyed vs named properties,
//! and includes negative cases where narrowing must NOT apply (read before
//! write, narrowing killed by a later `null` write).

use std::sync::{Arc, OnceLock};

use tsz_binder::lib_loader::LibFile;
use tsz_checker::test_utils::{
    check_source_with_libs_code_messages, load_lib_files, strict_checker_options,
};

fn dom_libs() -> &'static [Arc<LibFile>] {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(|| {
        load_lib_files(&[
            "es5.d.ts",
            "es2015.core.d.ts",
            "es2015.symbol.d.ts",
            "es2015.iterable.d.ts",
            "dom.d.ts",
        ])
    })
}

fn check_strict_dom(source: &str) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(source, "test.ts", strict_checker_options(), dom_libs())
}

fn ts2769_messages(source: &str) -> Vec<String> {
    check_strict_dom(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2769).then_some(message))
        .collect()
}

/// The msw `sse.ts` witness reduced: symbol-keyed element access narrowed by
/// the immediately preceding assignment, read in argument position of an
/// overloaded call whose first candidate is inference-bearing.
#[test]
fn symbol_keyed_assignment_narrows_argument_of_overloaded_call() {
    let source = r#"
type EventHandler<EventType extends Event> = (event: EventType) => any

const kOnOpen = Symbol('kOnOpen')

class Obs {
  private [kOnOpen]: EventHandler<Event> | null = null

  set onopen(handler: EventHandler<Event>) {
    this[kOnOpen] = handler.bind(this)
    this.addEventListener('open', this[kOnOpen])
  }

  public addEventListener<K extends keyof EventSourceEventMap>(
    type: K,
    listener: EventHandler<EventSourceEventMap[K]>,
  ): void
  public addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
  ): void
  public addEventListener(
    type: string,
    listener: EventHandler<MessageEvent> | EventListenerOrEventListenerObject,
  ): void {}
}
export {}
"#;
    let messages = ts2769_messages(source);
    assert!(
        messages.is_empty(),
        "assignment narrowing must survive speculative overload argument \
         collection; got TS2769: {messages:?}"
    );
}

/// Renamed binders + a named (non-symbol) private property: the rule is
/// structural, not tied to `kOnOpen`/`addEventListener` or computed keys.
#[test]
fn named_property_assignment_narrows_argument_with_renamed_binders() {
    let source = r#"
type Cb<T extends Event> = (event: T) => any

class Watcher {
  private stored: Cb<Event> | null = null

  set onping(fn: Cb<Event>) {
    this.stored = fn.bind(this)
    this.listen('ping', this.stored)
  }

  public listen<Q extends keyof EventSourceEventMap>(
    kind: Q,
    callback: Cb<EventSourceEventMap[Q]>,
  ): void
  public listen(kind: string, callback: EventListenerOrEventListenerObject): void
  public listen(
    kind: string,
    callback: Cb<MessageEvent> | EventListenerOrEventListenerObject,
  ): void {}
}
export {}
"#;
    let messages = ts2769_messages(source);
    assert!(
        messages.is_empty(),
        "named-property assignment narrowing must survive overload argument \
         collection; got TS2769: {messages:?}"
    );
}

/// Local variable form: `let slot: H | null = null; slot = handler.bind(t)`
/// followed by an overloaded call reading `slot`.
#[test]
fn local_variable_assignment_narrows_argument_of_overloaded_call() {
    let source = r#"
type EventHandler<EventType extends Event> = (event: EventType) => any

declare function on<K extends keyof EventSourceEventMap>(
  type: K,
  listener: EventHandler<EventSourceEventMap[K]>,
): void
declare function on(type: string, listener: EventListenerOrEventListenerObject): void

function setup(handler: EventHandler<Event>, target: object) {
  let slot: EventHandler<Event> | null = null
  slot = handler.bind(target)
  on('open', slot)
}
export {}
"#;
    let messages = ts2769_messages(source);
    assert!(
        messages.is_empty(),
        "local-variable assignment narrowing must survive overload argument \
         collection; got TS2769: {messages:?}"
    );
}

/// Negative: a later `null` write kills the narrowing — both tsc and tsz
/// report TS2769 because `null` matches no candidate.
#[test]
fn null_write_after_assignment_still_rejects_overloads() {
    let source = r#"
type EventHandler<EventType extends Event> = (event: EventType) => any

const kOnOpen = Symbol('kOnOpen')

class Obs {
  private [kOnOpen]: EventHandler<Event> | null = null

  set onopen(handler: EventHandler<Event>) {
    this[kOnOpen] = handler.bind(this)
    this[kOnOpen] = null
    this.addEventListener('open', this[kOnOpen])
  }

  public addEventListener<K extends keyof EventSourceEventMap>(
    type: K,
    listener: EventHandler<EventSourceEventMap[K]>,
  ): void
  public addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
  ): void
  public addEventListener(
    type: string,
    listener: EventHandler<MessageEvent> | EventListenerOrEventListenerObject,
  ): void {}
}
export {}
"#;
    let messages = ts2769_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "narrowing killed by a later null write must still fail overload \
         resolution exactly once; got: {messages:?}"
    );
}

/// Negative: reading the property BEFORE the assignment keeps the declared
/// union, so the overloaded call must fail in both compilers.
#[test]
fn read_before_write_keeps_declared_type_and_rejects_overloads() {
    let source = r#"
type EventHandler<EventType extends Event> = (event: EventType) => any

const kOnOpen = Symbol('kOnOpen')

class Obs {
  private [kOnOpen]: EventHandler<Event> | null = null

  set onopen(handler: EventHandler<Event>) {
    this.addEventListener('open', this[kOnOpen])
    this[kOnOpen] = handler.bind(this)
  }

  public addEventListener<K extends keyof EventSourceEventMap>(
    type: K,
    listener: EventHandler<EventSourceEventMap[K]>,
  ): void
  public addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
  ): void
  public addEventListener(
    type: string,
    listener: EventHandler<MessageEvent> | EventListenerOrEventListenerObject,
  ): void {}
}
export {}
"#;
    let messages = ts2769_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "a read before the write must keep the declared `| null` type and \
         fail overload resolution exactly once; got: {messages:?}"
    );
}

/// Concrete (non-inference-bearing) overload variant stays clean too: the
/// first candidate takes the concrete handler type directly.
#[test]
fn concrete_overload_variant_stays_clean() {
    let source = r#"
type EventHandler<EventType extends Event> = (event: EventType) => any

const kOnMessage = Symbol('kOnMessage')

class Obs {
  private [kOnMessage]: EventHandler<MessageEvent> | null = null

  set onmessage(handler: EventHandler<MessageEvent>) {
    this[kOnMessage] = handler.bind(this)
    this.addEventListener('message', this[kOnMessage])
  }

  public addEventListener(
    type: string,
    listener: EventHandler<MessageEvent>,
  ): void
  public addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
  ): void
  public addEventListener(
    type: string,
    listener: EventHandler<MessageEvent> | EventListenerOrEventListenerObject,
  ): void {}
}
export {}
"#;
    let messages = ts2769_messages(source);
    assert!(
        messages.is_empty(),
        "concrete overload variant must accept the narrowed handler; got \
         TS2769: {messages:?}"
    );
}
