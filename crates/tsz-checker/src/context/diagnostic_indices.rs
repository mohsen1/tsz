use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use crate::diagnostics::{Diagnostic, diagnostic_codes};

#[derive(Clone, Default)]
pub(crate) struct DiagnosticIndices {
    pub(crate) emitted: FxHashSet<(u32, u32)>,
    ts2353_2561_positions: BTreeSet<u32>,
    ts2322_msg_spans: FxHashMap<u64, Vec<(u32, u32)>>,
}

impl DiagnosticIndices {
    pub(crate) fn clear(&mut self) {
        self.emitted.clear();
        self.clear_aux();
    }

    pub(crate) fn rebuild_aux_from(&mut self, diagnostics: &[Diagnostic]) {
        self.clear_aux();
        for diag in diagnostics {
            self.update_aux_for(
                diag.code,
                diag.start,
                diag.start.saturating_add(diag.length),
                &diag.message_text,
            );
        }
    }

    pub(crate) fn update_aux_for(&mut self, code: u32, start: u32, end: u32, message: &str) {
        match code {
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
            | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID => {
                self.ts2353_2561_positions.insert(start);
            }
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE => {
                let hash = Self::ts2322_message_hash(message);
                self.ts2322_msg_spans
                    .entry(hash)
                    .or_default()
                    .push((start, end));
            }
            _ => {}
        }
    }

    pub(crate) fn has_excess_property_position_in(&self, start: u32, end: u32) -> bool {
        self.ts2353_2561_positions
            .range(start..end)
            .next()
            .is_some()
    }

    pub(crate) fn has_overlapping_ts2322(&self, message: &str, start: u32, end: u32) -> bool {
        let hash = Self::ts2322_message_hash(message);
        self.ts2322_msg_spans.get(&hash).is_some_and(|spans| {
            spans
                .iter()
                .any(|&(s, e)| (s <= start && e >= end) || (start <= s && end >= e))
        })
    }

    fn clear_aux(&mut self) {
        self.ts2353_2561_positions.clear();
        self.ts2322_msg_spans.clear();
    }

    fn ts2322_message_hash(message: &str) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        message.hash(&mut hasher);
        hasher.finish()
    }
}
