// Minimal #13862 isolation witness: HTMLDivElement (resolved via the
// HTMLElementTagNameMap["div"] index) and FileSystemEntry both bound from the
// DOM lib. Per resolver.rs::raw_symbol_fallback_def, HTMLDivElement's registered
// DefId collides (raw-u32) with FileSystemEntry's symbol id; the read of
// HTMLElementTagNameMap["div"] drives the colliding raw_symbol_fallback_def path.
// Keeps both names live so the content-difference (different-named) counter test
// has its colliding partner present. Smallest fixture that fires #14520.
declare const map: HTMLElementTagNameMap;
type Div = HTMLElementTagNameMap["div"];
declare const fse: FileSystemEntry;
const d: Div = map.div;
const tag: string = d.tagName;
const fsName: string = fse.name;
export {};
