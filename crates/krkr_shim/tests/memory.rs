#[test]
fn memory_query_initializes_all_counters() {
    let mut values = [u64::MAX; 6];
    unsafe {
        krkr_shim::ffi::krkr_host_memory(values.as_mut_ptr());
    }
    assert!(values.iter().all(|v| *v != u64::MAX));
    assert!(values[1] <= values[0]);
    assert!(values[3] <= values[2]);
    assert_eq!(&values[4..], &[0, 0]);
}
