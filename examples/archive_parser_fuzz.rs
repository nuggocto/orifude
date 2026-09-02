use std::io::{self, Read};

use orifude::packs::{MAX_ARCHIVE_BYTES, validate_archive_bytes};

fn main() -> io::Result<()> {
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_ARCHIVE_BYTES as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() <= MAX_ARCHIVE_BYTES {
        let _result = validate_archive_bytes(&bytes);
    }
    Ok(())
}
