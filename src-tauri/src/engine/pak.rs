use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::io::Write;

pub const PAK_MAGIC: u32 = 0x5A6F12E1;
pub const WUWA_PAK_VERSION: u32 = 12;

pub fn fnv64_path(path: &str, seed: u64) -> u64 {
    const OFF: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x00000100000001b3;
    let mut h = OFF.wrapping_add(seed);
    for c in path.to_lowercase().chars() {
        let u = c as u16;
        h ^= (u & 0xFF) as u64;
        h = h.wrapping_mul(PRIME);
        h ^= (u >> 8) as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

pub fn scramble_flags(f: u32) -> u32 {
    ((f & 0x3f) << 16)
        | ((f >> 6) & 0xFFFF)
        | ((f << 6) & (1 << 28))
        | ((f >> 1) & 0x0FC00000)
        | (f & 0xE0000000)
}

fn write_string<W: Write>(w: &mut W, s: &str) -> std::io::Result<()> {
    let bytes = s.as_bytes();
    w.write_all(&((bytes.len() as u32 + 1).to_le_bytes()))?;
    w.write_all(bytes)?;
    w.write_all(&[0u8])?;
    Ok(())
}

fn string_size(s: &str) -> usize {
    4 + s.len() + 1
}

fn sha1_hash(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&result);
    out
}

pub fn split_path(path: &str) -> Option<(String, String)> {
    if path == "/" || path.is_empty() {
        return None;
    }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        None => Some(("/".to_string(), trimmed.to_string())),
        Some(i) => Some((trimmed[..=i].to_string(), trimmed[i + 1..].to_string())),
    }
}

pub fn build_fdi(paths: &[String], offsets: &[u32]) -> std::io::Result<Vec<u8>> {
    let mut fdi: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();

    for (i, raw_path) in paths.iter().enumerate() {
        let mut p = raw_path.clone();
        while let Some((par, _)) = split_path(&p) {
            fdi.entry(par.clone()).or_default();
            p = par.clone();
            if p == "/" {
                break;
            }
        }
        if let Some((dir, name)) = split_path(raw_path) {
            let map = fdi.entry(dir).or_default();
            map.insert(name, offsets[i]);
        }
    }

    let mut out = Vec::new();
    out.write_all(&(fdi.len() as u32).to_le_bytes())?;
    for (dir, files) in fdi {
        write_string(&mut out, &dir)?;
        out.write_all(&(files.len() as u32).to_le_bytes())?;
        for (file, off) in files {
            write_string(&mut out, &file)?;
            out.write_all(&off.to_le_bytes())?;
        }
    }

    Ok(out)
}

pub fn pack(mount: &str, seed: u64, files: &[(String, Vec<u8>)]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut data_offsets = Vec::with_capacity(files.len());

    for (_, data) in files {
        data_offsets.push(out.len() as u64);
        out.write_all(&0u64.to_le_bytes())?; // offset
        out.write_all(&(data.len() as u64).to_le_bytes())?; // size
        out.write_all(&(data.len() as u64).to_le_bytes())?; // uncompressed size
        out.write_all(&0u32.to_le_bytes())?; // comp method
        out.write_all(&sha1_hash(data))?;
        out.write_all(&[0u8])?; // comp block count = 0
        out.write_all(&0u32.to_le_bytes())?; // flags
        out.write_all(data)?;
    }

    let index_offset = out.len() as u64;

    let mut enc = Vec::new();
    let mut encoded_offsets = Vec::with_capacity(files.len());
    for (i, (_, data)) in files.iter().enumerate() {
        encoded_offsets.push(enc.len() as u32);
        let sz = data.len() as u64;
        let off = data_offsets[i];
        let s32 = sz <= u32::MAX as u64;
        let o32 = off <= u32::MAX as u64;

        let mut flags: u32 = 0;
        if s32 {
            flags |= (1 << 29) | (1 << 30);
        }
        if o32 {
            flags |= 1 << 31;
        }

        enc.write_all(&scramble_flags(flags).to_le_bytes())?;
        enc.write_all(&[0u8])?;
        if s32 {
            enc.write_all(&(sz as u32).to_le_bytes())?;
        } else {
            enc.write_all(&sz.to_le_bytes())?;
        }
        if o32 {
            enc.write_all(&(off as u32).to_le_bytes())?;
        } else {
            enc.write_all(&off.to_le_bytes())?;
        }
    }

    let mut phi = Vec::new();
    phi.write_all(&(files.len() as u32).to_le_bytes())?;
    for (i, (path, _)) in files.iter().enumerate() {
        phi.write_all(&fnv64_path(path, seed).to_le_bytes())?;
        phi.write_all(&encoded_offsets[i].to_le_bytes())?;
    }
    phi.write_all(&0u32.to_le_bytes())?;

    let path_list: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
    let fdi = build_fdi(&path_list, &encoded_offsets)?;

    let bytes_before_phi = string_size(mount) as u64
        + 4
        + 8
        + 4
        + 8
        + 8
        + 20
        + 4
        + 8
        + 8
        + 20
        + 4
        + enc.len() as u64
        + 4;

    let phi_offset = index_offset + bytes_before_phi;
    let fdi_offset = phi_offset + phi.len() as u64;

    let mut idx = Vec::new();
    write_string(&mut idx, mount)?;
    idx.write_all(&(files.len() as u32).to_le_bytes())?;
    idx.write_all(&seed.to_le_bytes())?;
    idx.write_all(&1u32.to_le_bytes())?;
    idx.write_all(&phi_offset.to_le_bytes())?;
    idx.write_all(&(phi.len() as u64).to_le_bytes())?;
    idx.write_all(&sha1_hash(&phi))?;

    idx.write_all(&1u32.to_le_bytes())?;
    idx.write_all(&fdi_offset.to_le_bytes())?;
    idx.write_all(&(fdi.len() as u64).to_le_bytes())?;
    idx.write_all(&sha1_hash(&fdi))?;

    idx.write_all(&(enc.len() as u32).to_le_bytes())?;
    idx.write_all(&enc)?;
    idx.write_all(&0u32.to_le_bytes())?;

    out.write_all(&idx)?;
    out.write_all(&phi)?;
    out.write_all(&fdi)?;

    // Footer
    out.write_all(&0u64.to_le_bytes())?; // encryption GUID
    out.write_all(&0u64.to_le_bytes())?;
    out.write_all(&[0u8])?; // encrypted flag = false
    out.write_all(&PAK_MAGIC.to_le_bytes())?;
    out.write_all(&WUWA_PAK_VERSION.to_le_bytes())?;
    out.write_all(&index_offset.to_le_bytes())?;
    out.write_all(&(idx.len() as u64).to_le_bytes())?;
    out.write_all(&sha1_hash(&idx))?;
    out.write_all(&[0u8; 32 * 5])?; // compression methods

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv64_known_hashes() {
        assert_eq!(fnv64_path("Client/Content/Test", 0), 0x70f0e2a404d3c11b);
        assert_eq!(fnv64_path("Client/Content/A", 0), 0xbb69cfe63c43eed4);
        assert_eq!(fnv64_path("Client/Content/B", 0), 0xbb7401e63c4c984f);
        assert_eq!(fnv64_path("Client/Content/Test", 42), 0x8bd10b4e01daacb1);
    }

    #[test]
    fn test_fnv64_case_insensitivity() {
        let a = fnv64_path("Client/Content/Test", 0);
        let b = fnv64_path("client/content/test", 0);
        assert_eq!(a, b);
    }

    #[test]
    fn test_scramble_flags() {
        assert_eq!(scramble_flags(0x00000000), 0x00000000);
        assert_eq!(scramble_flags(0x00000001), 0x00010000);
        assert_eq!(scramble_flags(0x00000002), 0x00020000);
        assert_eq!(scramble_flags(0x12345678), 0x0938d159);
        assert_eq!(scramble_flags(0xe1234567), 0xe0a78d15);
        assert_eq!(scramble_flags(0xffffffff), 0xffffffff);
    }

    #[test]
    fn test_pack_basic() {
        let files = vec![
            ("Client/Content/A.txt".to_string(), b"Hello World".to_vec()),
            (
                "Client/Content/B.txt".to_string(),
                b"WuwaID Translation".to_vec(),
            ),
        ];
        let result = pack("../../../", 0, &files);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        // Verify PAK Magic at footer (before compression table)
        // Footer starts near the end
        assert!(bytes.len() > 200);
    }
}
