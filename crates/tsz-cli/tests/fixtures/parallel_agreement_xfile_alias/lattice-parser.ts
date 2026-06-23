import type { Junction, FacetChild } from './lattice-node'
export function parseJunction(raw: string): Junction {
  if (raw === 'merge' || raw === 'overlap' || raw === 'subtract') return raw
  throw new Error('bad junction')
}
export function flattenFacets(child: FacetChild): readonly string[] {
  if (child.kind === 'leaf') return [child.label]
  return child.facets.flatMap(flattenFacets)
}
