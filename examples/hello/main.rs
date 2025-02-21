use std::fs::OpenOptions;
use std::io::Write;
use edge_tts::{build_ssml, request_audio};

#[tokio::main]
async fn main() {
    let audio_data = request_audio(
        &build_ssml("晚上好，欢迎进入直播间。", "zh-CN-XiaoxiaoNeural", "medium", "medium", "medium"),
        "audio-24khz-48kbitrate-mono-mp3"
    ).await.unwrap();
    
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("test.mp3")
        .unwrap()
        .write(&audio_data)
        .unwrap();
}
