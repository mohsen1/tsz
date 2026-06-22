// #13862 witness: indexing HTMLElementTagNameMap by 'div' resolves HTMLDivElement,
// whose registered DefId collides (raw-u32) with a different-named lib symbol
// (FileSystemEntry). Forces the raw_symbol_fallback_def collision path.
declare const map: HTMLElementTagNameMap;
type Div = HTMLElementTagNameMap["div"];
const d: Div = map.div;
const tag: string = d.tagName;
function make(): HTMLElementTagNameMap["div"] { return map.div; }
export {};
