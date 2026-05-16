#!/usr/bin/env bash
#
# Shared project fixture metadata and config writers for benchmark and CI
# project-compile guards. Keep fixture pins and generated tsconfig shapes here
# so benchmark rows and compile guards cannot silently drift.

# External project fixture repositories and pinned refs.
: "${UTILITY_TYPES_REPO:=https://github.com/piotrwitek/utility-types.git}"
: "${UTILITY_TYPES_REF:=2ee1f6ecb241651ab22390fee7ee5349942efda2}"
: "${TS_TOOLBELT_REPO:=https://github.com/millsp/ts-toolbelt.git}"
: "${TS_TOOLBELT_REF:=b8a49285e3ed3a7d8bb8e0b433389eac46a5f140}"
: "${TS_ESSENTIALS_REPO:=https://github.com/ts-essentials/ts-essentials.git}"
: "${TS_ESSENTIALS_REF:=5abe8700b42068048bd3c368e0531b6defe56558}"
: "${NEXTJS_REPO:=https://github.com/vercel/next.js.git}"
: "${NEXTJS_REF:=09851e208cc62c8b6fe7a953b42c88e843129178}"
: "${RXJS_REPO:=https://github.com/ReactiveX/rxjs.git}"
: "${RXJS_REF:=e5351d02e225e275ac0e497c7b66eaa5f0c88791}"
: "${TYPE_FEST_REPO:=https://github.com/sindresorhus/type-fest.git}"
: "${TYPE_FEST_REF:=4005f60b65a7bd224154d6da46f45a63b42ce70f}"
: "${ZOD_REPO:=https://github.com/colinhacks/zod.git}"
: "${ZOD_REF:=93b0b6892cc0cfee8d0bec4e2e1242c7df771f95}"
: "${KYSELY_REPO:=https://github.com/kysely-org/kysely.git}"
: "${KYSELY_REF:=d4911be21cd568d3694dc7f879f72390635226d7}"
: "${LARGE_TS_REPO:=https://github.com/mohsen1/large-ts-repo.git}"
: "${LARGE_TS_REF:=e1b22bda18664a507ed0da19c155e0365d585b18}"
: "${TYPE_CHALLENGES_REPO:=https://github.com/type-challenges/type-challenges.git}"
: "${TYPE_CHALLENGES_REF:=0b0b0b18bcb7ac42dc22ce26ffb438231d4754b1}"

tsz_ensure_git_fixture() {
  local name="$1"
  local repo="$2"
  local ref="$3"
  local dir="$4"
  local reclone_dirty="${5:-0}"

  mkdir -p "$(dirname "$dir")"
  if [[ ! -d "$dir/.git" ]]; then
    echo "Cloning ${name} fixture..."
    rm -rf "$dir"
    git clone --quiet --no-tags --depth 1 "$repo" "$dir"
  fi

  if [[ "$reclone_dirty" == "1" ]] \
    && [[ -n "$(git -C "$dir" status --porcelain 2>/dev/null)" ]]; then
    echo "${name} fixture is dirty; recloning for reproducibility..."
    rm -rf "$dir"
    git clone --quiet --no-tags --depth 1 "$repo" "$dir"
  fi

  if [[ -n "$ref" ]]; then
    local current_ref
    current_ref="$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)"
    if [[ "$current_ref" != "$ref" ]]; then
      echo "Pinning ${name} to ${ref:0:12}..."
      git -C "$dir" fetch --quiet --depth 1 origin "$ref"
      git -C "$dir" checkout --quiet --detach FETCH_HEAD
    fi
  fi
}

tsz_rxjs_src_root() {
  local fixture_dir="$1"
  if [[ -d "$fixture_dir/packages/rxjs/src/internal" ]]; then
    printf '%s\n' "packages/rxjs/src"
  else
    printf '%s\n' "src"
  fi
}

tsz_write_utility_types_config() {
  local output="$1"
  cat > "$output" <<'JSON'
{
  "compilerOptions": {
    "strict": true,
    "lib": ["dom", "es2017"],
    "types": [],
    "target": "ES2015",
    "module": "commonjs",
    "skipLibCheck": true,
    "noEmit": true
  },
  "include": ["src/**/*.ts"],
  "exclude": ["src/**/*.snap.ts", "src/**/*.spec.ts"]
}
JSON
}

tsz_write_ts_toolbelt_config() {
  local output="$1"
  cat > "$output" <<'JSON'
{
  "compilerOptions": {
    "target": "ES2015",
    "module": "commonjs",
    "lib": ["esnext", "dom"],
    "types": [],
    "strict": false,
    "strictNullChecks": true,
    "strictFunctionTypes": true,
    "noImplicitAny": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "esModuleInterop": true,
    "downlevelIteration": true,
    "forceConsistentCasingInFileNames": true,
    "skipLibCheck": true,
    "noEmit": true,
    "ignoreDeprecations": "6.0"
  },
  "include": ["sources/**/*.ts"],
  "exclude": ["tests/**/*", "scripts/**/*", "node_modules/**/*"]
}
JSON
}

tsz_write_ts_essentials_config() {
  local output="$1"
  cat > "$output" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "module": "commonjs",
    "strict": true,
    "lib": ["es2018"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["lib/**/*.ts"],
  "exclude": ["test/**/*", "node_modules/**/*"]
}
JSON
}

tsz_write_rxjs_config() {
  local output="$1"
  local rxjs_src_root="$2"
  cat > "$output" <<JSON
{
  "compilerOptions": {
    "target": "es2017",
    "module": "esnext",
    "strict": true,
    "lib": ["es2018", "dom"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "noCheck": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler"
  },
  "include": ["${rxjs_src_root}/internal/**/*.ts"],
  "exclude": [
    "**/*.spec.ts",
    "**/*.test.ts",
    "node_modules/**/*",
    "**/internal/observable/dom/**",
    "**/internal/umd.ts"
  ]
}
JSON
}

tsz_write_type_fest_config() {
  local output="$1"
  cat > "$output" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "module": "esnext",
    "strict": true,
    "lib": ["es2022"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler"
  },
  "include": ["source/**/*.d.ts", "index.d.ts"],
  "exclude": ["test-d/**/*", "node_modules/**/*"]
}
JSON
}

tsz_write_zod_config() {
  local output="$1"
  cat > "$output" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "module": "esnext",
    "strict": true,
    "lib": ["es2022", "dom"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler"
  },
  "include": ["src/**/*.ts", "packages/zod/src/**/*.ts"],
  "exclude": [
    "**/*.test.ts",
    "**/__tests__/**",
    "**/benchmarks/**",
    "node_modules/**/*"
  ]
}
JSON
}

tsz_write_kysely_globals() {
  local output="$1"
  cat > "$output" <<'GLOBALSEOF'
declare const Buffer: {
  isBuffer(value: unknown): boolean;
  compare(left: unknown, right: unknown): number;
};
GLOBALSEOF
}

tsz_write_kysely_config() {
  local output="$1"
  cat > "$output" <<'JSON'
{
  "compilerOptions": {
    "target": "es2017",
    "module": "esnext",
    "strict": true,
    "lib": ["es2022", "dom"],
    "types": [],
    "skipLibCheck": true,
    "noEmit": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler"
  },
  "include": ["src/**/*.ts", "tsz-bench-globals.d.ts"],
  "exclude": [
    "**/*.test.ts",
    "test/**/*",
    "node_modules/**/*",
    "**/dialect/mssql/**",
    "**/util/object-utils.ts",
    "**/util/performance-now.ts"
  ]
}
JSON
}
