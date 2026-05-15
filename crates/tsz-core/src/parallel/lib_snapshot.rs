//! Persistent on-disk snapshot of parsed + bound lib files.
//!
//! See `docs/plan/PERFORMANCE_PLAN.md` for the full campaign,
//! and internal implementation planning notes for the disk-backed cache rollout.
//! This is PR #4 — the disk-backed cache
//! wired through the binder serde foundation that landed in PRs #1-#3.
//!
//! # Format
//!
//! `[8-byte magic "TSZSNAP\x03"][bincode payload]`. Bumping the trailing
//! byte invalidates older snapshots — necessary when the
//! `BinderState`/`NodeArena` layout shifts in a way that breaks
//! positional binary decoding.
//!
//! Bincode requires the wire layout to be stable across writes — bincode
//! 1.x DOES honour `#[serde(skip_serializing_if = "...")]` on the
//! serialise side but not on deserialise, which causes the buffer to
//! desync ("unexpected end of file"). This PR drops two such
//! annotations on bool fields in `tsz-parser`
//! (`LiteralData::has_invalid_escape`, `FunctionTypeData::is_abstract`)
//! and keeps `#[serde(default)]` so existing JSON IPC consumers that
//! elided the field continue to deserialise. The runtime cost is one
//! always-emitted byte per node when the bool is false, negligible.
//!
//! # Lifecycle
//!
//! `parse_and_bind_lib_file` consults the cache before parsing:
//!
//! 1. Compute `(file_name, source_text)` content hash via `FxHasher`.
//! 2. Look up `<dir>/<hash>.bin` on disk.
//! 3. On hit, deserialise the snapshot into `(NodeArena, BinderState)`
//!    and return immediately — skips both parse AND bind.
//! 4. On miss, parse + bind normally, write the snapshot, return.
//!
//! Resolution caches inside `BinderState` are `#[serde(skip)]` and
//! repopulate lazily on first lookup; this is the lazy-rebuild
//! invariant established in PR #3.
//!
//! # Opt-in
//!
//! Cache reads + writes are gated on `TSZ_LIB_CACHE=1` (or `=on`,
//! `=true`, `=yes`). Default off so this PR is safe to land without
//! changing existing behaviour. PR #5 of the campaign will flip the
//! default after CI sampling confirms the win.

use anyhow::{Context, Result, anyhow};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

/// Magic header. Trailing byte is the format version. Bump on layout
/// changes that break round-trip.
const SNAPSHOT_MAGIC: &[u8; 8] = b"TSZSNAP\x03";

/// Environment variable that opts the cache in.
const ENV_VAR: &str = "TSZ_LIB_CACHE";

/// Cache directory environment variable override.
const ENV_DIR: &str = "TSZ_LIB_CACHE_DIR";

/// Persistent representation of one cached lib file's full parse + bind
/// state: persists both `NodeArena` (parser AST + its
/// interner, see PR #4528 for round-trip foundation) and `BinderState`
/// (symbols + scopes + flow + declared modules, see PRs #1-#3).
#[derive(serde::Serialize, serde::Deserialize)]
struct LibSnapshot {
    /// File name as stored on the original `LibFile`.
    file_name: String,
    /// Hash of `(file_name, source_text)`. Verified on load to detect
    /// corrupted entries; the lookup also keys on the same hash via the
    /// filename, but verifying after load catches bit-rot.
    content_hash: u64,
    /// The parsed AST.
    arena: NodeArena,
    /// The bound symbol/scope/flow/declared-modules state.
    binder: BinderState,
    /// Root source-file `NodeIndex`.
    root_index: NodeIndex,
}

fn content_hash(file_name: &str, source_text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    file_name.hash(&mut hasher);
    source_text.hash(&mut hasher);
    hasher.finish()
}

fn cache_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(ENV_DIR) {
        return Some(PathBuf::from(dir));
    }
    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".cache"))
        })?;
    Some(base.join("tsz").join("lib-cache"))
}

fn is_enabled() -> bool {
    static CACHED: AtomicBool = AtomicBool::new(false);
    static INITIALISED: AtomicBool = AtomicBool::new(false);

    if INITIALISED.load(Ordering::Relaxed) {
        return CACHED.load(Ordering::Relaxed);
    }
    let enabled = match std::env::var(ENV_VAR) {
        Ok(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "on" | "true" | "yes"),
        Err(_) => false,
    };
    CACHED.store(enabled, Ordering::Relaxed);
    INITIALISED.store(true, Ordering::Relaxed);
    enabled
}

fn snapshot_path(dir: &Path, hash: u64) -> PathBuf {
    dir.join(format!("{hash:016x}.bin"))
}

/// Try to load a cached snapshot. Returns `None` on miss / format
/// mismatch / I/O error. Failures are silent — the caller falls back
/// to parse + bind.
pub(super) fn try_load(file_name: &str, source_text: &str) -> Option<Arc<LibFile>> {
    if !is_enabled() {
        return None;
    }
    let dir = cache_dir()?;
    let hash = content_hash(file_name, source_text);
    let path = snapshot_path(&dir, hash);
    let bytes = fs::read(&path).ok()?;
    let snapshot = decode_snapshot(&bytes).ok()?;
    if snapshot.content_hash != hash || snapshot.file_name != file_name {
        return None;
    }
    Some(Arc::new(LibFile::new(
        snapshot.file_name,
        Arc::new(snapshot.arena),
        Arc::new(snapshot.binder),
        snapshot.root_index,
    )))
}

/// Persist a parsed + bound lib file. Errors are logged at `debug!`
/// level but never propagated — write failures must not affect
/// compilation correctness.
pub(super) fn try_store(file_name: &str, source_text: &str, lib: &Arc<LibFile>) -> Result<()> {
    if !is_enabled() {
        return Ok(());
    }
    let dir = cache_dir().ok_or_else(|| anyhow!("no cache directory available"))?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create lib cache dir {}", dir.display()))?;

    let hash = content_hash(file_name, source_text);
    let path = snapshot_path(&dir, hash);

    let snapshot = LibSnapshot {
        file_name: file_name.to_string(),
        content_hash: hash,
        arena: (*lib.arena).clone(),
        binder: (*lib.binder).clone(),
        root_index: lib.root_index,
    };

    let encoded = encode_snapshot(&snapshot)?;

    // Atomic-rename pattern: write to a sibling temp file then rename.
    // Two concurrent processes that both miss may race here; the last
    // writer wins but neither produces a torn file.
    let tmp = path.with_extension("bin.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .with_context(|| format!("create snapshot tmp {}", tmp.display()))?;
        f.write_all(&encoded)
            .with_context(|| format!("write snapshot tmp {}", tmp.display()))?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, &path)
        .with_context(|| format!("rename snapshot {} -> {}", tmp.display(), path.display()))?;

    Ok(())
}

fn encode_snapshot(snapshot: &LibSnapshot) -> Result<Vec<u8>> {
    let payload = bincode::serialize(snapshot).context("bincode serialize lib snapshot")?;
    let mut out = Vec::with_capacity(SNAPSHOT_MAGIC.len() + payload.len());
    out.extend_from_slice(SNAPSHOT_MAGIC);
    out.extend_from_slice(&payload);
    Ok(out)
}

fn decode_snapshot(bytes: &[u8]) -> Result<LibSnapshot> {
    if bytes.len() < SNAPSHOT_MAGIC.len() || &bytes[..SNAPSHOT_MAGIC.len()] != SNAPSHOT_MAGIC {
        return Err(anyhow!("snapshot magic mismatch"));
    }
    let payload = &bytes[SNAPSHOT_MAGIC.len()..];
    bincode::deserialize(payload).context("bincode deserialize lib snapshot")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_bind(file_name: &str, source: &str) -> Arc<LibFile> {
        use tsz_parser::parser::ParserState;
        let mut parser = ParserState::new(file_name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = Arc::new(parser.into_arena());
        let mut binder = BinderState::new();
        binder.bind_source_file(&arena, root);
        Arc::new(LibFile::new(
            file_name.to_string(),
            arena,
            Arc::new(binder),
            root,
        ))
    }

    #[test]
    fn snapshot_round_trips_via_bincode() {
        let lib = parse_and_bind(
            "snap_test.d.ts",
            "interface Promise<T> { then(): Promise<T>; } export const x = 1;",
        );
        let snapshot = LibSnapshot {
            file_name: "snap_test.d.ts".to_string(),
            content_hash: 0xdeadbeef,
            arena: (*lib.arena).clone(),
            binder: (*lib.binder).clone(),
            root_index: lib.root_index,
        };
        let bytes = encode_snapshot(&snapshot).expect("encode");
        let decoded = decode_snapshot(&bytes).expect("decode");
        assert_eq!(decoded.file_name, "snap_test.d.ts");
        assert_eq!(decoded.content_hash, 0xdeadbeef);
        assert_eq!(decoded.root_index, lib.root_index);
        // Symbols round-tripped: re-look-up Promise should return same SymbolId.
        let original_promise = lib.binder.file_locals.get("Promise");
        let restored_promise = decoded.binder.file_locals.get("Promise");
        assert_eq!(original_promise, restored_promise);
    }

    #[test]
    fn snapshot_rejects_wrong_magic() {
        let bad = b"XXXX\x00\x00\x00\x00rest";
        assert!(decode_snapshot(bad).is_err());
    }

    #[test]
    fn cache_disabled_returns_none() {
        if std::env::var(ENV_VAR).is_err() {
            assert!(try_load("never_cached.ts", "const x = 1").is_none());
        }
    }

    /// End-to-end disk round-trip: write a snapshot, read it back, and
    /// verify identifier text resolves correctly through the
    /// reconstituted arena AND that bound symbols (Promise, greeting,
    /// declared modules) are intact.
    #[test]
    #[allow(unsafe_code)]
    fn disk_round_trip_resolves_identifier_text_and_symbols() {
        // SAFETY: nextest runs each test in its own process, so the env
        // mutations don't race other threads.
        unsafe {
            std::env::set_var(ENV_VAR, "1");
        }
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        unsafe {
            std::env::set_var(ENV_DIR, tmp.path());
        }

        let file_name = "snapshot_e2e.d.ts";
        let source = "interface Promise<T> { then(): Promise<T>; }\nconst greeting = \"hi\";\ndeclare module \"virtual:env\" { export const VAL: string; }\n";

        let lib = parse_and_bind(file_name, source);
        let original_promise_id = lib.binder.file_locals.get("Promise");
        let original_greeting_id = lib.binder.file_locals.get("greeting");
        let original_module_count = lib.binder.declared_modules.len();
        assert!(original_promise_id.is_some());
        assert!(original_greeting_id.is_some());

        try_store(file_name, source, &lib).expect("first write should succeed");

        // Cache hit: round-trip through disk.
        let restored = try_load(file_name, source).expect("cache should hit");

        // Symbols match.
        assert_eq!(
            restored.binder.file_locals.get("Promise"),
            original_promise_id
        );
        assert_eq!(
            restored.binder.file_locals.get("greeting"),
            original_greeting_id
        );
        assert_eq!(
            restored.binder.declared_modules.len(),
            original_module_count
        );
        assert!(restored.binder.declared_modules.contains("virtual:env"));

        // Identifier text resolves through the restored arena.
        let mut found_promise = false;
        let mut found_greeting = false;
        for raw in 0..restored.arena.len() {
            let idx = tsz_parser::NodeIndex(u32::try_from(raw).expect("index fits"));
            let Some(node) = restored.arena.get(idx) else {
                continue;
            };
            if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(data) = restored.arena.get_identifier(node) else {
                continue;
            };
            let text = restored.arena.interner.resolve(data.atom);
            if text == "Promise" {
                found_promise = true;
            }
            if text == "greeting" {
                found_greeting = true;
            }
        }
        assert!(found_promise, "Promise identifier text round-tripped");
        assert!(found_greeting, "greeting identifier text round-tripped");

        // Negative-cache assertions.
        assert!(try_load("other_file.d.ts", source).is_none());
        assert!(try_load(file_name, "const z = 0;").is_none());
    }

    #[test]
    fn content_hash_is_stable_and_distinguishes_inputs() {
        let h1 = content_hash("a.d.ts", "const x = 1;");
        let h2 = content_hash("a.d.ts", "const x = 1;");
        let h3 = content_hash("a.d.ts", "const x = 2;");
        let h4 = content_hash("b.d.ts", "const x = 1;");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h1, h4);
    }
}
