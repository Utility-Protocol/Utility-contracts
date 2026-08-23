'use client';

import React from 'react';
import {
  Globe,
  Activity,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Clock,
  RefreshCw,
  ArrowRightLeft,
  Database,
  ShieldCheck,
} from 'lucide-react';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type RegionRole = 'primary' | 'secondary' | 'tertiary';
export type RegionHealth = 'HEALTHY' | 'DEGRADED' | 'CRITICAL' | 'FAILOVER_IN_PROGRESS';

export interface RegionStatus {
  id: string;
  role: RegionRole;
  health: RegionHealth;
  replicationLagSeconds?: number;
  lastProbeTimestamp?: number;
}

export interface ReplicationPath {
  source: string;
  target: string;
  lagSeconds: number;
  withinRPO: boolean;
}

export interface FailoverEvent {
  fromRegion: string;
  toRegion: string;
  timestamp: number;
  success: boolean;
  durationSeconds?: number;
}

export interface DRTestResult {
  scenario: string;
  region: string;
  passed: boolean;
  measuredValue?: string;
  timestamp: number;
}

export interface MultiRegionDRPanelProps {
  regions: RegionStatus[];
  replicationPaths?: ReplicationPath[];
  lastDRTest?: DRTestResult;
  failoverHistory?: FailoverEvent[];
  rpoViolationCount?: number;
  overallHealth?: RegionHealth;
  className?: string;
}

// ---------------------------------------------------------------------------
// Style helpers
// ---------------------------------------------------------------------------

const healthStyles: Record<RegionHealth, string> = {
  HEALTHY: 'bg-green-100 text-green-800 border-green-200',
  DEGRADED: 'bg-yellow-100 text-yellow-800 border-yellow-200',
  CRITICAL: 'bg-red-100 text-red-800 border-red-200 animate-pulse',
  FAILOVER_IN_PROGRESS: 'bg-blue-100 text-blue-800 border-blue-200 animate-pulse',
};

const roleStyles: Record<RegionRole, string> = {
  primary: 'bg-blue-50 text-blue-700 border border-blue-200',
  secondary: 'bg-purple-50 text-purple-700 border border-purple-200',
  tertiary: 'bg-gray-50 text-gray-600 border border-gray-200',
};

const roleBadge: Record<RegionRole, string> = {
  primary: 'PRIMARY',
  secondary: 'SECONDARY',
  tertiary: 'TERTIARY',
};

function HealthIcon({ health }: { health: RegionHealth }) {
  switch (health) {
    case 'HEALTHY':
      return <CheckCircle className="w-4 h-4 text-green-600" />;
    case 'DEGRADED':
      return <AlertTriangle className="w-4 h-4 text-yellow-600" />;
    case 'CRITICAL':
      return <XCircle className="w-4 h-4 text-red-600" />;
    case 'FAILOVER_IN_PROGRESS':
      return <RefreshCw className="w-4 h-4 text-blue-600 animate-spin" />;
  }
}

function formatAge(timestamp?: number): string {
  if (!timestamp) return 'Unknown';
  const ageMs = Date.now() - timestamp;
  const ageSeconds = Math.floor(ageMs / 1000);
  if (ageSeconds < 60) return `${ageSeconds}s ago`;
  const ageMinutes = Math.floor(ageSeconds / 60);
  if (ageMinutes < 60) return `${ageMinutes}m ago`;
  const ageHours = Math.floor(ageMinutes / 60);
  return `${ageHours}h ago`;
}

function formatLag(lagSeconds?: number): string {
  if (lagSeconds === undefined) return '—';
  if (lagSeconds < 1) return '< 1s';
  return `${lagSeconds.toFixed(1)}s`;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function RegionCard({ region }: { region: RegionStatus }) {
  const isCritical = region.health === 'CRITICAL';
  const isFailingOver = region.health === 'FAILOVER_IN_PROGRESS';

  return (
    <div
      className={`rounded-lg border p-4 ${isCritical || isFailingOver ? 'border-red-200' : 'border-gray-200'} bg-white shadow-sm`}
    >
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center space-x-2">
          <Globe className="w-4 h-4 text-gray-500" />
          <span className="text-sm font-semibold text-gray-900">{region.id}</span>
        </div>
        <span className={`px-2 py-0.5 text-xs font-bold rounded uppercase tracking-wider ${roleStyles[region.role]}`}>
          {roleBadge[region.role]}
        </span>
      </div>

      {/* Health badge */}
      <div className="flex items-center space-x-2 mb-3">
        <HealthIcon health={region.health} />
        <span className={`px-2 py-0.5 text-xs font-semibold rounded border ${healthStyles[region.health]}`}>
          {region.health.replace('_', ' ')}
        </span>
      </div>

      {/* Replication lag */}
      {region.replicationLagSeconds !== undefined && (
        <div className="flex items-center justify-between text-xs text-gray-500">
          <span className="flex items-center space-x-1">
            <Database className="w-3 h-3" />
            <span>Replication lag</span>
          </span>
          <span
            className={
              region.replicationLagSeconds > 60
                ? 'text-red-600 font-semibold'
                : region.replicationLagSeconds > 30
                ? 'text-yellow-600 font-semibold'
                : 'text-green-600'
            }
          >
            {formatLag(region.replicationLagSeconds)}
          </span>
        </div>
      )}

      {/* Probe age */}
      <div className="flex items-center justify-between text-xs text-gray-400 mt-1">
        <span className="flex items-center space-x-1">
          <Clock className="w-3 h-3" />
          <span>Last probe</span>
        </span>
        <span>{formatAge(region.lastProbeTimestamp)}</span>
      </div>
    </div>
  );
}

function FailoverEventRow({ event }: { event: FailoverEvent }) {
  return (
    <div className="flex items-center justify-between py-2 border-b border-gray-100 last:border-0 text-sm">
      <div className="flex items-center space-x-2">
        {event.success ? (
          <CheckCircle className="w-4 h-4 text-green-500 flex-shrink-0" />
        ) : (
          <XCircle className="w-4 h-4 text-red-500 flex-shrink-0" />
        )}
        <span className="text-gray-700">
          {event.fromRegion}
          <ArrowRightLeft className="inline w-3 h-3 mx-1 text-gray-400" />
          {event.toRegion}
        </span>
      </div>
      <div className="flex items-center space-x-3 text-xs text-gray-500">
        {event.durationSeconds !== undefined && (
          <span>{event.durationSeconds.toFixed(1)}s</span>
        )}
        <span>{formatAge(event.timestamp)}</span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function MultiRegionDRPanel({
  regions,
  replicationPaths = [],
  lastDRTest,
  failoverHistory = [],
  rpoViolationCount = 0,
  overallHealth = 'HEALTHY',
  className = '',
}: MultiRegionDRPanelProps) {
  const isOverallCritical = overallHealth === 'CRITICAL' || overallHealth === 'FAILOVER_IN_PROGRESS';

  const primaryRegion = regions.find((r) => r.role === 'primary');
  const secondaryRegions = regions.filter((r) => r.role !== 'primary');
  const unhealthyCount = regions.filter((r) => r.health !== 'HEALTHY').length;
  const rpoViolatingPaths = replicationPaths.filter((p) => !p.withinRPO);

  return (
    <div className={`bg-white rounded-xl shadow-md border border-gray-200 p-6 ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between border-b border-gray-100 pb-4 mb-6">
        <div className="flex items-center space-x-3">
          <div
            className={`p-2 rounded-lg ${
              isOverallCritical ? 'bg-red-500 text-white' : 'bg-blue-600 text-white'
            }`}
          >
            {isOverallCritical ? (
              <AlertTriangle className="w-6 h-6" />
            ) : (
              <ShieldCheck className="w-6 h-6" />
            )}
          </div>
          <div>
            <h2 className="text-lg font-bold text-gray-900">Multi-Region DR Status</h2>
            <p className="text-xs text-gray-500">Replication Health and Failover Readiness</p>
          </div>
        </div>

        <div className="flex items-center space-x-3">
          {/* RPO violation indicator */}
          {rpoViolationCount > 0 && (
            <span className="flex items-center space-x-1 px-3 py-1 bg-red-100 text-red-700 text-xs font-semibold rounded-full border border-red-200">
              <AlertTriangle className="w-3 h-3" />
              <span>{rpoViolationCount} RPO violations</span>
            </span>
          )}

          {/* Overall health badge */}
          <span
            className={`px-3 py-1 text-xs font-bold rounded-full border uppercase tracking-wider ${healthStyles[overallHealth]}`}
          >
            {overallHealth.replace('_', ' ')}
          </span>
        </div>
      </div>

      {/* Summary stats */}
      <div className="grid grid-cols-3 gap-4 mb-6">
        <div className="text-center p-3 bg-gray-50 rounded-lg">
          <p className="text-2xl font-bold text-gray-900">{regions.length}</p>
          <p className="text-xs text-gray-500 mt-1">Regions</p>
        </div>
        <div className={`text-center p-3 rounded-lg ${unhealthyCount > 0 ? 'bg-red-50' : 'bg-green-50'}`}>
          <p className={`text-2xl font-bold ${unhealthyCount > 0 ? 'text-red-700' : 'text-green-700'}`}>
            {unhealthyCount}
          </p>
          <p className={`text-xs mt-1 ${unhealthyCount > 0 ? 'text-red-500' : 'text-green-500'}`}>
            Unhealthy regions
          </p>
        </div>
        <div className={`text-center p-3 rounded-lg ${rpoViolatingPaths.length > 0 ? 'bg-orange-50' : 'bg-green-50'}`}>
          <p className={`text-2xl font-bold ${rpoViolatingPaths.length > 0 ? 'text-orange-700' : 'text-green-700'}`}>
            {rpoViolatingPaths.length}
          </p>
          <p className={`text-xs mt-1 ${rpoViolatingPaths.length > 0 ? 'text-orange-500' : 'text-green-500'}`}>
            RPO violations
          </p>
        </div>
      </div>

      {/* Region grid */}
      <div className="mb-6">
        <h3 className="text-sm font-semibold text-gray-700 mb-3 flex items-center space-x-2">
          <Globe className="w-4 h-4" />
          <span>Region Health</span>
        </h3>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {/* Primary first */}
          {primaryRegion && <RegionCard region={primaryRegion} />}
          {secondaryRegions.map((region) => (
            <RegionCard key={region.id} region={region} />
          ))}
        </div>
      </div>

      {/* Replication paths */}
      {replicationPaths.length > 0 && (
        <div className="mb-6">
          <h3 className="text-sm font-semibold text-gray-700 mb-3 flex items-center space-x-2">
            <Database className="w-4 h-4" />
            <span>Replication Lag</span>
          </h3>
          <div className="space-y-2">
            {replicationPaths.map((path) => (
              <div
                key={`${path.source}-${path.target}`}
                className="flex items-center justify-between py-2 px-3 bg-gray-50 rounded-lg text-sm"
              >
                <span className="text-gray-600">
                  {path.source}
                  <ArrowRightLeft className="inline w-3 h-3 mx-2 text-gray-400" />
                  {path.target}
                </span>
                <div className="flex items-center space-x-2">
                  <span
                    className={
                      !path.withinRPO
                        ? 'text-red-600 font-semibold'
                        : path.lagSeconds > 30
                        ? 'text-yellow-600 font-semibold'
                        : 'text-green-600'
                    }
                  >
                    {formatLag(path.lagSeconds)}
                  </span>
                  {path.withinRPO ? (
                    <CheckCircle className="w-4 h-4 text-green-500" />
                  ) : (
                    <AlertTriangle className="w-4 h-4 text-red-500" />
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Last DR test */}
      {lastDRTest && (
        <div className="mb-6">
          <h3 className="text-sm font-semibold text-gray-700 mb-3 flex items-center space-x-2">
            <Activity className="w-4 h-4" />
            <span>Last DR Test</span>
          </h3>
          <div
            className={`flex items-center justify-between p-3 rounded-lg border ${
              lastDRTest.passed
                ? 'bg-green-50 border-green-200'
                : 'bg-red-50 border-red-200'
            }`}
          >
            <div className="flex items-center space-x-3">
              {lastDRTest.passed ? (
                <CheckCircle className="w-5 h-5 text-green-600" />
              ) : (
                <XCircle className="w-5 h-5 text-red-600" />
              )}
              <div>
                <p className={`text-sm font-semibold ${lastDRTest.passed ? 'text-green-800' : 'text-red-800'}`}>
                  {lastDRTest.scenario}
                </p>
                <p className="text-xs text-gray-500">
                  {lastDRTest.region} — {formatAge(lastDRTest.timestamp)}
                </p>
              </div>
            </div>
            {lastDRTest.measuredValue && (
              <span className="text-sm text-gray-600">{lastDRTest.measuredValue}</span>
            )}
          </div>
        </div>
      )}

      {/* Failover history */}
      {failoverHistory.length > 0 && (
        <div>
          <h3 className="text-sm font-semibold text-gray-700 mb-3 flex items-center space-x-2">
            <ArrowRightLeft className="w-4 h-4" />
            <span>Recent Failover Events</span>
          </h3>
          <div className="border border-gray-100 rounded-lg px-3 py-1">
            {failoverHistory.slice(-5).reverse().map((event, idx) => (
              <FailoverEventRow key={idx} event={event} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
