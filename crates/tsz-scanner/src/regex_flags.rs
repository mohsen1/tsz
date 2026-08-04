//! Shared positional classification for TypeScript regex literal flags.
//!
//! `tsc`'s `scanRegularExpressionWorker` walks a regex literal's trailing
//! flag run left to right and, for each flag character, decides in this
//! priority order: an incompatible Unicode-mode conflict (a `u` after an
//! already-seen `v`, or a `v` after an already-seen `u`) wins over a plain
//! duplicate, which wins over acceptance. The scanner's TS1499/1500/1502
//! diagnostics and the checker's target-gated TS1501 (`s`/`d`/`v` need a
//! later `target`) both need this same per-position verdict — TS1501 fires
//! only on an `Accepted` occurrence, never on one that lost to a conflict or
//! duplicate — so it is computed once here instead of independently in each
//! layer.

/// What tsc's positional flag scan decides for one flag character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegexFlagVerdict {
    /// First time this flag is seen, and, for `u`/`v`, the opposite Unicode
    /// mode flag has not been seen yet. The only verdict that reaches a
    /// target version gate.
    Accepted,
    /// A `u` following an already-seen `v`, or a `v` following an
    /// already-seen `u`. Wins over `Duplicate` even when this exact flag
    /// character was also already seen.
    Conflict,
    /// This exact flag character was already seen, and it is not itself a
    /// `u`/`v` conflict.
    Duplicate,
}

/// Tracks `u`/`v`/duplicate state across an ordered sequence of valid regex
/// flag characters (`g i m s u v y d`), one call to `advance` per flag.
#[derive(Default)]
pub struct RegexFlagScan {
    seen: u8,
    has_u: bool,
    has_v: bool,
}

impl RegexFlagScan {
    pub fn new() -> Self {
        Self::default()
    }

    const fn bit(flag: u8) -> Option<u8> {
        match flag {
            b'g' => Some(0),
            b'i' => Some(1),
            b'm' => Some(2),
            b's' => Some(3),
            b'u' => Some(4),
            b'v' => Some(5),
            b'y' => Some(6),
            b'd' => Some(7),
            _ => None,
        }
    }

    /// Feed the next valid flag character (the caller has already filtered
    /// out non-flag/invalid characters) and get tsc's verdict for it.
    pub fn advance(&mut self, flag: u8) -> RegexFlagVerdict {
        let verdict = if (flag == b'u' && self.has_v) || (flag == b'v' && self.has_u) {
            RegexFlagVerdict::Conflict
        } else if Self::bit(flag).is_some_and(|bit| self.seen & (1 << bit) != 0) {
            RegexFlagVerdict::Duplicate
        } else {
            RegexFlagVerdict::Accepted
        };

        if let Some(bit) = Self::bit(flag) {
            self.seen |= 1 << bit;
        }
        match flag {
            b'u' => self.has_u = true,
            b'v' => self.has_v = true,
            _ => {}
        }

        verdict
    }
}
