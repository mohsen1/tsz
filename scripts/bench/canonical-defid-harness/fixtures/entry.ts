import { pickA, a } from "./mod_a";
import { pickB, b } from "./mod_b";
// Use the cross-module Shared generically from both arenas to force
// cross-boundary identity resolution of the same declaration.
const ra = pickA(a);
const rb = pickB(b);
const merged: { id: number; name: string } = ra.id > rb.id ? ra : rb;
export { merged };
