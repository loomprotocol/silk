/**
 * @brief Example C based BPF program that tests the BugNum syscalls
 */
#include <solana_bignumber.h>
#include <solana_sdk.h>

static const char LONG_DEC_STRING[] =
    "1470463693494555670176851280755142329532258274256991544781479988";
static const char NEG_LONG_DEC_STRING[] =
    "-1470463693494555670176851280755142329532258274256991544781479988";

void test_constructors() {
  sol_log("BigNumber constructors");

  {
    SolBigNumber bn_0 = sol_bignum_from_u32(0);
    SolBigNumber bn_u32 = sol_bignum_init(1);
    sol_assert(sol_bignum_equal(&bn_0, &bn_u32));
    sol_bignum_deinit(&bn_u32);
    sol_bignum_deinit(&bn_0);
  }
  {
    SolBigNumber max_bn_u32 = sol_bignum_from_u32(UINT32_MAX);
    uint8_t expected[] = {255, 255, 255, 255};
    sol_assert(!sol_memcmp(max_bn_u32.data_addr, &expected, 4));
    sol_bignum_deinit(&max_bn_u32);
  }
  {
    uint8_t bytes[] = {255, 255, 255, 255, 255, 255, 255, 255};
    SolBigNumber bn = sol_bignum_from_bytes(bytes, SOL_ARRAY_SIZE(bytes));
    sol_assert(!sol_memcmp(bn.data_addr, bytes, 8));
    sol_bignum_deinit(&bn);
  }
  {
    uint8_t *bytes = sol_calloc(1024, 1);
    sol_memset(bytes, 255, 1024);
    SolBigNumber bn_long = sol_bignum_from_bytes(bytes, 1024);
    sol_assert(!sol_memcmp(bn_long.data_addr, bytes, 1024));
    sol_bignum_deinit(&bn_long);
  }
  {
    SolBigNumber bn_from_dec =
        sol_bignum_from_dec_str(LONG_DEC_STRING, sol_strlen(LONG_DEC_STRING));
    sol_assert(!bn_from_dec.is_negative);
    sol_bignum_deinit(&bn_from_dec);
  }
  {
    SolBigNumber bn_from_dec = sol_bignum_from_dec_str(
        NEG_LONG_DEC_STRING, sol_strlen(NEG_LONG_DEC_STRING));
    sol_assert(bn_from_dec.is_negative);
    sol_bignum_deinit(&bn_from_dec);
  }
}

void test_basic_maths() {
  sol_log("BigNumber Basic Maths");

  SolBigNumber bn_5 = sol_bignum_from_u32(5);
  SolBigNumber bn_258 = sol_bignum_from_u32(258);

  SolBigNumber added = sol_bignum_add(&bn_5, &bn_258);
  uint8_t add_expected[] = {1, 7};
  sol_assert(!sol_memcmp(added.data_addr, add_expected, 2));
  sol_bignum_deinit(&added);

  SolBigNumber subed = sol_bignum_sub(&bn_5, &bn_258);
  uint8_t sub_expected[] = {253};
  sol_assert(!sol_memcmp(subed.data_addr, sub_expected, 1));
  sol_assert(subed.is_negative);
  sol_bignum_deinit(&subed);

  SolBigNumber muled = sol_bignum_mul(&bn_5, &bn_5);
  uint8_t mul_expected[] = {25};
  sol_assert(!sol_memcmp(muled.data_addr, mul_expected, 1));
  sol_bignum_deinit(&muled);

  SolBigNumber bn_300 = sol_bignum_from_u32(300);
  SolBigNumber bn_10 = sol_bignum_from_u32(10);
  SolBigNumber dived = sol_bignum_div(&bn_300, &bn_10);
  uint8_t div_expected[] = {30};
  sol_assert(!sol_memcmp(dived.data_addr, div_expected, 1));
  sol_bignum_deinit(&bn_300);
  sol_bignum_deinit(&bn_10);
  sol_bignum_deinit(&dived);

  sol_bignum_deinit(&bn_258);
  sol_bignum_deinit(&bn_5);
}

void test_complex_maths() {
  sol_log("BigNumber Complex Maths");
  {
    SolBigNumber bn_arg1 = sol_bignum_from_u32(300);
    SolBigNumber sqr_res = sol_bignum_sqr(&bn_arg1);
    uint8_t sqr_expected[] = {1, 95, 144};
    sol_assert(!sol_memcmp(sqr_res.data_addr, sqr_expected, 3));
    sol_bignum_deinit(&bn_arg1);
    sol_bignum_deinit(&sqr_res);
  }
  {
    SolBigNumber bn_arg1 = sol_bignum_from_u32(8);
    SolBigNumber bn_arg2 = sol_bignum_from_u32(2);
    SolBigNumber exp_res = sol_bignum_exp(&bn_arg1, &bn_arg2);
    uint8_t exp_expected[] = {64};
    sol_assert(!sol_memcmp(exp_res.data_addr, exp_expected, 1));
    sol_bignum_deinit(&bn_arg2);
    sol_bignum_deinit(&bn_arg1);
    sol_bignum_deinit(&exp_res);
  }
  {
    SolBigNumber bn_arg1 = sol_bignum_from_u32(300);
    SolBigNumber bn_arg2 = sol_bignum_from_u32(11);
    SolBigNumber mod_sqr = sol_bignum_mod_sqr(&bn_arg1, &bn_arg2);
    uint8_t mod_sqr_expected[] = {9};
    sol_assert(!sol_memcmp(mod_sqr.data_addr, mod_sqr_expected, 1));
    sol_bignum_deinit(&bn_arg2);
    sol_bignum_deinit(&bn_arg1);
    sol_bignum_deinit(&mod_sqr);
  }
  {
    SolBigNumber bn_arg1 = sol_bignum_from_u32(300);
    SolBigNumber bn_arg2 = sol_bignum_from_u32(11);
    SolBigNumber bn_arg3 = sol_bignum_from_u32(7);
    SolBigNumber mod_exp = sol_bignum_mod_exp(&bn_arg1, &bn_arg2, &bn_arg3);
    uint8_t mod_exp_expected[] = {6};
    sol_assert(!sol_memcmp(mod_exp.data_addr, mod_exp_expected, 1));
    sol_bignum_deinit(&bn_arg3);
    sol_bignum_deinit(&bn_arg2);
    sol_bignum_deinit(&bn_arg1);
  }
  {
    SolBigNumber bn_arg1 = sol_bignum_from_u32(300);
    SolBigNumber bn_arg2 = sol_bignum_from_u32(11);
    SolBigNumber bn_arg3 = sol_bignum_from_u32(7);
    SolBigNumber mod_mul = sol_bignum_mod_mul(&bn_arg1, &bn_arg2, &bn_arg3);
    uint8_t mod_mul_expected[] = {3};
    sol_assert(!sol_memcmp(mod_mul.data_addr, mod_mul_expected, 1));
    sol_bignum_deinit(&bn_arg3);
    sol_bignum_deinit(&bn_arg2);
    sol_bignum_deinit(&bn_arg1);
    sol_bignum_deinit(&mod_mul);
  }
  {
    SolBigNumber bn_arg1 = sol_bignum_from_u32(415);
    SolBigNumber bn_arg2 = sol_bignum_from_u32(7);
    SolBigNumber mod_inv = sol_bignum_mod_inv(&bn_arg1, &bn_arg2);
    uint8_t mod_inv_expected[] = {4};
    sol_assert(!sol_memcmp(mod_inv.data_addr, mod_inv_expected, 1));
    sol_bignum_deinit(&bn_arg2);
    sol_bignum_deinit(&bn_arg1);
    sol_bignum_deinit(&mod_inv);
  }
}

void test_output_logging() {
  {
    SolBigNumber bn =
        sol_bignum_from_dec_str(LONG_DEC_STRING, sol_strlen(LONG_DEC_STRING));
    sol_bignum_log(&bn);
  }
  {
    SolBigNumber bn = sol_bignum_from_dec_str(NEG_LONG_DEC_STRING,
                                              sol_strlen(NEG_LONG_DEC_STRING));
    sol_bignum_log(&bn);
  }
}

extern uint64_t entrypoint(const uint8_t *input) {
  sol_log("bignum");

  test_constructors();
  test_basic_maths();
  test_complex_maths();
  test_output_logging();

  return SUCCESS;
}
