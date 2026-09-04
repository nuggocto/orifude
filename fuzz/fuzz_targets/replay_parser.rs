#![no_main]

use libfuzzer_sys::fuzz_target;
use orifude::storage::decode_replay_bytes;

fuzz_target!(|data: &[u8]| {
    let _result = decode_replay_bytes(data);
});
