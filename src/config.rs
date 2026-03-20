use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct Configuration {
    pub max_video_duration_minutes: u16,
    pub limit_duration: bool,
    pub max_audio_duration_minutes: u16,
    pub port: u16,
}

impl Configuration {
    pub fn from_env() -> Self {
        dotenv::dotenv().ok();
        envy::from_env::<Self>().expect("Błąd w konfiguracji .env")
    }
}
