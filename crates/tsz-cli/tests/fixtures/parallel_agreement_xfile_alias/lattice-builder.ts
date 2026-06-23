import type { Junction, FacetChild, Clearance, LatticeNodeProps } from './lattice-node'
export class LatticeBuilder {
  #props: LatticeNodeProps
  constructor(props: LatticeNodeProps) { this.#props = props }
  setProp<K extends keyof LatticeNodeProps>(key: K, value: LatticeNodeProps[K]): this {
    this.#props = { ...this.#props, [key]: value }
    return this
  }
  withJunction(j: Junction): this { return this.setProp('junction', j) }
  withClearance(c: Clearance): this { return this.setProp('clearance', c) }
  addFacet(f: FacetChild): this {
    return this.setProp('facets', [...this.#props.facets, f])
  }
}
