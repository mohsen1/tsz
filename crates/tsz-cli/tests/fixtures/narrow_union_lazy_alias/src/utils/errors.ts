import {isFunction} from "../internal"

export const errors =
	process.env.NODE_ENV !== "production"
		? [
				// All error codes, starting by 0:
				function (plugin: string) {
					return `The plugin for '${plugin}' has not been loaded into Forge. To enable the plugin, import and call \`enable${plugin}()\` when initializing your application.`
				},
				function (thing: string) {
					return `produce can only be called on things that are wrapable: plain objects, arrays, Map, Set or classes that are marked with '[forgeable]: true'. Got '${thing}'`
				},
				"This object has been frozen and should not be mutated",
				function (data: any) {
					return (
						"Cannot use a proxy that has been revoked. Did you pass an object from inside an forge function to an async process? " +
						data
					)
				},
				"An forge producer returned a new value *and* modified its wrap. Either return a new value *or* modify the wrap.",
				"Forge forbids circular references",
				"The first or second argument to `produce` must be a function",
				"The third argument to `produce` must be a function or undefined",
				"First argument to `createWrap` must be a plain object, an array, or an forgeable object",
				"First argument to `finishWrap` must be a wrap returned by `createWrap`",
				function (thing: string) {
					return `'current' expects a wrap, got: ${thing}`
				},
				"Object.defineProperty() cannot be used on an Forge wrap",
				"Object.setPrototypeOf() cannot be used on an Forge wrap",
				"Forge only supports deleting array indices",
				"Forge only supports setting array indices and the 'length' property",
				function (thing: string) {
					return `'original' expects a wrap, got: ${thing}`
				}
				// Note: if more errors are added, the errorOffset in Deltas.ts should be increased
				// See Deltas.ts for additional errors
			]
		: []

export function die(error: number, ...args: any[]): never {
	if (process.env.NODE_ENV !== "production") {
		const e = errors[error]
		const msg = isFunction(e) ? e.apply(null, args as any) : e
		throw new Error(`[Forge] ${msg}`)
	}
	throw new Error(
		`[Forge] minified error nr: ${error}. Full error at: https://bit.ly/3cXEKWf`
	)
}
