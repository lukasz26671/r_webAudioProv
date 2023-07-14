use std::{env, fs, io};
use std::path::PathBuf;
use actix_cors::Cors;
use actix_files as af;
use actix_web::{get, web, App, HttpResponse, HttpServer, HttpRequest};
use downloader::*;
use std::io::{BufRead, BufReader, Error};
use std::process::{Command, Stdio};

#[get("/download_id/{id}")]
async fn get_download_id(req: HttpRequest, path: web::Path<String>) -> HttpResponse { 
    let id = path.into_inner();

    let url = format!("https://www.youtube.com/watch?v={}", id);

    return match dl_get_video(&url).await {
        Ok(pbf) => {
            let f = af::NamedFile::open_async(pbf).await.unwrap();
            
            return f.into_response(&req);
        },
        Err(_) => {
            HttpResponse::NotFound().finish()
        },
    }
}
#[get("/stream_id/{id}")]
async fn get_stream_id(path: web::Path<String>) -> HttpResponse {
    let id = path.into_inner();

    let url = format!("https://www.youtube.com/watch?v={}", id);

    return match get_video(&url).await {
        Ok(uri) => {
            HttpResponse::Found().append_header(("Location", uri)).finish()
        },
        Err(_) => {
            HttpResponse::NotFound().finish()
        },
    }
}


async fn index(_req: HttpRequest) -> Result<af::NamedFile, io::Error> {
    let path: PathBuf = "./files/index.html".parse().unwrap();
    Ok(af::NamedFile::open(path)?)
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    let port: i32 = 3000;
    println!("Running on port {}", port);
    let _root: PathBuf = env::current_dir().unwrap();
    let tmp_path = _root.join("temp");
    fs::remove_dir_all(&tmp_path)?;
    fs::create_dir_all(&tmp_path)?;
    
    let _ = HttpServer::new(|| {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .service(get_download_id)
            .service(get_stream_id)
            .service(af::Files::new("/", "./public")
                .use_last_modified(true)
                .index_file("index.html")
            )
    })
    .bind(("127.0.0.1", 3000))?
    .bind(("0.0.0.0", 3000))?
    .run()
    .await;

    return Ok(());
}



pub mod downloader {
    use youtube_dl::{YoutubeDl, SingleVideo, YoutubeDlOutput};
    use std::{path::PathBuf, io::Error};
    use std::io;
    use std::fs;
    use std::env;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use tokio::fs::File;

    pub async fn process_file(filename: &str) -> Result<(), io::Error>{
        let mut root = env::current_dir().unwrap();
        root.extend(&["ffmpeg.exe"]);
        
        let mut cmd = Command::new(root.as_path())
            .args([
                "-i",
                &format!("{}.opus", filename),
                "-ab",
                "320k",
                &format!("{}.mp3", filename)
            ]).stdout(Stdio::piped())
            .spawn()
            .unwrap();
        {
            let stdout = cmd.stdout.as_mut().unwrap();
            let stdout_reader = BufReader::new(stdout);
            let stdout_lines = stdout_reader.lines();
            
            for line in stdout_lines {
                println!("Read: {:?}", line);
            }
        }
        cmd.wait().unwrap();
        Ok(())
    }
    
    pub async fn get_video(url: &String) -> Result<String, io::Error> {
        let mut ytdlp_path: PathBuf = PathBuf::new();
        let mut tmp_path: PathBuf = PathBuf::new();
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
            tmp_path = _root.join("/temp");
            
            println!("{}", tmp_path.to_str().unwrap());
        }

        let id = extract_id(&url);

        return match id {
            Some(value) => {
                println!("Video ID: {:?}", value);
                let video = download_video(&value, &ytdlp_path, Some(false)).await.unwrap_or_default();
                println!("Title: {:?}, channel: {:?}", video.title, video.channel);

                Ok(video.url.unwrap())
            },
            None => {
                Err(Error::new(io::ErrorKind::NotFound, "Video not found"))
            }
        }
    }
    pub async fn dl_get_video(url: &String) -> Result<PathBuf, io::Error> {

        let mut ytdlp_path: PathBuf = PathBuf::new();
        let mut tmp_path: PathBuf = PathBuf::new();
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
            tmp_path = _root.join("temp");
            println!("{}", tmp_path.to_str().unwrap());
        }

        let id = extract_id(&url);

        return match id {
            Some(value) => {
                println!("Video ID: {:?}", value);
                let vmetadata = get_video_metadata(&value, &ytdlp_path).await.unwrap();

                let fname = format!("{} [{}].mp3", vmetadata.title, vmetadata.id);

                let tmp_fpath = tmp_path.join(&fname);
                if tmp_fpath.exists() {
                    println!("File {} found in storage", tmp_fpath.to_str().unwrap());

                    return Ok(tmp_fpath);
                }
                
                let video = download_video(&value, &ytdlp_path, Some(true)).await.unwrap_or_default();
                println!("Title: {:?}, channel: {:?}", video.title, video.channel);

                println!("processing file");
                process_file(&format!("{} [{}]", video.title, video.id)).await.unwrap();
                println!("moving file");
                let p = move_video_to_temp(&_root, &fname).unwrap();
                println!("move finished");
                for path in fs::read_dir(&_root).unwrap() {
                    let path = path.unwrap().path();
                    match path.extension() {
                        None => {
                            
                        }
                        Some(ext) => {
                            use std::ffi::OsStr;
                            if ext == OsStr::new("opus") {
                                fs::remove_file(path).unwrap();
                            }
                        }
                    };
                }
                Ok(p)
            },
            None => {
                Err(Error::new(io::ErrorKind::NotFound, "Video not found"))
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

        let dl = download.unwrap_or_default();
        println!("{}", dl);
        let output = YoutubeDl::new(&url)
            .youtube_dl_path(ytdl_path)
            .socket_timeout("15")
            .format("bestaudio")
            .extract_audio(dl)
            .download(dl)
            .run_async().await;
            
        return match output {
            Ok(v) => {
                Some(v.into_single_video().unwrap())
            },
            Err(_) => {
                None
            },
        }
    }

    pub async fn get_video_metadata(id:  &String, ytdl_path: &PathBuf) -> Option<SingleVideo> {
        let url = format!("https://www.youtube.com/watch?v={}", id);
        let output = YoutubeDl::new(&url)
            .youtube_dl_path(ytdl_path)
            .socket_timeout("15")
            .format("bestaudio")
            .run_async().await;
        
        return match output {
            Ok(v) => {
                Some(v.into_single_video().unwrap())
            },
            Err(_) => {
                None
            },
        }
    }
    
    pub fn move_video_to_temp(root_dir : &PathBuf, filename: &String) -> Result<PathBuf, io::Error> {
        let mut temp_dir = root_dir.clone();
        temp_dir.extend(&["temp"]);

        let mut filepath = root_dir.clone();
        filepath.extend(&[&filename]);

        if !filepath.exists() {
            return Err(io::Error::new(io::ErrorKind::InvalidData,format!("{} does not exist", filename)));
        }
        
        fs::create_dir_all(&temp_dir)?;

        return match fs::rename(filepath, temp_dir.join(&filename)) {
            Ok(_) => {
                let p = temp_dir.join(&filename);
                println!("Successfully moved {} to {}", filename, p.to_str().unwrap());
                
                
                Ok(p)
            },
            Err(e) => {
                println!("Error: {}", e);
                Err(e)
            },
        };
    }
}
#[cfg(test)]
mod test {
    use std::env;
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