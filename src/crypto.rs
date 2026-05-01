use sha2::{Sha256, Digest};
pub fn double_sha256(data: &[u8]) -> [u8; 32] {
let mut hasher = Sha256::new();
 hasher.update(&data);
 let hasher1 = hasher.finalize();

 let mut hasher = Sha256::new();
 hasher.update(&hasher1);
 let hash2 = hasher.finalize();
 // use the Into trait to convert from generic array to [u8; 32]
  hash2.into() 

}
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}