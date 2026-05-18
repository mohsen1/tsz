export const REQUIRED_PROJECT_ROWS = [
  "utility-types-project",
  "ts-essentials-project",
  "rxjs-project",
  "type-fest-project",
  "vite-vanilla-ts-app",
  "nextjs-fresh-app",
  "nextjs",
  "large-ts-repo",
];

export const COMPILE_CANARY_PROJECT_ROWS = [
  "ts-toolbelt-project",
  "zod-project",
  "kysely-project",
  "type-challenges-project",
  "type-challenges-solutions-project",
  "type-challenges-assertion-candidates",
  "type-challenges-assertions-tsc-clean",
];

export const REQUIRED_COMPATIBILITY_FIELDS = [
  "state",
  "exit_class",
  "first_failure_class",
  "owner_track",
  "phase",
  "last_successful_phase",
  "diagnostic_status",
  "diagnostic_deltas",
  "diagnostic_subsystems",
  "known_blockers",
  "reduced_repro_path",
  "repro",
  "exit_codes",
  "files_reached",
  "peak_memory_bytes",
  "fixture_sources",
  "emit_status",
  "dts_status",
];

export const COMPATIBILITY_CORPUS_ROWS = [
  {
    name: "utility-types-project",
    label: "utility-types",
    owner: "Tracks 1, 2, 5",
    family: "baseline utility mapped/conditional surface",
  },
  {
    name: "rxjs-project",
    label: "RxJS",
    owner: "Tracks 1, 3, 7, 10",
    family: "observable/subject generics, module identity, generated config pressure",
  },
  {
    name: "kysely-project",
    label: "Kysely",
    owner: "Tracks 1, 3, 5, 6",
    family: "contextual generics, guards, indexed/property access",
  },
  {
    name: "zod-project",
    label: "Zod",
    owner: "Tracks 1, 2, 4, 6, 7",
    family: "recursive conditionals, object guards, class/generic identity",
  },
  {
    name: "ts-toolbelt-project",
    label: "ts-toolbelt",
    owner: "Tracks 1, 2, 3",
    family: "recursive type evaluation pressure",
  },
  {
    name: "type-fest-project",
    label: "type-fest",
    owner: "Tracks 1, 2, 5",
    family: "mapped/conditional/key-space utility surface",
  },
  {
    name: "ts-essentials-project",
    label: "ts-essentials",
    owner: "Tracks 1, 2, 5",
    family: "utility types plus recursive JSON shapes",
  },
  {
    name: "large-ts-repo",
    label: "large-ts-repo",
    owner: "Tracks 1, 7, 10",
    family: "residency/runtime/project graph stress",
  },
  {
    name: "nextjs-fresh-app",
    label: "generated Next app",
    owner: "Tracks 1, 7, 9",
    family: "generated app-router dependency/config sanity",
  },
  {
    name: "vite-vanilla-ts-app",
    label: "generated Vite app",
    owner: "Tracks 1, 7, 9",
    family: "generated app dependency/config sanity",
  },
  {
    name: "type-challenges-project",
    label: "type-challenges",
    owner: "Tracks 2, 3, 5",
    family: "advanced type-level challenge templates",
  },
  {
    name: "type-challenges-solutions-project",
    label: "type-challenges solutions",
    owner: "Tracks 2, 3, 5",
    family: "advanced type-level solved challenge programs",
  },
  {
    name: "type-challenges-assertion-candidates",
    label: "type-challenges assertions",
    owner: "Tracks 2, 3, 5",
    family: "assertion-level Type Challenges readiness comparison",
  },
  {
    name: "type-challenges-assertions-tsc-clean",
    label: "type-challenges tsc-clean assertions",
    owner: "Tracks 2, 3, 5",
    family: "tsz check over Type Challenges assertion candidates accepted by tsc",
  },
  {
    name: "nextjs",
    label: "Next.js full project",
    owner: "Tracks 1, 7, 9",
    family: "module graph plus generated app dependencies",
  },
];
