import { UsageData, MeterData, DashboardStats } from '@/types';

// Generate mock usage data for the last 24 hours
export function generateMockUsageData(): UsageData[] {
  const data: UsageData[] = [];
  const now = new Date();
  
  for (let i = 23; i >= 0; i--) {
    const timestamp = new Date(now.getTime() - i * 60 * 60 * 1000);
    const hour = timestamp.getHours();
    const isPeakHour = hour >= 18 && hour < 21;
    
    // Simulate varying usage patterns
    let baseUsage = 0.5 + Math.random() * 0.3;
    if (hour >= 6 && hour < 9) baseUsage *= 1.5; // Morning peak
    if (hour >= 18 && hour < 22) baseUsage *= 1.8; // Evening peak
    if (hour >= 0 && hour < 6) baseUsage *= 0.3; // Night low usage
    
    const rate = isPeakHour ? 15 : 10; // Peak: 15 XLM/kWh, Off-peak: 10 XLM/kWh
    const kWh = Number((baseUsage).toFixed(3));
    const XLM = Number((kWh * rate).toFixed(6));
    
    data.push({
      timestamp: timestamp.toISOString(),
      kWh,
      XLM,
      rate,
      isPeakHour
    });
  }
  
  return data;
}

// Generate mock meter data
export function generateMockMeterData(): MeterData {
  return {
    id: 'meter_001',
    user: 'GDUK... (User Wallet)',
    provider: 'GDST... (Utility Provider)',
    offPeakRate: 10,
    peakRate: 15,
    balance: 1250.75,
    totalUsage: 1247.85,
    totalSpend: 15678.42,
    lastUpdate: new Date().toISOString()
  };
}

// Calculate dashboard statistics
export function calculateStats(usageData: UsageData[]): DashboardStats {
  const totalKWh = usageData.reduce((sum, data) => sum + data.kWh, 0);
  const totalXLM = usageData.reduce((sum, data) => sum + data.XLM, 0);
  const currentHour = new Date().getHours();
  const isPeakHour = currentHour >= 18 && currentHour < 21;
  const currentRate = isPeakHour ? 15 : 10;
  
  // Calculate daily averages (assuming this data represents a typical day)
  const averageDailyUsage = totalKWh;
  const averageDailySpend = totalXLM;
  
  return {
    totalKWh: Number(totalKWh.toFixed(3)),
    totalXLM: Number(totalXLM.toFixed(6)),
    currentRate,
    isPeakHour,
    averageDailyUsage: Number(averageDailyUsage.toFixed(3)),
    averageDailySpend: Number(averageDailySpend.toFixed(6)),
    traceP99LatencyMs: 84,
    invalidTraceContexts: 0,
    otelExporterHealthy: true
  };
}

// Simulate real-time data updates
export function updateUsageData(currentData: UsageData[]): UsageData[] {
  const newData = [...currentData];
  const now = new Date();
  const hour = now.getHours();
  const isPeakHour = hour >= 18 && hour < 21;
  
  // Remove the oldest entry and add a new one
  if (newData.length > 0) {
    newData.shift();
  }
  
  // Generate new data point
  let baseUsage = 0.5 + Math.random() * 0.3;
  if (hour >= 6 && hour < 9) baseUsage *= 1.5;
  if (hour >= 18 && hour < 22) baseUsage *= 1.8;
  if (hour >= 0 && hour < 6) baseUsage *= 0.3;
  
  const rate = isPeakHour ? 15 : 10;
  const kWh = Number((baseUsage).toFixed(3));
  const XLM = Number((kWh * rate).toFixed(6));
  
  newData.push({
    timestamp: now.toISOString(),
    kWh,
    XLM,
    rate,
    isPeakHour
  });
  
  return newData;
}
