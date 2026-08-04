//! tsc's `getAwaitedTypeNoAlias`, the awaited-type walk TS1064's
//! `Did you mean to write 'Promise[T]'?` suggestion is formatted from.
//!
//! A child module rather than a sibling: it reads `ThenableAwaitInfo` and
//! `extract_awaited_type_from_valid_thenable`, which are private to the parent
//! and stay that way. It lives here rather than in `promise_checker.rs`
//! because that file sits against the 2000-line architecture cap.

use super::MAX_THENABLE_THIS_VALIDATION_DEPTH;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// tsc's `getAwaitedTypeNoAlias(t)`.
    ///
    /// A type that is not thenable is its own awaited type; a thenable is
    /// awaited recursively, since a `then` callback whose payload is itself
    /// thenable keeps unwrapping. `None` marks tsc's `undefined` result — a
    /// thenable whose `getPromisedTypeOfPromiseEx` still yields nothing —
    /// which the diagnostic callers render as `void`, matching tsc's
    /// `|| voidType`.
    pub(crate) fn awaited_type_no_alias(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.awaited_type_no_alias_with_depth(type_id, 0)
    }

    fn awaited_type_no_alias_with_depth(&mut self, type_id: TypeId, depth: u8) -> Option<TypeId> {
        if depth > MAX_THENABLE_THIS_VALIDATION_DEPTH {
            return Some(type_id);
        }
        // Unwrap the `Promise`/`PromiseLike` applications and distribute over
        // unions and intersections first, so only a user-written thenable
        // reaches the payload extraction below.
        let unwrapped = self.compute_awaited_type(type_id, 0);
        let info = self.extract_awaited_type_from_valid_thenable(unwrapped, true);
        match info.awaited_type {
            Some(payload) if payload != unwrapped => {
                self.awaited_type_no_alias_with_depth(payload, depth + 1)
            }
            // Thenable, yet no fulfillment payload survives: tsc's `undefined`
            // result. Everything else — a thenable that already awaits to
            // itself, and a type that is not thenable at all — is its own
            // awaited type.
            None if info.is_thenable => None,
            _ => Some(unwrapped),
        }
    }
}
