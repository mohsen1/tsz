import { VaultLatch } from './latches';

export type FreightName = `freight:${string}`;

export interface FreightNode<TCargo, TOutput, TName extends FreightName> {
  readonly name: TName;
  readonly handle: (cargo: TCargo) => Promise<TOutput>;
}

export class FreightLattice<
  TCargo,
  TNodes extends readonly FreightNode<TCargo, unknown, FreightName>[],
> {
  readonly #nodes: TNodes;

  public constructor(nodes: TNodes) {
    this.#nodes = nodes;
  }

  public names(): readonly FreightName[] {
    return this.#nodes.map((node) => node.name);
  }

  public async dispatchAll(cargo: TCargo): Promise<readonly unknown[]> {
    const collected: unknown[] = [];
    const latch = new VaultLatch(
      { namespace: 'lattice:dispatch-all', tags: ['freight'] },
      async () => {
        collected.length = 0;
        return undefined;
      },
    );
    await using _seal = latch;

    for (const node of this.#nodes) {
      collected.push(await node.handle(cargo));
    }
    return collected;
  }
}
