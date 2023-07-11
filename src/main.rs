use std::io;
use actix_cors::Cors;
use actix_web::{get, web, App, HttpResponse, HttpServer};
use downloader::*;

#[get("/download_id/{id}")]
async fn get_download_id(path: web::Path<String>) -> HttpResponse { 
    let id = path.into_inner();

    let url = format!("https://www.youtube.com/watch?v={}", id);

    match get_video(&url).await {
        Ok(uri) => {
            return HttpResponse::Found().append_header(("Location", uri)).finish();
        },
        Err(_) => {
            return HttpResponse::NotFound().finish();
        },
    }
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    let port: i32 = 3000;
    println!("Running on port {}", port);
    
    let _ = HttpServer::new(|| {
        let cors = Cors::permissive();
        
        App::new()
        .wrap(cors)
        .service(get_download_id)
        .route("/", web::get().to(HttpResponse::Ok))
    })
    .bind(("127.0.0.1", 3000))?
    .bind(("0.0.0.0", 3000))?
    .run()
    .await;

    return Ok(());
}



pub mod downloader {
    use youtube_dl::{YoutubeDl, SingleVideo};
    use std::{path::PathBuf, io::Error};
    use std::io;
    use std::fs;
    use std::env;

    pub async fn get_video(url: &String) -> Result<String, Error> {

        let mut ytdlp_path: PathBuf = PathBuf::new();
        let _root: PathBuf = env::current_dir().unwrap();
        if let Some(proj_dirs) = directories::ProjectDirs::from("me", "lukasz26671", "r_webaudioprov") {
            let dir = proj_dirs.data_dir();
            let mut ytdlp: PathBuf = env::current_dir().unwrap();
            ytdlp.extend(&["yt-dlp.exe"]);

            fs::create_dir_all(dir)?;
            if !dir.join("yt-dlp.exe").exists() {
                match fs::copy(ytdlp,dir.join("yt-dlp.exe").as_path()) {
                    Ok(_) => {
                        println!("Successfully copied");
                    },
                    Err(err) => {
                        panic!("failed to copy, {}", err);
                    },
                }
            }
            ytdlp_path = dir.join("yt-dlp.exe"); 
        }

        let id = extract_id(&url);
        match id {
            Some(value) => {
                println!("Video ID: {:?}", value);
                let video = download_video(&value, &ytdlp_path, None).await.unwrap_or_default();
                println!("Title: {:?}, channel: {:?}", video.title, video.channel);
                return Ok(video.url.unwrap())
            },
            None => {
                return Err(io::Error::new(io::ErrorKind::NotFound, "Video not found"));
            }
        }
    }

    pub fn extract_id(link : &String) -> Option<String> {
        let idx_of_id = link.find("v=").unwrap_or(0);
        let id : String = link.chars().skip(idx_of_id+2).take_while(|c| *c != '&' && *c != ' ' && *c != '\r' && *c!='\n').collect();

        if id.len() != 11 {
            return None;
        }

        return Some(id);
    }

    pub async fn download_video(id: &String, ytdl_path: &PathBuf, download : Option<bool>) -> Option<SingleVideo> {
        let url = format!("https://www.youtube.com/watch?v={}", id);

        println!("Downloading video: {}", url);

        let output = YoutubeDl::new(url)
            .youtube_dl_path(ytdl_path)
            .socket_timeout("15")
            .format("bestaudio")
            .download(download.unwrap_or_default())
            .run_async()
            .await;

        match output {
            Ok(v) => {
                return Some(v.into_single_video().unwrap())
            },
            Err(_) => {
                return None;
            },
        }
    }

    pub fn move_video_to_temp(root_dir : &PathBuf, filename: &String) -> Result<(), io::Error> {
        let mut temp_dir = root_dir.clone();
        temp_dir.extend(&["temp"]);

        let mut filepath = root_dir.clone();
        filepath.extend(&[&filename]);

        if !filepath.exists() {
            return Err(Error::new(io::ErrorKind::InvalidData,format!("{} does not exist", filename)));
        }

        fs::create_dir_all(&temp_dir)?;

        match fs::rename(filepath, temp_dir.join(&filename)) {
            Ok(_) => {
                println!("Successfully moved {} to {}", filename, temp_dir.join(&filename).to_str().unwrap());
                return Ok(())
            },
            Err(e) => {
                println!("Error: {}", e);
                return Err(e);
            },
        };
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_extract_id_passing() {
        let link = "https://www.youtube.com/watch?v=JIvKgSyvtxI&fbclid=abcdssf".to_owned();

        let id = extract_id(&link).unwrap();
        assert_eq!(id, "JIvKgSyvtxI");
    }
    #[test]
    fn test_extract_id_passing2() {
        let link = "https://www.youtube.com/watch?v=PpjdTwQwWWY".to_owned();

        let id = extract_id(&link).unwrap();
        assert_eq!(id, "PpjdTwQwWWY");
    }

    #[test]
    fn test_extract_id_failing() {
        let link = "https://www.youtube.com/watch?v=gibb".to_owned();

        let id = extract_id(&link);

        assert!(id.is_none());
    }
    #[test]
    fn test_extract_id_failing2() {
        let link = "https://www.youtube.com/watch?v=gibberishtoofuckinglong".to_owned();

        let id = extract_id(&link);

        assert!(id.is_none());
    }

    #[test]
    fn test_yt_extract_id() {
        let link = "https://www.youtube.com/watch?v=PpjdTwQwWWY".to_owned();

        let id = extract_id(&link).unwrap();
        let mut ytdlp: PathBuf = env::current_dir().unwrap();
        ytdlp.extend(&["yt-dlp.exe"]);

        let video = Runtime::new().unwrap().block_on(download_video(&id, &ytdlp, None)).unwrap();
        
        assert_eq!(video.id, "PpjdTwQwWWY");
    }
}