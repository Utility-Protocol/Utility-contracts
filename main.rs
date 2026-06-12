use chrono::Utc;
use clap::Parser;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(author, version, about = "IoT Payload Generator for IoT-Billing-Service Contracts", long_about = None)]
struct Args {
    /// Meter ID to simulate
    #[arg(short, long, default_value_t = 1)]
    meter_id: u64,

    /// Watt hours consumed since last reading
    #[arg(short, long, default_value_t = 1500)]
    watt_hours: u64,

    /// Abstract units consumed
    #[arg(short, long, default_value_t = 1)]
    units: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct MessageData {
    meter_id: u64,
    timestamp: u64,
    watt_hours_consumed: u64,
    units_consumed: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct IotPayload {
    #[serde(flatten)]
    data: MessageData,
    signature: String,
    public_key: String,
}

fn main() {
    let args = Args::parse();
    let mut csprng = OsRng;

    // Generate an ed25519 keypair securely
    let signing_key: SigningKey = SigningKey::generate(&mut csprng);
    let verifying_key: VerifyingKey = (&signing_key).into();

    let payload_data = MessageData {
        meter_id: args.meter_id,
        timestamp: Utc::now().timestamp() as u64,
        watt_hours_consumed: args.watt_hours,
        units_consumed: args.units,
    };

    // Generate ed25519 signature of the serialized JSON
    let message_bytes = serde_json::to_vec(&payload_data).unwrap();
    let signature = signing_key.sign(&message_bytes);

    let sig_hex = signature.to_bytes().iter().map(|b| format!("{:02x}", b)).collect::<String>();
    let pub_hex = verifying_key.to_bytes().iter().map(|b| format!("{:02x}", b)).collect::<String>();

    let final_payload = IotPayload {
        data: payload_data,
        signature: sig_hex,
        public_key: pub_hex,
    };

    let json_output = serde_json::to_string_pretty(&final_payload).unwrap();
    println!("{}", json_output);
}