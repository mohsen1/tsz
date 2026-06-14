import type { Brand } from './brands';
import type { Expand, NoInfer } from './tuple-utils';

export type SpaceId = Brand<string, 'SpaceId'>;
export type PhaseName = `phase:${string}`;
export type StepName = `step:${string}`;
export type StepDependency = `dep:${StepName}`;

export type StepContext<TInput> = {
  readonly executionId: string;
  readonly space: SpaceId;
  readonly phase: PhaseName;
  readonly attempt: number;
  readonly input: NoInfer<TInput>;
};

export type StepSuccess<TOutput> = {
  readonly state: 'success';
  readonly output: TOutput;
  readonly notes: readonly string[];
};

export type StepFailure = {
  readonly state: 'failure';
  readonly reason: string;
  readonly notes: readonly string[];
};

export type StepResult<TOutput> = StepSuccess<TOutput> | StepFailure;

export type StepDefinition<
  TInput = any,
  TOutput = any,
  TName extends StepName = StepName,
  TDependencies extends readonly StepDependency[] = readonly StepDependency[],
> = {
  readonly id: string;
  readonly name: TName;
  readonly phase: PhaseName;
  readonly dependsOn: TDependencies;
  readonly run: (
    input: NoInfer<TInput>,
    context: StepContext<TInput>,
  ) => Promise<StepResult<TOutput>>;
};

export type StepsByName<TDefinitions extends readonly StepDefinition[]> = {
  [Step in TDefinitions[number] as Step['name']]: Step;
};

export type StepByName<
  TDefinitions extends readonly StepDefinition[],
  TTarget extends keyof StepsByName<TDefinitions> & string,
> = StepsByName<TDefinitions>[TTarget];

export type StepInput<
  TDefinitions extends readonly StepDefinition[],
  TTarget extends keyof StepsByName<TDefinitions> & string,
> = TTarget extends keyof StepsByName<TDefinitions>
  ? StepByName<TDefinitions, TTarget> extends StepDefinition<infer TInput>
    ? TInput
    : never
  : never;

export type StepOutput<
  TDefinitions extends readonly StepDefinition[],
  TTarget extends keyof StepsByName<TDefinitions> & string,
> = TTarget extends keyof StepsByName<TDefinitions>
  ? StepByName<TDefinitions, TTarget> extends StepDefinition<any, infer TOutput>
    ? TOutput
    : never
  : never;

export type StepView<
  TDefinitions extends readonly StepDefinition[],
  TTarget extends keyof StepsByName<TDefinitions> & string,
> = Expand<{
  readonly name: TTarget;
  readonly input: StepInput<TDefinitions, TTarget>;
  readonly output: StepOutput<TDefinitions, TTarget>;
}>;

type StepRecord = {
  readonly name: StepName;
  readonly index: number;
};

export class StepRegistry<TDefinitions extends readonly StepDefinition[]> {
  readonly #definitions = new Map<StepName, StepDefinition>();
  readonly #records: StepRecord[] = [];

  public constructor(private readonly definitions: TDefinitions) {
    for (const definition of definitions) {
      this.#definitions.set(definition.name, definition);
      this.#records.push({
        name: definition.name,
        index: this.#records.length,
      });
    }
  }

  public names(): readonly StepName[] {
    return this.#records.map((record) => record.name);
  }

  public get<TName extends keyof StepsByName<TDefinitions> & string>(
    name: TName,
  ): StepsByName<TDefinitions>[TName] | undefined {
    return this.#definitions.get(name as StepName) as
      | StepsByName<TDefinitions>[TName]
      | undefined;
  }

  public dependenciesOf<TName extends StepName>(
    name: TName,
  ): readonly StepDependency[] {
    return (this.#definitions.get(name)?.dependsOn ?? []) as readonly StepDependency[];
  }

  public async run<TName extends keyof StepsByName<TDefinitions> & string>(
    name: TName,
    input: StepInput<TDefinitions, TName>,
    space: SpaceId,
  ): Promise<StepResult<StepOutput<TDefinitions, TName>>> {
    const definition = this.get(name);
    if (!definition) {
      throw new Error(`missing step ${name}`);
    }

    const typedDefinition = definition as unknown as StepDefinition<
      StepInput<TDefinitions, TName>,
      StepOutput<TDefinitions, TName>,
      TName,
      readonly StepDependency[]
    >;
    return typedDefinition.run(input, {
      executionId: `exec:${String(name)}`,
      space,
      phase: typedDefinition.phase,
      attempt: 0,
      input,
    });
  }
}

export const createRunOrder = <TDefinitions extends readonly StepDefinition[]>(
  definitions: TDefinitions,
): readonly (keyof StepsByName<TDefinitions> & string)[] => {
  const byName = new Map<StepName, StepDefinition>();
  const indegree = new Map<StepName, number>();
  const outgoing = new Map<StepName, StepName[]>();

  for (const definition of definitions) {
    byName.set(definition.name, definition);
    indegree.set(definition.name, definition.dependsOn.length);
    outgoing.set(definition.name, []);
  }

  for (const definition of definitions) {
    for (const dependency of definition.dependsOn) {
      const normalized = (dependency as string).replace('dep:', '') as StepName;
      outgoing.get(normalized)?.push(definition.name);
    }
  }

  const ready: StepName[] = [...byName.keys()].filter(
    (name) => (indegree.get(name) ?? 0) === 0,
  );
  const ordered: StepName[] = [];

  while (ready.length > 0) {
    const current = ready.shift();
    if (!current) {
      break;
    }
    ordered.push(current);
    for (const next of outgoing.get(current) ?? []) {
      const nextDegree = (indegree.get(next) ?? 0) - 1;
      indegree.set(next, nextDegree);
      if (nextDegree <= 0) {
        ready.push(next);
      }
    }
  }

  return ordered as readonly (keyof StepsByName<TDefinitions> & string)[];
};

export const runAllSteps = async <TDefinitions extends readonly StepDefinition[]>({
  definitions,
  inputByName,
  space,
}: {
  definitions: TDefinitions;
  inputByName: Partial<Record<keyof StepsByName<TDefinitions> & string, unknown>>;
  space: SpaceId;
}): Promise<Record<string, StepResult<unknown>>> => {
  const registry = new StepRegistry(definitions);
  const outputs: Record<string, StepResult<unknown>> = {};

  for (const name of createRunOrder(definitions)) {
    const input = inputByName[name] as never;
    outputs[name] = await registry.run(name, input, space);
  }

  return outputs;
};
