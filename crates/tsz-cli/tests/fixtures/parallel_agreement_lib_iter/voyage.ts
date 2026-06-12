const randomUUID = (): string => 'u';
import {
  asPlanId,
  asVoyageId,
  asRunId,
  buildManeuverEnvelope,
  buildSummary,
  normalizeTopology,
  type ManeuverConfig,
  type ManeuverEnvelope,
  type ManeuverPlan,
  type ManeuverPlanId,
  type ManeuverResult,
  type ManeuverRunId,
  type ManeuverVoyageId,
  type ManeuverBeacon,
  type ManeuverTopology,
  type ManeuverWindow,
} from './types';
import {
  appendCandidate,
  createManeuverEnvelopeResult,
  ConveyorPhase,
  ConveyorStage,
  runAdaptiveConveyor,
  type AdaptiveConveyorResult,
} from './conveyor';
import { AdaptiveGadgetRegistry, buildGadgetsFromConfig } from './gadgets';
import { buildGraphDiagnostics, buildManeuverGraph, summarizeGraph } from './mesh';

export type VoyageState = 'boot' | 'ready' | 'running' | 'complete' | 'failed';

export interface VoyageRequest {
  readonly tenantId: string;
  readonly siteId: string;
  readonly topology: ManeuverTopology;
  readonly beacons: readonly ManeuverBeacon[];
  readonly plans: readonly ManeuverPlan[];
  readonly context: Record<string, unknown>;
}

export interface ManeuverVoyageState {
  readonly voyageId: ManeuverVoyageId;
  readonly state: VoyageState;
  readonly phase: string;
  readonly createdAt: string;
  readonly startedAt?: string;
  readonly finishedAt?: string;
}

export interface VoyageResult<TOutput = object, TContext extends object = object> {
  readonly voyageId: ManeuverVoyageId;
  readonly runId: ManeuverRunId;
  readonly output: ManeuverResult<TOutput, TContext>;
  readonly diagnostics: readonly string[];
  readonly timeline: readonly string[];
  readonly conveyor: AdaptiveConveyorResult<{
    stage: ConveyorPhase;
    count: number;
    beaconCount: number;
    beaconDigest: string;
  }>;
  readonly graph: ReturnType<typeof buildManeuverGraph>;
}

const nowIso = (): string => new Date().toISOString();

const fallbackPlan = (voyageId: ManeuverVoyageId): ManeuverPlan => ({
  id: asPlanId(`${voyageId}:fallback`),
  title: 'fallback-plan',
  voyageId,
  confidence: 0.4,
  state: 'candidate',
  steps: [],
  createdAt: nowIso(),
  updatedAt: nowIso(),
});

const buildEnvelopeInput = (request: VoyageRequest) => {
  const voyageId = asVoyageId(`${request.tenantId}:${request.siteId}:${Date.now()}`);
  const plan = request.plans[0] ?? fallbackPlan(voyageId);
  const windows: ManeuverWindow[] = request.beacons.map((beacon, index) => ({
    id: asVoyageId(`${voyageId}:window:${index}`),
    from: nowIso(),
    to: new Date(Date.now() + (index + 1) * 45_000).toISOString(),
    timezone: beacon.namespace,
    blackoutMinutes: [index],
  }));
  return {
    voyageId,
    plan,
    beacons: request.beacons,
    windows,
    topology: normalizeTopology(request.topology),
    metadata: {
      tenantId: request.tenantId,
      siteId: request.siteId,
      planCount: request.plans.length,
    },
  };
};

const resolveTimeline = (phase: string): VoyageState =>
  phase === 'complete' ? 'complete' : phase === 'ready' ? 'ready' : phase === 'failed' ? 'failed' : 'running';

const conveyorStages = [
  {
    id: 'conveyor:discover',
    inputShape: 'discover',
    outputShape: 'discover',
    run: async (
      input: { readonly beaconCount: number; readonly beaconDigest: string },
      _traceId: string,
    ) => ({
      stage: 'conveyor:discover' as const,
      count: input.beaconCount + 1,
      beaconCount: input.beaconCount,
      beaconDigest: `${input.beaconDigest}:discover`,
    }),
  },
  {
    id: 'conveyor:simulate',
    inputShape: 'simulate',
    outputShape: 'simulate',
    run: async (
      input: { readonly beaconCount: number; readonly beaconDigest: string },
      _traceId: string,
    ) => ({
      stage: 'conveyor:simulate' as const,
      count: input.beaconCount + 2,
      beaconCount: input.beaconCount,
      beaconDigest: `${input.beaconDigest}:simulate`,
    }),
  },
  {
    id: 'conveyor:validate',
    inputShape: 'validate',
    outputShape: 'validate',
    run: async (
      input: { readonly beaconCount: number; readonly beaconDigest: string },
      _traceId: string,
    ) => ({
      stage: 'conveyor:validate' as const,
      count: input.beaconCount + 3,
      beaconCount: input.beaconCount,
      beaconDigest: `${input.beaconDigest}:validate`,
    }),
  },
] as const satisfies readonly ConveyorStage<
  { readonly beaconCount: number; readonly beaconDigest: string },
  { readonly stage: ConveyorPhase; readonly count: number; readonly beaconCount: number; readonly beaconDigest: string }
>[];

export const describeAdaptiveOutput = (result: VoyageResult<object, object>): string => {
  return `${result.voyageId}:${result.output.summary.health}:${result.output.summary.beaconCount}:${result.conveyor.output.count}`;
};

export const buildAdaptiveConfig = (input: {
  tenantId: string;
  siteId: string;
  zone: string;
  severityBudget: number;
  requestedBy: string;
},
): ManeuverConfig => ({
  voyageId: asVoyageId(`${input.tenantId}:${input.siteId}`),
  input: {
    tenantId: input.tenantId,
    siteId: input.siteId,
    zone: input.zone,
    severityBudget: input.severityBudget,
    requestedBy: input.requestedBy,
  },
  topology: normalizeTopology(input.siteId),
  phaseSequence: ['discover', 'shape', 'simulate', 'validate', 'recommend', 'execute', 'verify', 'close'],
  gadgets: [{ kind: 'recovery/ops/sim/normalize', version: '1.0.0' }],
  expectedOutput: {},
  inputSnapshot: {
    tenantId: input.tenantId,
    siteId: input.siteId,
    zone: input.zone,
    severityBudget: input.severityBudget,
    requestedBy: input.requestedBy,
  },
});

export const runAdaptiveVoyage = async <
  TContext extends object,
>(
  request: VoyageRequest,
  context: TContext,
): Promise<VoyageResult<object, TContext>> => {
  const envelopeInput = buildEnvelopeInput(request);
  const envelope = buildManeuverEnvelope(
    {
      ...envelopeInput,
      metadata: envelopeInput.metadata,
    },
    context,
    'discover',
  );

  const graph = buildManeuverGraph(envelope.envelope, envelopeInput.topology);
  const graphSummary = summarizeGraph(graph, envelope.summary);

  const diagnostics: string[] = [
    `topology=${envelopeInput.topology}`,
    `plan=${envelopeInput.plan.id}`,
    `route=${graphSummary.routeDigest}`,
    `beacons=${envelope.summary.beaconCount}`,
  ];

  const registry = new AdaptiveGadgetRegistry(
    buildGadgetsFromConfig({
      tenant: request.tenantId,
      labels: envelopeInput.beacons.map((beacon) => beacon.id),
      profile: envelopeInput.topology,
    }),
  );

  await using _scope = registry;

  const gadgetContext = {
    namespace: `${request.tenantId}:${request.siteId}`,
  } as const;
  const gadgetOutput = [
    ...(await registry.runPhase('discover', gadgetContext, {
      topology: envelopeInput.topology,
      beaconCount: request.beacons.length,
    })),
    ...(await registry.runPhase('validate', gadgetContext, {
      topology: envelopeInput.topology,
      beaconCount: request.beacons.length,
    })),
  ];

  diagnostics.push(`gadgets=${gadgetOutput.length}`);
  const candidates = gadgetOutput.map((entry, index) => ({
    id: envelopeInput.plan.id as ManeuverPlanId,
    score: Number((0.55 + index * 0.1).toFixed(2)),
    topology: envelopeInput.topology,
    rationale: `${entry.phase}:${entry.gadgetId}`,
    metadata: {
      gadget: entry.gadgetId,
      elapsed: entry.elapsedMs,
      gadgetPhase: entry.phase,
    },
  }));

  const conveyor = await runAdaptiveConveyor(
    conveyorStages,
    {
      beaconCount: request.beacons.length,
      beaconDigest: `beacons:${envelopeInput.beacons.length}`,
    } as const,
  );

  const base = createManeuverEnvelopeResult(
    envelope,
    {
      stage: conveyor.output.stage,
      count: conveyor.output.count,
      beaconCount: conveyor.output.beaconCount,
      beaconDigest: conveyor.output.beaconDigest,
    },
    buildSummary(envelope.envelope),
  );

  const selectedPlanId = envelopeInput.plan.id;
  const output = appendCandidate(base, selectedPlanId, candidates.length, `plan:${envelopeInput.plan.title}`);
  const graphDiag = buildGraphDiagnostics(graph);

  return {
    voyageId: envelope.id,
    runId: asRunId(`run:${randomUUID()}`),
    output: {
      ...output,
      candidates,
      selectedPlanId,
      context: envelope.context,
      summary: envelope.summary,
      output: output.output,
    },
    diagnostics: [...diagnostics, `graph=${graphDiag.fingerprint}`, `conveyor=${conveyor.timeline.length}`],
    timeline: [
      `state=${resolveTimeline('running')}`,
      `plan=${selectedPlanId}`,
      ...graphSummary.nodes,
    ],
    conveyor: {
      ...conveyor,
      output: {
        stage: conveyor.output.stage,
        count: conveyor.output.count,
        beaconCount: conveyor.output.beaconCount,
        beaconDigest: conveyor.output.beaconDigest,
      },
    },
    graph,
  };
};

export const inspectVoyageState = (voyageId: ManeuverVoyageId, phase: string): ManeuverVoyageState => ({
  voyageId,
  state: resolveTimeline(phase),
  phase,
  createdAt: nowIso(),
});
