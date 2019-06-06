//! useful extras for Account state
use crate::credit_debit_account::{CreditDebitAccount, KeyedCreditDebitAccount};
use crate::instruction::InstructionError;
use bincode::ErrorKind;

/// Convenience trait to covert bincode errors to instruction errors.
pub trait State<T> {
    fn state(&self) -> Result<T, InstructionError>;
    fn set_state(&mut self, state: &T) -> Result<(), InstructionError>;
}

impl<T> State<T> for CreditDebitAccount
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    fn state(&self) -> Result<T, InstructionError> {
        self.deserialize_data()
            .map_err(|_| InstructionError::InvalidAccountData)
    }
    fn set_state(&mut self, state: &T) -> Result<(), InstructionError> {
        self.serialize_data(state).map_err(|err| match *err {
            ErrorKind::SizeLimit => InstructionError::AccountDataTooSmall,
            _ => InstructionError::GenericError,
        })
    }
}

impl<'a, T> State<T> for KeyedCreditDebitAccount<'a>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    fn state(&self) -> Result<T, InstructionError> {
        self.account.state()
    }
    fn set_state(&mut self, state: &T) -> Result<(), InstructionError> {
        self.account.set_state(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credit_debit_account::CreditDebitAccount;
    use crate::pubkey::Pubkey;

    #[test]
    fn test_account_state() {
        let state = 42u64;

        assert!(CreditDebitAccount::default().set_state(&state).is_err());
        let res = CreditDebitAccount::default().state() as Result<u64, InstructionError>;
        assert!(res.is_err());

        let mut account =
            CreditDebitAccount::new(0, std::mem::size_of::<u64>(), &Pubkey::default());

        assert!(account.set_state(&state).is_ok());
        let stored_state: u64 = account.state().unwrap();
        assert_eq!(stored_state, state);
    }

    #[test]
    fn test_keyed_account_state() {
        let state = 42u64;
        let key0 = Pubkey::new_rand();

        let mut account =
            CreditDebitAccount::new(0, std::mem::size_of::<u64>(), &Pubkey::default());
        let mut keyed_account = KeyedCreditDebitAccount::new(&key0, false, &mut account);

        assert!(keyed_account.set_state(&state).is_ok());
        let stored_state: u64 = keyed_account.state().unwrap();
        assert_eq!(stored_state, state);
    }
}
