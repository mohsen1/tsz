import type { Div } from "./widgets";
export type Input = HTMLElementTagNameMap["input"];
export type Button = HTMLElementTagNameMap["button"];
export function wrap(d: Div): Div { return d; }
export const inp: Input = document.createElement("input");
