#![no_main]

use libfuzzer_sys::fuzz_target;
use orifude::packs::validate_metadata_bytes;

fuzz_target!(|data: &[u8]| {
    let _result = validate_metadata_bytes(data);
});
