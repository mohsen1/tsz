export const meta = {
  name: 'grow-canary-draft-extends',
  description: 'Draft high-quality issues for confirmed clusters that extend CLOSED issues (incomplete prior fixes), incorporating verifier corrections',
  phases: [{ title: 'Draft', detail: 'one perf-aware issue per confirmed extends-closed cluster' }],
}

const A = typeof args === 'string' ? JSON.parse(args) : args
const { binary, repoRoot, items } = A

const ISSUE = {
  type: 'object',
  required: ['title', 'body', 'labels'],
  properties: {
    title: { type: 'string' },
    body: { type: 'string' },
    labels: { type: 'array', items: { type: 'string' } },
  },
}

phase('Draft')
const issues = (await parallel(
  items.map((it) => () => agent(
    `Write a very high-quality GitHub issue for this CONFIRMED tsz root-cause cluster. It will be filed in tsz-org/tsz. Resolving it must move the listed real-world corpus project(s) toward compiling like tsc.

IMPORTANT CONTEXT: this cluster EXTENDS a prior issue that was already CLOSED by an INCOMPLETE fix: ${it.parentIssue}. Your issue must (a) reference that prior issue, (b) make crystal-clear what the prior fix covered vs what is STILL broken (the new structural condition), and (c) NOT re-file the already-fixed part.

CLUSTER (from analysis):
${JSON.stringify(it.cluster, null, 1)}

VERIFIER VERDICT + CORRECTIONS (AUTHORITATIVE — the cluster's own repros/claims had errors; the verifier RAN the binary and corrected them. Use the corrected repros and framing, NOT the cluster's originals where they conflict):
${JSON.stringify(it.verdict, null, 1)}

Fresh tsz binary (to re-derive/verify a clean repro if needed): ${binary}
tsz repo: ${repoRoot}

Before writing, RE-VERIFY the corrected minimal repro yourself: write it to a temp file and run \`${binary} --noEmit --strict --target es2022 --lib es2022,dom <file>\`. Use the repro that actually reproduces on the binary and is accepted by tsc. If the verifier flagged the cluster's repro as broken, use the verifier's corrected repro.

The issue body MUST contain, in this order:
- "Goal: green" on the first line (or "Goal: green/fast" if primarily perf).
- One-paragraph summary: which corpus project(s) are blocked, the symptom (count of false TS#### or timeout), that it reproduces on the current binary and tsc accepts; and ONE sentence on how it relates to the closed parent issue (what was fixed there vs what remains).
- "## Structural rule" — "When <structural condition>, tsc does X; tsz does Y via <owner layer>." Name the exact wrong semantic operation, owning layer, and the specific source site(s) from the finding (file:line in the tsz compiler).
- "## Minimal repro" — the VERIFIED standalone repro in a \`\`\`ts block, the exact command, the ACTUAL tsz output, and the expected tsc behavior. Include both the minimal form and (if helpful) the faithful library-shaped form.
- "## Relationship to ${it.parentIssue}" — precisely what the prior fix covered and why this case slips through (different code path / cross-file vs same-file / concrete-tuple vs mapped, per the verifier). Mention any in-flight branches that do NOT cover this.
- "## Affected corpus rows" — the project(s), with error counts and pinned-ref context.
- "## Adjacent cases to cover" — variants a correct fix must handle (renamed binders, alias/wrapper/nesting, generic vs concrete, positive + negative/fallback).
- "## Suggested fix (performance-aware)" — where to fix, and CRITICALLY the performance plan: complexity, the cache key / memoization / fuel / depth-guard, invalidation, and how to avoid a perf regression. A fix that compiles the project but regresses throughput is not acceptable. Use the verifier's perfNote/corrections.
- "## Verification" — exact project-compile-guard command (TSZ_PROJECT_COMPILE_SET=canary TSZ_PROJECT_COMPILE_FILTER='^<name>-project$') and targeted unit/conformance filters.

Anti-hardcoding: the fix must be structural, never keyed on a project/file/identifier name.

Return title, body (full markdown), labels (from: bug, false-positive, performance, checker, solver, type-inference, conditional-types, mapped-types, narrowing, module-resolution, grow). Always include "grow".`,
    { label: `draft:${it.cluster.clusterKey}`, phase: 'Draft', schema: ISSUE }
  ))
)).filter(Boolean)

return { issues }
