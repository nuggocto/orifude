#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../examples/support/domain_actions.rs"]
mod domain_actions;

fuzz_target!(|data: &[u8]| {
    if data.len() <= domain_actions::MAX_FUZZ_INPUT_BYTES {
        domain_actions::exercise(data);
    }
});
