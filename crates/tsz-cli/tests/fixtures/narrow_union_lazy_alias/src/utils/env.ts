// Should be no imports here!

/**
 * The sentinel value returned by producers to replace the wrap with undefined.
 */
export const NOTHING: unique symbol = Symbol.for("forge-nothing")

/**
 * To let Forge treat your class instances as plain immutable objects
 * (albeit with a custom prototype), you must define either an instance property
 * or a static property on each of your custom classes.
 *
 * Otherwise, your class instance will never be wraped, which means it won't be
 * safe to mutate in a produce callback.
 */
export const WRAPABLE: unique symbol = Symbol.for("forge-wrapable")

export const WRAP_TAG: unique symbol = Symbol.for("forge-state")
