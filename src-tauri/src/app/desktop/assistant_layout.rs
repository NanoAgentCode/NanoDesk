/// Logical pixels shared by icon placement and expanded-window sizing.
pub(super) const ICON_SIZE: f64 = 88.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    pub fn from_physical(x: i32, y: i32, width: u32, height: u32, scale: f64) -> Self {
        Self { x: x as f64 / scale, y: y as f64 / scale,
            width: width as f64 / scale, height: height as f64 / scale }
    }
}

pub(super) fn assistant_bounds(presentation: &str, current: Bounds, work_area: Option<Bounds>) -> Result<Bounds, &'static str> {
    let (mut width, mut height) = match presentation {
        "icon" => (ICON_SIZE, ICON_SIZE),
        // Composer, attachments and error feedback sit above the persistent icon.
        "input" => (532.0_f64, 220.0_f64),
        "picker" => (532.0, 420.0),
        _ => return Err("unknown assistant presentation"),
    };
    if let Some(area) = work_area {
        width = width.min(area.width);
        height = height.min(area.height);
    }
    let mut x = current.x + current.width - width;
    let mut y = current.y + current.height - height;
    if let Some(area) = work_area {
        x = x.clamp(area.x, area.x + area.width - width);
        y = y.clamp(area.y, area.y + area.height - height);
    }
    Ok(Bounds { x, y, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion_and_collapse_keep_the_icon_at_the_same_screen_position() {
        let icon = Bounds { x: 900.0, y: 600.0, width: ICON_SIZE, height: ICON_SIZE };
        let input = assistant_bounds("input", icon, None).unwrap();
        let picker = assistant_bounds("picker", input, None).unwrap();
        for bounds in [input, picker] {
            assert_eq!(bounds.x + bounds.width, icon.x + icon.width);
            assert_eq!(bounds.y + bounds.height, icon.y + icon.height);
            assert!(bounds.height >= ICON_SIZE + 100.0);
        }
        assert_eq!(assistant_bounds("icon", picker, None).unwrap(), icon);
    }

    #[test]
    fn respects_scaled_work_area_on_a_monitor_left_of_the_primary() {
        let area = Bounds::from_physical(-1920, 0, 1920, 1080, 1.5);
        let icon = Bounds::from_physical(-1900, 10, 132, 132, 1.5);
        let panel = assistant_bounds("picker", icon, Some(area)).unwrap();
        assert!(panel.x >= area.x && panel.y >= area.y);
        assert!(panel.x + panel.width <= area.x + area.width);
        assert!(panel.y + panel.height <= area.y + area.height);
        assert_eq!(panel.width, 532.0);
    }

    #[test]
    fn fits_small_work_areas_without_invalid_clamp_ranges() {
        let area = Bounds { x: 10.0, y: 20.0, width: 320.0, height: 300.0 };
        assert_eq!(assistant_bounds("picker", area, Some(area)).unwrap(), area);
        assert!(assistant_bounds("invalid", area, None).is_err());
    }
}
