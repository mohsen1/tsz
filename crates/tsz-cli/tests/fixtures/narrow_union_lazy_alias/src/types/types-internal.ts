import {
	SackState,
	CellScope,
	DictState,
	ListState,
	BagState,
	WRAP_TAG,
	Delta,
	DeltaPath
} from "../internal"

export type Bundleish = AnyDict | AnyList | AnyBag | AnySack
export type BundleishNoSack = AnyDict | AnyList | AnyBag

export type AnyDict = {[key: string]: any}
export type AnyList = Array<any>
export type AnySack = Set<any>
export type AnyBag = Map<any, any>

export const enum SlotKind {
	Object,
	Array,
	Map,
	Set
}

export interface CellBaseState {
	parent_?: CellState
	scope_: CellScope
	modified_: boolean
	finalized_: boolean
	isManual_: boolean
	assigned_: Map<any, boolean> | undefined
	key_?: string | number | symbol
	callbacks_: ((scope: CellScope) => void)[]
	wrapLocations_?: Map<any, (string | number | symbol)[]>
}

export type CellState =
	| DictState
	| ListState
	| BagState
	| SackState

// The _internal_ type used for wraps (not to be confused with Wrap, which is public facing)
export type Wrapped<Base = any, T extends CellState = CellState> = {
	[WRAP_TAG]: T
} & Base

export type MakeDeltas = (
	state: CellState,
	basePath: DeltaPath,
	deltas: Delta[],
	inverseDeltas: Delta[]
) => void
