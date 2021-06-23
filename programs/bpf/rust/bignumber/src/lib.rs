//! @brief BigNumber Syscall integration test
extern crate solana_program;
use solana_program::{bignumber::BigNumber, custom_panic_default, entrypoint::SUCCESS, msg};

const LONG_DEC_STRING: &str = "1470463693494555670176851280755142329532258274256991544781479988";
const NEG_LONG_DEC_STRING: &str =
    "-1470463693494555670176851280755142329532258274256991544781479988";

/// BigNumber construction
fn test_constructors() {
    msg!("BigNumber constructors");
    let base_bn_0 = BigNumber::new();
    assert_eq!(base_bn_0.size_in_bytes(), 1);
    assert_eq!(base_bn_0.to_vec(), vec![0u8]);
    let default_0 = BigNumber::default();
    assert_eq!(base_bn_0, default_0);
    let new_bn_0 = BigNumber::from_u32(0);
    assert_eq!(new_bn_0, default_0);
    let max_bn_u32 = BigNumber::from_u32(u32::MAX);
    assert_eq!(max_bn_u32.to_vec(), vec![255, 255, 255, 255]);
    let slice_vec = vec![255u8, 8];
    let bn_test_slice = BigNumber::from_bytes(&slice_vec);
    assert_eq!(bn_test_slice.to_vec(), slice_vec);
    let big_slice_vec = vec![255u8; 1024];
    let bn_test_slice = BigNumber::from_bytes(&big_slice_vec);
    assert_eq!(bn_test_slice.to_vec(), big_slice_vec);
    let bn_from_dec = BigNumber::from_dec_str(LONG_DEC_STRING);
    assert!(!bn_from_dec.is_negative());
    let bn_from_dec = BigNumber::from_dec_str(NEG_LONG_DEC_STRING);
    assert!(bn_from_dec.is_negative());
    // Test endian encoding
    let be_bytes_in = u32::to_be_bytes(256);
    let bn_bytes_out = BigNumber::from_bytes(&be_bytes_in).to_vec();
    assert_eq!(bn_bytes_out, [1, 0]);
    let be_bytes_in: Vec<u8> = vec![0, 0, 0, 0, 0, 0, 1, 0];
    let bn_bytes_out = BigNumber::from_bytes(&be_bytes_in).to_vec();
    assert_eq!(bn_bytes_out, [1, 0]);
}

/// BigNumber simple number and simple maths
fn test_basic_maths() {
    msg!("BigNumber Basic Maths");
    let bn_5 = BigNumber::from_u32(5);
    let bn_258 = BigNumber::from_u32(258);
    let added = bn_5.add(&bn_258);
    assert_eq!(added.to_vec(), [1, 7]);
    let subed = bn_5.sub(&bn_258);
    assert_eq!(subed.to_vec(), vec![253]);
    assert!(subed.is_negative());
    let muled = bn_5.mul(&bn_5);
    assert_eq!(muled.to_vec(), vec![25]);
    let bn_300 = BigNumber::from_u32(300);
    let bn_10 = BigNumber::from_u32(10);
    let dived = bn_300.div(&bn_10);
    assert_eq!(dived.to_vec(), vec![30]);
}

/// BigNumber bigger numbers and complex maths
fn test_complex_maths() {
    msg!("BigNumber Complex Maths");
    let bn_arg1 = BigNumber::from_u32(300);
    let sqr_res = bn_arg1.sqr();
    assert_eq!(sqr_res.to_vec(), vec![1, 95, 144]);
    let bn_arg2 = BigNumber::from_u32(8);
    let bn_arg3 = BigNumber::from_u32(2);
    let exp_res = bn_arg2.exp(&bn_arg3);
    assert_eq!(exp_res.to_vec(), vec![64]);
    let bn_arg1 = BigNumber::from_u32(300);
    let bn_arg2 = BigNumber::from_u32(11);
    let mod_sqr = bn_arg1.mod_sqr(&bn_arg2);
    assert_eq!(mod_sqr.to_vec(), vec![9]);
    let bn_arg1 = BigNumber::from_u32(300);
    let bn_arg2 = BigNumber::from_u32(11);
    let bn_arg3 = BigNumber::from_u32(7);
    let mod_exp = bn_arg1.mod_exp(&bn_arg2, &bn_arg3);
    assert_eq!(mod_exp.to_vec(), vec![6]);
    let mod_mul = bn_arg1.mod_mul(&bn_arg2, &bn_arg3);
    assert_eq!(mod_mul.to_vec(), vec![3]);
    let bn_arg1 = BigNumber::from_u32(415);
    let bn_arg2 = BigNumber::from_u32(7);
    let mod_inv = bn_arg1.mod_inv(&bn_arg2);
    assert_eq!(mod_inv.to_vec(), vec![4]);
}

fn test_output_logging() {
    let bn_from_dec = BigNumber::from_dec_str(LONG_DEC_STRING);
    bn_from_dec.log();
    let bn_from_dec = BigNumber::from_dec_str(NEG_LONG_DEC_STRING);
    bn_from_dec.log();
}

#[no_mangle]
pub extern "C" fn entrypoint(_input: *mut u8) -> u64 {
    msg!("bignum");
    test_constructors();
    test_basic_maths();
    test_complex_maths();
    test_output_logging();

    SUCCESS
}

custom_panic_default!();

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_basic_constructors_pass() {
        test_constructors();
    }
    #[test]
    fn test_simple_maths_pass() {
        test_basic_maths();
    }
    #[test]
    fn test_complex_maths_pass() {
        test_complex_maths();
    }

    #[test]
    fn test_logging() {
        test_output_logging();
    }
}
