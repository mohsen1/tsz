export const nominalTypeHack: unique symbol = Symbol();

type IsOptional<T> =
  undefined | null extends T ? true :
  undefined extends T ? true :
  null extends T ? true :
  false;

type RequiredKeys<V> = {
  [K in keyof V]-?:
    Exclude<V[K], undefined> extends Validator<infer T>
      ? IsOptional<T> extends true ? never : K
      : never;
}[keyof V];

type OptionalKeys<V> = Exclude<keyof V, RequiredKeys<V>>;
type InferPropsInner<V> = { [K in keyof V]-?: InferType<V[K]> };

interface Validator<T> {
  (props: object, propName: string): Error | null;
  [nominalTypeHack]?: T;
}

interface Requireable<T> extends Validator<T> {
  isRequired: Validator<NonNullable<T>>;
}

type ValidationMap<T> = { [K in keyof T]?: Validator<T[K]> };
type InferType<V> = V extends Validator<infer T> ? T : any;
type InferProps<V> =
  & InferPropsInner<Pick<V, RequiredKeys<V>>>
  & Partial<InferPropsInner<Pick<V, OptionalKeys<V>>>>;

declare const PropTypes: {
  any: Requireable<any>;
  array: Requireable<any[]>;
  bool: Requireable<boolean>;
  string: Requireable<string>;
  number: Requireable<number>;
  shape<P extends ValidationMap<any>>(type: P): Requireable<InferProps<P>>;
  oneOfType<T extends Validator<any>>(types: T[]): Requireable<NonNullable<InferType<T>>>;
};

interface Props {
  any?: any;
  array: string[];
  bool: boolean;
  shape: { foo: string; bar?: boolean; baz?: any };
  oneOfType: string | boolean | { foo?: string; bar: number };
}

type PropTypesMap = ValidationMap<Props>;

const innerProps = {
  foo: PropTypes.string.isRequired,
  bar: PropTypes.bool,
  baz: PropTypes.any,
};

const arrayOfTypes = [
  PropTypes.string,
  PropTypes.bool,
  PropTypes.shape({
    foo: PropTypes.string,
    bar: PropTypes.number.isRequired,
  }),
];

const propTypes: PropTypesMap = {
  any: PropTypes.any,
  array: PropTypes.array.isRequired,
  bool: PropTypes.bool.isRequired,
  shape: PropTypes.shape(innerProps).isRequired,
  oneOfType: PropTypes.oneOfType(arrayOfTypes).isRequired,
};

const propTypesWithoutAnnotation = {
  any: PropTypes.any,
  array: PropTypes.array.isRequired,
  bool: PropTypes.bool.isRequired,
  shape: PropTypes.shape(innerProps).isRequired,
  oneOfType: PropTypes.oneOfType(arrayOfTypes).isRequired,
};

type ExtractedProps = InferProps<typeof propTypes>;
type ExtractedPropsWithoutAnnotation = InferProps<typeof propTypesWithoutAnnotation>;
type ExtractPropsMatch =
  ExtractedProps extends ExtractedPropsWithoutAnnotation ? true : false;
const ok: true = null as any as ExtractPropsMatch;
