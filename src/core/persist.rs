use bevy::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::PathBuf;

/// User data files (settings, high scores) live in the OS config
/// directory under `rhythm/`. Loading tolerates a missing file but panics
/// loudly on an unreadable or invalid one; saving logs instead of
/// panicking so a full disk can't crash a play session.
pub fn user_data_path(file_name: &str) -> PathBuf {
    dirs::config_dir()
        .expect("no OS config directory available to store user data")
        .join("rhythm")
        .join(file_name)
}

pub fn load_user_data<T: DeserializeOwned + Default>(file_name: &str) -> T {
    let path = user_data_path(file_name);
    if !path.exists() {
        return T::default();
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    crate::core::jsonc::parse(&text)
        .unwrap_or_else(|error| panic!("invalid user data file {}: {error}", path.display()))
}

pub fn save_user_data<T: Serialize>(file_name: &str, data: &T) {
    let path = user_data_path(file_name);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().expect("user data path has a parent"))?;
        let json = serde_json::to_string_pretty(data).expect("user data always serializes");
        std::fs::write(&path, json)
    };
    match write() {
        Ok(()) => info!("saved {}", path.display()),
        Err(error) => error!("failed to save {}: {error}", path.display()),
    }
}
