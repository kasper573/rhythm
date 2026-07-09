use crate::core::platform::platform;
use bevy::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// User data files (settings, high scores) live wherever the installed
/// platform keeps them. Loading tolerates a missing file but panics
/// loudly on an unreadable or invalid one; saving logs instead of
/// panicking so a full disk can't crash a play session.
pub fn load_user_data<T: DeserializeOwned + Default>(file_name: &str) -> T {
    let location = platform().user_data_location(file_name);
    let text = platform()
        .load_user_data(file_name)
        .unwrap_or_else(|error| panic!("failed to read {location}: {error}"));
    let Some(text) = text else {
        return T::default();
    };
    crate::core::jsonc::parse(&text)
        .unwrap_or_else(|error| panic!("invalid user data file {location}: {error}"))
}

pub fn save_user_data<T: Serialize>(file_name: &str, data: &T) {
    let location = platform().user_data_location(file_name);
    let json = serde_json::to_string_pretty(data).expect("user data always serializes");
    match platform().save_user_data(file_name, &json) {
        Ok(()) => info!("saved {location}"),
        Err(error) => error!("failed to save {location}: {error}"),
    }
}
