
use anyhow::{anyhow, Result};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn generate_tts_audio(
    text: &str,
    _lang: &str,
    voice: &str,
    speed: f32,
) -> Result<(String, String)> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let filename = format!("output_{}.mp3", timestamp);
    let audio_path = format!("uploads/{}", filename);

    // Use espeak-ng for TTS generation (pure system command approach)
    let voice_name = match voice {
        "com.au" => "en-au",
        "co.uk" => "en-gb", 
        "us" => "en-us",
        "ca" => "en-ca",
        "ind" => "en-in",
        "za" => "en-za",
        "ie" => "en-ie",
        "nz" => "en-nz",
        "ng" => "en-ng",
        "tt" => "en-tt",
        "es" => "es",
        "mx" => "es-mx",
        "ar" => "es-ar",
        "cl" => "es-cl",
        _ => "en-us"
    };

    // Calculate words per minute based on speed (normal is around 175 wpm)
    let base_wpm = 175;
    let wpm = (base_wpm as f32 * speed) as u32;

    // Generate audio using espeak-ng
    let espeak_output = Command::new("espeak-ng")
        .args([
            "-v", voice_name,
            "-s", &wpm.to_string(),
            "-w", &audio_path,
            text,
        ])
        .output();

    match espeak_output {
        Ok(output) => {
            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("espeak-ng TTS failed: {}", error));
            }
        }
        Err(_) => {
            // Fallback to festival if espeak-ng is not available
            let festival_output = Command::new("festival")
                .args([
                    "--tts",
                ])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            match festival_output {
                Ok(mut process) => {
                    use std::io::Write;
                    if let Some(stdin) = process.stdin.as_mut() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                    
                    let output = process.wait_with_output()?;
                    if !output.status.success() {
                        return Err(anyhow!("Festival TTS also failed. No TTS engine available."));
                    }
                    
                    // Festival outputs to stdout, we need to convert to mp3
                    let temp_wav = format!("uploads/temp_{}.wav", timestamp);
                    std::fs::write(&temp_wav, &output.stdout)?;
                    
                    // Convert WAV to MP3 using FFmpeg
                    let ffmpeg_output = Command::new("ffmpeg")
                        .args([
                            "-y",
                            "-i", &temp_wav,
                            "-codec:a", "libmp3lame",
                            "-b:a", "128k",
                            &audio_path,
                        ])
                        .output()?;

                    // Clean up temp file
                    let _ = std::fs::remove_file(&temp_wav);

                    if !ffmpeg_output.status.success() {
                        let error = String::from_utf8_lossy(&ffmpeg_output.stderr);
                        return Err(anyhow!("FFmpeg conversion failed: {}", error));
                    }
                }
                Err(_) => {
                    // Final fallback: generate silence with text overlay (for demo purposes)
                    return generate_fallback_audio(text, &audio_path, speed).await;
                }
            }
        }
    }

    // Verify the audio file was created
    if !std::path::Path::new(&audio_path).exists() {
        return Err(anyhow!("Audio file was not generated"));
    }

    Ok((audio_path, filename))
}

async fn generate_fallback_audio(text: &str, audio_path: &str, speed: f32) -> Result<(String, String)> {
    // Calculate duration based on text length and speed
    let words = text.split_whitespace().count();
    let base_wpm = 175.0; // words per minute
    let duration = (words as f32) / (base_wpm * speed) * 60.0;
    let duration = duration.max(2.0); // minimum 2 seconds

    // Generate silence with the calculated duration
    let ffmpeg_output = Command::new("ffmpeg")
        .args([
            "-y",
            "-f", "lavfi",
            "-i", &format!("anullsrc=channel_layout=stereo:sample_rate=44100"),
            "-t", &duration.to_string(),
            "-codec:a", "libmp3lame",
            "-b:a", "128k",
            audio_path,
        ])
        .output()?;

    if !ffmpeg_output.status.success() {
        let error = String::from_utf8_lossy(&ffmpeg_output.stderr);
        return Err(anyhow!("Fallback audio generation failed: {}", error));
    }

    log::warn!("Generated fallback silent audio. Install espeak-ng or festival for actual TTS.");
    
    let filename = std::path::Path::new(audio_path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    
    Ok((audio_path.to_string(), filename))
}
