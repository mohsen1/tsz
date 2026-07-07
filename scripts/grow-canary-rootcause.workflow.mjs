export const meta = {
  name: 'grow-canary-rootcause',
  description: 'Deep root-cause analysis of failing grow-corpus canary projects; cluster by shared tsz shortcoming; draft perf-aware GitHub issues',
  phases: [
    { title: 'Analyze', detail: 'one agent per failing project: reduce to minimal repro, identify wrong semantic operation' },
    { title: 'Cluster', detail: 'group findings into shared root causes, cross-ref existing issues/PRs' },
    { title: 'Verify', detail: 'adversarially verify each cluster: repro minimal+reproduces, projects truly share root, tsc behavior correct, not a dup' },
    { title: 'Draft', detail: 'one high-quality issue per confirmed non-dup cluster, perf-aware fix' },
  ],
}

// args = {
//   binary: abs path to fresh dist-fast tsz,
//   guardRoot: abs path to .target/project-compile-guard (holds <name>.log + cloned fixtures + project-compatibility.jsonl),
//   repoRoot: abs path to the tsz worktree (for tsz source + scripts),
//   projects: [{ name, srcDir, log, exitClass, firstBlocker, tsconfig }],
//   openIssues: "free text: open issues + in-flight branches to cross-ref for dup detection",
// }
// args may arrive as an object or as a JSON-encoded string depending on the
// tool-call plumbing; parse defensively.
const A = typeof args === 'string' ? JSON.parse(args) : args
const { binary, guardRoot, repoRoot, projects, openIssues } = A

const FINDING = {
  type: 'object',
  required: ['project', 'exitClass', 'wrongOperation', 'owningLayer', 'minimalRepro', 'reproConfirmed', 'tscBehavior', 'suspectedClusterKey', 'confidence'],
  properties: {
    project: { type: 'string' },
    exitClass: { type: 'string', enum: ['crash-stackoverflow', 'crash-other', 'timeout', 'diagnostic-fp', 'diagnostic-missing-dep', 'mixed'] },
    firstBlockerCodes: { type: 'array', items: { type: 'string' } },
    wrongOperation: { type: 'string', description: 'relation|inference|narrowing|conditional-eval|mapped-eval|indexed-access-eval|template-eval|infer-eval|property-lookup|symbol-resolution|instantiation|recursion-guard|union-distribution|variance|diagnostic-display|parser-recovery|emit|other' },
    owningLayer: { type: 'string', enum: ['solver', 'checker', 'binder', 'parser', 'scanner', 'emitter', 'module-resolver', 'unknown'] },
    minimalRepro: { type: 'string', description: 'standalone .ts source that reproduces the failure on the binary; <40 lines if possible' },
    reproConfirmed: { type: 'boolean', description: 'did running the binary on minimalRepro reproduce the same failure class?' },
    reproObserved: { type: 'string', description: 'what the binary actually emitted on the repro (diagnostic text / panic / timed-out)' },
    tscBehavior: { type: 'string', description: 'what tsc does with the repro: accepts | rejects with TS#### — state how you know' },
    perfNote: { type: 'string', description: 'performance dimension: repeated operation, complexity, missing memo/cache/fuel; required for timeouts/crashes' },
    suspectedClusterKey: { type: 'string', description: 'short kebab slug for the shared root cause, e.g. cond-type-distribution-blowup' },
    relatedExistingIssue: { type: 'string', description: 'open issue # or in-flight branch this matches, or "none"' },
    evidence: { type: 'string', description: 'key log excerpt (file:line + diagnostic) anchoring the finding' },
    confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
  },
}

function analyzePrompt(p) {
  return `You are root-causing why the tsz TypeScript compiler fails to compile a real-world project. tsz aims to match \`tsc\` EXACTLY.

PROJECT: ${p.name}
Exit class from survey: ${p.exitClass}
First-blocker hint: ${p.firstBlocker || '(see log)'}
Fresh tsz binary: ${binary}
Compile log: ${p.log}
Project compatibility record: ${guardRoot}/project-compatibility.jsonl (grep for "${p.name}")
Cloned source root: ${guardRoot}/${p.name}  (compiled source under ${p.srcDir})
tsconfig used: ${p.tsconfig}
tsz repo (for tracing/source of the compiler): ${repoRoot}

YOUR JOB — produce ONE precise root-cause finding:
1. Read the compile log. Identify the FIRST real blocker (ignore TS2307 missing-dep / TS5097 config noise unless that is genuinely all there is — classify those as diagnostic-missing-dep). For crashes, find where it overflows/aborts; for timeouts, find which file/construct is non-terminating.
2. Open the actual source at the failing location(s). Understand the TypeScript construct: which exact type-level operation is involved (conditional, mapped, indexed-access, infer, template-literal, overload resolution, declaration-merge, variance, recursion, etc).
3. State the WRONG SEMANTIC OPERATION precisely (per the tsz bug discipline): "When <structural condition>, tsc does X; tsz does Y via <owner layer>." The reported witness is one instance, not the scope — find the general rule.
4. REDUCE to a minimal standalone .ts repro (aim <40 lines, no external deps). Write it to a temp file and RUN the binary: \`${binary} --noEmit --strict <file>\` (add --target es2022 --lib es2022,dom if needed). Confirm it reproduces the same failure class. Iterate to shrink it. For crashes, run with \`RUST_MIN_STACK=536870912\`; if it still overflows, that confirms unbounded (not just deep) recursion. For timeouts, find the smallest input whose compile time grows super-linearly (try scaling the input 2x and observe time blow-up).
5. Determine what tsc actually does with your minimal repro. If you have a tsc/tsgo available use it; otherwise reason from the TypeScript spec + your knowledge of these well-known libraries (they compile cleanly under their own tsc). State your basis.
6. PERFORMANCE LENS (critical): for crashes/timeouts, characterize the repeated operation and its complexity, and what cache/memo/fuel/depth-guard is missing or mis-keyed. For false-positives, note whether the correct fix risks a perf regression (e.g. more instantiation) and how to avoid it.
7. Optionally use \`TSZ_LOG\` tracing (see ${repoRoot} tracing docs) to pinpoint the solver/checker path, but only if it accelerates root-causing.

Anti-hardcoding: the root cause must be a structural/semantic rule, never a project-name or file-name special case.

Return the structured finding. Be exact about wrongOperation, owningLayer, and a confirmed minimalRepro. If you genuinely cannot reduce a minimal repro, set reproConfirmed=false and explain in reproObserved, but still give your best structural hypothesis.`
}

phase('Analyze')
const findings = (await parallel(
  projects.map((p) => () => agent(analyzePrompt(p), {
    label: `analyze:${p.name}`,
    phase: 'Analyze',
    schema: FINDING,
  }))
)).filter(Boolean)

log(`Analyzed ${findings.length}/${projects.length} projects`)

const CLUSTERS = {
  type: 'object',
  required: ['clusters'],
  properties: {
    clusters: {
      type: 'array',
      items: {
        type: 'object',
        required: ['clusterKey', 'title', 'projects', 'sharedRootCause', 'owningLayer', 'wrongOperation', 'dupStatus'],
        properties: {
          clusterKey: { type: 'string' },
          title: { type: 'string', description: 'proposed issue title' },
          projects: { type: 'array', items: { type: 'string' } },
          sharedRootCause: { type: 'string', description: 'the one structural shortcoming all listed projects hit' },
          owningLayer: { type: 'string' },
          wrongOperation: { type: 'string' },
          representativeRepro: { type: 'string', description: 'the cleanest minimal repro covering the cluster' },
          perfDimension: { type: 'string' },
          dupStatus: { type: 'string', description: 'NEW | dup-of-#### | extends-#### | covered-by-branch:<name>' },
          severity: { type: 'string', enum: ['blocks-compile-crash', 'blocks-compile-timeout', 'blocks-compile-fp', 'noise-config'] },
        },
      },
    },
    singletons: { type: 'array', items: { type: 'string' }, description: 'projects whose failure did not cluster, with one-line reason' },
  },
}

phase('Cluster')
const clustering = await agent(
  `You are clustering root-cause findings from ${findings.length} failing tsz canary projects into SHARED tsz shortcomings. Many projects fail for the SAME underlying reason — group them so that fixing one cluster makes multiple projects compile.

FINDINGS (JSON):
${JSON.stringify(findings, null, 1)}

EXISTING OPEN ISSUES + IN-FLIGHT WORK to cross-reference for dup/extends:
${openIssues}

Produce clusters where each cluster is ONE structural root cause shared by >=1 project (prefer merging; only keep a singleton if its root genuinely differs). For each cluster:
- clusterKey, proposed issue title (specific, structural — name the operation and the symptom)
- the projects it covers
- the shared root cause as a structural rule ("When <cond>, tsc does X; tsz does Y in <layer>")
- representative minimal repro (pick/merge the cleanest from the findings)
- performance dimension (complexity, cache/fuel, regression risk of the fix)
- dupStatus vs the existing issues/branches (NEW / dup-of-#### / extends-#### / covered-by-branch)
- severity

Separate genuine tsz bugs from config/missing-dep noise (mark severity noise-config). Be conservative about declaring two projects the same root cause — only cluster when the wrongOperation + structural condition genuinely match.`,
  { label: 'cluster', phase: 'Cluster', schema: CLUSTERS }
)

const clusters = (clustering.clusters || []).filter((c) => c.severity !== 'noise-config')
log(`Formed ${clusters.length} non-noise clusters (+${(clustering.clusters || []).length - clusters.length} noise)`)

const VERDICT = {
  type: 'object',
  required: ['clusterKey', 'reproReproduces', 'projectsTrulyShare', 'tscBehaviorCorrect', 'isDup', 'verdict'],
  properties: {
    clusterKey: { type: 'string' },
    reproReproduces: { type: 'boolean', description: 'did the representative repro actually reproduce on the binary when you ran it?' },
    reproObserved: { type: 'string' },
    projectsTrulyShare: { type: 'boolean', description: 'do the listed projects genuinely share this exact root cause? remove any that do not' },
    correctedProjects: { type: 'array', items: { type: 'string' } },
    tscBehaviorCorrect: { type: 'boolean' },
    isDup: { type: 'string', description: 'NEW | dup-of-#### | extends-####' },
    corrections: { type: 'string', description: 'any corrections to the cluster (root cause, layer, repro, perf claim)' },
    verdict: { type: 'string', enum: ['confirmed', 'confirmed-with-corrections', 'rejected'] },
  },
}

phase('Verify')
const verified = (await parallel(
  clusters.map((c) => () => agent(
    `Adversarially verify this proposed tsz root-cause cluster. Default to skepticism — try to REFUTE it.

CLUSTER:
${JSON.stringify(c, null, 1)}

Fresh tsz binary: ${binary}
tsz repo: ${repoRoot}
Cloned fixtures under: ${guardRoot}/<project>

Checks (run them, don't assume):
1. Write the representativeRepro to a temp file and RUN \`${binary} --noEmit --strict <file>\`. Does it ACTUALLY reproduce the claimed failure? Record exactly what the binary emitted.
2. Is the repro truly minimal, or does it conflate two issues? Shrink/split if needed.
3. Do the listed projects GENUINELY share this exact root cause? Spot-check at least the риskiest 1-2 by looking at their actual failing source. Remove any that don't fit.
4. Is the stated tsc behavior correct (tsc accepts / rejects as claimed)? These are well-known libraries that compile under their own tsc.
5. Is it a duplicate of an open issue/branch (${openIssues})?
6. Is the performance claim sound?

Return a verdict with corrections. If the repro does not reproduce on the binary, verdict=rejected.`,
    { label: `verify:${c.clusterKey}`, phase: 'Verify', schema: VERDICT }
  ))
)).filter(Boolean)

const verdictByKey = Object.fromEntries(verified.map((v) => [v.clusterKey, v]))
const confirmed = clusters
  .map((c) => ({ cluster: c, verdict: verdictByKey[c.clusterKey] }))
  .filter((x) => x.verdict && x.verdict.verdict !== 'rejected' && x.verdict.isDup === 'NEW')

log(`Confirmed ${confirmed.length} NEW clusters for issue drafting`)

const ISSUE = {
  type: 'object',
  required: ['title', 'body', 'labels'],
  properties: {
    title: { type: 'string' },
    body: { type: 'string', description: 'full GitHub issue markdown body' },
    labels: { type: 'array', items: { type: 'string' } },
  },
}

phase('Draft')
const issues = (await parallel(
  confirmed.map(({ cluster, verdict }) => () => agent(
    `Write a very high-quality GitHub issue for this CONFIRMED tsz root-cause cluster. It will be filed in tsz-org/tsz. Resolving it must make the listed real-world projects compile.

CLUSTER (after verification):
${JSON.stringify(cluster, null, 1)}

VERIFIER CORRECTIONS (authoritative — apply them):
${JSON.stringify(verdict, null, 1)}

The issue body MUST contain, in this order:
- "Goal: green" (these are corpus rows blocked from compiling) on the first line. If it is primarily a perf/non-termination blocker, use "Goal: green/fast".
- One-paragraph summary: which projects are blocked and the user-visible symptom (crash / >Ns timeout / N false TS#### errors).
- "## Structural rule" — "When <structural condition>, tsc does X; tsz does Y via <owner layer>." Name the exact wrong semantic operation and owning layer.
- "## Minimal repro" — the verified standalone repro in a \`\`\`ts block, plus the exact command and the ACTUAL tsz output vs expected tsc behavior.
- "## Affected corpus rows" — bullet list of the projects (with pinned-ref context) this unblocks.
- "## Adjacent cases to cover" — variants a correct fix must handle (renamed binders, alias/wrapper/nesting, generic vs concrete, positive + negative/fallback) per tsz bug discipline.
- "## Suggested fix (performance-aware)" — where in the solver/checker to fix, and CRITICALLY the performance plan: state complexity, the cache key / memoization / fuel / depth-guard involved, invalidation, and how to avoid a perf regression. Performance is a first-class requirement: a fix that makes the project compile but regresses throughput is not acceptable. If the bug is itself a perf bug (timeout/crash), give the algorithmic fix (memoize the repeated sub-evaluation, bound the recursion, cache by canonical key) with expected complexity change.
- "## Verification" — the exact project-compile-guard command and the targeted unit/conformance filters that would prove the fix.

Anti-hardcoding: the fix must be structural, never keyed on a project/file/identifier name.

Return title, body (full markdown), and labels (choose from: bug, false-positive, performance, panic, checker, solver, type-inference, conditional-types, mapped-types, grow). Always include "grow".`,
    { label: `draft:${cluster.clusterKey}`, phase: 'Draft', schema: ISSUE }
  ))
)).filter(Boolean)

return {
  surveyedProjects: projects.length,
  findings,
  clustersProposed: clustering.clusters || [],
  singletons: clustering.singletons || [],
  confirmedNew: confirmed.length,
  rejectedOrDup: clusters.length - confirmed.length,
  issues,
}
