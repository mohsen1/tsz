export type Div = HTMLElementTagNameMap["div"];
export type Span = HTMLElementTagNameMap["span"];
export type Anchor = HTMLElementTagNameMap["a"];
export function el<K extends keyof HTMLElementTagNameMap>(k: K): HTMLElementTagNameMap[K] {
  return document.createElement(k);
}
