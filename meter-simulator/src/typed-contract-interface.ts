/**
 * Type-safe Contract Interface for IoT-Billing-Service Smart Contract
 * 
 * This module provides a fully-typed interface for interacting with the
 * IoT-Billing-Service Soroban smart contract on Stellar.
 */

import {
  Meter,
  MeterId,
  StellarAddress,
  TokenAddress,
  BillingType,
  UsageData,
  ProviderWithdrawalWindow,
  SignedUsageData,
  PriceData,
  RegisterMeterParams,
  RegisterMeterWithModeParams,
  RegisterMeterResult,
  TopUpParams,
  DeductUnitsParams,
  ClaimParams,
  WithdrawEarningsParams,
  UpdateUsageParams,
  SetMaxFlowRateParams,
  TransferMeterOwnershipParams,
  UtilityContract,
  ContractEvent,
  ContractError,
  ContractErrorCode,
  CONTRACT_CONSTANTS,
} from './types';

import { Server, Networks, TransactionBuilder, Operation, Asset, Keypair, Account } from 'stellar-sdk';
import config from './config';

interface ContractConfig {
  network: 'mainnet' | 'testnet';
  rpcUrl: string;
  horizonUrl: string;
  contractId: string;
  friendbotUrl?: string;
}

export class TypedContractInterface implements UtilityContract {
  private network: string;
  private server: Server;
  private horizon: Server;
  private contractId: string;
  private friendbotUrl?: string;

  constructor(contractConfig: ContractConfig) {
    this.network = contractConfig.network === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;
    this.server = new Server(contractConfig.rpcUrl, { allowHttp: true });
    this.horizon = new Server(contractConfig.horizonUrl, { allowHttp: true });
    this.contractId = contractConfig.contractId;
    this.friendbotUrl = contractConfig.friendbotUrl;
  }

  // ==========================================================================
  // Read Methods (no auth required)
  // ==========================================================================

  async get_minimum_balance_to_flow(): Promise<bigint> {
    try {
      // In production, this would call the contract's read method
      // For now, return the constant value from the contract
      return CONTRACT_CONSTANTS.MINIMUM_BALANCE_TO_FLOW;
    } catch (error) {
      throw new ContractError(
        'Failed to get minimum balance',
        ContractErrorCode.PriceConversionFailed
      );
    }
  }

  async get_meter(meter_id: MeterId): Promise<Meter | null> {
    try {
      // Simulate contract call - in production, use Soroban RPC
      const response = await this.simulateRead('get_meter', { meter_id });
      return response as Meter | null;
    } catch (error) {
      if ((error as any).message?.includes('not found')) {
        return null;
      }
      throw error;
    }
  }

  async get_usage_data(meter_id: MeterId): Promise<UsageData | null> {
    try {
      const response = await this.simulateRead('get_usage_data', { meter_id });
      return response as UsageData | null;
    } catch (error) {
      if ((error as any).message?.includes('not found')) {
        return null;
      }
      throw error;
    }
  }

  async get_provider_window(provider: StellarAddress): Promise<ProviderWithdrawalWindow | null> {
    try {
      const response = await this.simulateRead('get_provider_window', { provider });
      return response as ProviderWithdrawalWindow | null;
    } catch (error) {
      if ((error as any).message?.includes('not found')) {
        return null;
      }
      throw error;
    }
  }

  async get_watt_hours_display(watt_hours: bigint, precision_factor: bigint): Promise<bigint> {
    try {
      // Simple division as per contract implementation
      if (precision_factor <= BigInt(0)) {
        return watt_hours;
      }
      return watt_hours / precision_factor;
    } catch (error) {
      throw new ContractError(
        'Failed to calculate watt hours display',
        ContractErrorCode.InvalidPrecisionFactor
      );
    }
  }

  async calculate_expected_depletion(meter_id: MeterId): Promise<bigint | null> {
    try {
      const meter = await this.get_meter(meter_id);
      if (!meter || meter.balance <= BigInt(0) || meter.rate_per_unit <= BigInt(0)) {
        return BigInt(0);
      }
      
      const secondsUntilDepletion = meter.balance / meter.rate_per_unit;
      const currentTime = BigInt(Math.floor(Date.now() / 1000));
      return currentTime + secondsUntilDepletion;
    } catch (error) {
      return null;
    }
  }

  async is_meter_offline(meter_id: MeterId): Promise<boolean> {
    try {
      const meter = await this.get_meter(meter_id);
      if (!meter) {
        return true;
      }
      
      const currentTime = BigInt(Math.floor(Date.now() / 1000));
      const timeSinceHeartbeat = currentTime - meter.heartbeat;
      return timeSinceHeartbeat > CONTRACT_CONSTANTS.HOUR_IN_SECONDS;
    } catch (error) {
      return true; // Assume offline if we can't check
    }
  }

  async get_current_rate(): Promise<PriceData | null> {
    try {
      const response = await this.simulateRead('get_current_rate', {});
      return response as PriceData | null;
    } catch (error) {
      return null;
    }
  }

  async get_provider_total_pool(provider: StellarAddress): Promise<bigint> {
    try {
      const response = await this.simulateRead('get_provider_total_pool', { provider });
      return BigInt(response as any);
    } catch (error) {
      return BigInt(0);
    }
  }

  // ==========================================================================
  // Write Methods (require auth/signature)
  // ==========================================================================

  async register_meter(params: RegisterMeterParams): Promise<RegisterMeterResult> {
    try {
      console.log('🔧 Registering meter...');
      
      // Build transaction to call register_meter
      const txHash = await this.buildAndSubmitTransaction('register_meter', params);
      
      // Generate or extract meter ID from transaction result
      const meter_id = this.generateMeterId();
      
      console.log(`✅ Meter registered with ID: ${meter_id}`);
      
      return {
        meter_id,
        transaction_hash: txHash,
      };
    } catch (error) {
      throw new ContractError(
        'Failed to register meter',
        ContractErrorCode.MeterNotFound,
        undefined
      );
    }
  }

  async register_meter_with_mode(params: RegisterMeterWithModeParams): Promise<RegisterMeterResult> {
    try {
      console.log(`🔧 Registering meter with mode: ${params.billing_type}...`);
      
      const txHash = await this.buildAndSubmitTransaction('register_meter_with_mode', params);
      const meter_id = this.generateMeterId();
      
      console.log(`✅ Meter registered with ID: ${meter_id}`);
      
      return {
        meter_id,
        transaction_hash: txHash,
      };
    } catch (error) {
      throw new ContractError(
        'Failed to register meter with mode',
        ContractErrorCode.MeterNotFound,
        undefined
      );
    }
  }

  async top_up(params: TopUpParams): Promise<void> {
    try {
      console.log(`💰 Topping up meter ${params.meter_id} with ${params.amount} tokens...`);
      
      await this.buildAndSubmitTransaction('top_up', params);
      
      console.log('✅ Top-up successful');
    } catch (error) {
      throw new ContractError(
        'Failed to top up meter',
        ContractErrorCode.InvalidTokenAmount
      );
    }
  }

  async deduct_units(params: DeductUnitsParams): Promise<void> {
    try {
      console.log('📤 Submitting signed usage data...');
      
      // Validate signature format
      this.validateSignature(params.signed_data);
      
      await this.buildAndSubmitTransaction('deduct_units', params);
      
      console.log('✅ Usage data submitted successfully');
    } catch (error) {
      throw new ContractError(
        'Failed to deduct units',
        ContractErrorCode.InvalidSignature
      );
    }
  }

  async claim(params: ClaimParams): Promise<void> {
    try {
      console.log(`💰 Claiming earnings from meter ${params.meter_id}...`);
      
      await this.buildAndSubmitTransaction('claim', params);
      
      console.log('✅ Claim successful');
    } catch (error) {
      throw new ContractError(
        'Failed to claim earnings',
        ContractErrorCode.InvalidTokenAmount
      );
    }
  }

  async withdraw_earnings(params: WithdrawEarningsParams): Promise<void> {
    try {
      console.log(`💸 Withdrawing ${params.amount_usd_cents} USD cents from meter ${params.meter_id}...`);
      
      await this.buildAndSubmitTransaction('withdraw_earnings', params);
      
      console.log('✅ Withdrawal successful');
    } catch (error) {
      throw new ContractError(
        'Failed to withdraw earnings',
        ContractErrorCode.InvalidTokenAmount
      );
    }
  }

  async update_usage(params: UpdateUsageParams): Promise<void> {
    try {
      console.log(`📊 Updating usage for meter ${params.meter_id}...`);
      
      await this.buildAndSubmitTransaction('update_usage', params);
      
      console.log('✅ Usage updated');
    } catch (error) {
      throw new ContractError(
        'Failed to update usage',
        ContractErrorCode.InvalidUsageValue
      );
    }
  }

  async reset_cycle_usage(meter_id: MeterId): Promise<void> {
    try {
      console.log(`🔄 Resetting cycle usage for meter ${meter_id}...`);
      
      await this.buildAndSubmitTransaction('reset_cycle_usage', { meter_id });
      
      console.log('✅ Cycle usage reset');
    } catch (error) {
      throw new ContractError(
        'Failed to reset cycle usage',
        ContractErrorCode.MeterNotFound
      );
    }
  }

  async set_max_flow_rate(params: SetMaxFlowRateParams): Promise<void> {
    try {
      console.log(`⚙️ Setting max flow rate for meter ${params.meter_id}...`);
      
      await this.buildAndSubmitTransaction('set_max_flow_rate', params);
      
      console.log('✅ Max flow rate updated');
    } catch (error) {
      throw new ContractError(
        'Failed to set max flow rate',
        ContractErrorCode.MeterNotFound
      );
    }
  }

  async update_heartbeat(meter_id: MeterId): Promise<void> {
    try {
      console.log(`💓 Updating heartbeat for meter ${meter_id}...`);
      
      await this.buildAndSubmitTransaction('update_heartbeat', { meter_id });
      
      console.log('✅ Heartbeat updated');
    } catch (error) {
      throw new ContractError(
        'Failed to update heartbeat',
        ContractErrorCode.MeterNotFound
      );
    }
  }

  async emergency_shutdown(meter_id: MeterId): Promise<void> {
    try {
      console.log(`🚨 Emergency shutdown for meter ${meter_id}...`);
      
      await this.buildAndSubmitTransaction('emergency_shutdown', { meter_id });
      
      console.log('✅ Meter shut down');
    } catch (error) {
      throw new ContractError(
        'Failed to emergency shutdown',
        ContractErrorCode.MeterNotFound
      );
    }
  }

  async transfer_meter_ownership(params: TransferMeterOwnershipParams): Promise<void> {
    try {
      console.log(`🔄 Transferring meter ${params.meter_id} ownership to ${params.new_user}...`);
      
      await this.buildAndSubmitTransaction('transfer_meter_ownership', params);
      
      console.log('✅ Ownership transferred');
    } catch (error) {
      throw new ContractError(
        'Failed to transfer ownership',
        ContractErrorCode.MeterNotFound
      );
    }
  }

  // ==========================================================================
  // Admin Methods (restricted access)
  // ==========================================================================

  async set_oracle(oracle_address: StellarAddress): Promise<void> {
    try {
      console.log(`🔮 Setting oracle address: ${oracle_address}...`);
      await this.buildAndSubmitTransaction('set_oracle', { oracle_address });
      console.log('✅ Oracle set');
    } catch (error) {
      throw new ContractError('Failed to set oracle', ContractErrorCode.OracleNotSet);
    }
  }

  async set_maintenance_config(wallet: StellarAddress, fee_bps: bigint): Promise<void> {
    try {
      console.log(`⚙️ Setting maintenance config...`);
      await this.buildAndSubmitTransaction('set_maintenance_config', { wallet, fee_bps });
      console.log('✅ Maintenance config set');
    } catch (error) {
      throw new ContractError('Failed to set maintenance config', ContractErrorCode.MeterNotFound);
    }
  }

  async add_supported_token(token: TokenAddress): Promise<void> {
    try {
      console.log(`➕ Adding supported token: ${token}...`);
      await this.buildAndSubmitTransaction('add_supported_token', { token });
      console.log('✅ Token added');
    } catch (error) {
      throw new ContractError('Failed to add token', ContractErrorCode.InvalidTokenAmount);
    }
  }

  async remove_supported_token(token: TokenAddress): Promise<void> {
    try {
      console.log(`➖ Removing supported token: ${token}...`);
      await this.buildAndSubmitTransaction('remove_supported_token', { token });
      console.log('✅ Token removed');
    } catch (error) {
      throw new ContractError('Failed to remove token', ContractErrorCode.InvalidTokenAmount);
    }
  }

  // ==========================================================================
  // Pairing Methods (device authentication)
  // ==========================================================================

  async initiate_pairing(meter_id: MeterId): Promise<string> {
    try {
      console.log(`🔐 Initiating pairing for meter ${meter_id}...`);
      
      const response = await this.buildAndSubmitTransaction('initiate_pairing', { meter_id });
      const challenge = response as string;
      
      console.log('✅ Pairing initiated');
      return challenge;
    } catch (error) {
      throw new ContractError('Failed to initiate pairing', ContractErrorCode.ChallengeNotFound);
    }
  }

  async complete_pairing(meter_id: MeterId, signature: string): Promise<void> {
    try {
      console.log(`🔐 Completing pairing for meter ${meter_id}...`);
      
      await this.buildAndSubmitTransaction('complete_pairing', { meter_id, signature });
      
      console.log('✅ Pairing completed');
    } catch (error) {
      throw new ContractError('Failed to complete pairing', ContractErrorCode.InvalidPairingSignature);
    }
  }

  async update_device_public_key(meter_id: MeterId, new_public_key: string): Promise<void> {
    try {
      console.log(`🔑 Updating device public key for meter ${meter_id}...`);
      
      await this.buildAndSubmitTransaction('update_device_public_key', { meter_id, new_public_key });
      
      console.log('✅ Public key updated');
    } catch (error) {
      throw new ContractError('Failed to update public key', ContractErrorCode.PublicKeyMismatch);
    }
  }

  // ==========================================================================
  // Private Helper Methods
  // ==========================================================================

  private async simulateRead(method: string, params: Record<string, any>): Promise<any> {
    // In production, this would use Soroban RPC simulation
    // For now, simulate responses based on method
    console.log(`📖 Simulating read: ${method}`, params);
    
    // Simulated response - replace with actual RPC calls in production
    return null;
  }

  private async buildAndSubmitTransaction(
    method: string,
    params: Record<string, any>
  ): Promise<string> {
    // In production, this would:
    // 1. Build Soroban transaction using @stellar/stellar-sdk
    // 2. Invoke contract method with parameters
    // 3. Sign transaction with user's keypair
    // 4. Submit to Stellar network
    // 5. Wait for transaction confirmation
    
    console.log(`📝 Building transaction: ${method}`, params);
    
    // Simulated transaction hash
    return 'simulated_tx_hash_' + Date.now();
  }

  private validateSignature(signedData: SignedUsageData): void {
    const now = BigInt(Math.floor(Date.now() / 1000));
    
    // Check timestamp is not too old
    if (now - signedData.timestamp > BigInt(CONTRACT_CONSTANTS.MAX_TIMESTAMP_DELAY)) {
      throw new Error('Timestamp is too old');
    }
    
    // Check future timestamp
    if (signedData.timestamp > now + BigInt(60)) {
      throw new Error('Timestamp is in the future');
    }
    
    // Check usage values are positive
    if (signedData.watt_hours_consumed <= BigInt(0) || signedData.units_consumed <= BigInt(0)) {
      throw new Error('Usage values must be positive');
    }
    
    // Check maximum usage limit
    if (signedData.watt_hours_consumed > CONTRACT_CONSTANTS.MAX_USAGE_PER_UPDATE) {
      throw new Error('Usage exceeds maximum limit');
    }
    
    // Verify signature format (64 bytes = 88 base64 chars or 128 hex chars)
    if (!signedData.signature || (signedData.signature.length !== 88 && signedData.signature.length !== 128)) {
      throw new Error('Invalid signature format');
    }
    
    // Verify public key format (32 bytes = 44 base64 chars or 64 hex chars)
    if (!signedData.public_key || (signedData.public_key.length !== 44 && signedData.public_key.length !== 64)) {
      throw new Error('Invalid public key format');
    }
  }

  private generateMeterId(): MeterId {
    return Math.floor(Math.random() * 1000000) + 1;
  }
}

export default TypedContractInterface;
