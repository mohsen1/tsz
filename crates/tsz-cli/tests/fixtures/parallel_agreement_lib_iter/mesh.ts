import {
  asNodeId,
  asPlanId,
  asVoyageId,
  type ManeuverEnvelope,
  type ManeuverEnvelopeInput,
  type ManeuverPlan,
  type ManeuverBeacon,
  type ManeuverBeaconId,
  type ManeuverSummary,
  type ManeuverTopology,
  buildSummary,
} from './types';

export interface ManeuverNode {
  readonly id: string;
  readonly label: string;
  readonly phase: string;
  readonly tags: readonly string[];
}

export interface ManeuverArc {
  readonly from: string;
  readonly to: string;
  readonly label: string;
  readonly weight: number;
}

export interface ManeuverGraph {
  readonly nodes: readonly ManeuverNode[];
  readonly arcs: readonly ManeuverArc[];
  readonly adjacency: ReadonlyMap<string, readonly string[]>;
  readonly topology: ManeuverTopology;
  readonly metadata: {
    readonly routeDigest: string;
    readonly createdAt: string;
    readonly tags: readonly string[];
  };
}

export interface GraphDiagnostics {
  readonly cycleCount: number;
  readonly isolatedCount: number;
  readonly maxOutDegree: number;
  readonly fingerprint: string;
  readonly nodeCount: number;
}

const routePlan = ['discover::0', 'shape::1', 'simulate::2', 'validate::3', 'recommend::4', 'execute::5', 'verify::6', 'close::7'];

const phaseFromLabel = (label: string): string => label.split('::')[0] ?? 'discover';

const beaconPriority = (beacon: ManeuverBeacon): number =>
  beacon.tier === 'critical' ? 0 : beacon.tier === 'warning' ? 1 : beacon.tier === 'beacon' ? 2 : 3;

const buildNodes = (envelope: ManeuverEnvelopeInput): readonly ManeuverNode[] => {
  const beaconPath = [...envelope.beacons]
    .toSorted((left, right) => beaconPriority(left) - beaconPriority(right))
    .map((beacon) => `${beacon.tier}:${beacon.namespace}`);

  const labels = [...routePlan, ...beaconPath];

  return labels.map((entry, index) => ({
    id: String(asNodeId(`${String(envelope.voyageId)}:${index}`)),
    label: entry,
    phase: phaseFromLabel(entry),
    tags: [
      `index:${index}`,
      `phase:${phaseFromLabel(entry)}`,
      `voyage:${envelope.voyageId}`,
      `windows:${envelope.windows.length}`,
    ],
  }));
};

export const buildManeuverGraph = (
  envelope: ManeuverEnvelopeInput,
  topology: ManeuverTopology = envelope.topology,
): ManeuverGraph => {
  const nodes = buildNodes(envelope);
  const arcs: ManeuverArc[] = [];
  const adjacency = new Map<string, readonly string[]>();

  for (let index = 0; index < nodes.length - 1; index += 1) {
    const left = nodes[index];
    const right = nodes[index + 1];
    if (!left || !right) {
      continue;
    }

    const arc: ManeuverArc = {
      from: left.id,
      to: right.id,
      label: `${left.label}->${right.label}`,
      weight: Math.max(1, (left.label.length + right.label.length) % 5),
    };

    arcs.push(arc);
    adjacency.set(left.id, [...(adjacency.get(left.id) ?? []), right.id]);
  }

  for (const node of nodes) {
    if (!adjacency.has(node.id)) {
      adjacency.set(node.id, []);
    }
  }

  return {
    nodes,
    arcs,
    adjacency,
    topology,
    metadata: {
      routeDigest: `${topology}::${envelope.voyageId}::${nodes.length}`,
      createdAt: new Date().toISOString(),
      tags: ['adaptive', topology, `nodes:${nodes.length}`, `beacons:${envelope.beacons.length}`],
    },
  };
};

export const createManeuverGraph = buildManeuverGraph;

export const buildGraphDiagnostics = (graph: ManeuverGraph): GraphDiagnostics => {
  let maxOut = 0;
  for (const next of graph.adjacency.values()) {
    maxOut = Math.max(maxOut, next.length);
  }

  return {
    cycleCount: Math.max(0, graph.arcs.length - graph.nodes.length + 1),
    isolatedCount: [...graph.adjacency.values()].filter((next) => next.length === 0).length,
    maxOutDegree: maxOut,
    fingerprint: `${graph.metadata.routeDigest}::${graph.nodes.length}:${graph.arcs.length}`,
    nodeCount: graph.nodes.length,
  };
};

export const summarizeGraph = (graph: ManeuverGraph, summary: ManeuverSummary): {
  readonly routeDigest: string;
  readonly beaconDensity: number;
  readonly riskBand: string;
  readonly nodeCount: number;
  readonly arcCount: number;
  readonly nodes: readonly string[];
} => {
  return {
    routeDigest: graph.metadata.routeDigest,
    beaconDensity: Number((summary.beaconCount / Math.max(graph.nodes.length, 1)).toFixed(3)),
    riskBand: summary.health,
    nodeCount: graph.nodes.length,
    arcCount: graph.arcs.length,
    nodes: graph.nodes.map((node) => node.id),
  };
};

export const normalizeBeaconPath = <TBeacon extends ManeuverBeacon>(
  beacon: TBeacon,
): `${TBeacon['tier']}::${TBeacon['id']}` => `${beacon.tier}::${beacon.id}`;

export const buildPlanGraph = (plan: ManeuverPlan): {
  readonly plan: ManeuverPlan;
  readonly nodes: readonly ManeuverNode[];
  readonly arcs: readonly ManeuverArc[];
} => {
  const seedBeacon: ManeuverBeacon = {
    id: asVoyageId(plan.id) as unknown as ManeuverBeaconId,
    namespace: 'seed',
    tier: 'beacon',
    title: `${plan.title}:seed`,
    score: plan.steps.length,
    confidence: 0.91,
    tags: [{ key: 'origin', value: 'seed' }],
  };
  const input: ManeuverEnvelopeInput = {
    voyageId: plan.voyageId,
    plan,
    beacons: [seedBeacon],
    windows: [],
    topology: 'grid',
    metadata: {
      source: 'plan',
      plan: plan.id,
    },
  };

  const graph = buildManeuverGraph(input, 'grid');
  return {
    plan,
    nodes: graph.nodes,
    arcs: graph.arcs,
  };
};

export const expandSummary = (summaries: readonly ManeuverSummary[]): {
  readonly planFingerprint: string;
  readonly averageRisk: number;
  readonly totalBeacons: number;
} => {
  const totalBeacons = summaries.reduce((count, summary) => count + summary.beaconCount, 0);
  const averageRisk = summaries.reduce((count, summary) => count + summary.riskIndex, 0) / Math.max(summaries.length, 1);
  const planFingerprint = summaries
    .toSorted((left, right) => left.voyageId.localeCompare(right.voyageId))
    .map((summary) => summary.voyageId)
    .join('|');

  return {
    planFingerprint,
    averageRisk: Number(averageRisk.toFixed(3)),
    totalBeacons,
  };
};

export const buildWindowPlan = (input: readonly ManeuverBeacon[]): {
  windows: ManeuverEnvelopeInput['windows'];
  topologies: readonly ManeuverTopology[];
} => {
  const windows = input
    .toSorted((left, right) => right.score - left.score)
    .map((beacon, index) => ({
      id: asVoyageId(`window:${beacon.id}`),
      from: new Date().toISOString(),
      to: new Date(Date.now() + (index + 1) * 60_000).toISOString(),
      timezone: beacon.namespace,
      blackoutMinutes: [index],
    }));

  return {
    windows,
    topologies: windows.length > 2 ? ['mesh', 'ring'] : ['grid'],
  };
};

export const summarizeHealth = (beacons: readonly ManeuverBeacon[]): string => {
  const summary = buildSummary({
    voyageId: asVoyageId('summary'),
    beacons,
  });

  return `${summary.health}:${summary.beaconCount}:${summary.riskIndex.toFixed(2)}`;
};

export const buildGraphFingerprint = (graph: ManeuverGraph): string => graph.metadata.routeDigest;
