use crate::{
    clock::{Epoch, INITIAL_RENT_EPOCH},
    lamports::LamportsError,
    pubkey::Pubkey,
};
use solana_program::{account_info::AccountInfo, sysvar::Sysvar};
use std::{cell::Ref, cell::RefCell, cmp, fmt, rc::Rc, sync::Arc};

/// An Account with data that is stored on chain
#[repr(C)]
#[frozen_abi(digest = "HawRVHh7t4d3H3bitWHFt25WhhoDmbJMCfWdESQQoYEy")]
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Default, AbiExample)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// lamports in the account
    pub lamports: u64,
    /// data held in this account
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    /// the program that owns this account. If executable, the program that loads this account.
    pub owner: Pubkey,
    /// this account's data contains a loaded program (and is now read-only)
    pub executable: bool,
    /// the epoch at which this account will next owe rent
    pub rent_epoch: Epoch,
}

/// An Account with data that is stored on chain
/// This will be the in-memory representation of the 'Account' struct data.
/// The existing 'Account' structure cannot easily change due to downstream projects.
#[derive(PartialEq, Eq, Clone, Default, AbiExample)]
pub struct AccountSharedData {
    /// lamports in the account
    lamports: u64,
    /// data held in this account
    data: Arc<Vec<u8>>,
    /// the program that owns this account. If executable, the program that loads this account.
    owner: Pubkey,
    /// this account's data contains a loaded program (and is now read-only)
    executable: bool,
    /// the epoch at which this account will next owe rent
    rent_epoch: Epoch,
}

/// Compares two ReadableAccounts
///
/// Returns true if accounts are essentially equivalent as in all fields are equivalent.
pub fn accounts_equal<T: ReadableAccount, U: ReadableAccount>(me: &T, other: &U) -> bool {
    me.lamports() == other.lamports()
        && me.data() == other.data()
        && me.owner() == other.owner()
        && me.executable() == other.executable()
        && me.rent_epoch() == other.rent_epoch()
}

impl From<AccountSharedData> for Account {
    fn from(mut other: AccountSharedData) -> Self {
        let account_data = Arc::make_mut(&mut other.data);
        Self {
            lamports: other.lamports,
            data: std::mem::take(account_data),
            owner: other.owner,
            executable: other.executable,
            rent_epoch: other.rent_epoch,
        }
    }
}

impl From<Account> for AccountSharedData {
    fn from(other: Account) -> Self {
        Self {
            lamports: other.lamports,
            data: Arc::new(other.data),
            owner: other.owner,
            executable: other.executable,
            rent_epoch: other.rent_epoch,
        }
    }
}

pub trait WritableAccount: ReadableAccount {
    fn set_lamports(&mut self, lamports: u64);
    fn checked_add_lamports(&mut self, lamports: u64) -> Result<(), LamportsError> {
        self.set_lamports(
            self.lamports()
                .checked_add(lamports)
                .ok_or(LamportsError::ArithmeticOverflow)?,
        );
        Ok(())
    }
    fn checked_sub_lamports(&mut self, lamports: u64) -> Result<(), LamportsError> {
        self.set_lamports(
            self.lamports()
                .checked_sub(lamports)
                .ok_or(LamportsError::ArithmeticUnderflow)?,
        );
        Ok(())
    }
    fn data_as_mut_slice(&mut self) -> &mut [u8];
    fn set_owner(&mut self, owner: Pubkey);
    fn copy_into_owner_from_slice(&mut self, source: &[u8]);
    fn set_executable(&mut self, executable: bool);
    fn set_rent_epoch(&mut self, epoch: Epoch);
    fn create(
        lamports: u64,
        data: Vec<u8>,
        owner: Pubkey,
        executable: bool,
        rent_epoch: Epoch,
    ) -> Self;
}

pub trait ReadableAccount: Sized {
    fn lamports(&self) -> u64;
    fn data(&self) -> &[u8];
    fn owner(&self) -> &Pubkey;
    fn executable(&self) -> bool;
    fn rent_epoch(&self) -> Epoch;
    fn to_account_shared_data(&self) -> AccountSharedData {
        AccountSharedData::create(
            self.lamports(),
            self.data().to_vec(),
            *self.owner(),
            self.executable(),
            self.rent_epoch(),
        )
    }
}

impl ReadableAccount for Account {
    fn lamports(&self) -> u64 {
        self.lamports
    }
    fn data(&self) -> &[u8] {
        &self.data
    }
    fn owner(&self) -> &Pubkey {
        &self.owner
    }
    fn executable(&self) -> bool {
        self.executable
    }
    fn rent_epoch(&self) -> Epoch {
        self.rent_epoch
    }
}

impl WritableAccount for Account {
    fn set_lamports(&mut self, lamports: u64) {
        self.lamports = lamports;
    }
    fn data_as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
    fn set_owner(&mut self, owner: Pubkey) {
        self.owner = owner;
    }
    fn copy_into_owner_from_slice(&mut self, source: &[u8]) {
        self.owner.as_mut().copy_from_slice(source);
    }
    fn set_executable(&mut self, executable: bool) {
        self.executable = executable;
    }
    fn set_rent_epoch(&mut self, epoch: Epoch) {
        self.rent_epoch = epoch;
    }
    fn create(
        lamports: u64,
        data: Vec<u8>,
        owner: Pubkey,
        executable: bool,
        rent_epoch: Epoch,
    ) -> Self {
        Account {
            lamports,
            data,
            owner,
            executable,
            rent_epoch,
        }
    }
}

impl WritableAccount for AccountSharedData {
    fn set_lamports(&mut self, lamports: u64) {
        self.lamports = lamports;
    }
    fn data_as_mut_slice(&mut self) -> &mut [u8] {
        let data = Arc::make_mut(&mut self.data);
        &mut data[..]
    }
    fn set_owner(&mut self, owner: Pubkey) {
        self.owner = owner;
    }
    fn copy_into_owner_from_slice(&mut self, source: &[u8]) {
        self.owner.as_mut().copy_from_slice(source);
    }
    fn set_executable(&mut self, executable: bool) {
        self.executable = executable;
    }
    fn set_rent_epoch(&mut self, epoch: Epoch) {
        self.rent_epoch = epoch;
    }
    fn create(
        lamports: u64,
        data: Vec<u8>,
        owner: Pubkey,
        executable: bool,
        rent_epoch: Epoch,
    ) -> Self {
        AccountSharedData {
            lamports,
            data: Arc::new(data),
            owner,
            executable,
            rent_epoch,
        }
    }
}

impl ReadableAccount for AccountSharedData {
    fn lamports(&self) -> u64 {
        self.lamports
    }
    fn data(&self) -> &[u8] {
        &self.data
    }
    fn owner(&self) -> &Pubkey {
        &self.owner
    }
    fn executable(&self) -> bool {
        self.executable
    }
    fn rent_epoch(&self) -> Epoch {
        self.rent_epoch
    }
}

impl ReadableAccount for Ref<'_, AccountSharedData> {
    fn lamports(&self) -> u64 {
        self.lamports
    }
    fn data(&self) -> &[u8] {
        &self.data
    }
    fn owner(&self) -> &Pubkey {
        &self.owner
    }
    fn executable(&self) -> bool {
        self.executable
    }
    fn rent_epoch(&self) -> Epoch {
        self.rent_epoch
    }
}

impl ReadableAccount for Ref<'_, Account> {
    fn lamports(&self) -> u64 {
        self.lamports
    }
    fn data(&self) -> &[u8] {
        &self.data
    }
    fn owner(&self) -> &Pubkey {
        &self.owner
    }
    fn executable(&self) -> bool {
        self.executable
    }
    fn rent_epoch(&self) -> Epoch {
        self.rent_epoch
    }
}

fn debug_fmt<T: ReadableAccount>(item: &T, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let data_len = cmp::min(64, item.data().len());
    let data_str = if data_len > 0 {
        format!(" data: {}", hex::encode(item.data()[..data_len].to_vec()))
    } else {
        "".to_string()
    };
    write!(
        f,
        "Account {{ lamports: {} data.len: {} owner: {} executable: {} rent_epoch: {}{} }}",
        item.lamports(),
        item.data().len(),
        item.owner(),
        item.executable(),
        item.rent_epoch(),
        data_str,
    )
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_fmt(self, f)
    }
}

impl fmt::Debug for AccountSharedData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_fmt(self, f)
    }
}

fn shared_new<T: WritableAccount>(lamports: u64, space: usize, owner: &Pubkey) -> T {
    T::create(
        lamports,
        vec![0u8; space],
        *owner,
        bool::default(),
        Epoch::default(),
    )
}

fn shared_new_ref<T: WritableAccount>(
    lamports: u64,
    space: usize,
    owner: &Pubkey,
) -> Rc<RefCell<T>> {
    Rc::new(RefCell::new(shared_new::<T>(lamports, space, owner)))
}

fn shared_new_data<T: serde::Serialize, U: WritableAccount>(
    lamports: u64,
    state: &T,
    owner: &Pubkey,
) -> Result<U, bincode::Error> {
    let data = bincode::serialize(state)?;
    Ok(U::create(
        lamports,
        data,
        *owner,
        bool::default(),
        Epoch::default(),
    ))
}
fn shared_new_ref_data<T: serde::Serialize, U: WritableAccount>(
    lamports: u64,
    state: &T,
    owner: &Pubkey,
) -> Result<RefCell<U>, bincode::Error> {
    Ok(RefCell::new(shared_new_data::<T, U>(
        lamports, state, owner,
    )?))
}

fn shared_new_data_with_space<T: serde::Serialize, U: WritableAccount>(
    lamports: u64,
    state: &T,
    space: usize,
    owner: &Pubkey,
) -> Result<U, bincode::Error> {
    let mut account = shared_new::<U>(lamports, space, owner);

    shared_serialize_data(&mut account, state)?;

    Ok(account)
}
fn shared_new_ref_data_with_space<T: serde::Serialize, U: WritableAccount>(
    lamports: u64,
    state: &T,
    space: usize,
    owner: &Pubkey,
) -> Result<RefCell<U>, bincode::Error> {
    Ok(RefCell::new(shared_new_data_with_space::<T, U>(
        lamports, state, space, owner,
    )?))
}

fn shared_deserialize_data<T: serde::de::DeserializeOwned, U: ReadableAccount>(
    account: &U,
) -> Result<T, bincode::Error> {
    bincode::deserialize(account.data())
}

fn shared_serialize_data<T: serde::Serialize, U: WritableAccount>(
    account: &mut U,
    state: &T,
) -> Result<(), bincode::Error> {
    if bincode::serialized_size(state)? > account.data().len() as u64 {
        return Err(Box::new(bincode::ErrorKind::SizeLimit));
    }
    bincode::serialize_into(&mut account.data_as_mut_slice(), state)
}

impl Account {
    pub fn new(lamports: u64, space: usize, owner: &Pubkey) -> Self {
        shared_new(lamports, space, owner)
    }
    pub fn new_ref(lamports: u64, space: usize, owner: &Pubkey) -> Rc<RefCell<Self>> {
        shared_new_ref(lamports, space, owner)
    }
    pub fn new_data<T: serde::Serialize>(
        lamports: u64,
        state: &T,
        owner: &Pubkey,
    ) -> Result<Self, bincode::Error> {
        shared_new_data(lamports, state, owner)
    }
    pub fn new_ref_data<T: serde::Serialize>(
        lamports: u64,
        state: &T,
        owner: &Pubkey,
    ) -> Result<RefCell<Self>, bincode::Error> {
        shared_new_ref_data(lamports, state, owner)
    }
    pub fn new_data_with_space<T: serde::Serialize>(
        lamports: u64,
        state: &T,
        space: usize,
        owner: &Pubkey,
    ) -> Result<Self, bincode::Error> {
        shared_new_data_with_space(lamports, state, space, owner)
    }
    pub fn new_ref_data_with_space<T: serde::Serialize>(
        lamports: u64,
        state: &T,
        space: usize,
        owner: &Pubkey,
    ) -> Result<RefCell<Self>, bincode::Error> {
        shared_new_ref_data_with_space(lamports, state, space, owner)
    }
    pub fn deserialize_data<T: serde::de::DeserializeOwned>(&self) -> Result<T, bincode::Error> {
        shared_deserialize_data(self)
    }
    pub fn serialize_data<T: serde::Serialize>(&mut self, state: &T) -> Result<(), bincode::Error> {
        shared_serialize_data(self, state)
    }
}

impl AccountSharedData {
    pub fn set_data_from_slice(&mut self, data: &[u8]) {
        let len = self.data.len();
        let len_different = len != data.len();
        let different = len_different || data != &self.data[..];
        if different {
            self.data = Arc::new(data.to_vec());
        }
    }
    pub fn set_data(&mut self, data: Vec<u8>) {
        self.data = Arc::new(data);
    }
    pub fn new(lamports: u64, space: usize, owner: &Pubkey) -> Self {
        shared_new(lamports, space, owner)
    }
    pub fn new_ref(lamports: u64, space: usize, owner: &Pubkey) -> Rc<RefCell<Self>> {
        shared_new_ref(lamports, space, owner)
    }
    pub fn new_data<T: serde::Serialize>(
        lamports: u64,
        state: &T,
        owner: &Pubkey,
    ) -> Result<Self, bincode::Error> {
        shared_new_data(lamports, state, owner)
    }
    pub fn new_ref_data<T: serde::Serialize>(
        lamports: u64,
        state: &T,
        owner: &Pubkey,
    ) -> Result<RefCell<Self>, bincode::Error> {
        shared_new_ref_data(lamports, state, owner)
    }
    pub fn new_data_with_space<T: serde::Serialize>(
        lamports: u64,
        state: &T,
        space: usize,
        owner: &Pubkey,
    ) -> Result<Self, bincode::Error> {
        shared_new_data_with_space(lamports, state, space, owner)
    }
    pub fn new_ref_data_with_space<T: serde::Serialize>(
        lamports: u64,
        state: &T,
        space: usize,
        owner: &Pubkey,
    ) -> Result<RefCell<Self>, bincode::Error> {
        shared_new_ref_data_with_space(lamports, state, space, owner)
    }
    pub fn deserialize_data<T: serde::de::DeserializeOwned>(&self) -> Result<T, bincode::Error> {
        shared_deserialize_data(self)
    }
    pub fn serialize_data<T: serde::Serialize>(&mut self, state: &T) -> Result<(), bincode::Error> {
        shared_serialize_data(self, state)
    }
}

pub type InheritableAccountFields = (u64, Epoch);
pub const DUMMY_INHERITABLE_ACCOUNT_FIELDS: InheritableAccountFields = (1, INITIAL_RENT_EPOCH);

/// Create an `Account` from a `Sysvar`.
#[deprecated(
    since = "1.5.17",
    note = "Please use `create_account_for_test` instead"
)]
pub fn create_account<S: Sysvar>(sysvar: &S, lamports: u64) -> Account {
    create_account_with_fields(sysvar, (lamports, INITIAL_RENT_EPOCH))
}

pub fn create_account_with_fields<S: Sysvar>(
    sysvar: &S,
    (lamports, rent_epoch): InheritableAccountFields,
) -> Account {
    let data_len = S::size_of().max(bincode::serialized_size(sysvar).unwrap() as usize);
    let mut account = Account::new(lamports, data_len, &solana_program::sysvar::id());
    to_account::<S, Account>(sysvar, &mut account).unwrap();
    account.rent_epoch = rent_epoch;
    account
}

pub fn create_account_for_test<S: Sysvar>(sysvar: &S) -> Account {
    create_account_with_fields(sysvar, DUMMY_INHERITABLE_ACCOUNT_FIELDS)
}

/// Create an `Account` from a `Sysvar`.
#[deprecated(
    since = "1.5.17",
    note = "Please use `create_account_shared_data_for_test` instead"
)]
pub fn create_account_shared_data<S: Sysvar>(sysvar: &S, lamports: u64) -> AccountSharedData {
    AccountSharedData::from(create_account_with_fields(
        sysvar,
        (lamports, INITIAL_RENT_EPOCH),
    ))
}

pub fn create_account_shared_data_with_fields<S: Sysvar>(
    sysvar: &S,
    fields: InheritableAccountFields,
) -> AccountSharedData {
    AccountSharedData::from(create_account_with_fields(sysvar, fields))
}

pub fn create_account_shared_data_for_test<S: Sysvar>(sysvar: &S) -> AccountSharedData {
    AccountSharedData::from(create_account_with_fields(
        sysvar,
        DUMMY_INHERITABLE_ACCOUNT_FIELDS,
    ))
}

/// Create a `Sysvar` from an `Account`'s data.
pub fn from_account<S: Sysvar, T: ReadableAccount>(account: &T) -> Option<S> {
    bincode::deserialize(account.data()).ok()
}

/// Serialize a `Sysvar` into an `Account`'s data.
pub fn to_account<S: Sysvar, T: WritableAccount>(sysvar: &S, account: &mut T) -> Option<()> {
    bincode::serialize_into(account.data_as_mut_slice(), sysvar).ok()
}

/// Return the information required to construct an `AccountInfo`.  Used by the
/// `AccountInfo` conversion implementations.
impl solana_program::account_info::Account for Account {
    fn get(&mut self) -> (&mut u64, &mut [u8], &Pubkey, bool, Epoch) {
        (
            &mut self.lamports,
            &mut self.data,
            &self.owner,
            self.executable,
            self.rent_epoch,
        )
    }
}

/// Create `AccountInfo`s
pub fn create_is_signer_account_infos<'a>(
    accounts: &'a mut [(&'a Pubkey, bool, &'a mut Account)],
) -> Vec<AccountInfo<'a>> {
    accounts
        .iter_mut()
        .map(|(key, is_signer, account)| {
            AccountInfo::new(
                key,
                *is_signer,
                false,
                &mut account.lamports,
                &mut account.data,
                &account.owner,
                account.executable,
                account.rent_epoch,
            )
        })
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn make_two_accounts(key: &Pubkey) -> (Account, AccountSharedData) {
        let mut account1 = Account::new(1, 2, key);
        account1.executable = true;
        account1.rent_epoch = 4;
        let mut account2 = AccountSharedData::new(1, 2, key);
        account2.executable = true;
        account2.rent_epoch = 4;
        assert!(accounts_equal(&account1, &account2));
        (account1, account2)
    }

    #[test]
    fn test_account_data_copy_as_slice() {
        let key = Pubkey::new_unique();
        let key2 = Pubkey::new_unique();
        let (mut account1, mut account2) = make_two_accounts(&key);
        account1.copy_into_owner_from_slice(key2.as_ref());
        account2.copy_into_owner_from_slice(key2.as_ref());
        assert!(accounts_equal(&account1, &account2));
        assert_eq!(account1.owner(), &key2);
    }

    #[test]
    fn test_account_set_data_from_slice() {
        let key = Pubkey::new_unique();
        let (_, mut account) = make_two_accounts(&key);
        assert_eq!(account.data(), &vec![0, 0]);
        account.set_data_from_slice(&[1, 2]);
        assert_eq!(account.data(), &vec![1, 2]);
        account.set_data_from_slice(&[1, 2, 3]);
        assert_eq!(account.data(), &vec![1, 2, 3]);
        account.set_data_from_slice(&[4, 5, 6]);
        assert_eq!(account.data(), &vec![4, 5, 6]);
        account.set_data_from_slice(&[4, 5, 6, 0]);
        assert_eq!(account.data(), &vec![4, 5, 6, 0]);
        account.set_data_from_slice(&[]);
        assert_eq!(account.data().len(), 0);
        account.set_data_from_slice(&[44]);
        assert_eq!(account.data(), &vec![44]);
        account.set_data_from_slice(&[44]);
        assert_eq!(account.data(), &vec![44]);
    }

    #[test]
    fn test_account_data_set_data() {
        let key = Pubkey::new_unique();
        let (_, mut account) = make_two_accounts(&key);
        assert_eq!(account.data(), &vec![0, 0]);
        account.set_data(vec![1, 2]);
        assert_eq!(account.data(), &vec![1, 2]);
        account.set_data(vec![]);
        assert_eq!(account.data().len(), 0);
    }

    #[test]
    #[should_panic(
        expected = "called `Result::unwrap()` on an `Err` value: Io(Kind(UnexpectedEof))"
    )]
    fn test_account_deserialize() {
        let key = Pubkey::new_unique();
        let (account1, _account2) = make_two_accounts(&key);
        account1.deserialize_data::<String>().unwrap();
    }

    #[test]
    #[should_panic(expected = "called `Result::unwrap()` on an `Err` value: SizeLimit")]
    fn test_account_serialize() {
        let key = Pubkey::new_unique();
        let (mut account1, _account2) = make_two_accounts(&key);
        account1.serialize_data(&"hello world").unwrap();
    }

    #[test]
    #[should_panic(
        expected = "called `Result::unwrap()` on an `Err` value: Io(Kind(UnexpectedEof))"
    )]
    fn test_account_shared_data_deserialize() {
        let key = Pubkey::new_unique();
        let (_account1, account2) = make_two_accounts(&key);
        account2.deserialize_data::<String>().unwrap();
    }

    #[test]
    #[should_panic(expected = "called `Result::unwrap()` on an `Err` value: SizeLimit")]
    fn test_account_shared_data_serialize() {
        let key = Pubkey::new_unique();
        let (_account1, mut account2) = make_two_accounts(&key);
        account2.serialize_data(&"hello world").unwrap();
    }

    #[test]
    fn test_to_account_shared_data() {
        let key = Pubkey::new_unique();
        let (account1, account2) = make_two_accounts(&key);
        assert!(accounts_equal(&account1, &account2));
        let account3 = account1.to_account_shared_data();
        let account4 = account2.to_account_shared_data();
        assert!(accounts_equal(&account1, &account3));
        assert!(accounts_equal(&account1, &account4));
    }

    #[test]
    fn test_account_shared_data() {
        let key = Pubkey::new_unique();
        let (account1, account2) = make_two_accounts(&key);
        assert!(accounts_equal(&account1, &account2));
        let account = account1;
        assert_eq!(account.lamports, 1);
        assert_eq!(account.lamports(), 1);
        assert_eq!(account.data.len(), 2);
        assert_eq!(account.data().len(), 2);
        assert_eq!(account.owner, key);
        assert_eq!(account.owner(), &key);
        assert!(account.executable);
        assert!(account.executable());
        assert_eq!(account.rent_epoch, 4);
        assert_eq!(account.rent_epoch(), 4);
        let account = account2;
        assert_eq!(account.lamports, 1);
        assert_eq!(account.lamports(), 1);
        assert_eq!(account.data.len(), 2);
        assert_eq!(account.data().len(), 2);
        assert_eq!(account.owner, key);
        assert_eq!(account.owner(), &key);
        assert!(account.executable);
        assert!(account.executable());
        assert_eq!(account.rent_epoch, 4);
        assert_eq!(account.rent_epoch(), 4);
    }

    // test clone and from for both types against expected
    fn test_equal(
        should_be_equal: bool,
        account1: &Account,
        account2: &AccountSharedData,
        account_expected: &Account,
    ) {
        assert_eq!(should_be_equal, accounts_equal(account1, account2));
        if should_be_equal {
            assert!(accounts_equal(account_expected, account2));
        }
        assert_eq!(
            accounts_equal(account_expected, account1),
            accounts_equal(account_expected, &account1.clone())
        );
        assert_eq!(
            accounts_equal(account_expected, account2),
            accounts_equal(account_expected, &account2.clone())
        );
        assert_eq!(
            accounts_equal(account_expected, account1),
            accounts_equal(account_expected, &AccountSharedData::from(account1.clone()))
        );
        assert_eq!(
            accounts_equal(account_expected, account2),
            accounts_equal(account_expected, &Account::from(account2.clone()))
        );
    }

    #[test]
    fn test_account_add_sub_lamports() {
        let key = Pubkey::new_unique();
        let (mut account1, mut account2) = make_two_accounts(&key);
        assert!(accounts_equal(&account1, &account2));
        account1.checked_add_lamports(1).unwrap();
        account2.checked_add_lamports(1).unwrap();
        assert!(accounts_equal(&account1, &account2));
        assert_eq!(account1.lamports(), 2);
        account1.checked_sub_lamports(2).unwrap();
        account2.checked_sub_lamports(2).unwrap();
        assert!(accounts_equal(&account1, &account2));
        assert_eq!(account1.lamports(), 0);
    }

    #[test]
    #[should_panic(expected = "Overflow")]
    fn test_account_checked_add_lamports_overflow() {
        let key = Pubkey::new_unique();
        let (mut account1, _account2) = make_two_accounts(&key);
        account1.checked_add_lamports(u64::MAX).unwrap();
    }

    #[test]
    #[should_panic(expected = "Underflow")]
    fn test_account_checked_sub_lamports_underflow() {
        let key = Pubkey::new_unique();
        let (mut account1, _account2) = make_two_accounts(&key);
        account1.checked_sub_lamports(u64::MAX).unwrap();
    }

    #[test]
    #[should_panic(expected = "Overflow")]
    fn test_account_checked_add_lamports_overflow2() {
        let key = Pubkey::new_unique();
        let (_account1, mut account2) = make_two_accounts(&key);
        account2.checked_add_lamports(u64::MAX).unwrap();
    }

    #[test]
    #[should_panic(expected = "Underflow")]
    fn test_account_checked_sub_lamports_underflow2() {
        let key = Pubkey::new_unique();
        let (_account1, mut account2) = make_two_accounts(&key);
        account2.checked_sub_lamports(u64::MAX).unwrap();
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_account_shared_data_all_fields() {
        let key = Pubkey::new_unique();
        let key2 = Pubkey::new_unique();
        let key3 = Pubkey::new_unique();
        let (mut account1, mut account2) = make_two_accounts(&key);
        assert!(accounts_equal(&account1, &account2));

        let mut account_expected = account1.clone();
        assert!(accounts_equal(&account1, &account_expected));
        assert!(accounts_equal(&account1, &account2.clone())); // test the clone here

        for field_index in 0..5 {
            for pass in 0..4 {
                if field_index == 0 {
                    if pass == 0 {
                        account1.checked_add_lamports(1).unwrap();
                    } else if pass == 1 {
                        account_expected.checked_add_lamports(1).unwrap();
                        account2.set_lamports(account2.lamports + 1);
                    } else if pass == 2 {
                        account1.set_lamports(account1.lamports + 1);
                    } else if pass == 3 {
                        account_expected.checked_add_lamports(1).unwrap();
                        account2.checked_add_lamports(1).unwrap();
                    }
                } else if field_index == 1 {
                    if pass == 0 {
                        account1.data[0] += 1;
                    } else if pass == 1 {
                        account_expected.data[0] += 1;
                        account2.data_as_mut_slice()[0] = account2.data[0] + 1;
                    } else if pass == 2 {
                        account1.data_as_mut_slice()[0] = account1.data[0] + 1;
                    } else if pass == 3 {
                        account_expected.data[0] += 1;
                        account2.data_as_mut_slice()[0] += 1;
                    }
                } else if field_index == 2 {
                    if pass == 0 {
                        account1.owner = key2;
                    } else if pass == 1 {
                        account_expected.owner = key2;
                        account2.set_owner(key2);
                    } else if pass == 2 {
                        account1.set_owner(key3);
                    } else if pass == 3 {
                        account_expected.owner = key3;
                        account2.owner = key3;
                    }
                } else if field_index == 3 {
                    if pass == 0 {
                        account1.executable = !account1.executable;
                    } else if pass == 1 {
                        account_expected.executable = !account_expected.executable;
                        account2.set_executable(!account2.executable);
                    } else if pass == 2 {
                        account1.set_executable(!account1.executable);
                    } else if pass == 3 {
                        account_expected.executable = !account_expected.executable;
                        account2.executable = !account2.executable;
                    }
                } else if field_index == 4 {
                    if pass == 0 {
                        account1.rent_epoch += 1;
                    } else if pass == 1 {
                        account_expected.rent_epoch += 1;
                        account2.set_rent_epoch(account2.rent_epoch + 1);
                    } else if pass == 2 {
                        account1.set_rent_epoch(account1.rent_epoch + 1);
                    } else if pass == 3 {
                        account_expected.rent_epoch += 1;
                        account2.rent_epoch += 1;
                    }
                }

                let should_be_equal = pass == 1 || pass == 3;
                test_equal(should_be_equal, &account1, &account2, &account_expected);

                // test new_ref
                if should_be_equal {
                    assert!(accounts_equal(
                        &Account::new_ref(
                            account_expected.lamports(),
                            account_expected.data().len(),
                            account_expected.owner()
                        )
                        .borrow(),
                        &AccountSharedData::new_ref(
                            account_expected.lamports(),
                            account_expected.data().len(),
                            account_expected.owner()
                        )
                        .borrow()
                    ));

                    {
                        // test new_data
                        let account1_with_data = Account::new_data(
                            account_expected.lamports(),
                            &account_expected.data()[0],
                            account_expected.owner(),
                        )
                        .unwrap();
                        let account2_with_data = AccountSharedData::new_data(
                            account_expected.lamports(),
                            &account_expected.data()[0],
                            account_expected.owner(),
                        )
                        .unwrap();

                        assert!(accounts_equal(&account1_with_data, &account2_with_data));
                        assert_eq!(
                            account1_with_data.deserialize_data::<u8>().unwrap(),
                            account2_with_data.deserialize_data::<u8>().unwrap()
                        );
                    }

                    // test new_data_with_space
                    assert!(accounts_equal(
                        &Account::new_data_with_space(
                            account_expected.lamports(),
                            &account_expected.data()[0],
                            1,
                            account_expected.owner()
                        )
                        .unwrap(),
                        &AccountSharedData::new_data_with_space(
                            account_expected.lamports(),
                            &account_expected.data()[0],
                            1,
                            account_expected.owner()
                        )
                        .unwrap()
                    ));

                    // test new_ref_data
                    assert!(accounts_equal(
                        &Account::new_ref_data(
                            account_expected.lamports(),
                            &account_expected.data()[0],
                            account_expected.owner()
                        )
                        .unwrap()
                        .borrow(),
                        &AccountSharedData::new_ref_data(
                            account_expected.lamports(),
                            &account_expected.data()[0],
                            account_expected.owner()
                        )
                        .unwrap()
                        .borrow()
                    ));

                    //new_ref_data_with_space
                    assert!(accounts_equal(
                        &Account::new_ref_data_with_space(
                            account_expected.lamports(),
                            &account_expected.data()[0],
                            1,
                            account_expected.owner()
                        )
                        .unwrap()
                        .borrow(),
                        &AccountSharedData::new_ref_data_with_space(
                            account_expected.lamports(),
                            &account_expected.data()[0],
                            1,
                            account_expected.owner()
                        )
                        .unwrap()
                        .borrow()
                    ));
                }
            }
        }
    }
}
