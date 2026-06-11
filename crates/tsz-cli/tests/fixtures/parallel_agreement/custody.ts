import { Sigil, normalizeLimit, withSigil } from './primitives.js';
import { CustodyClause, StewardRegionId, LedgerProfile, SeverityBand } from './shapes';

export interface CustodyCheck {
  readonly regionId: StewardRegionId;
  readonly ledgerId: Sigil<string, 'LedgerId'>;
  readonly clauseId: CustodyClause['id'];
  readonly severity: SeverityBand;
  readonly satisfied: boolean;
  readonly score: number;
  readonly findings: readonly string[];
}

export interface CustodyBatch {
  readonly regionId: StewardRegionId;
  readonly profile: LedgerProfile;
  readonly checks: readonly CustodyCheck[];
  readonly passed: number;
  readonly failed: number;
  readonly averageScore: number;
}

const computeScore = (value: number): number => {
  if (!Number.isFinite(value)) {
    return 0;
  }
  if (value < 0) {
    return 0;
  }
  return Math.min(100, value);
};

const custodyClauseSatisfied = (clause: CustodyClause, signals: readonly number[]): boolean => {
  const hasCriticalSignal = signals.some((value) => value > clause.maxRtoMinutes);
  const hasAuditRisk = signals.some((value) => value > clause.maxRpoMinutes);
  return !hasCriticalSignal && !hasAuditRisk;
};

export const evaluateCustody = (
  regionId: StewardRegionId,
  profile: LedgerProfile,
  clauses: readonly CustodyClause[],
  signalsByName: Record<string, readonly number[]>,
): CustodyBatch => {
  const checks: CustodyCheck[] = [];

  for (const clause of clauses) {
    const observed = signalsByName[clause.title] ?? [];
    const satisfied = custodyClauseSatisfied(clause, observed);
    const severityPenalty = clause.requiresEncryption ? 0 : 5;
    const score = satisfied
      ? 100 - severityPenalty
      : computeScore(100 - normalizeLimit(observed.length) * 0.5 - severityPenalty);
    const findings = clause.requiresEncryption
      ? observed.length > 0
        ? ['encryption requirement active']
        : []
      : ['rto/rpo tracked'];

    checks.push({
      regionId,
      ledgerId: profile.ledgerId,
      clauseId: clause.id,
      severity: profile.maxCriticality <= 3 ? 'low' : profile.maxCriticality <= 4 ? 'medium' : 'critical',
      satisfied,
      score,
      findings,
    });
  }

  const passed = checks.filter((check) => check.satisfied).length;
  const failed = checks.length - passed;
  const total = checks.reduce((sum, check) => sum + check.score, 0);
  const averageScore = checks.length > 0 ? computeScore(total / checks.length) : 0;

  return {
    regionId,
    profile,
    checks,
    passed,
    failed,
    averageScore,
  };
};

export const createCustodyEnvelope = (regionId: StewardRegionId): CustodyClause => ({
  id: withSigil(`${regionId}:default-custody`, 'CustodyId'),
  regionId,
  region: 'global',
  title: 'DR posture and audit baseline',
  description: 'Recovery lab operations should maintain minimal restoration and recovery guarantees',
  requiresEncryption: true,
  maxRtoMinutes: 30,
  maxRpoMinutes: 60,
  lastAuditAt: new Date().toISOString(),
});

export const custodyTrend = (history: readonly CustodyBatch[]): readonly { readonly at: string; readonly score: number }[] => {
  return history
    .slice()
    .sort((left, right) => left.regionId.localeCompare(right.regionId))
    .map((entry, index) => ({
      at: String(index),
      score: entry.averageScore,
    }));
};

export const custodyHealth = (batch: CustodyBatch): 'pass' | 'warn' | 'fail' => {
  if (batch.averageScore >= 90 && batch.failed === 0) {
    return 'pass';
  }
  if (batch.averageScore >= 70 && batch.failed < batch.checks.length / 2) {
    return 'warn';
  }
  return 'fail';
};
