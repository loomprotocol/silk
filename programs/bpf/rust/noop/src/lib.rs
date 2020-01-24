//! @brief Example Rust-based BPF program that prints out the parameters passed to it

#![allow(unreachable_code)]

extern crate solana_sdk;
use num_derive::FromPrimitive;

use solana_sdk::{
    account_info::AccountInfo, entrypoint, info, log::*, program_error::ProgramError,
    pubkey::Pubkey,
};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, FromPrimitive)]
pub enum NoopError {
    #[error("Eek")]
    Eek = 42,
}
impl From<NoopError> for ProgramError {
    fn from(e: NoopError) -> Self {
        ProgramError::CustomError(e as u32)
    }
}

#[derive(Debug, PartialEq)]
struct SStruct {
    x: u64,
    y: u64,
    z: u64,
}

#[inline(never)]
fn return_sstruct() -> SStruct {
    SStruct { x: 1, y: 2, z: 3 }
}

entrypoint!(process_instruction);
fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    info!("Program identifier:");
    program_id.log();

    //    if !instruction_data.is_empty() && instruction_data[0] == 0xff {
    //        return Err(NoopError::Eek.into());
    //    }

    // if accounts.is_empty() {
    //     return Err(ProgramError::NotEnoughAccountKeys);
    // }

    // Log the provided account keys and instruction input data.  In the case of
    // the no-op program, no account keys or input data are expected but real
    // programs will have specific requirements so they can do their work.
    info!("Account keys and instruction input data:");
    sol_log_params(accounts, instruction_data);

    {
        // Test - use std methods, unwrap

        // valid bytes, in a stack-allocated array
        let sparkle_heart = [240, 159, 146, 150];
        let result_str = std::str::from_utf8(&sparkle_heart).unwrap();
        assert_eq!(4, result_str.len());
        assert_eq!("💖", result_str);
        info!(result_str);
    }

    {
        // Test - struct return

        let s = return_sstruct();
        assert_eq!(s.x + s.y + s.z, 6);
    }

    {
        // Test - arch config
        #[cfg(not(target_arch = "bpf"))]
        panic!();
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    // Pulls in the stubs requried for `info!()`
    solana_sdk_bpf_test::stubs!();

    #[test]
    fn test_return_sstruct() {
        assert_eq!(SStruct { x: 1, y: 2, z: 3 }, return_sstruct());
    }
}
