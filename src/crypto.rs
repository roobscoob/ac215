use aes::Aes128;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};

/// Hardcoded key seed from the AC-825 firmware.
const KEY_SEED: [u8; 16] = [
    0xA3, 0x95, 0x0F, 0x1A, 0x86, 0x20, 0x2E, 0xEE, 0xD1, 0x64, 0x94, 0x13, 0xB5, 0x75, 0x46, 0x9E,
];

/// CRC-like bit scrambler used to derive the AES key from the seed.
/// Runs a 14-iteration LFSR with polynomial 0xB400, then applies a
/// fixed bit permutation.
fn scramble_pair(a: u8, b: u8) -> u16 {
    let mut val = ((a as u16) << 8) | (b as u16);
    for _ in 0..14 {
        val = if val & 1 != 0 {
            (val >> 1) ^ 0xB400
        } else {
            val >> 1
        };
    }
    ((val & 0x8000) >> 10)
        | ((val & 0x0020) << 5)
        | ((val & 0x0400) >> 8)
        | ((val & 0x0004) << 6)
        | ((val & 0x0100) << 7)
        | (val & 0x7ADB)
}

/// Derives the static AES-128 key from the hardcoded seed.
pub fn derive_key() -> [u8; 16] {
    let mut key = [0u8; 16];

    for i in (0..16).step_by(2) {
        let val = scramble_pair(KEY_SEED[i], KEY_SEED[i + 1]);
        key[i] = (val >> 8) as u8;
        key[i + 1] = (val & 0xFF) as u8;
    }

    key
}

/// AES-128-ECB cipher for AC-825 protocol packets.
#[derive(Clone)]
pub struct Cipher {
    inner: Aes128,
    key: [u8; 16],
}

impl Cipher {
    /// Create a cipher using the default static key.
    pub fn new() -> Self {
        let key = derive_key();
        Self::with_key(key)
    }

    /// Create a cipher with a specific 16-byte key.
    pub fn with_key(key: [u8; 16]) -> Self {
        let inner = Aes128::new(GenericArray::from_slice(&key));
        Self { inner, key }
    }

    /// Returns the raw AES key bytes.
    pub fn key(&self) -> &[u8; 16] {
        &self.key
    }

    /// Encrypt `data` in place using AES-128-ECB.
    /// `data.len()` must be a multiple of 16.
    ///
    /// # Panics
    /// Panics if `data.len()` is not a multiple of 16.
    pub fn encrypt(&self, data: &mut [u8]) {
        assert!(data.len() % 16 == 0, "data must be 16-byte aligned");
        for chunk in data.chunks_exact_mut(16) {
            let block = GenericArray::from_mut_slice(chunk);
            self.inner.encrypt_block(block);
        }
    }

    /// Decrypt `data` in place using AES-128-ECB.
    /// `data.len()` must be a multiple of 16.
    ///
    /// # Panics
    /// Panics if `data.len()` is not a multiple of 16.
    pub fn decrypt(&self, data: &mut [u8]) {
        assert!(data.len() % 16 == 0, "data must be 16-byte aligned");
        for chunk in data.chunks_exact_mut(16) {
            let block = GenericArray::from_mut_slice(chunk);
            self.inner.decrypt_block(block);
        }
    }
}

impl Default for Cipher {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cipher")
            .field("key", &format_args!("{:02X?}", self.key))
            .finish()
    }
}

/// Round up to the next multiple of 16.
pub fn pad_len(len: usize) -> usize {
    let rem = len & 0xF;
    if rem == 0 { len } else { len - rem + 16 }
}

/// Copy `src` into a zero-padded vec with length rounded up to 16 bytes.
pub fn pad_to_block(src: &[u8]) -> Vec<u8> {
    let mut buf = vec![0u8; pad_len(src.len())];
    buf[..src.len()].copy_from_slice(src);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_derivation() {
        let key = derive_key();
        assert_eq!(
            key,
            [
                0x94, 0x7F, 0xEB, 0x1C, 0x9D, 0x56, 0xED, 0xD8, 0x6E, 0x69, 0x8B, 0xF6, 0xD5, 0xB3,
                0xF4, 0x7B
            ]
        );
    }

    #[test]
    fn roundtrip() {
        let cipher = Cipher::new();
        let original = b"hello ac825!\0\0\0\0"; // 16 bytes
        let mut data = *original;
        cipher.encrypt(&mut data);
        assert_ne!(&data, original);
        cipher.decrypt(&mut data);
        assert_eq!(&data, original);
    }
}
