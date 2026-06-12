import { Sigil, normalizeLimit } from './primitives.js';
import type { LedgerProfile, LedgerWindow, SeverityBand } from './shapes';

export interface CadenceBucket {
  readonly band: SeverityBand;
  readonly totalWindows: number;
  readonly activeWindows: number;
  readonly utilization: number;
}

export interface WindowRequest {
  readonly profile: LedgerProfile;
  readonly band: SeverityBand;
  readonly requested: number;
  readonly preferredStart: string;
  readonly preferredEnd: string;
}

export interface WindowAllocation {
  readonly windowId: LedgerWindow['id'];
  readonly profileId: LedgerProfile['ledgerId'];
  readonly band: SeverityBand;
  readonly startAt: string;
  readonly endAt: string;
  readonly score: number;
}

const toNumber = (value: string): number => {
  const parsed = Number.parseInt(value.replace(/[^0-9]/g, ''), 10);
  return Number.isFinite(parsed) ? parsed : 0;
};

export const estimateWindowUtilization = (window: LedgerWindow, request: WindowRequest): number => {
  const start = toNumber(window.startsAt);
  const end = toNumber(window.endsAt);
  const prefStart = toNumber(request.preferredStart);
  const prefEnd = toNumber(request.preferredEnd);
  const span = Math.max(1, end - start);
  const overlapStart = Math.max(start, prefStart);
  const overlapEnd = Math.min(end, prefEnd);
  const overlap = Math.max(0, overlapEnd - overlapStart);
  const ratio = overlap / span;
  const bandMultiplier = request.band === 'critical' ? 1.2 : request.band === 'high' ? 1 : 0.8;
  return normalizeLimit(Math.round(ratio * 100 * bandMultiplier));
};

export const allocateWindow = (request: WindowRequest): WindowAllocation | null => {
  const candidates = request.profile.windowsByBand[request.band] ?? [];
  if (candidates.length === 0) {
    return null;
  }

  const requestedWindows = candidates.slice(0, Math.max(1, Math.min(request.requested, candidates.length)));
  const chosen = requestedWindows[0];
  const window: LedgerWindow = {
    id: String(chosen) as LedgerWindow['id'],
    regionId: request.profile.regionId,
    startsAt: request.preferredStart,
    endsAt: request.preferredEnd,
    openForAllBands: false,
    allowedBands: ['low', 'medium', 'high', 'critical'],
  };

  const score = estimateWindowUtilization(window, request);
  return {
    windowId: window.id,
    profileId: request.profile.ledgerId,
    band: request.band,
    startAt: window.startsAt,
    endAt: window.endsAt,
    score,
  };
};

export const bucketByBand = (profiles: readonly LedgerProfile[], bands: readonly SeverityBand[]): CadenceBucket[] => {
  const map: Record<SeverityBand, LedgerProfile[]> = {
    low: [],
    medium: [],
    high: [],
    critical: [],
  };

  for (const profile of profiles) {
    for (const band of bands) {
      const windows = profile.windowsByBand[band] ?? [];
      map[band] = map[band].concat(Array(windows.length).fill(profile));
    }
  }

  return Object.entries(map).map(([band, profileList]) => {
    const total = profileList.length;
    const activeWindows = profileList.filter((profile) => profile.state === 'active').length;
    const utilization = total > 0 ? normalizeLimit((activeWindows / total) * 100) : 0;
    return {
      band: band as SeverityBand,
      totalWindows: total,
      activeWindows,
      utilization,
    };
  });
};

export const buildCadence = (profiles: readonly LedgerProfile[]): readonly WindowAllocation[] => {
  const bands: SeverityBand[] = ['low', 'medium', 'high', 'critical'];
  const allocations: WindowAllocation[] = [];

  for (const profile of profiles) {
    for (const band of bands) {
      const ids = profile.windowsByBand[band];
      const request: WindowRequest = {
        profile,
        band,
        requested: Math.max(1, Math.min(3, ids.length)),
        preferredStart: new Date().toISOString(),
        preferredEnd: new Date(Date.now() + 60 * 60 * 1000).toISOString(),
      };
      const allocation = allocateWindow(request);
      if (allocation) {
        allocations.push(allocation);
      }
    }
  }

  return allocations;
};
