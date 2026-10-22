// Tauri caches this path in a ctor before main. A missing CRT constructor
// section can compile but leave the cache uninitialized on Win7 targets.
#[test]
fn startup_binary_cache_is_initialized() {
    let cached = tauri::utils::platform::current_exe().expect("Tauri startup binary cache");
    let actual = std::env::current_exe().expect("current test executable");
    assert_eq!(cached.canonicalize().unwrap(), actual.canonicalize().unwrap());
}
