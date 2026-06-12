// Simple validation script to check the meter simulator structure
const fs = require('fs');
const path = require('path');

console.log('🔍 Validating Meter Simulator Structure...\n');

const requiredFiles = [
  'package.json',
  'src/index.js',
  'src/config.js',
  'src/meter-device.js',
  'src/contract-interface.js',
  'src/mqtt-publisher.js',
  '.env.example',
  'README.md'
];

const optionalFiles = [
  'scripts/setup.sh',
  'scripts/setup.ps1',
  'examples/basic-usage.js',
  'tests/meter-device.test.js'
];

let errors = [];
let warnings = [];

// Check required files
console.log('📋 Checking required files:');
requiredFiles.forEach(file => {
  if (fs.existsSync(file)) {
    console.log(`✅ ${file}`);
  } else {
    console.log(`❌ ${file} - Missing!`);
    errors.push(`Missing required file: ${file}`);
  }
});

// Check optional files
console.log('\n📋 Checking optional files:');
optionalFiles.forEach(file => {
  if (fs.existsSync(file)) {
    console.log(`✅ ${file}`);
  } else {
    console.log(`⚠️  ${file} - Optional`);
    warnings.push(`Missing optional file: ${file}`);
  }
});

// Validate package.json
console.log('\n📦 Validating package.json:');
try {
  const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  
  if (packageJson.name === 'billing-contracts-meter-simulator') {
    console.log('✅ Package name correct');
  } else {
    console.log('❌ Package name incorrect');
    errors.push('Invalid package name');
  }
  
  if (packageJson.bin && packageJson.bin['meter-simulator']) {
    console.log('✅ CLI binary defined');
  } else {
    console.log('❌ CLI binary not defined');
    errors.push('CLI binary not defined');
  }
  
  const requiredDeps = ['commander', 'chalk', 'inquirer', 'mqtt', 'stellar-sdk', 'axios', 'tweetnacl'];
  requiredDeps.forEach(dep => {
    if (packageJson.dependencies && packageJson.dependencies[dep]) {
      console.log(`✅ Dependency: ${dep}`);
    } else {
      console.log(`❌ Missing dependency: ${dep}`);
      errors.push(`Missing dependency: ${dep}`);
    }
  });
  
} catch (error) {
  console.log('❌ Invalid package.json');
  errors.push('Invalid package.json');
}

// Validate main CLI file
console.log('\n🔧 Validating main CLI file:');
try {
  const cliContent = fs.readFileSync('src/index.js', 'utf8');
  
  if (cliContent.includes('#!/usr/bin/env node')) {
    console.log('✅ Shebang present');
  } else {
    console.log('⚠️  Shebang missing');
    warnings.push('Shebang missing');
  }
  
  if (cliContent.includes('program.version(\'1.0.0\')')) {
    console.log('✅ Version defined');
  } else {
    console.log('❌ Version not defined');
    errors.push('Version not defined');
  }
  
  const requiredCommands = ['generate-keys', 'register', 'simulate', 'send-reading', 'status'];
  requiredCommands.forEach(cmd => {
    if (cliContent.includes(`.command('${cmd}')`)) {
      console.log(`✅ Command: ${cmd}`);
    } else {
      console.log(`❌ Missing command: ${cmd}`);
      errors.push(`Missing command: ${cmd}`);
    }
  });
  
} catch (error) {
  console.log('❌ Invalid CLI file');
  errors.push('Invalid CLI file');
}

// Validate source files
console.log('\n📁 Validating source files:');
const sourceFiles = [
  'src/config.js',
  'src/meter-device.js',
  'src/contract-interface.js',
  'src/mqtt-publisher.js'
];

sourceFiles.forEach(file => {
  try {
    const content = fs.readFileSync(file, 'utf8');
    if (content.includes('module.exports')) {
      console.log(`✅ ${file} - Module exports present`);
    } else {
      console.log(`❌ ${file} - No module exports`);
      errors.push(`No module exports in ${file}`);
    }
  } catch (error) {
    console.log(`❌ ${file} - Invalid file`);
    errors.push(`Invalid file: ${file}`);
  }
});

// Summary
console.log('\n📊 Validation Summary:');
console.log(`✅ Success: ${errors.length === 0 ? 'PASS' : 'FAIL'}`);
console.log(`❌ Errors: ${errors.length}`);
console.log(`⚠️  Warnings: ${warnings.length}`);

if (errors.length > 0) {
  console.log('\n❌ Errors:');
  errors.forEach(error => console.log(`   - ${error}`));
}

if (warnings.length > 0) {
  console.log('\n⚠️  Warnings:');
  warnings.forEach(warning => console.log(`   - ${warning}`));
}

if (errors.length === 0) {
  console.log('\n🎉 Meter Simulator structure validation PASSED!');
  process.exit(0);
} else {
  console.log('\n💥 Meter Simulator structure validation FAILED!');
  process.exit(1);
}
