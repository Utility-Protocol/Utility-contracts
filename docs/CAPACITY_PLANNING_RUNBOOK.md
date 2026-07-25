# Capacity Planning with Historical Usage Trending

## Architecture

The usage dashboard now computes capacity forecasts from the same hourly usage samples used by the live dashboard:

1. Normalize the most recent usage samples into an estimated daily kWh load.
2. Compare the first half of the historical window with the most recent half to derive a growth trend.
3. Apply a configurable safety factor and reserve margin before projecting demand for 30 days.
4. Classify capacity risk as low, medium, high, or critical and expose the result in the dashboard.

The implementation is intentionally client-side and deterministic for the mock dashboard so the critical path stays under the 100 ms target. Production wiring should replace the mock samples with the metering API while preserving the pure `buildCapacityPlan` interface for testability.

## Alert Policy

| Risk | Condition | Action |
| --- | --- | --- |
| Low | < 75% usable capacity and no projected breach | Continue normal monitoring. |
| Medium | >= 75% usable capacity or breach within 30 days | Review provider capacity queue and open planning ticket. |
| High | >= 90% usable capacity or breach within 7 days | Page utility operations and prepare expansion/cutover plan. |
| Critical | >= 100% usable capacity | Start incident response and apply emergency throttling if required. |

## Monitoring and Dashboards

Track these fields from `CapacityPlan`:

- `currentDailyUsageKWh`
- `usableCapacityKWhPerDay`
- `utilizationPercent`
- `dailyGrowthRatePercent`
- `daysUntilCapacityBreach`
- `recommendedCapacityKWhPerDay`
- `riskLevel`

Recommended SLO panels:

- P99 dashboard forecast calculation latency, target < 100 ms.
- Capacity risk count by utility provider.
- Canary versus stable forecast delta after deployment.
- Alert delivery success rate, target 99.99% availability.

## Blue-Green and Canary Deployment

1. Deploy the new dashboard build to the green environment with forecast alerts disabled.
2. Mirror production meter samples into green and compare forecasts against blue for one hour.
3. Enable canary alerts for 5% of providers and verify alert volume matches expected risk distribution.
4. Increase to 25%, 50%, and 100% if canary forecast deltas stay below 5%.
5. Keep blue warm for rollback until one full daily cycle completes.

## Security Review Checklist

- Forecasts must use aggregate usage samples only; do not expose raw wallet secrets or meter keys.
- Alert payloads must omit precise customer location and device MAC addresses.
- Dashboard changes must pass dependency audit and standard repository review before production rollout.
- Treat capacity thresholds as configuration managed by operations, not user-provided input.
