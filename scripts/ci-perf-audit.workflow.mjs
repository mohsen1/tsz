export const meta = {
  name: 'ci-perf-audit',
  description: 'Study GitHub Actions workflows; find safe perf optimizations + dead code, checked against revert history',
  phases: [
    { title: 'Audit', detail: 'fan out by concern; every finding grounded in the workflow YAML + git history' },
    { title: 'Verify', detail: 'adversarially confirm each change is safe, does not re-introduce a revert, does not break a required gate' },
    { title: 'Synthesize', detail: 'prioritized, deduped, risk-rated optimization plan' },
  ],
}

const A = typeof args === 'string' ? JSON.parse(args) : args
const { repoRoot, context } = A

const FINDINGS = {
  type: 'object',
  required: ['findings'],
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['title', 'file', 'kind', 'change', 'evidence', 'wallTimeImpact', 'risk', 'gitHistoryChecked', 'confidence'],
        properties: {
          title: { type: 'string' },
          file: { type: 'string', description: 'workflow file:lines or job name' },
          kind: { type: 'string', enum: ['perf-criticalpath', 'perf-cache', 'perf-shard', 'perf-gate-off-pr', 'dead-code-remove', 'cost-reduce', 'correctness'] },
          change: { type: 'string', description: 'the concrete edit to make' },
          evidence: { type: 'string', description: 'quote the YAML + timing/history that justifies it' },
          wallTimeImpact: { type: 'string', description: 'estimated PR-CI wall-clock or job-min saved' },
          risk: { type: 'string', description: 'what could break; which CI Summary required contexts are affected' },
          gitHistoryChecked: { type: 'string', description: 'what git log -S / revert search you ran and what it showed — was this tried/reverted before?' },
          requiresGcloud: { type: 'boolean', description: 'does it need GCP config (Cloud Run/Build runner pool, sccache bucket)?' },
          confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
        },
      },
    },
  },
}

const AREAS = [
  {
    key: 'inventory-deadcode',
    prompt: `Inventory ALL GitHub Actions workflows in the tsz repo. Read each \`git show origin/main:.github/workflows/<f>\` (12 files: bench, ci, ci-health, daily-release, gh-pages, install-test, npm-publish, quality-tools, readme-benchmark-refresh, refresh-green-prs, release, rerun-on-comment). For EACH: purpose, triggers, whether it is actively needed, and whether it is removable dead code. Context: refresh-green-prs.yml is \`disabled_manually\` on GitHub — run \`git log origin/main\` on it to find WHY it was disabled and whether the file should be removed or kept. Three workflow registrations (claude-code-review, perf-bench-compare, update-readme) are ORPHANED (files already deleted from main) — note them as UI cruft (not repo-actionable) but confirm no remaining file references them. Check cross-references: \`workflow_run\` chains (bench triggers off CI? gh-pages off what?), reusable-workflow \`uses:\`, and composite actions under .github/actions. Identify any genuinely removable file or redundant duplicate workflow. repoRoot=${repoRoot}.`,
  },
  {
    key: 'ci-criticalpath',
    prompt: `Analyze the CRITICAL PATH of \`git show origin/main:.github/workflows/ci.yml\` (the constantly-running PR-blocking workflow). Parse the full job graph: every job's \`needs:\`, \`if:\`, matrix, runner (\`runs-on\`), and what the final "CI Summary" / merge-readiness gate depends on. Using the measured timing profile, find SAFE wall-clock reductions:
- Which slow jobs are on the PR critical path vs already parallel? (unit ~13m, dist-binaries ~11m, lint ~8m, project-compile-canary shard1 ~7.5m, wasm ~4m).
- Which jobs are needed ONLY for merge_group / release / bench and could be GATED OFF pull_request runs (run on merge_group only) — e.g. is dist-binaries needed before merge, or only for release/bench? Does any required CI-Summary context depend on it?
- Which jobs could be DEFERRED/PARALLELIZED or have their \`needs:\` relaxed to start earlier?
- Path-filter opportunities (skip jobs when only docs/scripts change — note ci.yml already does some).
CRITICAL: for every proposal, run git history to confirm it was not tried+reverted. Known landmines (do NOT re-propose): ${context.knownReverted}. For each finding give the exact job, the edit, wall-time impact, and the required-context risk. repoRoot=${repoRoot}. Timing: ${context.timing}`,
  },
  {
    key: 'build-cache-perf',
    prompt: `Study the BUILD + CACHE setup in \`git show origin/main:.github/workflows/ci.yml\` and any referenced scripts (scripts/ci/*.sh, especially gcp-full-ci.sh, suite-metadata.sh) and composite actions. Goal: cut compile time, which dominates (unit ~13m incl ~14m-ish compile per prior measurement; lint ~8m; dist-binaries ~11m; each suite rebuilds). Investigate:
- sccache: is it wired now? History shows sccache env wiring was REVERTED (commit d69f247674) and there were silent-exit issues on tsz-checker lib-test. Determine current state and whether a SAFE re-enable or alternative is possible; if previously reverted for a concrete reason, mark do-not-touch.
- cargo/target caching: actions/cache keys, whether compiled artifacts are shared between jobs (unit vs unit-checker-integration vs emit vs conformance vs fourslash all need a built binary — do they each rebuild from scratch, or reuse a cached/uploaded target?). Biggest win is usually building ONCE and reusing.
- The Cloud Run unit job (#12768) and the reverted Cloud Build private pool (#7591) — how does unit build today, and is there waste?
- dist-binaries: profile what it compiles and whether PR CI needs it.
Propose SAFE caching/artifact-reuse improvements with cache-key correctness (invalidation) reasoning. Confirm via git history nothing is a re-introduced revert. requiresGcloud=true for anything touching the runner pool / sccache bucket / Cloud config. repoRoot=${repoRoot}.`,
  },
  {
    key: 'sharding-and-bench',
    prompt: `Two parts. (1) SHARDING in \`git show origin/main:.github/workflows/ci.yml\`: project-compile-canary is IMBALANCED (shard1 7.5m vs shard0 4.3m vs shard2 3.3m); conformance 6 shards ~1.7m each; fourslash 6 shards ~3.6m each. Determine how shard assignment works (static list? derived from a mutable GCS timings blob — KNOWN issue #13397?). Propose: rebalance canary shards (cut the 7.5m tail), and assess whether conformance/fourslash shard COUNTS are optimal (too many tiny shards waste runner-spawn/setup time on a fixed-size pool; too few create a long tail). Find the runner-spawn/setup overhead per shard (checkout + submodule + build/restore) to judge. (2) bench.yml (1971 lines, runs constantly via workflow_run/schedule): scan for perf/cost waste — redundant rebuilds, whether it can reuse CI-built artifacts, dead matrix entries. Each finding: exact location, change, impact, risk, git-history check. repoRoot=${repoRoot}. Timing: ${context.timing}. Known: ${context.knownReverted}.`,
  },
]

phase('Audit')
const audited = (await parallel(
  AREAS.map((a) => () => agent(a.prompt, { label: `audit:${a.key}`, phase: 'Audit', schema: FINDINGS }))
)).filter(Boolean)
const all = audited.flatMap((r) => r.findings || [])
log(`Audit: ${all.length} raw findings`)

const VERDICT = {
  type: 'object',
  required: ['title', 'safe', 'reintroducesRevert', 'verdict'],
  properties: {
    title: { type: 'string' },
    safe: { type: 'boolean' },
    reintroducesRevert: { type: 'boolean', description: 'does it re-introduce a previously-reverted pattern?' },
    breaksRequiredGate: { type: 'boolean', description: 'does it remove/skip a CI Summary required context on PRs?' },
    correctedChange: { type: 'string' },
    wallTimeImpact: { type: 'string' },
    notes: { type: 'string' },
    verdict: { type: 'string', enum: ['confirmed', 'confirmed-with-corrections', 'rejected'] },
  },
}

phase('Verify')
const candidates = all.filter((f) => f.confidence !== 'low')
const verified = (await parallel(
  candidates.map((f) => () => agent(
    `Adversarially verify this proposed tsz CI/workflow change. Default to skepticism — CI changes are high-blast-radius. Confirm: (1) is it SAFE (won't break a required CI Summary context on PRs or merge_group)? (2) does it RE-INTRODUCE a previously-reverted pattern (run git log -S / --grep on origin/main to check; known reverts: ${context.knownReverted})? (3) is the wall-time claim real given the job graph? (4) does it need GCP config? Read the actual ci.yml/bench.yml job graph at origin/main to confirm needs/if/gate wiring. Return a verdict + corrected change. verdict=rejected if unsafe or a re-introduced revert.

FINDING:
${JSON.stringify(f, null, 1)}

repoRoot=${repoRoot}`,
    { label: `verify:${(f.title || '').slice(0, 28)}`, phase: 'Verify', schema: VERDICT }
  ))
)).filter(Boolean)

phase('Synthesize')
const synth = await agent(
  `Synthesize the verified CI-perf audit into a prioritized, DEDUPED plan. Include only confirmed / confirmed-with-corrections findings. Order by (wall-time-saved on PR CI, safety): quick safe wins first, riskier/bigger later. Separate: (A) ready-to-apply edits (low risk, no GCP), (B) needs-gcloud config, (C) dead-code/cleanup, (D) risky/needs-more-investigation.

RAW: ${JSON.stringify(candidates, null, 1)}
VERDICTS: ${JSON.stringify(verified, null, 1)}

Return {plan:[{title,file,change,bucket(A|B|C|D),wallTimeSaved,risk,requiresGcloud,verifyHow}], rejected:[{title,why}], estimatedWallClockBefore, estimatedWallClockAfter}.`,
  {
    label: 'synthesize', phase: 'Synthesize',
    schema: { type: 'object', required: ['plan'], properties: {
      plan: { type: 'array', items: { type: 'object', required: ['title', 'file', 'change', 'bucket'], properties: {
        title: { type: 'string' }, file: { type: 'string' }, change: { type: 'string' }, bucket: { type: 'string' }, wallTimeSaved: { type: 'string' }, risk: { type: 'string' }, requiresGcloud: { type: 'boolean' }, verifyHow: { type: 'string' } } } },
      rejected: { type: 'array', items: { type: 'object', properties: { title: { type: 'string' }, why: { type: 'string' } } } },
      estimatedWallClockBefore: { type: 'string' }, estimatedWallClockAfter: { type: 'string' },
    } },
  }
)

return { rawCount: all.length, verifiedCount: verified.length, plan: synth.plan || [], rejected: synth.rejected || [], before: synth.estimatedWallClockBefore, after: synth.estimatedWallClockAfter }
