//! The pinned digest of a weights file, spec section 10.
//!
//! The catalogue pins every model by sha256, the way `cli.lock` pins the CLI
//! binary, and the `.part` file is renamed only when the digest matches. That
//! is one small hash over a multi-gigabyte file, so it is written here rather
//! than pulled in as a dependency: the release binary is about 13 MB and this
//! is the only thing in it that needs sha256 at all.

use std::io::Read;
use std::path::Path;

/// Hex sha256 of these bytes.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

pub fn sha256_path(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    filled: usize,
    bit_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            filled: 0,
            bit_len: 0,
        }
    }

    /// Take these bytes a block at a time.
    ///
    /// The weights are gigabytes, so this walks whole 64-byte blocks rather
    /// than single bytes: one bounds check and one branch per block instead of
    /// per byte. The digest is the same either way.
    fn update(&mut self, data: &[u8]) {
        let mut rest = data;

        // Finish the block a previous call left part filled.
        if self.filled > 0 {
            let wanted = (64 - self.filled).min(rest.len());
            self.buffer[self.filled..self.filled + wanted].copy_from_slice(&rest[..wanted]);
            self.filled += wanted;
            rest = &rest[wanted..];
            if self.filled < 64 {
                return;
            }
            self.compress();
            self.bit_len += 512;
            self.filled = 0;
        }

        let mut blocks = rest.chunks_exact(64);
        for block in &mut blocks {
            self.buffer.copy_from_slice(block);
            self.compress();
            self.bit_len += 512;
        }

        // Whatever is left over waits for the next call or for `finalize`.
        let tail = blocks.remainder();
        self.buffer[..tail.len()].copy_from_slice(tail);
        self.filled = tail.len();
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.bit_len + (self.filled as u64) * 8;
        self.buffer[self.filled] = 0x80;
        self.filled += 1;
        if self.filled > 56 {
            self.buffer[self.filled..].fill(0);
            self.compress();
            self.buffer.fill(0);
            self.filled = 0;
        }
        self.buffer[self.filled..56].fill(0);
        self.buffer[56..].copy_from_slice(&bit_len.to_be_bytes());
        self.compress();

        let mut out = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..][..4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];

        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().enumerate().take(16) {
            let start = index * 4;
            *word = u32::from_be_bytes([
                self.buffer[start],
                self.buffer[start + 1],
                self.buffer[start + 2],
                self.buffer[start + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_empty_and_abc_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }

    /// The vectors above all fit in one block, so none of them reaches the two
    /// branches every real weights file takes: the compress in `update` when
    /// the buffer fills, and the two-block padding in `finalize` when the last
    /// block has no room for the length. These are the published FIPS 180-4
    /// vectors, so the expected digests are right independently of this code.
    #[test]
    fn sha256_matches_the_multi_block_vectors() {
        // 56 bytes: one block plus a tail with no room for the 8-byte length,
        // which is the two-block padding path.
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // 112 bytes: an exact two blocks, so `update` compresses twice and the
        // padding takes a third block of its own.
        const TWO_BLOCKS: &[u8] = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";
        assert_eq!(TWO_BLOCKS.len(), 112);
        assert_eq!(
            sha256_hex(TWO_BLOCKS),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
        // A million bytes, which is the streaming case a 2.5 GB file is.
        assert_eq!(
            sha256_hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// `update` takes whole blocks at a time, so a call that ends part way
    /// through one has to carry the remainder into the next call. A short read
    /// is what does that to a real file, and the digest must not notice.
    #[test]
    fn a_vector_split_across_calls_hashes_the_same_as_one_call() {
        const TWO_BLOCKS: &[u8] = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";

        // Every split, so no offset inside or across a block goes untried.
        for split in 0..=TWO_BLOCKS.len() {
            let mut hasher = Sha256::new();
            hasher.update(&TWO_BLOCKS[..split]);
            hasher.update(&TWO_BLOCKS[split..]);
            assert_eq!(
                hex_encode(&hasher.finalize()),
                "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
                "split at {split}"
            );
        }
    }

    /// `sha256_path` reads in 64 KB chunks, so a file larger than one chunk is
    /// the only thing that proves a block split across two reads still hashes
    /// to the same digest as the whole buffer.
    #[test]
    fn a_file_larger_than_one_read_hashes_the_same_as_its_bytes() {
        let directory = scratch("digest-stream");
        let path = directory.join("weights.bin");
        let bytes: Vec<u8> = (0..200_000u32).map(|index| (index % 251) as u8).collect();
        std::fs::write(&path, &bytes).expect("the file is written");

        assert_eq!(
            sha256_path(&path).expect("the file is read"),
            sha256_hex(&bytes)
        );
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("grammachy-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("the scratch directory is created");
        directory
    }
}
