import { Sigil, normalizeLimit } from './primitives.js';
import type { StewardContext, StewardSignal, SeverityBand, ConstraintEnvelope } from './shapes';

export type ConstraintState = 'met' | 'partial' | 'breached';

export interface ConstraintDefinition {
  readonly id: Sigil<string, 'ConstraintDefinitionId'>;
  readonly scope: string;
  readonly severity: SeverityBand;
  readonly maxLatencyMs: number;
  readonly maxCostBps: number;
  readonly requiresWindow: boolean;
}

export interface ConstraintExecution {
  readonly envelope: ConstraintEnvelope;
  readonly state: ConstraintState;
  readonly coverage: number;
  readonly reasons: readonly string[];
}

const clamp = (value: number): number => {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(100, value));
};

export const buildConstraint = (ctx: StewardContext, definition: ConstraintDefinition): ConstraintEnvelope => {
  const resource = `region/${ctx.regionId}` as Sigil<string, 'ResourceId'>;
  return {
    id: `${ctx.regionId}:${definition.id}` as ConstraintEnvelope['id'],
    regionId: ctx.regionId,
    title: `Constraint for ${definition.scope}`,
    required: [resource],
    forbidden: [
      ...(definition.requiresWindow ? ([`window:${ctx.timestamp}` as Sigil<string, 'ResourceId'>] as Sigil<string, 'ResourceId'>[]) : []),
    ],
    rationale: `${ctx.domain} controls for ${definition.scope}`,
  };
};

export const evaluateConstraintEnvelope = (envelope: ConstraintEnvelope, signals: readonly StewardSignal[]): ConstraintExecution => {
  const reasons: string[] = [];
  const covered = envelope.required.filter((resource) => resource.length > 0).length;
  const totalSignals = normalizeLimit(signals.length);
  const coverage = totalSignals > 0 ? clamp((covered / Math.max(1, totalSignals)) * 100) : 0;

  for (const signal of signals) {
    if (signal.value > 95 && envelope.forbidden.includes(`region:${signal.metric}` as Sigil<string, 'ResourceId'>)) {
      reasons.push(`forbidden metric observed ${signal.metric}`);
    }
    if (signal.tags.includes('regulatory-risk')) {
      reasons.push(`risk tag flagged on ${signal.metric}`);
    }
  }

  if (coverage < 20) {
    reasons.push('low signal coverage against constraint envelope');
  }

  let state: ConstraintState = 'met';
  if (reasons.length > 0) {
    state = coverage < 60 ? 'breached' : 'partial';
  }

  return {
    envelope,
    state,
    coverage,
    reasons,
  };
};

export const summarizeConstraintState = (executions: readonly ConstraintExecution[]) => {
  const met = executions.filter((entry) => entry.state === 'met').length;
  const partial = executions.filter((entry) => entry.state === 'partial').length;
  const breached = executions.filter((entry) => entry.state === 'breached').length;

  return {
    total: executions.length,
    met,
    partial,
    breached,
    health: met === executions.length ? 'green' : breached > 0 ? 'red' : 'yellow',
  };
};

export const normalizeConstraint = (signalCount: number, maxBreach: number): number => {
  const safeSignal = normalizeLimit(signalCount);
  const safeBreach = normalizeLimit(maxBreach);
  if (safeBreach === 0) {
    return 0;
  }
  return clamp(100 - (safeSignal / safeBreach) * 100);
};
