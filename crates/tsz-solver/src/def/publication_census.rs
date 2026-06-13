//! Env-gated census of `DefinitionStore` body publications.
//!
//! The mutation-isolation campaign (lifting the sequential fresh-checking
//! gate in the CLI driver) needs the shared `DefinitionStore` to become
//! immutable during parallel file checking. Today every per-file checker
//! re-derives and republishes def bodies into the shared store
//! (last-writer-wins), and sibling workers can observe bodies mid-rewrite.
//!
//! This module measures that republication traffic so the campaign can decide
//! which def classes must be pre-materialized before the parallel phase:
//!
//! - **who**: which `DefId`s (kind, name, file) get republished,
//! - **from where**: which call paths publish (via `#[track_caller]`
//!   attribution on [`super::DefinitionStore::set_body_with_params`]),
//! - **what changes**: first publication vs byte-identical republication vs
//!   a *different* body `TypeId` (the dangerous class — checker-relative
//!   forms are not interchangeable across checkers).
//!
//! Gated by the `TSZ_DEF_PUBLICATION_CENSUS` environment variable; when unset
//! the only cost on the publication path is one `OnceLock<bool>` load.
//! Recording takes a global mutex, which is acceptable at observed volumes
//! (~10^3..10^4 publications per project run). Output is rendered by
//! [`dump_to_string`]; the CLI driver owns file IO and name resolution.

use super::{DefId, DefKind, DefinitionInfo};
use crate::intern::TypeInterner;
use crate::types::{TypeData, TypeId, TypeParamInfo};
use rustc_hash::FxHashMap;
use std::panic::Location;
use std::sync::{Mutex, OnceLock};
use tsz_common::interner::Atom;

/// Cheap global gate: census recording is enabled iff
/// `TSZ_DEF_PUBLICATION_CENSUS` is set in the environment.
#[inline]
pub fn census_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("TSZ_DEF_PUBLICATION_CENSUS").is_some())
}

/// How a single `set_body_with_params` call related to the existing entry.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PublicationOutcome {
    /// Entry existed with no body yet: the first real publication.
    First,
    /// No entry existed; a minimal entry was minted (cross-file delegation).
    MintedMinimal,
    /// Republication with the identical body `TypeId` (and unchanged params).
    SameBody,
    /// Republication with the identical body `TypeId` but a different
    /// type-parameter list.
    SameBodyParamsChanged,
    /// Republication with a *different* body `TypeId` — the class that makes
    /// the shared store schedule-dependent under parallel checking.
    DifferentBody,
    /// Different body `TypeId` *and* a different type-parameter list.
    DifferentBodyParamsChanged,
    /// A different-body publication that was *dropped* because the def is
    /// frozen (publish-once after its finalized materialization): the store
    /// kept the finalized form.
    SuppressedDifferentBody,
    /// A pre-finalize different-body overwrite that was *deferred* (dropped)
    /// because the def belongs to the deferred-publication class: the store
    /// keeps the first form until the finalize entry point overwrites it.
    DeferredDifferentBody,
}

impl PublicationOutcome {
    const ALL: [Self; 8] = [
        Self::First,
        Self::MintedMinimal,
        Self::SameBody,
        Self::SameBodyParamsChanged,
        Self::DifferentBody,
        Self::DifferentBodyParamsChanged,
        Self::SuppressedDifferentBody,
        Self::DeferredDifferentBody,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::MintedMinimal => "minted-minimal",
            Self::SameBody => "same-body",
            Self::SameBodyParamsChanged => "same-body+params-changed",
            Self::DifferentBody => "different-body",
            Self::DifferentBodyParamsChanged => "different-body+params-changed",
            Self::SuppressedDifferentBody => "suppressed-different-body",
            Self::DeferredDifferentBody => "deferred-different-body",
        }
    }

    const fn is_different_body(self) -> bool {
        matches!(self, Self::DifferentBody | Self::DifferentBodyParamsChanged)
    }
}

/// Per-`DefId` aggregate.
struct DefCensus {
    name: Atom,
    kind: Option<DefKind>,
    file_id: Option<u32>,
    publications: u64,
    different_body: u64,
    /// Distinct body `TypeId`s observed (deduped, capped), each with the
    /// call site that first published that form.
    bodies: Vec<(TypeId, &'static str, u32)>,
    /// True once the distinct-body list overflowed [`MAX_TRACKED_BODIES`].
    bodies_overflowed: bool,
}

const MAX_TRACKED_BODIES: usize = 16;

#[derive(Default)]
struct CensusState {
    /// (caller file, caller line, outcome) -> count.
    by_site: FxHashMap<(&'static str, u32, PublicationOutcome), u64>,
    /// Same key, restricted to **lib interface** defs (`DefKind::Interface`
    /// with the non-program `u32::MAX` decl-file sentinel) — the def class
    /// the mutation-isolation campaign freezes; this cross-tab attributes
    /// the post-freeze residue to its write-through channels.
    by_site_lib_interface: FxHashMap<(&'static str, u32, PublicationOutcome), u64>,
    by_def: FxHashMap<DefId, DefCensus>,
}

/// `tsz_binder` symbols without a program declaration file (every lib-binder
/// symbol) carry `u32::MAX` as their declaration file index.
const NON_PROGRAM_FILE_SENTINEL: u32 = u32::MAX;

fn state() -> &'static Mutex<CensusState> {
    static STATE: OnceLock<Mutex<CensusState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CensusState::default()))
}

/// Record one body publication. Call only when [`census_enabled`] is true.
///
/// `prev_body` is the entry body before this write (`None` for both the
/// `First` and `MintedMinimal` outcomes, disambiguated by `outcome`).
pub fn record_publication(
    def_id: DefId,
    kind: Option<DefKind>,
    name: Atom,
    file_id: Option<u32>,
    new_body: TypeId,
    outcome: PublicationOutcome,
    caller: &'static Location<'static>,
) {
    let Ok(mut state) = state().lock() else {
        return;
    };
    *state
        .by_site
        .entry((caller.file(), caller.line(), outcome))
        .or_insert(0) += 1;
    if kind == Some(DefKind::Interface) && file_id == Some(NON_PROGRAM_FILE_SENTINEL) {
        *state
            .by_site_lib_interface
            .entry((caller.file(), caller.line(), outcome))
            .or_insert(0) += 1;
    }
    let entry = state.by_def.entry(def_id).or_insert_with(|| DefCensus {
        name,
        kind,
        file_id,
        publications: 0,
        different_body: 0,
        bodies: Vec::new(),
        bodies_overflowed: false,
    });
    // Later publications may carry richer metadata (a minted-minimal entry
    // gets a real kind/name once registered); keep the best-known values.
    if entry.kind.is_none() {
        entry.kind = kind;
    }
    if entry.name.is_none() && !name.is_none() {
        entry.name = name;
    }
    if entry.file_id.is_none() {
        entry.file_id = file_id;
    }
    entry.publications += 1;
    if outcome.is_different_body() {
        entry.different_body += 1;
    }
    if !entry.bodies.iter().any(|(body, _, _)| *body == new_body) {
        if entry.bodies.len() < MAX_TRACKED_BODIES {
            entry.bodies.push((new_body, caller.file(), caller.line()));
        } else {
            entry.bodies_overflowed = true;
        }
    }
}

/// Record an update against an existing guarded definition entry.
pub fn record_existing_publication(
    def_id: DefId,
    entry: &DefinitionInfo,
    new_body: TypeId,
    new_params: Option<&[TypeParamInfo]>,
    suppressed: bool,
    deferred: bool,
    caller: &'static Location<'static>,
) {
    let params_changed = new_params.is_some_and(|params| params != entry.type_params.as_slice());
    let outcome = if suppressed {
        PublicationOutcome::SuppressedDifferentBody
    } else if deferred {
        PublicationOutcome::DeferredDifferentBody
    } else {
        classify(entry.body, new_body, params_changed, true)
    };
    record_publication(
        def_id,
        Some(entry.kind),
        entry.name,
        entry.file_id,
        new_body,
        outcome,
        caller,
    );
}

/// Record publication of a body for a `DefId` that had no definition entry yet.
pub fn record_minted_minimal_publication(
    def_id: DefId,
    new_body: TypeId,
    caller: &'static Location<'static>,
) {
    record_publication(
        def_id,
        None,
        Atom::NONE,
        None,
        new_body,
        PublicationOutcome::MintedMinimal,
        caller,
    );
}

/// Classify a publication against the pre-write entry state.
pub fn classify(
    prev_body: Option<TypeId>,
    new_body: TypeId,
    params_changed: bool,
    entry_existed: bool,
) -> PublicationOutcome {
    match prev_body {
        None if !entry_existed => PublicationOutcome::MintedMinimal,
        None => PublicationOutcome::First,
        Some(prev) if prev == new_body => {
            if params_changed {
                PublicationOutcome::SameBodyParamsChanged
            } else {
                PublicationOutcome::SameBody
            }
        }
        Some(_) => {
            if params_changed {
                PublicationOutcome::DifferentBodyParamsChanged
            } else {
                PublicationOutcome::DifferentBody
            }
        }
    }
}

const fn kind_label(kind: Option<DefKind>) -> &'static str {
    match kind {
        Some(DefKind::TypeAlias) => "type-alias",
        Some(DefKind::Interface) => "interface",
        Some(DefKind::Class) => "class",
        Some(DefKind::ClassConstructor) => "class-constructor",
        Some(DefKind::Enum) => "enum",
        Some(DefKind::Namespace) => "namespace",
        Some(DefKind::Function) => "function",
        Some(DefKind::Variable) => "variable",
        None => "unknown",
    }
}

/// Shallow one-level description of a type for census body-form reports:
/// expands object/function shapes to property names and signature skeletons
/// so "what differs between republished body forms" is visible without a
/// full formatter context. Diagnostics tooling only.
pub fn describe_type_shallow(interner: &TypeInterner, id: TypeId) -> String {
    let Some(data) = interner.lookup(id) else {
        return "<not interned>".to_string();
    };
    match data {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let props: Vec<String> = shape
                .properties
                .iter()
                .take(8)
                .map(|p| format!("{}:{}", interner.resolve_atom(p.name), p.type_id.0))
                .collect();
            format!(
                "{data:?} flags={:?} symbol={:?} props[{}]={{{}{}}} sidx={:?} nidx={:?}",
                shape.flags,
                shape.symbol,
                shape.properties.len(),
                props.join(", "),
                if shape.properties.len() > 8 {
                    ", …"
                } else {
                    ""
                },
                shape.string_index.as_ref().map(|i| i.value_type.0),
                shape.number_index.as_ref().map(|i| i.value_type.0),
            )
        }
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            format!(
                "{data:?} tparams={} params={:?} ret={} method={}",
                shape.type_params.len(),
                shape.params.iter().map(|p| p.type_id.0).collect::<Vec<_>>(),
                shape.return_type.0,
                shape.is_method,
            )
        }
        other => format!("{other:?}"),
    }
}

/// Render the census as a human-readable report and clear the accumulated
/// state. Returns `None` when the census is disabled or empty.
///
/// `resolve_name` maps a def-name [`Atom`] to a string; `resolve_file` maps a
/// binder file index to a display name. Both are supplied by the caller (the
/// CLI driver) because the store does not own a string interner.
pub fn dump_to_string(
    resolve_name: &dyn Fn(Atom) -> String,
    resolve_file: &dyn Fn(u32) -> String,
    describe_type: &dyn Fn(TypeId) -> String,
) -> Option<String> {
    if !census_enabled() {
        return None;
    }
    let mut state = state().lock().ok()?;
    if state.by_site.is_empty() && state.by_def.is_empty() {
        return None;
    }
    let state = std::mem::take(&mut *state);

    let mut out = String::from("=== DefinitionStore body publication census ===\n");

    // ---- Totals by outcome ----
    let mut totals: FxHashMap<PublicationOutcome, u64> = FxHashMap::default();
    for (&(_, _, outcome), &count) in &state.by_site {
        *totals.entry(outcome).or_insert(0) += count;
    }
    let grand_total: u64 = totals.values().sum();
    out.push_str(&format!("total publications: {grand_total}\n"));
    for outcome in PublicationOutcome::ALL {
        let count = totals.get(&outcome).copied().unwrap_or(0);
        if count > 0 {
            out.push_str(&format!("  {:<32} {count}\n", outcome.label()));
        }
    }

    // ---- By call site x outcome ----
    out.push_str("\n--- publications by call site ---\n");
    let mut sites: Vec<_> = state
        .by_site
        .iter()
        .map(|(&(file, line, outcome), &count)| (file, line, outcome, count))
        .collect();
    sites.sort_by(|a, b| b.3.cmp(&a.3).then(a.0.cmp(b.0)).then(a.1.cmp(&b.1)));
    for (file, line, outcome, count) in sites {
        out.push_str(&format!(
            "  {count:>8}  {:<32} {file}:{line}\n",
            outcome.label()
        ));
    }

    // ---- Lib-interface defs by call site x outcome ----
    if !state.by_site_lib_interface.is_empty() {
        out.push_str("\n--- lib-interface publications by call site ---\n");
        let mut sites: Vec<_> = state
            .by_site_lib_interface
            .iter()
            .map(|(&(file, line, outcome), &count)| (file, line, outcome, count))
            .collect();
        sites.sort_by(|a, b| b.3.cmp(&a.3).then(a.0.cmp(b.0)).then(a.1.cmp(&b.1)));
        for (file, line, outcome, count) in sites {
            out.push_str(&format!(
                "  {count:>8}  {:<32} {file}:{line}\n",
                outcome.label()
            ));
        }
    }

    // ---- By def kind x file ----
    out.push_str("\n--- different-body republications by def kind ---\n");
    let mut by_kind: FxHashMap<&'static str, (u64, u64, u64)> = FxHashMap::default();
    for census in state.by_def.values() {
        let slot = by_kind.entry(kind_label(census.kind)).or_insert((0, 0, 0));
        slot.0 += census.publications;
        slot.1 += census.different_body;
        if census.different_body > 0 {
            slot.2 += 1;
        }
    }
    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by_key(|entry| std::cmp::Reverse(entry.1.1));
    out.push_str(&format!(
        "  {:<18} {:>10} {:>12} {:>10}\n",
        "kind", "pubs", "diff-body", "defs"
    ));
    for (kind, (pubs, diff, defs)) in kinds {
        out.push_str(&format!("  {kind:<18} {pubs:>10} {diff:>12} {defs:>10}\n"));
    }

    // ---- Different-body republications by file ----
    out.push_str("\n--- different-body republications by file ---\n");
    let mut by_file: FxHashMap<Option<u32>, u64> = FxHashMap::default();
    for census in state.by_def.values() {
        if census.different_body > 0 {
            *by_file.entry(census.file_id).or_insert(0) += census.different_body;
        }
    }
    let mut files: Vec<_> = by_file.into_iter().collect();
    files.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (file_id, count) in files.into_iter().take(40) {
        let name = file_id.map_or_else(|| "<no file>".to_string(), resolve_file);
        out.push_str(&format!("  {count:>8}  {name}\n"));
    }

    // ---- Top republished defs ----
    out.push_str("\n--- top defs by different-body republication ---\n");
    let mut defs: Vec<_> = state
        .by_def
        .iter()
        .filter(|(_, c)| c.different_body > 0)
        .collect();
    defs.sort_by_key(|entry| std::cmp::Reverse(entry.1.different_body));
    out.push_str(&format!(
        "  {:<10} {:>6} {:>10} {:>8}  {:<18} {:<28} {}\n",
        "def", "pubs", "diff-body", "bodies", "kind", "name", "file"
    ));
    let mut body_details = String::new();
    for (rank, (def_id, census)) in defs.iter().take(12).enumerate() {
        let name = resolve_name(census.name);
        body_details.push_str(&format!(
            "\n[{rank}] def #{} {} ({}) — {} distinct bodies{}\n",
            def_id.0,
            name,
            kind_label(census.kind),
            census.bodies.len(),
            if census.bodies_overflowed { "+" } else { "" },
        ));
        for (body, caller_file, caller_line) in &census.bodies {
            let desc = describe_type(*body);
            let desc: String = desc.chars().take(400).collect();
            body_details.push_str(&format!(
                "    {} = {desc}\n        first published by {caller_file}:{caller_line}\n",
                body.0
            ));
        }
    }
    for (def_id, census) in defs.into_iter().take(60) {
        let name = resolve_name(census.name);
        let file = census
            .file_id
            .map_or_else(|| "<no file>".to_string(), resolve_file);
        let bodies = if census.bodies_overflowed {
            format!("{}+", census.bodies.len())
        } else {
            census.bodies.len().to_string()
        };
        out.push_str(&format!(
            "  {:<10} {:>6} {:>10} {:>8}  {:<18} {:<28} {file}\n",
            format!("#{}", def_id.0),
            census.publications,
            census.different_body,
            bodies,
            kind_label(census.kind),
            name,
        ));
    }

    out.push_str("\n--- distinct body forms for top republished defs ---\n");
    out.push_str(&body_details);

    Some(out)
}
