export const meta = {
  name: 'app-canary-select',
  description: 'Research + verify candidate TS application/dashboard repos for the grow corpus (deps install, real .ts/.tsx signal)',
  phases: [
    { title: 'Verify', detail: 'one agent per candidate batch: confirm ref, source dir, package manager, install, own tsconfig, size, risks' },
    { title: 'Synthesize', detail: 'pick the best 20, diverse + manageable, with complete row metadata' },
  ],
}

const A = typeof args === 'string' ? JSON.parse(args) : args
const { candidates } = A

const ROWS = {
  type: 'object',
  required: ['rows'],
  properties: {
    rows: {
      type: 'array',
      items: {
        type: 'object',
        required: ['slug', 'repo', 'ref', 'source_dir', 'package_manager', 'install_cmd', 'tsconfig_path', 'monorepo', 'recommend', 'framework'],
        properties: {
          slug: { type: 'string', description: 'kebab id, e.g. umami' },
          label: { type: 'string' },
          repo: { type: 'string', description: 'https://github.com/owner/name.git' },
          ref: { type: 'string', description: 'current HEAD commit sha (full) from git ls-remote' },
          source_dir: { type: 'string', description: 'the .ts/.tsx-heavy app source dir relative to repo root (verify it exists at ref + has many .ts/.tsx)' },
          ts_file_count: { type: 'number', description: 'approx .ts/.tsx files under source_dir' },
          package_manager: { type: 'string', enum: ['pnpm', 'yarn', 'npm', 'bun', 'unknown'] },
          install_cmd: { type: 'string', description: 'exact frozen/ci install command for that pm (e.g. "pnpm install --frozen-lockfile --ignore-scripts")' },
          monorepo: { type: 'boolean', description: 'is it a monorepo (install must run at repo root for workspace deps)?' },
          install_root: { type: 'string', description: 'dir to run install in (repo root for monorepos)' },
          tsconfig_path: { type: 'string', description: "the app's own tsconfig to compile with (has jsx + paths); relative to repo root" },
          has_jsx: { type: 'boolean' },
          has_path_aliases: { type: 'boolean' },
          framework: { type: 'string', description: 'next|react|vue|svelte|angular|solid|other' },
          est_node_modules: { type: 'string', description: 'rough install size if known' },
          recommend: { type: 'boolean', description: 'include in the 20? false if too huge, mostly non-.ts (svelte/vue components), or uninstallable' },
          risks: { type: 'string', description: 'size, install flakiness, .svelte/.vue logic skipped, monorepo install heft, etc.' },
        },
      },
    },
  },
}

phase('Verify')
const verified = (await parallel(
  candidates.map((batch, i) => () => agent(
    `Verify these candidate open-source TypeScript APPLICATION repos for a compiler test corpus (we clone shallow, INSTALL deps, then run a TS type-checker over the app's own .ts/.tsx with its own tsconfig). For EACH candidate, use \`git ls-remote <repo> HEAD\` for the ref and \`gh api repos/<owner>/<name>/contents/<path>?ref=<sha>\` (or the raw file API) to inspect layout WITHOUT a full clone (keep it cheap). Determine:
- current HEAD commit sha (full)
- the .ts/.tsx-heavy app source dir (NOT a lib; the actual app/dashboard UI+logic). Confirm it exists at that ref and roughly how many .ts/.tsx files it has. AVOID dirs dominated by .svelte/.vue (a TS type-checker skips those — only their .ts files count).
- package manager (lockfile: pnpm-lock.yaml=pnpm, yarn.lock=yarn, package-lock.json=npm, bun.lockb=bun) + the exact frozen install command
- monorepo? (workspace deps require install at repo ROOT). Give install_root.
- the app's own tsconfig path (must have jsx + path aliases for a real app); confirm it exists
- framework, rough size, and RISKS (too huge to compile in ~150s, mostly non-.ts, monorepo install heft, native build scripts that need --ignore-scripts, etc.)
- recommend=true only if it is a real installable TS app with substantial .ts/.tsx and not absurdly huge.

CANDIDATES (batch ${i}):
${JSON.stringify(batch, null, 2)}

Return one row per candidate with the structured fields. Be accurate on ref (real sha), source_dir (exists + .ts-heavy), and package_manager (from the actual lockfile).`,
    { label: `verify:batch${i}`, phase: 'Verify', schema: ROWS }
  ))
)).filter(Boolean)

const all = verified.flatMap((r) => r.rows || [])
log(`Verified ${all.length} candidates`)

phase('Synthesize')
const pick = await agent(
  `From these verified candidate TS application repos, select the best 20 for a compiler test corpus. Prioritize: (1) real apps/dashboards with substantial .ts/.tsx (not .svelte/.vue-dominated), (2) DIVERSITY of domain (analytics, CRM, CMS, commerce, editor, project-mgmt, chat, dev-tools, secrets, scheduling) and framework (lean Next/React but include a couple non-React TS apps if their .ts content is substantial), (3) installable without absurd cost, (4) manageable size (a single app package preferred over a 1000-file monorepo). Drop candidates marked recommend=false or with disqualifying risks. Ensure all 20 have a real HEAD ref, an existing .ts-heavy source_dir, a package manager + install command, and an own tsconfig.

CANDIDATES:
${JSON.stringify(all, null, 2)}

Return the final 20 rows (same schema) plus a one-line rationale per pick in risks, and note any domain/framework gaps.`,
  { label: 'synthesize', phase: 'Synthesize', schema: ROWS }
)

return { verifiedCount: all.length, selected: pick.rows || [] }
