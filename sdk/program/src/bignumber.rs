//! Solana BigNumber
//!
//! Rust-based BigNumber provides safe math for arbitrarily large numbers.
//! Primarily focues on BPF program usage, non-BPF is also supported by
//! backing from the OpenSSL implementation

#[repr(C)]
#[derive(Debug)]
/// BPF structure for sending/receiving BigNumber data to/from syscalls
pub struct FfiBigNumber {
    pub data_len: u64,     // Length of bytes of data
    pub is_negative: bool, // False (0) or True (1) if data is positive or negative
    pub data: u64, // Pointer to the variable length big endian array represneting the value of BigNumber
}

#[cfg(not(target_arch = "bpf"))]
#[derive(AbiExample, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BigNumber {
    data: openssl::bn::BigNum, // For non-BPF we use OpenSSL
}
#[cfg(target_arch = "bpf")]
#[derive(AbiExample, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BigNumber {
    data: Vec<u8>,
    is_negative: bool,
}

impl BigNumber {
    /// Returns a BigNumber with initial value of 0
    pub fn new() -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let bn = openssl::bn::BigNum::new().unwrap();
            Self { data: bn }
        }
        #[cfg(target_arch = "bpf")]
        Self {
            data: vec![0],
            is_negative: false,
        }
    }

    /// Returns the size, in bytes, of BigNum. Typically used in
    /// assessing buffer size needed to fetch the bytes for serialization, etc.
    pub fn size_in_bytes(&self) -> usize {
        #[cfg(not(target_arch = "bpf"))]
        {
            self.data.num_bytes() as usize
        }
        #[cfg(target_arch = "bpf")]
        {
            self.data.len()
        }
    }

    /// Returns a big-endian byte vector representation
    /// of the absolute value of `self`
    pub fn to_vec(&self) -> Vec<u8> {
        #[cfg(not(target_arch = "bpf"))]
        {
            if self.data.num_bytes() == 0 {
                return vec![0u8];
            }
            self.data.to_vec()
        }
        #[cfg(target_arch = "bpf")]
        {
            self.data.clone()
        }
    }

    #[cfg(target_arch = "bpf")]
    pub fn as_ref(&self) -> &[u8] {
        &self.data
    }

    /// Returns a BigNumber with initial value set to a u32 value
    pub fn from_u32(u32_val: u32) -> Self {
        if u32_val == 0 {
            Self::new()
        } else {
            #[cfg(not(target_arch = "bpf"))]
            {
                let bn = openssl::bn::BigNum::from_u32(u32_val).unwrap();
                Self { data: bn }
            }
            #[cfg(target_arch = "bpf")]
            {
                extern "C" {
                    fn sol_bignum_from_u32_(bn_syscall_result: *mut u64, u32_data: u64) -> u64;
                }
                let mut data = Vec::<u8>::with_capacity(4);
                let mut bn_syscall_result = {
                    FfiBigNumber {
                        data: data.as_mut_ptr() as *mut _ as u64,
                        data_len: 4u64,
                        is_negative: false,
                    }
                };
                unsafe {
                    sol_bignum_from_u32_(
                        &mut bn_syscall_result as *mut _ as *mut u64,
                        u32_val as u64,
                    );
                    data.set_len(bn_syscall_result.data_len as usize);
                }
                Self {
                    is_negative: bn_syscall_result.is_negative,
                    data,
                }
            }
        }
    }

    /// Returns a BigNumber from a decimal string
    pub fn from_dec_str(string: &str) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let bn = openssl::bn::BigNum::from_dec_str(string).unwrap();
            Self { data: bn }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_from_dec_str_(
                    in_dec_str_ptr: *const u64,
                    in_len: u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            // The output buffer could be sized to 3.32 bits/digit but this would consume compute units
            let mut data = Vec::<u8>::with_capacity(string.len());
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: string.len() as u64,
                    is_negative: false,
                }
            };

            unsafe {
                sol_bignum_from_dec_str_(
                    string.as_ptr() as *const _ as *const u64,
                    string.len() as u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Construct a BigNumber from a unsigned absolute big endian array of bytes
    pub fn from_bytes(in_data: &[u8]) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            Self {
                data: openssl::bn::BigNum::from_slice(in_data).unwrap(),
            }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_from_bytes_(
                    in_bytes_ptr: *const u64,
                    in_bytes_len: u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let data_size = in_data.len();
            let mut data = Vec::<u8>::with_capacity(data_size);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_size as u64,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_from_bytes_(
                    in_data.as_ptr() as *const _ as *const u64,
                    data_size as u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Flag indicating if negative or not
    pub fn is_negative(&self) -> bool {
        #[cfg(not(target_arch = "bpf"))]
        {
            self.data.is_negative()
        }
        #[cfg(target_arch = "bpf")]
        {
            self.is_negative
        }
    }

    /// Adds self with another BigNumber and returns
    /// a new BigNumber
    pub fn add(&self, rhs: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            bn_done.checked_add(&self.data, &rhs.data).unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_add_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: rhs.data.as_ptr() as *const _ as u64,
                    data_len: rhs.data.len() as u64,
                    is_negative: rhs.is_negative,
                }
            };
            let data_len = std::cmp::max(lhs_ffi_in.data_len, rhs_ffi_in.data_len) + 1;
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_add_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }
    /// Subtracts BigNumber from self and returns
    /// a new BigNumber
    pub fn sub(&self, rhs: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            bn_done.checked_sub(&self.data, &rhs.data).unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_sub_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: rhs.data.as_ptr() as *const _ as u64,
                    data_len: rhs.data.len() as u64,
                    is_negative: rhs.is_negative,
                }
            };
            let data_len = std::cmp::max(lhs_ffi_in.data_len, rhs_ffi_in.data_len) + 1;
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_sub_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Multiplies another BigNumber with self and returns
    /// a new BigNumber
    pub fn mul(&self, rhs: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            let mut ctx = openssl::bn::BigNumContext::new().unwrap();
            bn_done
                .checked_mul(&self.data, &rhs.data, &mut ctx)
                .unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_mul_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: rhs.data.as_ptr() as *const _ as u64,
                    data_len: rhs.data.len() as u64,
                    is_negative: rhs.is_negative,
                }
            };
            let data_len = std::cmp::max(lhs_ffi_in.data_len, rhs_ffi_in.data_len) + 1;
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_mul_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Divides self by another  and returns
    /// a new BigNumber
    pub fn div(&self, rhs: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            let mut ctx = openssl::bn::BigNumContext::new().unwrap();
            bn_done
                .checked_div(&self.data, &rhs.data, &mut ctx)
                .unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_div_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: rhs.data.as_ptr() as *const _ as u64,
                    data_len: rhs.data.len() as u64,
                    is_negative: rhs.is_negative,
                }
            };
            let data_len = std::cmp::max(lhs_ffi_in.data_len, rhs_ffi_in.data_len) + 1;
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_div_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Returns a new BigNumber of self squared
    pub fn sqr(&self) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            let mut ctx = openssl::bn::BigNumContext::new().unwrap();
            bn_done.sqr(&self.data, &mut ctx).unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_sqr_(lhs_ffi_in_addr: *const u64, bn_syscall_result: *mut u64)
                    -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let data_len = lhs_ffi_in.data_len * 2;
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_sqr_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Raise self to exponent
    pub fn exp(&self, exponent: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            let mut ctx = openssl::bn::BigNumContext::new().unwrap();
            bn_done.exp(&self.data, &exponent.data, &mut ctx).unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_exp_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: exponent.data.as_ptr() as *const _ as u64,
                    data_len: exponent.data.len() as u64,
                    is_negative: exponent.is_negative,
                }
            };
            let data_len = lhs_ffi_in.data_len * rhs_ffi_in.data_len;
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_exp_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }
    /// Returns self squared % modulus
    pub fn mod_sqr(&self, modulus: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            let mut ctx = openssl::bn::BigNumContext::new().unwrap();
            bn_done
                .mod_sqr(&self.data, &modulus.data, &mut ctx)
                .unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_mod_sqr_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: modulus.data.as_ptr() as *const _ as u64,
                    data_len: modulus.data.len() as u64,
                    is_negative: modulus.is_negative,
                }
            };
            let data_len = modulus.data.len();
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len as u64,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_mod_sqr_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Compute modular exponentiation (self ^ exponent % modulus) and return the result
    pub fn mod_exp(&self, exponent: &BigNumber, modulus: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            let mut ctx = openssl::bn::BigNumContext::new().unwrap();
            bn_done
                .mod_exp(&self.data, &exponent.data, &modulus.data, &mut ctx)
                .unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_mod_exp_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    mod_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: exponent.data.as_ptr() as *const _ as u64,
                    data_len: exponent.data.len() as u64,
                    is_negative: exponent.is_negative,
                }
            };
            let mod_ffi_in = {
                FfiBigNumber {
                    data: modulus.data.as_ptr() as *const _ as u64,
                    data_len: modulus.data.len() as u64,
                    is_negative: modulus.is_negative,
                }
            };
            let data_len = modulus.data.len();
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len as u64,
                    is_negative: false,
                }
            };

            unsafe {
                sol_bignum_mod_exp_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mod_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Mod multiplier (self * multiplier) % modulus
    pub fn mod_mul(&self, multiplier: &BigNumber, modulus: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            let mut ctx = openssl::bn::BigNumContext::new().unwrap();
            bn_done
                .mod_mul(&self.data, &multiplier.data, &modulus.data, &mut ctx)
                .unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_mod_mul_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    mod_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: multiplier.data.as_ptr() as *const _ as u64,
                    data_len: multiplier.data.len() as u64,
                    is_negative: multiplier.is_negative,
                }
            };
            let mod_ffi_in = {
                FfiBigNumber {
                    data: modulus.data.as_ptr() as *const _ as u64,
                    data_len: modulus.data.len() as u64,
                    is_negative: modulus.is_negative,
                }
            };
            let data_len = modulus.data.len();
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len as u64,
                    is_negative: false,
                }
            };

            unsafe {
                sol_bignum_mod_mul_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mod_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }
    /// Finds the inverse of modulus on self
    pub fn mod_inv(&self, modulus: &BigNumber) -> Self {
        #[cfg(not(target_arch = "bpf"))]
        {
            let mut bn_done = openssl::bn::BigNum::new().unwrap();
            let mut ctx = openssl::bn::BigNumContext::new().unwrap();
            bn_done
                .mod_inverse(&self.data, &modulus.data, &mut ctx)
                .unwrap();
            Self { data: bn_done }
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_mod_inv_(
                    lhs_ffi_in_addr: *const u64,
                    rhs_ffi_in_addr: *const u64,
                    bn_syscall_result: *mut u64,
                ) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            let rhs_ffi_in = {
                FfiBigNumber {
                    data: modulus.data.as_ptr() as *const _ as u64,
                    data_len: modulus.data.len() as u64,
                    is_negative: modulus.is_negative,
                }
            };
            let data_len = modulus.data.len();
            let mut data = Vec::<u8>::with_capacity(data_len as usize);
            let mut bn_syscall_result = {
                FfiBigNumber {
                    data: data.as_mut_ptr() as *mut _ as u64,
                    data_len: data_len as u64,
                    is_negative: false,
                }
            };
            unsafe {
                sol_bignum_mod_inv_(
                    &lhs_ffi_in as *const _ as *const u64,
                    &rhs_ffi_in as *const _ as *const u64,
                    &mut bn_syscall_result as *mut _ as *mut u64,
                );
                data.set_len(bn_syscall_result.data_len as usize);
            }
            Self {
                is_negative: bn_syscall_result.is_negative,
                data,
            }
        }
    }

    /// Log a `BigNum` from a program to the solana logs
    pub fn log(&self) {
        #[cfg(not(target_arch = "bpf"))]
        {
            crate::program_stubs::sol_log(&self.data.to_dec_str().unwrap());
        }
        #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_bignum_log_(lhs_ffi_in_addr: *const u64) -> u64;
            }
            let lhs_ffi_in = {
                FfiBigNumber {
                    data: self.data.as_ptr() as *const _ as u64,
                    data_len: self.data.len() as u64,
                    is_negative: self.is_negative,
                }
            };
            unsafe { sol_bignum_log_(&lhs_ffi_in as *const _ as *const u64) };
        }
    }
}

#[cfg(not(target_arch = "bpf"))]
impl Clone for BigNumber {
    fn clone(&self) -> Self {
        match openssl::bn::BigNum::from_slice(&self.data.to_vec()) {
            Ok(bn) => Self { data: bn },
            Err(_) => panic!(),
        }
    }
}

impl Default for BigNumber {
    fn default() -> Self {
        BigNumber::new()
    }
}
