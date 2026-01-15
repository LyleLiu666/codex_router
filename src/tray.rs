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
    crate::icon::load_icon()
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
                quota: None,
            },
            ProfileSummary {
                name: "alpha".to_string(),
                email: Some("alpha@example.com".to_string()),
                is_current: true,
                quota: None,
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
