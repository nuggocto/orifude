use std::io::{self, Read};

use orifude::packs::{MAX_PUZZLE_BYTES, validate_puzzle_bytes};

fn main() -> io::Result<()> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(MAX_PUZZLE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 <= MAX_PUZZLE_BYTES {
        let _result = validate_puzzle_bytes("fuzz-pack", "fuzz-puzzle", &bytes);
    }
    Ok(())
}
