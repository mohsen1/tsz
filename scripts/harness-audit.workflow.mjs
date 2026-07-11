export const meta = {
  name: 'harness-audit',
  description: 'Audit the tsz bench/corpus/CI harness for concrete, evidenced bugs and safe fixes',
  phases: [
    { title: 'Audit', detail: 'fan out by harness area; each finding must be a concrete, evidenced bug/inconsistency/gap with a proposed fix' },
    { title: 'Verify', detail: 'adversarially confirm each finding is a real bug (not intended), fix is safe, check git intent' },
    { title: 'Synthesize', detail: 'dedupe into a prioritized fix list' },
  ],
}

const A = typeof args === 'string' ? JSON.parse(args) : args
const { repoRoot, binary } = A

const FINDINGS = {
  type: 'object',
  required: ['findings'],
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['title', 'file', 'kind', 'evidence', 'proposedFix', 'risk', 'confidence', 'isRealBug'],
        properties: {
          title: { type: 'string' },
          file: { type: 'string', description: 'path:line(s)' },
          kind: { type: 'string', enum: ['bug', 'inconsistency', 'gap', 'intended-behavior-not-a-bug'] },
          evidence: { type: 'string', description: 'why it is wrong — quote the code/output; reproduce if possible' },
          proposedFix: { type: 'string', description: 'concrete, minimal, safe fix' },
          affects: { type: 'string', description: 'which rows/jobs/behavior this affects' },
          risk: { type: 'string', description: 'what could break; does it touch the row-sync invariants enforced by test-project-rows.mjs?' },
          isRealBug: { type: 'boolean', description: 'false if after investigation it is intended behavior' },
          confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
        },
      },
    },
  },
}

const AREAS = [
  {
    key: 'config-writers',
    prompt: `Audit the bench fixture CONFIG WRITERS in ${repoRoot}/scripts/bench/project-fixtures.sh: every \`tsz_write_*_config\` function, \`tsz_write_basic_external_project_config\`, the external-stub writers, \`tsz_load_fixture_pins_from_rows\`, and the packed-row sync. Look for CONCRETE bugs: (a) a writer that still sets TS7-removed \`baseUrl\` or legacy \`moduleResolution: "node" | "node10" | "classic"\` (\`ignoreDeprecations: "6.0"\` cannot restore removed options; \`paths\` entries resolve relative to the config directory without \`baseUrl\`); (b) a source_dir that does not match where the project's real .ts sources live (cross-check against ${repoRoot}/scripts/bench/project-rows.mjs and the cloned fixture layout under ${repoRoot}/.target/project-compile-guard/<name>); (c) options that diverge from the project's REAL tsconfig in a way that injects false errors (missing experimentalDecorators, jsx, lib, types, allowImportingTsExtensions, customConditions, paths); (d) external-stub .d.ts that fail to declare namespaces/augmentations the source needs (e.g. NodeJS namespace, Reflect.getMetadata) causing false TS2833/TS2339; (e) shell quoting/heredoc bugs. For each, give file:line, quote the code, and a minimal safe fix. Distinguish true bugs from intended minimalism. You may run \`${binary} --noEmit -p <generated tsconfig>\` on a fixture to confirm a fix removes config-noise without unmasking unrelated errors.`,
  },
  {
    key: 'compile-guard',
    prompt: `Audit ${repoRoot}/scripts/ci/project-compile-guard.sh and ${repoRoot}/scripts/ci/lib/project-compile-fingerprint.sh (and any lib it sources). Look for CONCRETE bugs in: result-cache correctness (fingerprint key, stale-entry handling, caching a cycle-sentinel None), exit-class classification (project_failure_class, timeout vs nonzero), TSZ_PROJECT_COMPILE_SET dispatch, should_check_project filter, ALLOW_FAILURES semantics, the compatibility JSONL/summary record fields, RUST_MIN_STACK/timeout defaults, and any path-safety checks. Quote code at file:line, explain why it is wrong, give a minimal safe fix. Mark intended behavior as not-a-bug. Cross-check git log -S for intent where a construct looks deliberate.`,
  },
  {
    key: 'bench-runner',
    prompt: `Audit ${repoRoot}/scripts/bench/bench-vs-tsgo.sh and ${repoRoot}/scripts/bench/lib/bench-vs-tsgo-results.sh. Look for CONCRETE bugs in: the per-row run_<name>_project_benchmarks functions (should_run_compile_canary_project gating, is_benchmark_selected, print_header, ensure_<name>_fixture, the config writer call, run_project_benchmark args/source_dir), the dispatch/registration (run_isolated lines), result writers, and any row that is defined in project-rows.mjs but not wired here (or wired with a wrong source_dir). Cross-check against project-rows.mjs. Quote file:line, give safe fixes. Note: scripts/bench/test-project-rows.mjs enforces sync — any fix must keep it passing.`,
  },
  {
    key: 'row-metadata-and-ci',
    prompt: `Audit (1) the row metadata + sync: ${repoRoot}/scripts/bench/project-rows.mjs, validate-project-metadata.mjs, test-project-rows.mjs, row-utils.mjs — look for wrong/stale repo refs or source_dirs, drift between the .mjs and the shell/roadmap/ci consumers, or validation gaps. (2) the CI harness: ${repoRoot}/scripts/ci/suite-metadata.sh and shard planning (there is a KNOWN issue #13397 that shard planning derives from a mutable GCS timings blob — assess and propose a deterministic fix), plus ${repoRoot}/scripts/ci/gcp-full-ci.sh run_lint steps and check-crate-root-files.sh / arch guards for concrete bugs. Quote file:line, give safe minimal fixes, distinguish bugs from intended. Run \`node ${repoRoot}/scripts/bench/validate-project-metadata.mjs\` and \`node ${repoRoot}/scripts/bench/test-project-rows.mjs\` to confirm current state.`,
  },
]

phase('Audit')
const audited = (await parallel(
  AREAS.map((a) => () => agent(a.prompt, { label: `audit:${a.key}`, phase: 'Audit', schema: FINDINGS }))
)).filter(Boolean)

const allFindings = audited.flatMap((r) => r.findings || [])
log(`Audit surfaced ${allFindings.length} raw findings across ${AREAS.length} areas`)

const VERDICT = {
  type: 'object',
  required: ['title', 'isRealBug', 'fixSafe', 'verdict'],
  properties: {
    title: { type: 'string' },
    isRealBug: { type: 'boolean' },
    reproduced: { type: 'boolean', description: 'did you reproduce the bug / confirm the fix?' },
    fixSafe: { type: 'boolean', description: 'does the fix avoid breaking row-sync tests and other rows?' },
    correctedFix: { type: 'string', description: 'the fix to actually apply (corrected if needed)' },
    notes: { type: 'string' },
    verdict: { type: 'string', enum: ['confirmed', 'confirmed-with-corrections', 'rejected'] },
  },
}

phase('Verify')
const realFindings = allFindings.filter((f) => f.isRealBug && f.confidence !== 'low')
const verified = (await parallel(
  realFindings.map((f) => () => agent(
    `Adversarially verify this proposed tsz harness bug + fix. Default to skepticism: is it actually a bug, or intended behavior? Will the fix break anything (especially the row-sync invariants in scripts/bench/test-project-rows.mjs, or other rows)? Reproduce where possible (run the binary / the node validators / the guard on one row). Check git log/blame for intent.

FINDING:
${JSON.stringify(f, null, 1)}

repo: ${repoRoot}
binary: ${binary}

Return a verdict with the corrected fix to apply. verdict=rejected if it is intended behavior or the fix is unsafe.`,
    { label: `verify:${(f.title || '').slice(0, 30)}`, phase: 'Verify', schema: VERDICT }
  ))
)).filter(Boolean)

phase('Synthesize')
const synth = await agent(
  `Synthesize the verified tsz harness audit into a prioritized, DEDUPED fix list. Only include findings whose verdict is confirmed or confirmed-with-corrections.

RAW FINDINGS:
${JSON.stringify(realFindings, null, 1)}

VERDICTS:
${JSON.stringify(verified, null, 1)}

Return a JSON object {fixes: [{title, file, fix, priority(high|med|low), affects, risk, verifyCmd}], rejected: [{title, why}]}. Order fixes by value (rows unblocked / false-signal removed) and safety. Each fix must be concrete enough to apply directly. Note any that touch the test-project-rows.mjs sync invariants.`,
  {
    label: 'synthesize',
    phase: 'Synthesize',
    schema: {
      type: 'object',
      required: ['fixes'],
      properties: {
        fixes: { type: 'array', items: { type: 'object', required: ['title', 'file', 'fix', 'priority'], properties: {
          title: { type: 'string' }, file: { type: 'string' }, fix: { type: 'string' }, priority: { type: 'string' }, affects: { type: 'string' }, risk: { type: 'string' }, verifyCmd: { type: 'string' },
        } } },
        rejected: { type: 'array', items: { type: 'object', properties: { title: { type: 'string' }, why: { type: 'string' } } } },
      },
    },
  }
)

return { rawCount: allFindings.length, verifiedCount: verified.length, fixes: synth.fixes || [], rejected: synth.rejected || [] }
