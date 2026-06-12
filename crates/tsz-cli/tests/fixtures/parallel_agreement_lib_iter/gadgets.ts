const randomUUID = (): string => 'u';
import {
  asGadgetId,
  type GadgetKind,
  type ManeuverConfig,
  type ManeuverPhase,
  type ManeuverGadgetId,
  type ManeuverGadgetOutput,
} from './types';

export type ManeuverGadgetType<T extends string = string> = `recovery/ops/sim/${T}`;
export type ManeuverGadgetTag<T extends string = string> = `@maneuver/${T}`;

export interface ManeuverGadgetMetadata {
  readonly gadgetId: ManeuverGadgetId;
  readonly namespace: string;
  readonly version: `${number}.${number}.${number}`;
  readonly supports: readonly ManeuverPhase[];
  readonly weight: number;
  readonly tags: readonly string[];
}

export interface ManeuverGadgetInput<TPayload = unknown, TContext = { readonly namespace: string }> {
  readonly runId: string;
  readonly phase: ManeuverPhase;
  readonly payload: TPayload;
  readonly config: Readonly<Record<string, unknown>>;
  readonly context: TContext;
}

export interface ManeuverGadgetDescriptor<
  TInput = object,
  TOutput = object,
  TKind extends string = string,
> {
  readonly id: ManeuverGadgetId;
  readonly name: string;
  readonly kind: GadgetKind<TKind>;
  readonly version: `${number}.${number}.${number}`;
  readonly supports: readonly ManeuverPhase[];
  readonly metadata: ManeuverGadgetMetadata;
  readonly execute: (input: ManeuverGadgetInput<TInput, { readonly namespace: string }>) => Promise<ManeuverGadgetOutput<TOutput>>;
}

export interface GadgetLease {
  readonly gadgetId: ManeuverGadgetId;
  readonly removed: boolean;
  [Symbol.dispose](): void;
  [Symbol.asyncDispose](): Promise<void>;
}

type AnyGadget = ManeuverGadgetDescriptor<object, object, string>;

const orderedSupports = (left: ManeuverPhase, right: ManeuverPhase): number =>
  Number(left) > Number(right) ? 1 : 0;

const asGadgetTag = (value: string): ManeuverGadgetTag<string> => `@maneuver/${value}`;

export class AdaptiveGadgetRegistry<TGadgets extends readonly AnyGadget[]> {
  readonly #state = {
    gadgets: new Map<string, AnyGadget>(),
    phaseMap: new Map<ManeuverPhase, AnyGadget[]>(),
    order: [] as AnyGadget[],
  };
  #disposed = false;

  public constructor(gadgets: TGadgets) {
    for (const gadget of gadgets) {
      this.#register(gadget);
    }
  }

  public get count(): number {
    return this.#state.gadgets.size;
  }

  public get isDisposed(): boolean {
    return this.#disposed;
  }

  public getGadgetIds(): readonly ManeuverGadgetId[] {
    return [...this.#state.gadgets.keys()].map((gadgetId) => gadgetId as ManeuverGadgetId);
  }

  public supports(phase: ManeuverPhase): readonly AnyGadget[] {
    return [...(this.#state.phaseMap.get(phase) ?? [])];
  }

  public addGadget<TInput, TOutput, TKind extends string>(
    gadget: ManeuverGadgetDescriptor<TInput, TOutput, TKind>,
  ): GadgetLease {
    const execute = (
      input: ManeuverGadgetInput<object, { readonly namespace: string }>,
    ): Promise<ManeuverGadgetOutput<object>> =>
      gadget.execute(
        input as ManeuverGadgetInput<TInput, { readonly namespace: string }>,
      ) as Promise<ManeuverGadgetOutput<object>>;

    const normalized: AnyGadget = {
      ...gadget,
      metadata: {
        ...gadget.metadata,
        tags: [...gadget.metadata.tags, asGadgetTag(gadget.kind)],
      },
      execute,
    };
    this.#register(normalized);

    let active = true;
    const gadgetId = gadget.id;
    const removeGadget = (): void => {
      if (!active) {
        return;
      }
      active = false;
      this.#remove(gadgetId);
    };

    return {
      gadgetId,
      removed: false,
      [Symbol.dispose](): void {
        removeGadget();
      },
      async [Symbol.asyncDispose](): Promise<void> {
        removeGadget();
      },
    };
  }

  public async runPhase<TInput extends object, TContext extends { namespace?: string }, TOutput extends object>(
    phase: ManeuverPhase,
    context: TContext,
    input: TInput,
  ): Promise<readonly ManeuverGadgetOutput<TOutput>[]> {
    const outputs: ManeuverGadgetOutput<TOutput>[] = [];

    for (const gadget of this.supports(phase)) {
      const started = performance.now();
      const payload = {
        runId: `${phase}-${randomUUID()}`,
        phase,
        payload: input,
        config: { namespace: context.namespace ?? 'global', phase },
        context: { namespace: context.namespace ?? 'global' },
      } as ManeuverGadgetInput<TInput, { readonly namespace: string }>;

      const output = (await gadget.execute(payload)) as ManeuverGadgetOutput<TOutput>;
      outputs.push({
        ...output,
        gadgetId: gadget.id,
        phase,
        timestamp: new Date().toISOString(),
        elapsedMs: Math.max(0, performance.now() - started),
      });
    }

    return outputs;
  }

  public [Symbol.iterator](): IterableIterator<AnyGadget> {
    return this.#state.order[Symbol.iterator]();
  }

  public snapshot(): Readonly<Record<string, AnyGadget>> {
    const map: Record<string, AnyGadget> = {};
    for (const gadget of this.#state.order) {
      map[String(gadget.kind)] = gadget;
    }
    return map;
  }

  public [Symbol.dispose](): void {
    this.#disposed = true;
    this.#state.gadgets.clear();
    this.#state.phaseMap.clear();
    this.#state.order.length = 0;
  }

  public async [Symbol.asyncDispose](): Promise<void> {
    this.#disposed = true;
    this.#state.gadgets.clear();
    this.#state.phaseMap.clear();
    this.#state.order.length = 0;
  }

  #register(gadget: AnyGadget): void {
    const key = String(gadget.id);
    if (this.#state.gadgets.has(key)) {
      throw new Error(`gadget already exists: ${key}`);
    }

    this.#state.gadgets.set(key, gadget);
    this.#state.order.push(gadget);

    for (const phase of gadget.supports) {
      const bucket = this.#state.phaseMap.get(phase) ?? [];
      bucket.push(gadget);
      bucket.sort((left, right) => right.metadata.weight - left.metadata.weight);
      this.#state.phaseMap.set(phase, bucket);
    }
  }

  #remove(gadgetId: ManeuverGadgetId): void {
    const key = String(gadgetId);
    this.#state.gadgets.delete(key);
    this.#state.order = this.#state.order.filter((entry) => String(entry.id) !== key);
    for (const [phase, gadgets] of this.#state.phaseMap.entries()) {
      this.#state.phaseMap.set(
        phase,
        gadgets.filter((entry) => String(entry.id) !== key),
      );
    }
  }
}

export const defineGadgets = <
  const TGadgets extends readonly ManeuverGadgetDescriptor<object, object, string>[],
>(gadgets: TGadgets): TGadgets => {
  const keys = gadgets.map((gadget) => String(gadget.id));
  if (new Set(keys).size !== keys.length) {
    throw new Error('duplicate gadget ids');
  }
  return gadgets;
};

export const createGadget = <
  TInput,
  TOutput,
  TKind extends string,
>(
  kind: GadgetKind<TKind>,
  name: string,
  namespace: string,
  supports: readonly ManeuverPhase[],
  run: (input: ManeuverGadgetInput<TInput, { readonly namespace: string }>) => Promise<ManeuverGadgetOutput<TOutput>>,
): ManeuverGadgetDescriptor<TInput, TOutput, TKind> => ({
  id: asGadgetId(`${namespace}::${name}`),
  name,
  kind,
  version: '1.0.0',
  supports: supports.toSorted(orderedSupports),
  metadata: {
    gadgetId: asGadgetId(`${namespace}::${name}`),
    namespace,
    version: '1.0.0',
    supports: supports.toSorted(orderedSupports),
    weight: 1,
    tags: ['adaptive', kind],
  },
  execute: run,
});

export const buildGadgetRunInput = <
  TPayload extends object,
  TContext extends object,
>(
  phase: ManeuverPhase,
  gadgetId: string,
  payload: TPayload,
  context: TContext,
): ManeuverGadgetInput<TPayload, TContext> => ({
  runId: `${phase}-${gadgetId}-${Date.now()}`,
  phase,
  payload,
  config: { gadgetId },
  context,
});

export const buildGadgetsFromConfig = (config: {
  readonly tenant: string;
  readonly labels: readonly string[];
  readonly profile?: string;
}): readonly ManeuverGadgetDescriptor[] => {
  const namespace = `tenant:${config.tenant}`;
  const normalize = createGadget(
    'recovery/ops/sim/normalize',
    `normalize-${config.labels.length}`,
    namespace,
    ['discover', 'shape'],
    async (input) => ({
      gadgetId: asGadgetId(`${namespace}:normalize`),
      phase: input.phase,
      timestamp: new Date().toISOString(),
      elapsedMs: 0,
      payload: {
        normalized: true,
        topology: config.labels.join('-'),
      },
    }),
  );

  const score = createGadget(
    'recovery/ops/sim/score',
    `score-${config.profile ?? 'default'}`,
    namespace,
    ['simulate', 'validate'],
    async (input) => ({
      gadgetId: asGadgetId(`${namespace}:score`),
      phase: input.phase,
      timestamp: new Date().toISOString(),
      elapsedMs: 0,
      payload: {
        score: Math.min(config.labels.length + 3, 10),
      },
    }),
  );

  const recommend = createGadget(
    'recovery/ops/sim/recommend',
    `recommend-${config.tenant}`,
    namespace,
    ['recommend', 'execute', 'verify'],
    async (input) => ({
      gadgetId: asGadgetId(`${namespace}:recommend`),
      phase: input.phase,
      timestamp: new Date().toISOString(),
      elapsedMs: 0,
      payload: {
        recommended: true,
        labels: config.labels,
      },
    }),
  );

  return defineGadgets([normalize, score, recommend]);
};

export const simulateGadgetConfig = (config: ManeuverConfig): Record<string, unknown> => ({
  voyageId: String(config.voyageId),
  topology: config.topology,
  gadgetCount: config.gadgets.length,
  phases: config.phaseSequence.join(','),
});
