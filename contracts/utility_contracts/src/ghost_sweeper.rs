use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, BytesN, Env, Symbol, Vec,
};

use crate::{ContractError, DataKey, ContinuousFlow, StreamStatus};

/// Issue #262: Ledger Rent Sweeper for Depleted "Ghost" Devices
/// Prunes abandoned streams after 90 days to reduce ledger footprint

/// Number of days a stream can remain with zero balance before being eligible for pruning
pub const GHOST_STREAM_THRESHOLD_DAYS: u64 = 90;

/// Number of seconds in a day
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// Gas bounty percentage for relayers (in basis points, 1000 = 10%)
pub const RELAYER_BOUNTY_BPS: u32 = 500; // 5%

/// Minimum gas bounty to incentivize relayers
pub const MIN_GAS_BOUNTY: i128 = 100; // Minimum 100 tokens

/// Ghost stream pruning event
#[contracttype]
#[derive(Clone)]
pub struct GhostStreamPruned {
    pub stream_id: u64,
    pub device_mac: BytesN<32>,
    pub provider: Address,
    pub payer: Address,
    pub zero_balance_duration_days: u64,
    pub bytes_reclaimed: u64,
    pub archive_hash: BytesN<32>,
    pub gas_bounty_paid: i128,
    pub relayer: Address,
    pub timestamp: u64,
}

/// Archive hash for historical integrity
#[contracttype]
#[derive(Clone)]
pub struct StreamArchive {
    /// Original stream ID
    pub stream_id: u64,
    /// Device MAC address
    pub device_mac: BytesN<32>,
    /// Provider address
    pub provider: Address,
    /// Payer address
    pub payer: Address,
    /// Creation timestamp
    pub created_timestamp: u64,
    /// Final balance before pruning
    pub final_balance: i128,
    /// Total amount streamed over lifetime
    pub total_streamed: i128,
    /// Pruning timestamp
    pub pruned_timestamp: u64,
    /// Reason for pruning
    pub prune_reason: PruneReason,
    /// Cryptographic hash of all stream data
    pub data_hash: BytesN<32>,
}

/// Reason for stream pruning
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PruneReason {
    /// Stream had zero balance for threshold period
    ZeroBalanceExpired = 0,
    /// Stream was inactive for threshold period
    InactiveExpired = 1,
    /// Device was blacklisted/compromised
    DeviceCompromised = 2,
    /// Provider requested cleanup
    ProviderRequested = 3,
}

/// Ghost stream candidate metadata
#[contracttype]
#[derive(Clone)]
pub struct GhostStreamCandidate {
    pub stream_id: u64,
    pub device_mac: BytesN<32>,
    pub provider: Address,
    pub zero_balance_since: u64,
    days_zero_balance: u64,
    last_activity: u64,
    estimated_storage_bytes: u64,
    is_eligible_for_pruning: bool,
}

/// Sweeper operation result
#[contracttype]
#[derive(Clone)]
pub struct SweeperResult {
    pub streams_pruned: u32,
    pub total_bytes_reclaimed: u64,
    pub total_gas_bounty_paid: i128,
    pub operation_duration_seconds: u64,
    pub relayer: Address,
}

/// Ghost Stream Sweeper contract
#[contract]
pub struct GhostSweeper;

#[contractimpl]
impl GhostSweeper {
    /// Prune a single ghost stream that has been zero balance for over 90 days
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `stream_id` - ID of the stream to prune
    /// * `relayer` - Address of the relayer performing the cleanup
    /// 
    /// # Returns
    /// Gas bounty paid to the relayer
    /// 
    /// # Errors
    /// * `ContractError::MeterNotFound` - if stream doesn't exist
    /// * `ContractError::StreamNotEligibleForPruning` - if stream not eligible
    /// * `ContractError::StreamHasPendingBuffer` - if stream has pending buffer
    pub fn prune_ghost_stream(env: Env, stream_id: u64, relayer: Address) -> i128 {
        let current_time = env.ledger().timestamp();
        
        // Get stream data
        let stream_key = DataKey::ContinuousFlow(stream_id);
        let stream: ContinuousFlow = env
            .storage()
            .persistent()
            .get(&stream_key)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::MeterNotFound));

        // Check eligibility for pruning
        let eligibility = Self::check_pruning_eligibility(&env, &stream, current_time);
        if !eligibility.is_eligible {
            panic_with_error!(env, ContractError::StreamNotEligibleForPruning);
        }

        // Ensure stream has no pending buffer
        if stream.buffer_balance > 0 {
            panic_with_error!(env, ContractError::StreamHasPendingBuffer);
        }

        // Calculate gas bounty
        let storage_bytes = Self::estimate_stream_storage_size(&stream);
        let gas_bounty = Self::calculate_gas_bounty(storage_bytes);

        // Create archive hash for historical integrity
        let archive_hash = Self::create_stream_archive(&env, &stream, current_time);

        // Remove heavy stream metadata
        env.storage()
            .persistent()
            .remove(&stream_key);

        // Remove MAC address mapping
        if stream.device_mac_pubkey != BytesN::from_array(&[0u8; 32]) {
            let mac_key = DataKey::DeviceHash(stream.device_mac_pubkey.clone());
            env.storage()
                .persistent()
                .remove(&mac_key);
        }

        // Store lightweight archive hash
        let archive_key = DataKey::StreamArchive(stream_id);
        env.storage()
            .persistent()
            .set(&archive_key, &archive_hash);

        // Update global statistics
        Self::update_sweeper_statistics(&env, 1, storage_bytes, gas_bounty);

        // Emit pruning event
        let prune_event = GhostStreamPruned {
            stream_id,
            device_mac: stream.device_mac_pubkey.clone(),
            provider: stream.provider.clone(),
            payer: stream.payer.clone(),
            zero_balance_duration_days: eligibility.days_zero_balance,
            bytes_reclaimed: storage_bytes,
            archive_hash: archive_hash.data_hash,
            gas_bounty_paid: gas_bounty,
            relayer: relayer.clone(),
            timestamp: current_time,
        };

        env.events().publish(
            (symbol_short!("GSPrune"),),
            prune_event,
        );

        gas_bounty
    }

    /// Batch prune multiple ghost streams
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `stream_ids` - Vector of stream IDs to prune
    /// * `relayer` - Address of the relayer performing the cleanup
    /// 
    /// # Returns
    /// Summary of the sweeping operation
    pub fn batch_prune_ghost_streams(env: Env, stream_ids: Vec<u64>, relayer: Address) -> SweeperResult {
        let start_time = env.ledger().timestamp();
        let mut streams_pruned = 0u32;
        let mut total_bytes_reclaimed = 0u64;
        let mut total_gas_bounty = 0i128;

        for stream_id in stream_ids.iter() {
            let bounty = Self::prune_ghost_stream(env.clone(), stream_id, relayer.clone());
            streams_pruned += 1;
            total_gas_bounty += bounty;
            total_bytes_reclaimed += 500;
        }

        let operation_duration = env.ledger().timestamp() - start_time;

        let sweeper_result = SweeperResult {
            streams_pruned,
            total_bytes_reclaimed,
            total_gas_bounty_paid: total_gas_bounty,
            operation_duration_seconds: operation_duration,
            relayer,
        };

        // Emit batch operation event
        env.events().publish(
            (symbol_short!("BGSweep"),),
            sweeper_result.clone(),
        );

        sweeper_result
    }

    /// Get list of ghost stream candidates
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `limit` - Maximum number of candidates to return
    /// 
    /// # Returns
    /// Vector of ghost stream candidates
    pub fn get_ghost_stream_candidates(env: Env, limit: u32) -> Vec<GhostStreamCandidate> {
        let mut candidates = Vec::new(&env);
        let current_time = env.ledger().timestamp();
        
        // This is a simplified implementation
        // In production, you would iterate through all streams or use an index
        let stream_ids = Self::get_all_stream_ids(&env, limit);
        
        for stream_id in stream_ids.iter() {
            let stream_key = DataKey::ContinuousFlow(stream_id);
            if let Some(stream) = env.storage().persistent().get::<DataKey, ContinuousFlow>(&stream_key) {
                let eligibility = Self::check_pruning_eligibility(&env, &stream, current_time);
                
                if eligibility.is_eligible || eligibility.days_zero_balance > 30 {
                    let candidate = GhostStreamCandidate {
                        stream_id,
                        device_mac: stream.device_mac_pubkey.clone(),
                        provider: stream.provider.clone(),
                        zero_balance_since: eligibility.zero_balance_since,
                        days_zero_balance: eligibility.days_zero_balance,
                        last_activity: stream.last_flow_timestamp,
                        estimated_storage_bytes: Self::estimate_stream_storage_size(&stream),
                        is_eligible_for_pruning: eligibility.is_eligible,
                    };
                    
                    candidates.push_back(candidate);
                }
            }
        }

        candidates
    }

    /// Check if a stream is eligible for pruning
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `stream_id` - Stream ID to check
    /// 
    /// # Returns
    /// Ghost stream candidate if eligible, None otherwise
    pub fn check_stream_eligibility(env: Env, stream_id: u64) -> Option<GhostStreamCandidate> {
        let current_time = env.ledger().timestamp();
        
        let stream_key = DataKey::ContinuousFlow(stream_id);
        let stream: ContinuousFlow = env
            .storage()
            .persistent()
            .get(&stream_key)?;

        let eligibility = Self::check_pruning_eligibility(&env, &stream, current_time);
        
        Some(GhostStreamCandidate {
            stream_id,
            device_mac: stream.device_mac_pubkey.clone(),
            provider: stream.provider.clone(),
            zero_balance_since: eligibility.zero_balance_since,
            days_zero_balance: eligibility.days_zero_balance,
            last_activity: stream.last_flow_timestamp,
            estimated_storage_bytes: Self::estimate_stream_storage_size(&stream),
            is_eligible_for_pruning: eligibility.is_eligible,
        })
    }

    /// Get stream archive information
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `stream_id` - Stream ID
    /// 
    /// # Returns
    /// Stream archive if exists, None otherwise
    pub fn get_stream_archive(env: Env, stream_id: u64) -> Option<StreamArchive> {
        env.storage()
            .persistent()
            .get(&DataKey::StreamArchive(stream_id))
    }

    /// Get global sweeper statistics
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// 
    /// # Returns
    /// Sweeper statistics
    pub fn get_sweeper_statistics(env: Env) -> SweeperStatistics {
        env.storage()
            .persistent()
            .get(&DataKey::SweeperStatistics)
            .unwrap_or_else(|| SweeperStatistics {
                total_streams_pruned: 0,
                total_bytes_reclaimed: 0,
                total_gas_bounty_paid: 0,
                last_sweep_timestamp: 0,
                total_sweep_operations: 0,
            })
    }
}

impl GhostSweeper {
    /// Check if a stream is eligible for pruning
    fn check_pruning_eligibility(
        env: &Env,
        stream: &ContinuousFlow,
        current_time: u64,
    ) -> PruningEligibility {
        // Stream must be depleted or inactive
        if stream.status != StreamStatus::Depleted && stream.accumulated_balance > 0 {
            return PruningEligibility {
                is_eligible: false,
                days_zero_balance: 0,
                zero_balance_since: 0,
            };
        }

        // Calculate days since last activity
        let days_since_activity = if stream.last_flow_timestamp > 0 {
            (current_time - stream.last_flow_timestamp) / SECONDS_PER_DAY
        } else {
            (current_time - stream.created_timestamp) / SECONDS_PER_DAY
        };

        // Check if zero balance threshold is met
        let is_eligible = days_since_activity >= GHOST_STREAM_THRESHOLD_DAYS;

        PruningEligibility {
            is_eligible,
            days_zero_balance: days_since_activity,
            zero_balance_since: stream.last_flow_timestamp,
        }
    }

    /// Estimate storage size of a stream
    fn estimate_stream_storage_size(stream: &ContinuousFlow) -> u64 {
        // Base stream size + MAC address mapping + overhead
        let base_size = core::mem::size_of::<ContinuousFlow>() as u64;
        let mac_mapping_size = if stream.device_mac_pubkey != BytesN::from_array(&[0u8; 32]) {
            64 // Estimated size of MAC address mapping
        } else {
            0
        };
        
        base_size + mac_mapping_size + 100 // Add overhead for storage
    }

    /// Calculate gas bounty for relayer
    fn calculate_gas_bounty(storage_bytes: u64) -> i128 {
        // Bounty is proportional to storage reclaimed, with minimum
        let base_bounty = (storage_bytes as i128) * (RELAYER_BOUNTY_BPS as i128) / 10000;
        base_bounty.max(MIN_GAS_BOUNTY)
    }

    /// Create stream archive for historical integrity
    fn create_stream_archive(env: &Env, stream: &ContinuousFlow, current_time: u64) -> StreamArchive {
        // Create hash of all stream data
        let data_hash = env.crypto().sha256(&stream);

        StreamArchive {
            stream_id: stream.stream_id,
            device_mac: stream.device_mac_pubkey.clone(),
            provider: stream.provider.clone(),
            payer: stream.payer.clone(),
            created_timestamp: stream.created_timestamp,
            final_balance: stream.accumulated_balance,
            total_streamed: stream.accumulated_balance, // Simplified
            pruned_timestamp: current_time,
            prune_reason: PruneReason::ZeroBalanceExpired,
            data_hash,
        }
    }

    /// Update global sweeper statistics
    fn update_sweeper_statistics(env: &Env, streams_pruned: u32, bytes_reclaimed: u64, gas_bounty: i128) {
        let mut stats = Self::get_sweeper_statistics(env.clone());
        
        stats.total_streams_pruned += streams_pruned;
        stats.total_bytes_reclaimed += bytes_reclaimed;
        stats.total_gas_bounty_paid += gas_bounty;
        stats.last_sweep_timestamp = env.ledger().timestamp();
        stats.total_sweep_operations += 1;
        
        env.storage()
            .persistent()
            .set(&DataKey::SweeperStatistics, &stats);
    }

    /// Get all stream IDs (simplified implementation)
    fn get_all_stream_ids(env: &Env, limit: u32) -> Vec<u64> {
        // Size is exactly `limit` — known before the loop begins
        let mut stream_ids = Vec::new(env);
        
        // This is a placeholder - in production, you would have an index
        // For now, return some test stream IDs
        for i in 1..=limit {
            stream_ids.push_back(i as u64);
        }
        
        stream_ids
    }
}

/// Pruning eligibility result
#[derive(Debug, Clone)]
struct PruningEligibility {
    is_eligible: bool,
    days_zero_balance: u64,
    zero_balance_since: u64,
}

/// Global sweeper statistics
#[contracttype]
#[derive(Clone)]
pub struct SweeperStatistics {
    pub total_streams_pruned: u32,
    pub total_bytes_reclaimed: u64,
    pub total_gas_bounty_paid: i128,
    pub last_sweep_timestamp: u64,
    pub total_sweep_operations: u32,
}

// Add new DataKey variants for ghost sweeper
// These should be added to the main DataKey enum in lib.rs
/*
pub enum DataKey {
    // ... existing variants ...
    StreamArchive(u64),
    SweeperStatistics,
}
*/

// Add new error variants
/*
pub enum ContractError {
    // ... existing variants ...
    StreamNotEligibleForPruning,
    StreamHasPendingBuffer,
}
*/
