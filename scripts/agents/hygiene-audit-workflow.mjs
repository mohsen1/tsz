export const meta = {
  name: 'tsz-hygiene-audit',
  description: 'Audit tsz for DRY violations + code-hygiene debt; emit verified, deduped, hierarchical GitHub issue specs',
  whenToUse: 'Deep codebase hygiene sweep producing parent/child tech-debt issue specs',
  phases: [
    { title: 'Survey', detail: 'per-crate DRY auditors + cross-cutting concern auditors' },
    { title: 'Cluster', detail: 'dedup vs open issues, bucket into epic taxonomy' },
    { title: 'Verify', detail: 'adversarially confirm each finding against real code' },
    { title: 'Author', detail: 'write full techdebt-template bodies for survivors' },
  ],
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------
const WT = '/Users/mohsen/code/tsz-pr12885' // worktree pinned to origin/main
const ISSUES = '/tmp/tsz-hygiene/open-issues.json' // dedup corpus (number,title,labels)

const PERSONA = [
  'You are a deeply senior Rust compiler engineer. Simplicity, DRY, and clean',
  'code are your religion: you prize zero-cost abstractions, eliminating',
  'boilerplate via macros / derive_more / newtypes, small focused modules,',
  'idiomatic error handling, and a tight Clippy posture. You are skeptical and',
  'EVIDENCE-DRIVEN: every finding MUST carry a real `file:line` citation you',
  'verified by reading the code in this repo. You never report vague smells,',
  'cosmetic nits, or anything a formatter/clippy already auto-fixes trivially.',
  '',
  `All analysis targets the worktree at ${WT} (pinned to origin/main).`,
  'Use absolute paths under that root. Use ripgrep (rg), wc, and Read.',
  '',
  'IMPORTANT architecture context (from .claude/CLAUDE.md):',
  '- Pipeline: scanner -> parser -> binder -> checker -> solver -> emitter.',
  '- Identity handles are u32 newtypes: TypeId, SymbolId, FlowNodeId, Atom, DefId.',
  '- Hard 2000-physical-line cap per source/test file (no local allowlists).',
  '- Anti-hardcoding gate: no identifier/file-name string checks in compiler logic.',
  '- A separate agent is actively paying down ARCHITECTURE-BOUNDARY debt',
  '  (checker/binder/solver god-objects, TypeEnvironment dup, option engines).',
  '  Do NOT propose architecture-boundary refactors that overlap those; focus on',
  '  DRY/boilerplate/hygiene/lint-posture that those issues do not cover.',
  '',
  `DEDUP: read ${ISSUES} (JSON array of {number,title,labels}). If a finding`,
  'overlaps an existing OPEN issue, still report it but set duplicate_of_existing',
  'to that issue number so synthesis can drop or merge it.',
].join('\n')

const FINDING_PROPS = {
  title: { type: 'string', description: 'Proposed issue title, format: techdebt(scope): <imperative>' },
  concern: { type: 'string', enum: ['dry', 'derive', 'file-size', 'clippy', 'error-handling', 'naming', 'dead-code', 'api'] },
  crate: { type: 'string' },
  structural_rule: { type: 'string', description: 'When <condition>, idiomatic Rust does X; tsz does Y at <location>.' },
  evidence: { type: 'array', items: { type: 'string' }, description: 'file:line citations w/ short quote; at least 2 distinct sites for a DRY claim' },
  why_it_matters: { type: 'string' },
  proposed_fix: { type: 'string', description: 'Concrete idiomatic fix (macro, derive_more, newtype, split, lint).' },
  size: { type: 'string', enum: ['S', 'M', 'L'] },
  duplicate_of_existing: { type: ['integer', 'null'] },
}
const FINDINGS_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['findings'],
  properties: {
    findings: {
      type: 'array',
      items: { type: 'object', additionalProperties: false, required: ['title', 'concern', 'crate', 'structural_rule', 'evidence', 'why_it_matters', 'proposed_fix', 'size', 'duplicate_of_existing'], properties: FINDING_PROPS },
    },
  },
}

// ---------------------------------------------------------------------------
// Phase 1 — Survey
// ---------------------------------------------------------------------------
phase('Survey')

const CRATES = [
  { name: 'tsz-solver', hint: 'relations, evaluation, inference, instantiation, narrowing, type construction, caches' },
  { name: 'tsz-checker', hint: 'AST orchestration, diagnostics, error_reporter, type computation, query_boundaries' },
  { name: 'tsz-emitter', hint: 'JS/DTS emit transforms, declaration emitter, printers, helpers' },
  { name: 'tsz-binder', hint: 'symbols, scopes, flow graph, atom migration' },
  { name: 'tsz-core', hint: 'module_resolver, config/options, parallel driver, source files' },
  { name: 'tsz-parser', hint: 'AST in arenas, recovery, jsdoc' },
  { name: 'tsz-lsp', hint: 'providers, hover, completions, workspace symbols' },
  { name: 'tsz-cli', hint: 'driver, tsz_server binary, plan' },
  { name: 'tsz-common + tsz-scanner + tsz-lowering', hint: 'interning, perf_counters, lexing, lowering shared utilities' },
]

const CONCERNS = [
  { key: 'derive', label: 'derive-bloat', task:
    'Audit derive & trait-impl BOILERPLATE across all crates. Specifically: (1) the ~15 `pub struct XxxId(u32)` identity newtypes (TypeId, SymbolId, DefId, FlowNodeId, Atom, SourceId, TypeListId, ObjectShapeId, ...) that each repeat the same Copy/Clone/Debug/PartialEq/Eq/Hash[/Ord] derive cluster plus hand-written Display/From/index helpers — propose a single `define_id!` macro or small internal proc-macro crate. (2) ~334 derive sites with >=5 traits and the 99x `Clone, Debug, Serialize, Deserialize` cluster — where can derive_more (From/Into/Deref/Display/Add) or a shared derive alias remove boilerplate? (3) ~51 hand-written `impl Default` that are derivable (clippy::derivable_impls). (4) repeated manual trait impls (Hash/PartialEq/Ord) that could be derived or macro-generated. Cite concrete structs.' },
  { key: 'file-size', label: 'split-debt', task:
    'Audit MODULARIZATION debt. Find: (1) files OVER the hard 2000-line cap (contract violation) — there are ~6. (2) files parked in 1900-2000 lines (~131) that are deferred splits; identify the worst offenders by responsibility-count, not just length. (3) god-modules / core.rs files that aggregate unrelated concerns. Propose concrete module decompositions. Do NOT just list lengths — name the distinct responsibilities that should split out.' },
  { key: 'clippy', label: 'clippy-posture', task:
    `Audit the LINT POSTURE for prevention. Read ${WT}/Cargo.toml [workspace.lints] and ${WT}/crates/clippy.toml. Today: correctness/style/perf/complexity=deny + ~6 hand-picked warns; pedantic and nursery are OFF; no unwrap_used/expect_used discipline; nothing lints derive bloat or file size. Propose a CONCRETE, staged lint-tightening plan: which pedantic/nursery lints to promote (e.g. derivable_impls, use_self, redundant_clone[on], equatable_if_let, needless_pass_by_value, trivially_copy_pass_by_ref, semicolon_if_nothing_returned, manual_let_else, explicit_iter_loop), which to allow, unwrap/expect discipline outside tests aligned with the existing allow-unwrap-in-tests=false philosophy, and whether a CI file-size ratchet should back the 2000-line rule. Justify each promotion by what class of future bug/debt it prevents. Note any lints that would create churn and should be deferred.` },
  { key: 'error-handling', label: 'error-handling', task:
    'Audit ERROR-HANDLING & panic discipline. Find: unwrap()/expect()/panic!/unreachable!/todo! in non-test library paths that could be Result/Option flows; repeated ad-hoc error construction that should share an error type; inconsistent Result vs Option vs sentinel returns for the same kind of failure; .clone() that masks borrow issues (redundant_clone is already a warn — look for ones it misses). Cite sites and propose idiomatic fixes.' },
  { key: 'dry-cross', label: 'cross-crate-dry', task:
    'Audit CROSS-CRATTE / shared-utility duplication: helper functions, constants, small algorithms (string interning helpers, span math, name mangling, hashing, small visitors) re-implemented in multiple crates that should live once in tsz-common. Find copy-pasted utility blocks across crate boundaries. Cite >=2 sites per claim.' },
  { key: 'dead-code', label: 'dead-code-api', task:
    'Audit DEAD/REDUNDANT abstraction & API surface: pub items with no external users, dead struct fields (e.g. the known dead IdentifierData.type_arguments), feature flags that are always-on/off, duplicate type aliases, wrapper types that only forward, and over-broad pub(crate) surfaces. Cite sites. Avoid overlap with the active architecture-boundary agent.' },
]

const surveyTasks = []
for (const c of CRATES) {
  surveyTasks.push(() => agent(
    `${PERSONA}\n\nTASK: Audit crate ${c.name} (${c.hint}) ONLY for DRY violations and code-hygiene debt that is LOCAL to this crate's domain: copy-pasted logic across functions/modules, parallel/near-duplicate match arms, repeated boilerplate, duplicated small algorithms, near-identical structs, and modules doing too much. For each DRY claim cite at least two distinct file:line sites. Report your 4-8 highest-value findings only. Skip architecture-boundary refactors owned by other agents.`,
    { label: `survey:${c.name.split(' ')[0]}`, phase: 'Survey', schema: FINDINGS_SCHEMA }
  ))
}
for (const c of CONCERNS) {
  surveyTasks.push(() => agent(
    `${PERSONA}\n\nTASK (cross-cutting concern = ${c.label}): ${c.task}\n\nReport your 4-10 highest-value findings only, each with verified file:line evidence.`,
    { label: `survey:${c.label}`, phase: 'Survey', schema: FINDINGS_SCHEMA }
  ))
}

const surveyResults = await parallel(surveyTasks)
const allFindings = surveyResults.filter(Boolean).flatMap(r => r.findings || [])
log(`Survey collected ${allFindings.length} raw findings from ${surveyTasks.length} auditors`)

// ---------------------------------------------------------------------------
// Phase 2 — Cluster + dedup (barrier: needs all findings + the open-issue corpus)
// ---------------------------------------------------------------------------
phase('Cluster')

const EPICS = [
  { key: 'clippy-posture', title: 'techdebt(repo): tighten the Clippy/lint posture to prevent hygiene regressions' },
  { key: 'derive-boilerplate', title: 'techdebt(repo): eliminate derive bloat and identity/serde/Default boilerplate' },
  { key: 'module-split', title: 'techdebt(repo): modularize oversized and near-cap files' },
  { key: 'dry-logic', title: 'techdebt(repo): eliminate duplicated compiler logic (DRY sweep)' },
  { key: 'general-hygiene', title: 'techdebt(repo): general hygiene — error handling, dead code, shared utilities' },
]

const CLUSTER_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['children', 'dropped'],
  properties: {
    children: { type: 'array', items: { type: 'object', additionalProperties: false,
      required: ['epic_key', 'title', 'concern', 'crate', 'structural_rule', 'evidence', 'why_it_matters', 'proposed_fix', 'size'],
      properties: {
        epic_key: { type: 'string', enum: EPICS.map(e => e.key) },
        ...FINDING_PROPS,
      } } },
    dropped: { type: 'array', items: { type: 'object', additionalProperties: false, required: ['title', 'reason'], properties: { title: { type: 'string' }, reason: { type: 'string' } } } },
  },
}

const cluster = await agent(
  `${PERSONA}\n\nYou are the SYNTHESIS lead. Below are ${allFindings.length} raw findings from a fan-out audit (JSON). ` +
  `Read the open-issue corpus at ${ISSUES} and DROP any finding that materially overlaps an existing OPEN issue (record it in "dropped" with the existing issue number in the reason). ` +
  `MERGE near-duplicate findings into one strong child. Then assign every surviving child to exactly one epic from this taxonomy: ` +
  EPICS.map(e => `${e.key} = "${e.title}"`).join('; ') + '. ' +
  `Prefer FEWER, STRONGER children (merge aggressively); each child must be independently actionable and PR-sized. Keep the best evidence when merging. ` +
  `Output children (with merged evidence) and the dropped list.\n\nRAW FINDINGS:\n` + JSON.stringify(allFindings),
  { label: 'cluster:synthesis', phase: 'Cluster', schema: CLUSTER_SCHEMA }
)
const candidates = (cluster?.children || [])
log(`Clustered into ${candidates.length} candidate children (${(cluster?.dropped || []).length} dropped as dup/weak)`)

// ---------------------------------------------------------------------------
// Phase 3+4 — Verify (adversarial) then Author (pipeline per child)
// ---------------------------------------------------------------------------
const VERIFY_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['keep', 'confidence', 'verified_evidence', 'notes'],
  properties: {
    keep: { type: 'boolean' },
    confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
    verified_evidence: { type: 'array', items: { type: 'string' }, description: 'corrected/confirmed file:line citations actually read' },
    notes: { type: 'string' },
  },
}
const AUTHOR_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['title', 'labels', 'size', 'epic_key', 'body_md'],
  properties: {
    title: { type: 'string' },
    labels: { type: 'array', items: { type: 'string' } },
    size: { type: 'string', enum: ['S', 'M', 'L'] },
    epic_key: { type: 'string', enum: EPICS.map(e => e.key) },
    body_md: { type: 'string', description: 'Full GitHub issue body in markdown following the techdebt template' },
  },
}

const authored = await pipeline(
  candidates,
  // Stage 1: adversarial verification against the real code
  (child) => agent(
    `${PERSONA}\n\nADVERSARIAL VERIFY. Try to REFUTE this candidate tech-debt issue. Open the cited files in ${WT} and check: ` +
    `(a) does the code actually exhibit the claimed duplication/bloat/debt at those lines? (b) is it already idiomatic / auto-fixed by existing lints? ` +
    `(c) is it actually a duplicate of an existing OPEN issue in ${ISSUES}? (d) for a DRY claim, are there genuinely >=2 real duplicated sites? ` +
    `Correct or tighten the evidence to what you actually verified. Default keep=false if evidence is weak, stale, or unverifiable.\n\nCANDIDATE:\n` + JSON.stringify(child),
    { label: `verify:${child.concern}/${child.crate}`.slice(0, 48), phase: 'Verify', schema: VERIFY_SCHEMA }
  ).then(v => ({ child, v })),
  // Stage 2: author the full issue body for survivors only
  (res, child) => {
    if (!res || !res.v || !res.v.keep || res.v.confidence === 'low') return null
    const verified = { ...child, evidence: res.v.verified_evidence?.length ? res.v.verified_evidence : child.evidence, verify_notes: res.v.notes }
    return agent(
      `${PERSONA}\n\nAUTHOR the GitHub issue body for this VERIFIED tech-debt finding. Follow this exact template (markdown), matching the repo's existing techdebt issues:\n` +
      '\n## Summary\n<one paragraph + the structural rule: "When <condition>, idiomatic Rust does X; tsz does Y at <owner>.">\n' +
      '\n## Evidence\n<bulleted file:line citations with short quotes — only the verified ones>\n' +
      '\n## Why it matters\n<concrete maintenance/correctness/perf cost; reference DRY/boilerplate counts>\n' +
      '\n## Proposed fix\n<concrete idiomatic Rust: macro/derive_more/newtype/module split/lint promotion; sized S/M/L; multi-PR if L>\n' +
      '\n## Risks / coordination\n<behavior-preservation, ordering, overlap with other agents, verification commands>\n' +
      `\nDo NOT invent evidence beyond what is provided. Set labels to ["tech-debt"] plus at most one crate/area label from this set if clearly applicable: solver, checker, binder, parser, emitter, emit, lsp, cli, scanner, performance, ci, chore. Set epic_key=${child.epic_key}.\n\nVERIFIED FINDING:\n` + JSON.stringify(verified),
      { label: `author:${child.concern}`.slice(0, 48), phase: 'Author', schema: AUTHOR_SCHEMA }
    )
  }
)

const children = authored.filter(Boolean)
log(`Authored ${children.length} verified issue bodies`)

return {
  epics: EPICS,
  children,
  dropped: cluster?.dropped || [],
  stats: { raw_findings: allFindings.length, candidates: candidates.length, authored: children.length },
}
