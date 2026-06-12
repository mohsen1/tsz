import { Sigil } from './primitives.js';
import type { CustodyBatch } from './custody';
import type { StewardContext, LedgerEnvelope, StewardMatrix } from './shapes';

export interface StewardStore {
  saveEnvelope(ctx: StewardContext, envelope: LedgerEnvelope): Promise<void>;
  loadEnvelope(ctx: StewardContext, envelopeId: Sigil<string, 'LedgerEnvelopeId'>): Promise<LedgerEnvelope | null>;
  loadMatrix(ctx: StewardContext): Promise<StewardMatrix | null>;
  saveCustody(ctx: StewardContext, batch: CustodyBatch): Promise<void>;
}

export interface StewardMemoryState {
  envelopes: Map<string, LedgerEnvelope>;
  matrices: Map<string, StewardMatrix>;
  custody: Map<string, CustodyBatch[]>;
}

export class InMemoryStewardStore implements StewardStore {
  private readonly state: StewardMemoryState;

  constructor() {
    this.state = {
      envelopes: new Map(),
      matrices: new Map(),
      custody: new Map(),
    };
  }

  async saveEnvelope(ctx: StewardContext, envelope: LedgerEnvelope): Promise<void> {
    this.state.envelopes.set(`${ctx.regionId}:${envelope.id}`, envelope);
  }

  async loadEnvelope(ctx: StewardContext, envelopeId: Sigil<string, 'LedgerEnvelopeId'>): Promise<LedgerEnvelope | null> {
    return this.state.envelopes.get(`${ctx.regionId}:${envelopeId}`) ?? null;
  }

  async loadMatrix(ctx: StewardContext): Promise<StewardMatrix | null> {
    return this.state.matrices.get(ctx.regionId) ?? null;
  }

  async saveCustody(ctx: StewardContext, batch: CustodyBatch): Promise<void> {
    const key = `${ctx.regionId}:${ctx.domain}`;
    const existing = this.state.custody.get(key) ?? [];
    this.state.custody.set(key, [batch, ...existing]);
  }
}

export const storeKey = (ctx: StewardContext): string => `${ctx.regionId}:${ctx.domain}:${ctx.region}`;
