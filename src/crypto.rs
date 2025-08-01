use std::collections::HashMap;
use std::fs::{self};
use std::io::{self};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use once_cell::sync::OnceCell;
use base64::{engine::general_purpose, Engine as _};
use rand::rngs::OsRng;
use rsa::pkcs8::DecodePrivateKey;
use rsa::{pkcs8::EncodePublicKey, RsaPrivateKey, Oaep, RsaPublicKey};
use pkcs8::{DecodePublicKey, EncodePrivateKey};
use sha2::Sha256;

// Client holds its own private key in memory
static MY_PRIVKEY: OnceCell<RsaPrivateKey> = OnceCell::new();

#[derive(serde::Deserialize, serde::Serialize)]
struct PairJSON {
    pubkey: String,
    privkey: String,
}

pub fn generate_or_load_keys(username: &str) -> io::Result<String> {
    // Ensure /data/keys.json exists with correct permissions
    if !Path::new("data").exists() {
        fs::create_dir_all("data")?;
    }
    if !Path::new("data/keys.json").exists() {
        fs::write("data/keys.json", b"{}")?;
        #[cfg(unix)]
        fs::set_permissions("data/keys.json", fs::Permissions::from_mode(0o600))?;
    }

    // Load existing key map; tolerate empty/corrupt JSON
    let mut map: HashMap<String, PairJSON> = {
        let raw = fs::read_to_string("data/keys.json").unwrap_or_default();
        if raw.trim().is_empty() {
            HashMap::new()
        } else {
            serde_json::from_str(&raw).unwrap_or_else(|e| {
                eprintln!("keys.json corrupt ({e}), starting fresh");
                HashMap::new()
            })
        }
    };

    // Return existing key if present
    if let Some(pair) = map.get(username) {
        let priv_der = general_purpose::STANDARD
            .decode(&pair.privkey)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Bad base64 in keys.json"))?;
        let priv_key = RsaPrivateKey::from_pkcs8_der(&priv_der)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Bad private key: {e}")))?;
        let _ = MY_PRIVKEY.set(priv_key);
        return Ok(pair.pubkey.clone());
    }

    // Generate new key pair
    let priv_key = RsaPrivateKey::new(&mut OsRng, 1024)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("RSA gen failed: {e}")))?;

    let pub_key_der = priv_key
        .to_public_key()
        .to_public_key_der()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Public key encode failed: {e}")))?
        .as_bytes()
        .to_vec();
    let priv_key_der = priv_key
        .to_pkcs8_der()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("PKCS8 encode failed: {e}")))?
        .as_bytes()
        .to_vec();

    let pub_b64  = general_purpose::STANDARD.encode(pub_key_der);
    let priv_b64 = general_purpose::STANDARD.encode(priv_key_der);

    // Store client's private key in memory
    let _ = MY_PRIVKEY.set(priv_key);

    // Persist to keys.json atomically
    map.insert(
        username.to_owned(),
        PairJSON { pubkey: pub_b64.clone(), privkey: priv_b64 },
    );
    let json = serde_json::to_string_pretty(&map)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON encode failed: {e}")))?;
    fs::write("data/keys.json", json)?;
    #[cfg(unix)]
    fs::set_permissions("data/keys.json", fs::Permissions::from_mode(0o600))?;

    Ok(pub_b64)
}

pub fn encrypt(msg: &str, recipient_pubkey: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Decode recipient’s pubkey
    let der = general_purpose::STANDARD.decode(recipient_pubkey)?;
    let pub_key = RsaPublicKey::from_public_key_der(&der)?;

    // Encrypt with RSA-OAEP-SHA256
    let ciphertext = pub_key.encrypt(&mut OsRng, Oaep::new::<Sha256>(), msg.as_bytes())?;

    // Return Base64 ciphertext
    Ok(general_purpose::STANDARD.encode(ciphertext))
}

pub fn decrypt(msg: &str, recipient_privkey: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Decode recipient's privkey
    let der = general_purpose::STANDARD.decode(recipient_privkey)?;
    let priv_key = RsaPrivateKey::from_pkcs8_der(&der)?;

    // Decode ciphertext from Base64 to bytes
    let cipherbytes = general_purpose::STANDARD.decode(msg)?;

    // Decrypt with RSA-OAEP-SHA256
    let plaintext_bytes = priv_key.decrypt(Oaep::new::<Sha256>(), &cipherbytes)?;

    // Convert from UTF-8 bytes and return String
    Ok(String::from_utf8(plaintext_bytes)?)
}
