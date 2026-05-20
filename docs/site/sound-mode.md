---
title: Sound Mode
layout: layouts/base.njk
page_class: sound-mode
permalink: /sound-mode/index.html
extra_scripts: <script src="/sound-mode-page.js" type="module"></script>
---

# Sound Mode

Sound Mode is `tsz`'s experiment in stricter TypeScript checking.

`tsz` is a TypeScript checker, compiler, and language service written in Rust. The main work today is still `tsc` compatibility and performance. Sound Mode sits behind that work. It is where `tsz` explores what TypeScript could catch if it were willing to be stricter about some of the tradeoffs in today's checker.

The name is ambitious, but it should be read practically. This is about stronger static checks for real TypeScript programs. It is not a mathematical proof of soundness, and it does not make third-party `.d.ts` files truthful.

The timing is more interesting than it used to be. A stricter checker has always had a cost: humans have to fix the extra errors. That cost changes when more code is written and revised by AI. AI can sit in a tighter feedback loop with the checker. It can rewrite code, satisfy more precise types, and use diagnostics as guidance. That makes stricter TypeScript worth revisiting.

Sound Mode is available today only as a narrow demo through the playground/WASM `soundMode` input and a hidden `--sound` CLI flag. There is no stable `tsconfig` support yet.

## Why This Is Useful

TypeScript is useful because it is pragmatic. It accepts a lot of JavaScript patterns, works with a huge ecosystem, and gives developers room to model code that was not designed with static types in mind.

That pragmatism leaves gaps. Some gaps are fine. Some become places where bugs hide.

Sound Mode starts with a few of those places. Sticky freshness keeps object literal information alive after assigning through a variable, so an excess property typo does not disappear just because the object got a name. Method bivariance tightening makes method assignments behave less surprisingly when an implementation accepts a narrower parameter type than the interface promises. Nested `any` escape detection looks for cases where `any` leaks into typed structures and makes unchecked values look safer than they are.

These are small examples, but they point at a useful direction. TypeScript already has a strong culture of teams choosing stricter settings over time. Sound Mode asks what the next step could look like if the checker had more room to preserve intent and reject suspicious assignments.

The goal is not to make TypeScript unpleasant. A stricter mode only works if the errors are useful. It should catch mistakes that serious TypeScript users recognize, give clear diagnostics, and avoid breaking common patterns just to prove a point.

## What It Does Today

The current demo is intentionally small, but it is real enough to explain the direction. Start with a case TypeScript already catches. When an object literal is assigned directly to a narrower type, TypeScript keeps the object "fresh" and rejects the extra property.

```ts
interface Point2D {
  x: number;
  y: number;
}

const point: Point2D = {
  x: 1,
  y: 2,
  z: 3,
};
```

The more interesting case is what happens after the object has a name. In normal TypeScript, the extra `z` is no longer treated the same way. Sound Mode keeps that freshness signal alive long enough to flag the assignment. Toggle Sound Mode off and the diagnostic goes away.

<div data-sound-mode-example="sound_mode"></div>

The second demo tightens method parameter bivariance. In TypeScript today, methods have compatibility rules that are friendlier to existing JavaScript patterns, but that can let an implementation accept a narrower parameter than the interface says callers are allowed to pass. Sound Mode treats that as a boundary worth checking.

<div data-sound-mode-example="sound_mode_argument"></div>

The third demo follows a nested `any` escape into a more precise shape. This is not a complete `any` policy, but it shows the kind of thing Sound Mode should make visible. If a value is really unknown or untrusted, the code should say so at the boundary instead of letting `any` make the rest of the program look safer than it is.

<div data-sound-mode-example="sound_mode_array"></div>

These examples use TSZ-style labels in the page so the stricter diagnostics are easy to see. The final diagnostic design is still open.

## What Sound Mode Should Catch Next

The playground examples above are the parts of Sound Mode that work today. The next question is where stricter TypeScript can pay off after that.

The examples below are planned checks. Current Sound Mode does not report them yet. Some have diagnostic codes assigned, and one has prototype checker code that still needs to be connected to the real pipeline. That distinction matters.

### Mutable Array Covariance

Mutable array covariance is tracked as `TSZ2001`. A prototype helper exists, but it is not wired into the checker pipeline yet.

This check is about one of TypeScript's oldest sharp edges: assigning a more specific mutable array where a broader mutable array is expected, then writing the wrong thing through the broader type. The read side looks fine. The write side is where the bug enters. Sound Mode should make that mutation boundary visible.

```ts
class Animal {}
class Dog extends Animal {
  bark() {}
}
class Cat extends Animal {}

const dogs: Dog[] = [new Dog()];
const animals: Animal[] = dogs;

animals.push(new Cat());
dogs[1].bark(); // 💥 runtime crash: Cat has no bark()
```

### Unchecked Indexed Access

Unchecked indexed access is tracked as `TSZ5001`. Sound Mode does not currently imply `noUncheckedIndexedAccess`. Maybe this is not necessary with the existing `tsc` flag. We need to explore more here.

This check is about reads that look total but are partial at runtime. An array index, object key, or map-like access can miss. TypeScript often lets the result flow as if the value is definitely there. A stricter mode should be able to force the code to handle the missing case where the program actually has one.

```ts
const names: string[] = [];

const firstName = names[0];
firstName.toUpperCase(); // 💥 runtime crash: firstName is undefined
```

### Unsafe Assertions

Unsafe assertions are tracked as `TSZ1011`. This is planned work and has no implementation yet.

Assertions are useful, but they are also an escape hatch. Some assertions are harmless ways to help the checker. Others throw away information and replace it with a stronger claim the program has not earned. Sound Mode should eventually distinguish between those cases and complain when an assertion jumps across too much type information.

```ts
type Profile = {
  name: string;
};

const profile = JSON.parse('{"name":42}') as Profile;
profile.name.toUpperCase(); // 💥 runtime crash: name is a number
```

### Empty Array Reduction

Empty array reduction is tracked as `TSZ5003` and belongs in the later-candidate bucket.

This check is about a runtime crash that hides behind a familiar API. Calling `reduce` without an initial value can fail when the array is empty. The type may make the operation look safe, but the runtime still depends on a value being present. Sound Mode should eventually be able to flag that kind of unchecked assumption.

```ts
const amounts: number[] = [];

const total = amounts.reduce((sum, amount) => {
  return sum + amount;
}); // 💥 runtime crash: reduce of empty array with no initial value
```

## How To Try It

The easiest way to try Sound Mode is the playground. The examples on this page use the same WASM path as the playground, with `soundMode` enabled and disabled by the checkbox.

There is also a hidden CLI flag for local exploration.

```bash
tsz check --sound src/
```

That flag is not a stable configuration surface. There is no supported `tsconfig` option for Sound Mode today, and `compilerOptions.sound` is not something a normal TypeScript project should add. The current shape is enough to make the idea concrete while `tsz` continues the compatibility and performance work that comes first.

## Future plans

Sound Mode is not finished product work. The current demos are intentionally small, and `tsz` still needs to finish `tsc` compatibility and performance tuning before this becomes a main workstream.

One possible direction is a mode that treats `any` very differently in user code and library code. Many projects would benefit from banning or sharply limiting `any` in code they own, while still depending on libraries that use `any` internally or expose older declaration patterns. That only works if `tsz` can reliably tell user-authored code from library code and draw the boundary in a predictable way.

A more ambitious version would make libraries more Sound Mode ready automatically. For example, `tsz` could project unsafe, `any`-heavy declarations into safer boundaries that use `unknown` where that better represents what the caller actually knows. That would let application code get stricter checking without requiring every dependency to rewrite its declarations first.

There are hard details in that idea. Some `any` uses are intentional. Some library declarations depend on permissive behavior for good reasons. Some transformations would be wrong or too noisy. Sound Mode should earn trust by being precise about these boundaries.

The long-term question is whether `tsz` can offer a stricter TypeScript experience that works with real projects: stricter in user code, honest at dependency boundaries, and practical enough to adopt gradually.

## Looking for feedback

Sound Mode needs input from TypeScript users who have lived with the tradeoffs.

The most useful feedback is concrete code. Show a case TypeScript accepts today that you think a stricter checker should reject. Explain why the accepted code is dangerous, confusing, or too easy to write by accident. Small examples are ideal, especially when they come from real patterns.

Counterexamples matter just as much. If a stricter rule would reject code that is common, useful, and reasonable, that is important to know early. Sound Mode should not become a pile of clever restrictions that only work in toy programs.

The project is especially interested in where the boundary should be. What should be rejected in user-authored code? What should still be allowed in library declarations? Where should `any` become `unknown`? Where would that make code safer, and where would it just create noise?

Stricter TypeScript is worth exploring, but the shape has to come from real use. Sound Mode is the place to test those ideas before they harden into defaults.

The detailed plan is tracked in [SOUND_MODE.md](https://github.com/mohsen1/tsz/blob/main/docs/plan/SOUND_MODE.md), and broader milestones are in the [Internal Roadmap](https://github.com/mohsen1/tsz/blob/main/docs/plan/ROADMAP.md).
