/**
 * TypeScript bindings for IoT-Billing-Service Smart Contract
 * These types are auto-generated from the Soroban contract definitions
 * 
 * @fileoverview Type-safe interfaces for interacting with the IoT-Billing-Service contract
 * @version 1.0.0
 */

// ============================================================================
// Core Contract Types
// ============================================================================

/**
 * Billing type for meter configuration
 * - PrePaid: User pays in advance, balance decreases with usage
 * - PostPaid: User pays after usage, debt accumulates up to collateral limit
 */
export type BillingType = 'PrePaid' | 'PostPaid';

/**
 * Unique identifier for a meter
 */
export type MeterId = number;

/**
 * Stellar address (public key)
 */
export type StellarAddress = string;

/**
 * Token contract address on Stellar
 */
export type TokenAddress = string;

// ============================================================================
// Data Structures (mirrors contract structs)
// ============================================================================

/**
 * Usage data tracked by each meter
 */
export interface UsageData {
  /** Total watt-hours consumed since registration */
  total_watt_hours: bigint;
  /** Watt-hours consumed in current billing cycle */
  current_cycle_watt_hours: bigint;
  /** Peak watt-hours recorded in any single cycle */
  peak_usage_watt_hours: bigint;
  /** Timestamp of last reading (Unix epoch seconds) */
  last_reading_timestamp: bigint;
  /** Precision factor for accurate calculations */
  precision_factor: bigint;
}

/**
 * Meter configuration and state
 */
export interface Meter {
  /** Meter owner/user address */
  user: StellarAddress;
  /** Service provider address */
  provider: StellarAddress;
  /** Billing model (prepaid or postpaid) */
  billing_type: BillingType;
  /** Rate per second during off-peak hours (tokens/sec) */
  off_peak_rate: bigint;
  /** Rate per second during peak hours (tokens/sec) */
  peak_rate: bigint;
  /** Legacy rate field (deprecated, use off_peak_rate) */
  rate_per_second: bigint;
  /** Rate per unit of consumption */
  rate_per_unit: bigint;
  /** Current token balance (for prepaid) */
  balance: bigint;
  /** Accumulated debt (for postpaid) */
  debt: bigint;
  /** Collateral limit for postpaid meters */
  collateral_limit: bigint;
  /** Last update timestamp (Unix epoch seconds) */
  last_update: bigint;
  /** Whether the meter is currently active */
  is_active: boolean;
  /** Token contract address */
  token: TokenAddress;
  /** Usage statistics and tracking */
  usage_data: UsageData;
  /** Maximum flow rate per hour */
  max_flow_rate_per_hour: bigint;
  /** Last claim timestamp (Unix epoch seconds) */
  last_claim_time: bigint;
  /** Amount claimed in current hour window */
  claimed_this_hour: bigint;
  /** Heartbeat timestamp for liveness detection */
  heartbeat: bigint;
  /** Device public key for signature verification */
  device_public_key: string;
  /** Whether device pairing is complete */
  is_paired: boolean;
}

/**
 * Provider withdrawal window tracking
 */
export interface ProviderWithdrawalWindow {
  /** Amount withdrawn in current 24h window */
  daily_withdrawn: bigint;
  /** Window reset timestamp (Unix epoch seconds) */
  last_reset: bigint;
}

/**
 * Signed usage data submitted by devices
 */
export interface SignedUsageData {
  /** Meter identifier */
  meter_id: MeterId;
  /** Reading timestamp (Unix epoch seconds) */
  timestamp: bigint;
  /** Watt-hours consumed in this reading */
  watt_hours_consumed: bigint;
  /** Units consumed (derived from watt-hours) */
  units_consumed: bigint;
  /** Ed25519 signature (64 bytes as base64 or hex) */
  signature: string;
  /** Device public key (32 bytes as base64 or hex) */
  public_key: string;
}

/**
 * Usage report emitted by contract events
 */
export interface UsageReport {
  /** Meter identifier */
  meter_id: MeterId;
  /** Report timestamp (Unix epoch seconds) */
  timestamp: bigint;
  /** Watt-hours consumed */
  watt_hours_consumed: bigint;
  /** Units consumed */
  units_consumed: bigint;
}

/**
 * Price data from oracle
 */
export interface PriceData {
  /** Price value */
  price: bigint;
  /** Number of decimal places */
  decimals: number;
  /** Last update timestamp (Unix epoch seconds) */
  last_updated: bigint;
}

// ============================================================================
// Contract Method Parameters & Return Types
// ============================================================================

/**
 * Parameters for registering a new meter
 */
export interface RegisterMeterParams {
  /** User/Stellar account address */
  user: StellarAddress;
  /** Service provider address */
  provider: StellarAddress;
  /** Off-peak rate (base rate, tokens per second) */
  off_peak_rate: bigint;
  /** Token contract address */
  token: TokenAddress;
  /** Device public key (32 bytes) */
  device_public_key: string;
}

/**
 * Parameters for registering a meter with specific billing mode
 */
export interface RegisterMeterWithModeParams extends RegisterMeterParams {
  /** Billing type (PrePaid or PostPaid) */
  billing_type: BillingType;
}

/**
 * Result of meter registration
 */
export interface RegisterMeterResult {
  /** Assigned meter ID */
  meter_id: MeterId;
  /** Transaction hash */
  transaction_hash: string;
}

/**
 * Parameters for topping up meter balance
 */
export interface TopUpParams {
  /** Meter ID to top up */
  meter_id: MeterId;
  /** Amount to add (in token smallest units) */
  amount: bigint;
}

/**
 * Parameters for submitting signed usage data
 */
export interface DeductUnitsParams {
  /** Signed usage data from device */
  signed_data: SignedUsageData;
}

/**
 * Parameters for claiming earnings
 */
export interface ClaimParams {
  /** Meter ID to claim from */
  meter_id: MeterId;
}

/**
 * Parameters for withdrawing earnings
 */
export interface WithdrawEarningsParams {
  /** Meter ID to withdraw from */
  meter_id: MeterId;
  /** Amount in USD cents */
  amount_usd_cents: bigint;
}

/**
 * Parameters for updating usage data (user-initiated)
 */
export interface UpdateUsageParams {
  /** Meter ID */
  meter_id: MeterId;
  /** Watt-hours consumed */
  watt_hours_consumed: bigint;
}

/**
 * Parameters for setting maximum flow rate
 */
export interface SetMaxFlowRateParams {
  /** Meter ID */
  meter_id: MeterId;
  /** Maximum rate per hour */
  max_rate_per_hour: bigint;
}

/**
 * Parameters for transferring meter ownership
 */
export interface TransferMeterOwnershipParams {
  /** Meter ID to transfer */
  meter_id: MeterId;
  /** New user address */
  new_user: StellarAddress;
}

// ============================================================================
// Contract Interface
// ============================================================================

/**
 * IoT-Billing-Service Contract Interface
 * Provides type-safe methods for interacting with the smart contract
 */
export interface UtilityContract {
  // ==========================================================================
  // Read Methods (no auth required)
  // ==========================================================================
  
  /** Get minimum balance required to keep meter flowing */
  get_minimum_balance_to_flow(): Promise<bigint>;
  
  /** Get meter information by ID */
  get_meter(meter_id: MeterId): Promise<Meter | null>;
  
  /** Get usage data for a meter */
  get_usage_data(meter_id: MeterId): Promise<UsageData | null>;
  
  /** Get provider withdrawal window */
  get_provider_window(provider: StellarAddress): Promise<ProviderWithdrawalWindow | null>;
  
  /** Display watt-hours with proper precision */
  get_watt_hours_display(watt_hours: bigint, precision_factor: bigint): Promise<bigint>;
  
  /** Calculate expected depletion time */
  calculate_expected_depletion(meter_id: MeterId): Promise<bigint | null>;
  
  /** Check if meter is offline (>1 hour since heartbeat) */
  is_meter_offline(meter_id: MeterId): Promise<boolean>;
  
  /** Get current rate from oracle */
  get_current_rate(): Promise<PriceData | null>;
  
  /** Get provider's total pool across all meters */
  get_provider_total_pool(provider: StellarAddress): Promise<bigint>;
  
  // ==========================================================================
  // Write Methods (require auth/signature)
  // ==========================================================================
  
  /** Register a new meter (PrePaid by default) */
  register_meter(params: RegisterMeterParams): Promise<RegisterMeterResult>;
  
  /** Register a new meter with specific billing mode */
  register_meter_with_mode(params: RegisterMeterWithModeParams): Promise<RegisterMeterResult>;
  
  /** Top up meter balance */
  top_up(params: TopUpParams): Promise<void>;
  
  /** Submit signed usage data (deduct units) */
  deduct_units(params: DeductUnitsParams): Promise<void>;
  
  /** Claim earnings from a meter */
  claim(params: ClaimParams): Promise<void>;
  
  /** Withdraw earnings in XLM */
  withdraw_earnings(params: WithdrawEarningsParams): Promise<void>;
  
  /** Update usage data (user-initiated) */
  update_usage(params: UpdateUsageParams): Promise<void>;
  
  /** Reset cycle usage counter */
  reset_cycle_usage(meter_id: MeterId): Promise<void>;
  
  /** Set maximum flow rate for a meter */
  set_max_flow_rate(params: SetMaxFlowRateParams): Promise<void>;
  
  /** Update meter heartbeat */
  update_heartbeat(meter_id: MeterId): Promise<void>;
  
  /** Emergency shutdown of a meter */
  emergency_shutdown(meter_id: MeterId): Promise<void>;
  
  /** Transfer meter ownership to new user */
  transfer_meter_ownership(params: TransferMeterOwnershipParams): Promise<void>;
  
  // ==========================================================================
  // Admin Methods (restricted access)
  // ==========================================================================
  
  /** Set price oracle address */
  set_oracle(oracle_address: StellarAddress): Promise<void>;
  
  /** Set maintenance wallet and fee configuration */
  set_maintenance_config(wallet: StellarAddress, fee_bps: bigint): Promise<void>;
  
  /** Add a supported token */
  add_supported_token(token: TokenAddress): Promise<void>;
  
  /** Remove a supported token */
  remove_supported_token(token: TokenAddress): Promise<void>;
  
  // ==========================================================================
  // Pairing Methods (device authentication)
  // ==========================================================================
  
  /** Initiate device pairing, returns challenge */
  initiate_pairing(meter_id: MeterId): Promise<string>;
  
  /** Complete device pairing with signature */
  complete_pairing(meter_id: MeterId, signature: string): Promise<void>;
  
  /** Update device public key */
  update_device_public_key(meter_id: MeterId, new_public_key: string): Promise<void>;
}

// ============================================================================
// Event Types (emitted by contract)
// ============================================================================

/**
 * Meter activated event
 */
export interface ActiveEvent {
  event_type: 'Active';
  meter_id: MeterId;
  timestamp: bigint;
}

/**
 * Meter deactivated event
 */
export interface InactiveEvent {
  event_type: 'Inactive';
  meter_id: MeterId;
  timestamp: bigint;
}

/**
 * Pairing initiated event
 */
export interface PairInitEvent {
  event_type: 'PairInit';
  meter_id: MeterId;
  challenge: string;
}

/**
 * Pairing completed event
 */
export interface PairCompleteEvent {
  event_type: 'PairComplete';
  meter_id: MeterId;
  signature: string;
}

/**
 * Usage reported event
 */
export interface UsageReportedEvent {
  event_type: 'UsageReported';
  meter_id: MeterId;
  units_consumed: bigint;
  cost: bigint;
}

/**
 * Token top-up event
 */
export interface TokenUpEvent {
  event_type: 'TokenUp';
  meter_id: MeterId;
  xlm_amount: bigint;
  usd_cents: bigint;
}

/**
 * USD to XLM conversion event
 */
export interface USDtoXLMEvent {
  event_type: 'USDtoXLM';
  meter_id: MeterId;
  usd_cents: bigint;
  xlm_amount: bigint;
}

/**
 * Meter ownership transferred event
 */
export interface TransferEvent {
  event_type: 'Transfer';
  meter_id: MeterId;
  old_user: StellarAddress;
  new_user: StellarAddress;
}

/**
 * Union type of all contract events
 */
export type ContractEvent = 
  | ActiveEvent
  | InactiveEvent
  | PairInitEvent
  | PairCompleteEvent
  | UsageReportedEvent
  | TokenUpEvent
  | USDtoXLMEvent
  | TransferEvent;

// ============================================================================
// Error Types
// ============================================================================

/**
 * Contract error codes
 */
export enum ContractErrorCode {
  MeterNotFound = 1,
  OracleNotSet = 2,
  WithdrawalLimitExceeded = 3,
  PriceConversionFailed = 4,
  InvalidTokenAmount = 5,
  InvalidUsageValue = 6,
  UsageExceedsLimit = 7,
  InvalidPrecisionFactor = 8,
  InvalidSignature = 9,
  PublicKeyMismatch = 10,
  TimestampTooOld = 11,
  PairingAlreadyComplete = 12,
  ChallengeNotFound = 13,
  InvalidPairingSignature = 14,
  MeterNotPaired = 15,
}

/**
 * Custom error for contract-related errors
 */
export class ContractError extends Error {
  constructor(
    message: string,
    public code: ContractErrorCode,
    public transactionHash?: string
  ) {
    super(message);
    this.name = 'ContractError';
  }
}

// ============================================================================
// Constants (from contract)
// ============================================================================

export const CONTRACT_CONSTANTS = {
  /** Minimum balance to keep meter flowing (tokens) */
  MINIMUM_BALANCE_TO_FLOW: BigInt(500),
  
  /** Hour in seconds */
  HOUR_IN_SECONDS: 3600,
  
  /** Day in seconds */
  DAY_IN_SECONDS: 86400,
  
  /** Daily withdrawal percentage (10%) */
  DAILY_WITHDRAWAL_PERCENT: BigInt(10),
  
  /** Maximum usage per update (kWh) */
  MAX_USAGE_PER_UPDATE: BigInt('1000000000000'),
  
  /** Minimum precision factor */
  MIN_PRECISION_FACTOR: BigInt(1),
  
  /** Maximum timestamp delay for signatures (seconds) */
  MAX_TIMESTAMP_DELAY: 300,
  
  /** Peak hour start (18:00 UTC in seconds) */
  PEAK_HOUR_START: 64800,
  
  /** Peak hour end (21:00 UTC in seconds) */
  PEAK_HOUR_END: 75600,
  
  /** Peak rate multiplier numerator (3/2 = 1.5x) */
  PEAK_RATE_MULTIPLIER: BigInt(3),
  
  /** Rate precision divisor */
  RATE_PRECISION: BigInt(2),
  
  /** XLM precision (10^7 for 7 decimal places) */
  XLM_PRECISION: BigInt(10000000),
  
  /** XLM minimum increment (1 stroop) */
  XLM_MINIMUM_INCREMENT: BigInt(1),
} as const;

// ============================================================================
// Helper Type Guards
// ============================================================================

/**
 * Type guard to check if a value is a valid BillingType
 */
export function isBillingType(value: unknown): value is BillingType {
  return value === 'PrePaid' || value === 'PostPaid';
}

/**
 * Type guard to check if a string is a valid Stellar address
 */
export function isStellarAddress(value: string): boolean {
  return /^G[A-Z2-7]{55}$/.test(value);
}

/**
 * Type guard to check if an object is a valid Meter
 */
export function isMeter(value: unknown): value is Meter {
  if (!value || typeof value !== 'object') return false;
  const meter = value as Record<string, unknown>;
  return (
    typeof meter.user === 'string' &&
    typeof meter.provider === 'string' &&
    isBillingType(meter.billing_type) &&
    typeof meter.is_active === 'boolean' &&
    typeof meter.is_paired === 'boolean'
  );
}
