use crate::models::{DownloaderParams, PlaylistParams};
use crate::services::youtube::YoutubeService;
use crate::{AppState, SharedTasks};
use actix_files as af;
use actix_files::NamedFile;
use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use html_escape::encode_safe;
use std::sync::Arc;

#[get("/info_id/{id}")]
pub async fn get_info_id(
    path: web::Path<String>,
    yt_service: web::Data<YoutubeService>,
) -> HttpResponse {
    let id = path.into_inner();
    let url = format!("https://www.youtube.com/watch?v={}", id);

    match yt_service.get_metadata_resp(&url).await {
        Ok(metadata) => HttpResponse::Ok().json(metadata),
        Err(e) => {
            eprintln!("Błąd info_id: {}", e);
            HttpResponse::InternalServerError().body("Nie udało się pobrać metadanych")
        }
    }
}

#[get("/download_id/{id}")]
pub async fn get_download_id(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<DownloaderParams>,
    yt_service: web::Data<YoutubeService>,
) -> impl Responder {
    let id = path.into_inner();
    let url = format!("https://www.youtube.com/watch?v={}", id);
    let format = query.format.as_deref().unwrap_or("mp3");

    let result = if format == "mp4" {
        yt_service.download_video(&url).await
    } else {
        yt_service.download_audio(&url).await
    };

    match result {
        Ok(path_to_file) => match af::NamedFile::open_async(&path_to_file).await {
            Ok(named_file) => {
                let file_name = path_to_file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();

                let content_disposition = format!("attachment; filename=\"{}\"", file_name);
                let content_type = if format == "mp4" {
                    "video/mp4"
                } else {
                    "audio/mpeg"
                };

                named_file
                    .into_response(&req)
                    .customize()
                    .insert_header(("Content-Disposition", content_disposition))
                    .insert_header(("Content-Type", content_type))
            }
            Err(e) => HttpResponse::InternalServerError()
                .body(format!("Błąd pliku: {}", e))
                .customize(),
        },
        Err(e) => HttpResponse::BadRequest()
            .body(format!("Błąd pobierania: {}", e))
            .customize(),
    }
}

#[get("/stream_id/{id}")]
pub async fn stream_id(
    path: web::Path<String>,
    yt_service: web::Data<YoutubeService>,
) -> impl Responder {
    let id = path.into_inner();
    let url = format!("https://www.youtube.com/watch?v={}", id);

    let output = youtube_dl::YoutubeDl::new(&url)
        .youtube_dl_path(&yt_service.ytdlp_path)
        .format("bestaudio")
        .run_async()
        .await;

    match output {
        Ok(youtube_dl::YoutubeDlOutput::SingleVideo(video)) => {
            if let Some(stream_url) = video.url {
                return HttpResponse::TemporaryRedirect()
                    .insert_header(("Location", stream_url))
                    .finish();
            }
            HttpResponse::InternalServerError().body("Nie znaleziono strumienia")
        }
        _ => HttpResponse::InternalServerError().body("Błąd pobierania strumienia"),
    }
}

#[get("/download_playlist")]
pub async fn download_playlist_handler(
    query: web::Query<PlaylistParams>,
    yt_service: web::Data<YoutubeService>,
    tasks: SharedTasks,
) -> impl Responder {
    let task_id = uuid::Uuid::new_v4().to_string();
    let url = query.url.clone();

    let yt_service_owned = yt_service.get_ref().clone();
    let tasks_arc = tasks.get_ref().clone();
    let tid = task_id.clone();

    tasks_arc.insert(task_id.clone(), "Inicjowanie...".to_string());

    tokio::spawn(async move {
        match yt_service_owned
            .download_playlist_with_progress(&url, tid.clone(), tasks_arc.clone())
            .await
        {
            Ok(playlist_dir) => {
                let playlist_name = playlist_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("playlist")
                    .to_string();

                let zip_filename = format!("{}.zip", playlist_name);
                let zip_path = yt_service_owned.temp_dir.join(&zip_filename);

                tasks_arc.insert(tid.clone(), "Pakowanie ZIP...".to_string());

                if let Err(e) = YoutubeService::create_zip_from_folder(&playlist_dir, &zip_path) {
                    tasks_arc.insert(tid, format!("error: zip_failed: {}", e));
                } else {
                    tasks_arc.insert(tid, format!("ready|{}", playlist_name));
                }
            }
            Err(e) => {
                tasks_arc.insert(tid, format!("error: {}", e));
            }
        }
    });

    HttpResponse::Ok().json(serde_json::json!({
        "task_id": task_id
    }))
}

#[get("/get_zip/{task_id}")]
pub async fn get_zip_handler(
    task_id: web::Path<String>,
    yt_service: web::Data<YoutubeService>,
    tasks: SharedTasks,
    req: HttpRequest,
) -> impl Responder {
    let tid = task_id.into_inner();

    if let Some(status) = tasks.get(&tid) {
        let status_val = status.value();

        if status_val.starts_with("ready|") {
            let playlist_name = status_val.split('|').nth(1).unwrap_or("playlist");

            // nwm można było dać po task id (uuid), ale za każdym razem jest inne więc jakaś implementacja cache w przyszłości była, by trudniejsza lepiej już wziąć bajty z nazwy i z niej zrobić uuid
            let zip_path = yt_service.temp_dir.join(format!("{}.zip", playlist_name));

            let download_name = format!("{}.zip", playlist_name);

            return match NamedFile::open_async(&zip_path).await {
                Ok(file) => file
                    .set_content_disposition(actix_web::http::header::ContentDisposition {
                        disposition: actix_web::http::header::DispositionType::Attachment,
                        parameters: vec![actix_web::http::header::DispositionParam::Filename(
                            download_name,
                        )],
                    })
                    .into_response(&req),
                Err(_) => HttpResponse::NotFound().body("Plik ZIP nie istnieje na serwerze"),
            };
        }
    }

    HttpResponse::BadRequest().body("Zadanie nie jest jeszcze gotowe lub nie istnieje")
}

#[get("/download_playlist")]
pub async fn start_playlist_handler(
    query: web::Query<PlaylistParams>,
    data: web::Data<Arc<AppState>>,
) -> impl Responder {
    let task_id = uuid::Uuid::new_v4().to_string();
    let task_id_clone = task_id.clone();
    let data_clone = data.clone();
    let url = query.url.clone();

    data.tasks.insert(task_id.clone(), "processing".to_string());

    tokio::spawn(async move {
        data_clone.tasks.insert(
            task_id_clone.clone(),
            "Pobieranie metadanych...".to_string(),
        );
        match data_clone
            .yt_service
            .download_full_playlist_fast(&url)
            .await
        {
            Ok(dir) => {
                data_clone
                    .tasks
                    .insert(task_id_clone.clone(), "Pakowanie ZIP...".to_string());
                let zip_path = data_clone
                    .yt_service
                    .temp_dir
                    .join(format!("{}.zip", task_id_clone));
                if let Ok(_) = YoutubeService::create_zip_from_folder(&dir, &zip_path) {
                    data_clone.tasks.insert(task_id_clone, "ready".to_string());
                } else {
                    data_clone
                        .tasks
                        .insert(task_id_clone, "error: zip_failed".to_string());
                }
            }
            Err(e) => {
                data_clone
                    .tasks
                    .insert(task_id_clone, format!("error: {}", e));
            }
        }
    });

    HttpResponse::Ok().json(serde_json::json!({ "task_id": task_id }))
}

#[get("/status/{task_id}")]
pub async fn check_status_handler(
    task_id: web::Path<String>,
    tasks: SharedTasks,
) -> impl Responder {
    let tid = task_id.into_inner();

    match tasks.get(&tid) {
        Some(status) => HttpResponse::Ok().json(serde_json::json!({
            "status": status.value()
        })),
        None => HttpResponse::NotFound().json(serde_json::json!({
            "status": "error: task_not_found"
        })),
    }
}

#[get("/html_info_id/{id}")]
pub async fn html_get_info_id(
    path: web::Path<String>,
    yt_service: web::Data<YoutubeService>,
) -> impl Responder {
    let id = path.into_inner();
    let url = format!("https://www.youtube.com/watch?v={}", id);

    let metadata_result = yt_service.get_metadata(&url).await;

    let html = match metadata_result {
        Ok(metadata) => {
            let thumbnail_url = metadata
                .thumbnails
                .as_ref()
                .and_then(|t| t.iter().max_by_key(|x| x.width))
                .map(|t| t.url.as_str())
                .unwrap_or("");

            let mut dsc: String = metadata.short_desc.chars().take(300).collect();
            if metadata.short_desc.chars().count() > 300 {
                dsc.push_str("...");
            }

            format!(
                r#"
                <div class="row g-3 py-3 border-bottom text-white">
                    <div class="col-12 text-center">
                        <img src="{thumb}" style="object-fit: cover; width: 100%; max-height: 300px;"
                             alt="thumbnail" class="img-fluid img-thumbnail shadow-sm bg-dark border-secondary"/>
                    </div>
                    <div class="col-12">
                        <h4 class="mb-1">
                            <a href="{url}" target="_blank" class="text-info text-decoration-none">{title}</a>
                        </h4>
                        <p class="text-secondary small mb-2">{description}</p>
                        <div class="d-flex justify-content-between align-items-center">
                            <span class="badge bg-secondary">Autor: {author}</span>
                            <span class="badge bg-primary">Czas: {length}s</span>
                        </div>
                    </div>
                </div>
                "#,
                thumb = thumbnail_url,
                url = url,
                title = encode_safe(&metadata.title),
                description = encode_safe(&dsc),
                author = encode_safe(&metadata.author),
                length = metadata.length
            )
        }
        Err(e) => {
            eprintln!("Błąd metadanych: {}", e);
            r#"<div class="alert alert-danger">Nie udało się pobrać informacji o filmie. Sprawdź czy link jest poprawny.</div>"#.to_string()
        }
    };

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}
