import {
	CellScope,
	WRAP_TAG,
	isWrapable,
	NOTHING,
	DeltaPath,
	each,
	freeze,
	CellState,
	isWrap,
	SackState,
	set,
	SlotKind,
	getAddon,
	die,
	revokeScope,
	isFrozen,
	get,
	Delta,
	latest,
	prepareCopy,
	getFinalValue,
	getValue,
	ListState
} from "../internal"

export function processResult(result: any, scope: CellScope) {
	scope.unfinalizedWraps_ = scope.wraps_.length
	const baseWrap = scope.wraps_![0]
	const isReplaced = result !== undefined && result !== baseWrap

	if (isReplaced) {
		if (baseWrap[WRAP_TAG].modified_) {
			revokeScope(scope)
			die(4)
		}
		if (isWrapable(result)) {
			// Finalize the result in case it contains (or is) a subset of the wrap.
			result = finalize(scope, result)
		}
		const {deltaPlugin_} = scope
		if (deltaPlugin_) {
			deltaPlugin_.generateReplacementDeltas_(
				baseWrap[WRAP_TAG].base_,
				result,
				scope
			)
		}
	} else {
		// Finalize the base wrap.
		result = finalize(scope, baseWrap)
	}

	maybeFreeze(scope, result, true)

	revokeScope(scope)
	if (scope.deltas_) {
		scope.deltaListener_!(scope.deltas_, scope.inverseDeltas_!)
	}
	return result !== NOTHING ? result : undefined
}

function finalize(rootScope: CellScope, value: any) {
	// Don't recurse in tho recursive data structures
	if (isFrozen(value)) return value

	const state: CellState = value[WRAP_TAG]
	if (!state) {
		const finalValue = handleValue(value, rootScope.handledSet_, rootScope)
		return finalValue
	}

	// Never finalize wraps owned by another scope
	if (!isSameScope(state, rootScope)) {
		return value
	}

	// Unmodified wrap, return the (frozen) original
	if (!state.modified_) {
		return state.base_
	}

	if (!state.finalized_) {
		// Execute all registered wrap finalization callbacks
		const {callbacks_} = state
		if (callbacks_) {
			while (callbacks_.length > 0) {
				const callback = callbacks_.pop()!
				callback(rootScope)
			}
		}

		generateDeltasAndFinalize(state, rootScope)
	}

	// By now the root copy has been fully updated throughout its tree
	return state.copy_
}

function maybeFreeze(scope: CellScope, value: any, deep = false) {
	// we never freeze for a non-root scope; as it would prevent pruning for wraps inside wrapping objects
	if (!scope.parent_ && scope.forge_.autoFreeze_ && scope.canAutoFreeze_) {
		freeze(value, deep)
	}
}

function markStateFinalized(state: CellState) {
	state.finalized_ = true
	state.scope_.unfinalizedWraps_--
}

let isSameScope = (state: CellState, rootScope: CellScope) =>
	state.scope_ === rootScope

// A reusable empty array to avoid allocations
const EMPTY_LOCATIONS_RESULT: (string | symbol | number)[] = []

// Updates all references to a wrap in its parent to the finalized value.
// This handles cases where the same wrap appears multiple times in the parent, or has been moved around.
export function updateWrapInParent(
	parent: CellState,
	wrapValue: any,
	finalizedValue: any,
	originalKey?: string | number | symbol
): void {
	const parentCopy = latest(parent)
	const parentType = parent.type_

	// Fast path: Check if wrap is still at original key
	if (originalKey !== undefined) {
		const currentValue = get(parentCopy, originalKey, parentType)
		if (currentValue === wrapValue) {
			// Still at original location, just update it
			set(parentCopy, originalKey, finalizedValue, parentType)
			return
		}
	}

	// Slow path: Build reverse mapping of all children
	// to their indices in the parent, so that we can
	// replace all locations where this wrap appears.
	// We only have to build this once per parent.
	if (!parent.wrapLocations_) {
		const wrapLocations = (parent.wrapLocations_ = new Map())

		// Use `each` which works on Arrays, Maps, and Objects
		each(parentCopy, (key, value) => {
			if (isWrap(value)) {
				const keys = wrapLocations.get(value) || []
				keys.push(key)
				wrapLocations.set(value, keys)
			}
		})
	}

	// Look up all locations where this wrap appears
	const locations =
		parent.wrapLocations_.get(wrapValue) ?? EMPTY_LOCATIONS_RESULT

	// Update all locations
	for (const location of locations) {
		set(parentCopy, location, finalizedValue, parentType)
	}
}

// Register a callback to finalize a child wrap when the parent wrap is finalized.
// This assumes there is a parent -> child relationship between the two wraps,
// and we have a key to locate the child in the parent.
export function registerChildFinalizationCallback(
	parent: CellState,
	child: CellState,
	key: string | number | symbol
) {
	parent.callbacks_.push(function childCleanup(rootScope) {
		const state: CellState = child

		// Can only continue if this is a wrap owned by this scope
		if (!state || !isSameScope(state, rootScope)) {
			return
		}

		// Handle potential set value finalization first
		rootScope.mapSetPlugin_?.fixSackContents(state)

		const finalizedValue = getFinalValue(state)

		// Update all locations in the parent that referenced this wrap
		updateWrapInParent(parent, state.wrap_ ?? state, finalizedValue, key)

		generateDeltasAndFinalize(state, rootScope)
	})
}

function generateDeltasAndFinalize(state: CellState, rootScope: CellScope) {
	const shouldFinalize =
		state.modified_ &&
		!state.finalized_ &&
		(state.type_ === SlotKind.Set ||
			(state.type_ === SlotKind.Array &&
				(state as ListState).allIndicesReassigned_) ||
			(state.assigned_?.size ?? 0) > 0)

	if (shouldFinalize) {
		const {deltaPlugin_} = rootScope
		if (deltaPlugin_) {
			const basePath = deltaPlugin_!.getPath(state)

			if (basePath) {
				deltaPlugin_!.generateDeltas_(state, basePath, rootScope)
			}
		}

		markStateFinalized(state)
	}
}

export function handleCrossReference(
	target: CellState,
	key: string | number | symbol,
	value: any
) {
	const {scope_} = target
	// Check if value is a wrap from this scope
	if (isWrap(value)) {
		const state: CellState = value[WRAP_TAG]
		if (isSameScope(state, scope_)) {
			// Register callback to update this location when the wrap finalizes

			state.callbacks_.push(function crossReferenceCleanup() {
				// Update the target location with finalized value
				prepareCopy(target)

				const finalizedValue = getFinalValue(state)

				updateWrapInParent(target, value, finalizedValue, key)
			})
		}
	} else if (isWrapable(value)) {
		// Handle non-wrap objects that might contain wraps
		target.callbacks_.push(function nestedWrapCleanup() {
			const targetCopy = latest(target)

			// For Sets, check if value is still in the set
			if (target.type_ === SlotKind.Set) {
				if (targetCopy.has(value)) {
					// Process the value to replace any nested wraps
					handleValue(value, scope_.handledSet_, scope_)
				}
			} else {
				// Maps/objects
				if (get(targetCopy, key, target.type_) === value) {
					if (
						scope_.wraps_.length > 1 &&
						((target as Exclude<CellState, SackState>).assigned_!.get(key) ??
							false) === true &&
						target.copy_
					) {
						// This might be a non-wrap value that has wraps
						// inside. We do need to recurse here to handle those.
						handleValue(
							get(target.copy_, key, target.type_),
							scope_.handledSet_,
							scope_
						)
					}
				}
			}
		})
	}
}

export function handleValue(
	target: any,
	handledSet: Set<any>,
	rootScope: CellScope
) {
	if (!rootScope.forge_.autoFreeze_ && rootScope.unfinalizedWraps_ < 1) {
		// optimization: if an object is not a wrap, and we don't have to
		// deepfreeze everything, and we are sure that no wraps are left in the remaining object
		// cause we saw and finalized all wraps already; we can stop visiting the rest of the tree.
		// This benefits especially adding large data tree's without further processing.
		// See add-data.js perf test
		return target
	}

	// Skip if already handled, frozen, or not wrapable
	if (
		isWrap(target) ||
		handledSet.has(target) ||
		!isWrapable(target) ||
		isFrozen(target)
	) {
		return target
	}

	handledSet.add(target)

	// Process ALL properties/entries
	each(target, (key, value) => {
		if (isWrap(value)) {
			const state: CellState = value[WRAP_TAG]
			if (isSameScope(state, rootScope)) {
				// Replace wrap with finalized value

				const updatedValue = getFinalValue(state)

				set(target, key, updatedValue, target.type_)

				markStateFinalized(state)
			}
		} else if (isWrapable(value)) {
			// Recursively handle nested values
			handleValue(value, handledSet, rootScope)
		}
	})

	return target
}
