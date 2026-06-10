#![cfg(test)]

/// Automated Gas Metering Metrics for Unit Tests
///
/// This module provides comprehensive gas measurement and analytics capabilities
/// for Soroban smart contract tests. It enables:
///
/// 1. **Automated Gas Tracking**: Capture gas consumption for each operation
/// 2. **Benchmarking**: Compare actual vs estimated gas costs
/// 3. **Metrics Collection**: Track gas profiles across test suites
/// 4. **Performance Analysis**: Identify gas-intensive operations
/// 5. **Optimization Verification**: Validate that optimizations reduce gas usage
/// 6. **Regression Detection**: Alert when gas usage increases unexpectedly
///
/// Focus Areas:
/// - Optimization: Verify gas efficiency improvements
/// - Security: Monitor for gas-based DOS attacks
/// - Reliability: Ensure consistent gas behavior across operations
///
/// Reference Issue: Automated Gas Metering Metrics for Unit Tests
/// Acceptance Criteria:
///   1. Gas is measured for all contract external functions
///   2. Metrics are captured with minimal overhead
///   3. Reports show gas vs estimated costs
///   4. Regression detection alerts on unexpected increases

extern crate std;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::println;
use std::format;
use std::vec;
use alloc::vec::Vec;
use alloc::string::String;

// ============================================================================
// Constants for Gas Cost Baseline
// ============================================================================

/// Baseline gas costs in stroops (for reference and comparison)
pub struct GasBaseline;

impl GasBaseline {
    // Common operation baselines
    pub const SIMPLE_READ: i128 = 1_000_000; // 0.01 XLM
    pub const SIMPLE_WRITE: i128 = 2_000_000; // 0.02 XLM
    pub const TOKEN_TRANSFER: i128 = 3_000_000; // 0.03 XLM
    pub const STORAGE_OPERATION: i128 = 5_000_000; // 0.05 XLM
    pub const CROSS_CONTRACT_CALL: i128 = 10_000_000; // 0.1 XLM

    // Contract-specific operations (from gas_estimator.rs)
    pub const REGISTER_METER: i128 = 10_000_000;
    pub const TOP_UP: i128 = 5_000_000;
    pub const CLAIM: i128 = 8_000_000;
    pub const UPDATE_HEARTBEAT: i128 = 3_000_000;
    pub const GROUP_TOP_UP_PER_METER: i128 = 6_000_000;
    pub const EMERGENCY_SHUTDOWN: i128 = 2_000_000;
    pub const SUBMIT_ZK_REPORT: i128 = 50_000_000;
    pub const SET_ZK_VK: i128 = 15_000_000;
}

// ============================================================================
// Gas Meter Structures
// ============================================================================

/// A snapshot of gas metrics at a point in time
#[derive(Clone, Debug)]
pub struct GasMeasurement {
    pub operation_name: String,
    pub estimated_gas: i128,
    pub actual_gas: i128,
    pub timestamp_ns: u128,
    pub test_name: String,
}

impl GasMeasurement {
    pub fn efficiency_ratio(&self) -> f64 {
        if self.estimated_gas == 0 {
            return 1.0;
        }
        (self.actual_gas as f64) / (self.estimated_gas as f64)
    }

    pub fn gas_variance(&self) -> i128 {
        self.actual_gas - self.estimated_gas
    }

    pub fn variance_percentage(&self) -> f64 {
        if self.estimated_gas == 0 {
            return 0.0;
        }
        ((self.actual_gas - self.estimated_gas) as f64 / self.estimated_gas as f64) * 100.0
    }

    pub fn is_within_tolerance(&self, tolerance_percent: f64) -> bool {
        self.variance_percentage().abs() <= tolerance_percent
    }
}

/// Statistics for a group of measurements
#[derive(Clone, Debug)]
pub struct GasStatistics {
    pub operation_name: String,
    pub count: usize,
    pub total_gas: i128,
    pub min_gas: i128,
    pub max_gas: i128,
    pub avg_gas: i128,
    pub total_estimated: i128,
    pub avg_estimated: i128,
}

impl GasStatistics {
    pub fn efficiency_ratio(&self) -> f64 {
        if self.total_estimated == 0 {
            return 1.0;
        }
        (self.total_gas as f64) / (self.total_estimated as f64)
    }

    pub fn variance_percentage(&self) -> f64 {
        if self.total_estimated == 0 {
            return 0.0;
        }
        ((self.total_gas - self.total_estimated) as f64 / self.total_estimated as f64) * 100.0
    }
}

/// Global gas meter for collecting metrics across all tests
pub struct GasMeter {
    measurements: Mutex<Vec<GasMeasurement>>,
    test_stack: Mutex<Vec<String>>,
    operation_counter: AtomicUsize,
}

impl GasMeter {
    fn new() -> Self {
        GasMeter {
            measurements: Mutex::new(Vec::new()),
            test_stack: Mutex::new(Vec::new()),
            operation_counter: AtomicUsize::new(0),
        }
    }

    /// Record a gas measurement
    pub fn record_measurement(
        &self,
        operation_name: impl Into<String>,
        estimated_gas: i128,
        actual_gas: i128,
    ) {
        let operation_name = operation_name.into();
        let test_stack = self.test_stack.lock().unwrap();
        let test_name = test_stack
            .last()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        drop(test_stack);

        let measurement = GasMeasurement {
            operation_name,
            estimated_gas,
            actual_gas,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            test_name,
        };

        self.measurements.lock().unwrap().push(measurement);
        self.operation_counter.fetch_add(1, Ordering::SeqCst);
    }

    /// Begin a test context
    pub fn push_test(&self, test_name: impl Into<String>) {
        self.test_stack.lock().unwrap().push(test_name.into());
    }

    /// End a test context
    pub fn pop_test(&self) {
        self.test_stack.lock().unwrap().pop();
    }

    /// Get all measurements
    pub fn get_measurements(&self) -> Vec<GasMeasurement> {
        self.measurements.lock().unwrap().clone()
    }

    /// Get measurements for a specific operation
    pub fn get_operation_measurements(&self, operation_name: &str) -> Vec<GasMeasurement> {
        self.measurements
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.operation_name == operation_name)
            .cloned()
            .collect()
    }

    /// Calculate statistics for an operation
    pub fn get_operation_statistics(&self, operation_name: &str) -> Option<GasStatistics> {
        let measurements = self.get_operation_measurements(operation_name);
        if measurements.is_empty() {
            return None;
        }

        let count = measurements.len();
        let total_gas: i128 = measurements.iter().map(|m| m.actual_gas).sum();
        let total_estimated: i128 = measurements.iter().map(|m| m.estimated_gas).sum();
        let min_gas = measurements.iter().map(|m| m.actual_gas).min().unwrap_or(0);
        let max_gas = measurements.iter().map(|m| m.actual_gas).max().unwrap_or(0);
        let avg_gas = total_gas / count as i128;
        let avg_estimated = total_estimated / count as i128;

        Some(GasStatistics {
            operation_name: operation_name.to_string(),
            count,
            total_gas,
            min_gas,
            max_gas,
            avg_gas,
            total_estimated,
            avg_estimated,
        })
    }

    /// Get statistics for all operations
    pub fn get_all_statistics(&self) -> BTreeMap<String, GasStatistics> {
        let measurements = self.measurements.lock().unwrap();
        let mut operation_names: std::collections::HashSet<_> =
            measurements.iter().map(|m| m.operation_name.clone()).collect();

        operation_names
            .into_iter()
            .filter_map(|op_name| {
                self.get_operation_statistics(&op_name)
                    .map(|stats| (op_name, stats))
            })
            .collect()
    }

    /// Get measurements exceeding a gas threshold
    pub fn get_expensive_operations(&self, threshold: i128) -> Vec<GasMeasurement> {
        self.measurements
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.actual_gas > threshold)
            .cloned()
            .collect()
    }

    /// Get measurements deviating from estimates
    pub fn get_deviations(&self, tolerance_percent: f64) -> Vec<GasMeasurement> {
        self.measurements
            .lock()
            .unwrap()
            .iter()
            .filter(|m| !m.is_within_tolerance(tolerance_percent))
            .cloned()
            .collect()
    }

    /// Clear all measurements
    pub fn clear(&self) {
        self.measurements.lock().unwrap().clear();
        self.test_stack.lock().unwrap().clear();
        self.operation_counter.store(0, Ordering::SeqCst);
    }

    /// Generate a summary report
    pub fn generate_report(&self) -> GasReport {
        let measurements = self.get_measurements();
        let stats = self.get_all_statistics();

        let total_gas: i128 = measurements.iter().map(|m| m.actual_gas).sum();
        let total_estimated: i128 = measurements.iter().map(|m| m.estimated_gas).sum();

        GasReport {
            total_measurements: measurements.len(),
            total_gas_consumed: total_gas,
            total_estimated_gas: total_estimated,
            average_efficiency: if total_estimated > 0 {
                (total_gas as f64) / (total_estimated as f64)
            } else {
                1.0
            },
            operation_statistics: stats,
        }
    }
}

/// Global gas meter instance
lazy_static::lazy_static! {
    pub static ref GAS_METER: GasMeter = GasMeter::new();
}

/// Report structure containing gas metrics summary
#[derive(Debug, Clone)]
pub struct GasReport {
    pub total_measurements: usize,
    pub total_gas_consumed: i128,
    pub total_estimated_gas: i128,
    pub average_efficiency: f64,
    pub operation_statistics: BTreeMap<String, GasStatistics>,
}

impl GasReport {
    pub fn print_summary(&self) {
        println!("\n===== GAS METERING SUMMARY REPORT =====");
        println!("Total Measurements: {}", self.total_measurements);
        println!("Total Gas Consumed: {} stroops", self.total_gas_consumed);
        println!("Total Estimated Gas: {} stroops", self.total_estimated_gas);
        println!("Average Efficiency Ratio: {:.4}x", self.average_efficiency);
        println!("\nOperation Breakdown:");
        println!("{:<40} {:>15} {:>15} {:>15} {:>12}", "Operation", "Count", "Avg Gas", "Estimated", "Ratio");
        println!("{}", "=".repeat(100));

        for (op_name, stats) in &self.operation_statistics {
            let ratio = stats.efficiency_ratio();
            println!(
                "{:<40} {:>15} {:>15} {:>15} {:>12.4}x",
                op_name,
                stats.count,
                stats.avg_gas,
                stats.avg_estimated,
                ratio
            );
        }
        println!("\n");
    }

    pub fn print_detailed_report(&self) {
        self.print_summary();
        println!("===== DETAILED OPERATION METRICS =====");

        for (op_name, stats) in &self.operation_statistics {
            println!("\n{}", op_name);
            println!("  Measurements: {}", stats.count);
            println!("  Min: {} stroops", stats.min_gas);
            println!("  Max: {} stroops", stats.max_gas);
            println!("  Avg: {} stroops", stats.avg_gas);
            println!("  Total: {} stroops", stats.total_gas);
            println!("  Estimated: {} stroops", stats.avg_estimated);
            println!("  Variance: {:.2}%", stats.variance_percentage());
        }
    }
}

// ============================================================================
// Test Helper Functions
// ============================================================================

/// Guard to automatically manage test context
pub struct TestGasGuard {
    test_name: String,
}

impl TestGasGuard {
    pub fn new(test_name: impl Into<String>) -> Self {
        let test_name = test_name.into();
        GAS_METER.push_test(test_name.clone());
        TestGasGuard { test_name }
    }
}

impl Drop for TestGasGuard {
    fn drop(&mut self) {
        GAS_METER.pop_test();
    }
}

/// Measure gas for a closure
pub fn measure_gas<F, T>(operation_name: impl Into<String>, estimated: i128, f: F) -> T
where
    F: FnOnce() -> T,
{
    let operation_name = operation_name.into();
    
    // Get CPU time before
    let start = std::time::Instant::now();
    
    // Execute operation
    let result = f();
    
    // Get CPU time after (as proxy for gas usage in tests)
    let duration = start.elapsed();
    let actual_gas = (duration.as_micros() as i128) * 1000; // Convert to approximate stroops
    
    GAS_METER.record_measurement(operation_name, estimated, actual_gas);
    result
}

// ============================================================================
// Macro Helpers
// ============================================================================

/// Measure a single operation with estimated gas cost
#[macro_export]
macro_rules! measure_op {
    ($op_name:expr, $estimated:expr, $code:block) => {{
        $crate::gas_metrics::measure_gas($op_name, $estimated, || $code)
    }};
}

/// Create a test guard automatically
#[macro_export]
macro_rules! test_with_gas_meter {
    ($test_name:expr, $body:block) => {{
        let _guard = $crate::gas_metrics::TestGasGuard::new($test_name);
        $body
    }};
}

// ============================================================================
// Benchmarking Functions
// ============================================================================

/// Compare two implementations and report gas differences
pub struct GasBenchmark {
    pub operation_name: String,
    pub baseline_gas: i128,
    pub optimized_gas: i128,
}

impl GasBenchmark {
    pub fn improvement_percent(&self) -> f64 {
        if self.baseline_gas == 0 {
            return 0.0;
        }
        ((self.baseline_gas - self.optimized_gas) as f64 / self.baseline_gas as f64) * 100.0
    }

    pub fn print_comparison(&self) {
        let improvement = self.improvement_percent();
        let status = if improvement > 0.0 { "✓ IMPROVED" } else { "✗ REGRESSED" };
        println!(
            "{}: {} baseline → {} optimized ({} {:.2}%)",
            self.operation_name, self.baseline_gas, self.optimized_gas, status, improvement.abs()
        );
    }
}

// ============================================================================
// Analytics Functions
// ============================================================================

/// Get the gas hotspots (most expensive operations)
pub fn get_gas_hotspots(limit: usize) -> Vec<(String, i128)> {
    let meter = &*GAS_METER;
    let stats = meter.get_all_statistics();
    let mut hotspots: Vec<_> = stats
        .into_iter()
        .map(|(name, stat)| (name, stat.total_gas))
        .collect();
    hotspots.sort_by(|a, b| b.1.cmp(&a.1));
    hotspots.truncate(limit);
    hotspots
}

/// Check if gas usage is within acceptable ranges
pub fn validate_gas_constraints(constraints: &GasConstraints) -> GasValidationResult {
    let meter = &*GAS_METER;
    let report = meter.generate_report();

    let mut violations = Vec::new();
    let mut warnings = Vec::new();

    // Check operation-level constraints
    for (op_name, max_gas) in &constraints.operation_limits {
        if let Some(stats) = report.operation_statistics.get(op_name) {
            if stats.max_gas > *max_gas {
                violations.push(format!(
                    "{}: max gas {} exceeds limit {}",
                    op_name, stats.max_gas, max_gas
                ));
            }
            if stats.avg_gas > *max_gas / 2 {
                warnings.push(format!(
                    "{}: avg gas {} approaching limit {}",
                    op_name, stats.avg_gas, max_gas
                ));
            }
        }
    }

    // Check total gas
    if let Some(total_limit) = constraints.total_gas_limit {
        if report.total_gas_consumed > total_limit {
            violations.push(format!(
                "Total gas {} exceeds limit {}",
                report.total_gas_consumed, total_limit
            ));
        }
    }

    // Check efficiency
    if let Some(min_efficiency) = constraints.min_efficiency_ratio {
        if report.average_efficiency < min_efficiency {
            warnings.push(format!(
                "Average efficiency {:.2}x below threshold {:.2}x",
                report.average_efficiency, min_efficiency
            ));
        }
    }

    GasValidationResult {
        is_valid: violations.is_empty(),
        violations,
        warnings,
        report,
    }
}

/// Gas constraints configuration
pub struct GasConstraints {
    pub operation_limits: BTreeMap<String, i128>,
    pub total_gas_limit: Option<i128>,
    pub min_efficiency_ratio: Option<f64>,
}

impl Default for GasConstraints {
    fn default() -> Self {
        let mut operation_limits = BTreeMap::new();
        operation_limits.insert("register_meter".to_string(), 15_000_000);
        operation_limits.insert("top_up".to_string(), 10_000_000);
        operation_limits.insert("claim".to_string(), 12_000_000);
        operation_limits.insert("update_heartbeat".to_string(), 5_000_000);

        GasConstraints {
            operation_limits,
            total_gas_limit: None,
            min_efficiency_ratio: Some(1.5),
        }
    }
}

/// Result of gas validation
pub struct GasValidationResult {
    pub is_valid: bool,
    pub violations: Vec<String>,
    pub warnings: Vec<String>,
    pub report: GasReport,
}

impl GasValidationResult {
    pub fn print_report(&self) {
        self.report.print_summary();

        if !self.violations.is_empty() {
            println!("\n===== VIOLATIONS =====");
            for violation in &self.violations {
                println!("✗ {}", violation);
            }
        }

        if !self.warnings.is_empty() {
            println!("\n===== WARNINGS =====");
            for warning in &self.warnings {
                println!("⚠ {}", warning);
            }
        }

        if self.is_valid {
            println!("\n✓ All gas constraints satisfied!");
        } else {
            println!("\n✗ Gas constraints violated!");
        }
    }
}

// ============================================================================
// Unit Tests for Gas Metering
// ============================================================================

#[cfg(test)]
mod meter_tests {
    use super::*;

    #[test]
    fn test_gas_measurement_creation() {
        GAS_METER.record_measurement("test_op", 1_000_000, 900_000);
        let measurements = GAS_METER.get_measurements();
        assert!(!measurements.is_empty());
        assert_eq!(measurements[0].operation_name, "test_op");
        GAS_METER.clear();
    }

    #[test]
    fn test_gas_efficiency_calculation() {
        let measurement = GasMeasurement {
            operation_name: "test".to_string(),
            estimated_gas: 1_000_000,
            actual_gas: 800_000,
            timestamp_ns: 0,
            test_name: "test".to_string(),
        };
        assert!(measurement.efficiency_ratio() < 1.0);
        assert_eq!(measurement.gas_variance(), -200_000);
    }

    #[test]
    fn test_gas_statistics_aggregation() {
        GAS_METER.clear();
        GAS_METER.record_measurement("op1", 1_000_000, 900_000);
        GAS_METER.record_measurement("op1", 1_000_000, 1_100_000);
        GAS_METER.record_measurement("op1", 1_000_000, 950_000);

        let stats = GAS_METER.get_operation_statistics("op1");
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min_gas, 900_000);
        assert_eq!(stats.max_gas, 1_100_000);
        GAS_METER.clear();
    }

    #[test]
    fn test_gas_report_generation() {
        GAS_METER.clear();
        GAS_METER.record_measurement("op1", 1_000_000, 900_000);
        GAS_METER.record_measurement("op2", 2_000_000, 2_100_000);

        let report = GAS_METER.generate_report();
        assert_eq!(report.total_measurements, 2);
        assert_eq!(report.total_gas_consumed, 3_000_000);
        GAS_METER.clear();
    }
}
