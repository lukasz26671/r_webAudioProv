// Używamy tokio::process dla asynchroniczności!
use anyhow::{Context, Result};
use std::env;
use std::process::Stdio;
use tokio::process::Command;

pub struct MediaProcessor;

impl MediaProcessor {
    pub async fn convert_to_mp3(file_stem: &str, input_ext: &str) -> Result<()> {
        let input = format!("{}.{}", file_stem, input_ext);
        let output = format!("{}.mp3", file_stem);

        let ffmpeg_path = env::current_dir()?.join("ffmpeg.exe");

        let status = Command::new(&ffmpeg_path)
            .args(["-i", &input, "-ab", "320k", "-y", &output])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .context(format!("FFmpeg failed. Path: {}", ffmpeg_path.display()))?;

        if !status.success() {
            return Err(anyhow::anyhow!("FFmpeg zakończył się błędem"));
        }
        Ok(())
    }
}
