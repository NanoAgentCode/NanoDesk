# Win7 target compatibility patch

Source: crates.io `ctor` 0.8.0, as locked by NanoDesk before this patch.
Upstream: https://github.com/mmastrac/rust-ctor

Only change: in `src/macros/mod.rs`, treat `target_vendor = "win7"` like
`target_vendor = "pc"` for target validation and the Windows CRT constructor
section `.CRT$XCU`. Rust's `*-win7-windows-msvc` targets have the same Windows
calling convention and CRT constructor format, but a different vendor name.
All original source and license notices are retained.

Required because Tauri's starting-binary cache uses this macro before `main`.
Do not replace it with an empty constructor or spoof Rust's built-in cfg values.
Remove this patch once a compatible upstream dependency is adopted.
