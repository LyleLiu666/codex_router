use std::sync::mpsc::Sender;

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::profile::ProfileSummary;

#[derive(Debug, Clone)]
pub enum TrayEvent {
    OpenWindow,
    SwitchProfile(String),
    Quit,
}

const OPEN_ID: &str = "open-window";
const QUIT_ID: &str = "quit";
const PROFILE_PREFIX: &str = "profile:";

#[derive(Debug, Clone, PartialEq, Eq)]
enum MenuEntry {
    Item { id: String, label: String },
    Separator,
}

pub struct TrayHandle {
    tray_icon: TrayIcon,
}

impl TrayHandle {
    pub fn update_profiles(&self, profiles: &[ProfileSummary]) {
        let menu = build_menu(profiles);
        self.tray_icon.set_menu(Some(Box::new(menu)));
    }
}

fn default_icon() -> Icon {
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];

    #[derive(Clone, Copy)]
    struct Point {
        x: f32,
        y: f32,
    }

    fn dist_sq(a: Point, b: Point) -> f32 {
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        dx * dx + dy * dy
    }

    fn dist_sq_to_segment(p: Point, a: Point, b: Point) -> f32 {
        let ab = Point {
            x: b.x - a.x,
            y: b.y - a.y,
        };
        let ap = Point {
            x: p.x - a.x,
            y: p.y - a.y,
        };
        let ab_len_sq = ab.x * ab.x + ab.y * ab.y;
        if ab_len_sq <= f32::EPSILON {
            return dist_sq(p, a);
        }

        let mut t = (ap.x * ab.x + ap.y * ab.y) / ab_len_sq;
        t = t.clamp(0.0, 1.0);
        let closest = Point {
            x: a.x + ab.x * t,
            y: a.y + ab.y * t,
        };
        dist_sq(p, closest)
    }

    fn draw_circle(rgba: &mut [u8], size: usize, center: Point, radius: f32) {
        let r_sq = radius * radius;
        for y in 0..size {
            for x in 0..size {
                let p = Point {
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                };
                if dist_sq(p, center) <= r_sq {
                    let idx = (y * size + x) * 4;
                    rgba[idx] = 0;
                    rgba[idx + 1] = 0;
                    rgba[idx + 2] = 0;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }

    fn draw_line(rgba: &mut [u8], size: usize, start: Point, end: Point, thickness: f32) {
        let threshold = (thickness / 2.0) * (thickness / 2.0);
        for y in 0..size {
            for x in 0..size {
                let p = Point {
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                };
                if dist_sq_to_segment(p, start, end) <= threshold {
                    let idx = (y * size + x) * 4;
                    rgba[idx] = 0;
                    rgba[idx + 1] = 0;
                    rgba[idx + 2] = 0;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }

    let hub = Point { x: 16.0, y: 16.0 };
    let top_left = Point { x: 9.0, y: 11.0 };
    let top_right = Point { x: 23.0, y: 11.0 };
    let bottom = Point { x: 16.0, y: 25.0 };

    draw_line(&mut rgba, size, hub, top_left, 3.0);
    draw_line(&mut rgba, size, hub, top_right, 3.0);
    draw_line(&mut rgba, size, hub, bottom, 3.0);
    draw_circle(&mut rgba, size, hub, 3.2);
    draw_circle(&mut rgba, size, top_left, 2.6);
    draw_circle(&mut rgba, size, top_right, 2.6);
    draw_circle(&mut rgba, size, bottom, 2.6);

    Icon::from_rgba(rgba, size as u32, size as u32)
        .expect("failed to build tray icon")
}

fn menu_id_for_profile(name: &str) -> String {
    format!("{PROFILE_PREFIX}{name}")
}

fn menu_label_for_profile(profile: &ProfileSummary) -> String {
    let base = if profile.is_current {
        format!("* {}", profile.name)
    } else {
        profile.name.clone()
    };

    if let Some(email) = &profile.email {
        format!("{} ({})", base, email)
    } else {
        base
    }
}

fn menu_entries(profiles: &[ProfileSummary]) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    entries.push(MenuEntry::Item {
        id: OPEN_ID.to_string(),
        label: "Open Window".to_string(),
    });

    if !profiles.is_empty() {
        entries.push(MenuEntry::Separator);
        let mut sorted: Vec<&ProfileSummary> = profiles.iter().collect();
        sorted.sort_by(|a, b| {
            b.is_current
                .cmp(&a.is_current)
                .then_with(|| a.name.cmp(&b.name))
        });
        for profile in sorted {
            entries.push(MenuEntry::Item {
                id: menu_id_for_profile(&profile.name),
                label: menu_label_for_profile(profile),
            });
        }
    }

    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item {
        id: QUIT_ID.to_string(),
        label: "Quit".to_string(),
    });
    entries
}

fn tray_event_from_menu_id(id: &MenuId) -> Option<TrayEvent> {
    let id = id.as_ref();
    if id == OPEN_ID {
        return Some(TrayEvent::OpenWindow);
    }
    if id == QUIT_ID {
        return Some(TrayEvent::Quit);
    }
    if let Some(name) = id.strip_prefix(PROFILE_PREFIX) {
        if !name.is_empty() {
            return Some(TrayEvent::SwitchProfile(name.to_string()));
        }
    }
    None
}

fn build_menu(profiles: &[ProfileSummary]) -> Menu {
    let menu = Menu::new();
    for entry in menu_entries(profiles) {
        match entry {
            MenuEntry::Item { id, label } => {
                let item = MenuItem::with_id(id, label, true, None);
                let _ = menu.append(&item);
            }
            MenuEntry::Separator => {
                let separator = PredefinedMenuItem::separator();
                let _ = menu.append(&separator);
            }
        }
    }
    menu
}

pub fn start_tray(sender: Sender<TrayEvent>) -> TrayHandle {
    let menu = build_menu(&[]);
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Codex Router")
        .with_icon(default_icon())
        .with_icon_as_template(true)
        .build()
        .expect("failed to create tray icon");

    std::thread::spawn(move || {
        let menu_rx = MenuEvent::receiver();
        while let Ok(event) = menu_rx.recv() {
            if let Some(tray_event) = tray_event_from_menu_id(&event.id) {
                let should_quit = matches!(tray_event, TrayEvent::Quit);
                if sender.send(tray_event).is_err() {
                    break;
                }
                if should_quit {
                    break;
                }
            }
        }
    });

    TrayHandle { tray_icon }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_event_enum_exists() {
        let _ = TrayEvent::OpenWindow;
    }

    #[test]
    fn builds_default_icon() {
        let _ = default_icon();
    }

    #[test]
    fn maps_menu_id_to_tray_event() {
        let open = tray_event_from_menu_id(&MenuId::new(OPEN_ID));
        assert!(matches!(open, Some(TrayEvent::OpenWindow)));

        let quit = tray_event_from_menu_id(&MenuId::new(QUIT_ID));
        assert!(matches!(quit, Some(TrayEvent::Quit)));
    }

    #[test]
    fn menu_entries_include_profiles() {
        let profiles = vec![
            ProfileSummary {
                name: "beta".to_string(),
                email: None,
                is_current: false,
            },
            ProfileSummary {
                name: "alpha".to_string(),
                email: Some("alpha@example.com".to_string()),
                is_current: true,
            },
        ];

        let entries = menu_entries(&profiles);
        let mut ids = Vec::new();
        let mut labels = Vec::new();

        for entry in entries {
            if let MenuEntry::Item { id, label } = entry {
                if id.starts_with(PROFILE_PREFIX) {
                    ids.push(id);
                    labels.push(label);
                }
            }
        }

        assert_eq!(ids.len(), 2);
        assert_eq!(ids.first(), Some(&menu_id_for_profile("alpha")));
        assert!(labels.iter().any(|label| label.starts_with("* alpha")));
        assert!(labels.iter().any(|label| label.contains("beta")));
    }
}
