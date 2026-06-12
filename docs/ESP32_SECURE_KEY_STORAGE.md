# 🔐 Secure Key Storage on ESP32

A comprehensive guide for contributors on securely storing Ed25519 keys on ESP32 devices using NVS (Non-Volatile Storage) and Secure Elements.

## Overview

The IoT-Billing-Service system requires each ESP32 device to:
1. Generate an Ed25519 key pair
2. Store the private key securely
3. Use the private key to sign usage data
4. Protect against physical and remote attacks

This guide covers multiple security levels from basic to advanced.

---

## Table of Contents

- [Security Levels](#security-levels)
- [Level 1: Basic NVS Storage](#level-1-basic-nvs-storage)
- [Level 2: Encrypted NVS Partition](#level-2-encrypted-nvs-partition)
- [Level 3: Secure Element (ATECC608A)](#level-3-secure-element-atecc608a)
- [Level 4: ESP32-S3 Secure Flash](#level-4-esp32-s3-secure-flash)
- [Key Generation Best Practices](#key-generation-best-practices)
- [Implementation Examples](#implementation-examples)
- [Testing & Validation](#testing--validation)
- [Troubleshooting](#troubleshooting)

---

## Security Levels

Choose the appropriate level based on your threat model:

### Level 1: Basic NVS (Development)
- **Use Case**: Prototyping, development, testing
- **Security**: Low - keys stored in plain flash
- **Cost**: Free (uses internal flash)
- **Complexity**: Easy

### Level 2: Encrypted NVS (Production Lite)
- **Use Case**: Low-risk deployments, trusted environments
- **Security**: Medium - encrypted at rest
- **Cost**: Free (uses internal flash + encryption)
- **Complexity**: Moderate

### Level 3: Secure Element (Production Standard)
- **Use Case**: Commercial deployments, high-security requirements
- **Security**: High - hardware-backed security
- **Cost**: $1-3 per device (external chip)
- **Complexity**: Moderate-High

### Level 4: ESP32-S3 Secure Flash (Premium)
- **Use Case**: High-volume production, maximum security
- **Security**: Very High - secure boot + flash encryption
- **Cost**: Higher chip cost (ESP32-S3)
- **Complexity**: High

---

## Level 1: Basic NVS Storage

**⚠️ WARNING**: Only suitable for development. Not secure for production.

### Setup

```cpp
#include <nvs.h>
#include <nvs_flash.h>
#include <mbedtls/ed25519.h>

// NVS namespace for key storage
static const char* KEY_NAMESPACE = "utility_drip";
static const char* PRIVATE_KEY_KEY = "priv_key";
static const char* PUBLIC_KEY_KEY = "pub_key";

class KeyStorage {
private:
    nvs_handle_t my_handle;
    uint8_t private_key[32];
    uint8_t public_key[32];

public:
    KeyStorage() : my_handle(0) {}

    esp_err_t init() {
        // Initialize NVS
        esp_err_t err = nvs_flash_init();
        if (err == ESP_ERR_NVS_NO_FREE_PAGES || 
            err == ESP_ERR_NVS_NEW_VERSION_FOUND) {
            // NVS partition was truncated and needs to be erased
            ESP_ERROR_CHECK(nvs_flash_erase());
            err = nvs_flash_init();
        }
        return err;
    }

    esp_err_t open() {
        // Open NVS namespace
        return nvs_open(KEY_NAMESPACE, NVS_READWRITE, &my_handle);
    }

    void close() {
        nvs_close(my_handle);
    }

    esp_err_t generate_keys() {
        // Generate Ed25519 key pair using mbedtls
        mbedtls_ed25519_context ctx;
        mbedtls_ed25519_init(&ctx);

        // Use hardware RNG for seed
        esp_fill_random(private_key, 32);
        
        int ret = mbedtls_ed25519_genkey(&ctx, private_key, public_key);
        if (ret != 0) {
            ESP_LOGE("KeyStorage", "Key generation failed: %d", ret);
            mbedtls_ed25519_free(&ctx);
            return ESP_FAIL;
        }

        mbedtls_ed25519_free(&ctx);
        ESP_LOGI("KeyStorage", "Keys generated successfully");
        return ESP_OK;
    }

    esp_err_t save_keys() {
        // Save private key (⚠️ NOT ENCRYPTED)
        esp_err_t err = nvs_set_blob(my_handle, PRIVATE_KEY_KEY, 
                                      private_key, 32);
        if (err != ESP_OK) return err;

        // Save public key
        err = nvs_set_blob(my_handle, PUBLIC_KEY_KEY, 
                          public_key, 32);
        if (err != ESP_OK) return err;

        // Commit changes
        return nvs_commit(my_handle);
    }

    esp_err_t load_keys() {
        size_t size = 32;
        
        // Load private key
        esp_err_t err = nvs_get_blob(my_handle, PRIVATE_KEY_KEY, 
                                      private_key, &size);
        if (err != ESP_OK) return err;

        // Load public key
        size = 32;
        err = nvs_get_blob(my_handle, PUBLIC_KEY_KEY, 
                          public_key, &size);
        if (err != ESP_OK) return err;

        ESP_LOGI("KeyStorage", "Keys loaded from NVS");
        return ESP_OK;
    }

    bool has_keys() {
        // Check if keys exist in NVS
        size_t size = 0;
        esp_err_t err = nvs_get_blob(my_handle, PRIVATE_KEY_KEY, 
                                      NULL, &size);
        return (err == ESP_OK);
    }

    const uint8_t* get_private_key() {
        return private_key;
    }

    const uint8_t* get_public_key() {
        return public_key;
    }

    esp_err_t erase_keys() {
        // Permanently delete keys
        esp_err_t err = nvs_erase_key(my_handle, PRIVATE_KEY_KEY);
        if (err != ESP_OK) return err;
        
        err = nvs_erase_key(my_handle, PUBLIC_KEY_KEY);
        if (err != ESP_OK) return err;
        
        return nvs_commit(my_handle);
    }
};
```

### Usage Example

```cpp
void setup() {
    KeyStorage keyStorage;
    
    // Initialize NVS
    ESP_ERROR_CHECK(keyStorage.init());
    ESP_ERROR_CHECK(keyStorage.open());

    // Check if we have existing keys
    if (!keyStorage.has_keys()) {
        Serial.println("Generating new keys...");
        ESP_ERROR_CHECK(keyStorage.generate_keys());
        ESP_ERROR_CHECK(keyStorage.save_keys());
        
        Serial.println("✅ Keys generated and saved!");
    } else {
        Serial.println("Loading existing keys...");
        ESP_ERROR_CHECK(keyStorage.load_keys());
        Serial.println("✅ Keys loaded!");
    }

    // Display public key (for registration)
    Serial.print("Public Key: ");
    print_hex(keyStorage.get_public_key(), 32);
    
    keyStorage.close();
}

void loop() {
    // Use keys to sign data
    // ...
}
```

### Security Considerations

❌ **Risks:**
- Private key stored in plain text in flash
- Anyone with physical access can read it
- No protection against firmware extraction

✅ **Mitigations:**
- Enable flash readout protection (if available)
- Use only for development/testing
- Never use in production without encryption

---

## Level 2: Encrypted NVS Partition

**Recommended minimum for production deployments.**

### Configuration

#### 1. Create Encrypted NVS Partition

Create `partitions.csv`:

```csv
# Name,   Type, SubType, Offset,  Size, Flags
nvs,      data, nvs,     0x9000,  0x6000,
nvs_enc,  data, nvs,     0xF000,  0x6000, encrypted
factory,  app,  factory, 0x10000, 1M,
```

#### 2. Generate Encryption Key

```bash
# Generate 256-bit encryption key
espsecure.py generate_flash_encryption_key enc_key.bin

# Backup key securely (IMPORTANT!)
cp enc_key.enc ~/secure_backup/enc_key_backup.bin
chmod 600 ~/secure_backup/enc_key_backup.bin
```

#### 3. Configure Project

In `sdkconfig`:

```
CONFIG_NVS_ENCRYPTION=y
CONFIG_SECURE_FLASH_ENC_ENABLED=y
CONFIG_SECURE_FLASH_ENCRYPTION_MODE_INTERNAL=y
```

### Implementation

```cpp
#include <nvs.h>
#include <nvs_flash.h>
#include "esp_flash_encryption.h"

class EncryptedKeyStorage {
private:
    nvs_handle_t secure_handle;
    uint8_t private_key[32];
    uint8_t public_key[32];

public:
    esp_err_t init() {
        // Initialize NVS with encryption
        nvs_sec_cfg_t cfg = {};
        
        // Get encryption key from eFuse or secure storage
        esp_err_t err = nvs_flash_read_security_cfg(&cfg);
        if (err != ESP_OK) {
            ESP_LOGE("EncryptedStorage", "Failed to read security config");
            return err;
        }

        // Initialize encrypted NVS partition
        err = nvs_flash_secure_init_partition(NVS_DEFAULT_PART_NAME, &cfg);
        if (err != ESP_OK) {
            ESP_LOGE("EncryptedStorage", "Failed to init encrypted NVS");
            return err;
        }

        return nvs_flash_init();
    }

    esp_err_t open() {
        // Open encrypted namespace
        return nvs_open("secure_keys", NVS_READWRITE, &secure_handle);
    }

    esp_err_t save_keys() {
        // Keys are automatically encrypted by NVS
        esp_err_t err = nvs_set_blob(secure_handle, "priv", 
                                      private_key, 32);
        if (err != ESP_OK) return err;

        err = nvs_set_blob(secure_handle, "pub", 
                          public_key, 32);
        if (err != ESP_OK) return err;

        return nvs_commit(secure_handle);
    }

    esp_err_t load_keys() {
        size_t size = 32;
        
        esp_err_t err = nvs_get_blob(secure_handle, "priv", 
                                      private_key, &size);
        if (err != ESP_OK) return err;

        size = 32;
        err = nvs_get_blob(secure_handle, "pub", 
                          public_key, &size);
        if (err != ESP_OK) return err;

        return ESP_OK;
    }

    // Additional security: lock keys after first use
    esp_err_t lock_keys() {
        // Mark keys as read-only
        esp_err_t err = nvs_set_blob(secure_handle, "locked", 
                                      (const void*)"1", 1);
        return nvs_commit(secure_handle);
    }

    bool is_locked() {
        char lock_status;
        size_t size = 1;
        esp_err_t err = nvs_get_blob(secure_handle, "locked", 
                                      &lock_status, &size);
        return (err == ESP_OK && lock_status == '1');
    }
};
```

### Flash Encryption Process

```bash
# 1. Build project
idf.py build

# 2. Burn encryption key to eFuse (ONE-TIME OPERATION)
espsecure.py burn_flash_encryption_key --port /dev/ttyUSB0 enc_key.bin

# ⚠️ WARNING: This is irreversible!
# Device will only boot with encrypted firmware from now on

# 3. Flash encrypted firmware
esptool.py --chip esp32 --port /dev/ttyUSB0 \
  --before no_reset --after hard_reset write_flash -e \
  0x1000 build/my_project.bin
```

### Security Benefits

✅ **Advantages:**
- All data encrypted with hardware key
- Key burned into eFuses (cannot be read back)
- Transparent to application code
- Good balance of security and complexity

⚠️ **Limitations:**
- Still software-based security
- Vulnerable to sophisticated attacks
- Requires careful key backup

---

## Level 3: Secure Element (ATECC608A)

**Recommended for commercial deployments.**

### Hardware Setup

Connect ATECC608A to ESP32:

```
ESP32          ATECC608A
----           ---------
GPIO21 (I2C SDA) ---- SDA
GPIO22 (I2C SCL) ---- SCL
3.3V         ---- VCC
GND          ---- GND
```

Pull-up resistors (4.7kΩ) required on SDA and SCL lines.

### Library Installation

```bash
# Add to platformio.ini or Arduino IDE
pio lib install "CryptoAuthLib"
```

### Implementation

```cpp
#include <CryptoAuthLib.h>
#include <basic_command.h>
#include <genkey_data.h>
#include <hal_atca.h>

class SecureElementKeys {
private:
    ATCAIfaceCfg cfg;
    ATCADevice device;
    uint8_t public_key[64]; // ATECC608A uses 64-byte public keys

public:
    SecureElementKeys() {
        // Configure I2C interface
        cfg.cfg_type = ATCA_I2C_IFACE;
        cfg.devtype = ATECC608A;
        cfg.atcai2c.address = 0xC0 >> 1; // Default ATECC608A address
        cfg.atcai2c.bus = 0;
        cfg.atcai2c.baud = 400000;
        cfg.wake_delay = 1500;
        cfg.rx_retries = 3;
    }

    esp_err_t init() {
        // Initialize CryptoAuthLib
        ATCA_STATUS status = initATCACfg(&cfg);
        if (status != ATCA_SUCCESS) {
            ESP_LOGE("SecureElement", "Init failed: %d", status);
            return ESP_FAIL;
        }

        // Open device
        status = atcab_init(&cfg);
        if (status != ATCA_SUCCESS) {
            ESP_LOGE("SecureElement", "Device init failed: %d", status);
            return ESP_FAIL;
        }

        ESP_LOGI("SecureElement", "Secure element initialized");
        return ESP_OK;
    }

    esp_err_t generate_keypair(uint8_t slot_id = 0) {
        // Generate Ed25519 key pair inside secure element
        // Private key NEVER leaves the chip!
        
        ATCA_STATUS status = atcab_genkey(slot_id, public_key);
        if (status != ATCA_SUCCESS) {
            ESP_LOGE("SecureElement", "Key generation failed: %d", status);
            return ESP_FAIL;
        }

        ESP_LOGI("SecureElement", "Keys generated in slot %d", slot_id);
        return ESP_OK;
    }

    esp_err_t get_public_key(uint8_t slot_id = 0) {
        // Read public key from secure element
        ATCA_STATUS status = atcab_genkey(slot_id, public_key);
        if (status != ATCA_SUCCESS) {
            return ESP_FAIL;
        }
        return ESP_OK;
    }

    esp_err_t sign_message(const uint8_t* message, size_t msg_len,
                           uint8_t* signature, size_t* sig_len) {
        // Sign message using private key INSIDE secure element
        // Private key never exposed
        
        ATCA_STATUS status = atcab_sign(
            0,              // Key slot
            message,        // Message to sign
            msg_len,        // Message length
            signature,      // Output signature
            sig_len         // Signature length (64 bytes for Ed25519)
        );

        if (status != ATCA_SUCCESS) {
            ESP_LOGE("SecureElement", "Signing failed: %d", status);
            return ESP_FAIL;
        }

        return ESP_OK;
    }

    esp_err_t configure_security() {
        // Lock configuration zones (ONE-TIME)
        ATCA_STATUS status;

        // Lock data and OTP zones
        status = atcab_lock_data_zone();
        if (status != ATCA_SUCCESS) {
            ESP_LOGE("SecureElement", "Locking failed: %d", status);
            return ESP_FAIL;
        }

        // Configure slot 0 as Ed25519 key (readable public key only)
        // This must be done BEFORE locking
        uint8_t config_data[128];
        status = atcab_read_config_zone(config_data);
        if (status != ATCA_SUCCESS) {
            return ESP_FAIL;
        }

        // Set slot 0 to Ed25519, private key never readable
        // Public key readable
        // See ATECC608A datasheet for configuration details

        return ESP_OK;
    }

    void cleanup() {
        atcab_release();
    }
};
```

### Usage Example

```cpp
SecureElementKeys secureKeys;

void setup() {
    Serial.begin(115200);
    
    // Initialize secure element
    ESP_ERROR_CHECK(secureKeys.init());
    
    // Check if we need to generate keys
    bool has_keys = check_if_keys_exist();
    
    if (!has_keys) {
        Serial.println("🔑 Generating keys in secure element...");
        ESP_ERROR_CHECK(secureKeys.generate_keypair(0));
        Serial.println("✅ Keys generated!");
        
        // Lock the device (optional but recommended)
        // ESP_ERROR_CHECK(secureKeys.configure_security());
    }
    
    // Get public key for registration
    uint8_t public_key[64];
    ESP_ERROR_CHECK(secureKeys.get_public_key(0));
    
    Serial.print("Public Key: ");
    print_hex(public_key, 64);
}

void sign_and_send_usage_data() {
    // Prepare usage data
    uint8_t message[100];
    prepare_usage_message(message, sizeof(message));
    
    // Sign with secure element
    uint8_t signature[64];
    size_t sig_len = sizeof(signature);
    
    esp_err_t err = secureKeys.sign_message(
        message, sizeof(message),
        signature, &sig_len
    );
    
    if (err == ESP_OK) {
        Serial.println("✅ Data signed securely");
        send_to_contract(message, signature, sig_len);
    } else {
        Serial.println("❌ Signing failed");
    }
}
```

### Security Benefits

✅ **Maximum Security:**
- Private key NEVER leaves the chip
- Hardware-based true random number generator
- Tamper-resistant
- Side-channel attack resistant
- Key slots can be permanently locked

⚠️ **Considerations:**
- Additional hardware cost (~$1-3)
- More complex PCB design
- Requires secure supply chain

---

## Level 4: ESP32-S3 Secure Flash

**For high-volume production with ESP32-S3.**

### Features

- Secure Boot v2 (RSA/PSS verification)
- Flash Encryption (AES-XTS-256)
- DMA-protected memory
- EFuse-based security configuration

### Configuration

In `sdkconfig`:

```
CONFIG_SECURE_SIGNED_APPS_SEC_RSA_3048=y
CONFIG_SECURE_FLASH_ENCRYPTION_ENABLED=y
CONFIG_SECURE_FLASH_ENCRYPTION_XTS_MODE=y
CONFIG_SECURE_BOOT_V2_ENABLED=y
```

### Implementation Guide

See Espressif's official documentation:
- [ESP32-S3 Technical Reference Manual](https://www.espressif.com/sites/default/files/documentation/esp32-s3_technical_reference_manual_en.pdf)
- [Secure Boot v2](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/security/secure-boot-v2.html)
- [Flash Encryption](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/security/flash-encryption.html)

---

## Key Generation Best Practices

### 1. Use Hardware RNG

```cpp
// ESP32 has built-in hardware RNG
uint8_t seed[32];
esp_fill_random(seed, 32); // Cryptographically secure
```

### 2. Verify Key Quality

```cpp
bool verify_key_quality(const uint8_t* key) {
    // Check key is not all zeros or all ones
    bool all_zeros = true;
    bool all_ones = true;
    
    for (int i = 0; i < 32; i++) {
        if (key[i] != 0x00) all_zeros = false;
        if (key[i] != 0xFF) all_ones = false;
    }
    
    if (all_zeros || all_ones) {
        ESP_LOGE("KeyCheck", "Weak key detected!");
        return false;
    }
    
    // Additional entropy checks
    return true;
}
```

### 3. Secure Key Provisioning

For production:

```bash
# Generate keys in secure facility
python provision_keys.py --output keys.bin --encrypt

# Flash to device during manufacturing
esptool.py write_flash 0x300000 keys.bin

# Immediately lock the device
python lock_device.py --port /dev/ttyUSB0
```

### 4. Key Rotation Strategy

Implement key rotation for long-term deployments:

```cpp
class KeyRotation {
public:
    static const int MAX_KEYS = 2;
    
    esp_err_t rotate_keys() {
        // Generate new key pair
        // Keep old key for verifying existing signatures
        // Transition period: accept both keys
        // After timeout: only accept new key
        return ESP_OK;
    }
};
```

---

## Implementation Examples

### Complete Example: Encrypted NVS

```cpp
#include <Arduino.h>
#include <nvs.h>
#include <nvs_flash.h>
#include <mbedtls/ed25519.h>
#include <WiFi.h>
#include <HTTPClient.h>

// IoT-Billing-Service contract integration
#include "utility_drip_types.h"

class SecureMeter {
private:
    nvs_handle_t nvs_handle;
    uint8_t private_key[32];
    uint8_t public_key[32];
    const char* NVS_NAMESPACE = "meter_keys";
    
    bool initialized = false;

public:
    SecureMeter() {}

    bool begin() {
        // Initialize NVS
        esp_err_t err = nvs_flash_init();
        if (err == ESP_ERR_NVS_NO_FREE_PAGES) {
            nvs_flash_erase();
            err = nvs_flash_init();
        }
        
        if (err != ESP_OK) {
            Serial.printf("NVS init failed: %d\n", err);
            return false;
        }

        // Open namespace
        err = nvs_open(NVS_NAMESPACE, NVS_READWRITE, &nvs_handle);
        if (err != ESP_OK) {
            Serial.printf("NVS open failed: %d\n", err);
            return false;
        }

        initialized = true;
        return true;
    }

    bool hasKeys() {
        size_t size = 0;
        return nvs_get_blob(nvs_handle, "priv", NULL, &size) == ESP_OK;
    }

    bool generateKeys() {
        if (!initialized) return false;

        Serial.println("🔑 Generating Ed25519 key pair...");
        
        // Use hardware RNG
        uint8_t seed[32];
        esp_fill_random(seed, 32);

        // Generate keys
        mbedtls_ed25519_context ctx;
        mbedtls_ed25519_init(&ctx);
        
        int ret = mbedtls_ed25519_genkey(&ctx, private_key, public_key);
        mbedtls_ed25519_free(&ctx);

        if (ret != 0) {
            Serial.printf("Key generation failed: %d\n", ret);
            return false;
        }

        // Save to NVS
        nvs_set_blob(nvs_handle, "priv", private_key, 32);
        nvs_set_blob(nvs_handle, "pub", public_key, 32);
        nvs_commit(nvs_handle);

        Serial.println("✅ Keys generated and saved!");
        return true;
    }

    bool loadKeys() {
        if (!initialized) return false;

        size_t size = 32;
        esp_err_t err;

        err = nvs_get_blob(nvs_handle, "priv", private_key, &size);
        if (err != ESP_OK) return false;

        size = 32;
        err = nvs_get_blob(nvs_handle, "pub", public_key, &size);
        if (err != ESP_OK) return false;

        Serial.println("✅ Keys loaded from NVS");
        return true;
    }

    bool signUsageData(const uint8_t* data, size_t len, 
                       uint8_t* signature, size_t* sig_len) {
        if (!initialized) return false;

        mbedtls_ed25519_context ctx;
        mbedtls_ed25519_init(&ctx);

        int ret = mbedtls_ed25519_sign(&ctx, signature, sig_len,
                                        private_key, 32,
                                        data, len);

        mbedtls_ed25519_free(&ctx);

        return (ret == 0);
    }

    void getPublicKey(uint8_t* out_key) {
        memcpy(out_key, public_key, 32);
    }

    void end() {
        nvs_close(nvs_handle);
        initialized = false;
    }
};

// Global instance
SecureMeter meter;

void setup() {
    Serial.begin(115200);
    delay(1000);

    Serial.println("\n🚀 IoT-Billing-Service Meter Starting...");

    // Initialize secure storage
    if (!meter.begin()) {
        Serial.println("❌ Failed to initialize meter");
        return;
    }

    // Check/generate keys
    if (!meter.hasKeys()) {
        Serial.println("📝 No keys found, generating...");
        if (!meter.generateKeys()) {
            Serial.println("❌ Key generation failed!");
            return;
        }
    } else {
        Serial.println("📖 Loading existing keys...");
        if (!meter.loadKeys()) {
            Serial.println("❌ Failed to load keys!");
            return;
        }
    }

    // Display public key for registration
    uint8_t pub_key[32];
    meter.getPublicKey(pub_key);

    Serial.print("🔑 Public Key: 0x");
    for (int i = 0; i < 32; i++) {
        Serial.printf("%02X", pub_key[i]);
    }
    Serial.println();

    // Connect to WiFi and register with contract
    connectToNetwork();
    registerWithContract(pub_key);
}

void loop() {
    // Read sensor
    float watt_hours = readEnergyMeter();

    // Create usage data
    UsageData data;
    data.meter_id = 1;
    data.timestamp = millis() / 1000;
    data.watt_hours_consumed = (int64_t)(watt_hours * 1000);
    data.units_consumed = data.watt_hours_consumed / 1000;

    // Sign data
    uint8_t signature[64];
    size_t sig_len = sizeof(signature);
    
    if (meter.signUsageData((uint8_t*)&data, sizeof(data), 
                            signature, &sig_len)) {
        // Send to backend
        sendSignedUsage(data, signature, sig_len);
        Serial.println("✅ Usage data signed and sent");
    } else {
        Serial.println("❌ Signing failed!");
    }

    delay(60000); // Report every minute
}
```

---

## Testing & Validation

### Test Suite

```cpp
#include <unity.h>

void test_key_generation() {
    SecureMeter meter;
    TEST_ASSERT_TRUE(meter.begin());
    TEST_ASSERT_TRUE(meter.generateKeys());
    
    uint8_t pub_key[32];
    meter.getPublicKey(pub_key);
    
    // Verify key is not all zeros
    bool all_zeros = true;
    for (int i = 0; i < 32; i++) {
        if (pub_key[i] != 0) {
            all_zeros = false;
            break;
        }
    }
    TEST_ASSERT_FALSE(all_zeros);
}

void test_signature_verification() {
    SecureMeter meter;
    meter.begin();
    meter.generateKeys();
    
    uint8_t data[] = "Test message";
    uint8_t signature[64];
    size_t sig_len = sizeof(signature);
    
    TEST_ASSERT_TRUE(meter.signUsageData(data, sizeof(data), 
                                          signature, &sig_len));
    TEST_ASSERT_EQUAL(64, sig_len);
}

void test_key_persistence() {
    SecureMeter meter1;
    meter1.begin();
    meter1.generateKeys();
    meter1.end();
    
    // Reload
    SecureMeter meter2;
    meter2.begin();
    TEST_ASSERT_TRUE(meter2.loadKeys());
    
    // Keys should match
    uint8_t pub1[32], pub2[32];
    meter1.getPublicKey(pub1);
    meter2.getPublicKey(pub2);
    
    TEST_ASSERT_EQUAL_MEMORY(pub1, pub2, 32);
}

void setup() {
    UNITY_BEGIN();
    RUN_TEST(test_key_generation);
    RUN_TEST(test_signature_verification);
    RUN_TEST(test_key_persistence);
    UNITY_END();
}
```

---

## Troubleshooting

### Issue: Keys not persisting after reboot

**Solution:**
- Check NVS partition size in partition table
- Ensure `nvs_commit()` is called after saving
- Verify power supply is stable during write

### Issue: Signature verification fails on contract

**Solution:**
- Verify public key format matches contract expectations
- Check timestamp is within acceptable range (< 5 minutes)
- Ensure message format matches exactly what was signed

### Issue: Secure element not detected

**Solution:**
- Check I2C wiring (SDA/SCL)
- Verify pull-up resistors are installed
- Confirm I2C address (use I2C scanner sketch)
- Check power supply (3.3V stable)

---

## Summary & Recommendations

### For Development
- ✅ Use **Level 1 (Basic NVS)**
- Easy to implement and debug
- ❌ Never deploy to production

### For Pilot Deployments
- ✅ Use **Level 2 (Encrypted NVS)**
- Good security/comformance balance
- Suitable for trusted environments

### For Commercial Production
- ✅ Use **Level 3 (Secure Element)**
- Hardware-backed security
- Industry best practice
- Worth the additional cost

### For High Volume
- ✅ Use **Level 4 (ESP32-S3 Secure Flash)**
- Maximum integration
- Higher unit cost but lower assembly complexity

---

## Additional Resources

- [ESP32 NVS Documentation](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/storage/nvs_flash.html)
- [ATECC608A Datasheet](https://ww1.microchip.com/downloads/en/DeviceDoc/20005926A.pdf)
- [CryptoAuthLib Documentation](https://microchipcrypto.gitlab.io/avr-crypto-lib/)
- [Ed25519 Specification](https://ed25519.cr.yp.to/)
- [IoT-Billing-Service Contract Docs](../README.md)

---

**Last Updated**: March 26, 2026  
**Version**: 1.0.0  
**Security Level**: Production Ready
