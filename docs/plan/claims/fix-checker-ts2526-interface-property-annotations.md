**2026-04-26 23:15:00** TS2526 inside nested type literals on interface property/call/construct/index signatures was missed because `get_type_of_interface` lowers those annotations through TypeLowering, which silently maps `this` to `ThisType` and never reaches the checker's THIS_TYPE branch where TS2526 is emitted.

**2026-04-26 23:15:00** Fix: in `check_interface_declaration`, mirror what we already do for METHOD_SIGNATURE/accessor — eagerly resolve property/call/construct/index signature annotations through `CheckerState::get_type_from_type_node`, which delegates to `TypeNodeChecker` and walks nested type literals. The walker dispatches THIS_TYPE → emits TS2526 when `is_this_type_allowed` returns false.

**2026-04-26 23:15:00** Verified: `thisTypeErrors.ts` flips fingerprint-only → PASS. Adds 5 missing TS2526 fingerprints for interface I1's `a..e` properties. No churn in solver — pure checker orchestration fix at the §3 WHERE layer.
