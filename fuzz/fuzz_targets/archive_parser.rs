#![no_main]

use libfuzzer_sys::fuzz_target;
use orifude::packs::validate_archive_bytes;

fuzz_target!(|data: &[u8]| {
    let _result = validate_archive_bytes(data);
});
