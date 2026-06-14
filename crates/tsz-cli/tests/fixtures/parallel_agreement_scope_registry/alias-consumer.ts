import type { BrandInfo } from './brands';
import type {
  StepDefinition,
  StepInput,
  StepName,
  StepOutput,
  StepView,
  StepsByName,
} from './scope-registry';
import type { Pairwise } from './tuple-utils';

export type StepAudit<
  TDefinitions extends readonly StepDefinition[],
  TTarget extends keyof StepsByName<TDefinitions> & string,
> = {
  readonly key: `audit:${TTarget}`;
  readonly view: StepView<TDefinitions, TTarget>;
  readonly input: StepInput<TDefinitions, TTarget>;
  readonly output: Awaited<StepOutput<TDefinitions, TTarget>>;
};

export type AuditByName<TDefinitions extends readonly StepDefinition[]> = {
  [Name in keyof StepsByName<TDefinitions> & string as `audit:${Name}`]: StepAudit<
    TDefinitions,
    Name
  >;
};

export type DependencyPairs<
  TNames extends readonly StepName[],
  TDeps extends readonly StepName[],
> = Pairwise<TNames, TDeps>;

export type BrandedAuditMarker<TDefinitions extends readonly StepDefinition[]> =
  BrandInfo<keyof AuditByName<TDefinitions> & string>;

export const toAuditKeys = <TDefinitions extends readonly StepDefinition[]>(
  definitions: TDefinitions,
): readonly (keyof AuditByName<TDefinitions> & string)[] =>
  definitions.map((definition) => `audit:${definition.name}`) as readonly (keyof AuditByName<TDefinitions> & string)[];
