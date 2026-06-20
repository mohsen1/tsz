/**
 * Per-project one-sentence descriptions for benchmark/compatibility rows.
 *
 * Keys match the `name` field in PROJECT_ROW_DEFINITIONS from
 * scripts/bench/project-rows.mjs. Each value explains what the project is
 * and why it stresses the TypeScript type-checker.
 */
export const PROJECT_DESCRIPTIONS = {
  // ── external library rows ─────────────────────────────────────────────────

  "utility-types-project":
    "A widely-used utility-type library; exercises mapped types, conditional types, and the core utility-type surface that every real project depends on.",

  "ts-essentials-project":
    "A broad utility-type toolkit covering recursive JSON shapes, deep readonly, and strict-null helpers; stresses mapped/conditional evaluation with recursive object types.",

  "rxjs-project":
    "Observable/Subject generic pipelines with complex operator types; stresses module-graph identity, generated-config loading, and multi-file generic propagation.",

  "type-fest-project":
    "A large collection of advanced mapped, conditional, and key-space utility types; exercises the breadth of TypeScript's higher-order type operators.",

  "ts-toolbelt-project":
    "Over 200 files of deep recursive type utilities (list, object, function, union manipulation); a heavy generic-instantiation and constraint-evaluation workload.",

  "zod-project":
    "Runtime schema validation library with heavily recursive conditional generics across builder chains; stresses object-guard class/generic identity and recursive schema inference.",

  "kysely-project":
    "Type-safe SQL query builder with contextual generics, indexed/property-access chaining, and deep conditional result types; stresses generic narrowing under complex constraints.",

  "type-challenges-solutions-project":
    "Community solutions to 200+ advanced type-level puzzles; exercises the full breadth of conditional, mapped, template-literal, and infer-based type patterns.",

  "vite-vanilla-ts-app":
    "A generated Vite + TypeScript scaffold; validates dependency resolution, tsconfig wiring, and module-graph bootstrap for a minimal real-world app.",

  "nextjs-fresh-app":
    "A generated Next.js app-router project; validates JSX type-checking, Next.js type augmentations, and config resolution across a typical framework setup.",

  "large-ts-repo":
    "A synthetic multi-package monorepo designed to stress project-graph setup, cross-package type propagation, and peak memory/residency under large file counts.",

  "nextjs":
    "The full Next.js source tree; exercises module-graph complexity, framework-internal generics, and generated-config pressure at real-world scale.",

  "valibot-project":
    "A schema-validation library built on recursive conditional generics and brand types; stresses the solver's handling of deeply nested schema-builder inference.",

  "msw-project":
    "Mock Service Worker with pnpm-symlinked @types dependencies and project references; stresses multi-root module resolution and declaration-file merging.",

  "comlink-project":
    "A small, strict-mode Worker RPC library; serves as a clean lib-catalog coverage canary and baseline for node10/strict module resolution.",

  "effect-project":
    "A large-scale functional-effects library (~296k LOC) with layered generic schemas, HKT-style abstractions, and complex variance; a high-fidelity parity canary.",

  "drizzle-orm-project":
    "Type-safe ORM with JSON file routing, contextual-keyword binders, and a pnpm @types graph; exercises column/table generic inference and cross-package declaration merging.",

  "ts-rest-project":
    "API contract library combining Zod schemas with HTTP route generics; validates near-clean diagnostic parity under mixed-framework generic composition.",

  "ofetch-project":
    "A small isomorphic fetch wrapper; serves as a single-diagnostic canary for the TS5107 lib-reference chain rule and minimal-config resolution.",

  "ts-pattern-project":
    "Exhaustive pattern-matching library with union distribution and symbol-keyed computed-property members; stresses narrowing across value/type name merges.",

  "trpc-project":
    "End-to-end type-safe RPC with recursive router builders, deep generic procedure chaining, and input/output type propagation across client and server.",

  "tanstack-query-project":
    "Data-fetching library with complex query/mutation generic option bags, overloaded hook signatures, and key/data co-inference.",

  "tanstack-router-project":
    "File-based router with template-literal route path parsing, nested route-tree generics, and search-param inference; stresses template-literal type evaluation.",

  "zustand-project":
    "Minimal state-management library with middleware generic composition, slice typing, and set/get state inference; exercises higher-order generic patterns.",

  "jotai-project":
    "Atomic state-management with generic atom families, derived/async atom inference, and store typing; stresses contextual-type propagation through atom composition.",

  "fp-ts-project":
    "Functional programming library with higher-kinded type encodings, type-class hierarchies, and deep generic pipe/flow composition; stresses variance and generic substitution.",

  "io-ts-project":
    "Runtime codec library with recursive/branded type definitions and decode-result inference; exercises recursive generic instantiation and union discrimination.",

  "immer-project":
    "Immutable-state library with a recursive `Draft<T>` mapped/conditional type that strips readonly; stresses recursive mapped-type evaluation and produce-overload resolution.",

  "remeda-project":
    "Data-manipulation utility with data-first/data-last overload resolution and a large purry-typed surface; exercises overload selection and bidirectional generic inference.",

  "ts-morph-project":
    "TypeScript compiler-API wrapper with a large AST class hierarchy, compiler-API generic surface, and declaration-heavy graph; exercises deep inheritance and generic propagation.",

  "arktype-project":
    "Schema library with a type-level string parser, recursive schema inference, and extreme conditional/template evaluation; one of the most demanding generic workloads.",

  "superstruct-project":
    "Struct combinator library with recursive object validation typing; exercises generic combinator composition and nested-type inference.",

  "runtypes-project":
    "Runtime type combinator library with static type extraction and recursive record definitions; stresses recursive generic instantiation and union/intersection handling.",

  "hotscript-project":
    "Type-level higher-order functions and lazy computations via lambda/HOF encoding; stresses deep recursive conditional and template-literal type composition.",

  "typebox-project":
    "JSON-schema type builder with recursive static inference and a large conditional/template surface; exercises both schema-building generics and JSON-schema structural subtyping.",

  "class-transformer-project":
    "Decorator-metadata transformation library with class transform generics; exercises legacy decorator typing, metadata reflection, and class-level generic inference.",

  "type-graphql-project":
    "Decorator-driven GraphQL schema builder with generic resolver typing and a class hierarchy; stresses decorator + generic co-evaluation and schema type construction.",

  "neverthrow-project":
    "Result/Ok/Err generic monad library with chaining and combine overloads; exercises generic monad composition and tuple-spread overload resolution.",

  "xstate-project":
    "State-machine library with config generics, template-literal event/state typing, and a deep recursive typegen surface; stresses structural generic inference under complex configs.",

  "mobx-project":
    "Reactive state library with observable/computed generics, decorator and annotation surface, and reaction inference; exercises decorator typing and complex property descriptor generics.",

  // ── application/dashboard canary rows ─────────────────────────────────────

  "umami-project":
    "Real-world analytics dashboard (Next.js) with JSX, `@/*` path aliases, and a broad pnpm dependency graph; validates end-to-end app-router type-checking.",

  "excalidraw-project":
    "Real-world collaborative whiteboard editor (React) with complex component-prop generics, canvas type bindings, and a large client-side codebase.",

  "dub-project":
    "Real-world link-management dashboard (Next.js) with server actions, Prisma ORM types, and multi-tenant data models; exercises framework + database generic integration.",

  "formbricks-project":
    "Real-world experience-management dashboard (Next.js) with complex form-schema types, multi-workspace data models, and a large shared-component library.",

  "typebot-project":
    "Real-world chatbot builder (Next.js) with dynamic block types, recursive flow-graph typing, and a rich visual-editor component tree.",

  "lobe-chat-project":
    "Real-world AI chat application (Next.js) with LLM provider generics, plugin-system typing, and a large internationalized component surface.",

  "supabase-studio-project":
    "Real-world database studio dashboard (Next.js) with Supabase client generics, generated schema types, and a complex data-grid component hierarchy.",

  "infisical-project":
    "Real-world secrets-management dashboard (Next.js) with role-based access typing, Zod validation schemas, and a large API-client generic surface.",

  "payload-project":
    "Real-world headless CMS (Next.js) with deeply generic collection/field configuration types, plugin typing, and generated schema inference.",

  "medusa-project":
    "Real-world e-commerce backend (Node.js/NestJS) with complex service generics, module system typing, and a large entity/repository class hierarchy.",

  "outline-project":
    "Real-world knowledge-base wiki (React) with rich-text editor generics, permission-model typing, and a large component and model layer.",

  "trigger-dev-project":
    "Real-world background-jobs dashboard (Remix) with job-definition generics, event-payload typing, and multi-environment configuration types.",

  "joplin-project":
    "Real-world note-taking desktop app (Electron) with cross-platform IPC typing, plugin API generics, and a large React component tree.",

  "directus-project":
    "Real-world data-platform API (Node.js) with dynamic collection generics, permission-schema typing, and a large extension/hook surface.",

  "n8n-project":
    "Real-world workflow-automation backend (Node.js) with dynamic node/parameter typing, credential generics, and a large plugin-registry surface.",

  "cal-com-project":
    "Real-world scheduling dashboard (Next.js) with availability-slot generics, Prisma schema types, and a complex booking-flow component tree.",

  "documenso-project":
    "Real-world document-signing application (Remix) with PDF-field generics, Prisma schema types, and multi-step form validation.",

  "affine-project":
    "Real-world knowledge workspace (React) with block-protocol generics, collaborative-editing types, and a large plugin-based component architecture.",

  "immich-server-project":
    "Real-world photo-management backend (NestJS) with entity/DTO generics, ML-metadata typing, and a large asset-processing service hierarchy.",

  "rocketchat-project":
    "Real-world team-chat platform (Meteor/React) with real-time message generics, room/permission typing, and a very large client-side component surface.",
};
