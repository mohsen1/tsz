//! Cross-module import-cycle class-member degradation (M1 canary family).
//!
//! Structural rule: when module A and module B import from each other and each
//! declares a class whose member type references the other module's class (a
//! type-level cycle across arenas), `tsc` resolves every cross-file class
//! reference to the fully-membered declared instance type. Before the fix, the
//! cross-arena class-instance delegation spun up a fresh child checker per
//! reference — whose empty resolution set did not know the class was already in
//! flight — so the mutual reference re-delegated until the cross-arena depth cap
//! truncated the chain and dropped the surviving class's members, producing a
//! spurious `TS2339` "property does not exist" (the xstate
//! `StateMachine`<->`StateNode` family). The fix tracks each class-instance build
//! by its stable `(owner file, declaration node)` and returns a lazy
//! self-reference on re-entry, exactly as tsc defers a class inside its own
//! member signatures.
//!
//! This must be a real multi-module driver test (`crate::driver::compile`): the
//! in-crate checker harness resolves every file through a single context and
//! does not exercise the cross-arena class delegation that hosts the bug.
//!
//! The witnesses vary binder names (unrelated to any canary source) and are
//! checked in both root-file orders, because the bug is order-gated on which
//! file the driver checks first.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

fn compile_in_order(files: &[(&str, &str)], root_order: &[&str]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).expect("write repro file");
    }

    let mut argv: Vec<&str> = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "--lib",
        "es2022",
    ];
    argv.extend_from_slice(root_order);

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn ts2339(files: &[(&str, &str)], roots: &[&str]) -> Vec<(String, String)> {
    // Both root orders: the degradation is order-gated on which file is checked
    // first, so a fix must hold regardless of driver file ordering.
    let mut out: Vec<(String, String)> = Vec::new();
    for order in [roots.to_vec(), roots.iter().rev().copied().collect()] {
        for d in compile_in_order(files, &order) {
            if d.code == 2339 {
                out.push((order.join(","), d.message_text.clone()));
            }
        }
    }
    out
}

const WAREHOUSE: &str = r#"
import { Crate } from "./crate.ts";

export class Warehouse<TCtx, TEvt> {
  public label = '(warehouse)';
  public version?: string;
  public impls: { helpers: Record<string, unknown> } = { helpers: {} };
  public registry: Map<string, Crate<TCtx, TEvt>> = new Map();
  public root: Crate<TCtx, TEvt>;

  constructor() {
    this.root = new Crate({ _key: this.label, _owner: this as any });
  }

  public lookupById(crateId: string): Crate<TCtx, TEvt> {
    return this.registry.get(crateId)!;
  }
}
"#;

const CRATE: &str = r#"
import type { Warehouse } from "./warehouse.ts";

interface CrateOptions<TCtx, TEvt> {
  _parent?: Crate<TCtx, TEvt>;
  _key: string;
  _owner: Warehouse<any, any>;
}

export class Crate<TCtx, TEvt> {
  public parent?: Crate<TCtx, TEvt>;
  public key: string;
  public ident: string;
  public rank: number;
  public owner: Warehouse<TCtx, TEvt>;

  constructor(options: CrateOptions<TCtx, TEvt>) {
    this.parent = options._parent;
    this.key = options._key;
    this.owner = options._owner;
    this.ident = [this.owner.label, this.key].join('.');
    this.rank = this.owner.registry.size;
    this.owner.registry.set(this.ident, this);
  }

  public describe() {
    return { version: this.owner.version, helpers: this.owner.impls.helpers };
  }
}
"#;

/// The import cycle must not drop `Warehouse`'s members when `Crate` reads them
/// off its `owner: Warehouse<...>` field, in either root-file order.
#[test]
fn import_cycle_owner_members_survive_cross_arena() {
    let hits = ts2339(
        &[("warehouse.ts", WAREHOUSE), ("crate.ts", CRATE)],
        &["warehouse.ts", "crate.ts"],
    );
    assert!(
        hits.is_empty(),
        "cross-file class cycle must not drop members (false TS2339); got: {hits:#?}"
    );
}

// Renamed, differently-shaped variant: the value-importer holds an array member
// of the other class and a self-referential `parent`, and the importer reads a
// getter chain. Confirms the fix follows structure, not identifier text.
const NODE_HUB: &str = r#"
import { Leaf } from "./leaf.ts";

export class NodeHub<S, E> {
  public tag = 'hub';
  public spec?: number;
  public leaves: Leaf<S, E>[] = [];
  public primary: Leaf<S, E>;

  constructor() {
    this.primary = new Leaf(this as any);
  }
}
"#;

const LEAF: &str = r#"
import type { NodeHub } from "./node_hub.ts";

export class Leaf<S, E> {
  public up?: Leaf<S, E>;
  public host: NodeHub<S, E>;
  public name: string;

  constructor(host: NodeHub<S, E>) {
    this.host = host;
    this.name = this.host.tag;
  }

  public info() {
    return { spec: this.host.spec, count: this.host.leaves.length };
  }
}
"#;

#[test]
fn import_cycle_variant_host_members_survive() {
    let hits = ts2339(
        &[("node_hub.ts", NODE_HUB), ("leaf.ts", LEAF)],
        &["node_hub.ts", "leaf.ts"],
    );
    assert!(
        hits.is_empty(),
        "cross-file class cycle (variant) must not drop members; got: {hits:#?}"
    );
}

// Acyclic control: with the back-reference removed the imported class has no
// members that reference the importer, so there is genuinely no member to find —
// but the accessed members DO exist, so this must stay clean, proving the fix
// does not mask a real missing-member error.
const ACYCLIC_HUB: &str = r#"
import { Spoke } from "./spoke.ts";
export class AcyclicHub<S, E> {
  public tag = 'hub';
  public spec?: number;
  public one: Spoke<S, E>;
  constructor(s: Spoke<S, E>) { this.one = s; }
}
"#;

const SPOKE: &str = r#"
export class Spoke<S, E> {
  public name: string = '';
}
"#;

#[test]
fn acyclic_cross_module_stays_clean() {
    let hits = ts2339(
        &[("acyclic_hub.ts", ACYCLIC_HUB), ("spoke.ts", SPOKE)],
        &["acyclic_hub.ts", "spoke.ts"],
    );
    assert!(
        hits.is_empty(),
        "acyclic cross-module control must be clean; got: {hits:#?}"
    );
}
