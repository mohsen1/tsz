import {
	die,
	Wrap,
	isWrap,
	shallowCopy,
	each,
	WRAP_TAG,
	set,
	CellState,
	isWrapable,
	isFrozen
} from "../internal"

/** Takes a snapshot of the current state of a wrap and finalizes it (but without freezing). This is a great utility to print the current state during debugging (no Proxies in the way). The output of current can also be safely leaked outside the producer. */
export function current<T>(value: Wrap<T>): T
export function current(value: Wrap<any>): any {
	if (!isWrap(value)) die(10, value)
	return currentImpl(value)
}

function currentImpl(value: any): any {
	if (!isWrapable(value) || isFrozen(value)) return value
	const state: CellState | undefined = value[WRAP_TAG]
	let copy: any
	let strict = true // Default to strict for compatibility
	if (state) {
		if (!state.modified_) return state.base_
		// Optimization: avoid generating new wraps during copying
		state.finalized_ = true
		copy = shallowCopy(value, state.scope_.forge_.useStrictShallowCopy_)
		strict = state.scope_.forge_.shouldUseStrictIteration()
	} else {
		copy = shallowCopy(value, true)
	}
	// recurse
	each(
		copy,
		(key, childValue) => {
			set(copy, key, currentImpl(childValue))
		},
		strict
	)
	if (state) {
		state.finalized_ = false
	}
	return copy
}
