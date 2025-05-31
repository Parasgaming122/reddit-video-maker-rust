use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn generate_captions_from_text(text: &str, audio_duration: f64) -> Result<(String, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let total_words = words.len();

    if total_words == 0 {
        return Ok(("".to_string(), "No text to generate captions".to_string()));
    }

    let words_per_second = total_words as f64 / audio_duration;

    // Create segments of 3-6 words for TikTok-style captions
    let mut segments = Vec::new();
    let mut current_segment = Vec::new();

    for word in words {
        current_segment.push(word);

        // Create shorter segments for better readability
        if current_segment.len() >= 4
            && (word.ends_with(&['.', '!', '?', ',']) || current_segment.len() >= 6)
        {
            segments.push(current_segment.join(" "));
            current_segment.clear();
        } else if current_segment.len() >= 6 {
            segments.push(current_segment.join(" "));
            current_segment.clear();
        }
    }

    // Add remaining words
    if !current_segment.is_empty() {
        segments.push(current_segment.join(" "));
    }

    // Create SRT content
    let mut srt_content = String::new();
    let mut current_word_index = 0;

    for (i, segment) in segments.iter().enumerate() {
        let segment_words: Vec<&str> = segment.split_whitespace().collect();
        let words_in_segment = segment_words.len();

        let start_time = current_word_index as f64 / words_per_second;
        let end_time =
            ((current_word_index + words_in_segment) as f64 / words_per_second).min(audio_duration);

        srt_content.push_str(&format!("{}\n", i + 1));
        srt_content.push_str(&format!(
            "{} --> {}\n",
            format_time(start_time),
            format_time(end_time)
        ));
        srt_content.push_str(&format!("{}\n\n", segment.to_uppercase()));

        current_word_index += words_in_segment;
    }

    let caption_text = format!("Generated {} caption segments", segments.len());
    Ok((srt_content, caption_text))
}

fn format_time(seconds: f64) -> String {
    let hours = (seconds / 3600.0) as u32;
    let minutes = ((seconds % 3600.0) / 60.0) as u32;
    let secs = (seconds % 60.0) as u32;
    let millis = ((seconds % 1.0) * 1000.0) as u32;

    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, secs, millis)
}

pub async fn save_srt_file(srt_content: &str) -> Result<String> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let srt_filename = format!("captions_{}.srt", timestamp);
    let srt_path = format!("uploads/{}", srt_filename);

    std::fs::write(&srt_path, srt_content)?;

    Ok(srt_filename)
}
