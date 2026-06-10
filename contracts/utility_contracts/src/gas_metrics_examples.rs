/// Gas Metering Integration Examples and Utilities
///
/// This module provides practical examples and utilities for integrating
/// automated gas metering metrics into the test suite.
///
/// Usage Patterns:
/// 1. Manual measurement of operations
/// 2. Batch operation profiling
/// 3. Comparative benchmarking
/// 4. Regression detection

#![cfg(test)]

extern crate std;

use crate::gas_metrics::*;
use std::collections::BTreeMap;

// ============================================================================
// Example: Basic Operation Gas Measurement
// ============================================================================

/// Example: Measure a single operation
#[test]
fn example_measure_single_operation() {
    let _guard = TestGasGuard::new("example_measure_single_operation");

    // Measure a hypothetical operation
    let result = measure_gas("hypothetical_register_meter", GasBaseline::REGISTER_METER, || {
        // Simulate the operation
        let mut sum = 0i128;
        for i in 0..1000 {
            sum += i;
        }
        sum
    });

    // Operation completed, gas was recorded
    assert!(result > 0);

    // Retrieve metrics
    let measurements = GAS_METER.get_measurements();
    assert!(!measurements.is_empty());

    // Print report
    let report = GAS_METER.generate_report();
    report.print_summary();

    GAS_METER.clear();
}

// ============================================================================
// Example: Batch Operation Profiling
// ============================================================================

/// Example: Profile multiple operations in sequence
#[test]
fn example_batch_operation_profiling() {
    let _guard = TestGasGuard::new("example_batch_operation_profiling");

    let operations = vec![
        ("read_meter", GasBaseline::SIMPLE_READ),
        ("write_meter", GasBaseline::SIMPLE_WRITE),
        ("transfer_tokens", GasBaseline::TOKEN_TRANSFER),
        ("storage_operation", GasBaseline::STORAGE_OPERATION),
    ];

    for (op_name, estimated_gas) in operations {
        measure_gas(op_name, estimated_gas, || {
            // Simulate operation
            std::thread::sleep(std::time::Duration::from_micros(100));
        });
    }

    // Generate comprehensive report
    let report = GAS_METER.generate_report();
    report.print_detailed_report();

    GAS_METER.clear();
}

// ============================================================================
// Example: Comparative Benchmarking
// ============================================================================

/// Example: Compare baseline vs optimized implementation
#[test]
fn example_comparative_benchmark() {
    let _guard = TestGasGuard::new("example_comparative_benchmark");

    // Run baseline implementation
    measure_gas("baseline_calculation", 5_000_000, || {
        let mut result = 0i128;
        for i in 0..10000 {
            result = result.saturating_add(i);
        }
        result
    });

    // Run optimized implementation
    measure_gas("optimized_calculation", 3_000_000, || {
        // Same calculation, but optimized
        (0i128..10000).fold(0i128, |acc, i| acc.saturating_add(i))
    });

    // Get statistics
    let baseline_stats = GAS_METER.get_operation_statistics("baseline_calculation");
    let optimized_stats = GAS_METER.get_operation_statistics("optimized_calculation");

    if let (Some(baseline), Some(optimized)) = (baseline_stats, optimized_stats) {
        let benchmark = GasBenchmark {
            operation_name: "calculation".to_string(),
            baseline_gas: baseline.avg_gas,
            optimized_gas: optimized.avg_gas,
        };
        benchmark.print_comparison();
    }

    GAS_METER.clear();
}

// ============================================================================
// Example: Regression Detection
// ============================================================================

/// Example: Detect gas usage regressions
#[test]
fn example_regression_detection() {
    let _guard = TestGasGuard::new("example_regression_detection");

    // Record metrics for an operation
    for _ in 0..5 {
        measure_gas("streaming_operation", 10_000_000, || {
            // Simulate streaming operation
            std::thread::sleep(std::time::Duration::from_micros(50));
        });
    }

    // Check for deviations
    let deviations = GAS_METER.get_deviations(20.0); // 20% tolerance
    
    println!("Operations deviating from estimates:");
    for deviation in deviations {
        println!(
            "  {}: {:.2}% variance",
            deviation.operation_name,
            deviation.variance_percentage()
        );
    }

    GAS_METER.clear();
}

// ============================================================================
// Example: Gas Hotspot Analysis
// ============================================================================

/// Example: Identify the most expensive operations
#[test]
fn example_hotspot_analysis() {
    let _guard = TestGasGuard::new("example_hotspot_analysis");

    // Simulate multiple random operations
    let operations = vec![
        ("op_a", 2_000_000),
        ("op_b", 5_000_000),
        ("op_c", 1_000_000),
        ("op_d", 15_000_000),
        ("op_e", 8_000_000),
    ];

    for (op_name, estimated) in &operations {
        for _ in 0..3 {
            measure_gas(op_name, *estimated, || {
                std::thread::sleep(std::time::Duration::from_micros(10));
            });
        }
    }

    // Get hotspots
    let hotspots = get_gas_hotspots(3);
    println!("Top 3 gas hotspots:");
    for (i, (op_name, total_gas)) in hotspots.iter().enumerate() {
        println!("  {}. {} - {} stroops", i + 1, op_name, total_gas);
    }

    GAS_METER.clear();
}

// ============================================================================
// Example: Gas Constraints Validation
// ============================================================================

/// Example: Validate against gas constraints
#[test]
fn example_validate_gas_constraints() {
    let _guard = TestGasGuard::new("example_validate_gas_constraints");

    // Record some measurements
    measure_gas("operation_1", 5_000_000, || {
        std::thread::sleep(std::time::Duration::from_micros(100));
    });

    measure_gas("operation_2", 10_000_000, || {
        std::thread::sleep(std::time::Duration::from_micros(200));
    });

    // Define constraints
    let mut constraints = GasConstraints::default();
    constraints.operation_limits.insert("operation_1".to_string(), 8_000_000);
    constraints.operation_limits.insert("operation_2".to_string(), 20_000_000);
    constraints.total_gas_limit = Some(30_000_000);

    // Validate
    let result = validate_gas_constraints(&constraints);
    result.print_report();

    assert!(result.is_valid);

    GAS_METER.clear();
}

// ============================================================================
// Example: Stream Operations Gas Analysis
// ============================================================================

/// Example: Analyze gas for streaming operations specifically
#[test]
fn example_stream_operations_analysis() {
    let _guard = TestGasGuard::new("example_stream_operations_analysis");

    let stream_operations = vec![
        ("create_stream", GasBaseline::REGISTER_METER),
        ("update_flow_rate", GasBaseline::TOP_UP),
        ("withdraw_stream", GasBaseline::CLAIM),
        ("close_stream", GasBaseline::EMERGENCY_SHUTDOWN),
    ];

    // Simulate each operation multiple times
    for (op_name, estimated) in stream_operations {
        for iteration in 0..10 {
            let variance = (iteration % 3) as i128 * 500_000; // Vary gas usage
            measure_gas(op_name, estimated, || {
                std::thread::sleep(std::time::Duration::from_micros(50 + iteration as u64));
            });
        }
    }

    // Generate detailed report
    let report = GAS_METER.generate_report();
    report.print_detailed_report();

    // Find operations exceeding threshold
    let expensive = GAS_METER.get_expensive_operations(12_000_000);
    println!("\nOperations exceeding 12M stroops: {}", expensive.len());

    GAS_METER.clear();
}

// ============================================================================
// Example: Contract Initialization Gas Profile
// ============================================================================

/// Example: Profile initialization overhead
#[test]
fn example_initialization_profile() {
    let _guard = TestGasGuard::new("example_initialization_profile");

    // Measure initialization phases
    measure_gas("initialize_admin", 1_000_000, || {
        // Simulate admin setup
    });

    measure_gas("initialize_storage", 3_000_000, || {
        // Simulate storage initialization
    });

    measure_gas("initialize_token_interface", 2_000_000, || {
        // Simulate token interface setup
    });

    measure_gas("initialize_oracle_connection", 5_000_000, || {
        // Simulate oracle connection
    });

    let stats = GAS_METER.get_all_statistics();
    let total_init_gas: i128 = stats.iter().map(|(_, s)| s.total_gas).sum();

    println!("Total Initialization Gas: {} stroops", total_init_gas);
    println!("Initialization phases: {}", stats.len());

    GAS_METER.clear();
}

// ============================================================================
// Example: Gas Scaling Analysis
// ============================================================================

/// Example: Analyze how gas scales with operation size
#[test]
fn example_gas_scaling_analysis() {
    let _guard = TestGasGuard::new("example_gas_scaling_analysis");

    // Test operation with increasing complexity
    let sizes = vec![10, 50, 100, 500, 1000];

    for size in sizes {
        let op_name = format!("operation_size_{}", size);
        let estimated = (size as i128) * 10_000;

        measure_gas(&op_name, estimated, || {
            let mut result = 0i128;
            for i in 0..size {
                result = result.saturating_add(i as i128);
            }
            result
        });
    }

    // Analyze scaling
    let stats = GAS_METER.get_all_statistics();
    println!("Gas scaling analysis:");
    for (op_name, stat) in stats.iter().take(5) {
        let size: usize = op_name.split('_').last().unwrap().parse().unwrap_or(0);
        let per_unit = if size > 0 {
            stat.avg_gas / size as i128
        } else {
            0
        };
        println!("  {}: {}/unit at size {}", op_name, per_unit, size);
    }

    GAS_METER.clear();
}

// ============================================================================
// Example: Test-to-Production Gas Variance
// ============================================================================

/// Example: Verify test gas measurements match production expectations
#[test]
fn example_production_variance_check() {
    let _guard = TestGasGuard::new("example_production_variance_check");

    // Define production baseline estimates (from gas_estimator.rs)
    let production_estimates = vec![
        ("register_meter", GasBaseline::REGISTER_METER),
        ("top_up", GasBaseline::TOP_UP),
        ("claim", GasBaseline::CLAIM),
        ("update_heartbeat", GasBaseline::UPDATE_HEARTBEAT),
    ];

    for (op_name, estimated) in production_estimates {
        // Simulate operation in test environment
        measure_gas(op_name, estimated, || {
            // Operation logic here
            std::thread::sleep(std::time::Duration::from_micros(100));
        });
    }

    // Check variance against production estimates
    let deviations = GAS_METER.get_deviations(50.0); // 50% tolerance for test vs production
    
    if deviations.is_empty() {
        println!("✓ All test measurements within 50% of production estimates");
    } else {
        println!("⚠ {} operations exceed 50% variance", deviations.len());
        for dev in deviations {
            println!("  {}: {:.2}% variance", dev.operation_name, dev.variance_percentage());
        }
    }

    GAS_METER.clear();
}

// ============================================================================
// Utility: Performance Regression Test Suite
// ============================================================================

/// Utility structure for tracking performance across test runs
pub struct PerformanceBaseline {
    pub operation_baselines: BTreeMap<String, i128>,
}

impl PerformanceBaseline {
    pub fn new() -> Self {
        PerformanceBaseline {
            operation_baselines: BTreeMap::new(),
        }
    }

    pub fn add_baseline(&mut self, operation: String, gas_cost: i128) {
        self.operation_baselines.insert(operation, gas_cost);
    }

    pub fn check_regression(&self, max_regression_percent: f64) -> Vec<String> {
        let mut regressions = Vec::new();
        let stats = GAS_METER.get_all_statistics();

        for (op_name, baseline_gas) in &self.operation_baselines {
            if let Some(actual_stat) = stats.get(op_name) {
                let increase_percent =
                    ((actual_stat.avg_gas - baseline_gas) as f64 / *baseline_gas as f64) * 100.0;
                if increase_percent > max_regression_percent {
                    regressions.push(format!(
                        "{}: {:.2}% regression (from {} to {})",
                        op_name, increase_percent, baseline_gas, actual_stat.avg_gas
                    ));
                }
            }
        }

        regressions
    }
}

// ============================================================================
// Integration Test: Complete Gas Metering Workflow
// ============================================================================

/// Comprehensive integration test demonstrating full gas metering workflow
#[test]
fn integration_test_complete_gas_metering_workflow() {
    let _guard = TestGasGuard::new("integration_complete_workflow");

    println!("\n=== Gas Metering Integration Test ===\n");

    // Step 1: Establish baseline
    println!("Step 1: Establishing baseline operations...");
    for i in 0..3 {
        measure_gas("baseline_op", 5_000_000, || {
            std::thread::sleep(std::time::Duration::from_micros(50));
        });
    }

    // Step 2: Measure operations
    println!("Step 2: Measuring operations...");
    let operations = vec![
        ("create_stream", 10_000_000),
        ("process_payment", 8_000_000),
        ("update_config", 3_000_000),
    ];

    for (op, estimated) in operations {
        for _ in 0..5 {
            measure_gas(op, estimated, || {
                std::thread::sleep(std::time::Duration::from_micros(100));
            });
        }
    }

    // Step 3: Analyze results
    println!("Step 3: Analyzing results...");
    let report = GAS_METER.generate_report();
    report.print_summary();

    // Step 4: Identify hotspots
    println!("\nStep 4: Identifying hotspots...");
    let hotspots = get_gas_hotspots(5);
    for (op, gas) in hotspots {
        println!("  - {} consumes {} stroops", op, gas);
    }

    // Step 5: Validate constraints
    println!("\nStep 5: Validating constraints...");
    let constraints = GasConstraints::default();
    let validation = validate_gas_constraints(&constraints);
    println!("Validation result: {}", if validation.is_valid { "PASSED" } else { "FAILED" });

    GAS_METER.clear();
}
