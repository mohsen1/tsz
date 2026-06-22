import type { Shared } from "./shared";
export function pickA<T extends Shared>(x: T): T { return x; }
export const a: Shared = { id: 1, name: "a" };
