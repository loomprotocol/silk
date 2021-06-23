#pragma once
/**
 * @brief Solana C-based BPF program BigNumber functions and types
 */

#ifdef __cplusplus
extern "C" {
#endif

#include "solana_sdk.h"

/**
 * Return the maximum of two uint64_t numbers
 */
uint64_t max_u64(uint64_t lhs, uint64_t rhs) {
  if (lhs > rhs) {
    return lhs;
  } else {
    return rhs;
  }
}

/**
 * BigNumber structure required by tghe BigNum syscalls
 */
typedef struct {
  uint64_t
      data_len; /** Length of the data, in bytes, pointed to by data_addr */
  uint64_t
      is_negative;    /** Indicator if number is positive (0) or negative (1) */
  uint8_t *data_addr; /** Pointer to the array of big endian bytes representing
                          BigNumber value */
} SolBigNumber;

/**
 * Initialize a SolBigNumber, inclues allocating memory for the data, must call
 * `sol_bignum_deinit` to free that memory
 */
static SolBigNumber sol_bignum_init(uint64_t data_len) {
  SolBigNumber bn;
  bn.is_negative = false;
  bn.data_len = data_len;
  bn.data_addr = sol_calloc(data_len, 1);
  return bn;
}

/**
 * Deinitalize a SolBigNumber, free's the data memory
 */
static void sol_bignum_deinit(SolBigNumber *bn_ptr) {
  bn_ptr->is_negative = false;
  bn_ptr->data_len = 0;
  sol_free(bn_ptr->data_addr);
  bn_ptr->data_addr = 0;
}

/**
 * sol_bignum_equal
 *
 * @param lhs_bn_ptr Address of the lhs SolBigNumber
 * @param rhs_bn_ptr Address of the rhs SolBigNumber
 */
static bool sol_bignum_equal(SolBigNumber *lhs_bn_ptr,
                             SolBigNumber *rhs_bn_ptr) {
  if (lhs_bn_ptr->is_negative == rhs_bn_ptr->is_negative) {
    if (lhs_bn_ptr->data_len == rhs_bn_ptr->data_len) {
      if (!sol_memcmp(lhs_bn_ptr->data_addr, rhs_bn_ptr->data_addr,
                      lhs_bn_ptr->data_len)) {
        return true;
      }
    }
  }
  return false;
}

/**
 * sol_bignum_from_u32
 *
 * @param val_u32 unsigned 32 bit value to assign to the new object
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_from_u32(const uint64_t val_u32) {
  uint64_t sol_bignum_from_u32_(SolBigNumber * out_bn_ptr,
                                const uint64_t val_u32);

  SolBigNumber bn = sol_bignum_init(4);
  sol_bignum_from_u32_(&bn, val_u32);
  return bn;
}

/**
 * sol_bignum_from_dec_str
 *
 * @param in_dec_str_ptr address of decimal string
 * @param in_len number of byytes in the decimal string
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_from_dec_str(const char *in_dec_str_ptr,
                                            uint64_t str_len) {
  uint64_t sol_bignum_from_dec_str_(const uint64_t *dec_str_ptr,
                                    uint64_t str_len, SolBigNumber *out_bn_ptr);

  SolBigNumber bn = sol_bignum_init(str_len);
  sol_bignum_from_dec_str_(in_dec_str_ptr, str_len, &bn);
  return bn;
}

/**
 * sol_bignum_from_bytes
 * @param bytes_ptr array of bytes
 * @param bytes_len length of bytes
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_from_bytes(const uint8_t *bytes_ptr,
                                          uint64_t bytes_len) {
  uint64_t sol_bignum_from_bytes_(const uint64_t *bytes_ptr, uint64_t bytes_len,
                                  SolBigNumber *out_bn_ptr);

  SolBigNumber bn = sol_bignum_init(bytes_len);
  sol_bignum_from_bytes_(bytes_ptr, bytes_len, &bn);
  return bn;
}

/**
 * sol_bignum_add
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_rhs_ptr address of the rhs SolBigNumber structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_add(const SolBigNumber *in_bn_lhs_ptr,
                                   const SolBigNumber *in_bn_rhs_ptr) {
  uint64_t sol_bignum_add_(const SolBigNumber *in_bn_lhs_ptr,
                           const SolBigNumber *in_bn_rhs_ptr,
                           SolBigNumber *out_bn_ptr);
  uint64_t out_data_len =
      max_u64(in_bn_lhs_ptr->data_len, in_bn_rhs_ptr->data_len) + 1;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_add_(in_bn_lhs_ptr, in_bn_rhs_ptr, &bn);
  return bn;
}

/**
 * sol_bignum_sub
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_rhs_ptr address of the rhs SolBigNumber structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_sub(const SolBigNumber *in_bn_lhs_ptr,
                                   const SolBigNumber *in_bn_rhs_ptr) {
  uint64_t sol_bignum_sub_(const SolBigNumber *in_bn_lhs_ptr,
                           const SolBigNumber *in_bn_rhs_ptr,
                           SolBigNumber *out_bn_ptr);
  uint64_t out_data_len =
      max_u64(in_bn_lhs_ptr->data_len, in_bn_rhs_ptr->data_len) + 1;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_sub_(in_bn_lhs_ptr, in_bn_rhs_ptr, &bn);
  return bn;
}
/**
 * sol_bignum_mul
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_rhs_ptr address of the rhs SolBigNumber structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_mul(const SolBigNumber *in_bn_lhs_ptr,
                                   const SolBigNumber *in_bn_rhs_ptr) {
  uint64_t sol_bignum_mul_(const SolBigNumber *in_bn_lhs_ptr,
                           const SolBigNumber *in_bn_rhs_ptr,
                           SolBigNumber *out_bn_ptr);
  uint64_t out_data_len =
      max_u64(in_bn_lhs_ptr->data_len, in_bn_rhs_ptr->data_len) + 1;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_mul_(in_bn_lhs_ptr, in_bn_rhs_ptr, &bn);
  return bn;
}
/**
 * sol_bignum_div
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_rhs_ptr address of the rhs SolBigNumber structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_div(const SolBigNumber *in_bn_lhs_ptr,
                                   const SolBigNumber *in_bn_rhs_ptr) {
  uint64_t sol_bignum_div_(const SolBigNumber *in_bn_lhs_ptr,
                           const SolBigNumber *in_bn_rhs_ptr,
                           SolBigNumber *out_bn_ptr);
  uint64_t out_data_len =
      max_u64(in_bn_lhs_ptr->data_len, in_bn_rhs_ptr->data_len) + 1;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_div_(in_bn_lhs_ptr, in_bn_rhs_ptr, &bn);
  return bn;
}

/**
 * sol_bignum_sqr
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param out_bn_ptr address of the SolBigNumber result structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_sqr(const SolBigNumber *in_bn_lhs_ptr) {
  uint64_t sol_bignum_sqr_(const SolBigNumber *in_bn_lhs_ptr,
                           SolBigNumber *out_bn_ptr);
  uint64_t out_data_len = in_bn_lhs_ptr->data_len * 2;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_sqr_(in_bn_lhs_ptr, &bn);
  return bn;
}

/**
 * sol_bignum_exp
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_rhs_ptr address of the rhs SolBigNumber structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_exp(const SolBigNumber *in_bn_lhs_ptr,
                                   const SolBigNumber *in_bn_rhs_ptr) {
  uint64_t sol_bignum_exp_(const SolBigNumber *in_bn_lhs_ptr,
                           const SolBigNumber *in_bn_rhs_ptr,
                           SolBigNumber *out_bn_ptr);
  uint64_t out_data_len = in_bn_lhs_ptr->data_len + in_bn_rhs_ptr->data_len;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_exp_(in_bn_lhs_ptr, in_bn_rhs_ptr, &bn);
  return bn;
}

/**
 * sol_bignum_mod_sqr
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_mod_ptr address of the modulus SolBigNumber structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_mod_sqr(const SolBigNumber *in_bn_lhs_ptr,
                                       const SolBigNumber *in_bn_mod_ptr) {
  uint64_t sol_bignum_mod_sqr_(const SolBigNumber *in_bn_lhs_ptr,
                               const SolBigNumber *in_bn_rhs_ptr,
                               SolBigNumber *out_bn_ptr);
  uint64_t out_data_len = in_bn_mod_ptr->data_len;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_mod_sqr_(in_bn_lhs_ptr, in_bn_mod_ptr, &bn);
  return bn;
}

/**
 * sol_bignum_mod_exp
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_rhs_ptr address of the rhs SolBigNumber structure
 * @param in_bn_mod_ptr address of the modulus SolBigNumber structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_mod_exp(const SolBigNumber *in_bn_lhs_ptr,
                                       const SolBigNumber *in_bn_rhs_ptr,
                                       const SolBigNumber *in_bn_mod_ptr) {
  uint64_t sol_bignum_mod_exp_(
      const SolBigNumber *in_bn_lhs_ptr, const SolBigNumber *in_bn_rhs_ptr,
      const SolBigNumber *in_bn_mod_ptr, SolBigNumber *out_bn_ptr);
  uint64_t out_data_len = in_bn_mod_ptr->data_len;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_mod_exp_(in_bn_lhs_ptr, in_bn_rhs_ptr, in_bn_mod_ptr, &bn);
  return bn;
}

/**
 * sol_bignum_mod_mul
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_rhs_ptr address of the rhs SolBigNumber structure
 * @param in_bn_mod_ptr address of the rhs SolBigNumber structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_mod_mul(const SolBigNumber *in_bn_lhs_ptr,
                                       const SolBigNumber *in_bn_rhs_ptr,
                                       const SolBigNumber *in_bn_mod_ptr) {
  uint64_t sol_bignum_mod_mul_(
      const SolBigNumber *in_bn_lhs_ptr, const SolBigNumber *in_bn_rhs_ptr,
      const SolBigNumber *in_bn_mod_ptr, SolBigNumber *out_bn_ptr);
  uint64_t out_data_len = in_bn_mod_ptr->data_len;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_mod_mul_(in_bn_lhs_ptr, in_bn_rhs_ptr, in_bn_mod_ptr, &bn);
  return bn;
}

/**
 * sol_bignum_mod_inv
 * @param in_bn_lhs_ptr address of the lhs SolBigNumber structure
 * @param in_bn_rhs_ptr address of the rhs SolBigNumber structure
 * @param out_bn_ptr address of the SolBigNumber result structure
 * @return SolBigNumber result
 */
static SolBigNumber sol_bignum_mod_inv(const SolBigNumber *in_bn_lhs_ptr,
                                       const SolBigNumber *in_bn_mod_ptr) {
  uint64_t sol_bignum_mod_inv_(const SolBigNumber *in_bn_lhs_ptr,
                               const SolBigNumber *in_bn_rhs_ptr,
                               SolBigNumber *out_bn_ptr);
  uint64_t out_data_len = in_bn_mod_ptr->data_len;
  SolBigNumber bn = sol_bignum_init(out_data_len);
  sol_bignum_mod_inv_(in_bn_lhs_ptr, in_bn_mod_ptr, &bn);
  return bn;
}

/** Logs a SolBigNumber
 * @param in_bn_ptr address of the SolBigNumber to log
 */
static void sol_bignum_log(const SolBigNumber *in_bn_ptr) {
  uint64_t sol_bignum_log_(const SolBigNumber *in_bn_ptr);

  sol_bignum_log_(in_bn_ptr);
}

#ifdef __cplusplus
}
#endif
