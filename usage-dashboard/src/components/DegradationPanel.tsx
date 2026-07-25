'use client';

import React from 'react';
import { Shield, ShieldAlert, Cpu, Activity, Clock, ToggleLeft, ToggleRight, Radio, Server } from 'lucide-react';
import { SheddingLevel, FeatureKey, DegradationState } from '../lib/gracefulDegradation';

interface DegradationPanelProps {
  state: DegradationState;
  onLoadChange: (load: number) => void;
  onLatencyChange: (latency: number) => void;
  overrides: Partial<Record<FeatureKey, boolean>>;
  onToggleOverride: (feature: FeatureKey) => void;
  onClearOverrides: () => void;
}

const levelStyles = {
  [SheddingLevel.NORMAL]: 'bg-green-100 text-green-800 border-green-200',
  [SheddingLevel.MODERATE]: 'bg-yellow-100 text-yellow-800 border-yellow-200',
  [SheddingLevel.HIGH]: 'bg-orange-100 text-orange-800 border-orange-200',
  [SheddingLevel.CRITICAL]: 'bg-red-100 text-red-800 border-red-200 animate-pulse',
};

const featureNames: Record<FeatureKey, string> = {
  HIGH_FREQ_POLLING: 'High Frequency Polling (5s)',
  COMPLEX_FORECAST: 'Complex Capacity Forecasting',
  HEAVY_CHARTS: 'High-Fidelity Render Charts',
  ZK_VERIFICATION: 'On-Chain ZK-SNARK Validation',
  POSTPAID_STREAMS: 'New Postpaid Billing Streams',
};

export default function DegradationPanel({
  state,
  onLoadChange,
  onLatencyChange,
  overrides,
  onToggleOverride,
  onClearOverrides,
}: DegradationPanelProps) {
  const isLevelCritical = state.sheddingLevel === SheddingLevel.CRITICAL;

  return (
    <div className="bg-white rounded-xl shadow-md border border-gray-200 p-6">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-gray-100 pb-4 mb-6">
        <div className="flex items-center space-x-3">
          <div className={`p-2 rounded-lg ${isLevelCritical ? 'bg-red-500 text-white' : 'bg-primary-600 text-white'}`}>
            {isLevelCritical ? <ShieldAlert className="w-6 h-6" /> : <Shield className="w-6 h-6" />}
          </div>
          <div>
            <h2 className="text-lg font-bold text-gray-900">Resilience Control Panel</h2>
            <p className="text-xs text-gray-500">Feature Flags & Dynamic Capacity Shedding</p>
          </div>
        </div>
        <span
          className={`px-3 py-1 text-xs font-bold rounded-full border uppercase tracking-wider ${
            levelStyles[state.sheddingLevel]
          }`}
        >
          {SheddingLevel[state.sheddingLevel]} SHEDDING
        </span>
      </div>

      {/* Simulator Sliders */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-6">
        <div className="bg-gray-50 rounded-lg p-4 border border-gray-100">
          <div className="flex items-center justify-between mb-2">
            <label className="text-sm font-semibold text-gray-700 flex items-center gap-2">
              <Cpu className="w-4 h-4 text-gray-500" />
              Simulated System Load: <span className="font-bold text-primary-600">{state.simulatedLoadPercent}%</span>
            </label>
          </div>
          <input
            type="range"
            min="0"
            max="150"
            value={state.simulatedLoadPercent}
            onChange={(e) => onLoadChange(Number(e.target.value))}
            className="w-full h-2 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-primary-600"
          />
          <div className="flex justify-between text-[10px] text-gray-400 mt-1">
            <span>Idle (0%)</span>
            <span>Threshold (80%)</span>
            <span>Overload (150%)</span>
          </div>
        </div>

        <div className="bg-gray-50 rounded-lg p-4 border border-gray-100">
          <div className="flex items-center justify-between mb-2">
            <label className="text-sm font-semibold text-gray-700 flex items-center gap-2">
              <Clock className="w-4 h-4 text-gray-500" />
              P99 Network Latency: <span className="font-bold text-primary-600">{state.p99LatencyMs}ms</span>
            </label>
          </div>
          <input
            type="range"
            min="10"
            max="600"
            value={state.p99LatencyMs}
            onChange={(e) => onLatencyChange(Number(e.target.value))}
            className="w-full h-2 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-primary-600"
          />
          <div className="flex justify-between text-[10px] text-gray-400 mt-1">
            <span>Fast (10ms)</span>
            <span>Target (100ms)</span>
            <span>Congested (600ms)</span>
          </div>
        </div>
      </div>

      {/* System Indicators */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 mb-6">
        <div className="bg-blue-50/50 rounded-lg p-3 border border-blue-100 text-center">
          <Activity className="w-5 h-5 text-blue-600 mx-auto mb-1" />
          <span className="text-xs text-gray-500 block">SLA P99 Target</span>
          <span className="text-base font-bold text-gray-900">&lt; 100ms</span>
          <span className="text-[10px] text-blue-600 block mt-0.5 font-medium">Actual: {state.p99LatencyMs}ms</span>
        </div>

        <div className="bg-purple-50/50 rounded-lg p-3 border border-purple-100 text-center">
          <Server className="w-5 h-5 text-purple-600 mx-auto mb-1" />
          <span className="text-xs text-gray-500 block">Uptime SLA</span>
          <span className="text-base font-bold text-gray-900">99.99%</span>
          <span className="text-[10px] text-purple-600 block mt-0.5 font-medium">
            Simulated: {state.availabilityPercent}%
          </span>
        </div>

        <div className="bg-yellow-50/50 rounded-lg p-3 border border-yellow-100 text-center">
          <Radio className="w-5 h-5 text-yellow-600 mx-auto mb-1" />
          <span className="text-xs text-gray-500 block">Device Polling</span>
          <span className="text-base font-bold text-gray-900">
            {state.pollingIntervalMs / 1000}s
          </span>
          <span className="text-[10px] text-yellow-600 block mt-0.5 font-medium">
            Shed rate: {state.pollingIntervalMs > 5000 ? 'ACTIVE' : 'NONE'}
          </span>
        </div>
      </div>

      {/* Alert Feed */}
      {state.alertMessage && (
        <div className="mb-6 bg-amber-50 border border-amber-200 text-amber-800 px-4 py-3 rounded-lg text-sm font-medium">
          {state.alertMessage}
        </div>
      )}

      {/* Feature Flags Override Table */}
      <div className="border border-gray-100 rounded-lg overflow-hidden">
        <div className="bg-gray-50 px-4 py-2 border-b border-gray-100 flex items-center justify-between">
          <span className="text-xs font-bold text-gray-700 uppercase">Operational Feature Flags</span>
          <button
            onClick={onClearOverrides}
            disabled={Object.keys(overrides).length === 0}
            className="text-[10px] text-primary-600 hover:text-primary-700 font-bold disabled:text-gray-300"
          >
            Reset Overrides
          </button>
        </div>
        <div className="divide-y divide-gray-50">
          {(Object.keys(state.activeFlags) as FeatureKey[]).map((key) => {
            const isOverridden = overrides[key] !== undefined;
            const isActive = state.activeFlags[key];

            return (
              <div key={key} className="flex items-center justify-between px-4 py-3 bg-white">
                <div>
                  <span className="text-sm font-semibold text-gray-800 block">
                    {featureNames[key]}
                  </span>
                  <span className="text-[10px] text-gray-400">
                    System Key: <code className="bg-gray-100 px-1 py-0.5 rounded text-gray-500">{key}</code>
                  </span>
                </div>
                <div className="flex items-center space-x-3">
                  <span
                    className={`px-2 py-0.5 text-[10px] font-bold rounded-full ${
                      isActive ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'
                    }`}
                  >
                    {isActive ? 'ACTIVE' : 'SHED/DISABLED'}
                  </span>
                  <button
                    onClick={() => onToggleOverride(key)}
                    className="text-gray-500 hover:text-primary-600 focus:outline-none transition-colors"
                  >
                    {isOverridden ? (
                      overrides[key] ? (
                        <ToggleRight className="w-8 h-8 text-primary-600" />
                      ) : (
                        <ToggleLeft className="w-8 h-8 text-gray-300" />
                      )
                    ) : (
                      <span className="text-[10px] font-bold text-gray-400 border border-gray-200 rounded px-2 py-1 hover:border-primary-400">
                        OVERRIDE
                      </span>
                    )}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
