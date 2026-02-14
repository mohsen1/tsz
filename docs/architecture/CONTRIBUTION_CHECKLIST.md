# Architecture Contribution Checklist

Every semantic PR should answer these before merge.

1. Is this change `WHAT` (type algorithm) or `WHERE` (orchestration/diagnostic location)?
2. If `WHAT`, does implementation live in solver/query helpers instead of checker-local logic?
3. If `WHERE`, does checker call existing relation/query boundaries instead of re-implementing semantics?
4. Does the change preserve DefId-first resolution (`Lazy(DefId)` + `TypeEnvironment`)?
5. Are TS2322/TS2345/TS2416-family diagnostics routed through centralized gateways?
6. Are weak-type/excess-property/any-propagation behaviors explicit and policy-owned?
7. Did you avoid solver-internal imports from checker (`TypeKey`, `tsz_solver::types::*`)?
8. Did you avoid raw type construction/interner calls in checker?
9. What parity impact is expected (diagnostic code families and scenarios)?
10. Which architecture guard checks cover this change?
