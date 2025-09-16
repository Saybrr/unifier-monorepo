use base64;

fn main() {
    let test_data = b"Hello, World!";

    // Calculate xxHash64 of data and return as base64 string (matching Wabbajack format)
    let hash = xxhash_rust::xxh64::xxh64(test_data, 0);
    let bytes = hash.to_le_bytes();
    let base64_hash = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);

    println!("Content: {:?}", std::str::from_utf8(test_data).unwrap());
    println!("XXHash64: {}", hash);
    println!("Base64 hash: {}", base64_hash);

    // Also check what "dGVzdA==" decodes to
    let wrong_hash = "dGVzdA==";
    match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, wrong_hash) {
        Ok(bytes) => {
            if let Ok(decoded) = std::str::from_utf8(&bytes) {
                println!("\"dGVzdA==\" decodes to: {:?}", decoded);
            } else {
                println!("\"dGVzdA==\" decodes to bytes: {:?}", bytes);
            }
        }
        Err(e) => {
            println!("Error decoding dGVzdA==: {}", e);
        }
    }
}
