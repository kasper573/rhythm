use godot::classes::file_access::ModeFlags;
use godot::classes::{FileAccess, ProjectSettings};
use godot::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// User data files (settings, high scores) live in Godot's `user://`
/// directory on every platform. Loading tolerates a missing file but
/// panics loudly on an unreadable or invalid one; saving logs instead of
/// panicking so a full disk can't crash a play session.
pub fn load_user_data<T: DeserializeOwned + Default>(file_name: &str) -> T {
    let path = user_path(file_name);
    let location = user_data_location(file_name);
    if !FileAccess::file_exists(&path) {
        return T::default();
    }
    let file = FileAccess::open(&path, ModeFlags::READ)
        .unwrap_or_else(|| panic!("failed to read {location}"));
    let text = file.get_as_text().to_string();
    crate::core::jsonc::parse(&text)
        .unwrap_or_else(|error| panic!("invalid user data file {location}: {error}"))
}

pub fn save_user_data<T: Serialize>(file_name: &str, data: &T) {
    let location = user_data_location(file_name);
    let json = serde_json::to_string_pretty(data).expect("user data always serializes");
    match FileAccess::open(&user_path(file_name), ModeFlags::WRITE) {
        Some(mut file) => {
            file.store_string(&json);
            godot_print!("saved {location}");
        }
        None => godot_error!("failed to save {location}"),
    }
}

/// Where the named file lives on the real filesystem, for log messages.
pub fn user_data_location(file_name: &str) -> String {
    ProjectSettings::singleton()
        .globalize_path(&user_path(file_name))
        .to_string()
}

fn user_path(file_name: &str) -> GString {
    GString::from(&format!("user://{file_name}"))
}
