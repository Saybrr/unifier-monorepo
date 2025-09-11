use base64;

fn main() {
    let base64_hash = "h5C1sOVi8K8=";
    let actual_hash = "5e67475585765e6b9e90904efabd972d";

    // Decode base64 to bytes then to hex
    match base64::decode(base64_hash) {
        Ok(bytes) => {
            let hex_from_base64 = bytes.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();

            println!("Base64 hash: {}", base64_hash);
            println!("Decoded to hex: {}", hex_from_base64);
            println!("Actual computed hash: {}", actual_hash);
            println!("Match: {}", hex_from_base64 == actual_hash);
        }
        Err(e) => {
            println!("Error decoding base64: {}", e);
        }
    }
}
