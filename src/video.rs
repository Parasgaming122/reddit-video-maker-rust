use crate::captions::{generate_captions_from_text, save_srt_file};
use anyhow::{anyhow, Result};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn process_video(
    bg_file_data: Vec<u8>,
    bg_filename: String,
    aspect_ratio: String,
    audio_filename: String,
    original_text: String,
) -> Result<(String, Option<String>, Option<String>)> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    // Save background file
    let safe_bg_filename = format!(
        "bg_{}_{}",
        timestamp,
        bg_filename
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_')
            .collect::<String>()
    );
    let bg_path = format!("uploads/{}", safe_bg_filename);
    std::fs::write(&bg_path, bg_file_data)?;

    let output_filename = format!("video_{}.mp4", timestamp);
    let output_path = format!("uploads/{}", output_filename);

    // Convert audio to ensure compatibility
    let temp_audio = format!("uploads/converted_{}.aac", timestamp);
    let audio_path = format!("uploads/{}", audio_filename);

    let audio_convert = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            &audio_path,
            "-ar",
            "44100",
            "-ac",
            "2",
            "-c:a",
            "aac",
            &temp_audio,
        ])
        .output()?;

    if !audio_convert.status.success() {
        return Err(anyhow!(
            "Audio conversion failed: {}",
            String::from_utf8_lossy(&audio_convert.stderr)
        ));
    }

    // Get audio duration
    let duration_output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
            &temp_audio,
        ])
        .output()?;

    if !duration_output.status.success() {
        return Err(anyhow!("Could not determine audio duration"));
    }

    let duration_str = String::from_utf8_lossy(&duration_output.stdout);
    let duration_json: serde_json::Value = serde_json::from_str(&duration_str)?;
    let duration = duration_json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| anyhow!("Invalid duration format"))?;

    // Generate captions
    let (srt_content, caption_text, srt_filename) = if !original_text.trim().is_empty() {
        match generate_captions_from_text(&original_text, duration) {
            Ok((content, text)) => {
                if !content.is_empty() {
                    match save_srt_file(&content).await {
                        Ok(filename) => (Some(content), Some(text), Some(filename)),
                        Err(_) => (None, None, None),
                    }
                } else {
                    (None, None, None)
                }
            }
            Err(_) => (None, None, None),
        }
    } else {
        (None, None, None)
    };

    // Determine if background is image or video
    let ext = bg_filename.to_lowercase();
    let is_image = ext.ends_with(".jpg")
        || ext.ends_with(".jpeg")
        || ext.ends_with(".png")
        || ext.ends_with(".bmp")
        || ext.ends_with(".gif")
        || ext.ends_with(".webp");

    // Set up video scaling
    let scale_filter = if aspect_ratio == "9:16" {
        "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
    } else {
        "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2"
    };

    // Create FFmpeg command
    let mut ffmpeg_cmd = vec!["ffmpeg", "-y"];

    let duration_str = duration.to_string();
    
    if is_image {
        ffmpeg_cmd.extend([
            "-loop",
            "1",
            "-framerate",
            "30",
            "-t",
            &duration_str,
        ]);
    } else {
        ffmpeg_cmd.extend(["-stream_loop", "-1"]);
    }

    ffmpeg_cmd.extend(["-i", &bg_path, "-i", &temp_audio]);

    if !is_image {
        ffmpeg_cmd.extend(["-t", &duration_str]);
    }

    // Add filter complex
    let mut filter_complex = format!("[0:v]{}", scale_filter);

    if let Some(ref srt_content) = srt_content {
        let srt_path = format!("uploads/captions_{}.srt", timestamp);
        std::fs::write(&srt_path, srt_content)?;

        let base_font_size = if aspect_ratio == "16:9" { 32 } else { 36 };
        let font_size = (base_font_size as f32 * 0.7) as u32;
        let base_margin_v = if aspect_ratio == "16:9" { 40 } else { 80 };
        let margin_v = (base_margin_v as f32 * 0.7) as u32;

        let srt_path_escaped = srt_path.replace("\\", "\\\\").replace(":", "\\:");
        let subtitle_filter = format!(
            ",subtitles='{}':force_style='Fontsize={},PrimaryColour=&Hffffff,OutlineColour=&H000000,BackColour=&H80000000,Outline=3,Shadow=2,Alignment=2,MarginV={},Bold=1'",
            srt_path_escaped, font_size, margin_v
        );
        filter_complex.push_str(&subtitle_filter);
    }

    filter_complex.push_str("[v]");

    ffmpeg_cmd.extend(["-filter_complex", &filter_complex]);
    ffmpeg_cmd.extend(["-map", "[v]", "-map", "1:a"]);
    ffmpeg_cmd.extend(["-c:v", "libx264", "-preset", "fast", "-c:a", "copy"]);
    ffmpeg_cmd.extend(["-movflags", "+faststart", "-shortest", &output_path]);

    // Execute FFmpeg
    let ffmpeg_output = Command::new("ffmpeg")
        .args(&ffmpeg_cmd[1..]) // Skip "ffmpeg" as it's the command name
        .output()?;

    // Clean up temporary files
    let _ = std::fs::remove_file(&temp_audio);
    let _ = std::fs::remove_file(&bg_path);

    if !ffmpeg_output.status.success() {
        return Err(anyhow!(
            "Video creation failed: {}",
            String::from_utf8_lossy(&ffmpeg_output.stderr)
        ));
    }

    // Verify output exists
    if !std::path::Path::new(&output_path).exists() {
        return Err(anyhow!("Output video not generated"));
    }

    Ok((output_filename, caption_text, srt_filename))
}
