import { clampLedgerRatio, LedgerEdict, LedgerProfile, SeverityBand } from './shapes';

export type LedgerScope = LedgerEdict['scope'];

export interface EdictContext {
  readonly band: SeverityBand;
  readonly activeSignals: number;
  readonly criticalSignals: number;
  readonly coverage: number;
}

export interface EdictEvaluation {
  readonly edict: LedgerEdict;
  readonly passed: boolean;
  readonly score: number;
  readonly reason: string;
}

export interface ProfileCoverage {
  readonly ledgerId: LedgerProfile['ledgerId'];
  readonly totalEdicts: number;
  readonly passingEdicts: number;
  readonly score: number;
}

const normalizePenalty = (penalty: number): number => {
  if (!Number.isFinite(penalty)) {
    return 1;
  }
  return Math.max(0, Math.min(5, penalty));
};

const evaluateCondition = (edict: LedgerEdict, context: EdictContext): boolean => {
  if (!edict.enabled) {
    return true;
  }
  if (edict.condition.includes('critical_signal')) {
    return context.criticalSignals <= 1;
  }
  if (edict.condition.includes('coverage_gap')) {
    return context.coverage >= 0.5;
  }
  if (edict.condition.includes('band=')) {
    const target = edict.condition.split('band=')[1] as SeverityBand;
    return target === context.band;
  }
  if (edict.condition.includes('signal>=')) {
    const limit = Number(edict.condition.split('>=')[1]);
    return context.activeSignals >= (Number.isFinite(limit) ? limit : 0);
  }
  return true;
};

export const evaluateLedgerEdict = (edict: LedgerEdict, context: EdictContext): EdictEvaluation => {
  const passed = evaluateCondition(edict, context);
  const weight = clampLedgerRatio(100 - normalizePenalty(edict.penaltyPoints));
  const score = passed ? weight : -weight;
  const reason = passed ? 'Edict satisfied' : `Edict violated: ${edict.condition}`;
  return { edict, passed, score, reason };
};

export const evaluateLedgerProfile = (profile: LedgerProfile, context: EdictContext): ProfileCoverage => {
  const evaluations = profile.edicts.map((edict) => evaluateLedgerEdict(edict, context));
  const passingEdicts = evaluations.filter((entry) => entry.passed).length;
  const maxScore = profile.edicts.length * 5;
  const rawScore = evaluations.reduce((sum, entry) => sum + entry.score, 0);
  const score = maxScore > 0 ? clampLedgerRatio((rawScore + maxScore) / 2) / 20 : 0;
  return {
    ledgerId: profile.ledgerId,
    totalEdicts: profile.edicts.length,
    passingEdicts,
    score,
  };
};

export const rankProfiles = (profiles: readonly LedgerProfile[], context: EdictContext): readonly { readonly ledgerId: LedgerProfile['ledgerId']; readonly score: number }[] => {
  return profiles
    .map((profile) => {
      const evaluation = evaluateLedgerProfile(profile, context);
      return {
        ledgerId: profile.ledgerId,
        score: evaluation.score,
      };
    })
    .sort((left, right) => right.score - left.score);
};

export const summarizeEdicts = (edicts: readonly EdictEvaluation[]) => {
  const passed = edicts.filter((entry) => entry.passed);
  return {
    total: edicts.length,
    passed: passed.length,
    failed: edicts.length - passed.length,
    score: edicts.reduce((sum, entry) => sum + entry.score, 0),
  };
};

export const findBlockingEdicts = (edicts: readonly EdictEvaluation[]) => {
  return edicts
    .filter((entry) => !entry.passed)
    .map((entry) => ({
      edictId: entry.edict.id,
      reason: entry.reason,
      penalty: entry.score,
    }));
};

export const mergeEdictSummaries = (left: readonly EdictEvaluation[], right: readonly EdictEvaluation[]) => {
  const byId = new Map<string, EdictEvaluation>();
  for (const entry of [...left, ...right]) {
    byId.set(entry.edict.id, entry);
  }
  return [...byId.values()];
};

export const buildEdictIndex = (edicts: readonly LedgerEdict[]): Readonly<Record<string, LedgerEdict>> => {
  const map: Record<string, LedgerEdict> = {};
  for (const edict of edicts) {
    map[edict.id] = edict;
  }
  return map;
};
