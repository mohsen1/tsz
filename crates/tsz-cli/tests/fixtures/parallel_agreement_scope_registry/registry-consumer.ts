import { asBrand } from './brands';
import { toAuditKeys, type AuditByName } from './alias-consumer';
import {
  runAllSteps,
  StepRegistry,
  type StepDefinition,
  type StepDependency,
} from './scope-registry';

type LoadInput = {
  readonly value: number;
  readonly tenant: string;
};

type LoadOutput = {
  readonly normalized: string;
  readonly score: number;
};

type StoreInput = {
  readonly normalized: string;
  readonly score: number;
};

type StoreOutput = {
  readonly stored: true;
  readonly receipt: `receipt:${string}`;
};

type LoadStep = StepDefinition<LoadInput, LoadOutput, 'step:load', readonly []>;
type StoreStep = StepDefinition<
  StoreInput,
  StoreOutput,
  'step:store',
  readonly ['dep:step:load']
>;

const definitions: readonly [LoadStep, StoreStep] = [
  {
    id: 'load',
    name: 'step:load',
    phase: 'phase:prepare',
    dependsOn: [],
    async run(input: LoadInput) {
      return {
        state: 'success',
        output: {
          normalized: input.tenant.toUpperCase(),
          score: input.value + 1,
        },
        notes: ['loaded'],
      };
    },
  },
  {
    id: 'store',
    name: 'step:store',
    phase: 'phase:commit',
    dependsOn: ['dep:step:load'],
    async run(input: StoreInput) {
      return {
        state: 'success',
        output: {
          stored: true,
          receipt: `receipt:${input.normalized}`,
        },
        notes: ['stored'],
      };
    },
  },
];

const dependencyProbe: readonly StepDependency[] = definitions[1].dependsOn;

const registry = new StepRegistry(definitions);
const space = asBrand('space:fixture', 'SpaceId');

export const registryNames = registry.names();

export const auditKeys: readonly (keyof AuditByName<typeof definitions> & string)[] =
  toAuditKeys(definitions);

export const knownDependencies = dependencyProbe;

export const runLoad = () =>
  registry.run('step:load', { value: 41, tenant: 'north' }, space);

export const runStore = () =>
  registry.run('step:store', { normalized: 'NORTH', score: 42 }, space);

export const runEverything = () =>
  runAllSteps({
    definitions,
    space,
    inputByName: {
      'step:load': { value: 1, tenant: 'south' },
      'step:store': { normalized: 'SOUTH', score: 2 },
    },
  });
