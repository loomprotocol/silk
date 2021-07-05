use crate::{
    account_info::AccountInfo, entrypoint::ProgramResult, instruction::Instruction,
    program_error::InvokeError,
};

/// Invoke a cross-program instruction
///
/// Note that the program id of the instruction being issued must also be included in
/// `account_infos`.
pub fn invoke(instruction: &Instruction, account_infos: &[AccountInfo]) -> ProgramResult {
    invoke_signed(instruction, account_infos, &[])
}

/// Invoke a cross-program instruction with a caller-specified compute budget
///
/// Note that the program id of the instruction being issued must also be included in
/// `account_infos`.
pub fn invoke_with_budget(
    instruction: &Instruction,
    budget: u64,
    account_infos: &[AccountInfo],
) -> ProgramResult {
    invoke_signed_with_budget(instruction, budget, account_infos, &[])
}

/// Invoke a cross-program instruction with program signatures
///
/// Note that the program id of the instruction being issued must also be included in
/// `account_infos`.
pub fn invoke_signed(
    instruction: &Instruction,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
) -> ProgramResult {
    // Check that the account RefCells are consistent with the request
    for account_meta in instruction.accounts.iter() {
        for account_info in account_infos.iter() {
            if account_meta.pubkey == *account_info.key {
                if account_meta.is_writable {
                    let _ = account_info.try_borrow_mut_lamports()?;
                    let _ = account_info.try_borrow_mut_data()?;
                } else {
                    let _ = account_info.try_borrow_lamports()?;
                    let _ = account_info.try_borrow_data()?;
                }
                break;
            }
        }
    }

    #[cfg(target_arch = "bpf")]
    {
        let result = unsafe {
            sol_invoke_signed_rust(
                instruction as *const _ as *const u8,
                account_infos as *const _ as *const u8,
                account_infos.len() as u64,
                signers_seeds as *const _ as *const u8,
                signers_seeds.len() as u64,
            )
        };
        match result {
            crate::entrypoint::SUCCESS => Ok(()),
            _ => Err(result.into()),
        }
    }

    #[cfg(not(target_arch = "bpf"))]
    crate::program_stubs::sol_invoke_signed(instruction, account_infos, signers_seeds)
}

/// Invoke a cross-program instruction with program signatures and
/// with a caller-specified compute budget
///
/// Note that the program id of the instruction being issued must also be included in
/// `account_infos`.
///

pub fn invoke_signed_with_budget(
    instruction: &Instruction,
    budget: u64,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
) -> ProgramResult {
    invoke_with_options(instruction, budget, account_infos, signers_seeds, true)
}

pub fn invoke_with_rollback(
    instruction: &Instruction,
    _budget: Option<u64>,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
) -> Result<(), InvokeError> {
    match invoke_with_options(
        instruction,
        if let Some(budget) = _budget {
            budget
        } else {
            u64::MAX
        },
        account_infos,
        signers_seeds,
        false,
    ) {
        Err(e) => Err(InvokeError(e)),
        Ok(()) => Ok(()),
    }
}

fn invoke_with_options(
    instruction: &Instruction,
    _budget: u64,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
    _throw_unrecoverable_error: bool,
) -> ProgramResult {
    // Check that the account RefCells are consistent with the request
    for account_meta in instruction.accounts.iter() {
        for account_info in account_infos.iter() {
            if account_meta.pubkey == *account_info.key {
                if account_meta.is_writable {
                    let _ = account_info.try_borrow_mut_lamports()?;
                    let _ = account_info.try_borrow_mut_data()?;
                } else {
                    let _ = account_info.try_borrow_lamports()?;
                    let _ = account_info.try_borrow_data()?;
                }
                break;
            }
        }
    }

    #[cfg(target_arch = "bpf")]
    {
        let instruction_with_options = crate::instruction::InstructionWithOptions {
            instruction_addr: instruction as *const _ as u64,
            budget: _budget,
            throw_unrecoverable_error: _throw_unrecoverable_error,
        };
        let result = unsafe {
            sol_invoke_signed_with_options_rust(
                &instruction_with_options as *const _ as *const u8,
                account_infos as *const _ as *const u8,
                account_infos.len() as u64,
                signers_seeds as *const _ as *const u8,
                signers_seeds.len() as u64,
            )
        };
        match result {
            crate::entrypoint::SUCCESS => Ok(()),
            _ => Err(result.into()),
        }
    }

    #[cfg(not(target_arch = "bpf"))]
    crate::program_stubs::sol_invoke_signed(instruction, account_infos, signers_seeds)
}

#[cfg(target_arch = "bpf")]
extern "C" {
    fn sol_invoke_signed_rust(
        instruction_addr: *const u8,
        account_infos_addr: *const u8,
        account_infos_len: u64,
        signers_seeds_addr: *const u8,
        signers_seeds_len: u64,
    ) -> u64;
}

#[cfg(target_arch = "bpf")]
extern "C" {
    fn sol_invoke_signed_with_options_rust(
        instruction_addr: *const u8,
        account_infos_addr: *const u8,
        account_infos_len: u64,
        signers_seeds_addr: *const u8,
        signers_seeds_len: u64,
    ) -> u64;
}
