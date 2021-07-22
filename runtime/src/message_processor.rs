use crate::{
    accounts::Accounts, ancestors::Ancestors, instruction_recorder::InstructionRecorder,
    log_collector::LogCollector, native_loader::NativeLoader, rent_collector::RentCollector,
};
use log::*;
use serde::{Deserialize, Serialize};
use solana_measure::measure::Measure;
use solana_sdk::{
    account::{AccountSharedData, ReadableAccount, WritableAccount},
    account_utils::StateMut,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    compute_budget::ComputeBudget,
    feature_set::{
        instructions_sysvar_enabled, neon_evm_compute_budget, tx_wide_compute_cap,
        updated_verify_policy, FeatureSet,
    },
    ic_logger_msg, ic_msg,
    instruction::{CompiledInstruction, Instruction, InstructionError},
    keyed_account::{create_keyed_accounts_unified, keyed_account_at_index, KeyedAccount},
    message::Message,
    native_loader,
    process_instruction::{
        ComputeMeter, Executor, InvokeContext, InvokeContextStackFrame, Logger,
        ProcessInstructionWithContext,
    },
    pubkey::Pubkey,
    rent::Rent,
    system_program,
    sysvar::instructions,
    transaction::TransactionError,
};
use std::{
    cell::{Ref, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

pub struct Executors {
    pub executors: HashMap<Pubkey, Arc<dyn Executor>>,
    pub is_dirty: bool,
}
impl Default for Executors {
    fn default() -> Self {
        Self {
            executors: HashMap::default(),
            is_dirty: false,
        }
    }
}
impl Executors {
    pub fn insert(&mut self, key: Pubkey, executor: Arc<dyn Executor>) {
        let _ = self.executors.insert(key, executor);
        self.is_dirty = true;
    }
    pub fn get(&self, key: &Pubkey) -> Option<Arc<dyn Executor>> {
        self.executors.get(key).cloned()
    }
}

#[derive(Default, Debug)]
pub struct ProgramTiming {
    pub accumulated_us: u64,
    pub accumulated_units: u64,
    pub count: u32,
}

#[derive(Default, Debug)]
pub struct ExecuteDetailsTimings {
    pub serialize_us: u64,
    pub create_vm_us: u64,
    pub execute_us: u64,
    pub deserialize_us: u64,
    pub changed_account_count: u64,
    pub total_account_count: u64,
    pub total_data_size: usize,
    pub data_size_changed: usize,
    pub per_program_timings: HashMap<Pubkey, ProgramTiming>,
}
impl ExecuteDetailsTimings {
    pub fn accumulate(&mut self, other: &ExecuteDetailsTimings) {
        self.serialize_us += other.serialize_us;
        self.create_vm_us += other.create_vm_us;
        self.execute_us += other.execute_us;
        self.deserialize_us += other.deserialize_us;
        self.changed_account_count += other.changed_account_count;
        self.total_account_count += other.total_account_count;
        self.total_data_size += other.total_data_size;
        self.data_size_changed += other.data_size_changed;
        for (id, other) in &other.per_program_timings {
            let program_timing = self.per_program_timings.entry(*id).or_default();
            program_timing.accumulated_us = program_timing
                .accumulated_us
                .saturating_add(other.accumulated_us);
            program_timing.accumulated_units = program_timing
                .accumulated_units
                .saturating_add(other.accumulated_units);
            program_timing.count = program_timing.count.saturating_add(other.count);
        }
    }
    pub fn accumulate_program(&mut self, program_id: &Pubkey, us: u64, units: u64) {
        let program_timing = self.per_program_timings.entry(*program_id).or_default();
        program_timing.accumulated_us = program_timing.accumulated_us.saturating_add(us);
        program_timing.accumulated_units = program_timing.accumulated_units.saturating_add(units);
        program_timing.count = program_timing.count.saturating_add(1);
    }
}

// The relevant state of an account before an Instruction executes, used
// to verify account integrity after the Instruction completes
#[derive(Clone, Debug, Default)]
pub struct PreAccount {
    key: Pubkey,
    account: Rc<RefCell<AccountSharedData>>,
    changed: bool,
}
impl PreAccount {
    pub fn new(key: &Pubkey, account: &AccountSharedData) -> Self {
        Self {
            key: *key,
            account: Rc::new(RefCell::new(account.clone())),
            changed: false,
        }
    }

    pub fn verify(
        &self,
        program_id: &Pubkey,
        is_writable: bool,
        rent: &Rent,
        post: &AccountSharedData,
        timings: &mut ExecuteDetailsTimings,
        outermost_call: bool,
        updated_verify_policy: bool,
    ) -> Result<(), InstructionError> {
        let pre = self.account.borrow();

        // Only the owner of the account may change owner and
        //   only if the account is writable and
        //   only if the account is not executable and
        //   only if the data is zero-initialized or empty
        let owner_changed = pre.owner() != post.owner();
        if owner_changed
            && (!is_writable // line coverage used to get branch coverage
                || pre.executable()
                || program_id != pre.owner()
            || !Self::is_zeroed(post.data()))
        {
            return Err(InstructionError::ModifiedProgramId);
        }

        // An account not assigned to the program cannot have its balance decrease.
        if program_id != pre.owner() // line coverage used to get branch coverage
         && pre.lamports() > post.lamports()
        {
            return Err(InstructionError::ExternalAccountLamportSpend);
        }

        // The balance of read-only and executable accounts may not change
        let lamports_changed = pre.lamports() != post.lamports();
        if lamports_changed {
            if !is_writable {
                return Err(InstructionError::ReadonlyLamportChange);
            }
            if pre.executable() {
                return Err(InstructionError::ExecutableLamportChange);
            }
        }

        // Only the system program can change the size of the data
        //  and only if the system program owns the account
        let data_len_changed = pre.data().len() != post.data().len();
        if data_len_changed
            && (!system_program::check_id(program_id) // line coverage used to get branch coverage
                || !system_program::check_id(pre.owner()))
        {
            return Err(InstructionError::AccountDataSizeChanged);
        }

        // Only the owner may change account data
        //   and if the account is writable
        //   and if the account is not executable
        if !(program_id == pre.owner()
            && is_writable  // line coverage used to get branch coverage
            && !pre.executable())
            && pre.data() != post.data()
        {
            if pre.executable() {
                return Err(InstructionError::ExecutableDataModified);
            } else if is_writable {
                return Err(InstructionError::ExternalAccountDataModified);
            } else {
                return Err(InstructionError::ReadonlyDataModified);
            }
        }

        // executable is one-way (false->true) and only the account owner may set it.
        let executable_changed = pre.executable() != post.executable();
        if executable_changed {
            if !rent.is_exempt(post.lamports(), post.data().len()) {
                return Err(InstructionError::ExecutableAccountNotRentExempt);
            }
            let owner = if updated_verify_policy {
                post.owner()
            } else {
                pre.owner()
            };
            if !is_writable // line coverage used to get branch coverage
                || pre.executable()
                || program_id != owner
            {
                return Err(InstructionError::ExecutableModified);
            }
        }

        // No one modifies rent_epoch (yet).
        let rent_epoch_changed = pre.rent_epoch() != post.rent_epoch();
        if rent_epoch_changed {
            return Err(InstructionError::RentEpochModified);
        }

        if outermost_call {
            timings.total_account_count += 1;
            timings.total_data_size += post.data().len();
            if owner_changed
                || lamports_changed
                || data_len_changed
                || executable_changed
                || rent_epoch_changed
                || self.changed
            {
                timings.changed_account_count += 1;
                timings.data_size_changed += post.data().len();
            }
        }

        Ok(())
    }

    pub fn update(&mut self, account: &AccountSharedData) {
        let mut pre = self.account.borrow_mut();
        let rent_epoch = pre.rent_epoch();
        *pre = account.clone();
        pre.set_rent_epoch(rent_epoch);

        self.changed = true;
    }

    pub fn key(&self) -> &Pubkey {
        &self.key
    }

    pub fn lamports(&self) -> u64 {
        self.account.borrow().lamports()
    }

    pub fn executable(&self) -> bool {
        self.account.borrow().executable()
    }

    pub fn is_zeroed(buf: &[u8]) -> bool {
        const ZEROS_LEN: usize = 1024;
        static ZEROS: [u8; ZEROS_LEN] = [0; ZEROS_LEN];
        let mut chunks = buf.chunks_exact(ZEROS_LEN);

        chunks.all(|chunk| chunk == &ZEROS[..])
            && chunks.remainder() == &ZEROS[..chunks.remainder().len()]
    }
}

pub struct ThisComputeMeter {
    remaining: u64,
}
impl ComputeMeter for ThisComputeMeter {
    fn consume(&mut self, amount: u64) -> Result<(), InstructionError> {
        let exceeded = self.remaining < amount;
        self.remaining = self.remaining.saturating_sub(amount);
        if exceeded {
            return Err(InstructionError::ComputationalBudgetExceeded);
        }
        Ok(())
    }
    fn get_remaining(&self) -> u64 {
        self.remaining
    }
}
pub struct ThisInvokeContext<'a> {
    invoke_stack: Vec<InvokeContextStackFrame<'a>>,
    rent: Rent,
    pre_accounts: Vec<PreAccount>,
    accounts: &'a [(Pubkey, Rc<RefCell<AccountSharedData>>)],
    programs: &'a [(Pubkey, ProcessInstructionWithContext)],
    logger: Rc<RefCell<dyn Logger>>,
    compute_budget: ComputeBudget,
    #[allow(deprecated)]
    bpf_compute_budget: solana_sdk::process_instruction::BpfComputeBudget,
    compute_meter: Rc<RefCell<dyn ComputeMeter>>,
    executors: Rc<RefCell<Executors>>,
    instruction_recorder: Option<InstructionRecorder>,
    feature_set: Arc<FeatureSet>,
    pub timings: ExecuteDetailsTimings,
    account_db: Arc<Accounts>,
    ancestors: &'a Ancestors,
    #[allow(clippy::type_complexity)]
    sysvars: RefCell<Vec<(Pubkey, Option<Rc<Vec<u8>>>)>>,
}
impl<'a> ThisInvokeContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        program_id: &Pubkey,
        rent: Rent,
        message: &'a Message,
        instruction: &'a CompiledInstruction,
        executable_accounts: &'a [(Pubkey, Rc<RefCell<AccountSharedData>>)],
        accounts: &'a [(Pubkey, Rc<RefCell<AccountSharedData>>)],
        programs: &'a [(Pubkey, ProcessInstructionWithContext)],
        log_collector: Option<Rc<LogCollector>>,
        compute_budget: ComputeBudget,
        compute_meter: Rc<RefCell<dyn ComputeMeter>>,
        executors: Rc<RefCell<Executors>>,
        instruction_recorder: Option<InstructionRecorder>,
        feature_set: Arc<FeatureSet>,
        account_db: Arc<Accounts>,
        ancestors: &'a Ancestors,
    ) -> Self {
        let pre_accounts = MessageProcessor::create_pre_accounts(message, instruction, accounts);
        let keyed_accounts = MessageProcessor::create_keyed_accounts(
            message,
            instruction,
            executable_accounts,
            accounts,
        );
        let compute_meter = if feature_set.is_active(&tx_wide_compute_cap::id()) {
            compute_meter
        } else {
            Rc::new(RefCell::new(ThisComputeMeter {
                remaining: compute_budget.max_units,
            }))
        };
        let mut invoke_context = Self {
            invoke_stack: Vec::with_capacity(compute_budget.max_invoke_depth),
            rent,
            pre_accounts,
            accounts,
            programs,
            logger: Rc::new(RefCell::new(ThisLogger { log_collector })),
            compute_budget,
            bpf_compute_budget: compute_budget.into(),
            compute_meter,
            executors,
            instruction_recorder,
            feature_set,
            timings: ExecuteDetailsTimings::default(),
            account_db,
            ancestors,
            sysvars: RefCell::new(vec![]),
        };
        invoke_context
            .invoke_stack
            .push(InvokeContextStackFrame::new(
                *program_id,
                create_keyed_accounts_unified(&keyed_accounts),
            ));
        invoke_context
    }
}
impl<'a> InvokeContext for ThisInvokeContext<'a> {
    fn push(
        &mut self,
        key: &Pubkey,
        keyed_accounts: &[(bool, bool, &Pubkey, &RefCell<AccountSharedData>)],
    ) -> Result<(), InstructionError> {
        if self.invoke_stack.len() > self.compute_budget.max_invoke_depth {
            return Err(InstructionError::CallDepth);
        }

        let contains = self.invoke_stack.iter().any(|frame| frame.key == *key);
        let is_last = if let Some(last_frame) = self.invoke_stack.last() {
            last_frame.key == *key
        } else {
            false
        };
        if contains && !is_last {
            // Reentrancy not allowed unless caller is calling itself
            return Err(InstructionError::ReentrancyNotAllowed);
        }

        // Alias the keys and account references in the provided keyed_accounts
        // with the ones already existing in self, so that the lifetime 'a matches.
        fn transmute_lifetime<'a, 'b, T: Sized>(value: &'a T) -> &'b T {
            unsafe { std::mem::transmute(value) }
        }
        let keyed_accounts = keyed_accounts
            .iter()
            .map(|(is_signer, is_writable, search_key, account)| {
                self.accounts
                    .iter()
                    .position(|(key, _account)| key == *search_key)
                    .map(|index| {
                        // TODO
                        // Currently we are constructing new accounts on the stack
                        // before calling MessageProcessor::process_cross_program_instruction
                        // Ideally we would recycle the existing accounts here.
                        (
                            *is_signer,
                            *is_writable,
                            &self.accounts[index].0,
                            // &self.accounts[index] as &RefCell<AccountSharedData>
                            transmute_lifetime(*account),
                        )
                    })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(InstructionError::InvalidArgument)?;
        self.invoke_stack.push(InvokeContextStackFrame::new(
            *key,
            create_keyed_accounts_unified(keyed_accounts.as_slice()),
        ));
        Ok(())
    }
    fn pop(&mut self) {
        self.invoke_stack.pop();
    }
    fn invoke_depth(&self) -> usize {
        self.invoke_stack.len()
    }
    fn verify_and_update(
        &mut self,
        instruction: &CompiledInstruction,
        accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        write_privileges: &[bool],
    ) -> Result<(), InstructionError> {
        let stack_frame = self
            .invoke_stack
            .last()
            .ok_or(InstructionError::CallDepth)?;
        let logger = self.get_logger();
        MessageProcessor::verify_and_update(
            instruction,
            &mut self.pre_accounts,
            accounts,
            &stack_frame.key,
            &self.rent,
            write_privileges,
            &mut self.timings,
            logger,
            self.feature_set.is_active(&updated_verify_policy::id()),
        )
    }
    fn get_caller(&self) -> Result<&Pubkey, InstructionError> {
        self.invoke_stack
            .last()
            .map(|frame| &frame.key)
            .ok_or(InstructionError::CallDepth)
    }
    fn remove_first_keyed_account(&mut self) -> Result<(), InstructionError> {
        let stack_frame = &mut self
            .invoke_stack
            .last_mut()
            .ok_or(InstructionError::CallDepth)?;
        stack_frame.keyed_accounts_range.start =
            stack_frame.keyed_accounts_range.start.saturating_add(1);
        Ok(())
    }
    fn get_keyed_accounts(&self) -> Result<&[KeyedAccount], InstructionError> {
        self.invoke_stack
            .last()
            .map(|frame| &frame.keyed_accounts[frame.keyed_accounts_range.clone()])
            .ok_or(InstructionError::CallDepth)
    }
    fn get_programs(&self) -> &[(Pubkey, ProcessInstructionWithContext)] {
        self.programs
    }
    fn get_logger(&self) -> Rc<RefCell<dyn Logger>> {
        self.logger.clone()
    }
    #[allow(deprecated)]
    fn get_bpf_compute_budget(&self) -> &solana_sdk::process_instruction::BpfComputeBudget {
        &self.bpf_compute_budget
    }
    fn get_compute_meter(&self) -> Rc<RefCell<dyn ComputeMeter>> {
        self.compute_meter.clone()
    }
    fn add_executor(&self, pubkey: &Pubkey, executor: Arc<dyn Executor>) {
        self.executors.borrow_mut().insert(*pubkey, executor);
    }
    fn get_executor(&self, pubkey: &Pubkey) -> Option<Arc<dyn Executor>> {
        self.executors.borrow().get(pubkey)
    }
    fn record_instruction(&self, instruction: &Instruction) {
        if let Some(recorder) = &self.instruction_recorder {
            recorder.record_instruction(instruction.clone());
        }
    }
    fn is_feature_active(&self, feature_id: &Pubkey) -> bool {
        self.feature_set.is_active(feature_id)
    }
    fn get_account(&self, pubkey: &Pubkey) -> Option<Rc<RefCell<AccountSharedData>>> {
        self.accounts.iter().find_map(|(key, account)| {
            if key == pubkey {
                Some(account.clone())
            } else {
                None
            }
        })
    }
    fn update_timing(
        &mut self,
        serialize_us: u64,
        create_vm_us: u64,
        execute_us: u64,
        deserialize_us: u64,
    ) {
        self.timings.serialize_us += serialize_us;
        self.timings.create_vm_us += create_vm_us;
        self.timings.execute_us += execute_us;
        self.timings.deserialize_us += deserialize_us;
    }
    fn get_sysvar_data(&self, id: &Pubkey) -> Option<Rc<Vec<u8>>> {
        if let Ok(mut sysvars) = self.sysvars.try_borrow_mut() {
            // Try share from cache
            let mut result = sysvars
                .iter()
                .find_map(|(key, sysvar)| if id == key { sysvar.clone() } else { None });
            if result.is_none() {
                // Load it
                result = self
                    .account_db
                    .load_with_fixed_root(self.ancestors, id)
                    .map(|(account, _)| Rc::new(account.data().to_vec()));
                // Cache it
                sysvars.push((*id, result.clone()));
            }
            result
        } else {
            None
        }
    }
    fn get_compute_budget(&self) -> &ComputeBudget {
        &self.compute_budget
    }
}
pub struct ThisLogger {
    log_collector: Option<Rc<LogCollector>>,
}
impl Logger for ThisLogger {
    fn log_enabled(&self) -> bool {
        log_enabled!(log::Level::Info) || self.log_collector.is_some()
    }
    fn log(&self, message: &str) {
        debug!("{}", message);
        if let Some(log_collector) = &self.log_collector {
            log_collector.log(message);
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct MessageProcessor {
    #[serde(skip)]
    programs: Vec<(Pubkey, ProcessInstructionWithContext)>,
    #[serde(skip)]
    native_loader: NativeLoader,
}

impl std::fmt::Debug for MessageProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        #[derive(Debug)]
        struct MessageProcessor<'a> {
            programs: Vec<String>,
            native_loader: &'a NativeLoader,
        }

        // These are just type aliases for work around of Debug-ing above pointers
        type ErasedProcessInstructionWithContext = fn(
            &'static Pubkey,
            &'static [u8],
            &'static mut dyn InvokeContext,
        ) -> Result<(), InstructionError>;

        // rustc doesn't compile due to bug without this work around
        // https://github.com/rust-lang/rust/issues/50280
        // https://users.rust-lang.org/t/display-function-pointer/17073/2
        let processor = MessageProcessor {
            programs: self
                .programs
                .iter()
                .map(|(pubkey, instruction)| {
                    let erased_instruction: ErasedProcessInstructionWithContext = *instruction;
                    format!("{}: {:p}", pubkey, erased_instruction)
                })
                .collect::<Vec<_>>(),
            native_loader: &self.native_loader,
        };

        write!(f, "{:?}", processor)
    }
}

impl Default for MessageProcessor {
    fn default() -> Self {
        Self {
            programs: vec![],
            native_loader: NativeLoader::default(),
        }
    }
}
impl Clone for MessageProcessor {
    fn clone(&self) -> Self {
        MessageProcessor {
            programs: self.programs.clone(),
            native_loader: NativeLoader::default(),
        }
    }
}

#[cfg(RUSTC_WITH_SPECIALIZATION)]
impl ::solana_frozen_abi::abi_example::AbiExample for MessageProcessor {
    fn example() -> Self {
        // MessageProcessor's fields are #[serde(skip)]-ed and not Serialize
        // so, just rely on Default anyway.
        MessageProcessor::default()
    }
}

impl MessageProcessor {
    /// Add a static entrypoint to intercept instructions before the dynamic loader.
    pub fn add_program(
        &mut self,
        program_id: Pubkey,
        process_instruction: ProcessInstructionWithContext,
    ) {
        match self.programs.iter_mut().find(|(key, _)| program_id == *key) {
            Some((_, processor)) => *processor = process_instruction,
            None => self.programs.push((program_id, process_instruction)),
        }
    }

    pub fn add_loader(
        &mut self,
        program_id: Pubkey,
        process_instruction: ProcessInstructionWithContext,
    ) {
        self.add_program(program_id, process_instruction);
    }

    /// Create the KeyedAccounts that will be passed to the program
    fn create_keyed_accounts<'a>(
        message: &'a Message,
        instruction: &'a CompiledInstruction,
        executable_accounts: &'a [(Pubkey, Rc<RefCell<AccountSharedData>>)],
        accounts: &'a [(Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Vec<(bool, bool, &'a Pubkey, &'a RefCell<AccountSharedData>)> {
        executable_accounts
            .iter()
            .map(|(key, account)| (false, false, key, account as &RefCell<AccountSharedData>))
            .chain(instruction.accounts.iter().map(|index| {
                let index = *index as usize;
                (
                    message.is_signer(index),
                    message.is_writable(index),
                    &accounts[index].0,
                    &accounts[index].1 as &RefCell<AccountSharedData>,
                )
            }))
            .collect::<Vec<_>>()
    }

    /// Process an instruction
    /// This method calls the instruction's program entrypoint method
    fn process_instruction(
        &self,
        program_id: &Pubkey,
        instruction_data: &[u8],
        invoke_context: &mut dyn InvokeContext,
    ) -> Result<(), InstructionError> {
        if let Some(root_account) = invoke_context.get_keyed_accounts()?.iter().next() {
            let root_id = root_account.unsigned_key();
            if native_loader::check_id(&root_account.owner()?) {
                for (id, process_instruction) in &self.programs {
                    if id == root_id {
                        invoke_context.remove_first_keyed_account()?;
                        // Call the builtin program
                        return process_instruction(program_id, instruction_data, invoke_context);
                    }
                }
                // Call the program via the native loader
                return self.native_loader.process_instruction(
                    &native_loader::id(),
                    instruction_data,
                    invoke_context,
                );
            } else {
                let owner_id = &root_account.owner()?;
                for (id, process_instruction) in &self.programs {
                    if id == owner_id {
                        // Call the program via a builtin loader
                        return process_instruction(program_id, instruction_data, invoke_context);
                    }
                }
            }
        }
        Err(InstructionError::UnsupportedProgramId)
    }

    pub fn create_message(
        instruction: &Instruction,
        keyed_accounts: &[&KeyedAccount],
        signers: &[Pubkey],
        invoke_context: &Ref<&mut dyn InvokeContext>,
    ) -> Result<(Message, Pubkey, usize), InstructionError> {
        // Check for privilege escalation
        for account in instruction.accounts.iter() {
            let keyed_account = keyed_accounts
                .iter()
                .find_map(|keyed_account| {
                    if &account.pubkey == keyed_account.unsigned_key() {
                        Some(keyed_account)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    ic_msg!(
                        invoke_context,
                        "Instruction references an unknown account {}",
                        account.pubkey
                    );
                    InstructionError::MissingAccount
                })?;
            // Readonly account cannot become writable
            if account.is_writable && !keyed_account.is_writable() {
                ic_msg!(
                    invoke_context,
                    "{}'s writable privilege escalated",
                    account.pubkey
                );
                return Err(InstructionError::PrivilegeEscalation);
            }

            if account.is_signer && // If message indicates account is signed
            !( // one of the following needs to be true:
                keyed_account.signer_key().is_some() // Signed in the parent instruction
                || signers.contains(&account.pubkey) // Signed by the program
            ) {
                ic_msg!(
                    invoke_context,
                    "{}'s signer privilege escalated",
                    account.pubkey
                );
                return Err(InstructionError::PrivilegeEscalation);
            }
        }

        // validate the caller has access to the program account and that it is executable
        let program_id = instruction.program_id;
        match keyed_accounts
            .iter()
            .find(|keyed_account| &program_id == keyed_account.unsigned_key())
        {
            Some(keyed_account) => {
                if !keyed_account.executable()? {
                    ic_msg!(
                        invoke_context,
                        "Account {} is not executable",
                        keyed_account.unsigned_key()
                    );
                    return Err(InstructionError::AccountNotExecutable);
                }
            }
            None => {
                ic_msg!(invoke_context, "Unknown program {}", program_id);
                return Err(InstructionError::MissingAccount);
            }
        }

        let message = Message::new(&[instruction.clone()], None);
        let program_id_index = message.instructions[0].program_id_index as usize;

        Ok((message, program_id, program_id_index))
    }

    /// Entrypoint for a cross-program invocation from a native program
    pub fn native_invoke(
        invoke_context: &mut dyn InvokeContext,
        instruction: Instruction,
        keyed_account_indices: &[usize],
        signers: &[Pubkey],
    ) -> Result<(), InstructionError> {
        let invoke_context = RefCell::new(invoke_context);

        let (
            message,
            executable_accounts,
            accounts,
            keyed_account_indices_reordered,
            caller_write_privileges,
        ) = {
            let invoke_context = invoke_context.borrow();

            // Translate and verify caller's data
            let keyed_accounts = invoke_context.get_keyed_accounts()?;
            let keyed_accounts = keyed_account_indices
                .iter()
                .map(|index| keyed_account_at_index(keyed_accounts, *index))
                .collect::<Result<Vec<&KeyedAccount>, InstructionError>>()?;
            let (message, callee_program_id, _) =
                Self::create_message(&instruction, &keyed_accounts, signers, &invoke_context)?;
            let keyed_accounts = invoke_context.get_keyed_accounts()?;
            let mut caller_write_privileges = keyed_account_indices
                .iter()
                .map(|index| keyed_accounts[*index].is_writable())
                .collect::<Vec<bool>>();
            caller_write_privileges.insert(0, false);
            let mut accounts = vec![];
            let mut keyed_account_indices_reordered = vec![];
            let keyed_accounts = invoke_context.get_keyed_accounts()?;
            'root: for account_key in message.account_keys.iter() {
                for keyed_account_index in keyed_account_indices {
                    let keyed_account = &keyed_accounts[*keyed_account_index];
                    if account_key == keyed_account.unsigned_key() {
                        accounts.push((*account_key, Rc::new(keyed_account.account.clone())));
                        keyed_account_indices_reordered.push(*keyed_account_index);
                        continue 'root;
                    }
                }
                ic_msg!(
                    invoke_context,
                    "Instruction references an unknown account {}",
                    account_key
                );
                return Err(InstructionError::MissingAccount);
            }

            // Process instruction

            invoke_context.record_instruction(&instruction);

            let program_account =
                invoke_context
                    .get_account(&callee_program_id)
                    .ok_or_else(|| {
                        ic_msg!(invoke_context, "Unknown program {}", callee_program_id);
                        InstructionError::MissingAccount
                    })?;
            if !program_account.borrow().executable() {
                ic_msg!(
                    invoke_context,
                    "Account {} is not executable",
                    callee_program_id
                );
                return Err(InstructionError::AccountNotExecutable);
            }
            let programdata = if program_account.borrow().owner() == &bpf_loader_upgradeable::id() {
                if let UpgradeableLoaderState::Program {
                    programdata_address,
                } = program_account.borrow().state()?
                {
                    if let Some(account) = invoke_context.get_account(&programdata_address) {
                        Some((programdata_address, account))
                    } else {
                        ic_msg!(
                            invoke_context,
                            "Unknown upgradeable programdata account {}",
                            programdata_address,
                        );
                        return Err(InstructionError::MissingAccount);
                    }
                } else {
                    ic_msg!(
                        invoke_context,
                        "Upgradeable program account state not valid {}",
                        callee_program_id,
                    );
                    return Err(InstructionError::MissingAccount);
                }
            } else {
                None
            };
            let mut executable_accounts = vec![(callee_program_id, program_account)];
            if let Some(programdata) = programdata {
                executable_accounts.push(programdata);
            }
            (
                message,
                executable_accounts,
                accounts,
                keyed_account_indices_reordered,
                caller_write_privileges,
            )
        };

        #[allow(clippy::deref_addrof)]
        MessageProcessor::process_cross_program_instruction(
            &message,
            &executable_accounts,
            &accounts,
            &caller_write_privileges,
            *(&mut *(invoke_context.borrow_mut())),
        )?;

        // Copy results back to caller

        {
            let invoke_context = invoke_context.borrow();
            let keyed_accounts = invoke_context.get_keyed_accounts()?;
            for (src_keyed_account_index, ((_key, account), dst_keyed_account_index)) in accounts
                .iter()
                .zip(keyed_account_indices_reordered)
                .enumerate()
            {
                let dst_keyed_account = &keyed_accounts[dst_keyed_account_index];
                let src_keyed_account = account.borrow();
                if message.is_writable(src_keyed_account_index) && !src_keyed_account.executable() {
                    if dst_keyed_account.data_len()? != src_keyed_account.data().len()
                        && dst_keyed_account.data_len()? != 0
                    {
                        // Only support for `CreateAccount` at this time.
                        // Need a way to limit total realloc size across multiple CPI calls
                        ic_msg!(
                            invoke_context,
                            "Inner instructions do not support realloc, only SystemProgram::CreateAccount",
                        );
                        return Err(InstructionError::InvalidRealloc);
                    }
                    dst_keyed_account
                        .try_account_ref_mut()?
                        .set_lamports(src_keyed_account.lamports());
                    dst_keyed_account
                        .try_account_ref_mut()?
                        .set_owner(*src_keyed_account.owner());
                    dst_keyed_account
                        .try_account_ref_mut()?
                        .set_data(src_keyed_account.data().to_vec());
                }
            }
        }

        Ok(())
    }

    /// Process a cross-program instruction
    /// This method calls the instruction's program entrypoint function
    pub fn process_cross_program_instruction(
        message: &Message,
        executable_accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        caller_write_privileges: &[bool],
        invoke_context: &mut dyn InvokeContext,
    ) -> Result<(), InstructionError> {
        if let Some(instruction) = message.instructions.get(0) {
            let program_id = instruction.program_id(&message.account_keys);

            // Verify the calling program hasn't misbehaved
            invoke_context.verify_and_update(instruction, accounts, caller_write_privileges)?;

            // Construct keyed accounts
            let keyed_accounts =
                Self::create_keyed_accounts(message, instruction, executable_accounts, accounts);

            // Invoke callee
            invoke_context.push(program_id, &keyed_accounts)?;

            let mut message_processor = MessageProcessor::default();
            for (program_id, process_instruction) in invoke_context.get_programs().iter() {
                message_processor.add_program(*program_id, *process_instruction);
            }

            let mut result = message_processor.process_instruction(
                program_id,
                &instruction.data,
                invoke_context,
            );
            if result.is_ok() {
                // Verify the called program has not misbehaved
                let write_privileges: Vec<bool> = (0..message.account_keys.len())
                    .map(|i| message.is_writable(i))
                    .collect();
                result = invoke_context.verify_and_update(instruction, accounts, &write_privileges);
            }

            // Restore previous state
            invoke_context.pop();
            result
        } else {
            // This function is always called with a valid instruction, if that changes return an error
            Err(InstructionError::GenericError)
        }
    }

    /// Record the initial state of the accounts so that they can be compared
    /// after the instruction is processed
    pub fn create_pre_accounts(
        message: &Message,
        instruction: &CompiledInstruction,
        accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Vec<PreAccount> {
        let mut pre_accounts = Vec::with_capacity(instruction.accounts.len());
        {
            let mut work = |_unique_index: usize, account_index: usize| {
                if account_index < message.account_keys.len() && account_index < accounts.len() {
                    let account = accounts[account_index].1.borrow();
                    pre_accounts.push(PreAccount::new(&accounts[account_index].0, &account));
                    return Ok(());
                }
                Err(InstructionError::MissingAccount)
            };
            let _ = instruction.visit_each_account(&mut work);
        }
        pre_accounts
    }

    /// Verify there are no outstanding borrows
    pub fn verify_account_references(
        accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
    ) -> Result<(), InstructionError> {
        for (_, account) in accounts.iter() {
            account
                .try_borrow_mut()
                .map_err(|_| InstructionError::AccountBorrowOutstanding)?;
        }
        Ok(())
    }

    /// Verify the results of an instruction
    pub fn verify(
        message: &Message,
        instruction: &CompiledInstruction,
        pre_accounts: &[PreAccount],
        executable_accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        rent: &Rent,
        timings: &mut ExecuteDetailsTimings,
        logger: Rc<RefCell<dyn Logger>>,
        updated_verify_policy: bool,
    ) -> Result<(), InstructionError> {
        // Verify all executable accounts have zero outstanding refs
        Self::verify_account_references(executable_accounts)?;

        // Verify the per-account instruction results
        let (mut pre_sum, mut post_sum) = (0_u128, 0_u128);
        {
            let program_id = instruction.program_id(&message.account_keys);
            let mut work = |unique_index: usize, account_index: usize| {
                {
                    // Verify account has no outstanding references
                    let _ = accounts[account_index]
                        .1
                        .try_borrow_mut()
                        .map_err(|_| InstructionError::AccountBorrowOutstanding)?;
                }
                let account = accounts[account_index].1.borrow();
                pre_accounts[unique_index]
                    .verify(
                        program_id,
                        message.is_writable(account_index),
                        rent,
                        &account,
                        timings,
                        true,
                        updated_verify_policy,
                    )
                    .map_err(|err| {
                        ic_logger_msg!(
                            logger,
                            "failed to verify account {}: {}",
                            pre_accounts[unique_index].key,
                            err
                        );
                        err
                    })?;
                pre_sum += u128::from(pre_accounts[unique_index].lamports());
                post_sum += u128::from(account.lamports());
                Ok(())
            };
            instruction.visit_each_account(&mut work)?;
        }

        // Verify that the total sum of all the lamports did not change
        if pre_sum != post_sum {
            return Err(InstructionError::UnbalancedInstruction);
        }
        Ok(())
    }

    /// Verify the results of a cross-program instruction
    #[allow(clippy::too_many_arguments)]
    fn verify_and_update(
        instruction: &CompiledInstruction,
        pre_accounts: &mut [PreAccount],
        accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        program_id: &Pubkey,
        rent: &Rent,
        write_privileges: &[bool],
        timings: &mut ExecuteDetailsTimings,
        logger: Rc<RefCell<dyn Logger>>,
        updated_verify_policy: bool,
    ) -> Result<(), InstructionError> {
        // Verify the per-account instruction results
        let (mut pre_sum, mut post_sum) = (0_u128, 0_u128);
        let mut work = |_unique_index: usize, account_index: usize| {
            if account_index < write_privileges.len() && account_index < accounts.len() {
                let (key, account) = &accounts[account_index];
                let is_writable = write_privileges[account_index];
                // Find the matching PreAccount
                for pre_account in pre_accounts.iter_mut() {
                    if key == pre_account.key() {
                        {
                            // Verify account has no outstanding references
                            let _ = account
                                .try_borrow_mut()
                                .map_err(|_| InstructionError::AccountBorrowOutstanding)?;
                        }
                        let account = account.borrow();
                        pre_account
                            .verify(
                                program_id,
                                is_writable,
                                rent,
                                &account,
                                timings,
                                false,
                                updated_verify_policy,
                            )
                            .map_err(|err| {
                                ic_logger_msg!(logger, "failed to verify account {}: {}", key, err);
                                err
                            })?;
                        pre_sum += u128::from(pre_account.lamports());
                        post_sum += u128::from(account.lamports());
                        if is_writable && !pre_account.executable() {
                            pre_account.update(&account);
                        }
                        return Ok(());
                    }
                }
            }
            Err(InstructionError::MissingAccount)
        };
        instruction.visit_each_account(&mut work)?;

        // Verify that the total sum of all the lamports did not change
        if pre_sum != post_sum {
            return Err(InstructionError::UnbalancedInstruction);
        }
        Ok(())
    }

    /// Execute an instruction
    /// This method calls the instruction's program entrypoint method and verifies that the result of
    /// the call does not violate the bank's accounting rules.
    /// The accounts are committed back to the bank only if this function returns Ok(_).
    #[allow(clippy::too_many_arguments)]
    fn execute_instruction(
        &self,
        message: &Message,
        instruction: &CompiledInstruction,
        executable_accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        rent_collector: &RentCollector,
        log_collector: Option<Rc<LogCollector>>,
        executors: Rc<RefCell<Executors>>,
        instruction_recorder: Option<InstructionRecorder>,
        instruction_index: usize,
        feature_set: Arc<FeatureSet>,
        compute_budget: ComputeBudget,
        compute_meter: Rc<RefCell<dyn ComputeMeter>>,
        timings: &mut ExecuteDetailsTimings,
        account_db: Arc<Accounts>,
        ancestors: &Ancestors,
    ) -> Result<(), InstructionError> {
        // Fixup the special instructions key if present
        // before the account pre-values are taken care of
        if feature_set.is_active(&instructions_sysvar_enabled::id()) {
            for (pubkey, accont) in accounts.iter().take(message.account_keys.len()) {
                if instructions::check_id(pubkey) {
                    let mut mut_account_ref = accont.borrow_mut();
                    instructions::store_current_index(
                        mut_account_ref.data_as_mut_slice(),
                        instruction_index as u16,
                    );
                    break;
                }
            }
        }

        let program_id = instruction.program_id(&message.account_keys);

        let mut compute_budget = compute_budget;
        if feature_set.is_active(&neon_evm_compute_budget::id())
            && *program_id == crate::neon_evm_program::id()
        {
            // Bump the compute budget for neon_evm
            compute_budget.max_units = compute_budget.max_units.max(500_000);
            compute_budget.heap_size = Some(256 * 1024);
        }

        let mut invoke_context = ThisInvokeContext::new(
            program_id,
            rent_collector.rent,
            message,
            instruction,
            executable_accounts,
            accounts,
            &self.programs,
            log_collector,
            compute_budget,
            compute_meter,
            executors,
            instruction_recorder,
            feature_set,
            account_db,
            ancestors,
        );
        self.process_instruction(program_id, &instruction.data, &mut invoke_context)?;
        Self::verify(
            message,
            instruction,
            &invoke_context.pre_accounts,
            executable_accounts,
            accounts,
            &rent_collector.rent,
            timings,
            invoke_context.get_logger(),
            invoke_context.is_feature_active(&updated_verify_policy::id()),
        )?;

        timings.accumulate(&invoke_context.timings);

        Ok(())
    }

    /// Process a message.
    /// This method calls each instruction in the message over the set of loaded Accounts
    /// The accounts are committed back to the bank only if every instruction succeeds
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    pub fn process_message(
        &self,
        message: &Message,
        loaders: &[Vec<(Pubkey, Rc<RefCell<AccountSharedData>>)>],
        accounts: &[(Pubkey, Rc<RefCell<AccountSharedData>>)],
        rent_collector: &RentCollector,
        log_collector: Option<Rc<LogCollector>>,
        executors: Rc<RefCell<Executors>>,
        instruction_recorders: Option<&[InstructionRecorder]>,
        feature_set: Arc<FeatureSet>,
        compute_budget: ComputeBudget,
        compute_meter: Rc<RefCell<dyn ComputeMeter>>,
        timings: &mut ExecuteDetailsTimings,
        account_db: Arc<Accounts>,
        ancestors: &Ancestors,
    ) -> Result<(), TransactionError> {
        for (instruction_index, instruction) in message.instructions.iter().enumerate() {
            let mut time = Measure::start("execute_instruction");
            let pre_remaining_units = compute_meter.borrow().get_remaining();
            let instruction_recorder = instruction_recorders
                .as_ref()
                .map(|recorders| recorders[instruction_index].clone());
            let err = self
                .execute_instruction(
                    message,
                    instruction,
                    &loaders[instruction_index],
                    accounts,
                    rent_collector,
                    log_collector.clone(),
                    executors.clone(),
                    instruction_recorder,
                    instruction_index,
                    feature_set.clone(),
                    compute_budget,
                    compute_meter.clone(),
                    timings,
                    account_db.clone(),
                    ancestors,
                )
                .map_err(|err| TransactionError::InstructionError(instruction_index as u8, err));
            time.stop();
            let post_remaining_units = compute_meter.borrow().get_remaining();

            timings.accumulate_program(
                instruction.program_id(&message.account_keys),
                time.as_us(),
                pre_remaining_units - post_remaining_units,
            );

            err?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        account::Account,
        instruction::{AccountMeta, Instruction, InstructionError},
        message::Message,
        native_loader::create_loadable_account_for_test,
        process_instruction::MockComputeMeter,
    };

    #[test]
    fn test_invoke_context() {
        const MAX_DEPTH: usize = 10;
        let mut invoke_stack = vec![];
        let mut accounts = vec![];
        let mut metas = vec![];
        for i in 0..MAX_DEPTH {
            invoke_stack.push(solana_sdk::pubkey::new_rand());
            accounts.push((
                solana_sdk::pubkey::new_rand(),
                Rc::new(RefCell::new(AccountSharedData::new(
                    i as u64,
                    1,
                    &invoke_stack[i],
                ))),
            ));
            metas.push(AccountMeta::new(accounts[i].0, false));
        }
        for program_id in invoke_stack.iter() {
            accounts.push((
                *program_id,
                Rc::new(RefCell::new(AccountSharedData::new(
                    1,
                    1,
                    &solana_sdk::pubkey::Pubkey::default(),
                ))),
            ));
            metas.push(AccountMeta::new(*program_id, false));
        }

        let message = Message::new(
            &[Instruction::new_with_bytes(invoke_stack[0], &[0], metas)],
            None,
        );
        let ancestors = Ancestors::default();
        let mut invoke_context = ThisInvokeContext::new(
            &invoke_stack[0],
            Rent::default(),
            &message,
            &message.instructions[0],
            &[],
            &accounts,
            &[],
            None,
            ComputeBudget::default(),
            Rc::new(RefCell::new(MockComputeMeter::default())),
            Rc::new(RefCell::new(Executors::default())),
            None,
            Arc::new(FeatureSet::all_enabled()),
            Arc::new(Accounts::default()),
            &ancestors,
        );

        // Check call depth increases and has a limit
        let mut depth_reached = 1;
        for program_id in invoke_stack.iter().skip(1) {
            if Err(InstructionError::CallDepth) == invoke_context.push(program_id, &[]) {
                break;
            }
            depth_reached += 1;
        }
        assert_ne!(depth_reached, 0);
        assert!(depth_reached < MAX_DEPTH);

        // Mock each invocation
        for owned_index in (1..depth_reached).rev() {
            let not_owned_index = owned_index - 1;
            let metas = vec![
                AccountMeta::new(accounts[not_owned_index].0, false),
                AccountMeta::new(accounts[owned_index].0, false),
            ];
            let message = Message::new(
                &[Instruction::new_with_bytes(
                    invoke_stack[owned_index],
                    &[0],
                    metas,
                )],
                None,
            );

            // modify account owned by the program
            accounts[owned_index].1.borrow_mut().data_as_mut_slice()[0] =
                (MAX_DEPTH + owned_index) as u8;
            let mut these_accounts = accounts[not_owned_index..owned_index + 1].to_vec();
            these_accounts.push((
                message.account_keys[2],
                Rc::new(RefCell::new(AccountSharedData::new(
                    1,
                    1,
                    &solana_sdk::pubkey::Pubkey::default(),
                ))),
            ));
            let write_privileges: Vec<bool> = (0..message.account_keys.len())
                .map(|i| message.is_writable(i))
                .collect();
            invoke_context
                .verify_and_update(&message.instructions[0], &these_accounts, &write_privileges)
                .unwrap();
            assert_eq!(
                invoke_context.pre_accounts[owned_index]
                    .account
                    .borrow()
                    .data()[0],
                (MAX_DEPTH + owned_index) as u8
            );

            // modify account not owned by the program
            let data = accounts[not_owned_index].1.borrow_mut().data()[0];
            accounts[not_owned_index].1.borrow_mut().data_as_mut_slice()[0] =
                (MAX_DEPTH + not_owned_index) as u8;
            assert_eq!(
                invoke_context.verify_and_update(
                    &message.instructions[0],
                    &accounts[not_owned_index..owned_index + 1],
                    &write_privileges,
                ),
                Err(InstructionError::ExternalAccountDataModified)
            );
            assert_eq!(
                invoke_context.pre_accounts[not_owned_index]
                    .account
                    .borrow()
                    .data()[0],
                data
            );
            accounts[not_owned_index].1.borrow_mut().data_as_mut_slice()[0] = data;

            invoke_context.pop();
        }
    }

    #[test]
    fn test_is_zeroed() {
        const ZEROS_LEN: usize = 1024;
        let mut buf = [0; ZEROS_LEN];
        assert!(PreAccount::is_zeroed(&buf));
        buf[0] = 1;
        assert!(!PreAccount::is_zeroed(&buf));

        let mut buf = [0; ZEROS_LEN - 1];
        assert!(PreAccount::is_zeroed(&buf));
        buf[0] = 1;
        assert!(!PreAccount::is_zeroed(&buf));

        let mut buf = [0; ZEROS_LEN + 1];
        assert!(PreAccount::is_zeroed(&buf));
        buf[0] = 1;
        assert!(!PreAccount::is_zeroed(&buf));

        let buf = vec![];
        assert!(PreAccount::is_zeroed(&buf));
    }

    #[test]
    fn test_verify_account_references() {
        let accounts = vec![(
            solana_sdk::pubkey::new_rand(),
            Rc::new(RefCell::new(AccountSharedData::default())),
        )];

        assert!(MessageProcessor::verify_account_references(&accounts).is_ok());

        let mut _borrowed = accounts[0].1.borrow();
        assert_eq!(
            MessageProcessor::verify_account_references(&accounts),
            Err(InstructionError::AccountBorrowOutstanding)
        );
    }

    struct Change {
        program_id: Pubkey,
        is_writable: bool,
        rent: Rent,
        pre: PreAccount,
        post: AccountSharedData,
    }
    impl Change {
        pub fn new(owner: &Pubkey, program_id: &Pubkey) -> Self {
            Self {
                program_id: *program_id,
                rent: Rent::default(),
                is_writable: true,
                pre: PreAccount::new(
                    &solana_sdk::pubkey::new_rand(),
                    &AccountSharedData::from(Account {
                        owner: *owner,
                        lamports: std::u64::MAX,
                        data: vec![],
                        ..Account::default()
                    }),
                ),
                post: AccountSharedData::from(Account {
                    owner: *owner,
                    lamports: std::u64::MAX,
                    ..Account::default()
                }),
            }
        }
        pub fn read_only(mut self) -> Self {
            self.is_writable = false;
            self
        }
        pub fn executable(mut self, pre: bool, post: bool) -> Self {
            self.pre.account.borrow_mut().set_executable(pre);
            self.post.set_executable(post);
            self
        }
        pub fn lamports(mut self, pre: u64, post: u64) -> Self {
            self.pre.account.borrow_mut().set_lamports(pre);
            self.post.set_lamports(post);
            self
        }
        pub fn owner(mut self, post: &Pubkey) -> Self {
            self.post.set_owner(*post);
            self
        }
        pub fn data(mut self, pre: Vec<u8>, post: Vec<u8>) -> Self {
            self.pre.account.borrow_mut().set_data(pre);
            self.post.set_data(post);
            self
        }
        pub fn rent_epoch(mut self, pre: u64, post: u64) -> Self {
            self.pre.account.borrow_mut().set_rent_epoch(pre);
            self.post.set_rent_epoch(post);
            self
        }
        pub fn verify(&self) -> Result<(), InstructionError> {
            self.pre.verify(
                &self.program_id,
                self.is_writable,
                &self.rent,
                &self.post,
                &mut ExecuteDetailsTimings::default(),
                false,
                true,
            )
        }
    }

    #[test]
    fn test_verify_account_changes_owner() {
        let system_program_id = system_program::id();
        let alice_program_id = solana_sdk::pubkey::new_rand();
        let mallory_program_id = solana_sdk::pubkey::new_rand();

        assert_eq!(
            Change::new(&system_program_id, &system_program_id)
                .owner(&alice_program_id)
                .verify(),
            Ok(()),
            "system program should be able to change the account owner"
        );
        assert_eq!(
            Change::new(&system_program_id, &system_program_id)
                .owner(&alice_program_id)
                .read_only()
                .verify(),
            Err(InstructionError::ModifiedProgramId),
            "system program should not be able to change the account owner of a read-only account"
        );
        assert_eq!(
            Change::new(&mallory_program_id, &system_program_id)
                .owner(&alice_program_id)
                .verify(),
            Err(InstructionError::ModifiedProgramId),
            "system program should not be able to change the account owner of a non-system account"
        );
        assert_eq!(
            Change::new(&mallory_program_id, &mallory_program_id)
                .owner(&alice_program_id)
                .verify(),
            Ok(()),
            "mallory should be able to change the account owner, if she leaves clear data"
        );
        assert_eq!(
            Change::new(&mallory_program_id, &mallory_program_id)
                .owner(&alice_program_id)
                .data(vec![42], vec![0])
                .verify(),
            Ok(()),
            "mallory should be able to change the account owner, if she leaves clear data"
        );
        assert_eq!(
            Change::new(&mallory_program_id, &mallory_program_id)
                .owner(&alice_program_id)
                .executable(true, true)
                .data(vec![42], vec![0])
                .verify(),
            Err(InstructionError::ModifiedProgramId),
            "mallory should not be able to change the account owner, if the account executable"
        );
        assert_eq!(
            Change::new(&mallory_program_id, &mallory_program_id)
                .owner(&alice_program_id)
                .data(vec![42], vec![42])
                .verify(),
            Err(InstructionError::ModifiedProgramId),
            "mallory should not be able to inject data into the alice program"
        );
    }

    #[test]
    fn test_verify_account_changes_executable() {
        let owner = solana_sdk::pubkey::new_rand();
        let mallory_program_id = solana_sdk::pubkey::new_rand();
        let system_program_id = system_program::id();

        assert_eq!(
            Change::new(&owner, &system_program_id)
                .executable(false, true)
                .verify(),
            Err(InstructionError::ExecutableModified),
            "system program can't change executable if system doesn't own the account"
        );
        assert_eq!(
            Change::new(&owner, &system_program_id)
                .executable(true, true)
                .data(vec![1], vec![2])
                .verify(),
            Err(InstructionError::ExecutableDataModified),
            "system program can't change executable data if system doesn't own the account"
        );
        assert_eq!(
            Change::new(&owner, &owner).executable(false, true).verify(),
            Ok(()),
            "owner should be able to change executable"
        );
        assert_eq!(
            Change::new(&owner, &owner)
                .executable(false, true)
                .read_only()
                .verify(),
            Err(InstructionError::ExecutableModified),
            "owner can't modify executable of read-only accounts"
        );
        assert_eq!(
            Change::new(&owner, &owner).executable(true, false).verify(),
            Err(InstructionError::ExecutableModified),
            "owner program can't reverse executable"
        );
        assert_eq!(
            Change::new(&owner, &mallory_program_id)
                .executable(false, true)
                .verify(),
            Err(InstructionError::ExecutableModified),
            "malicious Mallory should not be able to change the account executable"
        );
        assert_eq!(
            Change::new(&owner, &owner)
                .executable(false, true)
                .data(vec![1], vec![2])
                .verify(),
            Ok(()),
            "account data can change in the same instruction that sets the bit"
        );
        assert_eq!(
            Change::new(&owner, &owner)
                .executable(true, true)
                .data(vec![1], vec![2])
                .verify(),
            Err(InstructionError::ExecutableDataModified),
            "owner should not be able to change an account's data once its marked executable"
        );
        assert_eq!(
            Change::new(&owner, &owner)
                .executable(true, true)
                .lamports(1, 2)
                .verify(),
            Err(InstructionError::ExecutableLamportChange),
            "owner should not be able to add lamports once marked executable"
        );
        assert_eq!(
            Change::new(&owner, &owner)
                .executable(true, true)
                .lamports(1, 2)
                .verify(),
            Err(InstructionError::ExecutableLamportChange),
            "owner should not be able to add lamports once marked executable"
        );
        assert_eq!(
            Change::new(&owner, &owner)
                .executable(true, true)
                .lamports(2, 1)
                .verify(),
            Err(InstructionError::ExecutableLamportChange),
            "owner should not be able to subtract lamports once marked executable"
        );
        let data = vec![1; 100];
        let min_lamports = Rent::default().minimum_balance(data.len());
        assert_eq!(
            Change::new(&owner, &owner)
                .executable(false, true)
                .lamports(0, min_lamports)
                .data(data.clone(), data.clone())
                .verify(),
            Ok(()),
        );
        assert_eq!(
            Change::new(&owner, &owner)
                .executable(false, true)
                .lamports(0, min_lamports - 1)
                .data(data.clone(), data)
                .verify(),
            Err(InstructionError::ExecutableAccountNotRentExempt),
            "owner should not be able to change an account's data once its marked executable"
        );
    }

    #[test]
    fn test_verify_account_changes_data_len() {
        let alice_program_id = solana_sdk::pubkey::new_rand();

        assert_eq!(
            Change::new(&system_program::id(), &system_program::id())
                .data(vec![0], vec![0, 0])
                .verify(),
            Ok(()),
            "system program should be able to change the data len"
        );
        assert_eq!(
            Change::new(&alice_program_id, &system_program::id())
            .data(vec![0], vec![0,0])
            .verify(),
        Err(InstructionError::AccountDataSizeChanged),
        "system program should not be able to change the data length of accounts it does not own"
        );
    }

    #[test]
    fn test_verify_account_changes_data() {
        let alice_program_id = solana_sdk::pubkey::new_rand();
        let mallory_program_id = solana_sdk::pubkey::new_rand();

        assert_eq!(
            Change::new(&alice_program_id, &alice_program_id)
                .data(vec![0], vec![42])
                .verify(),
            Ok(()),
            "alice program should be able to change the data"
        );
        assert_eq!(
            Change::new(&mallory_program_id, &alice_program_id)
                .data(vec![0], vec![42])
                .verify(),
            Err(InstructionError::ExternalAccountDataModified),
            "non-owner mallory should not be able to change the account data"
        );
        assert_eq!(
            Change::new(&alice_program_id, &alice_program_id)
                .data(vec![0], vec![42])
                .read_only()
                .verify(),
            Err(InstructionError::ReadonlyDataModified),
            "alice isn't allowed to touch a CO account"
        );
    }

    #[test]
    fn test_verify_account_changes_rent_epoch() {
        let alice_program_id = solana_sdk::pubkey::new_rand();

        assert_eq!(
            Change::new(&alice_program_id, &system_program::id()).verify(),
            Ok(()),
            "nothing changed!"
        );
        assert_eq!(
            Change::new(&alice_program_id, &system_program::id())
                .rent_epoch(0, 1)
                .verify(),
            Err(InstructionError::RentEpochModified),
            "no one touches rent_epoch"
        );
    }

    #[test]
    fn test_verify_account_changes_deduct_lamports_and_reassign_account() {
        let alice_program_id = solana_sdk::pubkey::new_rand();
        let bob_program_id = solana_sdk::pubkey::new_rand();

        // positive test of this capability
        assert_eq!(
            Change::new(&alice_program_id, &alice_program_id)
            .owner(&bob_program_id)
            .lamports(42, 1)
            .data(vec![42], vec![0])
            .verify(),
        Ok(()),
        "alice should be able to deduct lamports and give the account to bob if the data is zeroed",
    );
    }

    #[test]
    fn test_verify_account_changes_lamports() {
        let alice_program_id = solana_sdk::pubkey::new_rand();

        assert_eq!(
            Change::new(&alice_program_id, &system_program::id())
                .lamports(42, 0)
                .read_only()
                .verify(),
            Err(InstructionError::ExternalAccountLamportSpend),
            "debit should fail, even if system program"
        );
        assert_eq!(
            Change::new(&alice_program_id, &alice_program_id)
                .lamports(42, 0)
                .read_only()
                .verify(),
            Err(InstructionError::ReadonlyLamportChange),
            "debit should fail, even if owning program"
        );
        assert_eq!(
            Change::new(&alice_program_id, &system_program::id())
                .lamports(42, 0)
                .owner(&system_program::id())
                .verify(),
            Err(InstructionError::ModifiedProgramId),
            "system program can't debit the account unless it was the pre.owner"
        );
        assert_eq!(
            Change::new(&system_program::id(), &system_program::id())
                .lamports(42, 0)
                .owner(&alice_program_id)
                .verify(),
            Ok(()),
            "system can spend (and change owner)"
        );
    }

    #[test]
    fn test_verify_account_changes_data_size_changed() {
        let alice_program_id = solana_sdk::pubkey::new_rand();

        assert_eq!(
            Change::new(&alice_program_id, &system_program::id())
                .data(vec![0], vec![0, 0])
                .verify(),
            Err(InstructionError::AccountDataSizeChanged),
            "system program should not be able to change another program's account data size"
        );
        assert_eq!(
            Change::new(&alice_program_id, &alice_program_id)
                .data(vec![0], vec![0, 0])
                .verify(),
            Err(InstructionError::AccountDataSizeChanged),
            "non-system programs cannot change their data size"
        );
        assert_eq!(
            Change::new(&system_program::id(), &system_program::id())
                .data(vec![0], vec![0, 0])
                .verify(),
            Ok(()),
            "system program should be able to change account data size"
        );
    }

    #[test]
    fn test_verify_account_changes_owner_executable() {
        let alice_program_id = solana_sdk::pubkey::new_rand();
        let bob_program_id = solana_sdk::pubkey::new_rand();

        assert_eq!(
            Change::new(&alice_program_id, &alice_program_id)
                .owner(&bob_program_id)
                .executable(false, true)
                .verify(),
            Err(InstructionError::ExecutableModified),
            "Program should not be able to change owner and executable at the same time"
        );
    }

    #[test]
    fn test_process_message_readonly_handling() {
        #[derive(Serialize, Deserialize)]
        enum MockSystemInstruction {
            Correct,
            AttemptCredit { lamports: u64 },
            AttemptDataChange { data: u8 },
        }

        fn mock_system_process_instruction(
            _program_id: &Pubkey,
            data: &[u8],
            invoke_context: &mut dyn InvokeContext,
        ) -> Result<(), InstructionError> {
            let keyed_accounts = invoke_context.get_keyed_accounts()?;
            if let Ok(instruction) = bincode::deserialize(data) {
                match instruction {
                    MockSystemInstruction::Correct => Ok(()),
                    MockSystemInstruction::AttemptCredit { lamports } => {
                        keyed_accounts[0]
                            .account
                            .borrow_mut()
                            .checked_sub_lamports(lamports)?;
                        keyed_accounts[1]
                            .account
                            .borrow_mut()
                            .checked_add_lamports(lamports)?;
                        Ok(())
                    }
                    // Change data in a read-only account
                    MockSystemInstruction::AttemptDataChange { data } => {
                        keyed_accounts[1].account.borrow_mut().set_data(vec![data]);
                        Ok(())
                    }
                }
            } else {
                Err(InstructionError::InvalidInstructionData)
            }
        }

        let mock_system_program_id = Pubkey::new(&[2u8; 32]);
        let rent_collector = RentCollector::default();
        let mut message_processor = MessageProcessor::default();
        message_processor.add_program(mock_system_program_id, mock_system_process_instruction);

        let accounts = vec![
            (
                solana_sdk::pubkey::new_rand(),
                AccountSharedData::new_ref(100, 1, &mock_system_program_id),
            ),
            (
                solana_sdk::pubkey::new_rand(),
                AccountSharedData::new_ref(0, 1, &mock_system_program_id),
            ),
        ];

        let account = Rc::new(RefCell::new(create_loadable_account_for_test(
            "mock_system_program",
        )));
        let loaders = vec![vec![(mock_system_program_id, account)]];

        let executors = Rc::new(RefCell::new(Executors::default()));
        let ancestors = Ancestors::default();

        let account_metas = vec![
            AccountMeta::new(accounts[0].0, true),
            AccountMeta::new_readonly(accounts[1].0, false),
        ];
        let message = Message::new(
            &[Instruction::new_with_bincode(
                mock_system_program_id,
                &MockSystemInstruction::Correct,
                account_metas.clone(),
            )],
            Some(&accounts[0].0),
        );

        let result = message_processor.process_message(
            &message,
            &loaders,
            &accounts,
            &rent_collector,
            None,
            executors.clone(),
            None,
            Arc::new(FeatureSet::all_enabled()),
            ComputeBudget::new(),
            Rc::new(RefCell::new(MockComputeMeter::default())),
            &mut ExecuteDetailsTimings::default(),
            Arc::new(Accounts::default()),
            &ancestors,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(accounts[0].1.borrow().lamports(), 100);
        assert_eq!(accounts[1].1.borrow().lamports(), 0);

        let message = Message::new(
            &[Instruction::new_with_bincode(
                mock_system_program_id,
                &MockSystemInstruction::AttemptCredit { lamports: 50 },
                account_metas.clone(),
            )],
            Some(&accounts[0].0),
        );

        let result = message_processor.process_message(
            &message,
            &loaders,
            &accounts,
            &rent_collector,
            None,
            executors.clone(),
            None,
            Arc::new(FeatureSet::all_enabled()),
            ComputeBudget::new(),
            Rc::new(RefCell::new(MockComputeMeter::default())),
            &mut ExecuteDetailsTimings::default(),
            Arc::new(Accounts::default()),
            &ancestors,
        );
        assert_eq!(
            result,
            Err(TransactionError::InstructionError(
                0,
                InstructionError::ReadonlyLamportChange
            ))
        );

        let message = Message::new(
            &[Instruction::new_with_bincode(
                mock_system_program_id,
                &MockSystemInstruction::AttemptDataChange { data: 50 },
                account_metas,
            )],
            Some(&accounts[0].0),
        );

        let result = message_processor.process_message(
            &message,
            &loaders,
            &accounts,
            &rent_collector,
            None,
            executors,
            None,
            Arc::new(FeatureSet::all_enabled()),
            ComputeBudget::new(),
            Rc::new(RefCell::new(MockComputeMeter::default())),
            &mut ExecuteDetailsTimings::default(),
            Arc::new(Accounts::default()),
            &ancestors,
        );
        assert_eq!(
            result,
            Err(TransactionError::InstructionError(
                0,
                InstructionError::ReadonlyDataModified
            ))
        );
    }

    #[test]
    fn test_process_message_duplicate_accounts() {
        #[derive(Serialize, Deserialize)]
        enum MockSystemInstruction {
            BorrowFail,
            MultiBorrowMut,
            DoWork { lamports: u64, data: u8 },
        }

        fn mock_system_process_instruction(
            _program_id: &Pubkey,
            data: &[u8],
            invoke_context: &mut dyn InvokeContext,
        ) -> Result<(), InstructionError> {
            let keyed_accounts = invoke_context.get_keyed_accounts()?;
            if let Ok(instruction) = bincode::deserialize(data) {
                match instruction {
                    MockSystemInstruction::BorrowFail => {
                        let from_account = keyed_accounts[0].try_account_ref_mut()?;
                        let dup_account = keyed_accounts[2].try_account_ref_mut()?;
                        if from_account.lamports() != dup_account.lamports() {
                            return Err(InstructionError::InvalidArgument);
                        }
                        Ok(())
                    }
                    MockSystemInstruction::MultiBorrowMut => {
                        let from_lamports = {
                            let from_account = keyed_accounts[0].try_account_ref_mut()?;
                            from_account.lamports()
                        };
                        let dup_lamports = {
                            let dup_account = keyed_accounts[2].try_account_ref_mut()?;
                            dup_account.lamports()
                        };
                        if from_lamports != dup_lamports {
                            return Err(InstructionError::InvalidArgument);
                        }
                        Ok(())
                    }
                    MockSystemInstruction::DoWork { lamports, data } => {
                        {
                            let mut to_account = keyed_accounts[1].try_account_ref_mut()?;
                            let mut dup_account = keyed_accounts[2].try_account_ref_mut()?;
                            dup_account.checked_sub_lamports(lamports)?;
                            to_account.checked_add_lamports(lamports)?;
                            dup_account.set_data(vec![data]);
                        }
                        keyed_accounts[0]
                            .try_account_ref_mut()?
                            .checked_sub_lamports(lamports)?;
                        keyed_accounts[1]
                            .try_account_ref_mut()?
                            .checked_add_lamports(lamports)?;
                        Ok(())
                    }
                }
            } else {
                Err(InstructionError::InvalidInstructionData)
            }
        }

        let mock_program_id = Pubkey::new(&[2u8; 32]);
        let rent_collector = RentCollector::default();
        let mut message_processor = MessageProcessor::default();
        message_processor.add_program(mock_program_id, mock_system_process_instruction);

        let accounts = vec![
            (
                solana_sdk::pubkey::new_rand(),
                AccountSharedData::new_ref(100, 1, &mock_program_id),
            ),
            (
                solana_sdk::pubkey::new_rand(),
                AccountSharedData::new_ref(0, 1, &mock_program_id),
            ),
        ];

        let account = Rc::new(RefCell::new(create_loadable_account_for_test(
            "mock_system_program",
        )));
        let loaders = vec![vec![(mock_program_id, account)]];

        let executors = Rc::new(RefCell::new(Executors::default()));
        let ancestors = Ancestors::default();

        let account_metas = vec![
            AccountMeta::new(accounts[0].0, true),
            AccountMeta::new(accounts[1].0, false),
            AccountMeta::new(accounts[0].0, false),
        ];

        // Try to borrow mut the same account
        let message = Message::new(
            &[Instruction::new_with_bincode(
                mock_program_id,
                &MockSystemInstruction::BorrowFail,
                account_metas.clone(),
            )],
            Some(&accounts[0].0),
        );
        let result = message_processor.process_message(
            &message,
            &loaders,
            &accounts,
            &rent_collector,
            None,
            executors.clone(),
            None,
            Arc::new(FeatureSet::all_enabled()),
            ComputeBudget::new(),
            Rc::new(RefCell::new(MockComputeMeter::default())),
            &mut ExecuteDetailsTimings::default(),
            Arc::new(Accounts::default()),
            &ancestors,
        );
        assert_eq!(
            result,
            Err(TransactionError::InstructionError(
                0,
                InstructionError::AccountBorrowFailed
            ))
        );

        // Try to borrow mut the same account in a safe way
        let message = Message::new(
            &[Instruction::new_with_bincode(
                mock_program_id,
                &MockSystemInstruction::MultiBorrowMut,
                account_metas.clone(),
            )],
            Some(&accounts[0].0),
        );
        let result = message_processor.process_message(
            &message,
            &loaders,
            &accounts,
            &rent_collector,
            None,
            executors.clone(),
            None,
            Arc::new(FeatureSet::all_enabled()),
            ComputeBudget::new(),
            Rc::new(RefCell::new(MockComputeMeter::default())),
            &mut ExecuteDetailsTimings::default(),
            Arc::new(Accounts::default()),
            &ancestors,
        );
        assert_eq!(result, Ok(()));

        // Do work on the same account but at different location in keyed_accounts[]
        let message = Message::new(
            &[Instruction::new_with_bincode(
                mock_program_id,
                &MockSystemInstruction::DoWork {
                    lamports: 10,
                    data: 42,
                },
                account_metas,
            )],
            Some(&accounts[0].0),
        );
        let ancestors = Ancestors::default();
        let result = message_processor.process_message(
            &message,
            &loaders,
            &accounts,
            &rent_collector,
            None,
            executors,
            None,
            Arc::new(FeatureSet::all_enabled()),
            ComputeBudget::new(),
            Rc::new(RefCell::new(MockComputeMeter::default())),
            &mut ExecuteDetailsTimings::default(),
            Arc::new(Accounts::default()),
            &ancestors,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(accounts[0].1.borrow().lamports(), 80);
        assert_eq!(accounts[1].1.borrow().lamports(), 20);
        assert_eq!(accounts[0].1.borrow().data(), &vec![42]);
    }

    #[test]
    fn test_process_cross_program() {
        #[derive(Debug, Serialize, Deserialize)]
        enum MockInstruction {
            NoopSuccess,
            NoopFail,
            ModifyOwned,
            ModifyNotOwned,
        }

        fn mock_process_instruction(
            program_id: &Pubkey,
            data: &[u8],
            invoke_context: &mut dyn InvokeContext,
        ) -> Result<(), InstructionError> {
            let keyed_accounts = invoke_context.get_keyed_accounts()?;
            assert_eq!(*program_id, keyed_accounts[0].owner()?);
            assert_ne!(
                keyed_accounts[1].owner()?,
                *keyed_accounts[0].unsigned_key()
            );

            if let Ok(instruction) = bincode::deserialize(data) {
                match instruction {
                    MockInstruction::NoopSuccess => (),
                    MockInstruction::NoopFail => return Err(InstructionError::GenericError),
                    MockInstruction::ModifyOwned => {
                        keyed_accounts[0].try_account_ref_mut()?.data_as_mut_slice()[0] = 1
                    }
                    MockInstruction::ModifyNotOwned => {
                        keyed_accounts[1].try_account_ref_mut()?.data_as_mut_slice()[0] = 1
                    }
                }
            } else {
                return Err(InstructionError::InvalidInstructionData);
            }
            Ok(())
        }

        let caller_program_id = solana_sdk::pubkey::new_rand();
        let callee_program_id = solana_sdk::pubkey::new_rand();

        let mut program_account = AccountSharedData::new(1, 0, &native_loader::id());
        program_account.set_executable(true);
        let executable_accounts = vec![(
            callee_program_id,
            Rc::new(RefCell::new(program_account.clone())),
        )];

        let owned_account = AccountSharedData::new(42, 1, &callee_program_id);
        let not_owned_account = AccountSharedData::new(84, 1, &solana_sdk::pubkey::new_rand());

        #[allow(unused_mut)]
        let accounts = vec![
            (
                solana_sdk::pubkey::new_rand(),
                Rc::new(RefCell::new(owned_account)),
            ),
            (
                solana_sdk::pubkey::new_rand(),
                Rc::new(RefCell::new(not_owned_account)),
            ),
            (callee_program_id, Rc::new(RefCell::new(program_account))),
        ];

        let compiled_instruction = CompiledInstruction::new(2, &(), vec![0, 1, 2]);
        let programs: Vec<(_, ProcessInstructionWithContext)> =
            vec![(callee_program_id, mock_process_instruction)];
        let metas = vec![
            AccountMeta::new(accounts[0].0, false),
            AccountMeta::new(accounts[1].0, false),
        ];

        let instruction = Instruction::new_with_bincode(
            callee_program_id,
            &MockInstruction::NoopSuccess,
            metas.clone(),
        );
        let message = Message::new(&[instruction], None);

        let ancestors = Ancestors::default();
        let mut invoke_context = ThisInvokeContext::new(
            &caller_program_id,
            Rent::default(),
            &message,
            &compiled_instruction,
            &executable_accounts,
            &accounts,
            programs.as_slice(),
            None,
            ComputeBudget::default(),
            Rc::new(RefCell::new(MockComputeMeter::default())),
            Rc::new(RefCell::new(Executors::default())),
            None,
            Arc::new(FeatureSet::all_enabled()),
            Arc::new(Accounts::default()),
            &ancestors,
        );

        // not owned account modified by the caller (before the invoke)
        let caller_write_privileges = message
            .account_keys
            .iter()
            .enumerate()
            .map(|(i, _)| message.is_writable(i))
            .collect::<Vec<bool>>();
        accounts[0].1.borrow_mut().data_as_mut_slice()[0] = 1;
        assert_eq!(
            MessageProcessor::process_cross_program_instruction(
                &message,
                &executable_accounts,
                &accounts,
                &caller_write_privileges,
                &mut invoke_context,
            ),
            Err(InstructionError::ExternalAccountDataModified)
        );
        accounts[0].1.borrow_mut().data_as_mut_slice()[0] = 0;

        let cases = vec![
            (MockInstruction::NoopSuccess, Ok(())),
            (
                MockInstruction::NoopFail,
                Err(InstructionError::GenericError),
            ),
            (MockInstruction::ModifyOwned, Ok(())),
            (
                MockInstruction::ModifyNotOwned,
                Err(InstructionError::ExternalAccountDataModified),
            ),
        ];

        for case in cases {
            let instruction =
                Instruction::new_with_bincode(callee_program_id, &case.0, metas.clone());
            let message = Message::new(&[instruction], None);

            let ancestors = Ancestors::default();
            let mut invoke_context = ThisInvokeContext::new(
                &caller_program_id,
                Rent::default(),
                &message,
                &compiled_instruction,
                &executable_accounts,
                &accounts,
                programs.as_slice(),
                None,
                ComputeBudget::default(),
                Rc::new(RefCell::new(MockComputeMeter::default())),
                Rc::new(RefCell::new(Executors::default())),
                None,
                Arc::new(FeatureSet::all_enabled()),
                Arc::new(Accounts::default()),
                &ancestors,
            );

            let caller_write_privileges = message
                .account_keys
                .iter()
                .enumerate()
                .map(|(i, _)| message.is_writable(i))
                .collect::<Vec<bool>>();
            assert_eq!(
                MessageProcessor::process_cross_program_instruction(
                    &message,
                    &executable_accounts,
                    &accounts,
                    &caller_write_privileges,
                    &mut invoke_context,
                ),
                case.1
            );
        }
    }

    #[test]
    fn test_debug() {
        let mut message_processor = MessageProcessor::default();
        #[allow(clippy::unnecessary_wraps)]
        fn mock_process_instruction(
            _program_id: &Pubkey,
            _data: &[u8],
            _invoke_context: &mut dyn InvokeContext,
        ) -> Result<(), InstructionError> {
            Ok(())
        }
        #[allow(clippy::unnecessary_wraps)]
        fn mock_ix_processor(
            _pubkey: &Pubkey,
            _data: &[u8],
            _context: &mut dyn InvokeContext,
        ) -> Result<(), InstructionError> {
            Ok(())
        }
        let program_id = solana_sdk::pubkey::new_rand();
        message_processor.add_program(program_id, mock_process_instruction);
        message_processor.add_loader(program_id, mock_ix_processor);

        assert!(!format!("{:?}", message_processor).is_empty());
    }
}
