#!/usr/bin/env node

const { Command } = require('commander');
const chalk = require('chalk');
const inquirer = require('inquirer');
const fs = require('fs').promises;
const path = require('path');
const crypto = require('crypto');
const nacl = require('tweetnacl');
const bs58 = require('bs58');

const MeterDevice = require('./meter-device');
const ContractInterface = require('./contract-interface');
const MQTTPublisher = require('./mqtt-publisher');
const config = require('./config');

const program = new Command();

program
  .name('meter-simulator')
  .description('ESP32 meter simulator for Utility-Protocol smart contracts')
  .version('1.0.0');

// Generate device key pair
program
  .command('generate-keys')
  .description('Generate new Ed25519 key pair for meter device')
  .option('-o, --output <file>', 'Output file for keys', 'device-keys.json')
  .action(async (options) => {
    try {
      console.log(chalk.blue('🔑 Generating new device key pair...'));
      
      const keyPair = nacl.sign.keyPair();
      const keys = {
        private_key: bs58.encode(keyPair.secretKey.slice(0, 32)),
        public_key: bs58.encode(keyPair.publicKey),
        private_key_hex: Buffer.from(keyPair.secretKey.slice(0, 32)).toString('hex'),
        public_key_hex: Buffer.from(keyPair.publicKey).toString('hex'),
        public_key_base64: Buffer.from(keyPair.publicKey).toString('base64'),
        generated_at: new Date().toISOString()
      };

      await fs.writeFile(options.output, JSON.stringify(keys, null, 2));
      
      console.log(chalk.green('✅ Keys generated successfully!'));
      console.log(chalk.yellow(`📁 Saved to: ${options.output}`));
      console.log(chalk.cyan(`🔐 Public Key: ${keys.public_key}`));
      console.log(chalk.red(`⚠️  Keep the private key secure!`));
      
    } catch (error) {
      console.error(chalk.red('❌ Error generating keys:'), error.message);
      process.exit(1);
    }
  });

// Register meter with contract
program
  .command('register')
  .description('Register a new meter with the smart contract')
  .option('-k, --keys <file>', 'Device keys file', 'device-keys.json')
  .option('-u, --user <address>', 'User address')
  .option('-p, --provider <address>', 'Provider address')
  .option('-r, --rate <rate>', 'Off-peak rate (tokens per second)', '10')
  .option('-t, --token <address>', 'Token address (native XLM if not specified)')
  .action(async (options) => {
    try {
      console.log(chalk.blue('📝 Registering meter with contract...'));
      
      const keysData = await fs.readFile(options.keys, 'utf8');
      const keys = JSON.parse(keysData);
      
      const answers = await inquirer.prompt([
        {
          type: 'input',
          name: 'userAddress',
          message: 'Enter user address:',
          when: !options.user,
          validate: (input) => input.length > 0 || 'User address is required'
        },
        {
          type: 'input',
          name: 'providerAddress',
          message: 'Enter provider address:',
          when: !options.provider,
          validate: (input) => input.length > 0 || 'Provider address is required'
        },
        {
          type: 'number',
          name: 'rate',
          message: 'Enter off-peak rate (tokens per second):',
          default: parseInt(options.rate),
          when: !options.rate,
          validate: (input) => input > 0 || 'Rate must be positive'
        }
      ]);

      const contract = new ContractInterface(config.contract);
      const meterId = await contract.registerMeter({
        userAddress: options.user || answers.userAddress,
        providerAddress: options.provider || answers.providerAddress,
        offPeakRate: options.rate || answers.rate,
        tokenAddress: options.token,
        devicePublicKey: keys.public_key_base64
      });

      console.log(chalk.green(`✅ Meter registered successfully!`));
      console.log(chalk.cyan(`🔢 Meter ID: ${meterId}`));
      
      // Save meter configuration
      const meterConfig = {
        meter_id: meterId,
        keys: keys,
        user_address: options.user || answers.userAddress,
        provider_address: options.provider || answers.providerAddress,
        off_peak_rate: options.rate || answers.rate,
        token_address: options.token,
        registered_at: new Date().toISOString()
      };
      
      await fs.writeFile('meter-config.json', JSON.stringify(meterConfig, null, 2));
      console.log(chalk.yellow(`📁 Meter config saved to: meter-config.json`));
      
    } catch (error) {
      console.error(chalk.red('❌ Error registering meter:'), error.message);
      process.exit(1);
    }
  });

// Start simulation
program
  .command('simulate')
  .description('Start meter simulation with realistic usage patterns')
  .option('-c, --config <file>', 'Meter configuration file', 'meter-config.json')
  .option('-i, --interval <seconds>', 'Reporting interval in seconds', '30')
  .option('-m, --mode <mode>', 'Simulation mode (realistic/surge/low)', 'realistic')
  .option('--mqtt', 'Use MQTT for publishing (default: direct contract calls)')
  .option('--zk', 'Enable Zero-Knowledge privacy mode')
  .action(async (options) => {
    try {
      console.log(chalk.blue('🚀 Starting meter simulation...'));
      
      const configData = await fs.readFile(options.config, 'utf8');
      const meterConfig = JSON.parse(configData);
      
      const device = new MeterDevice(meterConfig);
      const contract = new ContractInterface(config.contract);
      
      let publisher;
      if (options.mqtt) {
        publisher = new MQTTPublisher(config.mqtt);
        await publisher.connect();
        console.log(chalk.green('📡 Connected to MQTT broker'));
      }
      
      console.log(chalk.cyan(`📊 Simulation mode: ${options.mode}`));
      console.log(chalk.cyan(`⏱️  Reporting interval: ${options.interval} seconds`));
      console.log(chalk.yellow('Press Ctrl+C to stop simulation\n'));
      
      const interval = setInterval(async () => {
        try {
          if (options.mqtt) {
            const usageData = device.generateUsageData(options.mode);
            await publisher.publishUsageData(usageData);
            console.log(chalk.green(`📡 Published usage: ${usageData.watt_hours_consumed}Wh, ${usageData.units_consumed} units`));
          } else if (options.zk) {
            const zkData = device.generateZkUsageData(options.mode);
            await contract.submitZkUsageData(zkData);
            console.log(chalk.green(`🛡️  Submitted ZK Privacy Report: ${zkData.units_consumed} units (Verified via Groth16)`));
          } else {
            const usageData = device.generateUsageData(options.mode);
            await contract.submitUsageData(usageData);
            console.log(chalk.green(`📤 Submitted usage: ${usageData.watt_hours_consumed}Wh, ${usageData.units_consumed} units`));
          }
          
        } catch (error) {
          console.error(chalk.red('❌ Error in simulation cycle:'), error.message);
        }
      }, options.interval * 1000);
      
      // Handle graceful shutdown
      process.on('SIGINT', async () => {
        console.log(chalk.yellow('\n🛑 Stopping simulation...'));
        clearInterval(interval);
        
        if (publisher) {
          await publisher.disconnect();
          console.log(chalk.green('📡 Disconnected from MQTT broker'));
        }
        
        console.log(chalk.blue('👋 Simulation stopped'));
        process.exit(0);
      });
      
    } catch (error) {
      console.error(chalk.red('❌ Error starting simulation:'), error.message);
      process.exit(1);
    }
  });

// Send single reading
program
  .command('send-reading')
  .description('Send a single usage reading')
  .option('-c, --config <file>', 'Meter configuration file', 'meter-config.json')
  .option('-w, --watts <watts>', 'Watt hours to report', '100')
  .option('-u, --units <units>', 'Units consumed', '1')
  .option('--mqtt', 'Use MQTT for publishing')
  .action(async (options) => {
    try {
      console.log(chalk.blue('📤 Sending single reading...'));
      
      const configData = await fs.readFile(options.config, 'utf8');
      const meterConfig = JSON.parse(configData);
      
      const device = new MeterDevice(meterConfig);
      const contract = new ContractInterface(config.contract);
      
      const usageData = device.generateCustomUsageData(
        parseInt(options.watts),
        parseInt(options.units)
      );
      
      if (options.mqtt) {
        const publisher = new MQTTPublisher(config.mqtt);
        await publisher.connect();
        await publisher.publishUsageData(usageData);
        await publisher.disconnect();
        console.log(chalk.green(`📡 Published via MQTT: ${usageData.watt_hours_consumed}Wh, ${usageData.units_consumed} units`));
      } else {
        await contract.submitUsageData(usageData);
        console.log(chalk.green(`📤 Submitted to contract: ${usageData.watt_hours_consumed}Wh, ${usageData.units_consumed} units`));
      }
      
    } catch (error) {
      console.error(chalk.red('❌ Error sending reading:'), error.message);
      process.exit(1);
    }
  });

// Show meter status
program
  .command('status')
  .description('Show meter status from contract')
  .option('-c, --config <file>', 'Meter configuration file', 'meter-config.json')
  .action(async (options) => {
    try {
      const configData = await fs.readFile(options.config, 'utf8');
      const meterConfig = JSON.parse(configData);
      
      const contract = new ContractInterface(config.contract);
      const meter = await contract.getMeter(meterConfig.meter_id);
      
      console.log(chalk.blue('📊 Meter Status:'));
      console.log(chalk.cyan(`🔢 Meter ID: ${meterConfig.meter_id}`));
      console.log(chalk.cyan(`👤 User: ${meter.user}`));
      console.log(chalk.cyan(`🏢 Provider: ${meter.provider}`));
      console.log(chalk.cyan(`💰 Balance: ${meter.balance}`));
      console.log(chalk.cyan(`💳 Debt: ${meter.debt}`));
      console.log(chalk.cyan(`⚡ Active: ${meter.is_active ? 'Yes' : 'No'}`));
      console.log(chalk.cyan(`🔗 Paired: ${meter.is_paired ? 'Yes' : 'No'}`));
      console.log(chalk.cyan(`⏰ Last Update: ${new Date(meter.last_update * 1000).toLocaleString()}`));
      console.log(chalk.cyan(`📈 Total Usage: ${meter.usage_data.total_watt_hours} Wh`));
      console.log(chalk.cyan(`🔥 Peak Usage: ${meter.usage_data.peak_usage_watt_hours} Wh`));
      console.log(chalk.gray(`🧠 Cache: ${JSON.stringify(contract.getCacheMetrics())}`));
      
    } catch (error) {
      console.error(chalk.red('❌ Error fetching status:'), error.message);
      process.exit(1);
    }
  });

// Show cache configuration and runtime guidance for monitoring hooks
program
  .command('cache-info')
  .description('Show cache configuration and monitoring labels')
  .action(() => {
    console.log(chalk.blue('🧠 Cache Configuration:'));
    console.log(chalk.cyan(`Enabled: ${config.cache.enabled ? 'Yes' : 'No'}`));
    console.log(chalk.cyan(`Default TTL: ${config.cache.defaultTtlSeconds}s`));
    console.log(chalk.cyan(`Meter TTL: ${config.cache.meterTtlSeconds}s`));
    console.log(chalk.cyan(`Usage TTL: ${config.cache.usageTtlSeconds}s`));
    console.log(chalk.cyan(`Redis Enabled: ${config.cache.redis.enabled ? 'Yes' : 'No'}`));
    console.log(chalk.cyan(`Redis URL: ${config.cache.redis.url}`));
    console.log(chalk.gray('Metrics labels: cache_hits, cache_misses, cache_sets, cache_deletes, cache_errors'));
  });

program.parse();
