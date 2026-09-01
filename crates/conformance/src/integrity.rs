//! Deterministic SHA-256 identities used by conformance evidence.

use std::path::Path;

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = sha256(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

// Small self-contained SHA-256 implementation. Conformance source identity is
// on the hot cache-validation path, so this cannot shell out once per corpus
// file, and adding a network-fetched build dependency would weaken bootstrap
// reproducibility.
fn sha256(bytes: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const ROUND: [u32; 64] = [
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

    let bit_length = (bytes.len() as u64).wrapping_mul(8);
    let padded_length = (bytes.len() + 1 + 8).div_ceil(64) * 64;
    let mut padded = Vec::with_capacity(padded_length);
    padded.extend_from_slice(bytes);
    padded.push(0x80);
    padded.resize(padded_length - 8, 0);
    padded.extend_from_slice(&bit_length.to_be_bytes());

    let mut state = INITIAL;
    for chunk in padded.as_chunks::<64>().0 {
        let mut schedule = [0_u32; 64];
        for (index, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            schedule[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(ROUND[index])
                .wrapping_add(schedule[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut output = [0_u8; 32];
    for (chunk, word) in output.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    output
}

pub fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Match the pinned-oracle resolver's package-tree hash: recursively visit
/// directory entries by lexical name and frame each regular file as
/// `<slash-relative-path>\0<exact-bytes>\0`. Symlinks and special files are
/// rejected instead of being followed.
pub fn sha256_directory(directory: &Path) -> anyhow::Result<String> {
    fn visit(root: &Path, current: &Path, framed: &mut Vec<u8>) -> anyhow::Result<()> {
        let mut entries = std::fs::read_dir(current)?
            .map(|entry| {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("oracle package path is not UTF-8"))?;
                Ok((name, entry))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        for (_name, entry) in entries {
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                visit(root, &path, framed)?;
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)?
                    .components()
                    .map(|component| {
                        component
                            .as_os_str()
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("oracle package path is not UTF-8"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?
                    .join("/");
                framed.extend_from_slice(relative.as_bytes());
                framed.push(0);
                framed.extend_from_slice(&std::fs::read(&path)?);
                framed.push(0);
            } else {
                anyhow::bail!("unsupported oracle package entry: {}", path.display());
            }
        }
        Ok(())
    }

    let mut framed = Vec::new();
    visit(directory, directory, &mut framed)?;
    Ok(sha256_bytes(&framed))
}

/// Hash sorted candidate facts with length framing so no path, disposition, or
/// source identity can be re-partitioned into an equivalent byte stream.
pub fn candidate_content_sha256(records: &[(String, String, String)]) -> String {
    let mut records = records.to_vec();
    records.sort();
    let mut framed = Vec::new();
    framed.extend_from_slice(b"tsz-conformance-candidate-content-v1\0");
    for (path, disposition, source_sha256) in records {
        for value in [path, disposition, source_sha256] {
            framed.extend_from_slice(&(value.len() as u64).to_be_bytes());
            framed.extend_from_slice(value.as_bytes());
        }
    }
    sha256_bytes(&framed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_standard_vector() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn candidate_digest_preserves_source_and_classification() {
        let base = vec![("a.ts".to_string(), "runnable".to_string(), "00".repeat(32))];
        let mut changed_source = base.clone();
        changed_source[0].2 = "11".repeat(32);
        let mut changed_class = base.clone();
        changed_class[0].1 = "skipped:@skip".to_string();
        assert_ne!(
            candidate_content_sha256(&base),
            candidate_content_sha256(&changed_source)
        );
        assert_ne!(
            candidate_content_sha256(&base),
            candidate_content_sha256(&changed_class)
        );
    }

    #[test]
    fn directory_hash_preserves_recursive_paths_and_exact_bytes() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(temp.path().join("dir")).expect("directory");
        std::fs::write(temp.path().join("a.txt"), b"one").expect("a");
        std::fs::write(temp.path().join("dir/b.txt"), b"two\n").expect("b");
        assert_eq!(
            sha256_directory(temp.path()).expect("tree hash"),
            sha256_bytes(b"a.txt\0one\0dir/b.txt\0two\n\0")
        );
    }
}
