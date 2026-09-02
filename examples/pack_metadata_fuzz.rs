use std::io::{self, Read};

use orifude::packs::{MAX_METADATA_BYTES, validate_metadata_bytes};

fn main() -> io::Result<()> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(MAX_METADATA_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 <= MAX_METADATA_BYTES {
        let _result = validate_metadata_bytes(&bytes);
    }
    Ok(())
}
