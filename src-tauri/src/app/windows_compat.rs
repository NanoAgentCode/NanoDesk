// sqlite-vec 0.1.7 maps its 64-bit popcount to MSVC's __popcnt64 even on
// x86, where that intrinsic is unavailable. Supply its C ABI using Rust's
// portable implementation; do not require the optional POPCNT CPU instruction.
#[cfg(all(target_os = "windows", target_arch = "x86"))]
#[no_mangle]
pub extern "C" fn __popcnt64(value: u64) -> u32 {
    value.count_ones()
}

#[cfg(test)]
mod tests {
    #[test]
    fn sqlite_vec_hamming_counts_both_halves_of_u64() {
        crate::db::register_sqlite_vec_extension();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        for (value, expected) in [(0u64, 0.0), (1, 1.0), (1 << 63, 1.0), (u64::MAX, 64.0), (0xaaaa_aaaa_5555_5555, 32.0)] {
            let result: f64 = conn.query_row(
                "SELECT vec_distance_hamming(vec_bit(?1), vec_bit(?2))",
                rusqlite::params![value.to_le_bytes().as_slice(), [0u8; 8].as_slice()],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(result, expected, "input {value:#018x}");
        }
    }
}
