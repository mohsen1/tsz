import { FreightLattice, type FreightName, type FreightNode } from './lattice';

export type RelayBucket<TNames extends FreightName> = {
  [TName in TNames]?: { readonly id: TName; readonly position: number };
};

const asStamp = <TName extends FreightName>(name: TName, position: number) => ({
  id: name,
  position,
});

export class RelayRegistry<
  TCargo,
  TNodes extends readonly FreightNode<TCargo, unknown, FreightName>[],
> {
  readonly #lattice: FreightLattice<TCargo, TNodes>;

  public constructor(lattice: FreightLattice<TCargo, TNodes>) {
    this.#lattice = lattice;
  }

  public async relayAll(cargo: TCargo): Promise<RelayBucket<TNodes[number]['name']>> {
    const outputs = await this.#lattice.dispatchAll(cargo);
    const stamped = outputs
      .map((entry, position) =>
        asStamp(this.#lattice.names()[position] as TNodes[number]['name'], position),
      )
      .reduce(
        (bucket, current) => {
          const next = { ...bucket };
          next[current.id] = current;
          return next;
        },
        Object.create(null) as RelayBucket<TNodes[number]['name']>,
      );
    return stamped;
  }
}
