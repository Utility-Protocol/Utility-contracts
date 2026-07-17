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
  traceP99LatencyMs: number;
  invalidTraceContexts: number;
  otelExporterHealthy: boolean;
}
