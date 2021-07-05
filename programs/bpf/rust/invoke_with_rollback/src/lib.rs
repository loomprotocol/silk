//! @brief Example Rust-based BPF program that exercises instruction introspection

extern crate solana_program;
use solana_program::{
    account_info::next_account_info,
    account_info::AccountInfo,
    entrypoint,
    entrypoint::ProgramResult,
    instruction::Instruction,
    log::sol_log_compute_units,
    msg,
    program::{invoke_signed_with_budget, invoke_with_rollback},
    program_error::{InvokeError, ProgramError},
    pubkey::Pubkey,
};

entrypoint!(process_instruction);
pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let error_program_info = next_account_info(account_info_iter)?;
    let inner_program_info = next_account_info(account_info_iter)?;
    // 7 * 25_000 = 175_000
    for _ in 0..7 {
        sol_log_compute_units();
        {
            let ix = Instruction::new_with_bincode(*error_program_info.key, &[0], vec![]);
            match invoke_with_rollback(
                &ix,
                Some(12_500),
                &[error_program_info.clone()],
                &vec![][..],
            ) {
                Err(InvokeError(ProgramError::Custom(0x01))) => {
                    msg!("invoke_with_rollback: error 1, rolled back")
                }
                ret => ret?,
            }
        }
        {
            let ix = Instruction::new_with_bincode(*inner_program_info.key, &[0], vec![]);
            match invoke_signed_with_budget(&ix, 25_000, &[inner_program_info.clone()], &vec![][..])
            {
                Err(ProgramError::ComputationalBudgetExceeded) => {
                    msg!("invoke_with_budget: inner CPI exceeded computational budget, rolled back")
                }
                ret => ret?,
            }
        }
    }
    sol_log_compute_units();
    Ok(())
}
