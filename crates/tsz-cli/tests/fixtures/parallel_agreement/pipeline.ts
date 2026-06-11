import { Sigil, withSigil } from './primitives.js';
import { evaluateCustody, createCustodyEnvelope } from './custody';
import {
  ConstraintEnvelope,
  StewardContext,
  StewardEvaluation,
  StewardMatrix,
  StewardSignal,
  LedgerEnvelope,
  LedgerProfile,
  LedgerEdict,
  LedgerWindow,
  createStewardRunId,
  SeverityBand,
} from './shapes';
import { evaluateConstraintEnvelope, summarizeConstraintState } from './limits';
import { evaluateLedgerProfile, evaluateLedgerEdict, findBlockingEdicts, rankProfiles } from './edicts';
import { InMemoryStewardStore, type StewardStore } from './bridge';

export interface StewardEngineInput {
  readonly context: StewardContext;
  readonly profile: LedgerProfile;
  readonly signals: readonly StewardSignal[];
  readonly profileList: readonly LedgerProfile[];
  readonly windows: readonly LedgerWindow[];
  readonly edicts: readonly LedgerEdict[];
}

export type ReadinessSnapshot = {
  readonly regionId: StewardContext['regionId'];
  readonly readiness: number;
  readonly ledgerCoverage: number;
  readonly warning: string[];
};

type RankedLedger = {
  readonly ledgerId: LedgerProfile['ledgerId'];
  readonly score: number;
};

const computeReadiness = (coverage: number, warningCount: number): number => {
  const base = Math.max(0, Math.min(100, coverage * 100));
  return Math.max(0, base - warningCount * 5);
};

export const buildConstraintEnvelope = (ctx: StewardContext, profile: LedgerProfile, index: number): ConstraintEnvelope => {
  return {
    id: `${ctx.regionId}:constraint-${index}` as Sigil<string, 'ConstraintEnvelopeId'>,
    regionId: ctx.regionId,
    title: `Constraint for ${profile.name}`,
    required: [String(profile.regionId) as Sigil<string, 'ResourceId'>],
    forbidden: [],
    rationale: `${ctx.domain} controls for ${profile.domain}`,
  };
};

export const buildEnvelope = (ctx: StewardContext, profiles: readonly LedgerProfile[]): LedgerEnvelope => {
  const activeProfiles = profiles.filter((profile) => profile.state === 'active');

  return {
    id: `${ctx.regionId}:envelope` as LedgerEnvelope['id'],
    regionId: ctx.regionId,
    title: `Ledger envelope for ${ctx.domain}`,
    policies: [...activeProfiles],
    windows: [...(ctx.state === 'active' ? [] : [])],
    edicts: activeProfiles.flatMap((profile) => profile.edicts),
    constraints: activeProfiles.map((profile, index) => buildConstraintEnvelope(ctx, profile, index)),
    custodyClauses: activeProfiles.map((profile, index) =>
      ({
        ...createCustodyEnvelope(ctx.regionId),
        title: `Clause for ${profile.name} #${index}`,
        description: `${profile.name} custody tracking`,
      }),
    ),
    createdAt: new Date().toISOString(),
  };
};

export const evaluateStewardMatrix = (ctx: StewardContext, profiles: readonly LedgerProfile[]): StewardMatrix => {
  const activeProfiles = profiles.filter((profile) => profile.state === 'active');
  const envelope = buildEnvelope(ctx, profiles);

  return {
    regionId: ctx.regionId,
    asOf: new Date().toISOString(),
    profileCount: activeProfiles.length,
    activeProfiles,
    envelopes: [envelope],
    custodyScore: activeProfiles.length === 0 ? 0 : Math.min(100, activeProfiles.length * 20),
  };
};

export const evaluateSteward = (
  input: StewardEngineInput,
  store: StewardStore = new InMemoryStewardStore(),
): ReadinessSnapshot => {
  const matrix = evaluateStewardMatrix(input.context, input.profileList);
  void store.loadMatrix(input.context);

  const contextSignalsByMetric: Record<string, readonly number[]> = {};
  for (const signal of input.signals) {
    contextSignalsByMetric[signal.metric] = [
      ...(contextSignalsByMetric[signal.metric] ?? []),
      signal.value,
    ];
  }

  const constraintStats = summarizeConstraintState(
    matrix.envelopes.flatMap((envelope) =>
      envelope.constraints.map((constraint) => evaluateConstraintEnvelope(constraint, input.signals)),
    ),
  );

  const profileContext = {
    band: 'critical' as SeverityBand,
    activeSignals: input.signals.length,
    criticalSignals: input.signals.filter((signal) => signal.severity === 'critical').length,
    coverage: input.signals.length > 0 ? Math.min(1, input.signals.length / 10) : 0,
  };

  const rankedProfiles = rankProfiles(input.profileList, profileContext);
  const ledgerCoverage = rankedProfiles.length > 0 ? rankedProfiles[0]?.score ?? 0 : 0;

  const profileViolations = input.profileList.flatMap((profile) => {
    const summary = evaluateLedgerProfile(profile, profileContext);
    if (summary.passingEdicts === summary.totalEdicts) {
      return [];
    }

    return profile.edicts.map((edict) => evaluateLedgerEdict(edict, profileContext));
  });

  const blockingEdicts = findBlockingEdicts(profileViolations);
  const custody = evaluateCustody(
    input.context.regionId,
    input.profile,
    matrix.envelopes.flatMap((envelope) => envelope.custodyClauses),
    contextSignalsByMetric,
  );

  const readiness = computeReadiness(ledgerCoverage, blockingEdicts.length + constraintStats.breached);

  const evaluation: StewardEvaluation = {
    regionId: input.context.regionId,
    runId: createStewardRunId(`${input.context.regionId}:run`),
    ledgerCoverage,
    warningCount: blockingEdicts.length,
    criticalCount: constraintStats.breached,
    readinessScore: readiness,
    ledgerSignals: profileViolations.map((entry) => ({
      edictId: entry.edict.id,
      fired: !entry.passed,
      weight: Math.abs(entry.score),
    })),
    windowCustody: false,
  };

  void store.saveCustody(input.context, custody);
  void store.saveEnvelope(input.context, matrix.envelopes[0]);

  const warnings = [
    `custody:${custodyHealth(custody)}`,
    `matrices:${matrix.envelopes.length}`,
    `signals:${input.signals.length}`,
    `edicts:${profileViolations.length}`,
    `blocking-edicts:${blockingEdicts.length}`,
  ];

  return {
    regionId: input.context.regionId,
    readiness,
    ledgerCoverage,
    warning: [...warnings, ...evaluation.ledgerSignals.map((signal) => `${String(signal.edictId)}:${signal.fired ? 'blocked' : 'ok'}`)],
  };
};

export const buildReadinessEnvelope = (ctx: StewardContext): LedgerEnvelope => {
  const profile: LedgerProfile = {
    ledgerId: withSigil(`${ctx.regionId}-base`, 'LedgerId'),
    regionId: ctx.regionId,
    name: 'Base ledger profile',
    domain: ctx.domain,
    state: ctx.state,
    maxConcurrent: 3,
    maxCriticality: 4,
    windowsByBand: {
      low: [],
      medium: [],
      high: [],
      critical: [],
    },
    edicts: [],
  };

  return {
    id: `${ctx.regionId}:readiness` as LedgerEnvelope['id'],
    regionId: ctx.regionId,
    title: 'readiness envelope',
    policies: [profile],
    windows: [],
    edicts: [],
    constraints: [],
    custodyClauses: [],
    createdAt: new Date().toISOString(),
  };
};

export const topRankedPolicies = (profiles: readonly LedgerProfile[]): RankedLedger[] => {
  return profiles
    .map((profile, index) => ({
      ledgerId: profile.ledgerId,
      score: 100 - index,
    }))
    .sort((left, right) => right.score - left.score);
};

const custodyHealth = (batch: ReturnType<typeof evaluateCustody>): 'pass' | 'warn' | 'fail' => {
  if (batch.averageScore >= 80) {
    return 'pass';
  }
  if (batch.averageScore >= 50 && batch.failed < batch.checks.length / 2) {
    return 'warn';
  }
  return 'fail';
};
