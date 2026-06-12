import {
  buildSummary,
  type ManeuverEnvelope,
  type ManeuverResult,
  type ManeuverSummary,
} from './types';
import type { DeepReadonly } from './helpers';

export type ConveyorPhase =
  `conveyor:${'discover' | 'shape' | 'simulate' | 'validate' | 'recommend' | 'execute' | 'verify' | 'close'}`;

export type StageDiagnostics = {
  readonly stageId: string;
  readonly durationMs: number;
  readonly event: string;
};

export interface ConveyorStage<TInput = object, TOutput = object> {
  readonly id: ConveyorPhase;
  readonly inputShape: string;
  readonly outputShape: string;
  run(input: TInput, traceId: string): Promise<TOutput>;
}

export type ConveyorInput<TStages extends readonly ConveyorStage[]> = TStages extends readonly [
  infer Head extends ConveyorStage<infer TInput, any>,
  ...ConveyorStage[]
]
  ? TInput
  : object;

export type ConveyorOutput<TStages extends readonly ConveyorStage[]> = TStages extends readonly [
  ...ConveyorStage[],
  ConveyorStage<any, infer TOutput>
]
  ? TOutput
  : object;

export interface AdaptiveConveyorResult<TPayload = object> {
  readonly runId: string;
  readonly voyageId: string;
  readonly output: TPayload;
  readonly diagnostics: readonly StageDiagnostics[];
  readonly timeline: readonly ConveyorPhase[];
}

export const buildConveyorRunId = (phase: ConveyorPhase, seed: string): string => `${phase}-${seed}-${Date.now()}`;

export const runAdaptiveConveyor = async <
  const TStages extends readonly ConveyorStage[],
  const TInput extends ConveyorInput<TStages>,
>(
  stages: TStages,
  input: TInput,
): Promise<AdaptiveConveyorResult<ConveyorOutput<TStages>>> => {
  const runId = buildConveyorRunId(stages[0]?.id ?? 'conveyor:discover', `${Date.now()}`);
  const timeline: ConveyorPhase[] = [stages[0]?.id ?? 'conveyor:discover'];
  const diagnostics: StageDiagnostics[] = [];
  let current: object = input as object;

  for (const stage of stages) {
    const started = performance.now();
    const result = await (stage.run as (input: object, traceId: string) => Promise<object>)(current, runId);
    current = result;
    diagnostics.push({
      stageId: stage.id,
      durationMs: Math.max(0, performance.now() - started),
      event: `${stage.inputShape}->${stage.outputShape}`,
    });
    timeline.push(stage.id);
  }

  return {
    runId,
    voyageId: `voyage-${runId}`,
    output: current as ConveyorOutput<TStages>,
    diagnostics,
    timeline,
  };
};

export const buildConveyorDigest = <TStages extends readonly ConveyorStage[]>(stages: TStages): string => {
  return stages.map((stage) => stage.id).join('|');
};

export const conveyorDiagnosticsFromOutput = <TStages extends readonly ConveyorStage[]>(
  run: AdaptiveConveyorResult<unknown>,
  stages: TStages,
): readonly string[] => [
  run.runId,
  run.voyageId,
  `stages=${stages.length}`,
  `diagnostics=${run.diagnostics.length}`,
  `timeline=${run.timeline.join('>')}`,
];

export const collectPlanSummaries = (runs: readonly AdaptiveConveyorResult[]): readonly string[] =>
  runs
    .toSorted((left, right) => right.diagnostics.length - left.diagnostics.length)
    .map((run) => `${run.runId}:${run.timeline.length}`);

export const createManeuverEnvelopeResult = <
  TPayload extends object,
  TContext extends object,
>(
  envelope: ManeuverEnvelope<TContext>,
  output: TPayload,
  summary: ManeuverSummary = buildSummary(envelope.envelope),
): ManeuverResult<TPayload, TContext> => {
  return {
    voyageId: envelope.id,
    runId: envelope.runId,
    output: output as DeepReadonly<TPayload>,
    context: envelope.context as DeepReadonly<TContext>,
    candidates: [],
    selectedPlanId: undefined,
    diagnostics: [`voyage=${envelope.id}`, `phase=${envelope.phase}`, `summary=${envelope.summary.beaconCount}`],
    summary,
  };
};

export const appendCandidate = <
  TOutput,
  TPlanId extends string,
>(
  result: ManeuverResult<TOutput>,
  planId: TPlanId,
  score: number,
  rationale: string,
): ManeuverResult<TOutput> => {
  return {
    ...result,
    candidates: [
      {
        id: planId as never,
        score,
        topology: 'grid',
        rationale,
        metadata: { source: 'adaptive-voyage', score },
      },
      ...result.candidates,
    ],
    selectedPlanId: result.selectedPlanId ?? (planId as never),
  };
};

export type RecursiveTuple<
  T extends readonly ConveyorStage[],
  Prefix extends string = 'stage',
> = T extends readonly [
  infer Head extends ConveyorStage,
  ...infer Tail extends readonly ConveyorStage[],
]
  ? readonly [
      stageId: Head['id'],
      phase: `${Prefix}:${Head['id']}`,
      output: Awaited<ReturnType<Head['run']>>,
      tail: RecursiveTuple<Tail, Prefix>,
    ]
  : readonly [
      stageId: `${Prefix}:end`,
      phase: `${Prefix}:end`,
      output: never,
      tail: never,
    ];

export const mapGadgetResultToTuple = <TStages extends readonly ConveyorStage[]>(
  result: readonly unknown[],
): RecursiveTuple<TStages> => result as RecursiveTuple<TStages>;

export const runAdaptiveTimeline = async <TStages extends readonly ConveyorStage[]>(stages: TStages): Promise<TStages> => {
  return stages;
};
