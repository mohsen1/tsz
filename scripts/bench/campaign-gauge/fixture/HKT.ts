// Higher-kinded-type registry: the fp-ts `URItoKind` idiom the #14344/#14345
// campaign substrate targets. Cross-file `declare module` augmentation feeds the
// registry; `keyof` + indexed-access read it back. Deterministic diagnostics
// under the composed flag stack are the gauge signal.
export interface URItoKind<A> {}
export type URIS = keyof URItoKind<unknown>;
export type Kind<U extends URIS, A> = U extends URIS ? URItoKind<A>[U] : never;
