//! Small lossless RGBA PNG writer, using stored DEFLATE blocks (no dependencies).
//! For occasional small diagnostic images only, never in the live frame loop.
use std::{fs::OpenOptions, io::{self, Write}, path::Path};
fn crc32(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes { c ^= b as u32; for _ in 0..8 { c = (c >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(c & 1)); } }
    !c
}
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len(); out.extend_from_slice(tag); out.extend_from_slice(data);
    let crc = crc32(&out[start..]); out.extend_from_slice(&crc.to_be_bytes());
}
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> io::Result<Vec<u8>> {
    let count = width as u64 * height as u64;
    if count == 0 || count > 4_194_304 || rgba.len() as u64 != count * 4 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid PNG dimensions"));
    }
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks_exact(width as usize * 4) { raw.push(0); raw.extend_from_slice(row); }
    let mut z = vec![0x78, 0x01];
    let blocks = raw.chunks(65535); let n = blocks.len();
    for (i, block) in blocks.enumerate() {
        z.push(if i + 1 == n { 1 } else { 0 });
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes()); z.extend_from_slice(&(!len).to_le_bytes()); z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &v in &raw { a = (a + v as u32) % 65521; b = (b + a) % 65521; }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new(); ihdr.extend_from_slice(&width.to_be_bytes()); ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr); chunk(&mut out, b"sRGB", &[0]); chunk(&mut out, b"IDAT", &z); chunk(&mut out, b"IEND", &[]);
    Ok(out)
}
pub fn write(path: &Path, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
    let bytes = encode(width, height, rgba)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&bytes)
}
pub fn write_replace(path: &Path, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
    let bytes = encode(width, height, rgba)?;
    let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(path)?;
    file.write_all(&bytes)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn crc_known_vector() { assert_eq!(crc32(b"123456789"), 0xcbf43926); }
    #[test] fn png_bounds() {
        assert!(encode(0, 1, &[]).is_err()); assert!(encode(u32::MAX, u32::MAX, &[]).is_err());
        assert!(encode(1, 1, &[1, 2, 3]).is_err());
        let p = encode(1, 1, &[1, 2, 3, 255]).unwrap(); assert_eq!(&p[..8], b"\x89PNG\r\n\x1a\n");
    }
    #[test] fn replace_is_explicit() {
        let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path=std::env::temp_dir().join(format!("glass-png-replace-{}-{stamp}.png",std::process::id()));
        write(&path,1,1,&[1,2,3,255]).unwrap();
        assert_eq!(write(&path,1,1,&[4,5,6,255]).unwrap_err().kind(),io::ErrorKind::AlreadyExists);
        let before=std::fs::read(&path).unwrap();
        write_replace(&path,1,1,&[4,5,6,255]).unwrap();
        let after=std::fs::read(&path).unwrap();
        assert_ne!(before,after);
        std::fs::remove_file(path).unwrap();
    }
}
