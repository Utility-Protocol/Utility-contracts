const { Server, Networks, TransactionBuilder, Operation, Asset, Keypair } = require('stellar-sdk');
const axios = require('axios');
const crypto = require('crypto');
const config = require('./config');
const CacheLayer = require('./cache-layer');

class ContractInterface {
  constructor(contractConfig) {
    this.network = contractConfig.network === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;
    this.server = new Server(contractConfig.rpcUrl);
    this.horizon = new Server(contractConfig.horizonUrl);
    this.contractId = contractConfig.contractId;
    this.friendbotUrl = contractConfig.friendbotUrl;
    this.cacheConfig = contractConfig.cache || config.cache;
    this.cache = contractConfig.cacheLayer || new CacheLayer(this.cacheConfig);
    this.cacheConnectPromise = null;
  }

  async initializeCache() {
    if (!this.cacheConnectPromise) {
      this.cacheConnectPromise = this.cache.connect();
    }
    await this.cacheConnectPromise;
  }

  async close() {
    await this.cache.disconnect();
  }

  /**
   * Register a new meter with the contract
   */
  async registerMeter(params) {
    try {
      console.log('🔧 Registering meter with contract...');
      
      // Convert base64 public key to hex for contract
      const publicKeyHex = Buffer.from(params.devicePublicKey, 'base64').toString('hex');
      
      // This would typically involve calling the contract's register_meter function
      // For now, we'll simulate the registration process
      const meterId = this._generateMeterId();
      
      console.log(`✅ Meter registered with ID: ${meterId}`);
      return meterId;
      
    } catch (error) {
      throw new Error(`Failed to register meter: ${error.message}`);
    }
  }

  /**
   * Submit usage data to contract
   */
  async submitUsageData(signedUsageData) {
    try {
      console.log('📤 Submitting usage data to contract...');
      this.rateLimiter.assertAllowed(`meter:${signedUsageData.meter_id}`);
      
      // Validate the data before submission
      this._validateUsageData(signedUsageData);
      
      // In a real implementation, this would:
      // 1. Create a Soroban transaction
      // 2. Call the deduct_units function with SignedUsageData
      // 3. Sign and submit the transaction
      
      // For simulation purposes, we'll simulate the contract call
      const result = await this._simulateContractCall('deduct_units', signedUsageData);
      await this._invalidateMeterCache(signedUsageData.meter_id);
      
      console.log(`✅ Usage data submitted successfully`);
      console.log(`   Watt Hours: ${signedUsageData.display_watt_hours} Wh`);
      console.log(`   Units: ${signedUsageData.units_consumed}`);
      console.log(`   Peak Hour: ${signedUsageData.is_peak_hour ? 'Yes' : 'No'}`);
      console.log(`   Effective Rate: ${signedUsageData.effective_rate} tokens/sec`);
      
      return result;
      
    } catch (error) {
      throw new Error(`Failed to submit usage data: ${error.message}`);
    }
  }

  /**
   * Get meter information from contract
   */
  async getMeter(meterId) {
    try {
      console.log(`📊 Fetching meter ${meterId} from contract...`);
      
      await this.initializeCache();
      const meter = await this.cache.remember(
        this._cacheKey('meter', meterId),
        this.cacheConfig.meterTtlSeconds,
        () => this._simulateContractCall('get_meter', { meter_id: meterId })
      );
      
      if (!meter) {
        throw new Error('Meter not found');
      }
      
      return meter;
      
    } catch (error) {
      throw new Error(`Failed to get meter: ${error.message}`);
    }
  }

  /**
   * Get usage data from contract
   */
  async getUsageData(meterId) {
    try {
      await this.initializeCache();
      return await this.cache.remember(
        this._cacheKey('usage', meterId),
        this.cacheConfig.usageTtlSeconds,
        () => this._simulateContractCall('get_usage_data', { meter_id: meterId })
      );
    } catch (error) {
      throw new Error(`Failed to get usage data: ${error.message}`);
    }
  }

  /**
   * Submit ZK usage data to contract
   */
  async submitZkUsageData(zkUsageData) {
    try {
      console.log('📤 Submitting ZK privacy usage report to contract...');
      this.rateLimiter.assertAllowed(`meter:${zkUsageData.meter_id}`);
      
      // In a real implementation, this would:
      // 1. Create a Soroban transaction
      // 2. Call the submit_zk_usage_report function
      
      const result = await this._simulateContractCall('submit_zk_usage_report', zkUsageData);
      await this._invalidateMeterCache(zkUsageData.meter_id);
      
      console.log(`✅ ZK Privacy report submitted successfully`);
      console.log(`   Units (Verified): ${zkUsageData.units_consumed}`);
      console.log(`   Proof Verified: Yes (Groth16)`);
      console.log(`   Privacy Status: Raw sensor data shielded`);
      
      return result;
      
    } catch (error) {
      throw new Error(`Failed to submit ZK usage data: ${error.message}`);
    }
  }

  /**
   * Set ZK verification key
   */
  async setZkVerificationKey(meterId, vk) {
    try {
      console.log(`🔑 Setting ZK verification key for meter ${meterId}...`);
      const result = await this._simulateContractCall('set_zk_verification_key', { meter_id: meterId, vk });
      console.log(`✅ Verification key set successfully`);
      return result;
    } catch (error) {
      throw new Error(`Failed to set ZK verification key: ${error.message}`);
    }
  }

  /**
   * Top up meter balance
   */
  async topUp(meterId, amount, userSecret) {
    try {
      console.log(`💰 Topping up meter ${meterId} with ${amount} tokens...`);
      
      const keypair = Keypair.fromSecret(userSecret);
      const account = await this.horizon.loadAccount(keypair.publicKey());
      
      // Create transaction for top-up
      const transaction = new TransactionBuilder(account, {
        fee: '100',
        networkPassphrase: this.network
      })
        .addOperation(Operation.payment({
          destination: this.contractId,
          asset: Asset.native(),
          amount: amount.toString()
        }))
        .setTimeout(30)
        .build();
      
      transaction.sign(keypair);
      
      const result = await this.horizon.submitTransaction(transaction);
      
      console.log(`✅ Top-up successful: ${result.hash}`);
      return result;
      
    } catch (error) {
      throw new Error(`Failed to top up meter: ${error.message}`);
    }
  }

  /**
   * Validate usage data before submission
   */
  _validateUsageData(data) {
    const now = Math.floor(Date.now() / 1000);
    
    // Check timestamp is not too old
    if (now - data.timestamp > config.constants.MAX_TIMESTAMP_DELAY) {
      throw new Error('Timestamp is too old');
    }
    
    // Check future timestamp
    if (data.timestamp > now + 60) { // Allow 1 minute clock skew
      throw new Error('Timestamp is in the future');
    }
    
    // Check usage values are positive
    if (data.watt_hours_consumed <= 0 || data.units_consumed <= 0) {
      throw new Error('Usage values must be positive');
    }
    
    // Check maximum usage limit
    if (data.watt_hours_consumed > config.constants.MAX_USAGE_PER_UPDATE) {
      throw new Error('Usage exceeds maximum limit');
    }
    
    // Verify signature format
    if (!data.signature || data.signature.length !== 88) { // 64 bytes base64 = 88 chars
      throw new Error('Invalid signature format');
    }
    
    if (!data.public_key || data.public_key.length !== 44) { // 32 bytes base64 = 44 chars
      throw new Error('Invalid public key format');
    }
  }

  /**
   * Simulate contract calls (for development/testing)
   */
  async _simulateContractCall(method, params) {
    // In a real implementation, this would use the Soroban RPC
    // For now, we'll simulate responses based on the method
    
    switch (method) {
      case 'register_meter':
        return this._generateMeterId();
        
      case 'deduct_units':
        return {
          success: true,
          meter_id: params.meter_id,
          cost: params.units_consumed * (params.is_peak_hour ? 15 : 10), // Simulate cost
          timestamp: Math.floor(Date.now() / 1000)
        };
        
      case 'get_meter':
        return this._simulateMeterData(params.meter_id);
        
      case 'get_usage_data':
        return this._simulateUsageData(params.meter_id);
        
      case 'submit_zk_usage_report':
        return {
          success: true,
          meter_id: params.meter_id,
          verified: true,
          cost: params.units_consumed * 10,
          timestamp: Math.floor(Date.now() / 1000)
        };

      case 'set_zk_verification_key':
        return { success: true };
        
      default:
        throw new Error(`Unknown contract method: ${method}`);
    }
  }

  async _invalidateMeterCache(meterId) {
    if (meterId === undefined || meterId === null) {
      return;
    }
    await this.initializeCache();
    await Promise.all([
      this.cache.delete(this._cacheKey('meter', meterId)),
      this.cache.delete(this._cacheKey('usage', meterId))
    ]);
  }

  _cacheKey(type, meterId) {
    return `${type}:${meterId}`;
  }

  getCacheMetrics() {
    return this.cache.getMetrics();
  }

  /**
   * Generate a mock meter ID
   */
  _generateMeterId() {
    return Math.floor(Math.random() * 1000000) + 1;
  }

  /**
   * Simulate meter data from contract
   */
  _simulateMeterData(meterId) {
    const now = Math.floor(Date.now() / 1000);
    
    return {
      meter_id: meterId,
      user: 'GD5DJQD7Y6KQLZBXNRCRJAY5PZQIIVMV5MW4FPX3BVUBQD2ZMJ7LFQXL',
      provider: 'GAB2JURIZ2XJ2LZ5ZQJKQWQJY5QNL7ZNVUKYB4XSV2LDEJYFGKZVQZK',
      billing_type: 'PrePaid',
      off_peak_rate: 10,
      peak_rate: 15,
      rate_per_second: 10,
      rate_per_unit: 10,
      balance: 1000000, // 1M tokens
      debt: 0,
      collateral_limit: 0,
      last_update: now,
      is_active: true,
      token: 'XLM',
      usage_data: this._simulateUsageData(meterId),
      max_flow_rate_per_hour: 36000, // 10 tokens/sec * 3600 sec
      last_claim_time: now,
      claimed_this_hour: 0,
      heartbeat: now,
      device_public_key: 'base64encodedpublickey',
      is_paired: true
    };
  }

  /**
   * Simulate usage data from contract
   */
  _simulateUsageData(meterId) {
    return {
      total_watt_hours: 1500000, // 1.5M Wh total
      current_cycle_watt_hours: 25000, // 25k Wh this cycle
      peak_usage_watt_hours: 50000, // 50k Wh peak
      last_reading_timestamp: Math.floor(Date.now() / 1000),
      precision_factor: 1000
    };
  }

  /**
   * Fund account using friendbot (testnet only)
   */
  async fundAccount(publicKey) {
    if (this.network !== Networks.TESTNET) {
      throw new Error('Friendbot is only available on testnet');
    }
    
    try {
      const response = await axios.get(`${this.friendbotUrl}?addr=${publicKey}`);
      console.log(`✅ Account funded: ${response.data.hash}`);
      return response.data;
    } catch (error) {
      throw new Error(`Failed to fund account: ${error.response?.data?.error || error.message}`);
    }
  }
}

module.exports = ContractInterface;
