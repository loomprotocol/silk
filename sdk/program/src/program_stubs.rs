//! @brief Syscall stubs when building for programs for non-BPF targets

#![cfg(not(target_arch = "bpf"))]

use crate::{
    account_info::AccountInfo, entrypoint::ProgramResult, instruction::Instruction,
    program_error::UNSUPPORTED_SYSVAR,
};
use std::sync::{Arc, RwLock};

lazy_static::lazy_static! {
    static ref SYSCALL_STUBS: Arc<RwLock<Box<dyn SyscallStubs>>> = Arc::new(RwLock::new(Box::new(DefaultSyscallStubs {})));
}

// The default syscall stubs may not do much, but `set_syscalls()` can be used
// to swap in alternatives
pub fn set_syscall_stubs(syscall_stubs: Box<dyn SyscallStubs>) -> Box<dyn SyscallStubs> {
    std::mem::replace(&mut SYSCALL_STUBS.write().unwrap(), syscall_stubs)
}

#[allow(clippy::integer_arithmetic)]
pub trait SyscallStubs: Sync + Send {
    fn sol_log(&self, message: &str) {
        println!("{}", message);
    }
    fn sol_log_compute_units(&self) {
        sol_log("SyscallStubs: sol_log_compute_units() not available");
    }
    fn sol_remaining_compute_units(&self, _var: &mut u64) {
        sol_log("SyscallStubs: sol_remaining_compute_units() defaulting to u64::MAX");
    }
    fn sol_invoke_signed(
        &self,
        _instruction: &Instruction,
        _account_infos: &[AccountInfo],
        _signers_seeds: &[&[&[u8]]],
    ) -> ProgramResult {
        sol_log("SyscallStubs: sol_invoke_signed() not available");
        Ok(())
    }
    fn sol_get_clock_sysvar(&self, _var_addr: *mut u8) -> u64 {
        UNSUPPORTED_SYSVAR
    }
    fn sol_get_epoch_schedule_sysvar(&self, _var_addr: *mut u8) -> u64 {
        UNSUPPORTED_SYSVAR
    }
    fn sol_get_fees_sysvar(&self, _var_addr: *mut u8) -> u64 {
        UNSUPPORTED_SYSVAR
    }
    fn sol_get_rent_sysvar(&self, _var_addr: *mut u8) -> u64 {
        UNSUPPORTED_SYSVAR
    }
    /// # Safety
    unsafe fn sol_memcpy(&self, dst: *mut u8, src: *const u8, n: usize) {
        // cannot be overlapping
        if dst as usize + n > src as usize && src as usize > dst as usize {
            panic!("memcpy does not support oveerlapping regions");
        }
        std::ptr::copy_nonoverlapping(src, dst, n as usize);
    }
    /// # Safety
    unsafe fn sol_memmove(&self, dst: *mut u8, src: *const u8, n: usize) {
        std::ptr::copy(src, dst, n as usize);
    }
    /// # Safety
    unsafe fn sol_memcmp(&self, s1: *const u8, s2: *const u8, n: usize, result: *mut i32) {
        let mut i = 0;
        while i < n {
            let a = *s1.add(i);
            let b = *s2.add(i);
            if a != b {
                *result = a as i32 - b as i32;
                return;
            }
            i += 1;
        }
        *result = 0
    }
    /// # Safety
    unsafe fn sol_memset(&self, s: *mut u8, c: u8, n: usize) {
        let s = std::slice::from_raw_parts_mut(s, n);
        for val in s.iter_mut().take(n) {
            *val = c;
        }
    }
}

struct DefaultSyscallStubs {}
impl SyscallStubs for DefaultSyscallStubs {}

pub(crate) fn sol_log(message: &str) {
    SYSCALL_STUBS.read().unwrap().sol_log(message);
}

pub(crate) fn sol_log_64(arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) {
    sol_log(&format!(
        "{:#x}, {:#x}, {:#x}, {:#x}, {:#x}",
        arg1, arg2, arg3, arg4, arg5
    ));
}

pub(crate) fn sol_log_compute_units() {
    SYSCALL_STUBS.read().unwrap().sol_log_compute_units();
}

pub(crate) fn sol_remaining_compute_units(var: &mut u64) {
    SYSCALL_STUBS
        .read()
        .unwrap()
        .sol_remaining_compute_units(var);
}

pub(crate) fn sol_invoke_signed(
    instruction: &Instruction,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
) -> ProgramResult {
    SYSCALL_STUBS
        .read()
        .unwrap()
        .sol_invoke_signed(instruction, account_infos, signers_seeds)
}

pub(crate) fn sol_get_clock_sysvar(var_addr: *mut u8) -> u64 {
    SYSCALL_STUBS.read().unwrap().sol_get_clock_sysvar(var_addr)
}

pub(crate) fn sol_get_epoch_schedule_sysvar(var_addr: *mut u8) -> u64 {
    SYSCALL_STUBS
        .read()
        .unwrap()
        .sol_get_epoch_schedule_sysvar(var_addr)
}

pub(crate) fn sol_get_fees_sysvar(var_addr: *mut u8) -> u64 {
    SYSCALL_STUBS.read().unwrap().sol_get_fees_sysvar(var_addr)
}

pub(crate) fn sol_get_rent_sysvar(var_addr: *mut u8) -> u64 {
    SYSCALL_STUBS.read().unwrap().sol_get_rent_sysvar(var_addr)
}

pub(crate) fn sol_memcpy(dst: *mut u8, src: *const u8, n: usize) {
    unsafe {
        SYSCALL_STUBS.read().unwrap().sol_memcpy(dst, src, n);
    }
}

pub(crate) fn sol_memmove(dst: *mut u8, src: *const u8, n: usize) {
    unsafe {
        SYSCALL_STUBS.read().unwrap().sol_memmove(dst, src, n);
    }
}

pub(crate) fn sol_memcmp(s1: *const u8, s2: *const u8, n: usize, result: *mut i32) {
    unsafe {
        SYSCALL_STUBS.read().unwrap().sol_memcmp(s1, s2, n, result);
    }
}

pub(crate) fn sol_memset(s: *mut u8, c: u8, n: usize) {
    unsafe {
        SYSCALL_STUBS.read().unwrap().sol_memset(s, c, n);
    }
}
