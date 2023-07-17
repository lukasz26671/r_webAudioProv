use std::ops::Deref;
use std::{env, fs, io};
use std::path::PathBuf;
use actix_cors::Cors;
use actix_files as af;
use actix_web::{get, web, App, HttpResponse, HttpServer, HttpRequest};
use downloader::*;
use serde::{Serialize, Deserialize};
use open;
use envy;
use dotenv::dotenv;

#[derive(Debug, Deserialize)]
pub struct DownloaderParams {
    format: Option<String>
}
#[derive(Deserialize, Debug)]
struct Configuration {
    #[serde(default="default_max_video_duration_minutes")]
    max_video_duration_minutes: u16, 
    #[serde(default="default_limit_duration")]
    limit_duration: bool,
    #[serde(default="default_max_audio_duration_minutes")]
    max_audio_duration_minutes: u16,
    #[serde(default="default_port")]
    port: u16
}

fn default_limit_duration() -> bool { true }

fn default_max_video_duration_minutes() -> u16 { 5 }

fn default_port() -> u16 { 3000 } 

fn default_max_audio_duration_minutes() -> u16 { 600 }

#[get("/download_id/{id}")]
async fn get_download_id(req: HttpRequest, path: web::Path<String>) -> HttpResponse { 
    let id = path.into_inner();
    let params = web::Query::<DownloaderParams>::from_query(req.query_string()).unwrap();
    let url = format!("https://www.youtube.com/watch?v={}", id);

    let format = params.format.as_deref().unwrap_or("mp3");

    if format == "mp4" {
        match dl_get_video(&url, true).await {
            Ok(pbf) => {
                let f = af::NamedFile::open_async(&pbf).await.unwrap();
                let hvalue = format!("attachment; filename={:?}", pbf.file_name().unwrap());
                let a = f.into_response(&req);
                return HttpResponse::Ok().append_header(("Content-Disposition", hvalue)).content_type("video/mp4").body(a.into_body());
            },
            Err(e) => {
                return HttpResponse::from_error(io::Error::new(io::ErrorKind::InvalidInput, e.to_string()));
            },
        }
    } else {
        match dl_get_audio(&url).await {
            Ok(pbf) => {
                let f = af::NamedFile::open_async(&pbf).await.unwrap();
                
                let hvalue = format!("attachment; filename={:?}", pbf.file_name().unwrap());
    
                let a = f.into_response(&req);
                println!("{}: {}", "Content-Disposition", &hvalue);
                return HttpResponse::Ok().append_header(("Content-Disposition", hvalue)).content_type("audio/mpeg").body(a.into_body());
            },
            Err(e) => {
                return HttpResponse::from_error(io::Error::new(io::ErrorKind::InvalidInput, e.to_string()));
            },
        }
    }
}
#[get("/stream_id/{id}")]
async fn get_stream_id(req: HttpRequest, path: web::Path<String>) -> HttpResponse {
    let id = path.into_inner();
    let params = web::Query::<DownloaderParams>::from_query(req.query_string()).unwrap();
    let url = format!("https://www.youtube.com/watch?v={}", id);

    let format = params.format.as_deref().unwrap_or("mp3");

    if format == "mp4" {
       return HttpResponse::MethodNotAllowed().body("Video streaming is not supported.");
    } else {
        return match get_audio(&url).await {
            Ok(uri) => {
                HttpResponse::Found().append_header(("Location", uri)).finish()
            },
            Err(e) => {
                return HttpResponse::from_error(io::Error::new(io::ErrorKind::InvalidInput, e.to_string()));
            },
        }
    }
}

#[get("/info_id/{id}")]
async fn get_info_id(req: HttpRequest, path: web::Path<String>) -> web::Json<MediaMetadata> {
    let id = path.into_inner();
    let url = format!("https://www.youtube.com/watch?v={}", id);

    let metadata = get_metadata_resp(&url).await.unwrap();

    return web::Json(metadata);
}
#[get("/html_info_id/{id}")]
async fn html_get_info_id(req: HttpRequest, path: web::Path<String>) -> String {
    use MediaMetadata;

    let id = path.into_inner();
    let url = format!("https://www.youtube.com/watch?v={}", id);

    let metadata_r = get_metadata_resp(&url).await;

    let res = || -> Result<String, io::Error> {
        let metadata = metadata_r.unwrap();

        let thumbnails = metadata.thumbnails.unwrap();
        let th = thumbnails.iter().filter(|x| !x.url.contains("maxres")).max_by_key(|x| x.width).unwrap();
    
        let len = metadata.short_desc.len();
        let mut dsc : String = metadata.short_desc.chars().take(300).collect();
        
        if len >= 300 {
            dsc = format!("{}...", dsc);
        }
    
        let html = format!(r#"
            <div class='col-12 col-md-4 align-items-center text-center justify-content-center'>
                <img src='{}' style='object-fit: cover; width: 100%; user-select: none;' alt='thumbnail' class='img-fluid img-thumbnail'/>
            </div>
            <div class="col-12 col-md-8 align-items-center">
                <h3><a href='{}'>{}</a></h3>
                <p>{}</p>
                <small>Author: {}</small>
                <small>Length: {}s</small>
            </div>
        </div>
        "#, th.url, url, metadata.title, dsc, metadata.author, metadata.length);
    
        return Ok(html);
    };

    return res().unwrap_or("".to_string());
}
    

#[actix_web::main]
async fn main() -> io::Result<()> {
    dotenv().ok();
    let c : Configuration = envy::from_env::<Configuration>().expect("Provide all configuration variables.");

    let port = c.port;

    println!("Running on port {}", c.port);
    let _root: PathBuf = env::current_dir().unwrap();
    let tmp_path = _root.join("temp");
    fs::remove_dir_all(&tmp_path)?;
    fs::create_dir_all(&tmp_path)?;
    
    let ws = HttpServer::new(|| {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .service(get_download_id)
            .service(get_stream_id)
            .service(get_info_id)
            .service(html_get_info_id)
            .service(af::Files::new("/", "./public")
                .use_last_modified(true)
                .index_file("index.html")
            )
    })
    .bind(("127.0.0.1", port))?
    .bind(("0.0.0.0", port))?
    .run();
    open::that(format!("http://localhost:{}", &port))?;
    ws.await?;

    return Ok(());
}



pub mod downloader {
    use rustube::video_info::player_response::video_details::Thumbnail;
    use youtube_dl::{YoutubeDl, SingleVideo};
    use std::ops::Deref;
    use std::{path::PathBuf, io::Error};
    use std::{io, path};
    use std::fs;
    use std::env;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use rustube::*;

    pub async fn process_audio(filename: &String) -> Result<(), io::Error>{
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
    pub async fn process_video(filename: &str) -> Result<(), io::Error>{
        let mut root = env::current_dir().unwrap();
        root.extend(&["ffmpeg.exe"]);
        let mut cmd = Command::new(root.as_path())
            .args([
                "-i",
                &format!("{}.webm", filename),
                "-preset",
                "fast",
                "-crf",
                "26",
                &format!("{}.mp4", filename)
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
    
    pub async fn get_audio(url: &String) -> Result<String, io::Error> {
        let _root: PathBuf = env::current_dir().unwrap();
        let (ytdlp_path, _) = setup(&_root).unwrap();

        let id = extract_id(&url);

        return match id {
            Some(value) => {
                println!("Video ID: {:?}", value);
                let video = download_audio(&value, &ytdlp_path, Some(false)).await.unwrap_or_default();
                println!("Title: {:?}, channel: {:?}", video.title, video.channel);

                Ok(video.url.unwrap())
            },
            None => {
                Err(Error::new(io::ErrorKind::NotFound, "Video not found"))
            }
        }
    }
    
    pub async fn get_video(url: &String) -> Result<PathBuf> {
        let _root: PathBuf = env::current_dir().unwrap();
        let (_, tmp_path) = setup(&_root).unwrap();

        let _id = extract_id(&url);
        println!("{}", &url);
        let i = Id::from_raw(&url)?;
        let v = Video::from_id(i.into_owned()).await?;

        let path = v.best_quality().unwrap().download_to_dir(tmp_path).await?;

        return Ok(path);
    }

    pub async fn dl_get_audio(url: &String) -> Result<PathBuf, io::Error> {
        let _root: PathBuf = env::current_dir().unwrap();
        let (ytdlp_path, tmp_path) = setup(&_root).unwrap();

        let id = extract_id(&url);

        return match id {
            Some(value) => {
                println!("Video ID: {:?}", value);
                let vmetadata = get_metadata(&value, &ytdlp_path, None).await.unwrap();

                let c : super::Configuration = envy::from_env::<super::Configuration>().expect("Provide config.");
                
                let duration = vmetadata.duration.unwrap_or_default().as_f64().unwrap();
                

                if c.limit_duration && duration > (c.max_audio_duration_minutes as f64 * 60.0) {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Audio duration exceeds maximum of {} hours", (c.max_audio_duration_minutes as f64 / 60.0))));
                }

                let nftitle: String = vmetadata.title.chars().filter(|c| c.is_ascii()).collect::<String>().replace("/", "").replace("|", "");

                let fname = format!("{} [{}].mp3", nftitle, vmetadata.id);

                let tmp_fpath = tmp_path.join(&fname);
                if tmp_fpath.exists() {
                    println!("File {} found in storage", tmp_fpath.to_str().unwrap());

                    return Ok(tmp_fpath);
                }
                
                let video = download_audio(&value, &ytdlp_path, Some(true)).await.unwrap_or_default();
                let out_name: String = format!("[{}].opus", video.id);

                let _fname = format!("{} [{}].opus", nftitle, video.id);

                tokio::fs::rename(&out_name, &_fname).await.unwrap();

                println!("processing file");
                process_audio(&format!("{} [{}]", nftitle, video.id)).await.unwrap();
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

    pub async fn dl_get_video(url: &String, process: bool) -> Result<PathBuf, io::Error> {
        let _root: PathBuf = env::current_dir().unwrap();
        let (ytdlp_path, tmp_path) = setup(&_root).unwrap();

        let id = extract_id(&url);

        return match id {
            Some(value) => {
                println!("Video ID: {:?}", value);
                let vmetadata = get_metadata(&value, &ytdlp_path, Some(false)).await.unwrap();
               
                let c : super::Configuration = envy::from_env::<super::Configuration>().expect("Provide config.");

                let duration = vmetadata.duration.unwrap_or_default().as_f64().unwrap();
                
                if c.limit_duration && duration > (c.max_video_duration_minutes as f64 * 60.0) {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Video duration exceeds maximum of {} minutes", c.max_video_duration_minutes)));
                }

                let nftitle: String = vmetadata.title.chars().filter(|c| c.is_ascii()).collect::<String>().replace("/", "").replace("|", "");

                let fname = format!("{} [{}].mp4", nftitle, vmetadata.id);

                let tmp_fpath = tmp_path.join(format!("{}.mp4", &fname));
                if tmp_fpath.exists() {
                    println!("File {} found in storage", tmp_fpath.to_str().unwrap());

                    return Ok(tmp_fpath);
                }
                
                download_video(&value, &ytdlp_path, Some(true)).await.unwrap_or_default();
                println!("Title: {:?}, channel: {:?}", vmetadata.title, vmetadata.channel);

                let out_name: String = format!("[{}].webm", &vmetadata.id);
                let _fnamewext = format!("{} [{}]", &nftitle, &vmetadata.id);
                let _fname = format!("{}.webm", &_fnamewext);

                tokio::fs::rename(&out_name, &_fname).await.unwrap();

                let mut ext: &str = "webm";

                if process {
                    println!("processing file");
                    ext = "mp4";
                    process_video(&_fnamewext).await.unwrap();
                }
                println!("moving file");
                let p = move_video_to_temp(&_root, &format!("{} [{}].{}", &nftitle, &vmetadata.id, ext)).unwrap();
                println!("move finished");
                for path in fs::read_dir(&_root).unwrap() {
                    let path = path.unwrap().path();
                    match path.extension() {
                        None => {}
                        Some(ext) => {
                            use std::ffi::OsStr;
                            if ext == OsStr::new("webm") {
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

    pub async fn download_audio(id: &String, ytdl_path: &PathBuf, download : Option<bool>) -> Option<SingleVideo> {
        let url = format!("https://www.youtube.com/watch?v={}", id);

        println!("Downloading video: {}", url);

        let dl = download.unwrap_or_default();

        let output = YoutubeDl::new(&url)
            .youtube_dl_path(ytdl_path)
            .socket_timeout("15")
            .format("bestaudio")
            .extract_audio(dl)
            .output_template("[%(id)s].%(ext)s")
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

    pub async fn download_video(id: &String, ytdl_path: &PathBuf, download : Option<bool>) -> Option<SingleVideo> {
        let url = format!("https://www.youtube.com/watch?v={}", id);

        println!("Downloading video: {}", url);

        let dl = download.unwrap_or_default();

        let output = YoutubeDl::new(&url)
            .youtube_dl_path(ytdl_path)
            .socket_timeout("15")
            .format("bestaudio+bestvideo")
            .output_template("[%(id)s].%(ext)s")
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

    pub async fn get_metadata(id:  &String, ytdl_path: &PathBuf, video: Option<bool>) -> Option<SingleVideo> {
        let url = format!("https://www.youtube.com/watch?v={}", id);

        let opt = if video.is_some_and(| x | x == true) { "bestaudio+bestvideo" } else { "bestaudio" };

        let output = YoutubeDl::new(&url)
            .youtube_dl_path(ytdl_path)
            .socket_timeout("15")
            .format(opt)
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

    pub fn setup(root : &PathBuf) -> Option<(PathBuf, PathBuf)> {
        if let Some(proj_dirs) = directories::ProjectDirs::from("me", "lukasz26671", "r_webaudioprov") {
            let dir = proj_dirs.data_dir();
            let mut ytdlp: PathBuf = env::current_dir().unwrap();
            ytdlp.extend(&["yt-dlp.exe"]);

            fs::create_dir_all(dir).unwrap();
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
            let p = dir.join("yt-dlp.exe");
            return Some((p, root.join("temp")));
        }
        return None;
    }

    pub async fn get_metadata_resp(url: &String) -> Result<MediaMetadata> {
        
        let id = Id::from_raw(&url)?;

        let descrambler = VideoFetcher::from_id(id.into_owned())?.fetch().await?;

        let info = descrambler.video_info();
        let details = descrambler.video_details();
        
        let result = MediaMetadata {
            title: (details.title.to_string()),
            author: (details.author.to_string()),
            short_desc: (details.short_description.to_string()),
            id: (details.video_id.to_string()),
            age_restricted: info.is_age_restricted,
            is_private: details.is_private,
            length: details.length_seconds,
            thumbnails: Some(details.thumbnails.to_vec())
        };

        Ok(result)
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    pub struct MediaMetadata {
        pub title: String,
        pub author: String,
        pub short_desc: String,
        pub id: String,
        pub length: u64,
        pub age_restricted: bool,
        pub is_private: bool,
        pub thumbnails: Option<Vec<Thumbnail>>,
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

        let video = Runtime::new().unwrap().block_on(download_audio(&id, &ytdlp, None)).unwrap();
        
        assert_eq!(video.id, "PpjdTwQwWWY");
    }
}