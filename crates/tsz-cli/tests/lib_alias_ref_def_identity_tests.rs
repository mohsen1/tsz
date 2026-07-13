//! Lib alias body member references must keep their def identity under
//! cross-arena delegation (ofetch canary, issue #15778).
//!
//! `canonical_lib_sym_id`'s largest-`SymbolId` fallback is a HEURISTIC
//! (ROBUSTNESS_AUDIT_2026-04-26 item #4): it picks a raw id by name in one
//! binder's numbering, and `get_lib_def_id` then resolves that raw id against
//! another numbering. When the two disagree, a lib alias body reference such
//! as `BodyInit`'s `XMLHttpRequestBodyInit` arm bound to a colliding def
//! (`AudioSampleFormat`), baking that type's literal union into `BodyInit` —
//! so `string` was no longer assignable to `RequestInit["body"]`.
//! `get_canonical_lib_def_id` now refuses a canonical-branch def whose
//! recorded name contradicts the requested name and falls through to the
//! name-verified election.
//!
//! These end-to-end tests drive the real `tsz` binary because the collision
//! needs the real multi-binder dom lib environment; the in-process checker
//! harness runs a cut-down lib where the numbering split cannot occur.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_lib_alias_ref_identity_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

/// Run `tsz` with the dom lib set on `source` and return combined output.
fn run_tsz_dom(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let file = temp.path.join("repro.ts");
    std::fs::write(&file, source).expect("write repro file");
    let output = Command::new(tsz_bin)
        .args([
            "repro.ts",
            "--strict",
            "--noEmit",
            "--pretty",
            "false",
            "--target",
            "es2022",
            "--lib",
            "es2022,dom,dom.iterable",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

/// The minimized ofetch witness: every declaration is load-bearing (the
/// heritage chain plus the contextually-typed generic function expression
/// drive `BodyInit`'s body computation through cross-arena delegation, where
/// the collision bound its `XMLHttpRequestBodyInit` arm to a wrong def).
/// tsc 7.0.2 reports no error; the collision made tsz emit a false TS2322.
const OFETCH_WITNESS: &str = r#"
type ResponseType = "json" | "text";
type MappedResponseType<R extends ResponseType, JsonType = any> = JsonType;
type FetchRequest = string | Request;
interface FetchResponse<T> extends Response { _data?: T; }
interface $Fetch {
  raw<T = any, R extends ResponseType = "json">(
    request: FetchRequest,
  ): Promise<FetchResponse<MappedResponseType<R, T>>>;
}
interface FetchHooks<T = any, R extends ResponseType = ResponseType> {
}
interface FetchOptions<R extends ResponseType = ResponseType, T = any>
  extends Omit<RequestInit, "body">,
    FetchHooks<T, R> {
  body?: RequestInit["body"] | Record<string, any>;
}
interface ResolvedFetchOptions<
  R extends ResponseType = ResponseType,
  T = any,
> extends FetchOptions<R, T> {
  headers: Headers;
}
interface FetchContext<T = any, R extends ResponseType = ResponseType> {
  request: FetchRequest;
  options: ResolvedFetchOptions<R>;
}
declare function isPayloadMethod(method?: string): boolean;
declare function isJSONSerializable(value: any): boolean;
declare function resolveFetchOptions<
  R extends ResponseType = ResponseType,
  T = any,
>(
  request: FetchRequest,
  options: FetchOptions<R, T> | undefined,
  defaults: FetchOptions<R, T> | undefined,
  Headers: typeof globalThis.Headers
): ResolvedFetchOptions<R, T>;
export function createFetch() {
  const $fetchRaw: $Fetch["raw"] = async function $fetchRaw<
    T = any,
    R extends ResponseType = "json",
  >(_request: FetchRequest, _options: FetchOptions<R> = {}) {
    const context: FetchContext = {
      request: _request,
      options: resolveFetchOptions<R, T>(_request, _options, undefined, Headers),
    };
    if (context.options.body && isPayloadMethod(context.options.method)) {
      if (isJSONSerializable(context.options.body)) {
        const contentType = context.options.headers.get("content-type");
        if (typeof context.options.body !== "string") {
          context.options.body =
            contentType === "application/x-www-form-urlencoded"
              ? new URLSearchParams(
                  context.options.body as Record<string, any>
                ).toString()
              : JSON.stringify(context.options.body);
        }
      }
    }
    return context as any;
  };
}
"#;

#[test]
fn string_stays_assignable_to_request_init_body_in_contextual_generic_body() {
    let Some(out) = run_tsz_dom("ofetch_witness", OFETCH_WITNESS) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS"),
        "the ofetch witness must be error-free (tsc 7.0.2 is clean); got:\n{out}"
    );
}

/// Negative control: a genuine mismatch against the same member type keeps
/// failing — the name-verified fall-through must not widen the relation.
#[test]
fn genuine_body_mismatch_still_reports_ts2322() {
    let source = r#"
export function poke(init: RequestInit) {
  init.body = 42;
}
"#;
    let Some(out) = run_tsz_dom("genuine_mismatch", source) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        out.contains("TS2322"),
        "a number is never a valid RequestInit body; got:\n{out}"
    );
}
