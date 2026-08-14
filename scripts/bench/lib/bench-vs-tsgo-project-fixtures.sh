#!/usr/bin/env bash

# Repetitive external-project fixture wrappers for bench-vs-tsgo.sh. This file
# is sourced after fixture paths and pins are initialized, so it intentionally
# shares the benchmark runner's shell scope.

ensure_valibot_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "valibot" "$VALIBOT_REPO" "$VALIBOT_REF" "$VALIBOT_DIR" 1 || return 1
}

ensure_msw_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "msw" "$MSW_REPO" "$MSW_REF" "$MSW_DIR" 1 || return 1
}

ensure_comlink_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "comlink" "$COMLINK_REPO" "$COMLINK_REF" "$COMLINK_DIR" 1 || return 1
}

ensure_effect_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "effect" "$EFFECT_REPO" "$EFFECT_REF" "$EFFECT_DIR" 1 || return 1
}

ensure_drizzle_orm_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "drizzle-orm" "$DRIZZLE_ORM_REPO" "$DRIZZLE_ORM_REF" "$DRIZZLE_ORM_DIR" 1 || return 1
}

ensure_ts_rest_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "ts-rest" "$TS_REST_REPO" "$TS_REST_REF" "$TS_REST_DIR" 1 || return 1
}

ensure_ofetch_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "ofetch" "$OFETCH_REPO" "$OFETCH_REF" "$OFETCH_DIR" 1 || return 1
}

ensure_ts_pattern_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "ts-pattern" "$TS_PATTERN_REPO" "$TS_PATTERN_REF" "$TS_PATTERN_DIR" 1 || return 1
}

ensure_radash_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "radash" "$RADASH_REPO" "$RADASH_REF" "$RADASH_DIR" 1 || return 1
}

ensure_valtio_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "valtio" "$VALTIO_REPO" "$VALTIO_REF" "$VALTIO_DIR" 1 || return 1
}

ensure_scule_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "scule" "$SCULE_REPO" "$SCULE_REF" "$SCULE_DIR" 1 || return 1
}

ensure_mitt_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "mitt" "$MITT_REPO" "$MITT_REF" "$MITT_DIR" 1 || return 1
}

ensure_change_case_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "change-case" "$CHANGE_CASE_REPO" "$CHANGE_CASE_REF" "$CHANGE_CASE_DIR" 1 || return 1
}

ensure_tiny_invariant_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "tiny-invariant" "$TINY_INVARIANT_REPO" "$TINY_INVARIANT_REF" "$TINY_INVARIANT_DIR" 1 || return 1
}

ensure_ts_belt_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "ts-belt" "$TS_BELT_REPO" "$TS_BELT_REF" "$TS_BELT_DIR" 1 || return 1
}

ensure_ts_extras_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "ts-extras" "$TS_EXTRAS_REPO" "$TS_EXTRAS_REF" "$TS_EXTRAS_DIR" 1 || return 1
}

ensure_superjson_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "superjson" "$SUPERJSON_REPO" "$SUPERJSON_REF" "$SUPERJSON_DIR" 1 || return 1
}

ensure_trpc_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "trpc" "$TRPC_REPO" "$TRPC_REF" "$TRPC_DIR" 1 || return 1
}

ensure_tanstack_query_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "tanstack-query" "$TANSTACK_QUERY_REPO" "$TANSTACK_QUERY_REF" "$TANSTACK_QUERY_DIR" 1 || return 1
}

ensure_tanstack_router_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "tanstack-router" "$TANSTACK_ROUTER_REPO" "$TANSTACK_ROUTER_REF" "$TANSTACK_ROUTER_DIR" 1 || return 1
}

ensure_zustand_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "zustand" "$ZUSTAND_REPO" "$ZUSTAND_REF" "$ZUSTAND_DIR" 1 || return 1
}

ensure_jotai_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "jotai" "$JOTAI_REPO" "$JOTAI_REF" "$JOTAI_DIR" 1 || return 1
}

ensure_fp_ts_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "fp-ts" "$FP_TS_REPO" "$FP_TS_REF" "$FP_TS_DIR" 1 || return 1
}

ensure_io_ts_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "io-ts" "$IO_TS_REPO" "$IO_TS_REF" "$IO_TS_DIR" 1 || return 1
}

ensure_immer_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "immer" "$IMMER_REPO" "$IMMER_REF" "$IMMER_DIR" 1 || return 1
}

ensure_remeda_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "remeda" "$REMEDA_REPO" "$REMEDA_REF" "$REMEDA_DIR" 1 || return 1
}

ensure_ts_morph_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "ts-morph" "$TS_MORPH_REPO" "$TS_MORPH_REF" "$TS_MORPH_DIR" 1 || return 1
}

ensure_arktype_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "arktype" "$ARKTYPE_REPO" "$ARKTYPE_REF" "$ARKTYPE_DIR" 1 || return 1
}

ensure_superstruct_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "superstruct" "$SUPERSTRUCT_REPO" "$SUPERSTRUCT_REF" "$SUPERSTRUCT_DIR" 1 || return 1
}

ensure_runtypes_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "runtypes" "$RUNTYPES_REPO" "$RUNTYPES_REF" "$RUNTYPES_DIR" 1 || return 1
}

ensure_hotscript_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "hotscript" "$HOTSCRIPT_REPO" "$HOTSCRIPT_REF" "$HOTSCRIPT_DIR" 1 || return 1
}

ensure_typebox_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "typebox" "$TYPEBOX_REPO" "$TYPEBOX_REF" "$TYPEBOX_DIR" 1 || return 1
}

ensure_class_transformer_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "class-transformer" "$CLASS_TRANSFORMER_REPO" "$CLASS_TRANSFORMER_REF" "$CLASS_TRANSFORMER_DIR" 1 || return 1
}

ensure_type_graphql_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "type-graphql" "$TYPE_GRAPHQL_REPO" "$TYPE_GRAPHQL_REF" "$TYPE_GRAPHQL_DIR" 1 || return 1
}

ensure_neverthrow_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "neverthrow" "$NEVERTHROW_REPO" "$NEVERTHROW_REF" "$NEVERTHROW_DIR" 1 || return 1
}

ensure_xstate_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "xstate" "$XSTATE_REPO" "$XSTATE_REF" "$XSTATE_DIR" 1 || return 1
}

ensure_mobx_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "mobx" "$MOBX_REPO" "$MOBX_REF" "$MOBX_DIR" 1 || return 1
}
