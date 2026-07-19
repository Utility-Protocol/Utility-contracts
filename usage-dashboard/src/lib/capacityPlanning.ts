import { CapacityPlan, CapacityPlanningConfig, CapacityPlanningPoint, CapacityRiskLevel, UsageData } from '@/types';

const HOURS_PER_DAY = 24;
const DEFAULT_CONFIG: CapacityPlanningConfig = {
  installedCapacityKWhPerDay: 18,
  reserveMarginPercent: 20,
  planningWindowDays: 30,
  growthSafetyFactorPercent: 10,
};

function round(value: number, digits = 2): number {
  const factor = 10 ** digits;
  return Math.round(value * factor) / factor;
}

function riskFor(utilizationPercent: number, daysUntilCapacityBreach: number | null): CapacityRiskLevel {
  if (utilizationPercent >= 100 || daysUntilCapacityBreach === 0) return 'critical';
  if (utilizationPercent >= 90 || (daysUntilCapacityBreach !== null && daysUntilCapacityBreach <= 7)) return 'high';
  if (utilizationPercent >= 75 || (daysUntilCapacityBreach !== null && daysUntilCapacityBreach <= 30)) return 'medium';
  return 'low';
}

export function buildCapacityPlan(
  usageData: UsageData[],
  config: Partial<CapacityPlanningConfig> = {},
): CapacityPlan {
  const planningConfig = { ...DEFAULT_CONFIG, ...config };
  const chronological = [...usageData].sort(
    (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime(),
  );

  const totalKWh = chronological.reduce((sum, point) => sum + point.kWh, 0);
  const observedHours = Math.max(chronological.length, 1);
  const currentDailyUsageKWh = (totalKWh / observedHours) * HOURS_PER_DAY;

  const midpoint = Math.max(Math.floor(chronological.length / 2), 1);
  const early = chronological.slice(0, midpoint);
  const recent = chronological.slice(midpoint);
  const earlyDaily = (early.reduce((sum, point) => sum + point.kWh, 0) / Math.max(early.length, 1)) * HOURS_PER_DAY;
  const recentDaily = (recent.reduce((sum, point) => sum + point.kWh, 0) / Math.max(recent.length, 1)) * HOURS_PER_DAY;
  const observedDays = observedHours / HOURS_PER_DAY;
  const rawTrend = observedDays > 0 ? ((recentDaily - earlyDaily) / Math.max(earlyDaily, 0.001)) / observedDays : 0;
  const dailyGrowthRate = Math.max(0, rawTrend) * (1 + planningConfig.growthSafetyFactorPercent / 100);

  const usableCapacity = planningConfig.installedCapacityKWhPerDay * (1 - planningConfig.reserveMarginPercent / 100);
  const utilizationPercent = (currentDailyUsageKWh / Math.max(usableCapacity, 0.001)) * 100;

  let daysUntilCapacityBreach: number | null = null;
  if (currentDailyUsageKWh >= usableCapacity) {
    daysUntilCapacityBreach = 0;
  } else if (dailyGrowthRate > 0) {
    daysUntilCapacityBreach = Math.ceil(Math.log(usableCapacity / currentDailyUsageKWh) / Math.log(1 + dailyGrowthRate));
  }

  const projected: CapacityPlanningPoint[] = Array.from({ length: planningConfig.planningWindowDays + 1 }, (_, day) => {
    const projectedKWh = currentDailyUsageKWh * (1 + dailyGrowthRate) ** day;
    const utilization = (projectedKWh / Math.max(usableCapacity, 0.001)) * 100;
    return {
      day,
      projectedKWh: round(projectedKWh),
      utilizationPercent: round(utilization),
      riskLevel: riskFor(utilization, daysUntilCapacityBreach !== null ? Math.max(daysUntilCapacityBreach - day, 0) : null),
    };
  });

  const riskLevel = riskFor(utilizationPercent, daysUntilCapacityBreach);
  const recommendedCapacityKWhPerDay = currentDailyUsageKWh * (1 + dailyGrowthRate) ** planningConfig.planningWindowDays
    / (1 - planningConfig.reserveMarginPercent / 100);

  return {
    currentDailyUsageKWh: round(currentDailyUsageKWh),
    installedCapacityKWhPerDay: planningConfig.installedCapacityKWhPerDay,
    usableCapacityKWhPerDay: round(usableCapacity),
    utilizationPercent: round(utilizationPercent),
    dailyGrowthRatePercent: round(dailyGrowthRate * 100, 2),
    daysUntilCapacityBreach,
    recommendedCapacityKWhPerDay: round(recommendedCapacityKWhPerDay),
    riskLevel,
    projected,
  };
}
