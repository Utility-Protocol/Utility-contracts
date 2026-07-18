'use client';

import { AlertTriangle, BarChart3, CheckCircle2, Gauge, TrendingUp } from 'lucide-react';
import { CapacityPlan } from '@/types';

interface CapacityPlanningPanelProps {
  plan: CapacityPlan;
}

const riskStyles = {
  low: 'bg-green-100 text-green-800 border-green-200',
  medium: 'bg-yellow-100 text-yellow-800 border-yellow-200',
  high: 'bg-orange-100 text-orange-800 border-orange-200',
  critical: 'bg-red-100 text-red-800 border-red-200',
};

export default function CapacityPlanningPanel({ plan }: CapacityPlanningPanelProps) {
  const breachText = plan.daysUntilCapacityBreach === null
    ? 'No breach projected in current trend'
    : plan.daysUntilCapacityBreach === 0
      ? 'Capacity breach now'
      : `${plan.daysUntilCapacityBreach} days until usable capacity breach`;

  const lastProjection = plan.projected[plan.projected.length - 1];

  return (
    <section className="chart-container" aria-labelledby="capacity-planning-title">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between mb-6">
        <div>
          <h2 id="capacity-planning-title" className="text-xl font-bold text-gray-900">
            Capacity Planning Forecast
          </h2>
          <p className="text-sm text-gray-600">
            Historical usage trend with reserve-margin-aware 30 day projection.
          </p>
        </div>
        <span className={`inline-flex items-center rounded-full border px-3 py-1 text-sm font-semibold capitalize ${riskStyles[plan.riskLevel]}`}>
          {plan.riskLevel === 'low' ? <CheckCircle2 className="mr-2 h-4 w-4" /> : <AlertTriangle className="mr-2 h-4 w-4" />}
          {plan.riskLevel} risk
        </span>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-6">
        <div className="rounded-lg bg-blue-50 p-4">
          <Gauge className="h-5 w-5 text-blue-600 mb-2" />
          <p className="text-sm text-gray-600">Current Utilization</p>
          <p className="text-2xl font-bold text-gray-900">{plan.utilizationPercent}%</p>
          <p className="text-xs text-gray-500">of {plan.usableCapacityKWhPerDay} kWh/day usable</p>
        </div>
        <div className="rounded-lg bg-purple-50 p-4">
          <TrendingUp className="h-5 w-5 text-purple-600 mb-2" />
          <p className="text-sm text-gray-600">Daily Trend</p>
          <p className="text-2xl font-bold text-gray-900">{plan.dailyGrowthRatePercent}%</p>
          <p className="text-xs text-gray-500">safety-factor adjusted</p>
        </div>
        <div className="rounded-lg bg-orange-50 p-4">
          <AlertTriangle className="h-5 w-5 text-orange-600 mb-2" />
          <p className="text-sm text-gray-600">Breach Forecast</p>
          <p className="text-lg font-bold text-gray-900">{breachText}</p>
        </div>
        <div className="rounded-lg bg-green-50 p-4">
          <BarChart3 className="h-5 w-5 text-green-600 mb-2" />
          <p className="text-sm text-gray-600">Recommended Capacity</p>
          <p className="text-2xl font-bold text-gray-900">{plan.recommendedCapacityKWhPerDay}</p>
          <p className="text-xs text-gray-500">kWh/day for target window</p>
        </div>
      </div>

      <div className="rounded-lg border border-gray-200 p-4">
        <div className="flex items-center justify-between text-sm text-gray-600 mb-2">
          <span>Today: {plan.currentDailyUsageKWh} kWh/day</span>
          <span>Day {lastProjection.day}: {lastProjection.projectedKWh} kWh/day</span>
        </div>
        <div className="h-3 w-full overflow-hidden rounded-full bg-gray-100">
          <div
            className="h-full rounded-full bg-primary-600 transition-all"
            style={{ width: `${Math.min(lastProjection.utilizationPercent, 100)}%` }}
            aria-label={`${lastProjection.utilizationPercent}% projected utilization`}
          />
        </div>
        <p className="mt-3 text-sm text-gray-600">
          Alert policy: medium at 75% utilization or 30 day breach, high at 90% or 7 day breach,
          critical at 100%. Use blue-green dashboard rollout and canary alerts before changing capacity limits.
        </p>
      </div>
    </section>
  );
}
