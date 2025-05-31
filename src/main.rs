
use actix_files as fs;
use actix_multipart::Multipart;
use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer, Result};
use futures_util::TryStreamExt as _;
use serde::{Deserialize, Serialize};

mod tts;
mod video;
mod captions;

use tts::*;
use video::*;

#[derive(Serialize)]
struct Voice {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct TtsVoices {
    en: Vec<Voice>,
    es: Vec<Voice>,
}

fn get_tts_voices() -> TtsVoices {
    TtsVoices {
        en: vec![
            Voice { id: "com.au".to_string(), name: "Australian".to_string() },
            Voice { id: "co.uk".to_string(), name: "British".to_string() },
            Voice { id: "us".to_string(), name: "American".to_string() },
            Voice { id: "ca".to_string(), name: "Canadian".to_string() },
            Voice { id: "ind".to_string(), name: "Indian".to_string() },
            Voice { id: "za".to_string(), name: "South African".to_string() },
            Voice { id: "ie".to_string(), name: "Irish".to_string() },
            Voice { id: "nz".to_string(), name: "New Zealand".to_string() },
            Voice { id: "ng".to_string(), name: "Nigerian".to_string() },
            Voice { id: "tt".to_string(), name: "Trinidad & Tobago".to_string() },
        ],
        es: vec![
            Voice { id: "es".to_string(), name: "Spanish (Spain)".to_string() },
            Voice { id: "mx".to_string(), name: "Mexican Spanish".to_string() },
            Voice { id: "ar".to_string(), name: "Argentinian Spanish".to_string() },
            Voice { id: "cl".to_string(), name: "Chilean Spanish".to_string() },
        ],
    }
}

#[derive(Deserialize)]
struct TtsRequest {
    text: String,
    lang: Option<String>,
    voice: Option<String>,
    speed: Option<f32>,
}

#[derive(Serialize)]
struct TtsResponse {
    audio: String,
    filename: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ffmpeg_error: Option<String>,
}

#[derive(Serialize)]
struct VideoResponse {
    video: String,
    aspect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    captions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    srt_file: Option<String>,
}

async fn index() -> Result<HttpResponse> {
    let html = include_str!("../index/index.html");
    let _tts_voices = get_tts_voices();

    // Simple template replacement for TTS voices
    let html_with_voices = html.replace("{% for voice in tts_voices['en'] %}", "")
        .replace("{{ voice.id }}", "us")
        .replace("{{ voice.name }}", "American")
        .replace("{% endfor %}", "");

    Ok(HttpResponse::Ok()
        .content_type("text/html")
        .body(html_with_voices))
}

async fn generate_tts(req: web::Json<TtsRequest>) -> Result<HttpResponse> {
    let text = &req.text;
    let lang = req.lang.as_deref().unwrap_or("en");
    let voice = req.voice.as_deref().unwrap_or("us");
    let speed = req.speed.unwrap_or(1.0);

    if text.trim().is_empty() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            error: "No text provided".to_string(),
            ffmpeg_error: None,
        }));
    }

    match generate_tts_audio(text, lang, voice, speed).await {
        Ok((_audio_path, filename)) => {
            Ok(HttpResponse::Ok().json(TtsResponse {
                audio: format!("/download/{}", filename),
                filename,
            }))
        }
        Err(e) => {
            log::error!("TTS generation failed: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("TTS generation failed: {}", e),
                ffmpeg_error: None,
            }))
        }
    }
}

async fn create_video(mut payload: Multipart) -> Result<HttpResponse> {
    let mut bg_file_data = Vec::new();
    let mut bg_filename = String::new();
    let mut aspect_ratio = "16:9".to_string();
    let mut audio_filename = String::new();
    let mut original_text = String::new();

    // Parse multipart form data
    while let Some(mut field) = payload.try_next().await? {
        match field.name() {
            "bg_file" => {
                bg_filename = field.content_disposition().get_filename().unwrap_or("unknown").to_string();
                while let Some(chunk) = field.try_next().await? {
                    bg_file_data.extend_from_slice(&chunk);
                }
            }
            "aspect" => {
                while let Some(chunk) = field.try_next().await? {
                    aspect_ratio = String::from_utf8_lossy(&chunk).to_string();
                }
            }
            "audio_filename" => {
                while let Some(chunk) = field.try_next().await? {
                    audio_filename = String::from_utf8_lossy(&chunk).to_string();
                }
            }
            "text" => {
                while let Some(chunk) = field.try_next().await? {
                    original_text.push_str(&String::from_utf8_lossy(&chunk));
                }
            }
            _ => {}
        }
    }

    if bg_file_data.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            error: "No background file uploaded".to_string(),
            ffmpeg_error: None,
        }));
    }

    if audio_filename.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            error: "No audio reference provided".to_string(),
            ffmpeg_error: None,
        }));
    }

    match process_video(bg_file_data, bg_filename, aspect_ratio.clone(), audio_filename, original_text).await {
        Ok((video_filename, caption_text, srt_filename)) => {
            Ok(HttpResponse::Ok().json(VideoResponse {
                video: format!("/download/{}", video_filename),
                aspect: aspect_ratio,
                captions: caption_text,
                srt_file: srt_filename.map(|f| format!("/download/{}", f)),
            }))
        }
        Err(e) => {
            log::error!("Video creation failed: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("Video creation failed: {}", e),
                ffmpeg_error: Some(e.to_string()),
            }))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    // Create uploads directory
    std::fs::create_dir_all("uploads").unwrap();

    log::info!("Starting Reddit Video Maker on 0.0.0.0:8080");

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .route("/", web::get().to(index))
            .route("/generate-tts", web::post().to(generate_tts))
            .route("/create-video", web::post().to(create_video))
            .service(fs::Files::new("/download", "uploads").show_files_listing())
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
