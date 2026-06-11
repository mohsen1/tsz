import { Sigil, normalizeLimit, withSigil } from './primitives.js';

export type StewardRegionId = Sigil<string, 'StewardRegionId'>;
export type StewardRunId = Sigil<string, 'StewardRunId'>;
export type LedgerId = Sigil<string, 'LedgerId'>;
export type EdictId = Sigil<string, 'EdictId'>;
export type CustodyId = Sigil<string, 'CustodyId'>;

export const severityBands = ['low', 'medium', 'high', 'critical'] as const;
export type SeverityBand = (typeof severityBands)[number];

export const domainStates = ['inactive', 'draft', 'active', 'paused', 'retired'] as const;
export type StewardDomainState = (typeof domainStates)[number];

export interface StewardContext {
  readonly regionId: StewardRegionId;
  readonly timestamp: string;
  readonly domain: string;
  readonly region: string;
  readonly state: StewardDomainState;
}

export interface StewardSignal {
  readonly id: Sigil<string, 'StewardSignalId'>;
  readonly metric: string;
  readonly severity: SeverityBand;
  readonly value: number;
  readonly tags: readonly string[];
  readonly observedAt: string;
}

export interface LedgerWindow {
  readonly id: Sigil<string, 'LedgerWindowId'>;
  readonly regionId: StewardRegionId;
  readonly startsAt: string;
  readonly endsAt: string;
  readonly openForAllBands: boolean;
  readonly allowedBands: readonly SeverityBand[];
}

export interface LedgerEdict<TScope extends string = 'global'> {
  readonly id: EdictId;
  readonly regionId: StewardRegionId;
  readonly ledgerId: LedgerId;
  readonly scope: TScope;
  readonly code: string;
  readonly condition: string;
  readonly penaltyPoints: number;
  readonly enabled: boolean;
  readonly createdAt: string;
}

export interface CustodyClause {
  readonly id: CustodyId;
  readonly regionId: StewardRegionId;
  readonly region: string;
  readonly title: string;
  readonly description: string;
  readonly requiresEncryption: boolean;
  readonly maxRtoMinutes: number;
  readonly maxRpoMinutes: number;
  readonly lastAuditAt: string;
}

export interface ConstraintEnvelope {
  readonly id: Sigil<string, 'ConstraintEnvelopeId'>;
  readonly regionId: StewardRegionId;
  readonly title: string;
  readonly required: readonly Sigil<string, 'ResourceId'>[];
  readonly forbidden: readonly Sigil<string, 'ResourceId'>[];
  readonly rationale: string;
}

export interface StewardEvaluation {
  readonly regionId: StewardRegionId;
  readonly runId: StewardRunId;
  readonly ledgerCoverage: number;
  readonly warningCount: number;
  readonly criticalCount: number;
  readonly readinessScore: number;
  readonly ledgerSignals: readonly { readonly edictId: EdictId; readonly fired: boolean; readonly weight: number }[];
  readonly windowCustody: boolean;
}

export interface LedgerEnvelope {
  readonly id: Sigil<string, 'LedgerEnvelopeId'>;
  readonly regionId: StewardRegionId;
  readonly title: string;
  readonly policies: readonly LedgerProfile[];
  readonly windows: readonly LedgerWindow[];
  readonly edicts: readonly LedgerEdict[];
  readonly constraints: readonly ConstraintEnvelope[];
  readonly custodyClauses: readonly CustodyClause[];
  readonly createdAt: string;
}

export interface LedgerProfile {
  readonly ledgerId: LedgerId;
  readonly regionId: StewardRegionId;
  readonly name: string;
  readonly domain: string;
  readonly state: StewardDomainState;
  readonly maxConcurrent: number;
  readonly maxCriticality: number;
  readonly windowsByBand: Record<SeverityBand, readonly LedgerWindow['id'][]>;
  readonly edicts: readonly LedgerEdict[];
}

export interface RankedLedger {
  readonly ledgerId: LedgerId;
  readonly score: number;
  readonly band: SeverityBand;
}

export interface StewardMatrix {
  readonly regionId: StewardRegionId;
  readonly asOf: string;
  readonly profileCount: number;
  readonly activeProfiles: readonly LedgerProfile[];
  readonly envelopes: readonly LedgerEnvelope[];
  readonly custodyScore: number;
}

export const createStewardRegionId = (value: string): StewardRegionId => withSigil(String(value).trim(), 'StewardRegionId');
export const createLedgerId = (value: string): LedgerId => withSigil(String(value).trim(), 'LedgerId');
export const createEdictId = (value: string): EdictId => withSigil(String(value).trim(), 'EdictId');
export const createStewardRunId = (value: string): StewardRunId => withSigil(String(value).trim(), 'StewardRunId');
export const clampLedgerRatio = (value: number): number => normalizeLimit(value);
export const buildLedgerMap = <T extends { readonly id: string }>(values: readonly T[]): Record<string, T> => {
  const map: Record<string, T> = {};
  for (const value of values) {
    map[value.id] = value;
  }
  return map;
};

export type OptionalBandRecord<T> = Partial<Record<SeverityBand, T>>;
export type BandBuckets<T> = {
  readonly low: readonly T[];
  readonly medium: readonly T[];
  readonly high: readonly T[];
  readonly critical: readonly T[];
};

export const normalizeSeverityBands = (bands: readonly SeverityBand[]): readonly SeverityBand[] => {
  const normalized = new Set<SeverityBand>();
  for (const band of bands) {
    normalized.add(band);
  }
  return Array.from(normalized);
};

export const pickTopSignals = (signals: readonly StewardSignal[], limit: number): readonly StewardSignal[] => {
  const target = normalizeLimit(limit);
  return [...signals]
    .sort((left, right) => right.value - left.value)
    .slice(0, target);
};
