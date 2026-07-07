use json_comments::StripComments;
use serde::de::DeserializeOwned;

/// Parses JSON with comments (JSONC), so hand-edited files like
/// `game_config.json` can carry `//` and `/* */` annotations.
pub fn parse<T: DeserializeOwned>(text: &str) -> serde_json::Result<T> {
    serde_json::from_reader(StripComments::new(text.as_bytes()))
}
