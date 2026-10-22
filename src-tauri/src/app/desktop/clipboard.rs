/// Explorer exposes copied files as CF_HDROP rather than web clipboard text.
#[tauri::command]
pub(crate) fn read_assistant_clipboard_paths() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "user32")]
        extern "system" {
            fn OpenClipboard(owner: *mut c_void) -> i32;
            fn CloseClipboard() -> i32;
            fn IsClipboardFormatAvailable(format: u32) -> i32;
            fn GetClipboardData(format: u32) -> *mut c_void;
        }
        #[link(name = "shell32")]
        extern "system" {
            fn DragQueryFileW(drop: *mut c_void, index: u32, buffer: *mut u16, length: u32) -> u32;
        }
        struct Clipboard;
        impl Drop for Clipboard {
            fn drop(&mut self) { unsafe { CloseClipboard(); } }
        }
        unsafe {
            const CF_HDROP: u32 = 15;
            if IsClipboardFormatAvailable(CF_HDROP) == 0 { return Ok(vec![]); }
            if OpenClipboard(std::ptr::null_mut()) == 0 { return Err("剪贴板正被占用，请重试粘贴。".into()); }
            let _clipboard = Clipboard;
            let handle = GetClipboardData(CF_HDROP);
            if handle.is_null() { return Err("读取剪贴板文件失败，请重新复制文件。".into()); }
            let count = DragQueryFileW(handle, u32::MAX, std::ptr::null_mut(), 0);
            let mut paths = Vec::new();
            for index in 0..count {
                let length = DragQueryFileW(handle, index, std::ptr::null_mut(), 0);
                let mut buffer = vec![0u16; length as usize + 1];
                let copied = DragQueryFileW(handle, index, buffer.as_mut_ptr(), buffer.len() as u32);
                if copied == 0 { return Err("读取剪贴板文件路径失败。".into()); }
                paths.push(String::from_utf16(&buffer[..copied as usize]).map_err(|error| error.to_string())?);
            }
            Ok(paths)
        }
    }
    #[cfg(not(windows))]
    { Ok(vec![]) }
}
