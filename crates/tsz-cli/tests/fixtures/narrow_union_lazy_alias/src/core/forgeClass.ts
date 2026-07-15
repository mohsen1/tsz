import {
	IProduceWithDeltas,
	IProduce,
	CellState,
	Wrapped,
	isWrapable,
	processResult,
	Delta,
	Bundleish,
	WRAP_TAG,
	Wrap,
	DeltaListener,
	isWrap,
	isBag,
	isSack,
	makeWrapWrap,
	getAddon,
	die,
	enterScope,
	revokeScope,
	leaveScope,
	useDeltasInScope,
	getScope,
	NOTHING,
	freeze,
	current,
	CellScope,
	registerChildFinalizationCallback,
	SlotKind,
	BagSackAddon,
	AnyBag,
	AnySack,
	isBundleish,
	isFunction,
	isBoolean,
	AddonBagSack,
	AddonDeltas
} from "../internal"

interface ProducersFns {
	produce: IProduce
	produceWithDeltas: IProduceWithDeltas
}

export type StrictKind = boolean | "class_only"

export class Forge implements ProducersFns {
	autoFreeze_: boolean = true
	useStrictShallowCopy_: StrictKind = false
	useStrictIteration_: boolean = false

	constructor(config?: {
		autoFreeze?: boolean
		useStrictShallowCopy?: StrictKind
		useStrictIteration?: boolean
	}) {
		if (isBoolean(config?.autoFreeze)) this.setAutoFreeze(config!.autoFreeze)
		if (isBoolean(config?.useStrictShallowCopy))
			this.setUseStrictShallowCopy(config!.useStrictShallowCopy)
		if (isBoolean(config?.useStrictIteration))
			this.setUseStrictIteration(config!.useStrictIteration)
	}

	/**
	 * The `produce` function takes a value and a "recipe function" (whose
	 * return value often depends on the base state). The recipe function is
	 * free to mutate its first argument however it wants. All mutations are
	 * only ever applied to a __copy__ of the base state.
	 *
	 * Pass only a function to create a "curried producer" which relieves you
	 * from passing the recipe function every time.
	 *
	 * Only plain objects and arrays are made mutable. All other objects are
	 * considered uncopyable.
	 *
	 * Note: This function is __bound__ to its `Forge` instance.
	 *
	 * @param {any} base - the initial state
	 * @param {Function} recipe - function that receives a proxy of the base state as first argument and which can be freely modified
	 * @param {Function} deltaListener - optional function that will be called with all the deltas produced here
	 * @returns {any} a new state, or the initial state if nothing was modified
	 */
	produce: IProduce = (base: any, recipe?: any, deltaListener?: any) => {
		// curried invocation
		if (isFunction(base) && !isFunction(recipe)) {
			const defaultBase = recipe
			recipe = base

			const self = this
			return function curriedProduce(
				this: any,
				base = defaultBase,
				...args: any[]
			) {
				return self.produce(base, (wrap: Wrapped) => recipe.call(this, wrap, ...args)) // prettier-ignore
			}
		}

		if (!isFunction(recipe)) die(6)
		if (deltaListener !== undefined && !isFunction(deltaListener)) die(7)

		let result

		// Only plain objects, arrays, and "forgeable classes" are wraped.
		if (isWrapable(base)) {
			const scope = enterScope(this)
			const proxy = makeWrap(scope, base, undefined)
			let hasError = true
			try {
				result = recipe(proxy)
				hasError = false
			} finally {
				// finally instead of catch + rethrow better preserves original stack
				if (hasError) revokeScope(scope)
				else leaveScope(scope)
			}
			useDeltasInScope(scope, deltaListener)
			return processResult(result, scope)
		} else if (!base || !isBundleish(base)) {
			result = recipe(base)
			if (result === undefined) result = base
			if (result === NOTHING) result = undefined
			if (this.autoFreeze_) freeze(result, true)
			if (deltaListener) {
				const p: Delta[] = []
				const ip: Delta[] = []
				getAddon(AddonDeltas).generateReplacementDeltas_(base, result, {
					deltas_: p,
					inverseDeltas_: ip
				} as CellScope) // dummy scope
				deltaListener(p, ip)
			}
			return result
		} else die(1, base)
	}

	produceWithDeltas: IProduceWithDeltas = (base: any, recipe?: any): any => {
		// curried invocation
		if (isFunction(base)) {
			return (state: any, ...args: any[]) =>
				this.produceWithDeltas(state, (wrap: any) => base(wrap, ...args))
		}

		let deltas: Delta[], inverseDeltas: Delta[]
		const result = this.produce(base, recipe, (p: Delta[], ip: Delta[]) => {
			deltas = p
			inverseDeltas = ip
		})
		return [result, deltas!, inverseDeltas!]
	}

	createWrap<T extends Bundleish>(base: T): Wrap<T> {
		if (!isWrapable(base)) die(8)
		if (isWrap(base)) base = current(base as Wrap<T>)
		const scope = enterScope(this)
		const proxy = makeWrap(scope, base, undefined)
		proxy[WRAP_TAG].isManual_ = true
		leaveScope(scope)
		return proxy as any
	}

	finishWrap<D extends Wrap<any>>(
		wrap: D,
		deltaListener?: DeltaListener
	): D extends Wrap<infer T> ? T : never {
		const state: CellState = wrap && (wrap as any)[WRAP_TAG]
		if (!state || !state.isManual_) die(9)
		const {scope_: scope} = state
		useDeltasInScope(scope, deltaListener)
		return processResult(undefined, scope)
	}

	/**
	 * Pass true to automatically freeze all copies created by Forge.
	 *
	 * By default, auto-freezing is enabled.
	 */
	setAutoFreeze(value: boolean) {
		this.autoFreeze_ = value
	}

	/**
	 * Pass true to enable strict shallow copy.
	 *
	 * By default, forge does not copy the object descriptors such as getter, setter and non-enumrable properties.
	 */
	setUseStrictShallowCopy(value: StrictKind) {
		this.useStrictShallowCopy_ = value
	}

	/**
	 * Pass false to use faster iteration that skips non-enumerable properties
	 * but still handles symbols for compatibility.
	 *
	 * By default, strict iteration is enabled (includes all own properties).
	 */
	setUseStrictIteration(value: boolean) {
		this.useStrictIteration_ = value
	}

	shouldUseStrictIteration(): boolean {
		return this.useStrictIteration_
	}

	applyDeltas<T extends Bundleish>(base: T, deltas: readonly Delta[]): T {
		// If a delta replaces the entire state, take that replacement as base
		// before applying deltas
		let i: number
		for (i = deltas.length - 1; i >= 0; i--) {
			const delta = deltas[i]
			if (delta.path.length === 0 && delta.op === "replace") {
				base = delta.value
				break
			}
		}
		// If there was a delta that replaced the entire state, start from the
		// delta after that.
		if (i > -1) {
			deltas = deltas.slice(i + 1)
		}

		const applyDeltasImpl = getAddon(AddonDeltas).applyDeltas_
		if (isWrap(base)) {
			// N.B: never hits if some delta a replacement, deltas are never wraps
			return applyDeltasImpl(base, deltas)
		}
		// Otherwise, produce a copy of the base state.
		return this.produce(base, (wrap: Wrapped) =>
			applyDeltasImpl(wrap, deltas)
		)
	}
}

export function makeWrap<T extends Bundleish>(
	rootScope: CellScope,
	value: T,
	parent?: CellState,
	key?: string | number | symbol
): Wrapped<T, CellState> {
	// precondition: makeWrap should be guarded by isWrapable, so we know we can safely wrap
	// returning a tuple here lets us skip a proxy access
	// to WRAP_TAG later
	const [wrap, state] = isBag(value)
		? getAddon(AddonBagSack).wrapBag_(value, parent)
		: isSack(value)
			? getAddon(AddonBagSack).wrapSack_(value, parent)
			: makeWrapWrap(value, parent)

	const scope = parent?.scope_ ?? getScope()
	scope.wraps_.push(wrap)

	// Ensure the parent callbacks are passed down so we actually
	// track all callbacks added throughout the tree
	state.callbacks_ = parent?.callbacks_ ?? []
	state.key_ = key

	if (parent && key !== undefined) {
		registerChildFinalizationCallback(parent, state, key)
	} else {
		// It's a root wrap, register it with the scope
		state.callbacks_.push(function rootWrapCleanup(rootScope) {
			rootScope.mapSetPlugin_?.fixSackContents(state)

			const {deltaPlugin_} = rootScope

			if (state.modified_ && deltaPlugin_) {
				deltaPlugin_.generateDeltas_(state, [], rootScope)
			}
		})
	}

	return wrap as any
}
