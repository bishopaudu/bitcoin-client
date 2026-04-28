
// ─── SHA256 implementation ────────────────────────────────────────────────────
// We implement SHA256 ourselves to avoid external dependencies.
// This is the standard SHA-256 algorithm as specified in FIPS 180-4.

// SHA256 initial hash values — the first 32 bits of the fractional parts
// of the square roots of the first 8 primes.
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

// SHA256 round constants — first 32 bits of fractional parts of
// the cube roots of the first 64 primes.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

// Compute SHA256 of a byte slice. Returns 32 bytes.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    // ── Step 1: Pre-processing (padding) ──────────────────────────────────
    // SHA256 works on 512-bit (64-byte) blocks. We must pad the message to
    // a multiple of 64 bytes, following a specific rule:
    //   - Append a single '1' bit (0x80 byte)
    //   - Append zeros until message length ≡ 448 (mod 512) bits
    //   - Append the original message length in bits as a 64-bit big-endian int

    let msg_len = data.len();
    let bit_len = (msg_len as u64) * 8; // Original message length in bits

    let mut padded = data.to_vec();

    // Append the 0x80 byte (represents the '1' bit followed by 7 zero bits)
    padded.push(0x80);

    // Pad with zeros until length ≡ 56 (mod 64), leaving 8 bytes for length
    while padded.len() % 64 != 56 {
        padded.push(0x00);
    }

    // Append original length in bits as big-endian 64-bit integer
    // Note: SHA256 uses BIG-endian for the length field (unlike Bitcoin's
    // general little-endian convention for its own fields)
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // ── Step 2: Process each 64-byte block ────────────────────────────────
    // Initialize hash values from the constants above
    let mut hash = H0;

    // Process each 512-bit chunk
    for chunk in padded.chunks(64) {
        // Create the message schedule: 64 words of 32 bits each
        let mut w = [0u32; 64];

        // First 16 words: read directly from the chunk as big-endian u32s
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }

        // Remaining 48 words: derived from previous words using bitwise ops
        // σ0 and σ1 are the SHA256 "sigma" functions
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7)
                ^ w[i - 15].rotate_right(18)
                ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17)
                ^ w[i - 2].rotate_right(19)
                ^ (w[i - 2] >> 10);
            // wrapping_add prevents overflow panics in debug mode
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Initialize working variables from current hash state
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = hash;

        // 64 rounds of compression
        for i in 0..64 {
            // Σ1: uppercase sigma function on e
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            // Choice function: if e then f else g
            let ch = (e & f) ^ (!e & g);
            // Temporary word 1
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            // Σ0: uppercase sigma function on a
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            // Majority function: majority vote of a, b, c
            let maj = (a & b) ^ (a & c) ^ (b & c);
            // Temporary word 2
            let temp2 = s0.wrapping_add(maj);

            // Rotate the working variables
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        // Add compressed chunk to current hash value (mod 2^32 via wrapping_add)
        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(h);
    }

    // ── Step 3: Produce the final hash ────────────────────────────────────
    // Convert the 8 x u32 words to a 32-byte array in big-endian order
    let mut result = [0u8; 32];
    for (i, word) in hash.iter().enumerate() {
        result[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    result
}

// Compute SHA256(SHA256(data)) — Bitcoin's standard double hash
pub fn double_sha256(data: &[u8]) -> [u8; 32] {
    sha256(&sha256(data))
}
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}