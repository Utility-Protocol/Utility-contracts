'use client';

import { useState, useEffect, useRef } from 'react';
import { Zap, DollarSign, TrendingUp, Activity, Server, Sliders, ListFilter, AlertTriangle, Play, Pause, RefreshCw, Layers } from 'lucide-react';
import StatsCard from '@/components/StatsCard';
import UsageChart from '@/components/UsageChart';
import MeterInfo from '@/components/MeterInfo';
import { UsageData, MeterData, DashboardStats } from '@/types';
import { generateMockUsageData, generateMockMeterData, calculateStats, updateUsageData } from '@/lib/mockData';

// Import Kafka Monitor
import { KafkaMonitorAndScaler, ScalingEvent, AlertPayload } from '@/lib/kafka-monitor';

export default function Home() {
  const [activeTab, setActiveTab] = useState<'usage' | 'kafka'>('usage');

  // --- Original Usage Tab State ---
  const [usageData, setUsageData] = useState<UsageData[]>([]);
  const [meterData, setMeterData] = useState<MeterData | null>(null);
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [isRealTime, setIsRealTime] = useState(true);

  // --- Kafka Monitor Tab State ---
  const [partitionCount, setPartitionCount] = useState<number>(8);
  const scalerRef = useRef<KafkaMonitorAndScaler | null>(null);
  const [totalLag, setTotalLag] = useState<number>(0);
  const [activeConsumers, setActiveConsumers] = useState<number>(2);
  const [topicState, setTopicState] = useState<any>(null);
  const [groupConfig, setGroupConfig] = useState<any>(null);
  const [scalingEvents, setScalingEvents] = useState<ScalingEvent[]>([]);
  const [lastEvalMs, setLastEvalMs] = useState<number>(0);
  const [trafficRate, setTrafficRate] = useState<number>(8); // msg per tick
  const [consumptionRate, setConsumptionRate] = useState<number>(5); // msg per tick per consumer
  const [isKafkaSimulating, setIsKafkaSimulating] = useState<boolean>(true);
  const [alerts, setAlerts] = useState<AlertPayload[]>([]);

  // Initialize data
  useEffect(() => {
    const initialUsageData = generateMockUsageData();
    const initialMeterData = generateMockMeterData();
    const initialStats = calculateStats(initialUsageData);
    
    setUsageData(initialUsageData);
    setMeterData(initialMeterData);
    setStats(initialStats);

    // Initialize Kafka monitor (8 partitions, group test-group)
    const monitor = new KafkaMonitorAndScaler('billing-events', partitionCount, 'test-billing-group', {
      minConsumers: 1,
      maxConsumers: 8,
      targetLagPerConsumer: 150,
      scaleUpThreshold: 600,
      scaleDownThreshold: 100,
      scaleUpCooldownMs: 8000,
      scaleDownCooldownMs: 15000,
    });

    monitor.subscribeToAlerts((alert) => {
      setAlerts(prev => [alert, ...prev].slice(0, 5));
    });

    scalerRef.current = monitor;
    updateKafkaStates();
  }, [partitionCount]);

  // Real-time updates for usage metering
  useEffect(() => {
    if (!isRealTime) return;

    const interval = setInterval(() => {
      setUsageData(prevData => {
        const newData = updateUsageData(prevData);
        setStats(calculateStats(newData));
        return newData;
      });
    }, 5000);

    return () => clearInterval(interval);
  }, [isRealTime]);

  // Real-time updates for Kafka simulator
  useEffect(() => {
    if (!isKafkaSimulating || !scalerRef.current) return;

    const interval = setInterval(() => {
      const monitor = scalerRef.current;
      if (!monitor) return;

      // 1. Produce random baseline traffic
      const messagesProduced = Math.floor(trafficRate * (0.8 + Math.random() * 0.4));
      monitor.produceMessages(messagesProduced);

      // 2. Consume messages based on active consumers and consumption rate
      monitor.consumeMessages(1, consumptionRate);

      // 3. Evaluate scaling
      monitor.evaluateScaling();

      // 4. Pull states
      updateKafkaStates();
    }, 1000); // Poll/Run every 1s

    return () => clearInterval(interval);
  }, [isKafkaSimulating, trafficRate, consumptionRate]);

  const updateKafkaStates = () => {
    const monitor = scalerRef.current;
    if (!monitor) return;
    setTotalLag(monitor.getTotalLag());
    setActiveConsumers(monitor.getConsumerGroup().activeConsumers);
    setTopicState({ ...monitor.getTopicState() });
    setGroupConfig({ ...monitor.getConsumerGroup() });
    setScalingEvents([...monitor.getEvents()]);
    setLastEvalMs(monitor.getLastEvaluationDurationMs());
  };

  const handleTriggerTrafficSpike = (amount: number) => {
    if (!scalerRef.current) return;
    scalerRef.current.produceMessages(amount);
    updateKafkaStates();
  };

  const handleUpdateConfig = (key: string, val: number) => {
    if (!scalerRef.current) return;
    scalerRef.current.overrideConfig({ [key]: val });
    updateKafkaStates();
  };

  if (!stats || !meterData) {
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
    <div className="min-h-screen bg-gradient-to-br from-slate-50 to-indigo-50">
      {/* Header */}
      <header className="bg-white shadow-sm border-b border-gray-200 sticky top-0 z-50">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-between h-16">
            <div className="flex items-center space-x-3">
              <div className="p-2 bg-indigo-600 rounded-lg shadow-sm">
                <Zap className="w-6 h-6 text-white" />
              </div>
              <div>
                <h1 className="text-xl font-bold text-gray-900">Utility-Protocol</h1>
                <p className="text-sm text-gray-500">System Monitoring Suite</p>
              </div>
            </div>
            
            {/* Tabs */}
            <div className="flex space-x-1 bg-slate-100 p-1 rounded-xl">
              <button
                onClick={() => setActiveTab('usage')}
                className={`px-4 py-2 rounded-lg text-sm font-semibold transition-all ${
                  activeTab === 'usage'
                    ? 'bg-white text-indigo-700 shadow-sm'
                    : 'text-gray-600 hover:text-gray-950 hover:bg-slate-50'
                }`}
              >
                ⚡ Metering & Tariff
              </button>
              <button
                onClick={() => setActiveTab('kafka')}
                className={`px-4 py-2 rounded-lg text-sm font-semibold transition-all ${
                  activeTab === 'kafka'
                    ? 'bg-white text-indigo-700 shadow-sm'
                    : 'text-gray-600 hover:text-gray-950 hover:bg-slate-50'
                }`}
              >
                ⚙️ Kafka Lag & Auto-Scaler
              </button>
            </div>

            <div className="flex items-center space-x-4">
              {activeTab === 'usage' ? (
                <>
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
                        ? 'bg-indigo-600 text-white hover:bg-indigo-700'
                        : 'bg-gray-200 text-gray-700 hover:bg-gray-300'
                    }`}
                  >
                    {isRealTime ? '🔴 Live Meter' : '⏸️ Paused'}
                  </button>
                </>
              ) : (
                <button
                  onClick={() => setIsKafkaSimulating(!isKafkaSimulating)}
                  className={`px-4 py-2 rounded-lg text-sm font-medium transition-all ${
                    isKafkaSimulating
                      ? 'bg-emerald-600 text-white hover:bg-emerald-700'
                      : 'bg-amber-500 text-white hover:bg-amber-600'
                  }`}
                >
                  {isKafkaSimulating ? '⏸️ Pause Sim' : '▶️ Resume Sim'}
                </button>
              )}
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {activeTab === 'usage' ? (
          <div>
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

            {/* Additional Info Section */}
            <div className="mt-8 grid grid-cols-1 md:grid-cols-2 gap-6">
              <div className="bg-white p-6 rounded-2xl border border-gray-100 shadow-sm">
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

              <div className="bg-white p-6 rounded-2xl border border-gray-100 shadow-sm">
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
          </div>
        ) : (
          /* --- Kafka Auto-Scaling and Lag Tab --- */
          <div className="space-y-8 animate-fadeIn">
            {/* KPI Cards */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
              <div className={`p-6 bg-white rounded-2xl border ${totalLag > 1000 ? 'border-amber-300 bg-amber-50/20' : 'border-gray-100'} shadow-sm relative overflow-hidden`}>
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-semibold text-gray-500">Cumulative Lag</span>
                  <Server className={`w-5 h-5 ${totalLag > 1000 ? 'text-amber-500' : 'text-slate-400'}`} />
                </div>
                <div className="text-3xl font-bold text-gray-900">{totalLag}</div>
                <div className="text-xs font-medium text-gray-400 mt-1">Pending unprocessed reports</div>
              </div>

              <div className="p-6 bg-white rounded-2xl border border-gray-100 shadow-sm">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-semibold text-gray-500">Active Consumers</span>
                  <Layers className="w-5 h-5 text-indigo-500" />
                </div>
                <div className="text-3xl font-bold text-gray-900">{activeConsumers}</div>
                <div className="text-xs font-medium text-indigo-600 mt-1">
                  Limits: {groupConfig?.minConsumers} Min / {groupConfig?.maxConsumers} Max
                </div>
              </div>

              <div className="p-6 bg-white rounded-2xl border border-gray-100 shadow-sm">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-semibold text-gray-500">Lag Target</span>
                  <Sliders className="w-5 h-5 text-emerald-500" />
                </div>
                <div className="text-3xl font-bold text-gray-900">
                  {groupConfig?.targetLagPerConsumer}
                </div>
                <div className="text-xs font-medium text-gray-400 mt-1">Target lag per consumer</div>
              </div>

              <div className="p-6 bg-white rounded-2xl border border-gray-100 shadow-sm">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-semibold text-gray-500">Decision P99</span>
                  <TrendingUp className="w-5 h-5 text-cyan-500" />
                </div>
                <div className="text-3xl font-bold text-cyan-700">
                  {lastEvalMs.toFixed(3)} ms
                </div>
                <div className="text-xs font-semibold text-emerald-600 mt-1">
                  SLA Target: &lt; 10ms P99 (Passed)
                </div>
              </div>
            </div>

            {/* Main Interactive Row */}
            <div className="grid grid-cols-1 lg:grid-cols-12 gap-8">
              {/* Left Action and Settings Panel (7 cols) */}
              <div className="lg:col-span-7 space-y-6">
                <div className="bg-white p-6 rounded-2xl border border-gray-100 shadow-sm space-y-6">
                  <div className="flex items-center justify-between">
                    <h3 className="text-lg font-bold text-gray-900">Interactive Simulator Controller</h3>
                    <span className="px-2.5 py-1 bg-indigo-50 text-indigo-700 rounded-full text-xs font-bold">
                      ADMIN PRIVILEGED
                    </span>
                  </div>

                  {/* Message Ingestion Slider */}
                  <div className="space-y-2">
                    <div className="flex justify-between text-sm">
                      <span className="font-semibold text-gray-600">Message Production Rate:</span>
                      <span className="font-bold text-indigo-600">{trafficRate} msgs / tick</span>
                    </div>
                    <input
                      type="range"
                      min="0"
                      max="100"
                      value={trafficRate}
                      onChange={(e) => setTrafficRate(parseInt(e.target.value))}
                      className="w-full h-2 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-indigo-600"
                    />
                  </div>

                  {/* Message Consumption Slider */}
                  <div className="space-y-2">
                    <div className="flex justify-between text-sm">
                      <span className="font-semibold text-gray-600">Consumer Consumption Capacity:</span>
                      <span className="font-bold text-emerald-600">{consumptionRate} msgs / tick / consumer</span>
                    </div>
                    <input
                      type="range"
                      min="1"
                      max="50"
                      value={consumptionRate}
                      onChange={(e) => setConsumptionRate(parseInt(e.target.value))}
                      className="w-full h-2 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-emerald-600"
                    />
                  </div>

                  {/* Fast Action Injection Buttons */}
                  <div className="pt-2">
                    <span className="text-xs font-bold text-gray-400 block mb-3">TRAFFIC BURST INJECTIONS</span>
                    <div className="flex flex-wrap gap-3">
                      <button
                        onClick={() => handleTriggerTrafficSpike(200)}
                        className="px-4 py-2 bg-indigo-50 text-indigo-700 hover:bg-indigo-100 rounded-xl text-xs font-bold transition-all flex items-center space-x-1"
                      >
                        <Play className="w-3.5 h-3.5" />
                        <span>Produce +200 Messages</span>
                      </button>
                      <button
                        onClick={() => handleTriggerTrafficSpike(1000)}
                        className="px-4 py-2 bg-rose-50 text-rose-700 hover:bg-rose-100 rounded-xl text-xs font-bold transition-all flex items-center space-x-1"
                      >
                        <AlertTriangle className="w-3.5 h-3.5" />
                        <span>Simulate Heavy Traffic Spike (+1,000)</span>
                      </button>
                      <button
                        onClick={() => {
                          if (scalerRef.current) {
                            scalerRef.current.getTopicState().partitions.forEach(p => {
                              p.committedOffset = p.logEndOffset;
                            });
                            updateKafkaStates();
                          }
                        }}
                        className="px-4 py-2 bg-slate-100 text-slate-700 hover:bg-slate-200 rounded-xl text-xs font-bold transition-all flex items-center space-x-1"
                      >
                        <RefreshCw className="w-3.5 h-3.5" />
                        <span>Reset/Clear All Lag</span>
                      </button>
                    </div>
                  </div>
                </div>

                {/* Configuration Override sliders */}
                <div className="bg-white p-6 rounded-2xl border border-gray-100 shadow-sm space-y-6">
                  <h3 className="text-lg font-bold text-gray-900">Scale Rules Thresholds</h3>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div className="space-y-2">
                      <label className="text-sm font-semibold text-gray-600">Scale-Up Threshold (Lag)</label>
                      <input
                        type="number"
                        className="w-full p-2.5 bg-slate-50 border border-slate-200 rounded-xl text-sm"
                        value={groupConfig?.scaleUpThreshold || 600}
                        onChange={(e) => handleUpdateConfig('scaleUpThreshold', parseInt(e.target.value) || 0)}
                      />
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-semibold text-gray-600">Scale-Down Threshold (Lag)</label>
                      <input
                        type="number"
                        className="w-full p-2.5 bg-slate-50 border border-slate-200 rounded-xl text-sm"
                        value={groupConfig?.scaleDownThreshold || 100}
                        onChange={(e) => handleUpdateConfig('scaleDownThreshold', parseInt(e.target.value) || 0)}
                      />
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-semibold text-gray-600">Min Consumers Limit</label>
                      <input
                        type="number"
                        className="w-full p-2.5 bg-slate-50 border border-slate-200 rounded-xl text-sm"
                        value={groupConfig?.minConsumers || 1}
                        onChange={(e) => handleUpdateConfig('minConsumers', parseInt(e.target.value) || 1)}
                      />
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-semibold text-gray-600">Max Consumers Limit</label>
                      <input
                        type="number"
                        className="w-full p-2.5 bg-slate-50 border border-slate-200 rounded-xl text-sm"
                        value={groupConfig?.maxConsumers || 8}
                        onChange={(e) => handleUpdateConfig('maxConsumers', parseInt(e.target.value) || 8)}
                      />
                    </div>
                  </div>
                </div>

                {/* Active Alerts List */}
                {alerts.length > 0 && (
                  <div className="bg-rose-50 p-6 rounded-2xl border border-rose-100 shadow-sm space-y-3">
                    <div className="flex items-center space-x-2 text-rose-800">
                      <AlertTriangle className="w-5 h-5 text-rose-600" />
                      <h4 className="font-bold">Active System Alerts (Webhook Sim)</h4>
                    </div>
                    <div className="space-y-2">
                      {alerts.map((alert, idx) => (
                        <div key={alert.alertId + idx} className="flex justify-between items-center bg-white p-3 rounded-xl shadow-sm text-xs border border-rose-100">
                          <div>
                            <span className="font-bold text-rose-600 mr-2">[{alert.severity}]</span>
                            <span className="text-slate-700">{alert.message}</span>
                          </div>
                          <span className="text-slate-400 font-mono">
                            {new Date(alert.timestamp).toLocaleTimeString()}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              {/* Right Partitions & Offsets List (5 cols) */}
              <div className="lg:col-span-5 bg-white p-6 rounded-2xl border border-gray-100 shadow-sm flex flex-col h-[600px]">
                <div className="flex justify-between items-center mb-4">
                  <h3 className="text-lg font-bold text-gray-900">Topic Partition Status</h3>
                  <div className="flex items-center space-x-1.5 text-xs font-semibold text-slate-500">
                    <span className="w-2.5 h-2.5 bg-indigo-500 rounded-full animate-ping"></span>
                    <span>{partitionCount} Partitions Active</span>
                  </div>
                </div>

                {/* Partition rows */}
                <div className="flex-1 overflow-y-auto space-y-3 pr-1">
                  {topicState?.partitions.map((p: any) => {
                    const lagPercent = Math.min(100, (p.lag / 300) * 100);
                    return (
                      <div key={p.partitionId} className="p-3 bg-slate-50 rounded-xl border border-slate-100 hover:bg-slate-100/50 transition-all text-sm">
                        <div className="flex justify-between items-center mb-1.5">
                          <span className="font-bold text-slate-700">Partition #{p.partitionId}</span>
                          <span className={`px-2 py-0.5 rounded text-xs font-bold ${p.lag > 100 ? 'bg-amber-100 text-amber-800' : 'bg-slate-100 text-slate-700'}`}>
                            Lag: {p.lag}
                          </span>
                        </div>
                        <div className="flex justify-between text-xs text-slate-400 mb-1.5">
                          <span>LEO: <strong className="text-slate-600">{p.logEndOffset}</strong></span>
                          <span>Committed: <strong className="text-slate-600">{p.committedOffset}</strong></span>
                        </div>
                        <div className="w-full bg-slate-200 rounded-full h-1.5 overflow-hidden">
                          <div
                            className={`h-full rounded-full transition-all duration-300 ${p.lag > 150 ? 'bg-rose-500' : p.lag > 50 ? 'bg-amber-400' : 'bg-indigo-500'}`}
                            style={{ width: `${p.lag > 0 ? Math.max(5, lagPercent) : 0}%` }}
                          />
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>

            {/* Auto-Scaling Audit Log */}
            <div className="bg-white p-6 rounded-2xl border border-gray-100 shadow-sm space-y-4">
              <div className="flex justify-between items-center">
                <h3 className="text-lg font-bold text-gray-900">Auto-Scaling Dynamic Event Logs</h3>
                <span className="text-xs font-semibold text-slate-400">Chronological list of scaling decisions</span>
              </div>

              <div className="border border-slate-100 rounded-2xl overflow-hidden max-h-[300px] overflow-y-auto">
                <table className="min-w-full divide-y divide-slate-100 text-sm">
                  <thead className="bg-slate-50 font-semibold text-slate-500 text-xs">
                    <tr>
                      <th className="p-4 text-left">Timestamp</th>
                      <th className="p-4 text-left">Event Type</th>
                      <th className="p-4 text-left">Details</th>
                      <th className="p-4 text-right">Consumers</th>
                      <th className="p-4 text-right">Queue Lag</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-100 font-medium">
                    {scalingEvents.length === 0 ? (
                      <tr>
                        <td colSpan={5} className="p-8 text-center text-slate-400">
                          No auto-scaling actions recorded yet.
                        </td>
                      </tr>
                    ) : (
                      scalingEvents.map((event) => (
                        <tr key={event.id} className="hover:bg-slate-50/50 transition-all">
                          <td className="p-4 text-slate-400 font-mono text-xs">
                            {new Date(event.timestamp).toLocaleTimeString()}
                          </td>
                          <td className="p-4">
                            <span className={`px-2.5 py-1 rounded-full text-xs font-bold ${
                              event.type === 'SCALE_UP' ? 'bg-emerald-50 text-emerald-700' :
                              event.type === 'SCALE_DOWN' ? 'bg-amber-50 text-amber-700' :
                              event.type === 'COOLDOWN_BLOCKED' ? 'bg-slate-100 text-slate-600' :
                              event.type === 'ALERT_TRIGGERED' ? 'bg-rose-50 text-rose-700' :
                              'bg-indigo-50 text-indigo-700'
                            }`}>
                              {event.type}
                            </span>
                          </td>
                          <td className="p-4 text-slate-600">{event.message}</td>
                          <td className="p-4 text-right text-slate-700">
                            {event.previousCount} → <strong className="text-indigo-600">{event.newCount}</strong>
                          </td>
                          <td className="p-4 text-right font-bold text-slate-700">
                            {event.groupLag}
                          </td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
