use super::guard;

/// Fill six memory counters in KiB: total, available, swap total/free, virtual total/used.
/// Unknown counters are zero. This host capability does not use the storage backend.
///
/// # Safety
/// `values` must point to six writable u64 values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_host_memory(values: *mut u64) {
    if values.is_null() {
        return;
    }
    let counters = guard([0; 6], || {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        [
            system.total_memory() / 1024,
            system.available_memory() / 1024,
            system.total_swap() / 1024,
            system.free_swap() / 1024,
            0,
            0,
        ]
    });
    std::ptr::copy_nonoverlapping(counters.as_ptr(), values, 6);
}
