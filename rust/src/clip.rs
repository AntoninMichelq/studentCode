//! Presse-papiers du système, pour « Coller » des menus clic droit (Ctrl+V passe par egui).
pub fn get() -> Option<String> {
    let t = arboard::Clipboard::new().ok()?.get_text().ok()?;
    Some(t.replace("\r\n", "\n"))
}
