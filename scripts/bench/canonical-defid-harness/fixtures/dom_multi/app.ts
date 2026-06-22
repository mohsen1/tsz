import { el, type Div, type Anchor } from "./widgets";
import { wrap, type Input, type Button } from "./forms";
const d: Div = el("div");
const a: Anchor = el("a");
const i: Input = el("input");
const b: Button = el("button");
const tags: string[] = [d.tagName, a.href, i.value, b.type, wrap(d).tagName];
export { tags };
