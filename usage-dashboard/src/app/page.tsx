'use client';

import { useState, useEffect } from 'react';
import { Zap, DollarSign, TrendingUp, Activity } from 'lucide-react';
import StatsCard from '@/components/StatsCard';
import UsageChart from '@/components/UsageChart';
import MeterInfo from '@/components/MeterInfo';
import { UsageData, MeterData, DashboardStats, CapacityPlan } from '@/types';
import { generateMockUsageData, generateMockMeterData, calculateStats, updateUsageData } from '@/lib/mockData';
import { buildCapacityPlan } from '@/lib/capacityPlanning';
import CapacityPlanningPanel from '@/components/CapacityPlanningPanel';

export default function Home() {
  const [usageData, setUsageData] = useState<UsageData[]>([]);
  const [meterData, setMeterData] = useState<MeterData | null>(null);
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [isRealTime, setIsRealTime] = useState(true);
  const [capacityPlan, setCapacityPlan] = useState<CapacityPlan | null>(null);

  // Initialize data
  useEffect(() => {
    const initialUsageData = generateMockUsageData();
    const initialMeterData = generateMockMeterData();
    const initialStats = calculateStats(initialUsageData);
    const initialCapacityPlan = buildCapacityPlan(initialUsageData);
    
    setUsageData(initialUsageData);
    setMeterData(initialMeterData);
    setStats(initialStats);
    setCapacityPlan(initialCapacityPlan);
  }, []);

  // Real-time updates
  useEffect(() => {
    if (!isRealTime) return;

    const interval = setInterval(() => {
      setUsageData(prevData => {
        const newData = updateUsageData(prevData);
        setStats(calculateStats(newData));
        setCapacityPlan(buildCapacityPlan(newData));
        return newData;
      });
    }, 5000); // Update every 5 seconds

    return () => clearInterval(interval);
  }, [isRealTime]);

  if (!stats || !meterData || !capacityPlan) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-blue-50 to-indigo-100 flex items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary-600 mx-auto"></div>
          <p className="mt-4 text-gray-600">Loading dashboard...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gradient-to-br from-blue-50 to-indigo-100">
      {/* Header */}
      <header className="bg-white shadow-sm border-b border-gray-200">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-between h-16">
            <div className="flex items-center space-x-3">
              <div className="p-2 bg-primary-600 rounded-lg">
                <Zap className="w-6 h-6 text-white" />
              </div>
              <div>
                <h1 className="text-xl font-bold text-gray-900">Utility-Protocol</h1>
                <p className="text-sm text-gray-500">Usage Dashboard</p>
              </div>
            </div>
            
            <div className="flex items-center space-x-4">
              <div className={`px-3 py-1 rounded-full text-sm font-medium ${
                stats.isPeakHour 
                  ? 'bg-red-100 text-red-800' 
                  : 'bg-green-100 text-green-800'
              }`}>
                {stats.isPeakHour ? '🔴 Peak Hours' : '🟢 Off-Peak'}
              </div>
              
              <button
                onClick={() => setIsRealTime(!isRealTime)}
                className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                  isRealTime 
                    ? 'bg-primary-600 text-white hover:bg-primary-700' 
                    : 'bg-gray-200 text-gray-700 hover:bg-gray-300'
                }`}
              >
                {isRealTime ? '🔴 Live' : '⏸️ Paused'}
              </button>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Stats Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
          <StatsCard
            title="24h Usage"
            value={stats.totalKWh.toString()}
            unit="kWh"
            change={12.5}
            icon={Zap}
            trend="up"
          />
          
          <StatsCard
            title="24h Cost"
            value={stats.totalXLM.toString()}
            unit="XLM"
            change={8.2}
            icon={DollarSign}
            trend="up"
          />
          
          <StatsCard
            title="Current Rate"
            value={stats.currentRate.toString()}
            unit="XLM/kWh"
            icon={TrendingUp}
            isHighlighted={stats.isPeakHour}
          />
          
          <StatsCard
            title="Daily Average"
            value={stats.averageDailyUsage.toString()}
            unit="kWh"
            change={-2.1}
            icon={Activity}
            trend="down"
          />
        </div>

        {/* Charts and Info */}
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
          {/* Usage Chart - Takes 2 columns */}
          <div className="lg:col-span-2">
            <UsageChart data={usageData} />
          </div>
          
          {/* Meter Info - Takes 1 column */}
          <div className="lg:col-span-1">
            <MeterInfo meterData={meterData} />
          </div>
        </div>

        {/* Capacity planning forecast */}
        <div className="mt-8">
          <CapacityPlanningPanel plan={capacityPlan} />
        </div>

        {/* Additional Info Section */}
        <div className="mt-8 grid grid-cols-1 md:grid-cols-2 gap-6">
          <div className="chart-container">
            <h3 className="text-lg font-bold text-gray-900 mb-4">Rate Schedule</h3>
            <div className="space-y-3">
              <div className="flex items-center justify-between p-3 bg-green-50 rounded-lg">
                <div className="flex items-center space-x-3">
                  <div className="w-3 h-3 bg-green-500 rounded-full"></div>
                  <span className="font-medium text-gray-900">Off-Peak Hours</span>
                </div>
                <span className="text-sm text-gray-600">21:00 - 18:00 UTC</span>
              </div>
              <div className="flex items-center justify-between p-3 bg-red-50 rounded-lg">
                <div className="flex items-center space-x-3">
                  <div className="w-3 h-3 bg-red-500 rounded-full"></div>
                  <span className="font-medium text-gray-900">Peak Hours</span>
                </div>
                <span className="text-sm text-gray-600">18:00 - 21:00 UTC</span>
              </div>
            </div>
          </div>
          
          <div className="chart-container">
            <h3 className="text-lg font-bold text-gray-900 mb-4">System Status</h3>
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm text-gray-600">Smart Contract</span>
                <span className="px-2 py-1 bg-green-100 text-green-800 text-xs font-medium rounded-full">
                  Operational
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm text-gray-600">Meter Connection</span>
                <span className="px-2 py-1 bg-green-100 text-green-800 text-xs font-medium rounded-full">
                  Connected
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm text-gray-600">Data Updates</span>
                <span className="px-2 py-1 bg-blue-100 text-blue-800 text-xs font-medium rounded-full">
                  Real-time
                </span>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
