mod config;
mod models;
mod routes;
mod services;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use std::{env, fs, io};

use crate::config::Configuration;
use crate::services::youtube::YoutubeService;

use dashmap::DashMap;
use std::sync::Arc;

pub type SharedTasks = web::Data<Arc<DashMap<String, String>>>;

pub struct AppState {
    pub yt_service: YoutubeService,
    // ID zadania | Status: ("processing", "ready", "error")
    pub tasks: DashMap<String, String>,
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    let config = Configuration::from_env();
    let port = config.port;

    let root = env::current_dir()?;
    let temp_path = root.join("temp");
    let ytdlp_path = root.join("yt-dlp.exe");

    // Czyszczenie i tworzenie folderu temp
    if temp_path.exists() {
        let _ = fs::remove_dir_all(&temp_path);
    }
    fs::create_dir_all(&temp_path)?;

    let tasks = Arc::new(DashMap::<String, String>::new());

    let yt_service = web::Data::new(YoutubeService::new(ytdlp_path, temp_path.clone()));

    println!("Server running on: http://localhost:{}", port);

    if let Err(e) = open::that(format!("http://localhost:{}", port)) {
        eprintln!("Nie udało się otworzyć przeglądarki: {}", e);
    }

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(tasks.clone()))
            .app_data(yt_service.clone())
            .wrap(Cors::permissive())
            .service(routes::media::get_info_id)
            .service(routes::media::html_get_info_id)
            .service(routes::media::get_download_id)
            .service(routes::media::stream_id)
            .service(routes::media::download_playlist_handler)
            .service(routes::media::check_status_handler)
            .service(routes::media::get_zip_handler)
            .service(
                actix_files::Files::new("/", "./public")
                    .index_file("index.html")
                    .use_last_modified(true),
            )
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
