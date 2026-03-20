use crate::models::{MediaMetadata, Thumbnail};
use crate::services::processor::MediaProcessor;
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use youtube_dl::{YoutubeDl, YoutubeDlOutput};

use dashmap::DashMap;
use std::fs::File;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

#[derive(Clone)]
pub struct YoutubeService {
    pub ytdlp_path: PathBuf,
    pub temp_dir: PathBuf,
}

impl YoutubeService {
    pub fn new(ytdlp_path: PathBuf, temp_dir: PathBuf) -> Self {
        Self {
            ytdlp_path,
            temp_dir,
        }
    }

    pub fn extract_id(url: &str) -> Option<String> {
        let idx = url.find("v=")?;
        let id: String = url
            .chars()
            .skip(idx + 2)
            .take_while(|&c| c != '&' && c != ' ')
            .collect();
        (id.len() == 11).then_some(id)
    }

    pub async fn get_metadata_resp(&self, url: &str) -> Result<MediaMetadata> {
        let output = YoutubeDl::new(url)
            .youtube_dl_path(&self.ytdlp_path)
            .flat_playlist(true)
            .run_async()
            .await
            .context("Błąd komunikacji z yt-dlp.exe")?;

        match output {
            YoutubeDlOutput::SingleVideo(video) => Ok(MediaMetadata {
                title: video.title.unwrap_or_else(|| "Brak tytułu".to_string()),
                author: video.uploader.unwrap_or_else(|| "Nieznany".to_string()),
                short_desc: video.description.unwrap_or_default(),
                id: video.id,
                length: video.duration.and_then(|d| d.as_u64()).unwrap_or(0),
                age_restricted: false,
                is_private: false,
                thumbnails: video.thumbnails.map(|t| {
                    t.into_iter()
                        .map(|thumb| Thumbnail {
                            url: thumb.url.unwrap_or_default(),
                            width: thumb.width.unwrap_or(0f64) as u64,
                            height: thumb.height.unwrap_or(0f64) as u64,
                        })
                        .collect()
                }),
            }),
            _ => Err(anyhow!("Podany URL nie prowadzi do pojedynczego filmu.")),
        }
    }

    pub async fn download_audio(&self, url: &str) -> Result<PathBuf> {
        let metadata = self.get_metadata_resp(url).await?;
        let clean_title = self.clean_filename(&metadata.title);
        let file_stem = format!("{}_{}", clean_title, metadata.id);

        YoutubeDl::new(url)
            .youtube_dl_path(&self.ytdlp_path)
            .extract_audio(true)
            .format("bestaudio")
            .output_template(format!("{}.%(ext)s", file_stem))
            .download_to_async("./")
            .await
            .context("Błąd podczas download_to_async")?;

        let extensions = ["m4a", "opus", "webm", "mp3"];
        let mut found_ext = None;
        for ext in extensions {
            let path = format!("{}.{}", file_stem, ext);
            if Path::new(&path).exists() {
                found_ext = Some(ext.to_string());
                break;
            }
        }

        let ext = found_ext.ok_or_else(|| anyhow!("Nie znaleziono pliku po download_to_async"))?;

        MediaProcessor::convert_to_mp3(&file_stem, &ext).await?;

        let final_mp3 = self.temp_dir.join(format!("{}.mp3", file_stem));

        if !self.temp_dir.exists() {
            fs::create_dir_all(&self.temp_dir)?;
        }

        fs::rename(format!("{}.mp3", file_stem), &final_mp3)?;
        let _ = fs::remove_file(format!("{}.{}", file_stem, ext));

        Ok(final_mp3)
    }

    pub async fn download_video(&self, url: &str) -> Result<PathBuf> {
        let metadata = self.get_metadata_resp(url).await?;
        let clean_title = self.clean_filename(&metadata.title);
        let file_stem = format!("{}_{}", clean_title, metadata.id);
        let target_path = self.temp_dir.join(format!("{}.mp4", file_stem));

        if target_path.exists() {
            return Ok(target_path);
        }

        YoutubeDl::new(url)
            .youtube_dl_path(&self.ytdlp_path)
            .format("bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best")
            .output_template(format!("{}.%(ext)s", file_stem))
            .download_to_async("./")
            .await
            .context("Błąd yt-dlp podczas pobierania wideo")?;

        let expected_file = format!("{}.mp4", file_stem);
        let fallback_file = format!("{}.mkv", file_stem);

        if Path::new(&expected_file).exists() {
            if !self.temp_dir.exists() {
                fs::create_dir_all(&self.temp_dir)?;
            }

            fs::rename(&expected_file, &target_path)
                .context("Błąd podczas przenoszenia gotowego MP4 do folderu temp")?;
        } else if Path::new(&fallback_file).exists() {
            let mkv_target = self.temp_dir.join(format!("{}.mkv", file_stem));
            fs::rename(&fallback_file, &mkv_target)?;
            return Ok(mkv_target);
        } else {
            return Err(anyhow!(
                "Nie znaleziono pliku wideo po pobraniu (szukano: {})",
                expected_file
            ));
        }

        Ok(target_path)
    }

    pub async fn download_full_playlist(&self, url: &str, format: &str) -> Result<PathBuf> {
        let playlist_id = "playlist_tmp";
        let playlist_dir = self.temp_dir.join(playlist_id);
        if !playlist_dir.exists() {
            fs::create_dir_all(&playlist_dir)?;
        }

        let mut dl = YoutubeDl::new(url);
        dl.youtube_dl_path(&self.ytdlp_path)
            .extra_arg("--yes-playlist");

        if format == "mp3" {
            dl.extract_audio(true)
                .format("bestaudio")
                .output_template(format!(
                    "{}/%(playlist_index)s_%(title)s.%(ext)s",
                    playlist_id
                ));
        } else {
            dl.format("bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]")
                .output_template(format!(
                    "{}/%(playlist_index)s_%(title)s.%(ext)s",
                    playlist_id
                ));
        }

        dl.download_to_async("./temp")
            .await
            .context("Błąd podczas pobierania playlisty przez yt-dlp")?;

        Ok(playlist_dir)
    }

    pub async fn download_playlist_scalable(&self, url: &str, format: &str) -> Result<PathBuf> {
        let output = YoutubeDl::new(url)
            .youtube_dl_path(&self.ytdlp_path)
            .flat_playlist(true)
            .run_async()
            .await?;

        let entries = match output {
            YoutubeDlOutput::Playlist(p) => p.entries.unwrap_or_default(),
            _ => return Err(anyhow!("To nie jest playlista")),
        };

        let total_count = entries.len();
        println!(
            "Rozpoczynam pobieranie playlisty: {} elementów",
            total_count
        );
        let progress = Arc::new(AtomicUsize::new(0));
        let format = format.to_string();

        let results = futures::stream::iter(entries)
            .map(|entry| {
                let format_clone = format.to_string();
                let yt_service = self.clone();
                let id = entry.id.clone();

                let progress_clone = Arc::clone(&progress);

                async move {
                    let current = progress_clone.fetch_add(1, Ordering::SeqCst) + 1;
                    println!(
                        "[{}/{}] Rozpoczynam pobieranie: {}",
                        current, total_count, id
                    );

                    let video_url = format!("https://www.youtube.com/watch?v={}", id);

                    if format_clone == "mp3" {
                        yt_service.download_audio(&video_url).await?;
                    } else {
                        yt_service.download_video(&video_url).await?;
                    }

                    println!("[{}/{}] ZAKOŃCZONO: {}", current, total_count, id);
                    Ok::<(), anyhow::Error>(())
                }
            })
            .buffer_unordered(3)
            .collect::<Vec<_>>()
            .await;
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        println!(
            "Zakończono: {}/{} pobranych pomyślnie",
            success_count, total_count
        );

        Ok(self.temp_dir.join("playlist_tmp"))
    }

    pub async fn download_full_playlist_fast(&self, url: &str) -> Result<PathBuf> {
        let output = YoutubeDl::new(url)
            .youtube_dl_path(&self.ytdlp_path)
            .flat_playlist(true)
            .run_async()
            .await
            .context("Nie udało się pobrać metadanych playlisty")?;

        let playlist_title = match output {
            YoutubeDlOutput::Playlist(p) => {
                p.title.unwrap_or_else(|| "Unknown_Playlist".to_string())
            }
            _ => "Single_Video_Playlist".to_string(),
        };

        let clean_folder_name = self.clean_filename(&playlist_title);
        let playlist_dir = self.temp_dir.join(&clean_folder_name);

        if playlist_dir.exists() {
            fs::remove_dir_all(&playlist_dir).ok();
        }
        fs::create_dir_all(&playlist_dir)?;

        let output_template = format!("{}/%(title)s.%(ext)s", clean_folder_name);

        let mut dl = YoutubeDl::new(url);
        dl.youtube_dl_path(&self.ytdlp_path)
            .extra_arg("--extract-audio")
             //.extra_arg("--external-downloader").extra_arg("aria2c")
            .extra_arg("--audio-format")
            .extra_arg("mp3")
            .extra_arg("--audio-quality")
            .extra_arg("192K")
            .extra_arg("--ignore-errors")
            .extra_arg("--no-part")
            //.extra_arg("--concurrent-fragments").extra_arg("5")
            .output_template(output_template);

        dl.download_to_async(&self.temp_dir)
            .await
            .context("yt-dlp nie ukończyło pobierania")?;

        let count = fs::read_dir(&playlist_dir)?.count();
        if count == 0 {
            return Err(anyhow::anyhow!(
                "Pobieranie zakończone, ale nie znaleziono żadnych plików!"
            ));
        }

        Ok(playlist_dir)
    }

    pub async fn download_playlist_with_progress(
        &self,
        url: &str,
        task_id: String,
        tasks: Arc<DashMap<String, String>>,
    ) -> Result<PathBuf> {
        tasks.insert(task_id.clone(), "Pobieranie listy filmów...".to_string());

        let output = YoutubeDl::new(url)
            .youtube_dl_path(&self.ytdlp_path)
            .flat_playlist(true)
            .run_async()
            .await?;

        let (playlist_title, entries) = match output {
            YoutubeDlOutput::Playlist(p) => (
                p.title.unwrap_or_else(|| "Playlist".to_string()),
                p.entries.unwrap_or_default(),
            ),
            _ => return Err(anyhow!("To nie jest playlista")),
        };

        let total = entries.len();
        let clean_folder_name = self.clean_filename(&playlist_title);
        let playlist_dir = self.temp_dir.join(&clean_folder_name);
        fs::create_dir_all(&playlist_dir)?;

        let progress = Arc::new(AtomicUsize::new(0));

        futures::stream::iter(entries)
            .map(|entry| {
                let yt = self.clone();
                let tasks_clone = tasks.clone();
                let tid = task_id.clone();
                let prog = progress.clone();
                let folder = clean_folder_name.clone();

                async move {
                    let id = entry.id.clone();
                    let video_url = format!("https://www.youtube.com/watch?v={}", id);

                    let current = prog.fetch_add(1, Ordering::SeqCst) + 1;
                    tasks_clone.insert(tid, format!("Pobieranie {} / {}", current, total));

                    let mut dl = YoutubeDl::new(&video_url);
                    dl.youtube_dl_path(&yt.ytdlp_path)
                        .extra_arg("--extract-audio")
                        .extra_arg("--ignore-error")
                        .extra_arg("--audio-format")
                        .extra_arg("mp3")
                        .extra_arg("--socket-timeout")
                        .extra_arg("30")
                        .extra_arg("--audio-quality")
                        .extra_arg("192K")
                        .output_template(format!("{}/%(title)s.%(ext)s", folder))
                        .download_to_async(&yt.temp_dir)
                        .await?;

                    Ok::<(), anyhow::Error>(())
                }
            })
            .buffer_unordered(3)
            .collect::<Vec<_>>()
            .await;

        tasks.insert(task_id, "Pakowanie ZIP...".to_string());
        Ok(playlist_dir)
    }

    pub fn create_zip_from_folder(src_dir: &Path, dst_file: &Path) -> Result<()> {
        let file = File::create(dst_file)?;
        let mut zip = zip::ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                let name = path.strip_prefix(src_dir)?;
                zip.start_file(name.to_string_lossy(), options)?;
                let mut f = File::open(path)?;
                std::io::copy(&mut f, &mut zip)?;
            }
        }

        zip.finish()?;
        Ok(())
    }

    fn clean_filename(&self, title: &str) -> String {
        title
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .replace(" ", "_")
    }

    pub async fn get_metadata(&self, url: &str) -> Result<MediaMetadata> {
        let output = YoutubeDl::new(url)
            .youtube_dl_path(&self.ytdlp_path)
            .flat_playlist(true)
            .run_async()
            .await
            .context("Błąd podczas komunikacji z yt-dlp")?;

        match output {
            YoutubeDlOutput::SingleVideo(video) => Ok(MediaMetadata {
                title: video.title.unwrap_or_else(|| "Brak tytułu".to_string()),
                author: video.uploader.unwrap_or_else(|| "Nieznany".to_string()),

                length: video
                    .duration
                    .and_then(|d| d.as_f64())
                    .map(|d| d as u64)
                    .unwrap_or(0),

                age_restricted: video.age_limit.is_some(),
                short_desc: video.description.unwrap_or_default(),

                thumbnails: video.thumbnails.map(|t| {
                    t.into_iter()
                        .map(|thumb| crate::models::Thumbnail {
                            url: thumb.url.unwrap_or_default(),
                            width: thumb.width.unwrap_or(64f64) as u64,
                            height: thumb.height.unwrap_or(64f64) as u64,
                        })
                        .collect()
                }),

                id: video.id,
                is_private: false,
            }),
            YoutubeDlOutput::Playlist(_) => Err(anyhow!(
                "To jest link do playlisty. Podgląd działa tylko dla pojedynczych filmów."
            )),
        }
    }
}
