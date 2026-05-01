# Loop Productivity Strategy (Conformance Grinding)

This doc captures lessons from a 45-iter session where most iterations
produced nothing while a handful unlocked 13+ tests each. Read at the
start of every iteration before sampling.

## Decisive rule

**Each iteration MUST commit to ONE candidate.** Investigate it
through to either (a) a shipped PR, or (b) a documented dead-end with
the specific reason it's blocked. Do not skip without writing the
reason.

Sampling 8-10 candidates and "all look complex, skip" is the unproductive
pattern. The +13 PR (#2134) and the +5 PR (#2109) both came from
forcing through what initially looked like "yet another display fix".

## Pick by leverage, not by ease

Three priority axes (in order):

1. **Pattern reach** — when fixing one test, how many *other* tests
   share the same pattern? The +13 #2134 fix targeted one test but
   unlocked 12 incidentals. Search the failure pool for the same
   error-code + message-shape signature **before** ranking the
   candidate.
2. **Diff=1 + diff=2** — single-fingerprint failures with one missing
   or one extra are usually one fix. diff>2 means multiple
   intertwined bugs.
3. **Recently-introduced failures** — if a test passed last week and
   fails now, the regression has a small blast radius. Use
   `git log -p crates/tsz-checker -p crates/tsz-solver` to spot
   recent changes touching the failure's area.

## Selection process

```bash
# Pattern reach: count failures matching a keyword
git rev-parse --short HEAD                                   # cache key
./scripts/conformance/conformance.sh run 2>&1 | tail -200 \
  | grep -E "TS\d+ test\.ts:" \
  | sed -E 's/.* (does not exist on type .*)\..*/\1/' \
  | sort | uniq -c | sort -rn | head -15
```

Then for the top families: pick a representative test, dig.

## Investigation discipline

- **Trace, don't guess.** Add `eprintln!` to suspect functions, run a
  minimal repro under `/tmp`, watch the actual TypeIds and code-paths
  hit. Five minutes of tracing beats fifty minutes of speculation.
- **Don't widen the filter.** When you find the bake / display site,
  fix it there, not by adding a flag that propagates through the
  whole instantiator. The "shallow_this_only" path needed three
  iterations of refinement; the "carve out the body field only" was
  one.
- **One PR per fix.** Even if you spot two unrelated bugs in the same
  trace, ship one and write a follow-up doc for the second.

## Ship-or-document threshold

If after 30 minutes of digging you haven't:
1. Reproduced the failure on a `/tmp/<test>.ts` file, and
2. Found at least one specific function where the wrong behavior is
   visible in a trace,

then **stop and write a `docs/plan/claims/investigation-<bug>.md`**
with what you traced, what you ruled out, and the next concrete
action for whoever picks it up. The investigation doc is itself a
shippable artifact.

Do not switch to a new candidate without writing the investigation.

## Dead-end exit

When the fix needs cross-system surgery (e.g., changes to binder +
solver + checker simultaneously), explicitly classify it:

- **Day-scale**: not appropriate for the loop. Document and surface to
  the user as a triage candidate, then pick a different test.
- **Iteration-scale**: keep going, even if it's iteration N+1 or N+2.
  Drop a `docs/plan/claims/investigation-<bug>.md` checkpoint at end
  of the iteration so the next firing has the context.

## Anti-patterns observed

- Sampling more candidates instead of digging into one.
- "Net 0" iterations where nothing changes — stop scheduling, write a
  status update to the user instead.
- Re-running `quick-pick.sh` instead of consulting prior session
  notes for already-triaged tests.
- "Skip — too deep" without writing what specifically was deep.
- Single-line fix attempts before reading the failure trace.

## Loop housekeeping

- **Don't ship chore-only PRs.** Doc cleanup, claim-doc flips, etc.
  do not move conformance and dilute the session's signal-to-PR ratio.
- **Cadence**: 5 minutes after a successful fix (let CI roll), 25
  minutes if the iteration produced no PR (let other agents land work
  that may surface new candidates), 25 min if disk-full or other
  infra-blocked.
- **End-of-iter summary must include**: the candidate considered,
  the trace finding, the decision (ship / write investigation /
  classify as day-scale), and net conformance impact.
