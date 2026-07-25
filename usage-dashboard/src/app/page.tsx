'use client';

import { useState, useEffect } from 'react';
import {
  Zap,
  DollarSign,
  TrendingUp,
  Activity,
  Shield,
  Send,
  Clock,
  RefreshCw,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  Database
} from 'lucide-react';
import StatsCard from '@/components/StatsCard';
import UsageChart from '@/components/UsageChart';
import MeterInfo from '@/components/MeterInfo';
import { UsageData, MeterData, DashboardStats, CapacityPlan } from '@/types';
import { generateMockUsageData, generateMockMeterData, calculateStats, updateUsageData } from '@/lib/mockData';
import { buildCapacityPlan } from '@/lib/capacityPlanning';
import CapacityPlanningPanel from '@/components/CapacityPlanningPanel';

// Types for Webhook Dashboard
interface WebhookLog {
  id: string;
  url: string;
  event: string;
  timestamp: string;
  attempts: number;
  status: 'SUCCESS' | 'FAILED' | 'RETRYING';
  statusCode?: number;
  errorMessage?: string;
}

export default function Home() {
  const [activeTab, setActiveTab] = useState<'usage' | 'kafka'>('usage');

  // --- Original Usage Tab State ---
  const [usageData, setUsageData] = useState<UsageData[]>([]);
  const [meterData, setMeterData] = useState<MeterData | null>(null);
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [isRealTime, setIsRealTime] = useState(true);
  const [capacityPlan, setCapacityPlan] = useState<CapacityPlan | null>(null);

  // Webhook Monitor State
  const [webhookLogs, setWebhookLogs] = useState<WebhookLog[]>([
    {
      id: 'wh_7g8h9i0j',
      url: 'https://energy-grid.subscriber.com/webhook',
      event: 'low_balance_alert',
      timestamp: new Date(Date.now() - 40000).toLocaleTimeString(),
      attempts: 1,
      status: 'SUCCESS',
      statusCode: 200,
    },
    {
      id: 'wh_1a2b3c4d',
      url: 'https://api.consumer-dapp.org/v1/alerts',
      event: 'low_balance_alert',
      timestamp: new Date(Date.now() - 120000).toLocaleTimeString(),
      attempts: 2,
      status: 'SUCCESS',
      statusCode: 201,
    },
    {
      id: 'wh_5e6f7g8h',
      url: 'http://127.0.0.1:8080/internal/billing',
      event: 'tamper_detected',
      timestamp: new Date(Date.now() - 300000).toLocaleTimeString(),
      attempts: 1,
      status: 'FAILED',
      errorMessage: 'SSRF Prevention: Loopback and localhost destinations are restricted.',
    }
  ]);

  const [webhookStats, setWebhookStats] = useState({
    successCount: 184,
    failureCount: 2,
    totalAttempts: 186,
    avgLatencyMs: 42,
    p99LatencyMs: 84,
    queueSize: 0,
    successRate: 98.92
  });

  // Simulator Form State
  const [simUrl, setSimUrl] = useState('https://energy-grid.subscriber.com/webhook');
  const [simSecret, setSimSecret] = useState('whsec_shared_secret_998');
  const [simEvent, setSimEvent] = useState('low_balance_alert');
  const [simIsAsymmetric, setSimIsAsymmetric] = useState(false);
  const [simSending, setSimSending] = useState(false);

  // Initialize data
  useEffect(() => {
    const initialUsageData = generateMockUsageData();
    const initialMeterData = generateMockMeterData();
    const initialStats = calculateStats(initialUsageData);
    const initialCapacityPlan = buildCapacityPlan(initialUsageData);
    
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
        setCapacityPlan(buildCapacityPlan(newData));
        return newData;
      });
    }, 5000);

    return () => clearInterval(interval);
  }, [isRealTime]);

  // Handle mock webhook simulation
  const handleSimulateWebhook = (e: React.FormEvent) => {
    e.preventDefault();
    if (!simUrl) return;

    setSimSending(true);

    setTimeout(() => {
      // 1. SSRF check
      const lowerUrl = simUrl.toLowerCase();
      let isSsrf = false;
      let ssrfReason = '';

      if (
        lowerUrl.includes('localhost') ||
        lowerUrl.includes('127.0.0.1') ||
        lowerUrl.includes('0.0.0.0') ||
        lowerUrl.includes('169.254') ||
        lowerUrl.includes('10.') ||
        lowerUrl.includes('192.168.')
      ) {
        isSsrf = true;
        ssrfReason = 'SSRF Prevention: Private IP spaces and loopback addresses are restricted.';
      }

      const newId = 'wh_' + Math.random().toString(36).substring(2, 10);
      const newLog: WebhookLog = {
        id: newId,
        url: simUrl,
        event: simEvent,
        timestamp: new Date().toLocaleTimeString(),
        attempts: isSsrf ? 1 : 1,
        status: isSsrf ? 'FAILED' : 'SUCCESS',
        statusCode: isSsrf ? undefined : 200,
        errorMessage: isSsrf ? ssrfReason : undefined
      };

      setWebhookLogs(prev => [newLog, ...prev]);

      setWebhookStats(prev => {
        const total = prev.totalAttempts + 1;
        const successes = prev.successCount + (isSsrf ? 0 : 1);
        const failures = prev.failureCount + (isSsrf ? 1 : 0);
        return {
          ...prev,
          totalAttempts: total,
          successCount: successes,
          failureCount: failures,
          successRate: Math.round((successes / total) * 10000) / 100
        };
      });

      setSimSending(false);
    }, 400); // Quick simulation latency
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
              <div className="p-2 bg-indigo-600 rounded-lg">
                <Zap className="w-6 h-6 text-white" />
              </div>
              <div>
                <h1 className="text-xl font-bold text-gray-900">Utility-Protocol</h1>
                <p className="text-sm text-gray-500">Usage & Webhook Dashboard</p>
              </div>
            </div>
            
            {/* Tabs */}
            <div className="flex space-x-1 bg-slate-100 p-1 rounded-xl">
              <button
                onClick={() => setIsRealTime(!isRealTime)}
                className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                  isRealTime 
                    ? 'bg-indigo-600 text-white hover:bg-indigo-700'
                    : 'bg-gray-200 text-gray-700 hover:bg-gray-300'
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

        {/* Webhooks Monitor Dashboard Section */}
        <section className="mt-12 bg-white rounded-2xl border border-gray-200 shadow-sm p-6 sm:p-8">
          <div className="flex flex-col md:flex-row md:items-center justify-between border-b border-gray-100 pb-5 mb-6">
            <div className="flex items-center space-x-3 mb-4 md:mb-0">
              <div className="p-2 bg-indigo-50 text-indigo-600 rounded-lg">
                <Shield className="w-6 h-6" />
              </div>
              <div>
                <h2 className="text-xl font-bold text-gray-900">🛡️ Webhook Delivery & Monitoring</h2>
                <p className="text-sm text-gray-500">Real-time off-chain webhook ingestion, retry schedules, and security logging</p>
              </div>
            </div>
            <div className="flex items-center space-x-2">
              <span className="flex h-2.5 w-2.5 relative">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-green-500"></span>
              </span>
              <span className="text-sm font-medium text-gray-600">SLA Active (&lt;100ms P99)</span>
            </div>
          </div>

          {/* Webhook Metrics Grid */}
          <div className="grid grid-cols-2 md:grid-cols-5 gap-4 mb-8">
            <div className="bg-gray-50 rounded-xl p-4 border border-gray-100 text-center">
              <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Success Rate</p>
              <p className="text-2xl font-black text-indigo-600 mt-1">{webhookStats.successRate}%</p>
            </div>
            <div className="bg-gray-50 rounded-xl p-4 border border-gray-100 text-center">
              <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Average Latency</p>
              <p className="text-2xl font-black text-gray-800 mt-1">{webhookStats.avgLatencyMs} ms</p>
            </div>
            <div className="bg-gray-50 rounded-xl p-4 border border-gray-100 text-center">
              <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider">P99 Latency</p>
              <p className="text-2xl font-black text-gray-800 mt-1">{webhookStats.p99LatencyMs} ms</p>
            </div>
            <div className="bg-gray-50 rounded-xl p-4 border border-gray-100 text-center">
              <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Queue Size</p>
              <p className="text-2xl font-black text-gray-800 mt-1">{webhookStats.queueSize}</p>
            </div>
            <div className="bg-gray-50 rounded-xl p-4 border border-gray-100 text-center col-span-2 md:col-span-1">
              <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Total Dispatched</p>
              <p className="text-2xl font-black text-gray-800 mt-1">{webhookStats.totalAttempts}</p>
            </div>
          </div>

          <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
            {/* Interactive Webhook Simulator */}
            <div className="lg:col-span-1 bg-gray-50 border border-gray-200 rounded-xl p-5">
              <h3 className="text-md font-bold text-gray-900 mb-4 flex items-center gap-2">
                <Send className="w-4 h-4 text-indigo-600" />
                Dispatch Simulator
              </h3>

              <form onSubmit={handleSimulateWebhook} className="space-y-4">
                <div>
                  <label className="block text-xs font-bold text-gray-600 uppercase mb-1">Target Endpoint URL</label>
                  <input
                    type="text"
                    value={simUrl}
                    onChange={(e) => setSimUrl(e.target.value)}
                    className="w-full text-sm border border-gray-300 rounded-lg px-3 py-2 text-gray-800 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                    placeholder="https://api.subscriber.com/webhook"
                    required
                  />
                  <p className="text-[10px] text-gray-500 mt-1">Try private IP/localhost to test SSRF shields!</p>
                </div>

                <div>
                  <label className="block text-xs font-bold text-gray-600 uppercase mb-1">Shared Secret (HMAC)</label>
                  <input
                    type="password"
                    value={simSecret}
                    onChange={(e) => setSimSecret(e.target.value)}
                    className="w-full text-sm border border-gray-300 rounded-lg px-3 py-2 text-gray-800 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                    placeholder="whsec_..."
                    required
                  />
                </div>

                <div>
                  <label className="block text-xs font-bold text-gray-600 uppercase mb-1">Alert Event</label>
                  <select
                    value={simEvent}
                    onChange={(e) => setSimEvent(e.target.value)}
                    className="w-full text-sm border border-gray-300 rounded-lg px-3 py-2 bg-white text-gray-800 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                  >
                    <option value="low_balance_alert">Low Balance Alert (Under 24h Remaining)</option>
                    <option value="tamper_detected">Tamper Detected (Nonce Desync)</option>
                    <option value="stream_disputed">Stream In Dispute (Pause Triggered)</option>
                  </select>
                </div>

                <div className="flex items-center">
                  <input
                    type="checkbox"
                    id="asymmetric"
                    checked={simIsAsymmetric}
                    onChange={(e) => setSimIsAsymmetric(e.target.checked)}
                    className="h-4 w-4 text-indigo-600 focus:ring-indigo-500 border-gray-300 rounded"
                  />
                  <label htmlFor="asymmetric" className="ml-2 text-xs text-gray-700 font-medium">
                    Enable asymmetric Ed25519 signing
                  </label>
                </div>

                <button
                  type="submit"
                  disabled={simSending}
                  className="w-full bg-indigo-600 hover:bg-indigo-700 text-white font-medium py-2 px-4 rounded-lg text-sm transition-colors flex items-center justify-center gap-2 disabled:opacity-50"
                >
                  {simSending ? (
                    <>
                      <RefreshCw className="w-4 h-4 animate-spin" /> Enqueueing...
                    </>
                  ) : (
                    <>
                      <Send className="w-4 h-4" /> Dispatch Webhook
                    </>
                  )}
                </button>
              </form>
            </div>

            {/* Webhook Delivery Logs */}
            <div className="lg:col-span-2 flex flex-col justify-between">
              <div>
                <h3 className="text-md font-bold text-gray-900 mb-4 flex items-center gap-2">
                  <Database className="w-4 h-4 text-indigo-600" />
                  Recent Deliveries
                </h3>

                <div className="overflow-x-auto border border-gray-100 rounded-xl">
                  <table className="min-w-full divide-y divide-gray-200 text-left">
                    <thead className="bg-gray-50 text-[10px] font-bold text-gray-500 uppercase tracking-wider">
                      <tr>
                        <th className="px-4 py-3">Event / Job</th>
                        <th className="px-4 py-3">Endpoint URL</th>
                        <th className="px-4 py-3">Time</th>
                        <th className="px-4 py-3">Status</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-100 text-xs text-gray-700 bg-white">
                      {webhookLogs.map((log) => (
                        <tr key={log.id} className="hover:bg-gray-50">
                          <td className="px-4 py-3">
                            <span className="font-semibold block text-gray-900">{log.event}</span>
                            <span className="text-[10px] text-gray-400 font-mono">{log.id}</span>
                          </td>
                          <td className="px-4 py-3 max-w-[160px] truncate" title={log.url}>
                            {log.url}
                          </td>
                          <td className="px-4 py-3 text-gray-500">{log.timestamp}</td>
                          <td className="px-4 py-3">
                            <div className="flex flex-col">
                              <span className={`inline-flex items-center gap-1 font-semibold ${
                                log.status === 'SUCCESS'
                                  ? 'text-green-600'
                                  : log.status === 'RETRYING'
                                    ? 'text-yellow-600'
                                    : 'text-red-600'
                              }`}>
                                {log.status === 'SUCCESS' && <CheckCircle2 className="w-3.5 h-3.5" />}
                                {log.status === 'RETRYING' && <RefreshCw className="w-3.5 h-3.5 animate-spin" />}
                                {log.status === 'FAILED' && <XCircle className="w-3.5 h-3.5" />}
                                {log.status}
                              </span>
                              {log.statusCode && (
                                <span className="text-[10px] text-gray-400">HTTP {log.statusCode}</span>
                              )}
                              {log.errorMessage && (
                                <span className="text-[10px] text-red-500 mt-0.5" title={log.errorMessage}>
                                  {log.errorMessage.substring(0, 40)}...
                                </span>
                              )}
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          </div>
        </section>

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
              <div className="flex items-center justify-between">
                <span className="text-sm text-gray-600">Trace P99 Latency</span>
                <span className={`px-2 py-1 text-xs font-medium rounded-full ${
                  stats.traceP99LatencyMs <= 100
                    ? 'bg-green-100 text-green-800'
                    : 'bg-yellow-100 text-yellow-800'
                }`}>
                  {stats.traceP99LatencyMs} ms
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm text-gray-600">Invalid Trace Contexts</span>
                <span className="px-2 py-1 bg-green-100 text-green-800 text-xs font-medium rounded-full">
                  {stats.invalidTraceContexts}
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm text-gray-600">OTel Exporter</span>
                <span className={`px-2 py-1 text-xs font-medium rounded-full ${
                  stats.otelExporterHealthy
                    ? 'bg-green-100 text-green-800'
                    : 'bg-yellow-100 text-yellow-800'
                }`}>
                  {stats.otelExporterHealthy ? 'Healthy' : 'Degraded'}
                </span>
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
