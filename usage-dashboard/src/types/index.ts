export interface UsageData {
  timestamp: string;
  kWh: number;
  XLM: number;
  rate: number;
  isPeakHour: boolean;
}

export interface MeterData {
  id: string;
  user: string;
  provider: string;
  offPeakRate: number;
  peakRate: number;
  balance: number;
  totalUsage: number;
  totalSpend: number;
  lastUpdate: string;
}

export interface DashboardStats {
  totalKWh: number;
  totalXLM: number;
  currentRate: number;
  isPeakHour: boolean;
  averageDailyUsage: number;
  averageDailySpend: number;
}


export type CapacityRiskLevel = 'low' | 'medium' | 'high' | 'critical';

export interface CapacityPlanningConfig {
  installedCapacityKWhPerDay: number;
  reserveMarginPercent: number;
  planningWindowDays: number;
  growthSafetyFactorPercent: number;
}

export interface CapacityPlanningPoint {
  day: number;
  projectedKWh: number;
  utilizationPercent: number;
  riskLevel: CapacityRiskLevel;
}

export interface CapacityPlan {
  currentDailyUsageKWh: number;
  installedCapacityKWhPerDay: number;
  usableCapacityKWhPerDay: number;
  utilizationPercent: number;
  dailyGrowthRatePercent: number;
  daysUntilCapacityBreach: number | null;
  recommendedCapacityKWhPerDay: number;
  riskLevel: CapacityRiskLevel;
  projected: CapacityPlanningPoint[];
}
