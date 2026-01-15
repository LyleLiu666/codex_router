use tray_icon::Icon;

pub fn load_icon_data() -> (Vec<u8>, u32, u32) {
    let icon_bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(icon_bytes)
        .expect("failed to load icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    (rgba, width, height)
}



pub fn load_tray_icon() -> Icon {
    let icon_bytes = include_bytes!("../assets/tray_icon.png");
    let image = image::load_from_memory(icon_bytes)
        .expect("failed to load tray icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    Icon::from_rgba(rgba, width, height).expect("failed to build tray icon")
}
