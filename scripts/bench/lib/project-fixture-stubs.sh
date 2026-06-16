#!/usr/bin/env bash
#
# External-module stub writers for benchmark / CI project-compile fixtures.
# Sourced by scripts/bench/project-fixtures.sh after TSZ_PROJECT_FIXTURES_ROOT
# is resolved. Each function emits self-contained ambient `.d.ts` stubs (and the
# `node_modules` shims they live under) for fixture rows whose real third-party
# dependencies are absent in the clone-only fixture checkout. They take a single
# argument -- the fixture's tsconfig output path -- and derive the fixture
# directory from it; they read no caller-side globals.
#
# Moved out of project-fixtures.sh to keep that file under the 2000-line shard
# ceiling. The heredoc convention and per-stub comments are preserved verbatim.

# Guard against double-sourcing (the stub writers are plain function defs, but
# re-sourcing is wasteful and can shadow a parent that already defined them).
if [ -n "${_TSZ_PROJECT_FIXTURE_STUBS_SOURCED:-}" ]; then
  return 0 2>/dev/null || true
fi
_TSZ_PROJECT_FIXTURE_STUBS_SOURCED=1

tsz_write_drizzle_orm_external_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"

  mkdir -p \
    "$fixture_dir/node_modules/@cloudflare/workers-types" \
    "$fixture_dir/node_modules/bun-types"

  cat > "$fixture_dir/tsz-bench-external-module.d.ts" <<'TYPES'
declare const tszBenchExternalModule: any;
export = tszBenchExternalModule;
TYPES

  cat > "$fixture_dir/tsz-bench-external-named-modules.d.ts" <<'TYPES'
type Buffer = any;
declare const Buffer: {
  isBuffer(value: unknown): boolean;
  compare(left: unknown, right: unknown): number;
  from(value: unknown, encoding?: string): Buffer;
};

interface ErrorConstructor {
  captureStackTrace?(targetObject: object, constructorOpt?: Function): void;
}

interface DurableObjectStorage {
  [key: string]: any;
}

type SqlStorageCursor<T = Record<string, unknown>> = any;
type SqlStorageValue = any;
type D1Response = any;

declare module '@aws-sdk/client-rds-data' {
  export class BeginTransactionCommand {
    constructor(input?: any);
  }
  export interface ColumnMetadata {
    name?: any;
    [key: string]: any;
  }
  export class CommitTransactionCommand {
    constructor(input?: any);
  }
  export class ExecuteStatementCommand {
    input: any;
    constructor(input?: any);
  }
  export interface ExecuteStatementCommandOutput {
    records?: Field[][];
    columnMetadata?: ColumnMetadata[];
    [key: string]: any;
  }
  export interface Field {
    arrayValue?: {
      stringValues?: any;
      longValues?: any;
      doubleValues?: any;
      booleanValues?: any;
      arrayValues?: any;
      [key: string]: any;
    };
    blobValue?: any;
    booleanValue?: any;
    doubleValue?: any;
    isNull?: any;
    longValue?: any;
    stringValue?: any;
    [key: string]: any;
  }
  export class RDSDataClient {
    constructor(config?: RDSDataClientConfig);
    send(command: any): Promise<any>;
    [key: string]: any;
  }
  export interface RDSDataClientConfig {
    [key: string]: any;
  }
  export class RollbackTransactionCommand {
    constructor(input?: any);
  }
  export const TypeHint: any;
  export type TypeHint = any;
}

declare module '@electric-sql/pglite' {
  export const PGlite: any;
  export type PGlite<T = any, U = any, V = any, W = any> = any;
  export const PGliteOptions: any;
  export type PGliteOptions<T = any, U = any, V = any, W = any> = any;
  export const QueryOptions: any;
  export type QueryOptions<T = any, U = any, V = any, W = any> = any;
  export const Results: any;
  export type Results<T = any, U = any, V = any, W = any> = any;
  export const Row: any;
  export type Row<T = any, U = any, V = any, W = any> = any;
  export const Transaction: any;
  export type Transaction<T = any, U = any, V = any, W = any> = any;
  export const types: any;
  export type types<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const InArgs: any;
  export type InArgs<T = any, U = any, V = any, W = any> = any;
  export const InStatement: any;
  export type InStatement<T = any, U = any, V = any, W = any> = any;
  export const ResultSet: any;
  export type ResultSet<T = any, U = any, V = any, W = any> = any;
  export const Transaction: any;
  export type Transaction<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client-wasm' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/http' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/node' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/sqlite3' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/web' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@libsql/client/ws' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@miniflare/d1' {
  export const D1Database: any;
  export type D1Database<T = any, U = any, V = any, W = any> = any;
}

declare module '@neondatabase/serverless' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const FullQueryResults: any;
  export type FullQueryResults<T = any, U = any, V = any, W = any> = any;
  export const HTTPQueryOptions: any;
  export type HTTPQueryOptions<T = any, U = any, V = any, W = any> = any;
  export const HTTPTransactionOptions: any;
  export type HTTPTransactionOptions<T = any, U = any, V = any, W = any> = any;
  export const NeonQueryFunction: any;
  export type NeonQueryFunction<T = any, U = any, V = any, W = any> = any;
  export const NeonQueryPromise: any;
  export type NeonQueryPromise<T = any, U = any, V = any, W = any> = any;
  export const Pool: any;
  export type Pool<T = any, U = any, V = any, W = any> = any;
  export const PoolClient: any;
  export type PoolClient<T = any, U = any, V = any, W = any> = any;
  export const PoolConfig: any;
  export type PoolConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryArrayConfig: any;
  export type QueryArrayConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryConfig: any;
  export type QueryConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryResult: any;
  export type QueryResult<T = any, U = any, V = any, W = any> = any;
  export const QueryResultRow: any;
  export type QueryResultRow<T = any, U = any, V = any, W = any> = any;
  export const neon: any;
  export type neon<T = any, U = any, V = any, W = any> = any;
  export const neonConfig: any;
  export type neonConfig<T = any, U = any, V = any, W = any> = any;
  export const types: any;
  export type types<T = any, U = any, V = any, W = any> = any;
}

declare module '@netlify/db' {
  export const getDatabase: any;
  export type getDatabase<T = any, U = any, V = any, W = any> = any;
}

declare module '@op-engineering/op-sqlite' {
  export const OPSQLiteConnection: any;
  export type OPSQLiteConnection<T = any, U = any, V = any, W = any> = any;
  export const QueryResult: any;
  export type QueryResult<T = any, U = any, V = any, W = any> = any;
}

declare module '@opentelemetry/api' {
  export const Span: any;
  export type Span<T = any, U = any, V = any, W = any> = any;
  export const Tracer: any;
  export type Tracer<T = any, U = any, V = any, W = any> = any;
}

declare module '@planetscale/database' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const Connection: any;
  export type Connection<T = any, U = any, V = any, W = any> = any;
  export const ExecutedQuery: any;
  export type ExecutedQuery<T = any, U = any, V = any, W = any> = any;
  export const Transaction: any;
  export type Transaction<T = any, U = any, V = any, W = any> = any;
}

declare module '@prisma/client' {
  export const Prisma: any;
  export type Prisma<T = any, U = any, V = any, W = any> = any;
}

declare module '@prisma/client/extension' {
  export const PrismaClient: any;
  export type PrismaClient<T = any, U = any, V = any, W = any> = any;
}

declare module '@tidbcloud/serverless' {
  export const Config: any;
  export type Config<T = any, U = any, V = any, W = any> = any;
  export const Connection: any;
  export type Connection<T = any, U = any, V = any, W = any> = any;
  export const ExecuteOptions: any;
  export type ExecuteOptions<T = any, U = any, V = any, W = any> = any;
  export const FullResult: any;
  export type FullResult<T = any, U = any, V = any, W = any> = any;
  export const Tx: any;
  export type Tx<T = any, U = any, V = any, W = any> = any;
  export const connect: any;
  export type connect<T = any, U = any, V = any, W = any> = any;
}

declare module '@upstash/redis' {
  export const Redis: any;
  export type Redis<T = any, U = any, V = any, W = any> = any;
}

declare module '@vercel/postgres' {
  export const QueryArrayConfig: any;
  export type QueryArrayConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryConfig: any;
  export type QueryConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryResult: any;
  export type QueryResult<T = any, U = any, V = any, W = any> = any;
  export const QueryResultRow: any;
  export type QueryResultRow<T = any, U = any, V = any, W = any> = any;
  export const VercelClient: any;
  export type VercelClient<T = any, U = any, V = any, W = any> = any;
  export const VercelPool: any;
  export type VercelPool<T = any, U = any, V = any, W = any> = any;
  export const VercelPoolClient: any;
  export type VercelPoolClient<T = any, U = any, V = any, W = any> = any;
  export const sql: any;
  export type sql<T = any, U = any, V = any, W = any> = any;
  export const types: any;
  export type types<T = any, U = any, V = any, W = any> = any;
}

declare module '@xata.io/client' {
  export const SQLPluginResult: any;
  export type SQLPluginResult<T = any, U = any, V = any, W = any> = any;
  export const SQLQueryResult: any;
  export type SQLQueryResult<T = any, U = any, V = any, W = any> = any;
}

declare module 'better-sqlite3' {
  export const Database: any;
  export type Database<T = any, U = any, V = any, W = any> = any;
  export const RunResult: any;
  export type RunResult<T = any, U = any, V = any, W = any> = any;
  export const Statement: any;
  export type Statement<T = any, U = any, V = any, W = any> = any;
}

declare module 'bun' {
  export const SQL: any;
  export type SQL<T = any, U = any, V = any, W = any> = any;
  export const SQLOptions: any;
  export type SQLOptions<T = any, U = any, V = any, W = any> = any;
  export const SavepointSQL: any;
  export type SavepointSQL<T = any, U = any, V = any, W = any> = any;
  export const TransactionSQL: any;
  export type TransactionSQL<T = any, U = any, V = any, W = any> = any;
}

declare module 'bun:sqlite' {
  export const Database: any;
  export type Database<T = any, U = any, V = any, W = any> = any;
  export const Statement: any;
  export type Statement<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm' {
  export const sql: any;
  export type sql<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/Gel-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const exceptAll: any;
  export type exceptAll<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const intersectAll: any;
  export type intersectAll<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/gel-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const exceptAll: any;
  export type exceptAll<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const intersectAll: any;
  export type intersectAll<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/mysql-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const exceptAll: any;
  export type exceptAll<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const intersectAll: any;
  export type intersectAll<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/pg-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const exceptAll: any;
  export type exceptAll<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const intersectAll: any;
  export type intersectAll<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/singlestore-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const minus: any;
  export type minus<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'drizzle-orm/sqlite-core' {
  export const except: any;
  export type except<T = any, U = any, V = any, W = any> = any;
  export const intersect: any;
  export type intersect<T = any, U = any, V = any, W = any> = any;
  export const union: any;
  export type union<T = any, U = any, V = any, W = any> = any;
  export const unionAll: any;
  export type unionAll<T = any, U = any, V = any, W = any> = any;
}

declare module 'expo-sqlite' {
  export const SQLiteDatabase: any;
  export type SQLiteDatabase<T = any, U = any, V = any, W = any> = any;
  export const SQLiteRunResult: any;
  export type SQLiteRunResult<T = any, U = any, V = any, W = any> = any;
  export const SQLiteStatement: any;
  export type SQLiteStatement<T = any, U = any, V = any, W = any> = any;
  export const addDatabaseChangeListener: any;
  export type addDatabaseChangeListener<T = any, U = any, V = any, W = any> = any;
  export const useEffect: any;
  export type useEffect<T = any, U = any, V = any, W = any> = any;
  export const useState: any;
  export type useState<T = any, U = any, V = any, W = any> = any;
}

declare module 'gel' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const ConnectOptions: any;
  export type ConnectOptions<T = any, U = any, V = any, W = any> = any;
  export const DateDuration: any;
  export type DateDuration<T = any, U = any, V = any, W = any> = any;
  export const Duration: any;
  export type Duration<T = any, U = any, V = any, W = any> = any;
  export const LocalDate: any;
  export type LocalDate<T = any, U = any, V = any, W = any> = any;
  export const LocalDateTime: any;
  export type LocalDateTime<T = any, U = any, V = any, W = any> = any;
  export const LocalTime: any;
  export type LocalTime<T = any, U = any, V = any, W = any> = any;
  export const RelativeDuration: any;
  export type RelativeDuration<T = any, U = any, V = any, W = any> = any;
  export const createClient: any;
  export type createClient<T = any, U = any, V = any, W = any> = any;
}

declare module 'gel/dist/transaction' {
  export const Transaction: any;
  export type Transaction<T = any, U = any, V = any, W = any> = any;
}

declare module 'knex' {
  export const Knex: any;
  export type Knex<T = any, U = any, V = any, W = any> = any;
}

declare module 'kysely' {
  export const ColumnType: any;
  export type ColumnType<T = any, U = any, V = any, W = any> = any;
}

declare module 'mysql2' {
  export const Connection: any;
  export type Connection<T = any, U = any, V = any, W = any> = any;
  export const Pool: any;
  export type Pool<T = any, U = any, V = any, W = any> = any;
  export const PoolOptions: any;
  export type PoolOptions<T = any, U = any, V = any, W = any> = any;
  export const createPool: any;
  export type createPool<T = any, U = any, V = any, W = any> = any;
}

declare module 'mysql2/promise' {
  export const Connection: any;
  export type Connection<T = any, U = any, V = any, W = any> = any;
  export const FieldPacket: any;
  export type FieldPacket<T = any, U = any, V = any, W = any> = any;
  export const OkPacket: any;
  export type OkPacket<T = any, U = any, V = any, W = any> = any;
  export const Pool: any;
  export type Pool<T = any, U = any, V = any, W = any> = any;
  export const PoolConnection: any;
  export type PoolConnection<T = any, U = any, V = any, W = any> = any;
  export const QueryOptions: any;
  export type QueryOptions<T = any, U = any, V = any, W = any> = any;
  export const ResultSetHeader: any;
  export type ResultSetHeader<T = any, U = any, V = any, W = any> = any;
  export const RowDataPacket: any;
  export type RowDataPacket<T = any, U = any, V = any, W = any> = any;
}

declare module 'node:events' {
  export const once: any;
  export type once<T = any, U = any, V = any, W = any> = any;
}

declare module 'pg' {
  export const Client: any;
  export type Client<T = any, U = any, V = any, W = any> = any;
  export const PoolClient: any;
  export type PoolClient<T = any, U = any, V = any, W = any> = any;
  export const QueryArrayConfig: any;
  export type QueryArrayConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryConfig: any;
  export type QueryConfig<T = any, U = any, V = any, W = any> = any;
  export const QueryResult: any;
  export type QueryResult<T = any, U = any, V = any, W = any> = any;
  export const QueryResultRow: any;
  export type QueryResultRow<T = any, U = any, V = any, W = any> = any;
}

declare module 'postgres' {
  export const Row: any;
  export type Row<T = any, U = any, V = any, W = any> = any;
  export const RowList: any;
  export type RowList<T = any, U = any, V = any, W = any> = any;
  export const Sql: any;
  export type Sql<T = any, U = any, V = any, W = any> = any;
  export const TransactionSql: any;
  export type TransactionSql<T = any, U = any, V = any, W = any> = any;
}

declare module 'react' {
  export const useEffect: any;
  export type useEffect<T = any, U = any, V = any, W = any> = any;
  export const useReducer: any;
  export type useReducer<T = any, U = any, V = any, W = any> = any;
  export const useState: any;
  export type useState<T = any, U = any, V = any, W = any> = any;
}

declare module 'sql.js' {
  export const BindParams: any;
  export type BindParams<T = any, U = any, V = any, W = any> = any;
  export const Database: any;
  export type Database<T = any, U = any, V = any, W = any> = any;
}
TYPES

  cat > "$fixture_dir/node_modules/bun-types/index.d.ts" <<'TYPES'
// Package marker for `/// <reference types="bun-types" />`.
TYPES

  cat > "$fixture_dir/node_modules/@cloudflare/workers-types/index.d.ts" <<'TYPES'
interface D1Database {
  [key: string]: any;
}

interface D1PreparedStatement {
  [key: string]: any;
  bind(...values: any[]): D1PreparedStatement;
}

interface D1Result<T = unknown> {
  [key: string]: any;
  results: T[];
}
TYPES
}

tsz_write_ts_rest_external_stubs() {
  # @ts-rest/core depends on `zod` (and `zod4/v4`) as a peer dependency. The
  # fixture clone runs no npm install and the bench baseline pins `"types": []`,
  # so without a stub tsc emits spurious TS2307 "Cannot find module 'zod'", which
  # cascades into TS2536 where generic params constrained by `z.AnyZodObject` are
  # indexed (`A['shape']`, `B['_def']['unknownKeys']`). Provide a permissive `z`
  # namespace whose every value/type member resolves to `any` so the constraints
  # and index accesses succeed, mirroring what tsc sees with zod's real types.
  # @ts-rest/core's OWN `./` source imports remain real (resolved by the
  # filesystem, not these ambient stubs).
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
declare module 'zod' {
  export type ZodError<T = any> = any;
  export const ZodError: any;
  export type ZodIssue = any;
  export const ZodIssue: any;
  export type ZodObject<A = any, B = any, C = any, D = any, E = any> = any;
  export const ZodObject: any;
  export namespace z {
    type infer<T = any> = any;
    type input<T = any> = any;
    type output<T = any> = any;
    // ts-rest indexes generic params constrained by these shapes
    // (`A['shape']`, `B['_def']['unknownKeys']`) and narrows them with
    // `'innerType' in obj`/`obj.shape`, so they must be object types with the
    // referenced members rather than bare `any` (which leaves a type-parameter
    // index unresolved and trips TS2536/TS2339).
    interface AnyZodObject {
      shape: any;
      _def: { unknownKeys: any; catchall: any; [key: string]: any };
      [key: string]: any;
    }
    interface ZodEffects<A = any, B = any, C = any> {
      innerType: any;
      [key: string]: any;
    }
    type ZodObject<A = any, B = any, C = any, D = any, E = any> = any;
    type ZodError<T = any> = any;
    type ZodIssueCode = any;
    type ZodNumber = any;
    type ZodOptional<T = any> = any;
    type ZodSchema<T = any> = any;
    type ZodString = any;
    type ZodTypeAny = any;
    const any: any;
    const array: any;
    const boolean: any;
    const coerce: any;
    const literal: any;
    const nativeEnum: any;
    const number: any;
    const object: any;
    const string: any;
    const union: any;
    const ZodError: any;
    const ZodIssueCode: any;
    namespace objectUtil {
      type MergeShapes<A = any, B = any> = any;
    }
  }
  export const z: typeof z;
}

declare module 'zod4/v4' {
  // `zod4/v4`'s `z` is only used in value position (`z4.string()`, `z4.union()`,
  // `z4.null()`); a single `any` export covers all member accesses.
  export const z: any;
}
TYPES
}

tsz_write_trpc_external_stubs() {
  # trpc's real root tsconfig sets `"types": ["node", "vitest/globals"]`, so the
  # upstream build resolves `NodeJS.Timeout` (used in packages/server/src/adapters/ws.ts)
  # via @types/node. The shared bench baseline pins `"types": []` and the fixture
  # clone has no node_modules, so without a stub tsz/tsc both emit spurious
  # TS2833 "Cannot find namespace 'NodeJS'". Mirror the kysely/type-graphql stub
  # pattern: alias the referenced members as `= any` so the global resolves
  # without unmasking unrelated assignability diffs.
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
declare namespace NodeJS {
  type Timeout = any;
  type Timer = any;
  type Immediate = any;
  type ErrnoException = any;
}
TYPES
}

tsz_write_zustand_external_stubs() {
  # zustand's source imports third-party packages (react, immer, the
  # @redux-devtools/extension Window augmentation, use-sync-external-store) and
  # the fixture clone runs no npm install with the bench baseline pinning
  # `"types": []`, so without stubs tsc emits spurious TS2307 plus a TS2339
  # cascade where `window.__REDUX_DEVTOOLS_EXTENSION__` (which the real
  # @redux-devtools/extension package adds to the `Window` interface) is read.
  # Provide named ambient any-modules plus the Window augmentation so tsc matches
  # its real-deps view; zustand's own `./` source stays real-checked.
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
declare module 'react' {
  // React's default export is dotted for hooks, some with explicit type args
  // (`React.useRef<U>(...)`); a generic call signature avoids TS2347 while the
  // index signature keeps every other member `any`.
  const React: {
    useRef: <T = any>(...args: any[]) => any;
    useCallback: <T = any>(...args: any[]) => any;
    useDebugValue: (...args: any[]) => any;
    useSyncExternalStore: (...args: any[]) => any;
    [key: string]: any;
  };
  export default React;
}

declare module 'immer' {
  export const produce: any;
  export type Draft<T = any> = any;
}

declare module 'use-sync-external-store/shim/with-selector' {
  // Imported as a default and invoked with explicit type args; a generic call
  // signature satisfies the type-arg call.
  const _default: { <T = any, U = any>(...args: any[]): any; [key: string]: any };
  export default _default;
}

declare module '@redux-devtools/extension';

interface Window {
  // `@redux-devtools/extension` augments `Window` with this connector; zustand
  // derives `type Config = Parameters<...['connect']>[0]` and extends it as an
  // interface, so the value must expose a `connect` method whose first argument
  // is an object type rather than collapsing to `unknown`.
  __REDUX_DEVTOOLS_EXTENSION__?: {
    connect(options: { [key: string]: any }): any;
    [key: string]: any;
  };
}
TYPES
  tsz_write_basic_external_project_config "$1" "src" \
    '    "allowImportingTsExtensions": true,
' \
    ', "tsz-bench-globals.d.ts"'
}

tsz_write_jotai_external_stubs() {
  # jotai's source imports react (hooks + JSX types) and the babel-plugin
  # entrypoints @babel/core / @babel/template. The fixture clone runs no npm
  # install and the bench baseline pins `"types": []`, so without stubs tsc emits
  # spurious TS2307 plus cascades: babel visitor callbacks become implicit-any
  # (TS7006), `babel`/`babel.Node` cannot resolve as a namespace (TS2503), and
  # `Symbol.observable` (which rxjs-style deps add to `SymbolConstructor`) is
  # missing (TS2339). Model react with generic-callable hooks, model @babel/core
  # as an `export =` namespace whose `PluginObj.visitor` methods type their
  # params, and augment `SymbolConstructor`. jotai's own `./` source stays real.
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
declare module 'react' {
  export type ReactNode = any;
  export type ReactElement<P = any, T = any> = any;
  export type FunctionComponent<P = any> = any;
  export const createContext: <T = any>(...args: any[]) => any;
  export const createElement: (...args: any[]) => any;
  export const useContext: <T = any>(...args: any[]) => any;
  export const useRef: <T = any>(...args: any[]) => any;
  // The reducer first argument is a callback whose params must be typed `any`
  // or jotai's inline `(prev) => ...` reducer trips TS7006.
  export const useReducer: <A = any, B = any, C = any>(
    reducer: (...args: any[]) => any,
    ...rest: any[]
  ) => any;
  // `useMemo` must preserve the factory's return type so jotai's
  // `useMemo(() => atom(...))` keeps the specific atom type that `useSetAtom`
  // then matches against `SetAtom<Args, Result>` (a bare `any` return collapses
  // it to `SetAtom<unknown[], unknown>` and trips TS2322).
  export const useMemo: <T = any>(factory: () => T, deps?: any) => T;
  export const useCallback: <T = any>(...args: any[]) => any;
  export const useEffect: (...args: any[]) => any;
  export const useDebugValue: (...args: any[]) => any;
  export const use: <T = any>(...args: any[]) => any;
  const React: {
    use: <T = any>(...args: any[]) => any;
    [key: string]: any;
  };
  export default React;
}

declare module '@babel/core' {
  // `export =` a namespace so the default import can be dotted as a namespace
  // (`babel.Node`, `babel.PluginItem`) while named imports (`{ PluginObj, types }`)
  // resolve through esModuleInterop. `PluginObj.visitor`'s methods carry a
  // `(...args: any[]) => any` signature so jotai's inline visitor callbacks get
  // a contextual any instead of tripping TS7006.
  namespace babel {
    export type Node = any;
    export type PluginItem = any;
    export interface PluginObj {
      // Visitor entries are either a method or an `{ enter?, exit? }` object;
      // both forms type their path/state params as `any`. `pre`/`post` are
      // top-level lifecycle hooks whose destructured args must also be `any`.
      visitor?: {
        [key: string]:
          | ((...args: any[]) => any)
          | { enter?: (...args: any[]) => any; exit?: (...args: any[]) => any };
      };
      pre?: (...args: any[]) => any;
      post?: (...args: any[]) => any;
      [key: string]: any;
    }
    export const types: any;
  }
  export = babel;
}

// `@babel/core`'s `babel` namespace is also referenced globally (without an
// import) as `babel.types.Expression`; @types/babel__core exposes it ambiently.
declare namespace babel {
  namespace types {
    type Expression = any;
    type V8IntrinsicIdentifier = any;
    type Node = any;
  }
  type Node = any;
  type PluginItem = any;
}

declare module '@babel/template' {
  const templateBuilder: any;
  export default templateBuilder;
}

// jotai's atomWithObservable reads `Symbol.observable`, which rxjs-style deps
// add to the global `SymbolConstructor`.
interface SymbolConstructor { readonly observable: symbol; }
TYPES
  tsz_write_basic_external_project_config "$1" "src" \
    '    "allowImportingTsExtensions": true,
' \
    ', "tsz-bench-globals.d.ts"'
}

tsz_write_type_graphql_external_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"

  cat > "$fixture_dir/tsz-bench-external-module.d.ts" <<'TYPES'
declare const tszBenchExternalModule: any;
export = tszBenchExternalModule;
TYPES

  cat > "$fixture_dir/tsz-bench-external-named-modules.d.ts" <<'TYPES'
declare module 'graphql' {
  export const GraphQLSchema: any;
  export type GraphQLSchema = any;
  export const GraphQLObjectType: any;
  export type GraphQLObjectType = any;
  export const GraphQLInputObjectType: any;
  export type GraphQLInputObjectType = any;
  export const GraphQLInterfaceType: any;
  export type GraphQLInterfaceType = any;
  export const GraphQLUnionType: any;
  export type GraphQLUnionType = any;
  export const GraphQLEnumType: any;
  export type GraphQLEnumType = any;
  export const GraphQLScalarType: any;
  export type GraphQLScalarType = any;
  export const GraphQLField: any;
  export type GraphQLField<T = any, U = any, V = any> = any;
  export const GraphQLFieldConfig: any;
  export type GraphQLFieldConfig<T = any, U = any, V = any> = any;
  export const GraphQLFieldConfigMap: any;
  export type GraphQLFieldConfigMap<T = any, U = any> = any;
  export const GraphQLInputFieldConfig: any;
  export type GraphQLInputFieldConfig = any;
  export const GraphQLInputFieldConfigMap: any;
  export type GraphQLInputFieldConfigMap = any;
  export const GraphQLArgument: any;
  export type GraphQLArgument = any;
  export const GraphQLArgumentConfig: any;
  export type GraphQLArgumentConfig = any;
  export const GraphQLEnumValueConfigMap: any;
  export type GraphQLEnumValueConfigMap = any;
  export const GraphQLIsTypeOfFn: any;
  export type GraphQLIsTypeOfFn<T = any, U = any> = any;
  export const GraphQLResolveInfo: any;
  export type GraphQLResolveInfo = any;
  export const GraphQLOutputType: any;
  export type GraphQLOutputType = any;
  export const GraphQLInputType: any;
  export type GraphQLInputType = any;
  export const GraphQLNamedType: any;
  export type GraphQLNamedType = any;
  export const GraphQLNonNull: any;
  export type GraphQLNonNull<T = any> = any;
  export const GraphQLList: any;
  export type GraphQLList<T = any> = any;
  export const GraphQLNullableType: any;
  export type GraphQLNullableType = any;
  export const GraphQLType: any;
  export type GraphQLType = any;
  export const GraphQLTypeResolver: any;
  export type GraphQLTypeResolver<T = any, U = any> = any;
  export const GraphQLFieldResolver: any;
  export type GraphQLFieldResolver<T = any, U = any, V = any, W = any> = any;
  export const GraphQLString: any;
  export const GraphQLInt: any;
  export const GraphQLFloat: any;
  export const GraphQLBoolean: any;
  export const GraphQLID: any;
  export const execute: any;
  export const parse: any;
  export const buildSchema: any;
  export const printSchema: any;
  export const getIntrospectionQuery: any;
  export const introspectionFromSchema: any;
  export const IntrospectionQuery: any;
  export type IntrospectionQuery = any;
  export const Source: any;
  export type Source = any;
  export const DocumentNode: any;
  export type DocumentNode = any;
  export const GraphQLFormattedError: any;
  export type GraphQLFormattedError = any;
  export const GraphQLError: any;
  export type GraphQLError = any;
}

declare module 'graphql/type' {
  export const GraphQLObjectType: any;
  export type GraphQLObjectType = any;
  export const GraphQLInputObjectType: any;
  export type GraphQLInputObjectType = any;
  export const GraphQLInterfaceType: any;
  export type GraphQLInterfaceType = any;
  export const GraphQLUnionType: any;
  export type GraphQLUnionType = any;
}

declare module 'graphql/language' {
  export const DirectiveLocation: any;
  export type DirectiveLocation = any;
}

declare module 'reflect-metadata' {}

declare module 'semver' {
  export const gte: any;
  export type gte = any;
}

declare module 'glob' {
  export const sync: any;
  export type sync = any;
}

declare module 'class-validator' {
  export const ValidatorOptions: any;
  export type ValidatorOptions = any;
  export const validate: any;
  export type validate = any;
}

declare namespace NodeJS {
  type ErrnoException = any;
  type Timeout = any;
  interface ProcessEnv { [k: string]: string | undefined }
}
declare var global: any;
TYPES
}

tsz_write_ofetch_external_stubs() {
  # ofetch's real tsconfig sets `"types": ["node"]`, so its upstream build sees
  # the node typings and the `undici` peer dependency. The shared bench baseline
  # pins `"types": []` and the fixture clone runs no npm install, so without a
  # stub tsc emits five spurious diagnostics, all from absent ambient typings
  # rather than ofetch's own source:
  #   * src/types.ts imports `undici` for `InstanceType<typeof
  #     import("undici").Dispatcher>` -> TS2307 "Cannot find module 'undici'".
  #   * src/fetch.ts imports `node:stream` for the `Readable` body cast ->
  #     TS2591 "Cannot find name 'node:stream'".
  #   * src/fetch.ts annotates `let abortTimeout: NodeJS.Timeout` -> TS2503
  #     "Cannot find namespace 'NodeJS'".
  #   * src/fetch.ts guards `Error.captureStackTrace` (a V8/@types/node global
  #     augmentation) -> TS2339 "Property 'captureStackTrace' does not exist on
  #     type 'ErrorConstructor'".
  # Mirror the trpc/type-graphql stub convention: provide named ambient
  # any-modules for the external deps plus the node global augmentations the
  # @types/node-backed build would supply. `Dispatcher` is a class so the
  # `typeof ... ` + `InstanceType<...>` resolves; `Readable` carries the `.pipe`
  # member the source casts to. ofetch's own `./*.ts` source stays real-checked.
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
declare module 'undici' {
  // ofetch reads `InstanceType<typeof import("undici").Dispatcher>`, so the
  // export must be a constructable value (a class) rather than bare `any`.
  export class Dispatcher {
    [key: string]: any;
  }
}

declare module 'node:stream' {
  // ofetch casts a request body to `Readable` and reads `.pipe`; an interface
  // with the referenced member resolves the cast without unmasking unrelated
  // assignability diffs.
  export interface Readable {
    pipe(...args: any[]): any;
    [key: string]: any;
  }
}

declare namespace NodeJS {
  type Timeout = any;
}

interface ErrorConstructor {
  captureStackTrace?(targetObject: object, constructorOpt?: Function): void;
}
TYPES
}
