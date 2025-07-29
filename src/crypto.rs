use aes::Aes256;
use block_modes::{BlockMode, Cbc};
use block_modes::block_padding::Pkcs7;
use rand::{rngs::OsRng, RngCore};
use base64::{engine::general_purpose::STANDARD, Engine as _};

type Aes256Cbc = Cbc<Aes256, Pkcs7>;
const KEY: &[u8; 32] = b"01234567012345670123456701234567";
const IV_LEN: usize = 16;

pub fn encrypt(plaintext: &str) -> Option<String> {
    let mut iv = [0u8; IV_LEN];
    OsRng.fill_bytes(&mut iv);

    let cipher = Aes256Cbc::new_from_slices(KEY, &iv).ok()?;
    let encrypted = cipher.encrypt_vec(plaintext.as_bytes());

    let mut combined = Vec::with_capacity(IV_LEN + encrypted.len());
    combined.extend_from_slice(&iv);
    combined.extend_from_slice(&encrypted);

    Some(STANDARD.encode(combined))
}

pub fn decrypt(encoded: &str) -> Option<String> {
    let data = STANDARD.decode(encoded).ok()?;
    if data.len() < IV_LEN {
        return None;
    }

    let (iv, ciphertext) = data.split_at(IV_LEN);
    let cipher = Aes256Cbc::new_from_slices(KEY, iv).ok()?;
    let decrypted = cipher.decrypt_vec(ciphertext).ok()?;

    String::from_utf8(decrypted).ok()
}
