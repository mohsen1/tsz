import { LatticeBuilder } from './lattice-builder'
import { parseJunction, flattenFacets } from './lattice-parser'
import type { LatticeNodeProps } from './lattice-node'
export function build(props: LatticeNodeProps): LatticeBuilder {
  return new LatticeBuilder(props)
    .withJunction(parseJunction('merge'))
    .withClearance('open')
}
export { flattenFacets }
