use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect { pub x: i32, pub y: i32, pub width: i32, pub height: i32 }

// Work entirely in physical pixels, including monitors with a negative origin.
pub(super) fn beside(item: Rect, area: Rect, desired: (i32, i32), gap: i32) -> Option<Rect> {
    let right = (item.x + item.width + gap).clamp(area.x, area.x + area.width);
    let left = (item.x - gap).clamp(area.x, area.x + area.width);
    let below = (item.y + item.height + gap).clamp(area.y, area.y + area.height);
    let above = (item.y - gap).clamp(area.y, area.y + area.height);
    let spaces = [
        Rect { x: right, y: area.y, width: area.x + area.width - right, height: area.height },
        Rect { x: area.x, y: area.y, width: left - area.x, height: area.height },
        Rect { x: area.x, y: below, width: area.width, height: area.y + area.height - below },
        Rect { x: area.x, y: area.y, width: area.width, height: above - area.y },
    ];
    let space = spaces.iter().find(|s| s.width >= desired.0 && s.height >= desired.1)
        .or_else(|| spaces.iter().filter(|s| s.width >= 240 && s.height >= 260)
            .max_by_key(|s| (s.width.min(desired.0) as i64) * (s.height.min(desired.1) as i64)))?;
    let width = desired.0.min(space.width);
    let height = desired.1.min(space.height);
    Some(Rect { x: item.x.clamp(space.x, space.x + space.width - width),
        y: item.y.clamp(space.y, space.y + space.height - height), width, height })
}

#[cfg(windows)]
pub(super) fn origin() -> Option<usize> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        let hwnd = GetAncestor(GetForegroundWindow(), GA_ROOTOWNER);
        let mut name = [0u16; 128];
        let len = GetClassNameW(hwnd, &mut name);
        matches!(String::from_utf16_lossy(&name[..len.max(0) as usize]).as_str(), "CabinetWClass" | "ExploreWClass" | "Progman" | "WorkerW")
            .then_some(hwnd.0 as usize)
    }
}

#[cfg(not(windows))]
pub(super) fn origin() -> Option<usize> { None }

#[cfg(windows)]
pub(super) fn selected(path: &Path, origin: usize) -> Result<Rect, String> {
    use windows::Win32::{Foundation::HWND, System::Com::*, UI::Accessibility::*};
    struct Apartment;
    impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
    // This function runs on a blocking worker; UI Automation must not run on the UI thread.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(|e| e.to_string())?;
        let _apartment = Apartment;
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let root = automation.ElementFromHandle(HWND(origin as *mut _)).map_err(|e| e.to_string())?;
        let selected = automation.CreatePropertyCondition(UIA_SelectionItemIsSelectedPropertyId, &true.into()).map_err(|e| e.to_string())?;
        let list_item = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &UIA_ListItemControlTypeId.0.into()).map_err(|e| e.to_string())?;
        let condition = automation.CreateAndCondition(&selected, &list_item).map_err(|e| e.to_string())?;
        let elements = root.FindAll(TreeScope_Descendants, &condition).map_err(|e| e.to_string())?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let mut matches = Vec::new();
        for index in 0..elements.Length().map_err(|e| e.to_string())? {
            let element = elements.GetElement(index).map_err(|e| e.to_string())?;
            let current_name = element.CurrentName().map_err(|e| e.to_string())?.to_string();
            if current_name.eq_ignore_ascii_case(&name) || current_name.eq_ignore_ascii_case(&stem) {
                let bounds = element.CurrentBoundingRectangle().map_err(|e| e.to_string())?;
                if bounds.right > bounds.left && bounds.bottom > bounds.top {
                    matches.push(Rect { x: bounds.left, y: bounds.top, width: bounds.right - bounds.left, height: bounds.bottom - bounds.top });
                }
            }
        }
        if matches.len() == 1 { Ok(matches[0]) }
        else { Err("无法确认所选文件图标的位置，请保持文件选中后重新右键打开文档对话。".into()) }
    }
}

#[cfg(not(windows))]
pub(super) fn selected(_: &Path, _: usize) -> Result<Rect, String> { Err("此定位方式仅支持 Windows。".into()) }

#[cfg(test)]
mod tests {
    use super::*;
    fn overlap(a: Rect, b: Rect) -> bool { a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height }
    #[test]
    fn prefers_right_then_left_without_covering_icon_or_label() {
        let area = Rect { x: 0, y: 0, width: 1920, height: 1200 };
        for x in [20, 950, 1800] {
            let item = Rect { x, y: 400, width: 100, height: 90 };
            let window = beside(item, area, (764, 1100), 12).unwrap();
            assert!(!overlap(item, window));
            assert_eq!(window.width, 764);
            assert_eq!(window.height, 1100);
            if x == 20 { assert_eq!(window.x, 132); }
            if x == 1800 { assert_eq!(window.x + window.width, 1788); }
        }
    }
    #[test]
    fn shrinks_to_available_space_and_handles_negative_monitor_origins() {
        let area = Rect { x: -1280, y: -200, width: 1280, height: 900 };
        let item = Rect { x: -700, y: 200, width: 110, height: 100 };
        let window = beside(item, area, (764, 1100), 12).unwrap();
        assert!(!overlap(item, window));
        assert!(window.x >= area.x && window.y >= area.y);
        assert!(window.x + window.width <= 0 && window.y + window.height <= 700);
        assert!(window.width < 764 || window.height < 1100);
    }
    #[test]
    fn uses_above_or_below_when_neither_side_fits() {
        let area = Rect { x: 0, y: 0, width: 1000, height: 1000 };
        for y in [100, 850] {
            let item = Rect { x: 450, y, width: 100, height: 50 };
            let window = beside(item, area, (800, 300), 12).unwrap();
            assert!(!overlap(item, window));
            assert_eq!((window.width, window.height), (800, 300));
            assert!(window.y >= item.y + item.height + 12 || window.y + window.height <= item.y - 12);
        }
    }
    #[test]
    fn refuses_to_cover_item_when_no_usable_space_remains() {
        let area = Rect { x: 0, y: 0, width: 300, height: 300 };
        assert!(beside(area, area, (764, 1100), 12).is_none());
    }
}
