#!/usr/bin/env python3
"""tszd -- a NATIVE (cargo-free) rust-analyzer diagnostics daemon for tsz.

One daemon per worktree. It launches rust-analyzer once, pays the indexing
cost once, and then answers diagnostic queries in ~1-2s over a Unix socket --
without ever running `cargo check` (checkOnSave is permanently OFF).

Why this exact design (measured in the rust-lsp-agent-env harness, v0-v9):
  * Native preflight on tsz: solve-safe, fastest turns, real cargo checks 4->1.
  * Flycheck-backed preflight (cargo under LSP): 6.4x slower turns. Never.
  * Hard budgets: lost solves. The gate NEVER refuses a clean check.

Non-obvious constraints baked in (each one was a real bug found by agent
trials, not code review -- do not "simplify" them away):
  1. rust-analyzer publishes NATIVE diagnostics only for OPEN documents; the
     daemon opens the dirty-cone crates' sources or errors stay invisible.
  2. didOpen during workspace loading is silently dropped; open AFTER load.
  3. rust-analyzer only re-publishes for documents it sees CHANGE; unchanged
     open docs get a version-bump re-sync per query.
  4. A file reverted to HEAD keeps a stale editor overlay; previously synced
     files get one final re-sync.
  5. `unresolved-method` & friends live behind diagnostics.experimental.
  6. The daemon's own startup cargo (build scripts / proc macros) uses an
     isolated target dir (.target/tszd) so it never contends with the
     agent's cargo on the repo-pinned .target.

Protocol: one JSON object per line per connection over the Unix socket at
.tsz-ra/daemon.sock. Commands: ping, diag, scope, stats, shutdown.
State dir: .tsz-ra/ (gitignored): daemon.json, daemon.log, scope.json,
events.jsonl (written by the cargo gate, summarized by `ra stats`).
"""

from __future__ import annotations

import hashlib
import json
import os
import queue
import shutil
import socket
import subprocess
import sys
import threading
import time
from collections import deque
from pathlib import Path

STATE_DIR = ".tsz-ra"
# Memory reality check (measured): rust-analyzer on tsz with build scripts,
# proc macros and one crate's sources open is ~10.5 GB RSS. Shut down after a
# short idle window so an abandoned session never squats on that much RAM.
IDLE_SHUTDOWN_S = 30 * 60  # no requests for 30min -> exit
WARM_DIAG_WAIT = 2.0           # quiescence after per-query re-sync
INIT_TIMEOUT = 180.0
REQUEST_TIMEOUT = 90.0
# Diagnostics-affecting non-.rs inputs (cache key must include them).
IMPORTANT_INPUTS = ("Cargo.toml", "Cargo.lock", "build.rs", "config.toml",
                    "rust-toolchain", "rust-toolchain.toml")
SEVERITY = {1: "error", 2: "warning", 3: "information", 4: "hint"}


def log(ws: Path, msg: str) -> None:
    try:
        p = ws / STATE_DIR / "daemon.log"
        p.parent.mkdir(exist_ok=True)
        with open(p, "a", encoding="utf-8") as fh:
            fh.write(f"{time.strftime('%Y-%m-%dT%H:%M:%S')} {msg}\n")
    except OSError:
        pass


def find_rust_analyzer() -> str | None:
    try:
        proc = subprocess.run(["rustup", "which", "rust-analyzer"],
                              capture_output=True, text=True, timeout=15)
        cand = proc.stdout.strip()
        if proc.returncode == 0 and cand and Path(cand).exists():
            return cand
    except (OSError, subprocess.SubprocessError):
        pass
    return shutil.which("rust-analyzer")


def path_to_uri(p: Path) -> str:
    return "file://" + str(p)


def uri_to_rel(uri: str, ws: Path) -> str:
    raw = uri.removeprefix("file://")
    try:
        return str(Path(raw).resolve().relative_to(ws))
    except ValueError:
        return raw


# --------------------------------------------------------------------------- #
# Minimal LSP client (framed JSON-RPC over stdio)
# --------------------------------------------------------------------------- #


class LspClient:
    def __init__(self, workspace: Path) -> None:
        self.ws = workspace.resolve()
        ra = find_rust_analyzer()
        if not ra:
            raise RuntimeError("rust-analyzer not found (rustup component add rust-analyzer)")
        env = {**os.environ}
        self.proc = subprocess.Popen(
            [ra], cwd=self.ws, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        self._id = 0
        self._lock = threading.Lock()
        self._replies: dict[int, queue.Queue] = {}
        self._diags: dict[str, list[dict]] = {}     # uri -> diagnostics (replace on publish)
        self._progress_active: set[str] = set()
        self._last_publish = 0.0
        self._opened: dict[str, int] = {}           # abs path -> version
        threading.Thread(target=self._reader, daemon=True).start()
        self._initialize()

    # -- wire ---------------------------------------------------------------- #

    def _send(self, obj: dict) -> None:
        data = json.dumps(obj).encode("utf-8")
        frame = b"Content-Length: %d\r\n\r\n%b" % (len(data), data)
        with self._lock:
            assert self.proc.stdin is not None
            self.proc.stdin.write(frame)
            self.proc.stdin.flush()

    def _request(self, method: str, params: dict, timeout: float = REQUEST_TIMEOUT) -> dict:
        self._id += 1
        rid = self._id
        q: queue.Queue = queue.Queue()
        self._replies[rid] = q
        self._send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        try:
            return q.get(timeout=timeout)
        finally:
            self._replies.pop(rid, None)

    def _notify(self, method: str, params: dict) -> None:
        self._send({"jsonrpc": "2.0", "method": method, "params": params})

    def _reader(self) -> None:
        out = self.proc.stdout
        assert out is not None
        while True:
            line = out.readline()
            if not line:
                return
            if not line.lower().startswith(b"content-length:"):
                continue
            length = int(line.split(b":")[1].strip())
            while line not in (b"\r\n", b"\n", b""):
                line = out.readline()
            body = out.read(length)
            try:
                msg = json.loads(body)
            except json.JSONDecodeError:
                continue
            self._handle(msg)

    def _handle(self, msg: dict) -> None:
        if "id" in msg and ("result" in msg or "error" in msg):
            q = self._replies.get(msg["id"])
            if q is not None:
                q.put(msg)
            return
        method = msg.get("method")
        if method == "textDocument/publishDiagnostics":
            p = msg.get("params") or {}
            self._diags[p.get("uri", "")] = p.get("diagnostics", [])
            self._last_publish = time.monotonic()
        elif method == "$/progress":
            p = msg.get("params") or {}
            token = str(p.get("token"))
            kind = (p.get("value") or {}).get("kind")
            if kind == "begin":
                self._progress_active.add(token)
            elif kind == "end":
                self._progress_active.discard(token)
        elif method == "workspace/configuration" and "id" in msg:
            items = (msg.get("params") or {}).get("items", [])
            self._send({"jsonrpc": "2.0", "id": msg["id"], "result": [None] * len(items)})
        elif method == "window/workDoneProgress/create" and "id" in msg:
            self._send({"jsonrpc": "2.0", "id": msg["id"], "result": None})

    # -- lifecycle ------------------------------------------------------------ #

    def _initialize(self) -> None:
        resp = self._request("initialize", {
            "processId": os.getpid(),
            "rootUri": path_to_uri(self.ws),
            "workspaceFolders": [{"uri": path_to_uri(self.ws), "name": self.ws.name}],
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {"relatedInformation": True},
                    "synchronization": {"didSave": True},
                },
                "workspace": {"configuration": True, "workspaceFolders": True},
                "window": {"workDoneProgress": True},
            },
            "initializationOptions": {
                # NATIVE ONLY. checkOnSave would make every query run cargo --
                # measured at 6.4x slower agent turns on tsz. Never enable it.
                "checkOnSave": False,
                # Isolated target for the daemon's own startup cargo (build
                # scripts / proc macros): never contend with the agent's .target.
                "cargo": {"buildScripts": {"enable": True},
                          "targetDir": ".target/tszd"},
                "procMacro": {"enable": True},
                # unresolved-method etc. are gated behind "experimental".
                "diagnostics": {"enable": True, "experimental": {"enable": True}},
                # Memory: measured ~10.5 GB RSS on tsz with defaults. Skip cache
                # priming (we analyze on demand) and cap the query LRU.
                "cachePriming": {"enable": False},
                "lru": {"capacity": 64},
            },
        }, timeout=INIT_TIMEOUT)
        if "error" in resp:
            raise RuntimeError(f"initialize failed: {resp['error']}")
        self._notify("initialized", {})

    def shutdown(self) -> None:
        try:
            self._request("shutdown", {}, timeout=5)
            self._notify("exit", {})
        except Exception:
            pass
        try:
            self.proc.terminate()
        except Exception:
            pass

    # -- documents ------------------------------------------------------------ #

    def sync_files(self, paths: list[Path]) -> int:
        """didOpen unopened files, version-bump didChange opened ones, didClose
        deleted ones. Constraint 3: rust-analyzer only re-publishes for docs it
        sees change, so callers re-sync ALL relevant docs, not just edited ones."""
        n = 0
        for p in paths:
            key = str(p)
            uri = path_to_uri(p)
            if not p.exists():
                if key in self._opened:
                    self._notify("textDocument/didClose", {"textDocument": {"uri": uri}})
                    self._opened.pop(key, None)
                    self._diags.pop(uri, None)
                continue
            try:
                text = p.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            if key not in self._opened:
                self._opened[key] = 1
                self._notify("textDocument/didOpen", {"textDocument": {
                    "uri": uri, "languageId": "rust", "version": 1, "text": text}})
            else:
                self._opened[key] += 1
                self._notify("textDocument/didChange", {
                    "textDocument": {"uri": uri, "version": self._opened[key]},
                    "contentChanges": [{"text": text}]})
            self._notify("textDocument/didSave", {"textDocument": {"uri": uri}, "text": text})
            n += 1
        return n

    # -- navigation ------------------------------------------------------------ #

    def _doc_position(self, rel: str, line: int, col: int) -> dict:
        """1-based agent coordinates -> LSP TextDocumentPositionParams.

        The document must be open for rust-analyzer to answer reliably; sync it
        on demand (a didOpen of one file is cheap).
        """
        p = (self.ws / rel).resolve()
        if str(p) not in self._opened:
            self.sync_files([p])
        return {"textDocument": {"uri": path_to_uri(p)},
                "position": {"line": max(0, line - 1), "character": max(0, col - 1)}}

    def _loc_to_dict(self, loc: dict) -> dict:
        uri = loc.get("uri") or loc.get("targetUri", "")
        rng = loc.get("range") or loc.get("targetSelectionRange") or {}
        start = rng.get("start") or {}
        return {"file": uri_to_rel(uri, self.ws),
                "line": int(start.get("line", 0)) + 1,
                "col": int(start.get("character", 0)) + 1}

    def definition(self, rel: str, line: int, col: int) -> list[dict]:
        resp = self._request("textDocument/definition",
                             self._doc_position(rel, line, col))
        res = resp.get("result") or []
        if isinstance(res, dict):
            res = [res]
        return [self._loc_to_dict(loc) for loc in res]

    def references(self, rel: str, line: int, col: int, cap: int = 30) -> list[dict]:
        params = self._doc_position(rel, line, col)
        params["context"] = {"includeDeclaration": False}
        resp = self._request("textDocument/references", params)
        return [self._loc_to_dict(loc) for loc in (resp.get("result") or [])[:cap]]

    def hover(self, rel: str, line: int, col: int) -> str:
        resp = self._request("textDocument/hover",
                             self._doc_position(rel, line, col))
        contents = (resp.get("result") or {}).get("contents") or {}
        if isinstance(contents, dict):
            return contents.get("value", "")
        if isinstance(contents, list):
            return "\n".join(c.get("value", c) if isinstance(c, dict) else str(c)
                             for c in contents)
        return str(contents)

    def symbols(self, query: str, cap: int = 30) -> list[dict]:
        resp = self._request("workspace/symbol", {"query": query})
        out = []
        for s in (resp.get("result") or [])[:cap]:
            d = self._loc_to_dict(s.get("location") or {})
            d["name"] = s.get("name", "")
            d["kind"] = s.get("kind", 0)
            out.append(d)
        return out

    # -- diagnostics ----------------------------------------------------------- #

    def wait_quiescent(self, wait: float, ceiling: float = 120.0) -> None:
        """Wait until indexing progress is over and publishes go quiet."""
        deadline = time.monotonic() + ceiling
        settle = time.monotonic() + wait
        while time.monotonic() < deadline:
            if self._progress_active:
                settle = time.monotonic() + wait
                time.sleep(0.1)
                continue
            if self._last_publish > settle - wait:
                settle = self._last_publish + wait
            if time.monotonic() >= settle:
                return
            time.sleep(0.1)

    def diagnostics(self, wait: float = WARM_DIAG_WAIT) -> list[dict]:
        self.wait_quiescent(wait)
        out: list[dict] = []
        for uri, diags in self._diags.items():
            rel = uri_to_rel(uri, self.ws)
            for d in diags:
                start = (d.get("range") or {}).get("start") or {}
                out.append({
                    "file": rel,
                    "line": int(start.get("line", 0)) + 1,
                    "col": int(start.get("character", 0)) + 1,
                    "severity": SEVERITY.get(d.get("severity"), "info"),
                    "code": (d.get("code") if isinstance(d.get("code"), str)
                             else (d.get("code") or {}).get("value", "") if isinstance(d.get("code"), dict)
                             else str(d.get("code") or "")),
                    "message": (d.get("message") or "").split("\n")[0][:200],
                })
        out.sort(key=lambda d: ({"error": 0, "warning": 1}.get(d["severity"], 2),
                                d["file"], d["line"]))
        return out


# --------------------------------------------------------------------------- #
# Daemon server
# --------------------------------------------------------------------------- #


class Daemon:
    def __init__(self, ws: Path) -> None:
        self.ws = ws.resolve()
        self.state = self.ws / STATE_DIR
        self.state.mkdir(exist_ok=True)
        self.client: LspClient | None = None
        self.sock: socket.socket | None = None
        self._stop = threading.Event()
        self._last_request = time.monotonic()
        self._synced: set[str] = set()      # constraint 4: re-sync reverted files
        self._open_cone: set[str] = set()   # crates currently opened
        self._cache: tuple[str, list[dict]] | None = None
        self._stats = {"queries": 0, "cache_hits": 0, "started": time.time()}

    # -- dirty cone ------------------------------------------------------------ #

    def _git(self, *args: str) -> str:
        try:
            return subprocess.run(["git", "-C", str(self.ws), *args],
                                  capture_output=True, text=True, timeout=30).stdout
        except (OSError, subprocess.SubprocessError):
            return ""

    def changed_files(self) -> list[str]:
        base = self._git("merge-base", "HEAD", "origin/main").strip() or "HEAD"
        files = set(self._git("diff", "--name-only", base).splitlines())
        files |= set(self._git("diff", "--name-only", "HEAD").splitlines())
        files |= set(self._git("ls-files", "--others", "--exclude-standard").splitlines())
        return sorted(f for f in files if f.strip())

    def scope_crates(self) -> set[str]:
        """Dirty cone: crates containing changed files, plus explicit adds."""
        crates: set[str] = set()
        for f in self.changed_files():
            parts = f.split("/")
            if len(parts) >= 3 and parts[0] == "crates":
                crates.add(parts[1])
        scope_file = self.state / "scope.json"
        if scope_file.exists():
            try:
                crates |= set(json.loads(scope_file.read_text()))
            except (OSError, json.JSONDecodeError):
                pass
        return crates

    def crate_sources(self, crates: set[str]) -> list[str]:
        rels = []
        for line in self._git("ls-files", "crates").splitlines():
            parts = line.split("/")
            if len(parts) >= 3 and parts[1] in crates and line.endswith(".rs"):
                rels.append(line)
        return rels

    # -- lifecycle -------------------------------------------------------------- #

    def start(self) -> None:
        log(self.ws, "daemon starting")
        t0 = time.monotonic()
        self.client = LspClient(self.ws)
        # Constraint 2: let workspace loading settle BEFORE opening documents.
        self.client.wait_quiescent(wait=1.5, ceiling=INIT_TIMEOUT)
        self._refresh_cone(initial=True)
        warm = time.monotonic() - t0
        log(self.ws, f"warm in {warm:.1f}s; cone={sorted(self._open_cone)}")

        path = self.state / "daemon.sock"
        if path.exists():
            path.unlink()
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.bind(str(path))
        self.sock.listen(8)
        (self.state / "daemon.json").write_text(json.dumps({
            "pid": os.getpid(), "socket": str(path), "warm_s": round(warm, 1),
            "started": time.strftime("%Y-%m-%dT%H:%M:%S"),
        }))
        # Ready: clear the startup guard so `ra up` relies on ping again.
        (self.state / "starting.pid").unlink(missing_ok=True)
        self.sock.settimeout(5.0)
        while not self._stop.is_set():
            if time.monotonic() - self._last_request > IDLE_SHUTDOWN_S:
                log(self.ws, "idle shutdown")
                break
            try:
                conn, _ = self.sock.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            with conn:
                self._serve_one(conn)
        self._cleanup()

    def _refresh_cone(self, initial: bool = False) -> list[str]:
        """Open (or extend) the dirty-cone crates' sources. Returns new rels."""
        assert self.client is not None
        cone = self.scope_crates()
        new = cone - self._open_cone
        if not new and not initial:
            return []
        rels = self.crate_sources(new if not initial else cone)
        if rels:
            self.client.sync_files([self.ws / r for r in rels])
            log(self.ws, f"opened {len(rels)} sources for crates {sorted(new or cone)}")
        self._open_cone |= cone
        return rels

    def _serve_one(self, conn: socket.socket) -> None:
        try:
            conn.settimeout(REQUEST_TIMEOUT)
            buf = b""
            while b"\n" not in buf:
                chunk = conn.recv(65536)
                if not chunk:
                    return
                buf += chunk
            req = json.loads(buf.split(b"\n", 1)[0])
            resp = self._dispatch(req)
        except Exception as exc:  # noqa: BLE001 -- always answer
            resp = {"ok": False, "error": f"{type(exc).__name__}: {exc}"}
        try:
            conn.sendall((json.dumps(resp) + "\n").encode("utf-8"))
        except OSError:
            pass

    def _signature(self) -> str:
        parts = []
        for rel in self.changed_files():
            base = rel.rsplit("/", 1)[-1]
            if not (rel.endswith(".rs") or base in IMPORTANT_INPUTS):
                continue
            p = self.ws / rel
            try:
                parts.append(rel + ":" + hashlib.sha256(p.read_bytes()).hexdigest())
            except OSError:
                parts.append(rel + ":missing")
        return hashlib.sha256("\n".join(parts).encode()).hexdigest()

    def _dispatch(self, req: dict) -> dict:
        cmd = req.get("cmd")
        self._last_request = time.monotonic()
        if cmd == "ping":
            return {"ok": True, "ready": True}
        if cmd == "shutdown":
            self._stop.set()
            return {"ok": True}
        if cmd == "scope":
            add = req.get("add")
            scope_file = self.state / "scope.json"
            cur = set()
            if scope_file.exists():
                try:
                    cur = set(json.loads(scope_file.read_text()))
                except (OSError, json.JSONDecodeError):
                    pass
            if add:
                cur.add(add)
                scope_file.write_text(json.dumps(sorted(cur)))
                self._refresh_cone()
            return {"ok": True, "scope": sorted(self._open_cone | cur)}
        if cmd == "stats":
            return {"ok": True, **self._stats,
                    "open_cone": sorted(self._open_cone)}
        if cmd == "diag":
            return self._diag()
        if cmd in ("def", "refs", "hover"):
            assert self.client is not None
            rel, line, col = req["file"], int(req["line"]), int(req["col"])
            if cmd == "def":
                return {"ok": True, "locations": self.client.definition(rel, line, col)}
            if cmd == "refs":
                return {"ok": True, "locations": self.client.references(rel, line, col)}
            return {"ok": True, "hover": self.client.hover(rel, line, col)}
        if cmd == "symbols":
            assert self.client is not None
            return {"ok": True, "symbols": self.client.symbols(str(req.get("query", "")))}
        return {"ok": False, "error": f"unknown cmd {cmd!r}"}

    def _diag(self) -> dict:
        assert self.client is not None
        self._stats["queries"] += 1
        sig = self._signature()
        if self._cache and self._cache[0] == sig:
            self._stats["cache_hits"] += 1
            return {"ok": True, "cached": True, "diagnostics": self._cache[1]}
        self._refresh_cone()
        changed = {f for f in self.changed_files() if f.endswith(".rs")}
        # Constraints 3+4: re-sync changed + previously-synced (revert case) +
        # every open cone doc (unchanged-but-affected files re-publish).
        cone_rels = set(self.crate_sources(self._open_cone))
        to_sync = changed | self._synced | cone_rels
        if to_sync:
            self.client.sync_files([self.ws / r for r in sorted(to_sync)])
        self._synced = changed
        diags = self.client.diagnostics()
        self._cache = (sig, diags)
        return {"ok": True, "cached": False, "diagnostics": diags}

    def _cleanup(self) -> None:
        for name in ("daemon.sock", "daemon.json"):
            try:
                (self.state / name).unlink(missing_ok=True)
            except OSError:
                pass
        if self.client:
            self.client.shutdown()
        if self.sock:
            try:
                self.sock.close()
            except OSError:
                pass
        log(self.ws, "daemon stopped")


def main() -> int:
    ws = Path(sys.argv[sys.argv.index("--workspace") + 1]
              if "--workspace" in sys.argv else ".").resolve()
    Daemon(ws).start()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
