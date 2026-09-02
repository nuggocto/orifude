use std::io::{self, Read};

use orifude::storage::{MAX_REPLAY_BYTES, decode_replay_bytes};

fn main() -> io::Result<()> {
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_REPLAY_BYTES as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() <= MAX_REPLAY_BYTES {
        let _result = decode_replay_bytes(&bytes);
    }
    Ok(())
}
