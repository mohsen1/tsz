import {
	CellState,
	Delta,
	Wrapped,
	CellBaseState,
	AnyBag,
	AnySack,
	SlotKind,
	die,
	CellScope,
	ListState
} from "../internal"

export const AddonBagSack = "MapSet"
export const AddonDeltas = "Deltas"
export const AddonListMethods = "ArrayMethods"

export type DeltasAddon = {
	generateDeltas_(
		state: CellState,
		basePath: DeltaPath,
		rootScope: CellScope
	): void
	generateReplacementDeltas_(
		base: any,
		replacement: any,
		rootScope: CellScope
	): void
	applyDeltas_<T>(wrap: T, deltas: readonly Delta[]): T
	getPath: (state: CellState) => DeltaPath | null
}

export type BagSackAddon = {
	wrapBag_<T extends AnyBag>(target: T, parent?: CellState): [T, CellState]
	wrapSack_<T extends AnySack>(target: T, parent?: CellState): [T, CellState]
	fixSackContents: (state: CellState) => void
}

export type ListMethodsAddon = {
	createMethodInterceptor: (state: ListState, method: string) => Function
	isArrayOperationMethod: (method: string) => boolean
	isMutatingArrayMethod: (method: string) => boolean
}

/** Plugin utilities */
const plugins: {
	Deltas?: DeltasAddon
	MapSet?: BagSackAddon
	ArrayMethods?: ListMethodsAddon
} = {}

type Plugins = typeof plugins

export function getAddon<K extends keyof Plugins>(
	pluginKey: K
): Exclude<Plugins[K], undefined> {
	const plugin = plugins[pluginKey]
	if (!plugin) {
		die(0, pluginKey)
	}
	// @ts-ignore
	return plugin
}

export let isAddonLoaded = <K extends keyof Plugins>(pluginKey: K): boolean =>
	!!plugins[pluginKey]

export let clearAddon = <K extends keyof Plugins>(pluginKey: K): void => {
	delete plugins[pluginKey]
}

export function loadAddon<K extends keyof Plugins>(
	pluginKey: K,
	implementation: Plugins[K]
): void {
	if (!plugins[pluginKey]) plugins[pluginKey] = implementation
}
/** Map / Set plugin */

export interface BagState extends CellBaseState {
	type_: SlotKind.Map
	copy_: AnyBag | undefined
	base_: AnyBag
	revoked_: boolean
	wrap_: Wrapped<AnyBag, BagState>
}

export interface SackState extends CellBaseState {
	type_: SlotKind.Set
	copy_: AnySack | undefined
	base_: AnySack
	wraps_: Map<any, Wrapped> // maps the original value to the wrap value in the new set
	revoked_: boolean
	wrap_: Wrapped<AnySack, SackState>
}

/** Deltas plugin */

export type DeltaPath = (string | number)[]
