export type Junction = 'merge' | 'overlap' | 'subtract'
export type FacetChild =
  | { readonly kind: 'leaf'; readonly label: string }
  | { readonly kind: 'group'; readonly facets: readonly FacetChild[] }
export type Clearance = 'restricted' | 'open'
export interface LatticeNodeProps {
  readonly junction: Junction
  readonly facets: readonly FacetChild[]
  readonly clearance: Clearance
}
