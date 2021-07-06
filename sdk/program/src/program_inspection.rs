/// Return the remaining compute units the program may consume
#[inline]
pub fn sol_remaining_compute_units() -> u64 {
    let mut result = u64::MAX;

    #[cfg(target_arch = "bpf")]
    unsafe {
        extern "C" {
            fn sol_remaining_compute_units_(result: *mut u64);
        }
        sol_remaining_compute_units_(&mut result as *mut u64);
    }
    #[cfg(not(target_arch = "bpf"))]
    crate::program_stubs::sol_remaining_compute_units(&mut result);

    result
}
