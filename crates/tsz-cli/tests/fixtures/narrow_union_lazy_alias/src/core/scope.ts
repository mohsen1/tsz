import {
	Delta,
	DeltaListener,
	Wrapped,
	Forge,
	WRAP_TAG,
	CellState,
	SlotKind,
	getAddon,
	DeltasAddon,
	BagSackAddon,
	isAddonLoaded,
	AddonBagSack,
	AddonDeltas,
	ListMethodsAddon,
	AddonListMethods
} from "../internal"

/** Each scope represents a `produce` call. */

export interface CellScope {
	deltas_?: Delta[]
	inverseDeltas_?: Delta[]
	deltaPlugin_?: DeltasAddon
	mapSetPlugin_?: BagSackAddon
	arrayMethodsPlugin_?: ListMethodsAddon
	canAutoFreeze_: boolean
	wraps_: any[]
	parent_?: CellScope
	deltaListener_?: DeltaListener
	forge_: Forge
	unfinalizedWraps_: number
	handledSet_: Set<any>
	processedForDeltas_: Set<any>
}

let currentScope: CellScope | undefined

export let getScope = () => currentScope!

let createScope = (
	parent_: CellScope | undefined,
	forge_: Forge
): CellScope => ({
	wraps_: [],
	parent_,
	forge_,
	// Whenever the modified wrap contains a wrap from another scope, we
	// need to prevent auto-freezing so the unowned wrap can be finalized.
	canAutoFreeze_: true,
	unfinalizedWraps_: 0,
	handledSet_: new Set(),
	processedForDeltas_: new Set(),
	mapSetPlugin_: isAddonLoaded(AddonBagSack)
		? getAddon(AddonBagSack)
		: undefined,
	arrayMethodsPlugin_: isAddonLoaded(AddonListMethods)
		? getAddon(AddonListMethods)
		: undefined
})

export function useDeltasInScope(
	scope: CellScope,
	deltaListener?: DeltaListener
) {
	if (deltaListener) {
		scope.deltaPlugin_ = getAddon(AddonDeltas) // assert we have the plugin
		scope.deltas_ = []
		scope.inverseDeltas_ = []
		scope.deltaListener_ = deltaListener
	}
}

export function revokeScope(scope: CellScope) {
	leaveScope(scope)
	scope.wraps_.forEach(revokeWrap)
	// @ts-ignore
	scope.wraps_ = null
}

export function leaveScope(scope: CellScope) {
	if (scope === currentScope) {
		currentScope = scope.parent_
	}
}

export let enterScope = (forge: Forge) =>
	(currentScope = createScope(currentScope, forge))

function revokeWrap(wrap: Wrapped) {
	const state: CellState = wrap[WRAP_TAG]
	if (state.type_ === SlotKind.Object || state.type_ === SlotKind.Array)
		state.revoke_()
	else state.revoked_ = true
}
