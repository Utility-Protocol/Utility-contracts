export enum SheddingLevel {
  NORMAL = 0,
  MODERATE = 1,
  HIGH = 2,
  CRITICAL = 3,
}

export type FeatureKey =
  | 'HIGH_FREQ_POLLING'
  | 'COMPLEX_FORECAST'
  | 'HEAVY_CHARTS'
  | 'ZK_VERIFICATION'
  | 'POSTPAID_STREAMS';

export interface DegradationState {
  sheddingLevel: SheddingLevel;
  p99LatencyMs: number;
  simulatedLoadPercent: number;
  activeFlags: Record<FeatureKey, boolean>;
  availabilityPercent: number;
  alertMessage: string | null;
  pollingIntervalMs: number;
}

/**
 * Pure, deterministic calculation of system degradation state.
 * Guaranteed to execute under 1ms (target <100ms budget).
 */
export function calculateDegradationState(
  loadPercent: number,
  latencyMs: number,
  manualOverrides: Partial<Record<FeatureKey, boolean>> = {}
): DegradationState {
  const startTime = typeof performance !== 'undefined' ? performance.now() : 0;

  // 1. Determine Shedding Level based on Load & Latency thresholds
  let sheddingLevel = SheddingLevel.NORMAL;

  if (loadPercent >= 95 || latencyMs >= 500) {
    sheddingLevel = SheddingLevel.CRITICAL;
  } else if (loadPercent >= 90 || latencyMs >= 200) {
    sheddingLevel = SheddingLevel.HIGH;
  } else if (loadPercent >= 80 || latencyMs >= 100) {
    sheddingLevel = SheddingLevel.MODERATE;
  }

  // 2. Set default active features based on shedding level
  const activeFlags: Record<FeatureKey, boolean> = {
    HIGH_FREQ_POLLING: sheddingLevel === SheddingLevel.NORMAL,
    COMPLEX_FORECAST: sheddingLevel <= SheddingLevel.MODERATE,
    HEAVY_CHARTS: sheddingLevel <= SheddingLevel.MODERATE,
    ZK_VERIFICATION: sheddingLevel <= SheddingLevel.HIGH,
    POSTPAID_STREAMS: sheddingLevel <= SheddingLevel.MODERATE,
  };

  // 3. Apply manual developer/operator overrides
  for (const key of Object.keys(manualOverrides) as FeatureKey[]) {
    if (manualOverrides[key] !== undefined) {
      activeFlags[key] = manualOverrides[key]!;
    }
  }

  // 4. Calculate dynamic polling interval (Capacity Shedding for reporting rate)
  let pollingIntervalMs = 5000; // Normal: 5 seconds
  if (!activeFlags.HIGH_FREQ_POLLING) {
    if (sheddingLevel === SheddingLevel.MODERATE) {
      pollingIntervalMs = 15000; // Moderate: 15s
    } else if (sheddingLevel === SheddingLevel.HIGH) {
      pollingIntervalMs = 30000; // High: 30s
    } else {
      pollingIntervalMs = 120000; // Critical: 120s (2 mins)
    }
  }

  // 5. Generate appropriate alert messages
  let alertMessage: string | null = null;
  switch (sheddingLevel) {
    case SheddingLevel.MODERATE:
      alertMessage =
        '⚠️ Alert: Moderate load detected. Switched to cached capacity planning forecasts. Polling interval relaxed to 15s.';
      break;
    case SheddingLevel.HIGH:
      alertMessage =
        '⚠️ Warning: High congestion. Disabled heavy charts. Polling interval relaxed to 30s. Restricting postpaid creation.';
      break;
    case SheddingLevel.CRITICAL:
      alertMessage =
        '🚨 Emergency: Critical overload! Bypassing ZK-SNARK proof verifications (optimistic path). Background cleanup suspended. Polling interval is 120s.';
      break;
    default:
      alertMessage = null;
  }

  // 6. Compute Simulated Availability/Uptime percentage (Target: 99.99%)
  // If shedding is active, we preserve 99.99% availability.
  // If user disabled shedding features manually under heavy load, availability drops dramatically!
  let availabilityPercent = 99.99;
  if (loadPercent > 100) {
    const isUnderShedding = !activeFlags.HIGH_FREQ_POLLING && !activeFlags.HEAVY_CHARTS;
    if (!isUnderShedding) {
      // Overloaded system without capacity shedding decays in availability
      const excess = loadPercent - 100;
      availabilityPercent = Math.max(90, 99.99 - excess * 1.5);
    }
  }

  // Latency Safety boundary check
  const executionTime = typeof performance !== 'undefined' ? performance.now() - startTime : 0;
  if (executionTime > 100) {
    console.warn(`Degradation calculation exceeded budget: ${executionTime}ms`);
  }

  return {
    sheddingLevel,
    p99LatencyMs: latencyMs,
    simulatedLoadPercent: loadPercent,
    activeFlags,
    availabilityPercent: Number(availabilityPercent.toFixed(2)),
    alertMessage,
    pollingIntervalMs,
  };
}
