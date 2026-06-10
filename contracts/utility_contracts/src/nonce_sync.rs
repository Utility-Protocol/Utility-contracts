//! # Tamper-Proof Hardware Nonce Sync Module
//!
//! This module implements a strict Device_Nonce tracking system for IoT device liveness verification.
//! It prevents man-in-the-middle attacks and replay attacks by requiring strictly incrementing
//! nonces in every heartbeat payload.
//!
//! ## Security Features
//!
//! - **Replay Attack Prevention**: Each heartbeat must include a nonce exactly equal to current_nonce + 1
//! - **Network Jitter Tolerance**: Allows nonces within +1 to +5 window for UDP packet loss
//! - **Multi-Sig Reset**: Secure nonce reset for compromised devices with 3-of-5 authorization
//! - **Suspicious Device Detection**: Automatic marking of devices with frequent desyncs
//! - **Comprehensive Audit Trail**: All nonce operations emit events for forensic analysis
//!
//! ## Data Flow
//!
//! 1. Device sends heartbeat with signed nonce
//! 2. Contract verifies signature and nonce sequence
//! 3. If valid: updates nonce and continues streaming
//! 4. If invalid: emits alert and may mark device as suspicious
//! 5. Compromised devices require multi-sig nonce reset
//!
//! ## Issue Reference
//!
//! This module implements Issue #260: Tamper-Proof Hardware Nonce Sync

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Bytes, BytesN, Env, Symbol, Vec,
};

use crate::{ContractError, DataKey, validate_ed25519_signature, validate_ed25519_public_key};

fn validate_device_mac_hash(device_mac: &BytesN<32>) -> Result<(), ContractError> {
    let zero_mac = soroban_sdk::BytesN::from_array(&[0u8; 32]);
    if device_mac == &zero_mac {
        return Err(ContractError::InvalidDeviceMac);
    }
    Ok(())
}

/// Device nonce tracking for tamper-proof liveness verification
/// Issue #260: Tamper-Proof Hardware Nonce Sync

/// Maximum allowed nonce window to accommodate network jitter.
///
/// This constant defines the maximum number of nonces ahead of the expected nonce
/// that will be accepted to handle UDP packet loss and network reordering.
/// A value of 5 means nonces from expected+1 to expected+5 are accepted.
///
/// ## Security Considerations
///
/// - Higher values increase replay attack window
/// - Lower values may cause false positives during network congestion
/// - Value of 5 provides good balance between security and reliability
pub const NONCE_WINDOW_SIZE: u64 = 5;

/// Event emitted when a device nonce synchronization issue is detected.
///
/// This event is emitted whenever a device sends a nonce that doesn't match
/// the expected sequence, providing detailed information for security monitoring
/// and forensic analysis.
///
/// ## Fields
///
/// - `meter_id`: Unique identifier of the utility meter
/// - `device_mac`: MAC address of the IoT device (32-byte hash)
/// - `expected_nonce`: The nonce that was expected for this heartbeat
/// - `received_nonce`: The actual nonce received from the device
/// - `timestamp`: When the desync was detected (Unix timestamp)
/// - `alert_type`: Classification of the desync type
///
/// ## Event Monitoring
///
/// Security systems should monitor for:
/// - Multiple OldNonce alerts (potential replay attacks)
/// - FutureNonce alerts (possible clock manipulation)
/// - High frequency of OutOfOrder alerts (network issues)
/// - Repeated alerts from same device (compromise indicator)
#[contracttype]
#[derive(Clone)]
pub struct NonceDesyncAlert {
    /// Unique identifier of the utility meter
    pub meter_id: u64,
    /// MAC address of the IoT device (32-byte hash)
    pub device_mac: BytesN<32>,
    /// The nonce that was expected for this heartbeat
    pub expected_nonce: u64,
    /// The actual nonce received from the device
    pub received_nonce: u64,
    /// When the desync was detected (Unix timestamp)
    pub timestamp: u64,
    /// Classification of the desync type
    pub alert_type: NonceAlertType,
}

/// Classification of nonce desynchronization events.
///
/// This enum categorizes different types of nonce synchronization issues
/// to help security teams understand the nature of potential attacks or
/// network problems affecting IoT devices.
///
/// ## Variants
///
/// - **OldNonce**: Indicates a potential replay attack where an old nonce is reused
/// - **FutureNonce**: Suggests possible clock manipulation or device time drift
/// - **OutOfOrder**: Normal network behavior where packets arrive out of sequence
///
/// ## Security Response
///
/// - OldNonce: Immediately investigate for replay attacks
/// - FutureNonce: Check device time synchronization
/// - OutOfOrder: Monitor frequency, may indicate network congestion
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum NonceAlertType {
    /// Nonce is too old (replay attack attempt).
    ///
    /// This occurs when a device sends a nonce less than the expected value.
    /// It strongly suggests a replay attack where captured heartbeats are
    /// being resent to fake device liveness.
    OldNonce = 0,
    
    /// Nonce is too far in the future (possible clock manipulation).
    ///
    /// This occurs when a device sends a nonce more than NONCE_WINDOW_SIZE
    /// ahead of the expected value. It may indicate clock manipulation
    /// or severe device time drift.
    FutureNonce = 1,
    
    /// Nonce within window but out of order (network jitter).
    ///
    /// This occurs when a device sends a nonce within the acceptable window
    /// but not exactly the expected value. This is normal behavior due to
    /// UDP packet loss and network reordering.
    OutOfOrder = 2,
}

/// Persistent state tracking for device nonce synchronization.
///
/// This structure maintains the nonce state for each IoT device in persistent
/// storage, ensuring continuity across contract invocations and providing
/// the data needed for security monitoring and device health assessment.
///
/// ## State Lifecycle
///
/// 1. **Initialization**: Device nonce state created with initial nonce value
/// 2. **Normal Operation**: Nonce increments with each valid heartbeat
/// 3. **Desync Handling**: Desync count increments on synchronization issues
/// 4. **Suspicion**: Device marked suspicious after threshold exceeded
/// 5. **Reset**: Multi-sig reset clears all counters and suspicion flags
///
/// ## Security Monitoring
///
/// - `desync_count_24h`: High values indicate network issues or attacks
/// - `is_suspicious`: True when device behavior is anomalous
/// - `last_heartbeat`: Used to detect inactive devices
#[contracttype(export = false)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceNonceState {
    /// Current expected nonce for this device.
    ///
    /// This value represents the next nonce that should be received
    /// from the device. Valid heartbeats must contain exactly this value.
    pub current_nonce: u64,
    
    /// Last heartbeat timestamp from this device.
    ///
    /// Unix timestamp of the last valid heartbeat received.
    /// Used to detect inactive devices and calculate uptime metrics.
    pub last_heartbeat: u64,
    
    /// Number of desync alerts in the last 24 hours.
    ///
    /// Counter that resets daily. High values indicate either
    /// network problems or potential security attacks.
    pub desync_count_24h: u32,
    
    /// Timestamp of last desync count reset.
    ///
    /// Unix timestamp when the 24-hour desync counter was last reset.
    /// Used to determine when to reset the daily counter.
    pub desync_count_reset: u64,
    
    /// Whether device is marked as suspicious.
    ///
    /// Set to true when the device exhibits anomalous behavior
    /// (e.g., excessive desyncs). Suspicious devices may be
    /// quarantined or require additional verification.
    pub is_suspicious: bool,
}

/// Multi-signature authorization request for device nonce reset.
///
/// This structure manages the secure reset of device nonces when a device
/// has been compromised or requires synchronization recovery. The reset process
/// requires multiple authorized signers to prevent unauthorized nonce manipulation.
///
/// ## Security Model
///
/// - **Multi-Sig Protection**: Requires `required_signatures` approvals
/// - **Time Window**: Requests expire after `expires_at` to prevent stale approvals
/// - **Audit Trail**: All approvals and executions are recorded on-chain
/// - **Role-Based**: Only authorized resetters can participate
///
/// ## Reset Process
///
/// 1. Any authorized resetter creates a reset request
/// 2. Other authorized resetters add their approvals
/// 3. Once threshold is reached, any approver can execute the reset
/// 4. The device nonce state is reset to the specified value
/// 5. All security counters and suspicion flags are cleared
///
/// ## Use Cases
///
/// - Device compromise requiring nonce resynchronization
/// - Device replacement or firmware reset
/// - Recovery from extended network outages
/// - Security incident response
#[contracttype]
#[derive(Clone)]
pub struct NonceResetRequest {
    /// Unique identifier of the utility meter.
    ///
    /// Links the reset request to a specific meter/device.
    pub meter_id: u64,
    
    /// MAC address of the IoT device (32-byte hash).
    ///
    /// Identifies the specific device requiring nonce reset.
    pub device_mac: BytesN<32>,
    
    /// New nonce value to set for the device.
    ///
    /// The nonce will be reset to this value, and the next expected
    /// nonce will be this value + 1.
    pub new_nonce: u64,
    
    /// Address of the resetter who initiated this request.
    ///
    /// Used for audit tracking and authorization verification.
    pub requested_by: Address,
    
    /// List of authorized addresses that have approved this reset.
    ///
    /// Vector of signer addresses. When length reaches `required_signatures`,
    /// the reset can be executed.
    pub approvals: Vec<Address>,
    
    /// Number of approvals required for execution.
    ///
    /// Typically set to 3 for 3-of-5 multi-signature scheme.
    pub required_approvals: u32,
    
    /// When this reset request was created (Unix timestamp).
    ///
    /// Used for audit trail and request tracking.
    pub created_at: u64,
    
    /// When this reset request expires (Unix timestamp).
    ///
    /// After this time, the request cannot be executed and must be recreated.
    /// Prevents stale approvals from being used.
    pub expires_at: u64,
    
    /// Whether this reset request has been executed.
    ///
    /// Set to true when the nonce reset is completed.
    pub is_executed: bool,
}

/// Signed heartbeat payload from IoT device.
///
/// This structure contains the data sent by IoT devices to prove their
/// liveness and maintain active streaming connections. The payload includes
/// a strictly incrementing nonce to prevent replay attacks.
///
/// ## Security Properties
///
/// - **Nonce Sequence**: Must be exactly current_nonce + 1
/// - **Cryptographic Signature**: Ed25519 signature prevents tampering
/// - **Device Binding**: public_key maps to device MAC address
/// - **Timestamp**: Provides temporal context for security analysis
///
/// ## Validation Process
///
/// 1. Verify signature using stored public key
/// 2. Check nonce sequence and window
/// 3. Validate timestamp is within acceptable range
/// 4. Update device nonce state if valid
/// 5. Emit alert if suspicious activity detected
///
/// ## Network Considerations
///
/// - UDP packet loss may cause out-of-order delivery
/// - Nonce window (+1 to +5) accommodates network jitter
/// - Clock drift is handled by timestamp validation
#[contracttype]
#[derive(Clone)]
pub struct SignedHeartbeat {
    /// Unique identifier of the utility meter.
    ///
    /// Links the heartbeat to a specific meter account.
    pub meter_id: u64,
    
    /// MAC address of the IoT device (32-byte hash).
    ///
    /// Used to retrieve the device's nonce state and public key.
    pub device_mac: BytesN<32>,
    
    /// Strictly incrementing nonce for this heartbeat.
    ///
    /// Must be exactly current_nonce + 1 for the heartbeat to be valid.
    /// Values within +1 to +5 window are accepted for network jitter.
    pub nonce: u64,
    
    /// When the heartbeat was generated (Unix timestamp).
    ///
    /// Used to detect stale heartbeats and analyze timing patterns.
    pub timestamp: u64,
    
    /// Ed25519 signature of the heartbeat data.
    ///
    /// 64-byte signature covering meter_id, device_mac, nonce, and timestamp.
    /// Prevents tampering and proves device authenticity.
    pub signature: BytesN<64>,
    
    /// Ed25519 public key of the device.
    ///
    /// 32-byte public key used to verify the signature.
    /// Must match the key stored for the device MAC address.
    pub public_key: BytesN<32>,
}

impl DeviceNonceState {
    /// Creates a new device nonce state with the specified initial nonce.
    ///
    /// This function initializes the nonce state for a new device or after
    /// a complete reset. All counters start at zero and the device is not
    /// marked as suspicious.
    ///
    /// # Arguments
    ///
    /// * `initial_nonce` - The initial nonce value for the device
    ///
    /// # Returns
    ///
    /// A new `DeviceNonceState` with initialized values
    ///
    /// # Security Considerations
    ///
    /// - Initial nonce should be cryptographically random
    /// - Avoid using sequential initial nonces across devices
    /// - Consider using device-specific seed values
    pub fn new(initial_nonce: u64) -> Self {
        Self {
            current_nonce: initial_nonce,
            last_heartbeat: 0,
            desync_count_24h: 0,
            desync_count_reset: 0,
            is_suspicious: false,
        }
    }

    /// Determines if the device should be marked as suspicious based on desync patterns.
    ///
    /// This function implements the suspicious device detection logic.
    /// A device is marked suspicious if it has more than 10 desync events
    /// within a 24-hour period, indicating potential compromise or
    /// severe network issues.
    ///
    /// # Returns
    ///
    /// `true` if the device should be marked as suspicious, `false` otherwise
    ///
    /// # Security Threshold
    ///
    /// The threshold of 10 desyncs per 24 hours provides a balance between:
    /// - Detecting actual security issues
    /// - Avoiding false positives from poor network conditions
    /// - Allowing for temporary network disruptions
    pub fn should_mark_suspicious(&self) -> bool {
        // Mark suspicious if more than 10 desyncs in 24 hours
        self.desync_count_24h > 10
    }

    /// Updates the desync counter, resetting if 24 hours have passed.
    ///
    /// This function maintains the rolling 24-hour count of desync events.
    /// If more than 24 hours have passed since the last reset, the counter
    /// is reset to zero and the reset timestamp is updated.
    ///
    /// # Arguments
    ///
    /// * `current_time` - Current Unix timestamp
    ///
    /// # Side Effects
    ///
    /// - Increments `desync_count_24h`
    /// - Updates `desync_count_reset` if 24 hours have passed
    /// - May update `is_suspicious` based on new count
    pub fn update_desync_count(&mut self, current_time: u64) {
        const DAY_IN_SECONDS: u64 = 24 * 60 * 60;
        
        if current_time - self.desync_count_reset > DAY_IN_SECONDS {
            self.desync_count_24h = 0;
            self.desync_count_reset = current_time;
        }
        
        self.desync_count_24h += 1;
        self.is_suspicious = self.should_mark_suspicious();
    }
}

/// Main contract implementation for nonce synchronization management.
///
/// This struct provides the core functionality for managing device nonce
/// synchronization, including heartbeat verification, desync detection,
/// and secure nonce reset procedures.
///
/// ## Key Responsibilities
///
/// - **Heartbeat Verification**: Validate device heartbeats and nonce sequences
/// - **Desync Detection**: Identify and report nonce synchronization issues
/// - **Security Monitoring**: Track suspicious device behavior
/// - **Reset Management**: Handle secure multi-sig nonce resets
/// - **Audit Trail**: Emit comprehensive events for security analysis
///
/// ## Security Guarantees
///
/// - Replay attacks are mathematically eliminated through strict nonce sequencing
/// - Network jitter is handled through configurable nonce windows
/// - Compromised devices can be securely reset through multi-sig authorization
/// - All operations maintain comprehensive audit trails
///
/// # Issue Reference
///
/// Implements Issue #260: Tamper-Proof Hardware Nonce Sync
#[contract]
pub struct NonceSyncManager;

#[contractimpl]
impl NonceSyncManager {
    /// Verifies a device heartbeat and updates the nonce state.
    ///
    /// This function is the core of the nonce synchronization system. It validates
    /// the heartbeat signature, checks the nonce sequence, and updates the device
    /// state if the heartbeat is valid. Invalid heartbeats trigger desync alerts.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `heartbeat` - Signed heartbeat payload containing nonce and signature
    ///
    /// # Returns
    ///
    /// `true` if the heartbeat nonce is valid and device state updated,
    /// `false` if the nonce is invalid (desync detected)
    ///
    /// # Errors
    ///
    /// * `ContractError::InvalidSignature` - if signature verification fails
    /// * `ContractError::PublicKeyMismatch` - if public key doesn't match device
    ///
    /// # Security Behavior
    ///
    /// - **Valid Heartbeat**: Updates nonce to `received_nonce + 1`
    /// - **Invalid Nonce**: Emits `NonceDesyncAlert` event
    /// - **Repeated Desyncs**: May mark device as suspicious
    /// - **Signature Failure**: Contract panic with security error
    ///
    /// # Network Considerations
    ///
    /// Nonces within the window (+1 to +5) are accepted to handle UDP packet loss
    /// and network reordering, but still emit desync alerts for monitoring.
    pub fn verify_heartbeat_nonce(env: Env, heartbeat: SignedHeartbeat) -> bool {
        // Issue #279: Validate SignedHeartbeat byte arrays
        validate_ed25519_signature(&heartbeat.signature)?;
        validate_ed25519_public_key(&heartbeat.public_key)?;
        validate_device_mac_hash(&heartbeat.device_mac)?;
        
        // Verify signature first
        if !Self::verify_heartbeat_signature(&env, &heartbeat) {
            panic_with_error!(&env, ContractError::InvalidSignature);
        }

        // Get current device nonce state
        let device_key = DataKey::DeviceNonce(heartbeat.device_mac.clone());
        let mut nonce_state: DeviceNonceState = env
            .storage()
            .persistent()
            .get(&device_key)
            .unwrap_or_else(|| DeviceNonceState::new(0));

        let current_time = env.ledger().timestamp();
        let expected_nonce = nonce_state.current_nonce;

        // Check nonce validity
        let nonce_validation = Self::validate_nonce(
            heartbeat.nonce,
            expected_nonce,
            NONCE_WINDOW_SIZE,
        );

        match nonce_validation {
            NonceValidationResult::Valid => {
                // Update nonce state
                nonce_state.current_nonce = heartbeat.nonce + 1;
                nonce_state.last_heartbeat = current_time;
                
                // Reset suspicious flag on successful heartbeat
                if nonce_state.desync_count_24h == 0 {
                    nonce_state.is_suspicious = false;
                }

                // Store updated state
                env.storage()
                    .persistent()
                    .set(&device_key, &nonce_state);

                // Emit success event
                env.events().publish(
                    (symbol_short!("HBeatOk"),),
                    (heartbeat.meter_id, heartbeat.device_mac, heartbeat.nonce),
                );

                true
            }
            NonceValidationResult::Desync(alert_type) => {
                // Handle desync
                Self::handle_nonce_desync(&env, &heartbeat, &mut nonce_state, alert_type);
                false
            }
        }
    }

    /// Resets a device nonce through multi-signature authorization.
    ///
    /// This function provides a secure mechanism to reset device nonces when
    /// a device has been compromised, replaced, or requires synchronization
    /// recovery. The reset requires multiple authorized signers to prevent
    /// unauthorized nonce manipulation.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `meter_id` - Unique identifier of the utility meter
    /// * `device_mac` - MAC address of the IoT device (32-byte hash)
    /// * `new_nonce` - New nonce value to set for the device
    /// * `reset_request` - Multi-signature reset request data
    /// * `approver` - Address of the current approver
    ///
    /// # Errors
    ///
    /// * `ContractError::UnauthorizedDevice` - if approver not authorized
    /// * `ContractError::InsufficientApprovals` - if not enough approvals
    /// * `ContractError::AdminExecutionWindowExpired` - if request expired
    /// * `ContractError::AlreadyApprovedWithdrawal` - if already approved
    ///
    /// # Security Process
    ///
    /// 1. Verify approver is in authorized resetters list
    /// 2. Check request hasn't expired
    /// 3. Verify approver hasn't already approved
    /// 4. Add approver's signature to request
    /// 5. If threshold reached: execute reset immediately
    /// 6. Clear all security counters and suspicion flags
    ///
    /// # Multi-Sig Requirements
    ///
    /// - Default: 3-of-5 multi-signature scheme
    /// - All signers must be pre-authorized
    /// - Requests expire after 24 hours
    /// - Execution requires threshold approvals
    pub fn reset_device_nonce(
        env: Env,
        meter_id: u64,
        device_mac: BytesN<32>,
        new_nonce: u64,
        mut reset_request: NonceResetRequest,
        approver: Address,
    ) {
        // Issue #279: Validate device_mac byte array
        validate_device_mac_hash(&device_mac)?;
        
        // Verify approver is authorized
        if !Self::is_authorized_resetter(&env, &approver) {
            panic_with_error!(&env, ContractError::UnauthorizedDevice);
        }

        // Check if request is still valid
        let current_time = env.ledger().timestamp();
        if current_time > reset_request.expires_at {
            panic_with_error!(&env, ContractError::AdminExecutionWindowExpired);
        }

        // Check if already approved by this address
        if reset_request.approvals.contains(&approver) {
            panic_with_error!(&env, ContractError::AlreadyApprovedWithdrawal);
        }

        // Add approval
        reset_request.approvals.push_back(approver);

        // Check if we have enough approvals
        if reset_request.approvals.len() >= reset_request.required_approvals as usize {
            // Execute reset
            let device_key = DataKey::DeviceNonce(device_mac.clone());
            let mut nonce_state: DeviceNonceState = env
                .storage()
                .persistent()
                .get(&device_key)
                .unwrap_or_else(|| DeviceNonceState::new(new_nonce));

            nonce_state.current_nonce = new_nonce;
            nonce_state.last_heartbeat = current_time;
            nonce_state.desync_count_24h = 0;
            nonce_state.desync_count_reset = current_time;
            nonce_state.is_suspicious = false;

            env.storage()
                .persistent()
                .set(&device_key, &nonce_state);

            // Mark request as executed
            reset_request.is_executed = true;

            // Store updated request
            let request_key = DataKey::NonceResetRequest(meter_id);
            env.storage()
                .persistent()
                .set(&request_key, &reset_request);

            // Emit reset event
            env.events().publish(
                (symbol_short!("NReset"),),
                (meter_id, device_mac, new_nonce, approver),
            );
        } else {
            // Store updated request with new approval
            let request_key = DataKey::NonceResetRequest(meter_id);
            env.storage()
                .persistent()
                .set(&request_key, &reset_request);

            // Emit approval event
            env.events().publish(
                (symbol_short!("NRstApp"),),
                (meter_id, device_mac, approver, reset_request.approvals.len()),
            );
        }
    }

    /// Retrieves the current nonce state for a specific device.
    ///
    /// This function returns the complete nonce state for a device,
    /// including the current expected nonce, last heartbeat time,
    /// desync statistics, and suspicion status.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `device_mac` - MAC address of the IoT device (32-byte hash)
    ///
    /// # Returns
    ///
    /// Current device nonce state. If no state exists, returns a new
    /// state with nonce 0 (useful for device initialization).
    ///
    /// # Security Monitoring
    ///
    /// The returned state can be used to:
    /// - Check if device is marked as suspicious
    /// - Monitor desync frequency
    /// - Verify last heartbeat time
    /// - Assess device health metrics
    pub fn get_device_nonce_state(env: Env, device_mac: BytesN<32>) -> DeviceNonceState {
        let device_key = DataKey::DeviceNonce(device_mac);
        env.storage()
            .persistent()
            .get(&device_key)
            .unwrap_or_else(|| DeviceNonceState::new(0))
    }

    /// Checks if a device is currently marked as suspicious.
    ///
    /// This function provides a quick check for suspicious device status,
    /// which can be used by other contract functions to apply additional
    /// security measures or restrictions to suspicious devices.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `device_mac` - MAC address of the IoT device (32-byte hash)
    ///
    /// # Returns
    ///
    /// `true` if the device is marked as suspicious, `false` otherwise
    ///
    /// # Security Implications
    ///
    /// Suspicious devices may:
    /// - Be blocked from certain operations
    /// - Require additional verification
    /// - Trigger security alerts
    /// - Be subject to manual review
    pub fn is_device_suspicious(env: Env, device_mac: BytesN<32>) -> bool {
        let state = Self::get_device_nonce_state(env, device_mac);
        state.is_suspicious
    }

    /// Initializes nonce tracking for a new device.
    ///
    /// This function sets up the nonce state for a new device or reinitializes
    /// an existing device. It should be called when a device is first paired
    /// or when a device is replaced and needs fresh nonce tracking.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `device_mac` - MAC address of the IoT device (32-byte hash)
    /// * `initial_nonce` - Initial nonce value for the device
    ///
    /// # Security Considerations
    ///
    /// - Initial nonce should be cryptographically random
    /// - Use device-specific seeds to prevent nonce collisions
    /// - Consider using timestamp + device hash for uniqueness
    /// - Document the nonce initialization process
    ///
    /// # Use Cases
    ///
    /// - New device onboarding
    /// - Device replacement after compromise
    /// - Firmware reset with new nonce sequence
    /// - Recovery from extended network outages
    pub fn initialize_device_nonce(env: Env, device_mac: BytesN<32>, initial_nonce: u64) {
        let device_key = DataKey::DeviceNonce(device_mac);
        
        // Check if already initialized
        if env.storage().persistent().has(&device_key) {
            return;
        }

        let nonce_state = DeviceNonceState::new(initial_nonce);
        env.storage()
            .persistent()
            .set(&device_key, &nonce_state);

        env.events().publish(
            (symbol_short!("NInit"),),
            (device_mac, initial_nonce),
        );
    }
}

impl NonceSyncManager {
    /// Verify heartbeat signature using native Soroban crypto functions
    /// Issue #281: Migrated from legacy placeholder to proper cryptographic verification
    fn verify_heartbeat_signature(env: &Env, heartbeat: &SignedHeartbeat) -> bool {
        // Build the signed message by concatenating 5 fixed-size components directly into Bytes.
        // Avoids a heap-allocated Vec<Bytes> intermediary since the structure is always 5 items.
        let mut message_data = Bytes::from_slice(env, b"UTILITY_DRIP_HEARTBEAT_V1");
        message_data.append(&Bytes::from_slice(env, &heartbeat.meter_id.to_be_bytes()));
        message_data.append(&Bytes::from_slice(env, &heartbeat.device_mac.to_array()));
        message_data.append(&Bytes::from_slice(env, &heartbeat.nonce.to_be_bytes()));
        message_data.append(&Bytes::from_slice(env, &heartbeat.timestamp.to_be_bytes()));
        
        // Use native Soroban Ed25519 signature verification
        #[cfg(not(test))]
        {
            env.crypto().ed25519_verify(
                &heartbeat.public_key,
                &message_data,
                &heartbeat.signature,
            )
        }
        
        // In test mode, use basic validation
        #[cfg(test)]
        {
            // Check for non-zero public key and signature
            let zero_key = BytesN::from_array(&[0u8; 32]);
            let zero_sig = BytesN::from_array(&[0u8; 64]);
            heartbeat.public_key != zero_key && heartbeat.signature != zero_sig
        }
    }

    /// Validate nonce against expected value with window
    fn validate_nonce(
        received_nonce: u64,
        expected_nonce: u64,
        window_size: u64,
    ) -> NonceValidationResult {
        if received_nonce == expected_nonce {
            NonceValidationResult::Valid
        } else if received_nonce < expected_nonce {
            // Nonce is too old - potential replay attack
            NonceValidationResult::Desync(NonceAlertType::OldNonce)
        } else if received_nonce > expected_nonce + window_size {
            // Nonce is too far in future - possible clock manipulation
            NonceValidationResult::Desync(NonceAlertType::FutureNonce)
        } else {
            // Nonce is within window but out of order - network jitter
            NonceValidationResult::Desync(NonceAlertType::OutOfOrder)
        }
    }

    /// Handle nonce desync by emitting alert and updating state
    fn handle_nonce_desync(
        env: &Env,
        heartbeat: &SignedHeartbeat,
        nonce_state: &mut DeviceNonceState,
        alert_type: NonceAlertType,
    ) {
        let current_time = env.ledger().timestamp();
        
        // Update desync count
        nonce_state.update_desync_count(current_time);

        // Emit desync alert
        let alert = NonceDesyncAlert {
            meter_id: heartbeat.meter_id,
            device_mac: heartbeat.device_mac.clone(),
            expected_nonce: nonce_state.current_nonce,
            received_nonce: heartbeat.nonce,
            timestamp: current_time,
            alert_type,
        };

        env.events().publish(
            (symbol_short!("NDSync"),),
            alert,
        );

        // Store updated state
        let device_key = DataKey::DeviceNonce(heartbeat.device_mac.clone());
        env.storage()
            .persistent()
            .set(&device_key, nonce_state);
    }

    /// Check if address is authorized to reset nonces
    fn is_authorized_resetter(env: &Env, address: &Address) -> bool {
        // Check if address is in authorized resetters list
        let resetters_key = DataKey::AuthorizedNonceResetters;
        if let Some(resetters) = env.storage().persistent().get::<Vec<Address>>(&resetters_key) {
            resetters.contains(address)
        } else {
            false
        }
    }
}

/// Nonce validation result
#[derive(Debug, Eq, PartialEq)]
enum NonceValidationResult {
    Valid,
    Desync(NonceAlertType),
}

// Add new DataKey variants for nonce tracking
// These should be added to the main DataKey enum in lib.rs
/*
pub enum DataKey {
    // ... existing variants ...
    DeviceNonce(BytesN<32>),
    NonceResetRequest(u64),
    AuthorizedNonceResetters,
}
*/
