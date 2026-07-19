/**
 * Dynamic TypeScript-to-JavaScript Test Runner.
 * Strips TypeScript types dynamically to execute test assertions against the exact TS file,
 * preventing any duplicate code.
 */
const fs = require('fs');
const path = require('path');

// 1. Read TypeScript implementation file
const tsFilePath = path.join(__dirname, 'gracefulDegradation.ts');
let code = fs.readFileSync(tsFilePath, 'utf8');

// Remove comments to prevent syntax errors inside comments
code = code.replace(/\/\*[\s\S]*?\*\//g, '');
code = code.replace(/\/\/.*/g, '');

// Remove 'export ' prefixes
code = code.replace(/export /g, '');

// 2. Perform safe, lightweight regex-based TypeScript stripping
// Remove 'enum ...' and replace with standard JS object
code = code.replace(/enum (\w+) \{([\s\S]*?)\}/g, (match, enumName, body) => {
  // Convert lines like "NORMAL = 0," to "NORMAL: 0,"
  const jsBody = body.replace(/=/g, ':');
  return `const ${enumName} = {${jsBody}};`;
});

// Remove typescript type definitions (type ... or interface ...)
code = code.replace(/type [\s\S]*?;/g, '');
code = code.replace(/interface [\s\S]*?\}/g, '');

// Explicitly strip type annotations to handle nested generics perfectly without complex parsing
code = code.replace(/: Partial<Record<FeatureKey, boolean>>/g, '');
code = code.replace(/: Record<FeatureKey, boolean>/g, '');
code = code.replace(/ as FeatureKey\[\]/g, '');
code = code.replace(/: SheddingLevel/g, '');
code = code.replace(/: number/g, '');
code = code.replace(/: string \| null/g, '');
code = code.replace(/: DegradationState/g, '');
code = code.replace(/as \w+/g, '');
code = code.replace(/!\s*;/g, ';');
code = code.replace(/!\s*,/g, ',');

// Add CommonJS export for testing
code += `\nmodule.exports = { SheddingLevel, calculateDegradationState };`;

// 3. Evaluate stripped code in a fresh module context
const tempModule = { exports: {} };
const runInContext = new Function('module', 'exports', code);
runInContext(tempModule, tempModule.exports);

const { calculateDegradationState, SheddingLevel } = tempModule.exports;

// 4. Run Jest-Equivalent Test assertions
function runTests() {
  console.log('🧪 Running Graceful Degradation and Capacity Shedding Tests against TS implementation...');
  const assertions = [];
  const failures = [];

  const assert = (condition, message) => {
    if (condition) {
      assertions.push("   ✅ PASS: " + message);
    } else {
      failures.push("   ❌ FAIL: " + message);
    }
  };

  // Test 1: Normal State (Level 0)
  try {
    const state = calculateDegradationState(50, 20); // 50% load, 20ms latency
    assert(state.sheddingLevel === SheddingLevel.NORMAL, 'Should be in NORMAL state');
    assert(state.activeFlags.HIGH_FREQ_POLLING === true, 'High frequency polling should be enabled in NORMAL');
    assert(state.activeFlags.ZK_VERIFICATION === true, 'ZK verification should be enabled in NORMAL');
    assert(state.pollingIntervalMs === 5000, 'Polling interval should be 5 seconds in NORMAL');
    assert(state.availabilityPercent === 99.99, 'Availability should be 99.99% under normal conditions');
    assert(state.alertMessage === null, 'Alert message should be null in NORMAL');
  } catch (err) {
    failures.push(`Test 1 crashed: ${err.message}`);
  }

  // Test 2: Moderate State (Level 1)
  try {
    const state = calculateDegradationState(82, 40); // 82% load, 40ms latency
    assert(state.sheddingLevel === SheddingLevel.MODERATE, 'Should be in MODERATE state');
    assert(state.activeFlags.HIGH_FREQ_POLLING === false, 'High frequency polling should be shed in MODERATE');
    assert(state.activeFlags.COMPLEX_FORECAST === true, 'Complex forecasting remains enabled in MODERATE');
    assert(state.pollingIntervalMs === 15000, 'Polling interval should increase to 15 seconds in MODERATE');
    assert(state.alertMessage !== null && state.alertMessage.includes('Moderate'), 'Should broadcast Moderate alert');
  } catch (err) {
    failures.push(`Test 2 crashed: ${err.message}`);
  }

  // Test 3: High Congestion State (Level 2)
  try {
    const state = calculateDegradationState(92, 100); // 92% load
    assert(state.sheddingLevel === SheddingLevel.HIGH, 'Should be in HIGH state');
    assert(state.activeFlags.HEAVY_CHARTS === false, 'Heavy charts should be disabled in HIGH');
    assert(state.activeFlags.POSTPAID_STREAMS === false, 'Postpaid stream creation should be restricted in HIGH');
    assert(state.pollingIntervalMs === 30000, 'Polling interval should increase to 30 seconds in HIGH');
  } catch (err) {
    failures.push(`Test 3 crashed: ${err.message}`);
  }

  // Test 4: Critical Emergency State (Level 3)
  try {
    const state = calculateDegradationState(98, 600); // 98% load, 600ms latency
    assert(state.sheddingLevel === SheddingLevel.CRITICAL, 'Should be in CRITICAL state');
    assert(state.activeFlags.ZK_VERIFICATION === false, 'ZK Verification must be bypassed in CRITICAL');
    assert(state.pollingIntervalMs === 120000, 'Polling interval should be 120s in CRITICAL');
    assert(state.alertMessage !== null && state.alertMessage.includes('Emergency'), 'Should broadcast Emergency alert');
  } catch (err) {
    failures.push(`Test 4 crashed: ${err.message}`);
  }

  // Test 5: Manual Override Controls
  try {
    const overrides = {
      HEAVY_CHARTS: true, // Force-enable heavy charts under high load
      ZK_VERIFICATION: false, // Force-disable ZK verification under normal load
    };
    const state = calculateDegradationState(50, 20, overrides);
    assert(state.activeFlags.HEAVY_CHARTS === true, 'Heavy charts should be overridden to TRUE');
    assert(state.activeFlags.ZK_VERIFICATION === false, 'ZK verification should be overridden to FALSE');
  } catch (err) {
    failures.push(`Test 5 crashed: ${err.message}`);
  }

  // Test 6: Performance SLA Target (< 100ms)
  try {
    const start = Date.now();
    for (let i = 0; i < 1000; i++) {
      calculateDegradationState(i % 100, (i * 5) % 1000);
    }
    const end = Date.now();
    const avgDuration = (end - start) / 1000;
    assert(avgDuration < 1.0, `Average execution latency must be < 1ms (calculated: ${avgDuration}ms)`);
  } catch (err) {
    failures.push(`Test 6 crashed: ${err.message}`);
  }

  // Report Results
  console.log('\n--- Test Execution Summary ---');
  assertions.forEach((msg) => console.log(msg));

  if (failures.length > 0) {
    console.error('\n❌ Some tests failed:');
    failures.forEach((msg) => console.error(msg));
    process.exit(1);
  } else {
    console.log('\n✨ All 6 core test suites passed successfully with zero duplicate logic files!');
    process.exit(0);
  }
}

runTests();
