
import type {
  Brand,
  DeepReadonly,
  KeyPaths,
  PathValue,
} from './helpers';

export type ManeuverVoyageId = Brand<string, 'ManeuverVoyageId'>;
export type ManeuverPlanId = Brand<string, 'ManeuverPlanId'>;
export type ManeuverRunId = Brand<string, 'ManeuverRunId'>;
export type ManeuverNodeId = Brand<string, 'ManeuverNodeId'>;
export type ManeuverBeaconId = Brand<string, 'ManeuverBeaconId'>;
export type ManeuverGadgetId = Brand<string, 'ManeuverGadgetId'>;

export type ManeuverPhase =
  | 'discover'
  | 'shape'
  | 'simulate'
  | 'validate'
  | 'recommend'
  | 'execute'
  | 'verify'
  | 'close';

export type ManeuverTopology = 'grid' | 'mesh' | 'chain' | 'ring';
export type ManeuverHealth = 'ok' | 'degraded' | 'failed' | 'recovering';
export type ManeuverBeaconTier = 'beacon' | 'warning' | 'critical' | 'postmortem';
export type GadgetKind<TName extends string = string> = `recovery/ops/sim/${TName}`;
export type TemplateTag<TKind extends string> = `${TKind}:template`;
export type StageLabel<TPhase extends ManeuverPhase = ManeuverPhase> = `${TPhase}::${string}`;
export type TaggedPlan<TTopology extends ManeuverTopology = ManeuverTopology> = {
  readonly kind: `topology:${TTopology}`;
  readonly tags: readonly string[];
};
export type TopologyByBeaconCount = {
  readonly count: number;
  readonly topology: ManeuverTopology;
};

export interface ManeuverTag {
  readonly key: string;
  readonly value: string;
}

export interface ManeuverInput {
  readonly tenantId: string;
  readonly siteId: string;
  readonly zone: string;
  readonly severityBudget: number;
  readonly requestedBy: string;
}

export interface ManeuverWindow {
  readonly id: ManeuverVoyageId;
  readonly from: string;
  readonly to: string;
  readonly timezone: string;
  readonly blackoutMinutes: readonly number[];
}

export interface ManeuverBeacon {
  readonly id: ManeuverBeaconId;
  readonly namespace: string;
  readonly tier: ManeuverBeaconTier;
  readonly title: string;
  readonly score: number;
  readonly confidence: number;
  readonly tags: readonly ManeuverTag[];
}

export interface ManeuverStep {
  readonly id: ManeuverPlanId;
  readonly name: string;
  readonly kind: GadgetKind<string>;
  readonly command: string;
  readonly durationMinutes: number;
  readonly dependsOn: readonly ManeuverPlanId[];
  readonly weight: number;
  readonly reversible: boolean;
  readonly tags: readonly string[];
}

export interface ManeuverPlan {
  readonly id: ManeuverPlanId;
  readonly title: string;
  readonly voyageId: ManeuverVoyageId;
  readonly confidence: number;
  readonly state: 'draft' | 'active' | 'candidate' | 'blocked';
  readonly steps: readonly ManeuverStep[];
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface ManeuverEnvelopeInput {
  readonly voyageId: ManeuverVoyageId;
  readonly plan: ManeuverPlan;
  readonly beacons: readonly ManeuverBeacon[];
  readonly windows: readonly ManeuverWindow[];
  readonly topology: ManeuverTopology;
  readonly metadata: DeepReadonly<Record<string, unknown>>;
}

export interface ManeuverSummary {
  readonly voyageId: ManeuverVoyageId;
  readonly beaconCount: number;
  readonly criticalCount: number;
  readonly riskIndex: number;
  readonly health: ManeuverHealth;
}

export interface ManeuverEnvelope<TContext extends object = object> {
  readonly id: ManeuverVoyageId;
  readonly runId: ManeuverRunId;
  readonly phase: ManeuverPhase;
  readonly createdAt: string;
  readonly envelope: ManeuverEnvelopeInput;
  readonly context: DeepReadonly<TContext>;
  readonly summary: ManeuverSummary;
}

export interface ManeuverGadgetOutput<TPayload = unknown> {
  readonly gadgetId: ManeuverGadgetId;
  readonly phase: ManeuverPhase;
  readonly timestamp: string;
  readonly elapsedMs: number;
  readonly payload: DeepReadonly<TPayload>;
}

export interface ManeuverCandidate<T = object> {
  readonly id: ManeuverPlanId;
  readonly score: number;
  readonly topology: ManeuverTopology;
  readonly rationale: string;
  readonly metadata: DeepReadonly<T>;
}

export interface ManeuverResult<TPayload = object, TContext extends object = object> {
  readonly voyageId: ManeuverVoyageId;
  readonly runId: ManeuverRunId;
  readonly output: DeepReadonly<TPayload>;
  readonly context: DeepReadonly<TContext>;
  readonly candidates: readonly ManeuverCandidate[];
  readonly selectedPlanId?: ManeuverPlanId;
  readonly diagnostics: readonly string[];
  readonly summary: ManeuverSummary;
}

export type ManeuverOutput<TPayload = object, TContext extends object = object> = ManeuverResult<TPayload, TContext>;

export interface VoyageExecutionContext {
  readonly namespace: string;
  readonly runId: ManeuverRunId;
}

export type ManeuverBeaconPath<TBeacon extends ManeuverBeacon> = KeyPaths<TBeacon>;
export type ManeuverTagValue<TPayload> = {
  [Key in keyof TPayload as Key extends `_${string}` ? never : Key]: TPayload[Key];
};

export type ManeuverRunMetadata<TVersion extends string = string> = {
  readonly schemaVersion: TVersion;
  readonly namespace: string;
  readonly correlationId: Brand<string, 'CorrelationId'>;
  readonly buildTag: TemplateTag<`v${TVersion}`>;
};

export type ManeuverConfig<TInput extends object = object, TOutput = object> = {
  readonly voyageId: ManeuverVoyageId;
  readonly input: ManeuverInput;
  readonly topology: ManeuverTopology;
  readonly phaseSequence: readonly ManeuverPhase[];
  readonly gadgets: readonly { readonly kind: GadgetKind<string>; readonly version: string }[];
  readonly expectedOutput: TOutput;
  readonly inputSnapshot: TInput;
};

export const maneuverPhaseSchema = [
  'discover',
  'shape',
  'simulate',
  'validate',
  'recommend',
  'execute',
  'verify',
  'close',
] as const;

export const maneuverBeaconTierSchema = ['beacon', 'warning', 'critical', 'postmortem'] as const;

export const asVoyageId = (value: string): ManeuverVoyageId => value as ManeuverVoyageId;
export const asPlanId = (value: string): ManeuverPlanId => value as ManeuverPlanId;
export const asRunId = (value: string): ManeuverRunId => value as ManeuverRunId;
export const asGadgetId = (value: string): ManeuverGadgetId => value as ManeuverGadgetId;
export const asNodeId = (value: string): ManeuverNodeId => value as ManeuverNodeId;

const ensureNumber = (value: unknown): number =>
  typeof value === 'number' && Number.isFinite(value) ? value : 0;

export const normalizeTopology = (value: string): ManeuverTopology =>
  value === 'mesh' || value === 'chain' || value === 'ring' || value === 'grid' ? value : 'grid';

export const riskFromBeacons = (beacons: readonly ManeuverBeacon[]): number => {
  const critical = beacons.filter((beacon) => beacon.tier === 'critical').length;
  const warning = beacons.filter((beacon) => beacon.tier === 'warning').length;
  const base = Math.max(beacons.length, 1);
  return (critical * 1.8 + warning * 0.9 + beacons.length * 0.15) / base;
};

export const beaconScore = (beacon: ManeuverBeacon): number =>
  ensureNumber(beacon.score) + ensureNumber(beacon.confidence) * 60;

export const resolveHealth = (risk: number): ManeuverHealth =>
  risk > 1.4 ? 'failed' : risk > 1.1 ? 'degraded' : risk > 0.9 ? 'recovering' : 'ok';

export const buildSummary = (envelope: Pick<ManeuverEnvelopeInput, 'voyageId' | 'beacons'>): ManeuverSummary => {
  const risk = riskFromBeacons(envelope.beacons);
  return {
    voyageId: envelope.voyageId,
    beaconCount: envelope.beacons.length,
    criticalCount: envelope.beacons.filter((beacon) => beacon.tier === 'critical').length,
    riskIndex: risk,
    health: resolveHealth(risk),
  };
};

export const buildManeuverEnvelope = <TContext extends object>(
  input: ManeuverEnvelopeInput,
  context: TContext,
  phase: ManeuverPhase = 'discover',
): ManeuverEnvelope<TContext> => ({
  id: input.voyageId,
  runId: asRunId(`${input.voyageId}:${Date.now()}`),
  phase,
  createdAt: new Date().toISOString(),
  envelope: input,
  context: context as DeepReadonly<TContext>,
  summary: buildSummary(input),
});

export const buildBeaconFingerprint = (beacons: readonly ManeuverBeacon[]): string =>
  beacons
    .toSorted((left, right) => left.tier.localeCompare(right.tier))
    .map((beacon) => `${beacon.id}::${beacon.tier}::${beacon.score.toFixed(2)}`)
    .join('|');

export type RebasedTuple<T extends readonly unknown[]> =
  T extends readonly [infer Head, ...infer Tail]
    ? [Head, ...RebasedTuple<Tail>]
    : [];

export type ExtractedPath<T, TPath extends string> = TPath extends keyof T & string
  ? T[TPath]
  : PathValue<T, TPath>;

export const buildPlanFingerprint = <TOutput extends object>(
  envelope: ManeuverEnvelope<TOutput>,
  candidates: number,
): string => `${buildBeaconFingerprint(envelope.envelope.beacons)}::${candidates}:${envelope.summary.health}`;

export const normalizeTopologyMap = (topologies: readonly ManeuverTopology[]): TopologyByBeaconCount => ({
  count: topologies.length,
  topology: normalizeTopology(topologies[topologies.length - 1] ?? 'grid'),
});

export const parseTopology = <TInput extends string>(value: TInput): TopologyByBeaconCount & TaggedPlan => {
  const normalized = normalizeTopology(value) as ManeuverTopology;
  return {
    count: value.length,
    topology: normalized,
    kind: `topology:${normalized}` as const,
    tags: [value.length > 0 ? `raw:${value}` : 'raw:empty', `normalized:${normalized}`],
  } satisfies TopologyByBeaconCount & TaggedPlan;
};

export const describeBeacon = (beacon: ManeuverBeacon): `${ManeuverBeaconTier}:${string}` => `${beacon.tier}:${beacon.id}`;

