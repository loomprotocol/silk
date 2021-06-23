use crate::{alloc, BpfError};
use alloc::Alloc;
use openssl::bn::*;
use solana_rbpf::{
    aligned_memory::AlignedMemory,
    ebpf::MM_HEAP_START,
    error::EbpfError,
    memory_region::{AccessType, MemoryMapping},
    question_mark,
    vm::{EbpfVm, SyscallObject, SyscallRegistry},
};
use solana_runtime::message_processor::MessageProcessor;
use solana_sdk::{
    account::{Account, AccountSharedData, ReadableAccount},
    account_info::AccountInfo,
    account_utils::StateMut,
    bignumber::FfiBigNumber,
    blake3, bpf_loader, bpf_loader_deprecated,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    clock::Clock,
    entrypoint::{MAX_PERMITTED_DATA_INCREASE, SUCCESS},
    epoch_schedule::EpochSchedule,
    feature_set::{
        bignum_syscall_enabled, blake3_syscall_enabled, cpi_data_cost, demote_sysvar_write_locks,
        enforce_aligned_host_addrs, keccak256_syscall_enabled, memory_ops_syscalls,
        sysvar_via_syscall, update_data_on_realloc,
    },
    hash::{Hasher, HASH_BYTES},
    ic_msg,
    instruction::{AccountMeta, Instruction, InstructionError},
    keccak,
    keyed_account::KeyedAccount,
    native_loader,
    process_instruction::{self, stable_log, ComputeMeter, InvokeContext, Logger},
    pubkey::{Pubkey, PubkeyError, MAX_SEEDS},
    rent::Rent,
    sysvar::{self, fees::Fees, Sysvar, SysvarId},
};
use std::{
    alloc::Layout,
    cell::{Ref, RefCell, RefMut},
    mem::{align_of, size_of},
    rc::Rc,
    slice::from_raw_parts_mut,
    str::{from_utf8, Utf8Error},
};
use thiserror::Error as ThisError;

/// Maximum signers
pub const MAX_SIGNERS: usize = 16;

/// Error definitions
#[derive(Debug, ThisError, PartialEq)]
pub enum SyscallError {
    #[error("{0}: {1:?}")]
    InvalidString(Utf8Error, Vec<u8>),
    #[error("BPF program panicked")]
    Abort,
    #[error("BPF program Panicked in {0} at {1}:{2}")]
    Panic(String, u64, u64),
    #[error("cannot borrow invoke context")]
    InvokeContextBorrowFailed,
    #[error("malformed signer seed: {0}: {1:?}")]
    MalformedSignerSeed(Utf8Error, Vec<u8>),
    #[error("Could not create program address with signer seeds: {0}")]
    BadSeeds(PubkeyError),
    #[error("Program {0} not supported by inner instructions")]
    ProgramNotSupported(Pubkey),
    #[error("{0}")]
    InstructionError(InstructionError),
    #[error("Unaligned pointer")]
    UnalignedPointer,
    #[error("Too many signers")]
    TooManySigners,
    #[error("Instruction passed to inner instruction is too large ({0} > {1})")]
    InstructionTooLarge(usize, usize),
    #[error("Too many accounts passed to inner instruction")]
    TooManyAccounts,
    #[error("Overlapping copy")]
    CopyOverlapping,
    #[error("BigNumber: Modular exponentiation error {0}")]
    BigNumberModExpError(String),
    #[error("BigNumber: Bytes would exceed buffer provided")]
    BigNumberToBytesError,
    #[error("BigNumber: expects {0} args but provided {1}")]
    BigNumberArgError(u64, u64),
    #[error("BigNumber: add error {0}")]
    BigNumberAddError(String),
    #[error("BigNumber: sub error {0}")]
    BigNumberSubError(String),
    #[error("BigNumber: mul error {0}")]
    BigNumberMulError(String),
    #[error("BigNumber: div error {0}")]
    BigNumberDivError(String),
    #[error("BigNumber: exp error {0}")]
    BigNumberExpError(String),
    #[error("BigNumber: sqr error {0}")]
    BigNumberSqrError(String),
    #[error("BigNumber: mod_sqr error {0}")]
    BigNumberModSqrError(String),
    #[error("BigNumber: mod_mul error {0}")]
    BigNumberModMulError(String),
    #[error("BigNumber: mod_inv error {0}")]
    BigNumberModInvError(String),
    #[error("BigNumber: error in executing operation {0}")]
    BigNumberOperationError(String),
    #[error("BigNumber: unknow error encountered")]
    BigNumberErrorUnknown,
    #[error("BigNumber: error in converting decimal string to BigNum")]
    BigNumberFromDecStrError,
    #[error("BigNumber: size > MAX_BIGNUM_BYTE_SIZE bytes (1 k) error")]
    BigNumberSizeError,
    #[error("BigNumber: from_slice error {0}")]
    BigNumberFromSliceError(String),
}
impl From<SyscallError> for EbpfError<BpfError> {
    fn from(error: SyscallError) -> Self {
        EbpfError::UserError(error.into())
    }
}

trait SyscallConsume {
    fn consume(&mut self, amount: u64) -> Result<(), EbpfError<BpfError>>;
}
impl SyscallConsume for Rc<RefCell<dyn ComputeMeter>> {
    fn consume(&mut self, amount: u64) -> Result<(), EbpfError<BpfError>> {
        self.try_borrow_mut()
            .map_err(|_| SyscallError::InvokeContextBorrowFailed)?
            .consume(amount)
            .map_err(SyscallError::InstructionError)?;
        Ok(())
    }
}

/// Program heap allocators are intended to allocate/free from a given
/// chunk of memory.  The specific allocator implementation is
/// selectable at build-time.
/// Only one allocator is currently supported

/// Simple bump allocator, never frees
use crate::allocator_bump::BpfAllocator;

pub fn register_syscalls(
    invoke_context: &mut dyn InvokeContext,
) -> Result<SyscallRegistry, EbpfError<BpfError>> {
    let mut syscall_registry = SyscallRegistry::default();

    syscall_registry.register_syscall_by_name(b"abort", SyscallAbort::call)?;
    syscall_registry.register_syscall_by_name(b"sol_panic_", SyscallPanic::call)?;
    syscall_registry.register_syscall_by_name(b"sol_log_", SyscallLog::call)?;
    syscall_registry.register_syscall_by_name(b"sol_log_64_", SyscallLogU64::call)?;

    syscall_registry
        .register_syscall_by_name(b"sol_log_compute_units_", SyscallLogBpfComputeUnits::call)?;

    syscall_registry.register_syscall_by_name(b"sol_log_pubkey", SyscallLogPubkey::call)?;

    syscall_registry.register_syscall_by_name(
        b"sol_create_program_address",
        SyscallCreateProgramAddress::call,
    )?;
    syscall_registry.register_syscall_by_name(
        b"sol_try_find_program_address",
        SyscallTryFindProgramAddress::call,
    )?;

    syscall_registry.register_syscall_by_name(b"sol_sha256", SyscallSha256::call)?;

    if invoke_context.is_feature_active(&keccak256_syscall_enabled::id()) {
        syscall_registry.register_syscall_by_name(b"sol_keccak256", SyscallKeccak256::call)?;
    }

    if invoke_context.is_feature_active(&blake3_syscall_enabled::id()) {
        syscall_registry.register_syscall_by_name(b"sol_blake3", SyscallBlake3::call)?;
    }

    if invoke_context.is_feature_active(&sysvar_via_syscall::id()) {
        syscall_registry
            .register_syscall_by_name(b"sol_get_clock_sysvar", SyscallGetClockSysvar::call)?;
        syscall_registry.register_syscall_by_name(
            b"sol_get_epoch_schedule_sysvar",
            SyscallGetEpochScheduleSysvar::call,
        )?;
        syscall_registry
            .register_syscall_by_name(b"sol_get_fees_sysvar", SyscallGetFeesSysvar::call)?;
        syscall_registry
            .register_syscall_by_name(b"sol_get_rent_sysvar", SyscallGetRentSysvar::call)?;
    }

    if invoke_context.is_feature_active(&memory_ops_syscalls::id()) {
        syscall_registry.register_syscall_by_name(b"sol_memcpy_", SyscallMemcpy::call)?;
        syscall_registry.register_syscall_by_name(b"sol_memmove_", SyscallMemmove::call)?;
        syscall_registry.register_syscall_by_name(b"sol_memcmp_", SyscallMemcmp::call)?;
        syscall_registry.register_syscall_by_name(b"sol_memset_", SyscallMemset::call)?;
    }

    // Cross-program invocation syscalls
    syscall_registry
        .register_syscall_by_name(b"sol_invoke_signed_c", SyscallInvokeSignedC::call)?;
    syscall_registry
        .register_syscall_by_name(b"sol_invoke_signed_rust", SyscallInvokeSignedRust::call)?;

    // Memory allocator
    syscall_registry.register_syscall_by_name(b"sol_alloc_free_", SyscallAllocFree::call)?;

    if invoke_context.is_feature_active(&bignum_syscall_enabled::id()) {
        syscall_registry
            .register_syscall_by_name(b"sol_bignum_from_u32_", SyscallBigNumFromU32::call)?;
        syscall_registry
            .register_syscall_by_name(b"sol_bignum_from_dec_str_", SyscallBigNumFromDecStr::call)?;
        syscall_registry
            .register_syscall_by_name(b"sol_bignum_mod_exp_", SyscallBigNumModExp::call)?;
        syscall_registry.register_syscall_by_name(b"sol_bignum_log_", SyscallBigNumLog::call)?;
        syscall_registry.register_syscall_by_name(b"sol_bignum_add_", SyscallBigNumAdd::call)?;
        syscall_registry.register_syscall_by_name(b"sol_bignum_sub_", SyscallBigNumSub::call)?;
        syscall_registry.register_syscall_by_name(b"sol_bignum_mul_", SyscallBigNumMul::call)?;
        syscall_registry.register_syscall_by_name(b"sol_bignum_div_", SyscallBigNumDiv::call)?;
        syscall_registry.register_syscall_by_name(b"sol_bignum_exp_", SyscallBigNumExp::call)?;
        syscall_registry.register_syscall_by_name(b"sol_bignum_sqr_", SyscallBigNumSqr::call)?;
        syscall_registry
            .register_syscall_by_name(b"sol_bignum_mod_sqr_", SyscallBigNumModSqr::call)?;
        syscall_registry
            .register_syscall_by_name(b"sol_bignum_mod_mul_", SyscallBigNumModMul::call)?;
        syscall_registry
            .register_syscall_by_name(b"sol_bignum_mod_inv_", SyscallBigNumModInv::call)?;
        syscall_registry
            .register_syscall_by_name(b"sol_bignum_from_bytes_", SyscallBigNumFromBytes::call)?;
    }

    Ok(syscall_registry)
}

macro_rules! bind_feature_gated_syscall_context_object {
    ($vm:expr, $is_feature_active:expr, $syscall_context_object:expr $(,)?) => {
        if $is_feature_active {
            match $vm.bind_syscall_context_object($syscall_context_object, None) {
                Err(EbpfError::SyscallNotRegistered(_)) | Ok(()) => {}
                Err(err) => {
                    return Err(err);
                }
            }
        }
    };
}

pub fn bind_syscall_context_objects<'a>(
    loader_id: &'a Pubkey,
    vm: &mut EbpfVm<'a, BpfError, crate::ThisInstructionMeter>,
    invoke_context: &'a mut dyn InvokeContext,
    heap: AlignedMemory,
) -> Result<(), EbpfError<BpfError>> {
    let bpf_compute_budget = invoke_context.get_bpf_compute_budget();
    let enforce_aligned_host_addrs =
        invoke_context.is_feature_active(&enforce_aligned_host_addrs::id());

    // Syscall functions common across languages

    vm.bind_syscall_context_object(Box::new(SyscallAbort {}), None)?;
    vm.bind_syscall_context_object(
        Box::new(SyscallPanic {
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
            enforce_aligned_host_addrs,
        }),
        None,
    )?;
    vm.bind_syscall_context_object(
        Box::new(SyscallLog {
            compute_meter: invoke_context.get_compute_meter(),
            logger: invoke_context.get_logger(),
            loader_id,
            enforce_aligned_host_addrs,
        }),
        None,
    )?;
    vm.bind_syscall_context_object(
        Box::new(SyscallLogU64 {
            cost: bpf_compute_budget.log_64_units,
            compute_meter: invoke_context.get_compute_meter(),
            logger: invoke_context.get_logger(),
        }),
        None,
    )?;

    vm.bind_syscall_context_object(
        Box::new(SyscallLogBpfComputeUnits {
            cost: 0,
            compute_meter: invoke_context.get_compute_meter(),
            logger: invoke_context.get_logger(),
        }),
        None,
    )?;

    vm.bind_syscall_context_object(
        Box::new(SyscallLogPubkey {
            cost: bpf_compute_budget.log_pubkey_units,
            compute_meter: invoke_context.get_compute_meter(),
            logger: invoke_context.get_logger(),
            loader_id,
            enforce_aligned_host_addrs,
        }),
        None,
    )?;

    vm.bind_syscall_context_object(
        Box::new(SyscallCreateProgramAddress {
            cost: bpf_compute_budget.create_program_address_units,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
            enforce_aligned_host_addrs,
        }),
        None,
    )?;

    vm.bind_syscall_context_object(
        Box::new(SyscallTryFindProgramAddress {
            cost: bpf_compute_budget.create_program_address_units,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
            enforce_aligned_host_addrs,
        }),
        None,
    )?;

    vm.bind_syscall_context_object(
        Box::new(SyscallSha256 {
            sha256_base_cost: bpf_compute_budget.sha256_base_cost,
            sha256_byte_cost: bpf_compute_budget.sha256_byte_cost,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
            enforce_aligned_host_addrs,
        }),
        None,
    )?;

    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&keccak256_syscall_enabled::id()),
        Box::new(SyscallKeccak256 {
            base_cost: bpf_compute_budget.sha256_base_cost,
            byte_cost: bpf_compute_budget.sha256_byte_cost,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );

    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&memory_ops_syscalls::id()),
        Box::new(SyscallMemcpy {
            cost: invoke_context.get_bpf_compute_budget().cpi_bytes_per_unit,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&memory_ops_syscalls::id()),
        Box::new(SyscallMemmove {
            cost: invoke_context.get_bpf_compute_budget().cpi_bytes_per_unit,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&memory_ops_syscalls::id()),
        Box::new(SyscallMemcmp {
            cost: invoke_context.get_bpf_compute_budget().cpi_bytes_per_unit,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&memory_ops_syscalls::id()),
        Box::new(SyscallMemset {
            cost: invoke_context.get_bpf_compute_budget().cpi_bytes_per_unit,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&blake3_syscall_enabled::id()),
        Box::new(SyscallBlake3 {
            base_cost: bpf_compute_budget.sha256_base_cost,
            byte_cost: bpf_compute_budget.sha256_byte_cost,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );

    // Bignum
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumFromU32 {
            cost: bpf_compute_budget.bignum_from_u32_base_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );

    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumFromDecStr {
            cost: bpf_compute_budget.bignum_from_dec_str_base_cost,
            word_cost: bpf_compute_budget.bignum_from_dec_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );

    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumAdd {
            cost: bpf_compute_budget.bignum_add_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumSub {
            cost: bpf_compute_budget.bignum_sub_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumMul {
            cost: bpf_compute_budget.bignum_mul_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumDiv {
            cost: bpf_compute_budget.bignum_div_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumExp {
            cost: bpf_compute_budget.bignum_exp_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumSqr {
            cost: bpf_compute_budget.bignum_sqr_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumModSqr {
            cost: bpf_compute_budget.bignum_mod_sqr_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumModMul {
            cost: bpf_compute_budget.bignum_mod_mul_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumModInv {
            cost: bpf_compute_budget.bignum_mod_inv_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumModExp {
            cost: bpf_compute_budget.bignum_mod_exp_base_cost,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumLog {
            cost: bpf_compute_budget.bignum_log_cost,
            word_cost: bpf_compute_budget.bignum_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            logger: invoke_context.get_logger(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        invoke_context.is_feature_active(&bignum_syscall_enabled::id()),
        Box::new(SyscallBigNumFromBytes {
            cost: bpf_compute_budget.bignum_from_u32_base_cost,
            word_cost: bpf_compute_budget.bignum_from_bytes_word_cost,
            word_div_cost: bpf_compute_budget.bignum_word_cost_divisor,
            compute_meter: invoke_context.get_compute_meter(),
            loader_id,
        }),
    );

    let is_sysvar_via_syscall_active = invoke_context.is_feature_active(&sysvar_via_syscall::id());

    let invoke_context = Rc::new(RefCell::new(invoke_context));

    bind_feature_gated_syscall_context_object!(
        vm,
        is_sysvar_via_syscall_active,
        Box::new(SyscallGetClockSysvar {
            invoke_context: invoke_context.clone(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        is_sysvar_via_syscall_active,
        Box::new(SyscallGetEpochScheduleSysvar {
            invoke_context: invoke_context.clone(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        is_sysvar_via_syscall_active,
        Box::new(SyscallGetFeesSysvar {
            invoke_context: invoke_context.clone(),
            loader_id,
        }),
    );
    bind_feature_gated_syscall_context_object!(
        vm,
        is_sysvar_via_syscall_active,
        Box::new(SyscallGetRentSysvar {
            invoke_context: invoke_context.clone(),
            loader_id,
        }),
    );

    // Cross-program invocation syscalls
    vm.bind_syscall_context_object(
        Box::new(SyscallInvokeSignedC {
            invoke_context: invoke_context.clone(),
            loader_id,
        }),
        None,
    )?;
    vm.bind_syscall_context_object(
        Box::new(SyscallInvokeSignedRust {
            invoke_context: invoke_context.clone(),
            loader_id,
        }),
        None,
    )?;

    // Memory allocator
    vm.bind_syscall_context_object(
        Box::new(SyscallAllocFree {
            aligned: *loader_id != bpf_loader_deprecated::id(),
            allocator: BpfAllocator::new(heap, MM_HEAP_START),
        }),
        None,
    )?;

    Ok(())
}

fn translate(
    memory_mapping: &MemoryMapping,
    access_type: AccessType,
    vm_addr: u64,
    len: u64,
) -> Result<u64, EbpfError<BpfError>> {
    memory_mapping.map::<BpfError>(access_type, vm_addr, len)
}

fn translate_type_inner<'a, T>(
    memory_mapping: &MemoryMapping,
    access_type: AccessType,
    vm_addr: u64,
    loader_id: &Pubkey,
    enforce_aligned_host_addrs: bool,
) -> Result<&'a mut T, EbpfError<BpfError>> {
    if !enforce_aligned_host_addrs
        && loader_id != &bpf_loader_deprecated::id()
        && (vm_addr as *mut T).align_offset(align_of::<T>()) != 0
    {
        return Err(SyscallError::UnalignedPointer.into());
    }

    let host_addr = translate(memory_mapping, access_type, vm_addr, size_of::<T>() as u64)?;

    if enforce_aligned_host_addrs
        && loader_id != &bpf_loader_deprecated::id()
        && (host_addr as *mut T).align_offset(align_of::<T>()) != 0
    {
        return Err(SyscallError::UnalignedPointer.into());
    }
    Ok(unsafe { &mut *(host_addr as *mut T) })
}
fn translate_type_mut<'a, T>(
    memory_mapping: &MemoryMapping,
    vm_addr: u64,
    loader_id: &Pubkey,
    enforce_aligned_host_addrs: bool,
) -> Result<&'a mut T, EbpfError<BpfError>> {
    translate_type_inner::<T>(
        memory_mapping,
        AccessType::Store,
        vm_addr,
        loader_id,
        enforce_aligned_host_addrs,
    )
}
fn translate_type<'a, T>(
    memory_mapping: &MemoryMapping,
    vm_addr: u64,
    loader_id: &Pubkey,
    enforce_aligned_host_addrs: bool,
) -> Result<&'a T, EbpfError<BpfError>> {
    translate_type_inner::<T>(
        memory_mapping,
        AccessType::Load,
        vm_addr,
        loader_id,
        enforce_aligned_host_addrs,
    )
    .map(|value| &*value)
}

fn translate_slice_inner<'a, T>(
    memory_mapping: &MemoryMapping,
    access_type: AccessType,
    vm_addr: u64,
    len: u64,
    loader_id: &Pubkey,
    enforce_aligned_host_addrs: bool,
) -> Result<&'a mut [T], EbpfError<BpfError>> {
    if !enforce_aligned_host_addrs
        && loader_id != &bpf_loader_deprecated::id()
        && (vm_addr as u64 as *mut T).align_offset(align_of::<T>()) != 0
    {
        return Err(SyscallError::UnalignedPointer.into());
    }
    if len == 0 {
        return Ok(&mut []);
    }

    let host_addr = translate(
        memory_mapping,
        access_type,
        vm_addr,
        len.saturating_mul(size_of::<T>() as u64),
    )?;

    if enforce_aligned_host_addrs
        && loader_id != &bpf_loader_deprecated::id()
        && (host_addr as *mut T).align_offset(align_of::<T>()) != 0
    {
        return Err(SyscallError::UnalignedPointer.into());
    }
    Ok(unsafe { from_raw_parts_mut(host_addr as *mut T, len as usize) })
}
fn translate_slice_mut<'a, T>(
    memory_mapping: &MemoryMapping,
    vm_addr: u64,
    len: u64,
    loader_id: &Pubkey,
    enforce_aligned_host_addrs: bool,
) -> Result<&'a mut [T], EbpfError<BpfError>> {
    translate_slice_inner::<T>(
        memory_mapping,
        AccessType::Store,
        vm_addr,
        len,
        loader_id,
        enforce_aligned_host_addrs,
    )
}
fn translate_slice<'a, T>(
    memory_mapping: &MemoryMapping,
    vm_addr: u64,
    len: u64,
    loader_id: &Pubkey,
    enforce_aligned_host_addrs: bool,
) -> Result<&'a [T], EbpfError<BpfError>> {
    translate_slice_inner::<T>(
        memory_mapping,
        AccessType::Load,
        vm_addr,
        len,
        loader_id,
        enforce_aligned_host_addrs,
    )
    .map(|value| &*value)
}

/// Take a virtual pointer to a string (points to BPF VM memory space), translate it
/// pass it to a user-defined work function
fn translate_string_and_do(
    memory_mapping: &MemoryMapping,
    addr: u64,
    len: u64,
    loader_id: &Pubkey,
    enforce_aligned_host_addrs: bool,
    work: &mut dyn FnMut(&str) -> Result<u64, EbpfError<BpfError>>,
) -> Result<u64, EbpfError<BpfError>> {
    let buf = translate_slice::<u8>(
        memory_mapping,
        addr,
        len,
        loader_id,
        enforce_aligned_host_addrs,
    )?;
    let i = match buf.iter().position(|byte| *byte == 0) {
        Some(i) => i,
        None => len as usize,
    };
    match from_utf8(&buf[..i]) {
        Ok(message) => work(message),
        Err(err) => Err(SyscallError::InvalidString(err, buf[..i].to_vec()).into()),
    }
}

/// Abort syscall functions, called when the BPF program calls `abort()`
/// LLVM will insert calls to `abort()` if it detects an untenable situation,
/// `abort()` is not intended to be called explicitly by the program.
/// Causes the BPF program to be halted immediately
pub struct SyscallAbort {}
impl SyscallObject<BpfError> for SyscallAbort {
    fn call(
        &mut self,
        _arg1: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        _memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        *result = Err(SyscallError::Abort.into());
    }
}

/// Panic syscall function, called when the BPF program calls 'sol_panic_()`
/// Causes the BPF program to be halted immediately
/// Log a user's info message
pub struct SyscallPanic<'a> {
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
    enforce_aligned_host_addrs: bool,
}
impl<'a> SyscallObject<BpfError> for SyscallPanic<'a> {
    fn call(
        &mut self,
        file: u64,
        len: u64,
        line: u64,
        column: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(len), result);
        *result = translate_string_and_do(
            memory_mapping,
            file,
            len,
            self.loader_id,
            self.enforce_aligned_host_addrs,
            &mut |string: &str| Err(SyscallError::Panic(string.to_string(), line, column).into()),
        );
    }
}

/// Log a user's info message
pub struct SyscallLog<'a> {
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    logger: Rc<RefCell<dyn Logger>>,
    loader_id: &'a Pubkey,
    enforce_aligned_host_addrs: bool,
}
impl<'a> SyscallObject<BpfError> for SyscallLog<'a> {
    fn call(
        &mut self,
        addr: u64,
        len: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(len), result);
        question_mark!(
            translate_string_and_do(
                memory_mapping,
                addr,
                len,
                self.loader_id,
                self.enforce_aligned_host_addrs,
                &mut |string: &str| {
                    stable_log::program_log(&self.logger, string);
                    Ok(0)
                },
            ),
            result
        );
        *result = Ok(0);
    }
}

/// Log 5 64-bit values
pub struct SyscallLogU64 {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    logger: Rc<RefCell<dyn Logger>>,
}
impl SyscallObject<BpfError> for SyscallLogU64 {
    fn call(
        &mut self,
        arg1: u64,
        arg2: u64,
        arg3: u64,
        arg4: u64,
        arg5: u64,
        _memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(self.cost), result);
        stable_log::program_log(
            &self.logger,
            &format!(
                "{:#x}, {:#x}, {:#x}, {:#x}, {:#x}",
                arg1, arg2, arg3, arg4, arg5
            ),
        );
        *result = Ok(0);
    }
}

/// Log current compute consumption
pub struct SyscallLogBpfComputeUnits {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    logger: Rc<RefCell<dyn Logger>>,
}
impl SyscallObject<BpfError> for SyscallLogBpfComputeUnits {
    fn call(
        &mut self,
        _arg1: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        _memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(self.cost), result);
        let logger = question_mark!(
            self.logger
                .try_borrow_mut()
                .map_err(|_| SyscallError::InvokeContextBorrowFailed),
            result
        );
        if logger.log_enabled() {
            logger.log(&format!(
                "Program consumption: {} units remaining",
                self.compute_meter.borrow().get_remaining()
            ));
        }
        *result = Ok(0);
    }
}

/// Log 5 64-bit values
pub struct SyscallLogPubkey<'a> {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    logger: Rc<RefCell<dyn Logger>>,
    loader_id: &'a Pubkey,
    enforce_aligned_host_addrs: bool,
}
impl<'a> SyscallObject<BpfError> for SyscallLogPubkey<'a> {
    fn call(
        &mut self,
        pubkey_addr: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(self.cost), result);
        let pubkey = question_mark!(
            translate_type::<Pubkey>(
                memory_mapping,
                pubkey_addr,
                self.loader_id,
                self.enforce_aligned_host_addrs,
            ),
            result
        );
        stable_log::program_log(&self.logger, &pubkey.to_string());
        *result = Ok(0);
    }
}

/// Dynamic memory allocation syscall called when the BPF program calls
/// `sol_alloc_free_()`.  The allocator is expected to allocate/free
/// from/to a given chunk of memory and enforce size restrictions.  The
/// memory chunk is given to the allocator during allocator creation and
/// information about that memory (start address and size) is passed
/// to the VM to use for enforcement.
pub struct SyscallAllocFree {
    aligned: bool,
    allocator: BpfAllocator,
}
impl SyscallObject<BpfError> for SyscallAllocFree {
    fn call(
        &mut self,
        size: u64,
        free_addr: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        _memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        let align = if self.aligned {
            align_of::<u128>()
        } else {
            align_of::<u8>()
        };
        let layout = match Layout::from_size_align(size as usize, align) {
            Ok(layout) => layout,
            Err(_) => {
                *result = Ok(0);
                return;
            }
        };
        *result = if free_addr == 0 {
            match self.allocator.alloc(layout) {
                Ok(addr) => Ok(addr as u64),
                Err(_) => Ok(0),
            }
        } else {
            self.allocator.dealloc(free_addr, layout);
            Ok(0)
        };
    }
}

fn translate_program_address_inputs<'a>(
    seeds_addr: u64,
    seeds_len: u64,
    program_id_addr: u64,
    memory_mapping: &MemoryMapping,
    loader_id: &Pubkey,
    enforce_aligned_host_addrs: bool,
) -> Result<(Vec<&'a [u8]>, &'a Pubkey), EbpfError<BpfError>> {
    let untranslated_seeds = translate_slice::<&[&u8]>(
        memory_mapping,
        seeds_addr,
        seeds_len,
        loader_id,
        enforce_aligned_host_addrs,
    )?;
    if untranslated_seeds.len() > MAX_SEEDS {
        return Err(SyscallError::BadSeeds(PubkeyError::MaxSeedLengthExceeded).into());
    }
    let seeds = untranslated_seeds
        .iter()
        .map(|untranslated_seed| {
            translate_slice::<u8>(
                memory_mapping,
                untranslated_seed.as_ptr() as *const _ as u64,
                untranslated_seed.len() as u64,
                loader_id,
                enforce_aligned_host_addrs,
            )
        })
        .collect::<Result<Vec<_>, EbpfError<BpfError>>>()?;
    let program_id = translate_type::<Pubkey>(
        memory_mapping,
        program_id_addr,
        loader_id,
        enforce_aligned_host_addrs,
    )?;
    Ok((seeds, program_id))
}

/// Create a program address
struct SyscallCreateProgramAddress<'a> {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
    enforce_aligned_host_addrs: bool,
}
impl<'a> SyscallObject<BpfError> for SyscallCreateProgramAddress<'a> {
    fn call(
        &mut self,
        seeds_addr: u64,
        seeds_len: u64,
        program_id_addr: u64,
        address_addr: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        let (seeds, program_id) = question_mark!(
            translate_program_address_inputs(
                seeds_addr,
                seeds_len,
                program_id_addr,
                memory_mapping,
                self.loader_id,
                self.enforce_aligned_host_addrs,
            ),
            result
        );

        question_mark!(self.compute_meter.consume(self.cost), result);
        let new_address = match Pubkey::create_program_address(&seeds, program_id) {
            Ok(address) => address,
            Err(_) => {
                *result = Ok(1);
                return;
            }
        };
        let address = question_mark!(
            translate_slice_mut::<u8>(
                memory_mapping,
                address_addr,
                32,
                self.loader_id,
                self.enforce_aligned_host_addrs,
            ),
            result
        );
        address.copy_from_slice(new_address.as_ref());
        *result = Ok(0);
    }
}

/// Create a program address
struct SyscallTryFindProgramAddress<'a> {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
    enforce_aligned_host_addrs: bool,
}
impl<'a> SyscallObject<BpfError> for SyscallTryFindProgramAddress<'a> {
    fn call(
        &mut self,
        seeds_addr: u64,
        seeds_len: u64,
        program_id_addr: u64,
        address_addr: u64,
        bump_seed_addr: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        let (seeds, program_id) = question_mark!(
            translate_program_address_inputs(
                seeds_addr,
                seeds_len,
                program_id_addr,
                memory_mapping,
                self.loader_id,
                self.enforce_aligned_host_addrs,
            ),
            result
        );

        let mut bump_seed = [std::u8::MAX];
        for _ in 0..std::u8::MAX {
            {
                let mut seeds_with_bump = seeds.to_vec();
                seeds_with_bump.push(&bump_seed);

                question_mark!(self.compute_meter.consume(self.cost), result);
                if let Ok(new_address) =
                    Pubkey::create_program_address(&seeds_with_bump, program_id)
                {
                    let bump_seed_ref = question_mark!(
                        translate_type_mut::<u8>(
                            memory_mapping,
                            bump_seed_addr,
                            self.loader_id,
                            self.enforce_aligned_host_addrs,
                        ),
                        result
                    );
                    let address = question_mark!(
                        translate_slice_mut::<u8>(
                            memory_mapping,
                            address_addr,
                            32,
                            self.loader_id,
                            self.enforce_aligned_host_addrs,
                        ),
                        result
                    );
                    *bump_seed_ref = bump_seed[0];
                    address.copy_from_slice(new_address.as_ref());
                    *result = Ok(0);
                    return;
                }
            }
            bump_seed[0] -= 1;
        }
        *result = Ok(1);
    }
}

/// SHA256
pub struct SyscallSha256<'a> {
    sha256_base_cost: u64,
    sha256_byte_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
    enforce_aligned_host_addrs: bool,
}
impl<'a> SyscallObject<BpfError> for SyscallSha256<'a> {
    fn call(
        &mut self,
        vals_addr: u64,
        vals_len: u64,
        result_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(self.sha256_base_cost), result);
        let hash_result = question_mark!(
            translate_slice_mut::<u8>(
                memory_mapping,
                result_addr,
                HASH_BYTES as u64,
                self.loader_id,
                self.enforce_aligned_host_addrs,
            ),
            result
        );
        let mut hasher = Hasher::default();
        if vals_len > 0 {
            let vals = question_mark!(
                translate_slice::<&[u8]>(
                    memory_mapping,
                    vals_addr,
                    vals_len,
                    self.loader_id,
                    self.enforce_aligned_host_addrs,
                ),
                result
            );
            for val in vals.iter() {
                let bytes = question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        val.as_ptr() as u64,
                        val.len() as u64,
                        self.loader_id,
                        self.enforce_aligned_host_addrs,
                    ),
                    result
                );
                question_mark!(
                    self.compute_meter
                        .consume(self.sha256_byte_cost * (val.len() as u64 / 2)),
                    result
                );
                hasher.hash(bytes);
            }
        }
        hash_result.copy_from_slice(&hasher.result().to_bytes());
        *result = Ok(0);
    }
}

fn get_sysvar<T: std::fmt::Debug + Sysvar + SysvarId>(
    id: &Pubkey,
    var_addr: u64,
    loader_id: &Pubkey,
    memory_mapping: &MemoryMapping,
    invoke_context: Rc<RefCell<&mut dyn InvokeContext>>,
) -> Result<u64, EbpfError<BpfError>> {
    let invoke_context = invoke_context
        .try_borrow()
        .map_err(|_| SyscallError::InvokeContextBorrowFailed)?;

    invoke_context.get_compute_meter().consume(
        invoke_context.get_bpf_compute_budget().sysvar_base_cost + size_of::<T>() as u64,
    )?;
    let var = translate_type_mut::<T>(
        memory_mapping,
        var_addr,
        loader_id,
        invoke_context.is_feature_active(&enforce_aligned_host_addrs::id()),
    )?;

    *var = process_instruction::get_sysvar::<T>(*invoke_context, id)
        .map_err(SyscallError::InstructionError)?;

    Ok(SUCCESS)
}

/// Get a Clock sysvar
struct SyscallGetClockSysvar<'a> {
    invoke_context: Rc<RefCell<&'a mut dyn InvokeContext>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallGetClockSysvar<'a> {
    fn call(
        &mut self,
        var_addr: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        *result = get_sysvar::<Clock>(
            &sysvar::clock::id(),
            var_addr,
            self.loader_id,
            memory_mapping,
            self.invoke_context.clone(),
        );
    }
}
/// Get a EpochSchedule sysvar
struct SyscallGetEpochScheduleSysvar<'a> {
    invoke_context: Rc<RefCell<&'a mut dyn InvokeContext>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallGetEpochScheduleSysvar<'a> {
    fn call(
        &mut self,
        var_addr: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        *result = get_sysvar::<EpochSchedule>(
            &sysvar::epoch_schedule::id(),
            var_addr,
            self.loader_id,
            memory_mapping,
            self.invoke_context.clone(),
        );
    }
}
/// Get a Fees sysvar
struct SyscallGetFeesSysvar<'a> {
    invoke_context: Rc<RefCell<&'a mut dyn InvokeContext>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallGetFeesSysvar<'a> {
    fn call(
        &mut self,
        var_addr: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        *result = get_sysvar::<Fees>(
            &sysvar::fees::id(),
            var_addr,
            self.loader_id,
            memory_mapping,
            self.invoke_context.clone(),
        );
    }
}
/// Get a Rent sysvar
struct SyscallGetRentSysvar<'a> {
    invoke_context: Rc<RefCell<&'a mut dyn InvokeContext>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallGetRentSysvar<'a> {
    fn call(
        &mut self,
        var_addr: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        *result = get_sysvar::<Rent>(
            &sysvar::rent::id(),
            var_addr,
            self.loader_id,
            memory_mapping,
            self.invoke_context.clone(),
        );
    }
}

// Keccak256
pub struct SyscallKeccak256<'a> {
    base_cost: u64,
    byte_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallKeccak256<'a> {
    fn call(
        &mut self,
        vals_addr: u64,
        vals_len: u64,
        result_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(self.base_cost), result);
        let hash_result = question_mark!(
            translate_slice_mut::<u8>(
                memory_mapping,
                result_addr,
                keccak::HASH_BYTES as u64,
                self.loader_id,
                true,
            ),
            result
        );
        let mut hasher = keccak::Hasher::default();
        if vals_len > 0 {
            let vals = question_mark!(
                translate_slice::<&[u8]>(memory_mapping, vals_addr, vals_len, self.loader_id, true),
                result
            );
            for val in vals.iter() {
                let bytes = question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        val.as_ptr() as u64,
                        val.len() as u64,
                        self.loader_id,
                        true,
                    ),
                    result
                );
                question_mark!(
                    self.compute_meter
                        .consume(self.byte_cost * (val.len() as u64 / 2)),
                    result
                );
                hasher.hash(bytes);
            }
        }
        hash_result.copy_from_slice(&hasher.result().to_bytes());
        *result = Ok(0);
    }
}

/// memcpy
pub struct SyscallMemcpy<'a> {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallMemcpy<'a> {
    fn call(
        &mut self,
        dst_addr: u64,
        src_addr: u64,
        n: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // cannot be overlapping
        if dst_addr + n > src_addr && src_addr > dst_addr {
            *result = Err(SyscallError::CopyOverlapping.into());
            return;
        }

        question_mark!(self.compute_meter.consume(n / self.cost), result);
        let dst = question_mark!(
            translate_slice_mut::<u8>(memory_mapping, dst_addr, n, self.loader_id, true),
            result
        );
        let src = question_mark!(
            translate_slice::<u8>(memory_mapping, src_addr, n, self.loader_id, true),
            result
        );
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), n as usize);
        }
        *result = Ok(0);
    }
}
/// memcpy
pub struct SyscallMemmove<'a> {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallMemmove<'a> {
    fn call(
        &mut self,
        dst_addr: u64,
        src_addr: u64,
        n: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(n / self.cost), result);
        let dst = question_mark!(
            translate_slice_mut::<u8>(memory_mapping, dst_addr, n, self.loader_id, true),
            result
        );
        let src = question_mark!(
            translate_slice::<u8>(memory_mapping, src_addr, n, self.loader_id, true),
            result
        );
        unsafe {
            std::ptr::copy(src.as_ptr(), dst.as_mut_ptr(), n as usize);
        }
        *result = Ok(0);
    }
}
/// memcmp
pub struct SyscallMemcmp<'a> {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallMemcmp<'a> {
    fn call(
        &mut self,
        s1_addr: u64,
        s2_addr: u64,
        n: u64,
        cmp_result_addr: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(n / self.cost), result);
        let s1 = question_mark!(
            translate_slice::<u8>(memory_mapping, s1_addr, n, self.loader_id, true),
            result
        );
        let s2 = question_mark!(
            translate_slice::<u8>(memory_mapping, s2_addr, n, self.loader_id, true),
            result
        );
        let cmp_result = question_mark!(
            translate_type_mut::<i32>(memory_mapping, cmp_result_addr, self.loader_id, true),
            result
        );
        let mut i = 0;
        while i < n as usize {
            let a = s1[i];
            let b = s2[i];
            if a != b {
                *cmp_result = a as i32 - b as i32;
                *result = Ok(0);
                return;
            }
            i += 1;
        }
        *cmp_result = 0;
        *result = Ok(0);
    }
}
/// memset
pub struct SyscallMemset<'a> {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallMemset<'a> {
    fn call(
        &mut self,
        s_addr: u64,
        c: u64,
        n: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(n / self.cost), result);
        let s = question_mark!(
            translate_slice_mut::<u8>(memory_mapping, s_addr, n, self.loader_id, true),
            result
        );
        for val in s.iter_mut().take(n as usize) {
            *val = c as u8;
        }
        *result = Ok(0);
    }
}

// Blake3
pub struct SyscallBlake3<'a> {
    base_cost: u64,
    byte_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBlake3<'a> {
    fn call(
        &mut self,
        vals_addr: u64,
        vals_len: u64,
        result_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(self.compute_meter.consume(self.base_cost), result);
        let hash_result = question_mark!(
            translate_slice_mut::<u8>(
                memory_mapping,
                result_addr,
                blake3::HASH_BYTES as u64,
                self.loader_id,
                true,
            ),
            result
        );
        let mut hasher = blake3::Hasher::default();
        if vals_len > 0 {
            let vals = question_mark!(
                translate_slice::<&[u8]>(memory_mapping, vals_addr, vals_len, self.loader_id, true),
                result
            );
            for val in vals.iter() {
                let bytes = question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        val.as_ptr() as u64,
                        val.len() as u64,
                        self.loader_id,
                        true,
                    ),
                    result
                );
                question_mark!(
                    self.compute_meter
                        .consume(self.byte_cost * (val.len() as u64 / 2)),
                    result
                );
                hasher.hash(bytes);
            }
        }
        hash_result.copy_from_slice(&hasher.result().to_bytes());
        *result = Ok(0);
    }
}

// Cross-program invocation syscalls

struct AccountReferences<'a> {
    lamports: &'a mut u64,
    owner: &'a mut Pubkey,
    data: &'a mut [u8],
    vm_data_addr: u64,
    ref_to_len_in_vm: &'a mut u64,
    serialized_len_ptr: &'a mut u64,
}
type TranslatedAccount<'a> = (
    Rc<RefCell<AccountSharedData>>,
    Option<AccountReferences<'a>>,
);
type TranslatedAccounts<'a> = (
    Vec<Rc<RefCell<AccountSharedData>>>,
    Vec<Option<AccountReferences<'a>>>,
);

/// Implemented by language specific data structure translators
trait SyscallInvokeSigned<'a> {
    fn get_context_mut(&self) -> Result<RefMut<&'a mut dyn InvokeContext>, EbpfError<BpfError>>;
    fn get_context(&self) -> Result<Ref<&'a mut dyn InvokeContext>, EbpfError<BpfError>>;
    fn translate_instruction(
        &self,
        addr: u64,
        memory_mapping: &MemoryMapping,
        enforce_aligned_host_addrs: bool,
    ) -> Result<Instruction, EbpfError<BpfError>>;
    fn translate_accounts(
        &self,
        account_keys: &[Pubkey],
        program_account_index: usize,
        account_infos_addr: u64,
        account_infos_len: u64,
        memory_mapping: &MemoryMapping,
    ) -> Result<TranslatedAccounts<'a>, EbpfError<BpfError>>;
    fn translate_signers(
        &self,
        program_id: &Pubkey,
        signers_seeds_addr: u64,
        signers_seeds_len: u64,
        memory_mapping: &MemoryMapping,
        enforce_aligned_host_addrs: bool,
    ) -> Result<Vec<Pubkey>, EbpfError<BpfError>>;
}

/// Cross-program invocation called from Rust
pub struct SyscallInvokeSignedRust<'a> {
    invoke_context: Rc<RefCell<&'a mut dyn InvokeContext>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallInvokeSigned<'a> for SyscallInvokeSignedRust<'a> {
    fn get_context_mut(&self) -> Result<RefMut<&'a mut dyn InvokeContext>, EbpfError<BpfError>> {
        self.invoke_context
            .try_borrow_mut()
            .map_err(|_| SyscallError::InvokeContextBorrowFailed.into())
    }
    fn get_context(&self) -> Result<Ref<&'a mut dyn InvokeContext>, EbpfError<BpfError>> {
        self.invoke_context
            .try_borrow()
            .map_err(|_| SyscallError::InvokeContextBorrowFailed.into())
    }
    fn translate_instruction(
        &self,
        addr: u64,
        memory_mapping: &MemoryMapping,
        enforce_aligned_host_addrs: bool,
    ) -> Result<Instruction, EbpfError<BpfError>> {
        let ix = translate_type::<Instruction>(
            memory_mapping,
            addr,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?;

        check_instruction_size(
            ix.accounts.len(),
            ix.data.len(),
            &self.invoke_context.borrow(),
        )?;

        let accounts = translate_slice::<AccountMeta>(
            memory_mapping,
            ix.accounts.as_ptr() as u64,
            ix.accounts.len() as u64,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?
        .to_vec();
        let data = translate_slice::<u8>(
            memory_mapping,
            ix.data.as_ptr() as u64,
            ix.data.len() as u64,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?
        .to_vec();
        Ok(Instruction {
            program_id: ix.program_id,
            accounts,
            data,
        })
    }

    fn translate_accounts(
        &self,
        account_keys: &[Pubkey],
        program_account_index: usize,
        account_infos_addr: u64,
        account_infos_len: u64,
        memory_mapping: &MemoryMapping,
    ) -> Result<TranslatedAccounts<'a>, EbpfError<BpfError>> {
        let invoke_context = self.invoke_context.borrow();
        let enforce_aligned_host_addrs =
            invoke_context.is_feature_active(&enforce_aligned_host_addrs::id());

        let account_infos = translate_slice::<AccountInfo>(
            memory_mapping,
            account_infos_addr,
            account_infos_len,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?;
        check_account_infos(account_infos.len(), &invoke_context)?;
        let account_info_keys = account_infos
            .iter()
            .map(|account_info| {
                translate_type::<Pubkey>(
                    memory_mapping,
                    account_info.key as *const _ as u64,
                    self.loader_id,
                    enforce_aligned_host_addrs,
                )
            })
            .collect::<Result<Vec<_>, EbpfError<BpfError>>>()?;

        let translate = |account_info: &AccountInfo,
                         invoke_context: &Ref<&mut dyn InvokeContext>| {
            // Translate the account from user space

            let lamports = {
                // Double translate lamports out of RefCell
                let ptr = translate_type::<u64>(
                    memory_mapping,
                    account_info.lamports.as_ptr() as u64,
                    self.loader_id,
                    enforce_aligned_host_addrs,
                )?;
                translate_type_mut::<u64>(
                    memory_mapping,
                    *ptr,
                    self.loader_id,
                    enforce_aligned_host_addrs,
                )?
            };
            let owner = translate_type_mut::<Pubkey>(
                memory_mapping,
                account_info.owner as *const _ as u64,
                self.loader_id,
                enforce_aligned_host_addrs,
            )?;

            let (data, vm_data_addr, ref_to_len_in_vm, serialized_len_ptr) = {
                // Double translate data out of RefCell
                let data = *translate_type::<&[u8]>(
                    memory_mapping,
                    account_info.data.as_ptr() as *const _ as u64,
                    self.loader_id,
                    enforce_aligned_host_addrs,
                )?;

                if invoke_context.is_feature_active(&cpi_data_cost::id()) {
                    invoke_context.get_compute_meter().consume(
                        data.len() as u64
                            / invoke_context.get_bpf_compute_budget().cpi_bytes_per_unit,
                    )?;
                }

                let translated = translate(
                    memory_mapping,
                    AccessType::Store,
                    unsafe { (account_info.data.as_ptr() as *const u64).offset(1) as u64 },
                    8,
                )? as *mut u64;
                let ref_to_len_in_vm = unsafe { &mut *translated };
                let ref_of_len_in_input_buffer = unsafe { data.as_ptr().offset(-8) };
                let serialized_len_ptr = translate_type_mut::<u64>(
                    memory_mapping,
                    ref_of_len_in_input_buffer as *const _ as u64,
                    self.loader_id,
                    enforce_aligned_host_addrs,
                )?;
                let vm_data_addr = data.as_ptr() as u64;
                (
                    translate_slice_mut::<u8>(
                        memory_mapping,
                        vm_data_addr,
                        data.len() as u64,
                        self.loader_id,
                        enforce_aligned_host_addrs,
                    )?,
                    vm_data_addr,
                    ref_to_len_in_vm,
                    serialized_len_ptr,
                )
            };

            Ok((
                Rc::new(RefCell::new(AccountSharedData::from(Account {
                    lamports: *lamports,
                    data: data.to_vec(),
                    executable: account_info.executable,
                    owner: *owner,
                    rent_epoch: account_info.rent_epoch,
                }))),
                Some(AccountReferences {
                    lamports,
                    owner,
                    data,
                    vm_data_addr,
                    ref_to_len_in_vm,
                    serialized_len_ptr,
                }),
            ))
        };

        get_translated_accounts(
            account_keys,
            program_account_index,
            &account_info_keys,
            account_infos,
            &invoke_context,
            translate,
        )
    }

    fn translate_signers(
        &self,
        program_id: &Pubkey,
        signers_seeds_addr: u64,
        signers_seeds_len: u64,
        memory_mapping: &MemoryMapping,
        enforce_aligned_host_addrs: bool,
    ) -> Result<Vec<Pubkey>, EbpfError<BpfError>> {
        let mut signers = Vec::new();
        if signers_seeds_len > 0 {
            let signers_seeds = translate_slice::<&[&[u8]]>(
                memory_mapping,
                signers_seeds_addr,
                signers_seeds_len,
                self.loader_id,
                enforce_aligned_host_addrs,
            )?;
            if signers_seeds.len() > MAX_SIGNERS {
                return Err(SyscallError::TooManySigners.into());
            }
            for signer_seeds in signers_seeds.iter() {
                let untranslated_seeds = translate_slice::<&[u8]>(
                    memory_mapping,
                    signer_seeds.as_ptr() as *const _ as u64,
                    signer_seeds.len() as u64,
                    self.loader_id,
                    enforce_aligned_host_addrs,
                )?;
                if untranslated_seeds.len() > MAX_SEEDS {
                    return Err(SyscallError::InstructionError(
                        InstructionError::MaxSeedLengthExceeded,
                    )
                    .into());
                }
                let seeds = untranslated_seeds
                    .iter()
                    .map(|untranslated_seed| {
                        translate_slice::<u8>(
                            memory_mapping,
                            untranslated_seed.as_ptr() as *const _ as u64,
                            untranslated_seed.len() as u64,
                            self.loader_id,
                            enforce_aligned_host_addrs,
                        )
                    })
                    .collect::<Result<Vec<_>, EbpfError<BpfError>>>()?;
                let signer = Pubkey::create_program_address(&seeds, program_id)
                    .map_err(SyscallError::BadSeeds)?;
                signers.push(signer);
            }
            Ok(signers)
        } else {
            Ok(vec![])
        }
    }
}
impl<'a> SyscallObject<BpfError> for SyscallInvokeSignedRust<'a> {
    fn call(
        &mut self,
        instruction_addr: u64,
        account_infos_addr: u64,
        account_infos_len: u64,
        signers_seeds_addr: u64,
        signers_seeds_len: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        *result = call(
            self,
            instruction_addr,
            account_infos_addr,
            account_infos_len,
            signers_seeds_addr,
            signers_seeds_len,
            memory_mapping,
        );
    }
}

/// Rust representation of C's SolInstruction
#[derive(Debug)]
struct SolInstruction {
    program_id_addr: u64,
    accounts_addr: u64,
    accounts_len: usize,
    data_addr: u64,
    data_len: usize,
}

/// Rust representation of C's SolAccountMeta
#[derive(Debug)]
struct SolAccountMeta {
    pubkey_addr: u64,
    is_writable: bool,
    is_signer: bool,
}

/// Rust representation of C's SolAccountInfo
#[derive(Debug)]
struct SolAccountInfo {
    key_addr: u64,
    lamports_addr: u64,
    data_len: u64,
    data_addr: u64,
    owner_addr: u64,
    rent_epoch: u64,
    is_signer: bool,
    is_writable: bool,
    executable: bool,
}

/// Rust representation of C's SolSignerSeed
#[derive(Debug)]
struct SolSignerSeedC {
    addr: u64,
    len: u64,
}

/// Rust representation of C's SolSignerSeeds
#[derive(Debug)]
struct SolSignerSeedsC {
    addr: u64,
    len: u64,
}

/// Cross-program invocation called from C
pub struct SyscallInvokeSignedC<'a> {
    invoke_context: Rc<RefCell<&'a mut dyn InvokeContext>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallInvokeSigned<'a> for SyscallInvokeSignedC<'a> {
    fn get_context_mut(&self) -> Result<RefMut<&'a mut dyn InvokeContext>, EbpfError<BpfError>> {
        self.invoke_context
            .try_borrow_mut()
            .map_err(|_| SyscallError::InvokeContextBorrowFailed.into())
    }
    fn get_context(&self) -> Result<Ref<&'a mut dyn InvokeContext>, EbpfError<BpfError>> {
        self.invoke_context
            .try_borrow()
            .map_err(|_| SyscallError::InvokeContextBorrowFailed.into())
    }

    fn translate_instruction(
        &self,
        addr: u64,
        memory_mapping: &MemoryMapping,
        enforce_aligned_host_addrs: bool,
    ) -> Result<Instruction, EbpfError<BpfError>> {
        let ix_c = translate_type::<SolInstruction>(
            memory_mapping,
            addr,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?;

        check_instruction_size(
            ix_c.accounts_len,
            ix_c.data_len,
            &self.invoke_context.borrow(),
        )?;
        let program_id = translate_type::<Pubkey>(
            memory_mapping,
            ix_c.program_id_addr,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?;
        let meta_cs = translate_slice::<SolAccountMeta>(
            memory_mapping,
            ix_c.accounts_addr,
            ix_c.accounts_len as u64,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?;
        let data = translate_slice::<u8>(
            memory_mapping,
            ix_c.data_addr,
            ix_c.data_len as u64,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?
        .to_vec();
        let accounts = meta_cs
            .iter()
            .map(|meta_c| {
                let pubkey = translate_type::<Pubkey>(
                    memory_mapping,
                    meta_c.pubkey_addr,
                    self.loader_id,
                    enforce_aligned_host_addrs,
                )?;
                Ok(AccountMeta {
                    pubkey: *pubkey,
                    is_signer: meta_c.is_signer,
                    is_writable: meta_c.is_writable,
                })
            })
            .collect::<Result<Vec<AccountMeta>, EbpfError<BpfError>>>()?;

        Ok(Instruction {
            program_id: *program_id,
            accounts,
            data,
        })
    }

    fn translate_accounts(
        &self,
        account_keys: &[Pubkey],
        program_account_index: usize,
        account_infos_addr: u64,
        account_infos_len: u64,
        memory_mapping: &MemoryMapping,
    ) -> Result<TranslatedAccounts<'a>, EbpfError<BpfError>> {
        let invoke_context = self.invoke_context.borrow();
        let enforce_aligned_host_addrs =
            invoke_context.is_feature_active(&enforce_aligned_host_addrs::id());

        let account_infos = translate_slice::<SolAccountInfo>(
            memory_mapping,
            account_infos_addr,
            account_infos_len,
            self.loader_id,
            enforce_aligned_host_addrs,
        )?;
        check_account_infos(account_infos.len(), &invoke_context)?;
        let account_info_keys = account_infos
            .iter()
            .map(|account_info| {
                translate_type::<Pubkey>(
                    memory_mapping,
                    account_info.key_addr,
                    self.loader_id,
                    enforce_aligned_host_addrs,
                )
            })
            .collect::<Result<Vec<_>, EbpfError<BpfError>>>()?;

        let translate = |account_info: &SolAccountInfo,
                         invoke_context: &Ref<&mut dyn InvokeContext>| {
            // Translate the account from user space

            let lamports = translate_type_mut::<u64>(
                memory_mapping,
                account_info.lamports_addr,
                self.loader_id,
                enforce_aligned_host_addrs,
            )?;
            let owner = translate_type_mut::<Pubkey>(
                memory_mapping,
                account_info.owner_addr,
                self.loader_id,
                enforce_aligned_host_addrs,
            )?;
            let vm_data_addr = account_info.data_addr;

            if invoke_context.is_feature_active(&cpi_data_cost::id()) {
                invoke_context.get_compute_meter().consume(
                    account_info.data_len
                        / invoke_context.get_bpf_compute_budget().cpi_bytes_per_unit,
                )?;
            }

            let data = translate_slice_mut::<u8>(
                memory_mapping,
                vm_data_addr,
                account_info.data_len,
                self.loader_id,
                enforce_aligned_host_addrs,
            )?;

            let first_info_addr = &account_infos[0] as *const _ as u64;
            let addr = &account_info.data_len as *const u64 as u64;
            let vm_addr = account_infos_addr + (addr - first_info_addr);
            let _ = translate(
                memory_mapping,
                AccessType::Store,
                vm_addr,
                size_of::<u64>() as u64,
            )?;
            let ref_to_len_in_vm = unsafe { &mut *(addr as *mut u64) };

            let ref_of_len_in_input_buffer =
                unsafe { (account_info.data_addr as *mut u8).offset(-8) };
            let serialized_len_ptr = translate_type_mut::<u64>(
                memory_mapping,
                ref_of_len_in_input_buffer as *const _ as u64,
                self.loader_id,
                enforce_aligned_host_addrs,
            )?;

            Ok((
                Rc::new(RefCell::new(AccountSharedData::from(Account {
                    lamports: *lamports,
                    data: data.to_vec(),
                    executable: account_info.executable,
                    owner: *owner,
                    rent_epoch: account_info.rent_epoch,
                }))),
                Some(AccountReferences {
                    lamports,
                    owner,
                    data,
                    vm_data_addr,
                    ref_to_len_in_vm,
                    serialized_len_ptr,
                }),
            ))
        };

        get_translated_accounts(
            account_keys,
            program_account_index,
            &account_info_keys,
            account_infos,
            &invoke_context,
            translate,
        )
    }

    fn translate_signers(
        &self,
        program_id: &Pubkey,
        signers_seeds_addr: u64,
        signers_seeds_len: u64,
        memory_mapping: &MemoryMapping,
        enforce_aligned_host_addrs: bool,
    ) -> Result<Vec<Pubkey>, EbpfError<BpfError>> {
        if signers_seeds_len > 0 {
            let signers_seeds = translate_slice::<SolSignerSeedC>(
                memory_mapping,
                signers_seeds_addr,
                signers_seeds_len,
                self.loader_id,
                enforce_aligned_host_addrs,
            )?;
            if signers_seeds.len() > MAX_SIGNERS {
                return Err(SyscallError::TooManySigners.into());
            }
            Ok(signers_seeds
                .iter()
                .map(|signer_seeds| {
                    let seeds = translate_slice::<SolSignerSeedC>(
                        memory_mapping,
                        signer_seeds.addr,
                        signer_seeds.len,
                        self.loader_id,
                        enforce_aligned_host_addrs,
                    )?;
                    if seeds.len() > MAX_SEEDS {
                        return Err(SyscallError::InstructionError(
                            InstructionError::MaxSeedLengthExceeded,
                        )
                        .into());
                    }
                    let seeds_bytes = seeds
                        .iter()
                        .map(|seed| {
                            translate_slice::<u8>(
                                memory_mapping,
                                seed.addr,
                                seed.len,
                                self.loader_id,
                                enforce_aligned_host_addrs,
                            )
                        })
                        .collect::<Result<Vec<_>, EbpfError<BpfError>>>()?;
                    Pubkey::create_program_address(&seeds_bytes, program_id)
                        .map_err(|err| SyscallError::BadSeeds(err).into())
                })
                .collect::<Result<Vec<_>, EbpfError<BpfError>>>()?)
        } else {
            Ok(vec![])
        }
    }
}
impl<'a> SyscallObject<BpfError> for SyscallInvokeSignedC<'a> {
    fn call(
        &mut self,
        instruction_addr: u64,
        account_infos_addr: u64,
        account_infos_len: u64,
        signers_seeds_addr: u64,
        signers_seeds_len: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        *result = call(
            self,
            instruction_addr,
            account_infos_addr,
            account_infos_len,
            signers_seeds_addr,
            signers_seeds_len,
            memory_mapping,
        );
    }
}

fn get_translated_accounts<'a, T, F>(
    account_keys: &[Pubkey],
    program_account_index: usize,
    account_info_keys: &[&Pubkey],
    account_infos: &[T],
    invoke_context: &Ref<&mut dyn InvokeContext>,
    do_translate: F,
) -> Result<TranslatedAccounts<'a>, EbpfError<BpfError>>
where
    F: Fn(&T, &Ref<&mut dyn InvokeContext>) -> Result<TranslatedAccount<'a>, EbpfError<BpfError>>,
{
    let mut accounts = Vec::with_capacity(account_keys.len());
    let mut refs = Vec::with_capacity(account_keys.len());
    for (i, ref account_key) in account_keys.iter().enumerate() {
        let account = invoke_context.get_account(account_key).ok_or_else(|| {
            ic_msg!(
                invoke_context,
                "Instruction references an unknown account {}",
                account_key
            );
            SyscallError::InstructionError(InstructionError::MissingAccount)
        })?;

        if i == program_account_index || account.borrow().executable() {
            // Use the known account
            accounts.push(account);
            refs.push(None);
        } else if let Some(account_info) =
            account_info_keys
                .iter()
                .zip(account_infos)
                .find_map(|(key, account_info)| {
                    if key == account_key {
                        Some(account_info)
                    } else {
                        None
                    }
                })
        {
            let (account, account_ref) = do_translate(account_info, invoke_context)?;
            accounts.push(account);
            refs.push(account_ref);
        } else {
            ic_msg!(
                invoke_context,
                "Instruction references an unknown account {}",
                account_key
            );
            return Err(SyscallError::InstructionError(InstructionError::MissingAccount).into());
        }
    }

    Ok((accounts, refs))
}

fn check_instruction_size(
    num_accounts: usize,
    data_len: usize,
    invoke_context: &Ref<&mut dyn InvokeContext>,
) -> Result<(), EbpfError<BpfError>> {
    let size = num_accounts
        .saturating_mul(size_of::<AccountMeta>())
        .saturating_add(data_len);
    let max_size = invoke_context
        .get_bpf_compute_budget()
        .max_cpi_instruction_size;
    if size > max_size {
        return Err(SyscallError::InstructionTooLarge(size, max_size).into());
    }
    Ok(())
}

fn check_account_infos(
    len: usize,
    invoke_context: &Ref<&mut dyn InvokeContext>,
) -> Result<(), EbpfError<BpfError>> {
    if len * size_of::<Pubkey>()
        > invoke_context
            .get_bpf_compute_budget()
            .max_cpi_instruction_size
    {
        // Cap the number of account_infos a caller can pass to approximate
        // maximum that accounts that could be passed in an instruction
        return Err(SyscallError::TooManyAccounts.into());
    };
    Ok(())
}

fn check_authorized_program(
    program_id: &Pubkey,
    instruction_data: &[u8],
) -> Result<(), EbpfError<BpfError>> {
    if native_loader::check_id(program_id)
        || bpf_loader::check_id(program_id)
        || bpf_loader_deprecated::check_id(program_id)
        || (bpf_loader_upgradeable::check_id(program_id)
            && !(bpf_loader_upgradeable::is_upgrade_instruction(instruction_data)
                || bpf_loader_upgradeable::is_set_authority_instruction(instruction_data)))
    {
        return Err(SyscallError::ProgramNotSupported(*program_id).into());
    }
    Ok(())
}

#[allow(clippy::type_complexity)]
fn get_upgradeable_executable(
    callee_program_id: &Pubkey,
    program_account: &Rc<RefCell<AccountSharedData>>,
    invoke_context: &Ref<&mut dyn InvokeContext>,
) -> Result<Option<(Pubkey, Rc<RefCell<AccountSharedData>>)>, EbpfError<BpfError>> {
    if program_account.borrow().owner() == &bpf_loader_upgradeable::id() {
        match program_account.borrow().state() {
            Ok(UpgradeableLoaderState::Program {
                programdata_address,
            }) => {
                if let Some(account) = invoke_context.get_account(&programdata_address) {
                    Ok(Some((programdata_address, account)))
                } else {
                    ic_msg!(
                        invoke_context,
                        "Unknown upgradeable programdata account {}",
                        programdata_address,
                    );
                    Err(SyscallError::InstructionError(InstructionError::MissingAccount).into())
                }
            }
            _ => {
                ic_msg!(
                    invoke_context,
                    "Invalid upgradeable program account {}",
                    callee_program_id,
                );
                Err(SyscallError::InstructionError(InstructionError::InvalidAccountData).into())
            }
        }
    } else {
        Ok(None)
    }
}

/// Call process instruction, common to both Rust and C
fn call<'a>(
    syscall: &mut dyn SyscallInvokeSigned<'a>,
    instruction_addr: u64,
    account_infos_addr: u64,
    account_infos_len: u64,
    signers_seeds_addr: u64,
    signers_seeds_len: u64,
    memory_mapping: &MemoryMapping,
) -> Result<u64, EbpfError<BpfError>> {
    let (
        message,
        executables,
        accounts,
        account_refs,
        caller_write_privileges,
        demote_sysvar_write_locks,
    ) = {
        let invoke_context = syscall.get_context()?;

        invoke_context
            .get_compute_meter()
            .consume(invoke_context.get_bpf_compute_budget().invoke_units)?;

        let enforce_aligned_host_addrs =
            invoke_context.is_feature_active(&enforce_aligned_host_addrs::id());

        let caller_program_id = invoke_context
            .get_caller()
            .map_err(SyscallError::InstructionError)?;

        // Translate and verify caller's data

        let instruction = syscall.translate_instruction(
            instruction_addr,
            memory_mapping,
            enforce_aligned_host_addrs,
        )?;
        let signers = syscall.translate_signers(
            caller_program_id,
            signers_seeds_addr,
            signers_seeds_len,
            memory_mapping,
            enforce_aligned_host_addrs,
        )?;
        let keyed_account_refs = invoke_context
            .get_keyed_accounts()
            .map_err(SyscallError::InstructionError)?
            .iter()
            .collect::<Vec<&KeyedAccount>>();
        let (message, callee_program_id, callee_program_id_index) =
            MessageProcessor::create_message(
                &instruction,
                &keyed_account_refs,
                &signers,
                &invoke_context,
            )
            .map_err(SyscallError::InstructionError)?;
        let caller_write_privileges = message
            .account_keys
            .iter()
            .map(|key| {
                if let Some(keyed_account) = keyed_account_refs
                    .iter()
                    .find(|keyed_account| key == keyed_account.unsigned_key())
                {
                    keyed_account.is_writable()
                } else {
                    false
                }
            })
            .collect::<Vec<bool>>();
        check_authorized_program(&callee_program_id, &instruction.data)?;
        let (accounts, account_refs) = syscall.translate_accounts(
            &message.account_keys,
            callee_program_id_index,
            account_infos_addr,
            account_infos_len,
            memory_mapping,
        )?;

        // Construct executables

        let program_account = accounts
            .get(callee_program_id_index)
            .ok_or_else(|| {
                ic_msg!(invoke_context, "Unknown program {}", callee_program_id,);
                SyscallError::InstructionError(InstructionError::MissingAccount)
            })?
            .clone();
        let programdata_executable =
            get_upgradeable_executable(&callee_program_id, &program_account, &invoke_context)?;
        let mut executables = vec![(callee_program_id, program_account)];
        if let Some(executable) = programdata_executable {
            executables.push(executable);
        }

        // Record the instruction

        invoke_context.record_instruction(&instruction);

        (
            message,
            executables,
            accounts,
            account_refs,
            caller_write_privileges,
            invoke_context.is_feature_active(&demote_sysvar_write_locks::id()),
        )
    };

    // Process instruction

    #[allow(clippy::deref_addrof)]
    match MessageProcessor::process_cross_program_instruction(
        &message,
        &executables,
        &accounts,
        &caller_write_privileges,
        *(&mut *(syscall.get_context_mut()?)),
    ) {
        Ok(()) => (),
        Err(err) => {
            return Err(SyscallError::InstructionError(err).into());
        }
    }

    // Copy results back to caller
    {
        let invoke_context = syscall.get_context()?;
        for (i, (account, account_ref)) in accounts.iter().zip(account_refs).enumerate() {
            let account = account.borrow();
            if let Some(mut account_ref) = account_ref {
                if message.is_writable(i, demote_sysvar_write_locks) && !account.executable() {
                    *account_ref.lamports = account.lamports();
                    *account_ref.owner = *account.owner();
                    if account_ref.data.len() != account.data().len() {
                        if !account_ref.data.is_empty() {
                            // Only support for `CreateAccount` at this time.
                            // Need a way to limit total realloc size across multiple CPI calls
                            ic_msg!(
                                invoke_context,
                                "Inner instructions do not support realloc, only SystemProgram::CreateAccount",
                            );
                            return Err(SyscallError::InstructionError(
                                InstructionError::InvalidRealloc,
                            )
                            .into());
                        }
                        if account.data().len()
                            > account_ref.data.len() + MAX_PERMITTED_DATA_INCREASE
                        {
                            ic_msg!(
                                invoke_context,
                                "SystemProgram::CreateAccount data size limited to {} in inner instructions",
                                MAX_PERMITTED_DATA_INCREASE
                            );
                            return Err(SyscallError::InstructionError(
                                InstructionError::InvalidRealloc,
                            )
                            .into());
                        }
                        if invoke_context.is_feature_active(&update_data_on_realloc::id()) {
                            account_ref.data = translate_slice_mut::<u8>(
                                memory_mapping,
                                account_ref.vm_data_addr,
                                account.data().len() as u64,
                                &bpf_loader_deprecated::id(), // Don't care since it is byte aligned
                                true,
                            )?;
                        } else {
                            let _ = translate(
                                memory_mapping,
                                AccessType::Store,
                                account_ref.vm_data_addr,
                                account.data().len() as u64,
                            )?;
                        }
                        *account_ref.ref_to_len_in_vm = account.data().len() as u64;
                        *account_ref.serialized_len_ptr = account.data().len() as u64;
                    }
                    account_ref
                        .data
                        .copy_from_slice(&account.data()[0..account_ref.data.len()]);
                }
            }
        }
    }

    Ok(SUCCESS)
}

/// Bignum data costs calculator
pub const MAX_BIGNUM_BYTE_SIZE: u64 = 1024;
macro_rules! calc_bignum_cost {
    ($input_self:expr, $input_size_in_bytes:expr) => {
        $input_self.cost
            + ($input_self.word_cost as f64
                * ($input_size_in_bytes as f64 / $input_self.word_div_cost as f64).ceil())
                as u64
    };
}

/// Transposes a bn::BigNum into an FfiBignumber array of bytes
fn bignum_to_ffibignumber(
    bignum: &BigNum,
    bnffi: &mut FfiBigNumber,
    loader_id: &Pubkey,
    memory_mapping: &MemoryMapping,
    result: &mut Result<u64, EbpfError<BpfError>>,
) {
    // Get the BigNum vector and size
    let big_number_bytes = bignum.to_vec();
    let big_number_len = big_number_bytes.len() as u64;
    (*bnffi).is_negative = bignum.is_negative();
    // Fail if result size exceeds 1024 bytes (8192 bits)
    if big_number_len > MAX_BIGNUM_BYTE_SIZE {
        *result = Err(SyscallError::BigNumberSizeError.into())
    }
    // Fail if we can't support the full array of bytes
    else if big_number_len as u64 > bnffi.data_len {
        *result = Err(SyscallError::BigNumberArgError(big_number_len, bnffi.data_len).into())
    } else {
        // Get the output array pointer
        let bn_buffer_out = question_mark!(
            translate_slice_mut::<u8>(memory_mapping, bnffi.data, bnffi.data_len, loader_id, true),
            result
        );
        // Zero result
        if big_number_len == 0 {
            bn_buffer_out[0] = 0u8;
            (*bnffi).data_len = 1;
        } else if big_number_len == bnffi.data_len {
            (*bn_buffer_out).copy_from_slice(&big_number_bytes);
        } else {
            bn_buffer_out[..big_number_bytes.len()].copy_from_slice(&big_number_bytes);
            (*bnffi).data_len = big_number_len;
        }
        *result = Ok(SUCCESS)
    }
}

/// Transposes a FfiBignumber array of bytes to a bn::BigNum
fn ffibignumber_to_bignum(ffi: &FfiBigNumber, ffi_data: &[u8]) -> Result<BigNum, SyscallError> {
    let ffi_bn = BigNum::from_slice(ffi_data);
    match ffi_bn {
        Ok(mut bn) => {
            bn.set_negative(ffi.is_negative);
            Ok(bn)
        }
        Err(es) => {
            if let Some(reason) = es.errors()[0].reason() {
                Err(SyscallError::BigNumberOperationError(reason.to_string()))
            } else {
                Err(SyscallError::BigNumberErrorUnknown)
            }
        }
    }
}

/// BIGNUM sol_bignum_from_u32 ingests a u32 and
/// produces an absolute big endian array
struct SyscallBigNumFromU32<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumFromU32<'a> {
    fn call(
        &mut self,
        bn_ffi_out_addr: u64,
        u32_val: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(self, 4f64)),
            result
        );
        // Get the output FfiBigNumber structure
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );
        // Process BigNum call and populate results and
        // collect, if any, error from OpenSSL
        match BigNum::from_u32(u32_val as u32) {
            Ok(bn) => bignum_to_ffibignumber(
                &bn,
                bn_syscall_result,
                self.loader_id,
                memory_mapping,
                result,
            ),
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into())
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into())
                }
            }
        }
    }
}
/// BIGNUM sol_bignum_from_dec_str bounds checks,
/// validates and ingests a decimal string and
/// produces an absoluate big endian array
struct SyscallBigNumFromDecStr<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumFromDecStr<'a> {
    fn call(
        &mut self,
        in_dec_str_addr: u64,
        in_dec_str_len: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        question_mark!(
            self.compute_meter
                .consume(calc_bignum_cost!(self, in_dec_str_len as f64)),
            result
        );

        // Get the string and convert to BigNum
        let in_dec_raw = question_mark!(
            translate_slice::<u8>(
                memory_mapping,
                in_dec_str_addr,
                in_dec_str_len,
                self.loader_id,
                true
            ),
            result
        );

        let i = match in_dec_raw.iter().position(|byte| *byte == 0) {
            Some(i) => i,
            None => in_dec_str_len as usize,
        };
        let raw_string = match from_utf8(&in_dec_raw[..i]) {
            Ok(s) => s,
            Err(_) => {
                *result = Err(SyscallError::BigNumberToBytesError.into());
                return;
            }
        };

        // Get the output FfiBigNumber structure
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );
        // Handle expected result > MAX_BIGNUM_BYTE_SIZE bytes
        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        }
        // Handle buffer too small or empty strings
        else if bn_syscall_result.data_len == 0 || raw_string.is_empty() {
            *result = Err(SyscallError::BigNumberToBytesError.into())
        } else {
            // Process BigNum call and populate results
            match BigNum::from_dec_str(raw_string) {
                Ok(bn) => bignum_to_ffibignumber(
                    &bn,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(_) => *result = Err(SyscallError::BigNumberFromDecStrError.into()),
            };
        }
    }
}

/// BIGNUM sol_bignum_from_slice ingests a absolute big endian array for
/// validation and size checking
struct SyscallBigNumFromBytes<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumFromBytes<'a> {
    fn call(
        &mut self,
        in_bytes_addr: u64,
        in_bytes_len: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Compute costs
        question_mark!(
            self.compute_meter
                .consume(calc_bignum_cost!(self, in_bytes_len as f64)),
            result
        );

        // Get array pointer of bytes to consume
        let rhs_data = question_mark!(
            translate_slice::<u8>(
                memory_mapping,
                in_bytes_addr,
                in_bytes_len,
                self.loader_id,
                true
            ),
            result
        );
        // Process BigNum call and populate results
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );
        // Check max length of 1024 bytes
        if !(1..=MAX_BIGNUM_BYTE_SIZE).contains(&in_bytes_len) {
            *result = Err(SyscallError::BigNumberSizeError.into())
        } else {
            match BigNum::from_slice(rhs_data) {
                Ok(bn_result) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        *result =
                            Err(SyscallError::BigNumberFromSliceError(reason.to_string()).into());
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into());
                    }
                }
            }
        }
    }
}

/// BIGNUM sol_bignum_add bounds checks and adds two BigNumber and
/// produces a result.
struct SyscallBigNumAdd<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumAdd<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(
                self,
                (bn_syscall_lhs.data_len + bn_syscall_rhs.data_len) as f64
            )),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the data pointer big numbers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into())
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );
        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.checked_add(&lhs_bn, &rhs_bn) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberAddError(reason.to_string()).into())
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into())
                    }
                }
            };
        }
    }
}

/// BIGNUM sol_bignum_sub bounds checks and subtracts two BigNumber and
/// produces a result.
struct SyscallBigNumSub<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumSub<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(
                self,
                (bn_syscall_lhs.data_len + bn_syscall_rhs.data_len) as f64
            )),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );

        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.checked_sub(&lhs_bn, &rhs_bn) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberSubError(reason.to_string()).into())
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into())
                    }
                }
            };
        }
    }
}
/// BIGNUM sol_bignum_mul bounds checks and multiplies two BigNumber and
/// produces a result.
struct SyscallBigNumMul<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumMul<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(
                self,
                (bn_syscall_lhs.data_len + bn_syscall_rhs.data_len) as f64
            )),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let mut bn_ctx = match BigNumContext::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );

        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.checked_mul(&lhs_bn, &rhs_bn, &mut bn_ctx) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberMulError(reason.to_string()).into());
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into());
                    }
                }
            }
        }
    }
}
/// BIGNUM sol_bignum_div bounds checks and divides two BigNumber and
/// produces a result.
struct SyscallBigNumDiv<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumDiv<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(
                self,
                (bn_syscall_lhs.data_len + bn_syscall_rhs.data_len) as f64
            )),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let mut bn_ctx = match BigNumContext::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );

        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.checked_div(&lhs_bn, &rhs_bn, &mut bn_ctx) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberDivError(reason.to_string()).into());
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into());
                    }
                }
            }
        }
    }
}
/// BIGNUM sol_bignum_sqr bounds checks squares a BigNumber and
/// produces a result.
struct SyscallBigNumSqr<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumSqr<'a> {
    fn call(
        &mut self,
        rhs_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter
                .consume(calc_bignum_cost!(self, bn_syscall_rhs.data_len as f64)),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get number to sqr ffi
        let rhs_data = question_mark!(
            translate_slice::<u8>(
                memory_mapping,
                bn_syscall_rhs.data,
                bn_syscall_rhs.data_len as u64,
                self.loader_id,
                true
            ),
            result
        );
        // Convert to BigNum
        let rhs_bn = match BigNum::from_slice(rhs_data) {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let mut bn_ctx = match BigNumContext::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );

        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.sqr(&rhs_bn, &mut bn_ctx) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberSqrError(reason.to_string()).into());
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into());
                    }
                }
            }
        }
    }
}
/// BIGNUM sol_bignum_exp bounds checks raises a BigNumber by exponent
/// and produces a result.
struct SyscallBigNumExp<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumExp<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(
                self,
                (bn_syscall_lhs.data_len + bn_syscall_rhs.data_len) as f64
            )),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let mut bn_ctx = match BigNumContext::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );
        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.exp(&lhs_bn, &rhs_bn, &mut bn_ctx) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberExpError(reason.to_string()).into())
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into())
                    }
                }
            }
        }
    }
}
/// BIGNUM sol_bignum_mod_sqr bounds checks and performs
/// lhs^2 % rhs and produces a result
struct SyscallBigNumModSqr<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumModSqr<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(
                self,
                (bn_syscall_lhs.data_len + bn_syscall_rhs.data_len) as f64
            )),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let mut bn_ctx = match BigNumContext::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );

        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.mod_sqr(&lhs_bn, &rhs_bn, &mut bn_ctx) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        *result =
                            Err(SyscallError::BigNumberModSqrError(reason.to_string()).into());
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into());
                    }
                }
            }
        }
    }
}

/// Calculates a data cost for mod_exp as per https://eips.ethereum.org/EIPS/eip-198
fn calculate_exp_byte_cost(
    base: &BigNum,
    exponent: &BigNum,
    modulus: &BigNum,
) -> Result<u64, SyscallError> {
    let zero = match BigNum::from_u32(0) {
        Ok(bn) => bn,
        Err(es) => {
            if let Some(reason) = es.errors()[0].reason() {
                return Err(SyscallError::BigNumberOperationError(reason.to_string()));
            } else {
                return Err(SyscallError::BigNumberErrorUnknown);
            }
        }
    };
    let exp_bytes_length = exponent.num_bytes() as u32;
    let exp_bits_length = exponent.num_bits() as u32;
    let base_bytes_length = base.num_bytes() as u32;
    // Adjusted length calculation
    let adjusted_exp_length = match exp_bytes_length {
        0..=32 if exponent == &zero => 0,
        0..=32 => exp_bits_length - 1,
        _ => {
            let first_32 = match BigNum::from_slice(&exponent.as_ref().to_vec()[0..31]) {
                Ok(bn) => bn,
                Err(es) => {
                    if let Some(reason) = es.errors()[0].reason() {
                        return Err(SyscallError::BigNumberOperationError(reason.to_string()));
                    } else {
                        return Err(SyscallError::BigNumberErrorUnknown);
                    }
                }
            };
            if first_32 == zero {
                8 * (exp_bytes_length - 32)
            } else {
                8 * (exp_bytes_length - 32) + ((first_32.num_bits() as u32) - 1)
            }
        }
    } as u64;
    // mult_complexity
    let mult_compexity_arg = std::cmp::max(base_bytes_length, modulus.num_bytes() as u32) as u64;
    let mult_complexity = if mult_compexity_arg <= 64 {
        mult_compexity_arg * mult_compexity_arg
    } else if mult_compexity_arg <= 1024 {
        (((mult_compexity_arg * mult_compexity_arg) / 4) + (96 * mult_compexity_arg)) - 3_072
    } else {
        (((mult_compexity_arg * mult_compexity_arg) / 16) + (480 * mult_compexity_arg)) - 199_680
    };
    // Calc data costs and return
    Ok(
        ((mult_complexity * std::cmp::max(adjusted_exp_length, 1u64)) as f64 / 20f64).floor()
            as u64,
    )
}
/// BIGNUM sol_bignum_mod_exp bounds checks and performs
/// lhs^rhs % mod and produces a result
struct SyscallBigNumModExp<'a> {
    cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumModExp<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        mod_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the MOD FfiBigNumber value structure
        let bn_syscall_mod = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, mod_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let mod_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_mod.data,
                        bn_syscall_mod.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        // Compute costs
        let calc_val = match calculate_exp_byte_cost(&lhs_bn, &rhs_bn, &mod_bn) {
            Ok(val) => val,
            Err(e) => {
                *result = Err(e.into());
                return;
            }
        };
        question_mark!(self.compute_meter.consume(self.cost + calc_val), result);
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_mod.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let mut bn_ctx = match BigNumContext::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );

        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.mod_exp(&lhs_bn, &rhs_bn, &mod_bn, &mut bn_ctx) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(e) => {
                    if let Some(reason) = e.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberModExpError(reason.to_string()).into())
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into())
                    }
                }
            }
        }
    }
}
/// BIGNUM sol_bignum_mod_mul bounds checks and performs
/// lhs*rhs % mod and produces a result
struct SyscallBigNumModMul<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumModMul<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        mod_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the MOD FfiBigNumber value structure
        let bn_syscall_mod = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, mod_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(
                self,
                (bn_syscall_lhs.data_len + bn_syscall_rhs.data_len + bn_syscall_mod.data_len)
                    as f64
            )),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_mod.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let mod_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_mod.data,
                        bn_syscall_mod.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let mut bn_ctx = match BigNumContext::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );

        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.mod_mul(&lhs_bn, &rhs_bn, &mod_bn, &mut bn_ctx) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(e) => {
                    if let Some(reason) = e.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberModMulError(reason.to_string()).into())
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into())
                    }
                }
            }
        }
    }
}
/// BIGNUM sol_bignum_mod_inv bounds checks and performs
/// inverse(lhs % rhs) and produces a result
struct SyscallBigNumModInv<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumModInv<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        rhs_ffi_in_addr: u64,
        bn_ffi_out_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Get the RHS FfiBigNumber value structure
        let bn_syscall_rhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, rhs_ffi_in_addr, self.loader_id, true),
            result
        );
        // Compute costs
        question_mark!(
            self.compute_meter.consume(calc_bignum_cost!(
                self,
                (bn_syscall_lhs.data_len + bn_syscall_rhs.data_len) as f64
            )),
            result
        );
        // Bounds check on input
        if bn_syscall_rhs.data_len > MAX_BIGNUM_BYTE_SIZE
            || bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE
        {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        let rhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_rhs.data,
                        bn_syscall_rhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );

        // Process BigNum call and populate results
        let mut bn_result = match BigNum::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let mut bn_ctx = match BigNumContext::new() {
            Ok(bn) => bn,
            Err(es) => {
                if let Some(reason) = es.errors()[0].reason() {
                    *result = Err(SyscallError::BigNumberOperationError(reason.to_string()).into());
                } else {
                    *result = Err(SyscallError::BigNumberErrorUnknown.into());
                }
                return;
            }
        };
        let bn_syscall_result = question_mark!(
            translate_type_mut::<FfiBigNumber>(
                memory_mapping,
                bn_ffi_out_addr,
                self.loader_id,
                true
            ),
            result
        );

        if bn_syscall_result.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
        } else {
            match bn_result.mod_inverse(&lhs_bn, &rhs_bn, &mut bn_ctx) {
                Ok(_) => bignum_to_ffibignumber(
                    &bn_result,
                    bn_syscall_result,
                    self.loader_id,
                    memory_mapping,
                    result,
                ),
                Err(e) => {
                    if let Some(reason) = e.errors()[0].reason() {
                        *result = Err(SyscallError::BigNumberModInvError(reason.to_string()).into())
                    } else {
                        *result = Err(SyscallError::BigNumberErrorUnknown.into())
                    }
                }
            }
        }
    }
}
/// Logs a BigNumber to system log
struct SyscallBigNumLog<'a> {
    cost: u64,
    word_cost: u64,
    word_div_cost: u64,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    logger: Rc<RefCell<dyn Logger>>,
    loader_id: &'a Pubkey,
}
impl<'a> SyscallObject<BpfError> for SyscallBigNumLog<'a> {
    fn call(
        &mut self,
        lhs_ffi_in_addr: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &MemoryMapping,
        result: &mut Result<u64, EbpfError<BpfError>>,
    ) {
        // Get the LHS FfiBigNumber value structure
        let bn_syscall_lhs = question_mark!(
            translate_type::<FfiBigNumber>(memory_mapping, lhs_ffi_in_addr, self.loader_id, true),
            result
        );
        question_mark!(
            self.compute_meter
                .consume(calc_bignum_cost!(self, bn_syscall_lhs.data_len as f64)),
            result
        );
        // Bounds check on input
        if bn_syscall_lhs.data_len > MAX_BIGNUM_BYTE_SIZE {
            *result = Err(SyscallError::BigNumberSizeError.into());
            return;
        }

        // Get the BigNum from data pointers
        let lhs_bn = question_mark!(
            ffibignumber_to_bignum(
                bn_syscall_lhs,
                question_mark!(
                    translate_slice::<u8>(
                        memory_mapping,
                        bn_syscall_lhs.data,
                        bn_syscall_lhs.data_len as u64,
                        self.loader_id,
                        true
                    ),
                    result
                )
            ),
            result
        );
        stable_log::program_log(&self.logger, &lhs_bn.to_string());
        *result = Ok(0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use solana_rbpf::{
        ebpf::HOST_ALIGN, memory_region::MemoryRegion, user_error::UserError, vm::Config,
    };
    use solana_sdk::{
        bpf_loader,
        fee_calculator::FeeCalculator,
        hash::hashv,
        process_instruction::{MockComputeMeter, MockInvokeContext, MockLogger},
    };
    use std::str::FromStr;

    const DEFAULT_CONFIG: Config = Config {
        max_call_depth: 20,
        stack_frame_size: 4_096,
        enable_instruction_meter: true,
        enable_instruction_tracing: false,
    };

    const LONG_DEC_STRING: &str =
        "1470463693494555670176851280755142329532258274256991544781479988\
    712408107190720087233560906792937436573943189716784305633216335039\
    300236370809933808677983409391545753391897467230180786617074456716\
    591448871466263060696957107957862111484694673874424855359234132302\
    162208163361387727626078022804936564470716886986414133429438273232\
    416190048073715996321578752244853524209178212395809614878549824744\
    227969245726015222693764433413133633359171080169137831743765672068\
    374040331773668233371864426354886263106537340208256187214278963052\
    996538599452325797319977413534714912781503130883692806087195354368\
    8304190675878204079994222";

    const NEG_LONG_DEC_STRING: &str =
        "-1470463693494555670176851280755142329532258274256991544781479988\
    712408107190720087233560906792937436573943189716784305633216335039\
    300236370809933808677983409391545753391897467230180786617074456716\
    591448871466263060696957107957862111484694673874424855359234132302\
    162208163361387727626078022804936564470716886986414133429438273232\
    416190048073715996321578752244853524209178212395809614878549824744\
    227969245726015222693764433413133633359171080169137831743765672068\
    374040331773668233371864426354886263106537340208256187214278963052\
    996538599452325797319977413534714912781503130883692806087195354368\
    8304190675878204079994222";

    const NOT_A_VALID_DEC_STRING: &str = "ZBikk123A972";

    macro_rules! assert_access_violation {
        ($result:expr, $va:expr, $len:expr) => {
            match $result {
                Err(EbpfError::AccessViolation(_, _, va, len, _)) if $va == va && len == len => (),
                _ => panic!(),
            }
        };
    }

    #[test]
    fn test_translate() {
        const START: u64 = 100;
        const LENGTH: u64 = 1000;
        let data = vec![0u8; LENGTH as usize];
        let addr = data.as_ptr() as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion::new_from_slice(&data, START, 0, false)],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let cases = vec![
            (true, START, 0, addr),
            (true, START, 1, addr),
            (true, START, LENGTH, addr),
            (true, START + 1, LENGTH - 1, addr + 1),
            (false, START + 1, LENGTH, 0),
            (true, START + LENGTH - 1, 1, addr + LENGTH - 1),
            (true, START + LENGTH, 0, addr + LENGTH),
            (false, START + LENGTH, 1, 0),
            (false, START, LENGTH + 1, 0),
            (false, 0, 0, 0),
            (false, 0, 1, 0),
            (false, START - 1, 0, 0),
            (false, START - 1, 1, 0),
            (true, START + LENGTH / 2, LENGTH / 2, addr + LENGTH / 2),
        ];
        for (ok, start, length, value) in cases {
            if ok {
                assert_eq!(
                    translate(&memory_mapping, AccessType::Load, start, length).unwrap(),
                    value
                )
            } else {
                assert!(translate(&memory_mapping, AccessType::Load, start, length).is_err())
            }
        }
    }

    #[test]
    fn test_translate_type() {
        // Pubkey
        let pubkey = solana_sdk::pubkey::new_rand();
        let addr = &pubkey as *const _ as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 100,
                len: std::mem::size_of::<Pubkey>() as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        let translated_pubkey =
            translate_type::<Pubkey>(&memory_mapping, 100, &bpf_loader::id(), true).unwrap();
        assert_eq!(pubkey, *translated_pubkey);

        // Instruction
        let instruction = Instruction::new_with_bincode(
            solana_sdk::pubkey::new_rand(),
            &"foobar",
            vec![AccountMeta::new(solana_sdk::pubkey::new_rand(), false)],
        );
        let addr = &instruction as *const _ as u64;
        let mut memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 96,
                len: std::mem::size_of::<Instruction>() as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        let translated_instruction =
            translate_type::<Instruction>(&memory_mapping, 96, &bpf_loader::id(), true).unwrap();
        assert_eq!(instruction, *translated_instruction);
        memory_mapping.resize_region::<BpfError>(0, 1).unwrap();
        assert!(
            translate_type::<Instruction>(&memory_mapping, 100, &bpf_loader::id(), true).is_err()
        );
    }

    #[test]
    fn test_translate_slice() {
        // zero len
        let good_data = vec![1u8, 2, 3, 4, 5];
        let data: Vec<u8> = vec![];
        assert_eq!(0x1 as *const u8, data.as_ptr());
        let addr = good_data.as_ptr() as *const _ as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 100,
                len: good_data.len() as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        let translated_data = translate_slice::<u8>(
            &memory_mapping,
            data.as_ptr() as u64,
            0,
            &bpf_loader::id(),
            true,
        )
        .unwrap();
        assert_eq!(data, translated_data);
        assert_eq!(0, translated_data.len());

        // u8
        let mut data = vec![1u8, 2, 3, 4, 5];
        let addr = data.as_ptr() as *const _ as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 100,
                len: data.len() as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        let translated_data = translate_slice::<u8>(
            &memory_mapping,
            100,
            data.len() as u64,
            &bpf_loader::id(),
            true,
        )
        .unwrap();
        assert_eq!(data, translated_data);
        data[0] = 10;
        assert_eq!(data, translated_data);
        assert!(translate_slice::<u8>(
            &memory_mapping,
            data.as_ptr() as u64,
            u64::MAX,
            &bpf_loader::id(),
            true,
        )
        .is_err());

        assert!(translate_slice::<u8>(
            &memory_mapping,
            100 - 1,
            data.len() as u64,
            &bpf_loader::id(),
            true,
        )
        .is_err());

        // u64
        let mut data = vec![1u64, 2, 3, 4, 5];
        let addr = data.as_ptr() as *const _ as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 96,
                len: (data.len() * size_of::<u64>()) as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        let translated_data = translate_slice::<u64>(
            &memory_mapping,
            96,
            data.len() as u64,
            &bpf_loader::id(),
            true,
        )
        .unwrap();
        assert_eq!(data, translated_data);
        data[0] = 10;
        assert_eq!(data, translated_data);
        assert!(
            translate_slice::<u64>(&memory_mapping, 96, u64::MAX, &bpf_loader::id(), true,)
                .is_err()
        );

        // Pubkeys
        let mut data = vec![solana_sdk::pubkey::new_rand(); 5];
        let addr = data.as_ptr() as *const _ as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 100,
                len: (data.len() * std::mem::size_of::<Pubkey>()) as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        let translated_data = translate_slice::<Pubkey>(
            &memory_mapping,
            100,
            data.len() as u64,
            &bpf_loader::id(),
            true,
        )
        .unwrap();
        assert_eq!(data, translated_data);
        data[0] = solana_sdk::pubkey::new_rand(); // Both should point to same place
        assert_eq!(data, translated_data);
    }

    #[test]
    fn test_translate_string_and_do() {
        let string = "Gaggablaghblagh!";
        let addr = string.as_ptr() as *const _ as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 100,
                len: string.len() as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        assert_eq!(
            42,
            translate_string_and_do(
                &memory_mapping,
                100,
                string.len() as u64,
                &bpf_loader::id(),
                true,
                &mut |string: &str| {
                    assert_eq!(string, "Gaggablaghblagh!");
                    Ok(42)
                }
            )
            .unwrap()
        );
    }

    #[test]
    #[should_panic(expected = "UserError(SyscallError(Abort))")]
    fn test_syscall_abort() {
        let memory_mapping =
            MemoryMapping::new::<UserError>(vec![MemoryRegion::default()], &DEFAULT_CONFIG)
                .unwrap();
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        SyscallAbort::call(
            &mut SyscallAbort {},
            0,
            0,
            0,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        result.unwrap();
    }

    #[test]
    #[should_panic(expected = "UserError(SyscallError(Panic(\"Gaggablaghblagh!\", 42, 84)))")]
    fn test_syscall_sol_panic() {
        let string = "Gaggablaghblagh!";
        let addr = string.as_ptr() as *const _ as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 100,
                len: string.len() as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter {
                remaining: string.len() as u64 - 1,
            }));
        let mut syscall_panic = SyscallPanic {
            compute_meter,
            loader_id: &bpf_loader::id(),
            enforce_aligned_host_addrs: true,
        };
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_panic.call(
            100,
            string.len() as u64,
            42,
            84,
            0,
            &memory_mapping,
            &mut result,
        );
        assert_eq!(
            Err(EbpfError::UserError(BpfError::SyscallError(
                SyscallError::InstructionError(InstructionError::ComputationalBudgetExceeded)
            ))),
            result
        );

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter {
                remaining: string.len() as u64,
            }));
        let mut syscall_panic = SyscallPanic {
            compute_meter,
            loader_id: &bpf_loader::id(),
            enforce_aligned_host_addrs: true,
        };
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_panic.call(
            100,
            string.len() as u64,
            42,
            84,
            0,
            &memory_mapping,
            &mut result,
        );
        result.unwrap();
    }

    #[test]
    fn test_syscall_sol_log() {
        let string = "Gaggablaghblagh!";
        let addr = string.as_ptr() as *const _ as u64;

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter { remaining: 1000000 }));
        let log = Rc::new(RefCell::new(vec![]));
        let logger: Rc<RefCell<dyn Logger>> =
            Rc::new(RefCell::new(MockLogger { log: log.clone() }));
        let mut syscall_sol_log = SyscallLog {
            compute_meter,
            logger,
            loader_id: &bpf_loader::id(),
            enforce_aligned_host_addrs: true,
        };
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 100,
                len: string.len() as u64,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_log.call(
            100,
            string.len() as u64,
            0,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        result.unwrap();
        assert_eq!(log.borrow().len(), 1);
        assert_eq!(log.borrow()[0], "Program log: Gaggablaghblagh!");

        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_log.call(
            101, // AccessViolation
            string.len() as u64,
            0,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        assert_access_violation!(result, 101, string.len() as u64);
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_log.call(
            100,
            string.len() as u64 * 2, // AccessViolation
            0,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        assert_access_violation!(result, 100, string.len() as u64 * 2);
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_log.call(
            100,
            string.len() as u64,
            0,
            0,
            0,
            &memory_mapping,
            &mut result,
        );

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter {
                remaining: (string.len() as u64 * 2) - 1,
            }));
        let logger: Rc<RefCell<dyn Logger>> = Rc::new(RefCell::new(MockLogger { log }));
        let mut syscall_sol_log = SyscallLog {
            compute_meter,
            logger,
            loader_id: &bpf_loader::id(),
            enforce_aligned_host_addrs: true,
        };
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_log.call(
            100,
            string.len() as u64,
            0,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        result.unwrap();
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_log.call(
            100,
            string.len() as u64,
            0,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        assert_eq!(
            Err(EbpfError::UserError(BpfError::SyscallError(
                SyscallError::InstructionError(InstructionError::ComputationalBudgetExceeded)
            ))),
            result
        );
    }

    #[test]
    fn test_syscall_sol_log_u64() {
        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter {
                remaining: std::u64::MAX,
            }));
        let log = Rc::new(RefCell::new(vec![]));
        let logger: Rc<RefCell<dyn Logger>> =
            Rc::new(RefCell::new(MockLogger { log: log.clone() }));
        let mut syscall_sol_log_u64 = SyscallLogU64 {
            cost: 0,
            compute_meter,
            logger,
        };
        let memory_mapping = MemoryMapping::new::<UserError>(vec![], &DEFAULT_CONFIG).unwrap();

        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_log_u64.call(1, 2, 3, 4, 5, &memory_mapping, &mut result);
        result.unwrap();

        assert_eq!(log.borrow().len(), 1);
        assert_eq!(log.borrow()[0], "Program log: 0x1, 0x2, 0x3, 0x4, 0x5");
    }

    #[test]
    fn test_syscall_sol_pubkey() {
        let pubkey = Pubkey::from_str("MoqiU1vryuCGQSxFKA1SZ316JdLEFFhoAu6cKUNk7dN").unwrap();
        let addr = &pubkey.as_ref()[0] as *const _ as u64;

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter { remaining: 2 }));
        let log = Rc::new(RefCell::new(vec![]));
        let logger: Rc<RefCell<dyn Logger>> =
            Rc::new(RefCell::new(MockLogger { log: log.clone() }));
        let mut syscall_sol_pubkey = SyscallLogPubkey {
            cost: 1,
            compute_meter,
            logger,
            loader_id: &bpf_loader::id(),
            enforce_aligned_host_addrs: true,
        };
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![MemoryRegion {
                host_addr: addr,
                vm_addr: 100,
                len: 32,
                vm_gap_shift: 63,
                is_writable: false,
            }],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_pubkey.call(100, 0, 0, 0, 0, &memory_mapping, &mut result);
        result.unwrap();
        assert_eq!(log.borrow().len(), 1);
        assert_eq!(
            log.borrow()[0],
            "Program log: MoqiU1vryuCGQSxFKA1SZ316JdLEFFhoAu6cKUNk7dN"
        );
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_pubkey.call(
            101, // AccessViolation
            32,
            0,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        assert_access_violation!(result, 101, 32);
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall_sol_pubkey.call(100, 32, 0, 0, 0, &memory_mapping, &mut result);
        assert_eq!(
            Err(EbpfError::UserError(BpfError::SyscallError(
                SyscallError::InstructionError(InstructionError::ComputationalBudgetExceeded)
            ))),
            result
        );
    }

    #[test]
    fn test_syscall_sol_alloc_free() {
        // large alloc
        {
            let heap = AlignedMemory::new_with_size(100, HOST_ALIGN);
            let memory_mapping = MemoryMapping::new::<UserError>(
                vec![MemoryRegion::new_from_slice(
                    heap.as_slice(),
                    MM_HEAP_START,
                    0,
                    true,
                )],
                &DEFAULT_CONFIG,
            )
            .unwrap();
            let mut syscall = SyscallAllocFree {
                aligned: true,
                allocator: BpfAllocator::new(heap, MM_HEAP_START),
            };
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
            syscall.call(100, 0, 0, 0, 0, &memory_mapping, &mut result);
            assert_ne!(result.unwrap(), 0);
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
            syscall.call(100, 0, 0, 0, 0, &memory_mapping, &mut result);
            assert_eq!(result.unwrap(), 0);
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
            syscall.call(u64::MAX, 0, 0, 0, 0, &memory_mapping, &mut result);
            assert_eq!(result.unwrap(), 0);
        }
        // many small unaligned allocs
        {
            let heap = AlignedMemory::new_with_size(100, HOST_ALIGN);
            let memory_mapping = MemoryMapping::new::<UserError>(
                vec![MemoryRegion::new_from_slice(
                    heap.as_slice(),
                    MM_HEAP_START,
                    0,
                    true,
                )],
                &DEFAULT_CONFIG,
            )
            .unwrap();
            let mut syscall = SyscallAllocFree {
                aligned: false,
                allocator: BpfAllocator::new(heap, MM_HEAP_START),
            };
            for _ in 0..100 {
                let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
                syscall.call(1, 0, 0, 0, 0, &memory_mapping, &mut result);
                assert_ne!(result.unwrap(), 0);
            }
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
            syscall.call(100, 0, 0, 0, 0, &memory_mapping, &mut result);
            assert_eq!(result.unwrap(), 0);
        }
        // many small aligned allocs
        {
            let heap = AlignedMemory::new_with_size(100, HOST_ALIGN);
            let memory_mapping = MemoryMapping::new::<UserError>(
                vec![MemoryRegion::new_from_slice(
                    heap.as_slice(),
                    MM_HEAP_START,
                    0,
                    true,
                )],
                &DEFAULT_CONFIG,
            )
            .unwrap();
            let mut syscall = SyscallAllocFree {
                aligned: true,
                allocator: BpfAllocator::new(heap, MM_HEAP_START),
            };
            for _ in 0..12 {
                let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
                syscall.call(1, 0, 0, 0, 0, &memory_mapping, &mut result);
                assert_ne!(result.unwrap(), 0);
            }
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
            syscall.call(100, 0, 0, 0, 0, &memory_mapping, &mut result);
            assert_eq!(result.unwrap(), 0);
        }
        // aligned allocs

        fn check_alignment<T>() {
            let heap = AlignedMemory::new_with_size(100, HOST_ALIGN);
            let memory_mapping = MemoryMapping::new::<UserError>(
                vec![MemoryRegion::new_from_slice(
                    heap.as_slice(),
                    MM_HEAP_START,
                    0,
                    true,
                )],
                &DEFAULT_CONFIG,
            )
            .unwrap();
            let mut syscall = SyscallAllocFree {
                aligned: true,
                allocator: BpfAllocator::new(heap, MM_HEAP_START),
            };
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
            syscall.call(
                size_of::<u8>() as u64,
                0,
                0,
                0,
                0,
                &memory_mapping,
                &mut result,
            );
            let address = result.unwrap();
            assert_ne!(address, 0);
            assert_eq!((address as *const u8).align_offset(align_of::<u8>()), 0);
        }
        check_alignment::<u8>();
        check_alignment::<u16>();
        check_alignment::<u32>();
        check_alignment::<u64>();
        check_alignment::<u128>();
    }

    #[test]
    fn test_syscall_sha256() {
        let bytes1 = "Gaggablaghblagh!";
        let bytes2 = "flurbos";

        #[allow(dead_code)]
        struct MockSlice {
            pub addr: u64,
            pub len: usize,
        }
        let mock_slice1 = MockSlice {
            addr: 4096,
            len: bytes1.len(),
        };
        let mock_slice2 = MockSlice {
            addr: 8192,
            len: bytes2.len(),
        };
        let bytes_to_hash = [mock_slice1, mock_slice2];
        let hash_result = [0; HASH_BYTES];
        let ro_len = bytes_to_hash.len() as u64;
        let ro_va = 96;
        let rw_va = 192;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![
                MemoryRegion {
                    host_addr: bytes1.as_ptr() as *const _ as u64,
                    vm_addr: 4096,
                    len: bytes1.len() as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: bytes2.as_ptr() as *const _ as u64,
                    vm_addr: 8192,
                    len: bytes2.len() as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: bytes_to_hash.as_ptr() as *const _ as u64,
                    vm_addr: 96,
                    len: 32,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: hash_result.as_ptr() as *const _ as u64,
                    vm_addr: rw_va,
                    len: HASH_BYTES as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
            ],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter {
                remaining: (bytes1.len() + bytes2.len()) as u64,
            }));
        let mut syscall = SyscallSha256 {
            sha256_base_cost: 0,
            sha256_byte_cost: 2,
            compute_meter,
            loader_id: &bpf_loader_deprecated::id(),
            enforce_aligned_host_addrs: true,
        };

        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(ro_va, ro_len, rw_va, 0, 0, &memory_mapping, &mut result);
        result.unwrap();

        let hash_local = hashv(&[bytes1.as_ref(), bytes2.as_ref()]).to_bytes();
        assert_eq!(hash_result, hash_local);
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(
            ro_va - 1, // AccessViolation
            ro_len,
            rw_va,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        assert_access_violation!(result, ro_va - 1, ro_len);
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(
            ro_va,
            ro_len + 1, // AccessViolation
            rw_va,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        assert_access_violation!(result, ro_va, ro_len + 1);
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(
            ro_va,
            ro_len,
            rw_va - 1, // AccessViolation
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        assert_access_violation!(result, rw_va - 1, HASH_BYTES as u64);

        syscall.call(ro_va, ro_len, rw_va, 0, 0, &memory_mapping, &mut result);
        assert_eq!(
            Err(EbpfError::UserError(BpfError::SyscallError(
                SyscallError::InstructionError(InstructionError::ComputationalBudgetExceeded)
            ))),
            result
        );
    }

    #[test]
    fn test_syscall_get_sysvar() {
        // Test clock sysvar
        {
            let got_clock = Clock::default();
            let got_clock_va = 2048;

            let memory_mapping = MemoryMapping::new::<UserError>(
                vec![MemoryRegion {
                    host_addr: &got_clock as *const _ as u64,
                    vm_addr: got_clock_va,
                    len: size_of::<Clock>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                }],
                &DEFAULT_CONFIG,
            )
            .unwrap();

            let src_clock = Clock {
                slot: 1,
                epoch_start_timestamp: 2,
                epoch: 3,
                leader_schedule_epoch: 4,
                unix_timestamp: 5,
            };
            let mut invoke_context = MockInvokeContext::new(vec![]);
            let mut data = vec![];
            bincode::serialize_into(&mut data, &src_clock).unwrap();
            invoke_context
                .sysvars
                .push((sysvar::clock::id(), Some(Rc::new(data))));

            let mut syscall = SyscallGetClockSysvar {
                invoke_context: Rc::new(RefCell::new(&mut invoke_context)),
                loader_id: &bpf_loader::id(),
            };
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);

            syscall.call(got_clock_va, 0, 0, 0, 0, &memory_mapping, &mut result);
            result.unwrap();
            assert_eq!(got_clock, src_clock);
        }

        // Test epoch_schedule sysvar
        {
            let got_epochschedule = EpochSchedule::default();
            let got_epochschedule_va = 2048;

            let memory_mapping = MemoryMapping::new::<UserError>(
                vec![MemoryRegion {
                    host_addr: &got_epochschedule as *const _ as u64,
                    vm_addr: got_epochschedule_va,
                    len: size_of::<EpochSchedule>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                }],
                &DEFAULT_CONFIG,
            )
            .unwrap();

            let src_epochschedule = EpochSchedule {
                slots_per_epoch: 1,
                leader_schedule_slot_offset: 2,
                warmup: false,
                first_normal_epoch: 3,
                first_normal_slot: 4,
            };
            let mut invoke_context = MockInvokeContext::new(vec![]);
            let mut data = vec![];
            bincode::serialize_into(&mut data, &src_epochschedule).unwrap();
            invoke_context
                .sysvars
                .push((sysvar::epoch_schedule::id(), Some(Rc::new(data))));

            let mut syscall = SyscallGetEpochScheduleSysvar {
                invoke_context: Rc::new(RefCell::new(&mut invoke_context)),
                loader_id: &bpf_loader::id(),
            };
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);

            syscall.call(
                got_epochschedule_va,
                0,
                0,
                0,
                0,
                &memory_mapping,
                &mut result,
            );
            result.unwrap();
            assert_eq!(got_epochschedule, src_epochschedule);
        }

        // Test fees sysvar
        {
            let got_fees = Fees::default();
            let got_fees_va = 2048;

            let memory_mapping = MemoryMapping::new::<UserError>(
                vec![MemoryRegion {
                    host_addr: &got_fees as *const _ as u64,
                    vm_addr: got_fees_va,
                    len: size_of::<Fees>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                }],
                &DEFAULT_CONFIG,
            )
            .unwrap();

            let src_fees = Fees {
                fee_calculator: FeeCalculator {
                    lamports_per_signature: 1,
                },
            };
            let mut invoke_context = MockInvokeContext::new(vec![]);
            let mut data = vec![];
            bincode::serialize_into(&mut data, &src_fees).unwrap();
            invoke_context
                .sysvars
                .push((sysvar::fees::id(), Some(Rc::new(data))));

            let mut syscall = SyscallGetFeesSysvar {
                invoke_context: Rc::new(RefCell::new(&mut invoke_context)),
                loader_id: &bpf_loader::id(),
            };
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);

            syscall.call(got_fees_va, 0, 0, 0, 0, &memory_mapping, &mut result);
            result.unwrap();
            assert_eq!(got_fees, src_fees);
        }

        // Test rent sysvar
        {
            let got_rent = Rent::default();
            let got_rent_va = 2048;

            let memory_mapping = MemoryMapping::new::<UserError>(
                vec![MemoryRegion {
                    host_addr: &got_rent as *const _ as u64,
                    vm_addr: got_rent_va,
                    len: size_of::<Rent>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                }],
                &DEFAULT_CONFIG,
            )
            .unwrap();

            let src_rent = Rent {
                lamports_per_byte_year: 1,
                exemption_threshold: 2.0,
                burn_percent: 3,
            };
            let mut invoke_context = MockInvokeContext::new(vec![]);
            let mut data = vec![];
            bincode::serialize_into(&mut data, &src_rent).unwrap();
            invoke_context
                .sysvars
                .push((sysvar::rent::id(), Some(Rc::new(data))));

            let mut syscall = SyscallGetRentSysvar {
                invoke_context: Rc::new(RefCell::new(&mut invoke_context)),
                loader_id: &bpf_loader::id(),
            };
            let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);

            syscall.call(got_rent_va, 0, 0, 0, 0, &memory_mapping, &mut result);
            result.unwrap();
            assert_eq!(got_rent, src_rent);
        }
    }

    // Bignum tests
    // creates new bignum with u32 value
    fn new_u32_bignum(val: u32) -> (Vec<u8>, bool) {
        let mut my_buffer = Vec::<u8>::with_capacity(4);
        let bytes_len = 4u64;
        let mut sol_out_ffi = FfiBigNumber {
            data: 1024u64,
            data_len: bytes_len,
            is_negative: false,
        };
        // let bytes_len_addr = &mut bytes_len as *mut _ as u64;
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![
                MemoryRegion {
                    host_addr: &mut sol_out_ffi as *mut _ as u64,
                    vm_addr: 96,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
                MemoryRegion {
                    host_addr: my_buffer.as_mut_ptr() as *mut _ as u64,
                    vm_addr: 1024,
                    len: bytes_len,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
            ],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter { remaining: 400 }));
        let mut syscall = SyscallBigNumFromU32 {
            cost: 100,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter,
            loader_id: &bpf_loader::id(),
        };
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(96, val as u64, 0, 0, 0, &memory_mapping, &mut result);
        result.unwrap();
        unsafe { my_buffer.set_len(sol_out_ffi.data_len as usize) };
        (my_buffer, sol_out_ffi.is_negative)
    }
    // creates new bignum from decimal string
    fn new_dec_str_bignum(string: &str) -> Result<(Vec<u8>, bool), EbpfError<BpfError>> {
        let dec_str_addr = string.as_ptr() as *const _ as u64;
        let dec_str_len = string.len();
        // Return Ffi
        let mut my_buffer = Vec::<u8>::with_capacity(dec_str_len);
        let bytes_len = my_buffer.capacity() as u64;
        let mut sol_out_ffi = FfiBigNumber {
            data: 8196u64,
            data_len: bytes_len,
            is_negative: false,
        };

        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![
                MemoryRegion {
                    host_addr: dec_str_addr,
                    vm_addr: 2048,
                    len: dec_str_len as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: &mut sol_out_ffi as *mut _ as u64,
                    vm_addr: 96,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
                MemoryRegion {
                    host_addr: my_buffer.as_mut_ptr() as *mut _ as u64,
                    vm_addr: 8196,
                    len: bytes_len,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
            ],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter { remaining: 10_000 }));
        let mut syscall = SyscallBigNumFromDecStr {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter,
            loader_id: &bpf_loader::id(),
        };
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(
            2048,
            dec_str_len as u64,
            96,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        match result {
            Ok(_) => {
                unsafe { my_buffer.set_len(sol_out_ffi.data_len as usize) };
                Ok((my_buffer, sol_out_ffi.is_negative))
            }
            Err(e) => Err(e),
        }
    }
    // creates new bignum from a slice of data
    fn new_from_slice_bignum(val: &[u8]) -> Result<(Vec<u8>, bool), EbpfError<BpfError>> {
        let mut my_buffer = Vec::<u8>::with_capacity(val.len());
        let bytes_len = val.len();
        let mut sol_out_ffi = FfiBigNumber {
            data: 8192u64,
            data_len: bytes_len as u64,
            is_negative: false,
        };

        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![
                MemoryRegion {
                    host_addr: val.as_ptr() as *const _ as u64,
                    vm_addr: 0,
                    len: bytes_len as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: &mut sol_out_ffi as *mut _ as u64,
                    vm_addr: 4096,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
                MemoryRegion {
                    host_addr: my_buffer.as_mut_ptr() as *mut _ as u64,
                    vm_addr: 8192,
                    len: bytes_len as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
            ],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter { remaining: 400 }));
        let mut syscall = SyscallBigNumFromBytes {
            cost: 100,
            word_cost: 1,
            word_div_cost: 32,
            compute_meter,
            loader_id: &bpf_loader::id(),
        };

        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(
            0,
            bytes_len as u64,
            4096,
            0,
            0,
            &memory_mapping,
            &mut result,
        );
        match result {
            Ok(_) => {
                unsafe { my_buffer.set_len(sol_out_ffi.data_len as usize) };
                Ok((my_buffer, sol_out_ffi.is_negative))
            }
            Err(e) => Err(e),
        }
    }

    #[test]
    fn test_syscall_bignum_from_u32() {
        let (bn_20, is_negative) = new_u32_bignum(20u32);
        assert!(!is_negative);
        let bn = BigNum::from_u32(20u32).unwrap().to_vec();
        assert_eq!(bn_20, bn);
    }

    #[test]
    fn test_syscall_bignum_from_dec_str() {
        let (bn_long_dec, is_negative) = new_dec_str_bignum(LONG_DEC_STRING).unwrap();
        assert!(!is_negative);
        let bns = BigNum::from_dec_str(LONG_DEC_STRING).unwrap();
        let bns_vec = bns.as_ref().to_vec();
        assert_eq!(bn_long_dec, bns_vec);
        let (bn_neg_long_dec, is_negative) = new_dec_str_bignum(NEG_LONG_DEC_STRING).unwrap();
        assert!(is_negative);
        let bns = BigNum::from_dec_str(NEG_LONG_DEC_STRING).unwrap();
        let bns_vec = bns.as_ref().to_vec();
        assert_eq!(bn_neg_long_dec, bns_vec);
    }

    #[test]
    fn test_syscall_bignum_from_dec_str_fails() {
        assert!(new_dec_str_bignum(NOT_A_VALID_DEC_STRING).is_err());
        assert!(new_dec_str_bignum("").is_err());
        assert!(new_dec_str_bignum(&format!("{}{}", LONG_DEC_STRING, LONG_DEC_STRING)).is_err());
    }

    #[test]
    fn test_syscall_bignum_from_slice() {
        let (bn_from_slice, is_negative) =
            new_from_slice_bignum(&[255, 255, 255, 255, 255, 255, 255, 255]).unwrap();
        assert_eq!(bn_from_slice, [255, 255, 255, 255, 255, 255, 255, 255]);
        assert!(!is_negative);
        assert!(new_from_slice_bignum(&vec![255u8; 1024]).is_ok());
    }
    #[test]
    fn test_syscall_bignum_from_slice_fail() {
        assert!(new_from_slice_bignum(&vec![255u8; 2048]).is_err());
        assert!(new_from_slice_bignum(&[]).is_err());
    }

    // Wrapper for two argument bignum calls
    fn invoke_bignum_two_arg_calls(
        lhs: u32,
        rhs: u32,
        syscall: &mut dyn solana_rbpf::vm::SyscallObject<BpfError>,
    ) -> Result<(Vec<u8>, bool), EbpfError<BpfError>> {
        let (bn_arg1, bn1_is_negative) = new_u32_bignum(lhs);
        let (bn_arg2, bn2_is_negative) = new_u32_bignum(rhs);
        let bn1_len = bn_arg1.len();
        let bn2_len = bn_arg2.len();
        // Setup Ffi
        let lhs_in_ffi = FfiBigNumber {
            data: 1024u64,
            data_len: bn1_len as u64,
            is_negative: bn1_is_negative,
        };
        let rhs_in_ffi = FfiBigNumber {
            data: 2048u64,
            data_len: bn2_len as u64,
            is_negative: bn2_is_negative,
        };
        let mut result_vec = Vec::<u8>::with_capacity(bn1_len + bn2_len);
        let mut sol_out_ffi = FfiBigNumber {
            data: 4096u64,
            data_len: result_vec.capacity() as u64,
            is_negative: false,
        };

        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![
                MemoryRegion {
                    host_addr: &lhs_in_ffi as *const _ as u64,
                    vm_addr: 24,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: bn_arg1.as_ptr() as *const _ as u64,
                    vm_addr: 1024,
                    len: bn1_len as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: &rhs_in_ffi as *const _ as u64,
                    vm_addr: 48,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: bn_arg2.as_ptr() as *const _ as u64,
                    vm_addr: 2048,
                    len: bn2_len as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
                MemoryRegion {
                    host_addr: &mut sol_out_ffi as *mut _ as u64,
                    vm_addr: 72,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
                MemoryRegion {
                    host_addr: result_vec.as_mut_ptr() as *mut _ as u64,
                    vm_addr: 4096,
                    len: result_vec.capacity() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
            ],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(24, 48, 72, 0, 0, &memory_mapping, &mut result);
        match result {
            Ok(_) => {
                unsafe { result_vec.set_len(sol_out_ffi.data_len as usize) };
                Ok((result_vec, sol_out_ffi.is_negative))
            }
            Err(e) => Err(e),
        }
    }

    fn invoke_bignum_three_arg_calls(
        lhs: u32,
        rhs: u32,
        mod_arg: u32,
        syscall: &mut dyn solana_rbpf::vm::SyscallObject<BpfError>,
    ) -> Result<(Vec<u8>, bool), EbpfError<BpfError>> {
        let (bn_arg1, bn1_is_negative) = new_u32_bignum(lhs);
        let (bn_arg2, bn2_is_negative) = new_u32_bignum(rhs);
        let (bn_arg3, bn3_is_negative) = new_u32_bignum(mod_arg);
        let bn1_len = bn_arg1.len();
        let bn2_len = bn_arg2.len();
        let bn3_len = bn_arg3.len();
        // Setup Ffi
        let lhs_in_ffi = FfiBigNumber {
            data: 1024u64,
            data_len: bn1_len as u64,
            is_negative: bn1_is_negative,
        };
        let rhs_in_ffi = FfiBigNumber {
            data: 2048u64,
            data_len: bn2_len as u64,
            is_negative: bn2_is_negative,
        };
        let mod_in_ffi = FfiBigNumber {
            data: 4096u64,
            data_len: bn3_len as u64,
            is_negative: bn3_is_negative,
        };
        let mut result_vec = Vec::<u8>::with_capacity(bn1_len * bn3_len);
        let mut sol_out_ffi = FfiBigNumber {
            data: 5120u64,
            data_len: result_vec.capacity() as u64,
            is_negative: false,
        };
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![
                MemoryRegion {
                    host_addr: &lhs_in_ffi as *const _ as u64,
                    vm_addr: 24,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: bn_arg1.as_ptr() as *const _ as u64,
                    vm_addr: 1024,
                    len: bn1_len as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: &rhs_in_ffi as *const _ as u64,
                    vm_addr: 48,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: bn_arg2.as_ptr() as *const _ as u64,
                    vm_addr: 2048,
                    len: bn2_len as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: &mod_in_ffi as *const _ as u64,
                    vm_addr: 72,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: bn_arg3.as_ptr() as *const _ as u64,
                    vm_addr: 4096,
                    len: bn3_len as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: &mut sol_out_ffi as *mut _ as u64,
                    vm_addr: 96,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
                MemoryRegion {
                    host_addr: result_vec.as_mut_ptr() as *mut _ as u64,
                    vm_addr: 5120,
                    len: result_vec.capacity() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
            ],
            &DEFAULT_CONFIG,
        )
        .unwrap();
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(24, 48, 72, 96, 0, &memory_mapping, &mut result);
        match result {
            Ok(_) => {
                unsafe { result_vec.set_len(sol_out_ffi.data_len as usize) };
                Ok((result_vec, sol_out_ffi.is_negative))
            }
            Err(e) => Err(e),
        }
    }

    #[test]
    fn test_syscall_bignum_simple_math_pass() {
        let mut syscall_bn_add = SyscallBigNumAdd {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        let mut syscall_bn_sub = SyscallBigNumSub {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        let mut syscall_bn_mul = SyscallBigNumMul {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        let mut syscall_bn_div = SyscallBigNumDiv {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        assert!(invoke_bignum_two_arg_calls(5, 258, &mut syscall_bn_add).is_ok());
        assert!(invoke_bignum_two_arg_calls(5, 258, &mut syscall_bn_sub).is_ok());
        assert!(invoke_bignum_two_arg_calls(300, 10, &mut syscall_bn_div).is_ok());
        assert!(invoke_bignum_two_arg_calls(5, 5, &mut syscall_bn_mul).is_ok());
    }

    #[test]
    fn test_syscall_bignum_simple_math_fail() {
        let mut syscall_bn_div = SyscallBigNumDiv {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        assert!(invoke_bignum_two_arg_calls(300, 0, &mut syscall_bn_div).is_err());
    }

    #[test]
    fn test_syscall_bignum_complex_math() {
        let mut syscall_bn_exp = SyscallBigNumExp {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        let mut syscall_bn_mod_sqr = SyscallBigNumModSqr {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        let mut syscall_bn_mod_inv = SyscallBigNumModInv {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        assert!(invoke_bignum_two_arg_calls(8, 2, &mut syscall_bn_exp).is_ok());
        assert!(invoke_bignum_two_arg_calls(11, 7, &mut syscall_bn_mod_sqr).is_ok());
        assert!(invoke_bignum_two_arg_calls(415, 77, &mut syscall_bn_mod_inv).is_ok());
    }

    #[test]
    fn test_syscall_bignum_complex_math2() {
        let mut syscall_bn_mod_exp = SyscallBigNumModExp {
            cost: 1,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        let mut syscall_bn_mod_mul = SyscallBigNumModMul {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter: Rc::new(RefCell::new(MockComputeMeter { remaining: 200_u64 })),
            loader_id: &bpf_loader::id(),
        };
        assert!(invoke_bignum_three_arg_calls(300, 11, 7, &mut syscall_bn_mod_exp).is_ok());
        assert!(invoke_bignum_three_arg_calls(300, 11, 7, &mut syscall_bn_mod_mul).is_ok());
    }

    #[test]
    fn test_syscall_bignum_sqr() {
        let (bn_arg1, bn1_is_negative) = new_u32_bignum(300);
        let bn1_len = bn_arg1.len();

        // Setup Ffi
        let rhs_in_ffi = FfiBigNumber {
            data: 1024u64,
            data_len: bn1_len as u64,
            is_negative: bn1_is_negative,
        };
        let mut result_vec = Vec::<u8>::with_capacity(bn1_len * 2);
        let mut sol_out_ffi = FfiBigNumber {
            data: 4096u64,
            data_len: result_vec.capacity() as u64,
            is_negative: false,
        };
        let memory_mapping = MemoryMapping::new::<UserError>(
            vec![
                MemoryRegion {
                    host_addr: &rhs_in_ffi as *const _ as u64,
                    vm_addr: 24,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: bn_arg1.as_ptr() as *const _ as u64,
                    vm_addr: 1024,
                    len: bn1_len as u64,
                    vm_gap_shift: 63,
                    is_writable: false,
                },
                MemoryRegion {
                    host_addr: &mut sol_out_ffi as *mut _ as u64,
                    vm_addr: 72,
                    len: std::mem::size_of::<FfiBigNumber>() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
                MemoryRegion {
                    host_addr: result_vec.as_mut_ptr() as *mut _ as u64,
                    vm_addr: 4096,
                    len: result_vec.capacity() as u64,
                    vm_gap_shift: 63,
                    is_writable: true,
                },
            ],
            &DEFAULT_CONFIG,
        )
        .unwrap();

        let compute_meter: Rc<RefCell<dyn ComputeMeter>> =
            Rc::new(RefCell::new(MockComputeMeter {
                remaining: (20 + 20) as u64,
            }));
        let mut syscall = SyscallBigNumSqr {
            cost: 1,
            word_cost: 6,
            word_div_cost: 32,
            compute_meter,
            loader_id: &bpf_loader::id(),
        };
        let mut result: Result<u64, EbpfError<BpfError>> = Ok(0);
        syscall.call(24, 72, 0, 8, 16, &memory_mapping, &mut result);
        result.unwrap();
        unsafe { result_vec.set_len(sol_out_ffi.data_len as usize) };
        assert_eq!(result_vec, vec![1, 95, 144]);
    }
    #[test]
    fn test_valid_big_endian() {
        let be_bytes_in = u32::to_be_bytes(256);
        let bn_bytes_out = BigNum::from_slice(&be_bytes_in).unwrap().to_vec();
        assert_eq!(bn_bytes_out, [1, 0]);

        let be_bytes_in: Vec<u8> = vec![0, 0, 0, 0, 0, 0, 1, 0];
        let bn_bytes_out = BigNum::from_slice(&be_bytes_in).unwrap().to_vec();
        assert_eq!(bn_bytes_out, [1, 0]);
    }
}
