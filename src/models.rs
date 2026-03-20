pub(crate) use rustube::video_info::player_response::video_details::Thumbnail;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct DownloaderParams {
    pub format: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MediaMetadata {
    pub title: String,
    pub author: String,
    pub length: u64,
    pub age_restricted: bool,
    pub short_desc: String,
    pub thumbnails: Option<Vec<Thumbnail>>,
    pub id: String,
    pub is_private: bool,
}
#[derive(serde::Deserialize)]
pub struct PlaylistParams {
    pub url: String,
}
