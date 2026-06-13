export type GaugeWindow<TPayload extends Record<string, unknown>> = {
  readonly emit: (entry: TPayload) => void;
  readonly drain: () => Promise<readonly TPayload[]>;
};

export const makeGaugeWindow = <
  TPayload extends Record<string, unknown>,
>(): GaugeWindow<TPayload> => {
  const entries: TPayload[] = [];
  return {
    emit: (entry) => {
      entries.push(entry);
    },
    drain: async () => entries.slice(),
  };
};

export async function flushGauges<TPayload extends Record<string, unknown>>(
  window: GaugeWindow<TPayload>,
): Promise<number> {
  const drained = await window.drain();
  return drained.length;
}
