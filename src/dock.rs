#[cfg(target_os = "macos")]
mod mac {
    use std::sync::mpsc::Sender;
    use std::sync::OnceLock;

    use objc2::rc::Retained;
    use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
    use objc2_app_kit::NSApplicationDidBecomeActiveNotification;
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject};

    use crate::tray::TrayEvent;

    static SENDER: OnceLock<Sender<TrayEvent>> = OnceLock::new();

    pub fn start_dock_observer(tx: Sender<TrayEvent>) {
        if SENDER.set(tx).is_err() {
            tracing::warn!("Dock observer sender already set");
            return;
        }

        unsafe {
            let center = NSNotificationCenter::defaultCenter();
            let observer = AppObserver::new();
            // We leak the observer to keep it alive for the duration of the app
            // In a real app we might want to clean up, but for the main app lifecycle this is fine.
            let observer_ptr = Retained::into_raw(observer);
            let observer_ref = &*observer_ptr;

            center.addObserver_selector_name_object(
                observer_ref,
                objc2::sel!(appDidBecomeActive:),
                Some(NSApplicationDidBecomeActiveNotification),
                None,
            );
        }
    }

    declare_class!(
        struct AppObserver;

        unsafe impl ClassType for AppObserver {
            type Super = NSObject;
            type Mutability = mutability::InteriorMutable;
            const NAME: &'static str = "CodexRouterAppObserver";
        }

        impl DeclaredClass for AppObserver {}

        unsafe impl AppObserver {
            #[method(appDidBecomeActive:)]
            fn app_did_become_active(&self, _notification: &NSNotification) {
                if let Some(tx) = SENDER.get() {
                    let _ = tx.send(TrayEvent::OpenWindow);
                }
            }
        }
    );

    impl AppObserver {
        fn new() -> Retained<Self> {
            unsafe { msg_send_id![Self::class(), new] }
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(not(target_os = "macos"))]
pub fn start_dock_observer(_tx: std::sync::mpsc::Sender<crate::tray::TrayEvent>) {
    // No-op on non-macOS
}
