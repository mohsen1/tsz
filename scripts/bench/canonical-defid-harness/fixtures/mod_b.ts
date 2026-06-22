import type { Shared as S, Other } from "./shared";
export function pickB<T extends S>(x: T): T { return x; }
export const b: S = { id: 2, name: "b" };
export const o: Other = { kind: "k", data: null };
