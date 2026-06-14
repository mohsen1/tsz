//! Diagnostic emission methods for `CheckerContext`.
//!
//! Extracted from `context/core.rs` to keep that file below the 2000-line
//! hard limit (CLAUDE.md §19). All methods operate on `CheckerContext` fields
//! and contain no cross-method dependencies beyond what `self` provides.

use crate::context::CheckerContext;
use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages};

/// Decide whether a freshly produced diagnostic should replace an already
/// emitted one that collides with it on the dedup key.
///
/// The two diagnostics are only ever compared here when they tie on
/// location/code/message (`compare_skip_related_information` is `Equal`) but
/// carry different related ("see also") information. Such pairs are genuinely
/// distinct in rendered output, yet the dedup key cannot tell them apart, so
/// "keep whichever was produced first" makes the surviving diagnostic — and
/// therefore its sorted position — depend on relation traversal / solver
/// memoization order. That is the non-determinism the canonical comparator's
/// related-information tiebreaker is meant to remove but cannot, because dedup
/// drops one entry before the final sort ever runs.
///
/// Resolve it deterministically: prefer the diagnostic that carries the richer
/// elaboration (more related entries, so no "see also" context is lost to a
/// shorter equivalent path), and break ties on equal-length related lists with
/// the same canonical order [`Diagnostic::compare`] would impose. The result is
/// independent of which path reached the diagnostic first.
pub(crate) fn prefers_candidate_diagnostic(existing: &Diagnostic, candidate: &Diagnostic) -> bool {
    debug_assert!(
        existing.compare_skip_related_information(candidate).is_eq(),
        "collision reconciliation requires matching location/code/message"
    );
    match candidate
        .related_information
        .len()
        .cmp(&existing.related_information.len())
    {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => candidate.compare(existing).is_lt(),
    }
}

impl<'a> CheckerContext<'a> {
    /// Deterministically reconcile a dedup-key collision against the already
    /// emitted diagnostic list. Only diagnostics that carry related information
    /// can outrank an existing entry, so plain (related-free) re-derivations —
    /// the overwhelmingly common collision — short-circuit before any scan.
    fn reconcile_related_information_collision(&mut self, candidate: Diagnostic) {
        if candidate.related_information.is_empty() {
            // A related-free diagnostic can never carry more "see also" context
            // than an existing entry it ties with, so it cannot win; leaving the
            // existing entry in place preserves the established dedup behavior.
            return;
        }
        for existing in &mut self.diagnostics {
            if !existing
                .compare_skip_related_information(&candidate)
                .is_eq()
            {
                continue;
            }
            if prefers_candidate_diagnostic(existing, &candidate) {
                *existing = candidate;
            }
            return;
        }
    }

    fn diagnostic_dedup_key_from_parts(&self, start: u32, code: u32, message: &str) -> (u32, u32) {
        if code == 2318 && start == 0 {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            message.hash(&mut hasher);
            (hasher.finish() as u32, code)
        } else if code == 18047
            || code == 18048
            || code == 18049
            || code == 2322
            || code == 2339
            || code == 2374
            || code == 2411
            || code == 2413
            || code == 2416
            || code == 2430
            || code == 2536
            || code == 2537
            || code == 2538
            || code == 4094
        {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            message.hash(&mut hasher);
            (start ^ (hasher.finish() as u32), code)
        } else {
            (start, code)
        }
    }

    pub fn diagnostic_dedup_key(&self, diag: &Diagnostic) -> (u32, u32) {
        self.diagnostic_dedup_key_from_parts(diag.start, diag.code, &diag.message_text)
    }

    pub(crate) fn rebuild_diagnostic_aux_indices(&mut self) {
        self.diagnostic_indices.rebuild_aux_from(&self.diagnostics);
    }

    pub fn rebuild_emitted_diagnostics_from_current(&mut self) {
        self.diagnostic_indices.emitted.clear();
        // Also synchronize the TS2454 dedup set: remove entries for TS2454
        // diagnostics that are no longer in the diagnostics list (e.g., removed
        // by a prior `retain` call). Without this, removed TS2454 errors stay
        // in the dedup set and cannot be re-emitted on subsequent passes.
        let ts2454_positions: rustc_hash::FxHashSet<u32> = self
            .diagnostics
            .iter()
            .filter(|d| d.code == 2454)
            .map(|d| d.start)
            .collect();
        self.emitted_ts2454_errors
            .retain(|&(pos, _)| ts2454_positions.contains(&pos));
        for diag in &self.diagnostics {
            let key = self.diagnostic_dedup_key(diag);
            self.diagnostic_indices.emitted.insert(key);
        }
        self.rebuild_diagnostic_aux_indices();
    }

    /// Add an error diagnostic (with deduplication).
    /// Diagnostics with the same (start, code) are only emitted once.
    /// Exceptions:
    /// - TS2374 uses (start ^ `message_hash`, code) because union index
    ///   signatures can duplicate several key components at one span.
    /// - TS2411 uses (start ^ `message_hash`, code) to allow a single property to
    ///   fail against both string and number index signatures at the same span.
    /// - TS2413 uses the same scheme because one index signature can violate
    ///   multiple wider index signatures at the same span.
    /// - TS2430 uses (start ^ `message_hash`, code) to allow multiple
    ///   "incorrectly extends" errors at the same interface name when an interface
    ///   incompatibly extends several distinct bases.
    /// - TS2536/TS2537/TS2538 use the same scheme so indexed-access failures can
    ///   report multiple distinct messages at the same indexed-access start.
    /// - TS4094 uses (start ^ `message_hash`, code) because tsc anchors every
    ///   private/protected member of an exported anonymous class expression at the
    ///   owning variable/function name, producing one TS4094 per member at the
    ///   same span.
    pub fn error(&mut self, start: u32, length: u32, message: String, code: u32) {
        if self.reconcile_name_resolution_precedence(start, code) {
            return;
        }

        // Check if we've already emitted this diagnostic
        let key = self.diagnostic_dedup_key_from_parts(start, code, &message);
        if self.diagnostic_indices.emitted.contains(&key) {
            return;
        }
        self.diagnostic_indices.emitted.insert(key);
        tracing::debug!(
            code,
            start,
            length,
            file = %self.file_name,
            message = %message,
            "diagnostic"
        );
        let end = start.saturating_add(length);
        self.diagnostic_indices
            .update_aux_for(code, start, end, &message);
        self.diagnostics.push(Diagnostic::error(
            self.file_name.clone(),
            start,
            length,
            message,
            code,
        ));
    }

    /// Emit TS7039 ("Mapped object type implicitly has an 'any' template
    /// type.") for a mapped-type node that omits its template type under
    /// `noImplicitAny`.
    ///
    /// Single source of truth for the diagnostic: the type-lowering path
    /// (`get_type_from_mapped_type`) and the type-alias / member missing-name
    /// validation walks all route through here, so the message, code, and span
    /// have one owner and the same mapped-type node cannot emit the diagnostic
    /// more than once for structural reasons. Previously each walk emitted it
    /// twice and only the `(start, code)` dedup collapsed the redundant pair —
    /// a fragile masking that depended on both emissions resolving to the same
    /// span. Anchors on the mapped-type node, matching the span `tsc` reports.
    pub(crate) fn report_mapped_type_missing_template(
        &mut self,
        mapped_idx: tsz_parser::parser::NodeIndex,
    ) {
        let Some((start, end)) = self.get_node_span(mapped_idx) else {
            return;
        };
        self.error(
            start,
            end.saturating_sub(start),
            diagnostic_messages::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE.to_string(),
            diagnostic_codes::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
        );
    }

    /// Push a diagnostic with deduplication.
    /// Diagnostics with the same (start, code) are only emitted once.
    /// Exceptions:
    /// - TS2318 (missing global type) at position 0 uses message hash to allow multiple distinct
    ///   global type errors.
    /// - TS2411 uses (start ^ `message_hash`, code) to allow a single property to
    ///   report both string and number index incompatibilities.
    /// - TS2416 (incorrectly extends/implements property type) uses (start ^ `message_hash`,
    ///   code) to allow distinct per-base diagnostics at the same property position
    ///   (e.g., a class that both extends a base and implements an interface where the
    ///   same property is incompatible against both).
    /// - TS2430 (incorrectly extends interface) uses (start ^ `message_hash`, code) to allow
    ///   multiple per-base diagnostics at the same interface name position.
    /// - TS4094 uses (start ^ `message_hash`, code) so each private/protected member of an
    ///   exported anonymous class expression emits its own diagnostic at the owning
    ///   variable/function name span.
    pub fn push_diagnostic(&mut self, diag: Diagnostic) {
        if self.reconcile_name_resolution_precedence(diag.start, diag.code) {
            return;
        }
        if diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE {
            let diag_end = diag.start.saturating_add(diag.length);
            if self
                .diagnostic_indices
                .has_excess_property_position_in(diag.start, diag_end)
            {
                return;
            }
            if self.diagnostic_indices.has_overlapping_ts2322(
                &diag.message_text,
                diag.start,
                diag_end,
            ) {
                return;
            }
        }

        let key = self.diagnostic_dedup_key(&diag);

        if self.diagnostic_indices.emitted.contains(&key) {
            // The dedup key already exists. When this candidate ties on
            // location/code/message but carries different related information,
            // pick the surviving entry deterministically instead of keeping
            // whichever relation path produced its diagnostic first.
            self.reconcile_related_information_collision(diag);
            return;
        }
        self.diagnostic_indices.emitted.insert(key);
        tracing::debug!(
            code = diag.code,
            start = diag.start,
            length = diag.length,
            file = %diag.file,
            message = %diag.message_text,
            "diagnostic"
        );
        self.diagnostic_indices.update_aux_for(
            diag.code,
            diag.start,
            diag.start.saturating_add(diag.length),
            &diag.message_text,
        );
        self.diagnostics.push(diag);
    }

    /// Read-only view of the diagnostics emitted since the buffer held `start`
    /// entries.
    ///
    /// Checker paths that inspect their own freshly emitted diagnostics (to
    /// decide a downgrade, detect an excess-property report, etc.) should go
    /// through this accessor rather than indexing `diagnostics` directly, so
    /// the buffer's `Vec` representation stays an implementation detail of the
    /// context. `start` is clamped, so a speculative shrink between capture and
    /// read cannot panic the slice.
    pub(crate) fn recent_diagnostics(&self, start: usize) -> &[Diagnostic] {
        &self.diagnostics[start.min(self.diagnostics.len())..]
    }

    /// Finalize the diagnostics emitted since the buffer held `start` entries.
    ///
    /// Some checker paths can only compute a diagnostic's final span, code, or
    /// message after the sub-check that emitted it has run — for example,
    /// repositioning a missing-property error onto an initializer anchor, or
    /// downgrading TS2739 to TS2322 once the relation outcome is known. Routing
    /// every such finalization through this one boundary keeps the buffer's
    /// `Vec` representation an implementation detail of the context instead of
    /// scattering raw `diagnostics[start..]` index patching across the checker,
    /// and confines the "the tail may still be rewritten" window to a single,
    /// documented place. `start` is clamped, so a speculative shrink between
    /// emission and finalization cannot panic the slice.
    ///
    /// The `diagnostic_indices` aux state is intentionally left untouched: the
    /// tail was already deduplicated and indexed when it was first pushed, so
    /// finalizing in place reproduces exactly what the prior scattered in-place
    /// patching produced.
    pub(crate) fn finalize_recent_diagnostics(
        &mut self,
        start: usize,
        mut finalize: impl FnMut(&mut Diagnostic),
    ) {
        let len = self.diagnostics.len();
        for diag in &mut self.diagnostics[start.min(len)..] {
            finalize(diag);
        }
    }

    /// Reconcile precedence between the name-resolution diagnostics that can
    /// collide at one span: TS2301 ("Initializer of instance member cannot
    /// reference identifier declared in constructor") outranks TS2304 ("Cannot
    /// find name"), TS2552 ("Cannot find name ... Did you mean?"), and TS2663
    /// ("Did you mean the instance member 'this.X'?") because it explains the
    /// problem more precisely; TS2552/TS2663 name suggestions outrank the
    /// generic TS2304.
    ///
    /// Shared by both emission entry points ([`Self::error`] and
    /// [`Self::push_diagnostic`]) so the precedence rules cannot drift.
    /// Returns `true` when the incoming diagnostic is outranked and must be
    /// dropped; lower-precedence diagnostics already emitted at the same span
    /// are evicted as a side effect.
    fn reconcile_name_resolution_precedence(&mut self, start: u32, code: u32) -> bool {
        let emitted = &self.diagnostic_indices.emitted;
        match code {
            2304 | 2552 | 2663 => {
                if emitted.contains(&(start, 2301)) {
                    return true;
                }
                if code == 2304
                    && (emitted.contains(&(start, 2552)) || emitted.contains(&(start, 2663)))
                {
                    return true;
                }
            }
            _ => {}
        }
        let evict: &[u32] = match code {
            2301 => &[2304, 2552, 2663],
            2552 | 2663 => &[2304],
            _ => return false,
        };
        self.diagnostics
            .retain(|diag| !(diag.start == start && evict.contains(&diag.code)));
        for &evicted in evict {
            self.diagnostic_indices.emitted.remove(&(start, evicted));
        }
        false
    }
}

#[cfg(test)]
mod collision_reconciliation_tests {
    use super::prefers_candidate_diagnostic;
    use crate::diagnostics::Diagnostic;

    fn base() -> Diagnostic {
        Diagnostic::error(
            "a.ts",
            10,
            4,
            "Type 'A' is not assignable to type 'B'.",
            2322,
        )
    }

    #[test]
    fn richer_related_information_is_preferred() {
        let lean = base();
        let rich = base().with_related("a.ts", 0, 1, "'B' is declared here.");
        // The richer elaboration wins regardless of which side is the incumbent.
        assert!(prefers_candidate_diagnostic(&lean, &rich));
        assert!(!prefers_candidate_diagnostic(&rich, &lean));
    }

    #[test]
    fn equal_length_related_breaks_tie_canonically() {
        // Two equally rich elaborations that blame different "see also"
        // locations must resolve to the canonically smaller one, independent of
        // which was produced first.
        let earlier = base().with_related("a.ts", 5, 1, "'A' is declared here.");
        let later = base().with_related("a.ts", 99, 1, "'A' is declared here.");
        assert!(prefers_candidate_diagnostic(&later, &earlier));
        assert!(!prefers_candidate_diagnostic(&earlier, &later));
    }

    #[test]
    fn identical_diagnostics_do_not_replace() {
        let a = base().with_related("a.ts", 5, 1, "'A' is declared here.");
        let b = base().with_related("a.ts", 5, 1, "'A' is declared here.");
        assert!(!prefers_candidate_diagnostic(&a, &b));
        assert!(!prefers_candidate_diagnostic(&b, &a));
    }

    #[test]
    fn selection_is_order_independent() {
        // Whichever order three equivalent-but-distinct diagnostics arrive in,
        // the deterministic winner is the same one.
        let mut variants = [
            base().with_related("a.ts", 30, 1, "'A' is declared here."),
            base().with_related("a.ts", 10, 1, "'A' is declared here."),
            base().with_related("a.ts", 20, 1, "'A' is declared here."),
        ];
        let fold_winner = |order: &[Diagnostic]| -> Diagnostic {
            let mut winner = order[0].clone();
            for cand in &order[1..] {
                if prefers_candidate_diagnostic(&winner, cand) {
                    winner = cand.clone();
                }
            }
            winner
        };
        let baseline = fold_winner(&variants);
        variants.reverse();
        assert_eq!(fold_winner(&variants), baseline);
        variants.swap(0, 2);
        assert_eq!(fold_winner(&variants), baseline);
        // The canonical winner is the smallest related position.
        assert_eq!(baseline.related_information[0].start, 10);
    }
}
