use std::hint::black_box;
use std::io::{self, Read};
use std::process::ExitCode;

#[path = "support/domain_actions.rs"]
mod domain_actions;

use domain_actions::{MAX_FUZZ_INPUT_BYTES, exercise};

fn main() -> ExitCode {
    let mut input = Vec::with_capacity(MAX_FUZZ_INPUT_BYTES + 1);
    match io::stdin()
        .take(u64::try_from(MAX_FUZZ_INPUT_BYTES + 1).expect("the fuzz bound must fit in u64"))
        .read_to_end(&mut input)
    {
        Ok(_) if input.len() <= MAX_FUZZ_INPUT_BYTES => {
            exercise(black_box(&input));
            ExitCode::SUCCESS
        }
        Ok(_) => {
            eprintln!("domain action input exceeds {MAX_FUZZ_INPUT_BYTES} bytes");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("failed to read domain action input: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_corpus_preserves_invariants_and_replay_equivalence() {
        let cases: [&[u8]; 6] = [
            &[],
            &[0],
            &[0; MAX_FUZZ_INPUT_BYTES],
            &[u8::MAX; MAX_FUZZ_INPUT_BYTES],
            b"foldinkundoreset",
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        ];
        for input in cases {
            exercise(input);
        }
    }
}
