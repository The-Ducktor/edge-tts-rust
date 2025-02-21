use anyhow::anyhow;
use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, instrument};
use xml::escape::{escape_str_attribute, escape_str_pcdata};

const SYNTH_URL: &str = "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1?TrustedClientToken=6A5AA1D4EAFF4E9FB37E23D68491D6F4";

#[instrument]
fn random_request_id() -> String {
    let mut buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(&buf[..])
}

#[instrument(skip(s))]
fn parse_headers(s: impl AsRef<str>) -> Vec<(String, String)> {
    s.as_ref()
        .split("\r\n")
        .filter_map(|s| {
            if !s.is_empty() {
                let mut iter = s.splitn(2, ':');
                Some((
                    iter.next().unwrap_or_default().to_owned(),
                    iter.next().unwrap_or_default().to_owned(),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Build SSML for text-to-speech request
///
/// # Arguments
/// * `voice_short_name` - eg: "zh-CN-XiaoxiaoNeural"
/// * `pitch` - One of: "x-low", "low", "medium", "high", "x-high", "default"
/// * `rate` - One of: "x-slow", "slow", "medium", "fast", "x-fast", "default"
/// * `volume` - One of: "silent", "x-soft", "soft", "medium", "loud", "x-loud", "default"
#[instrument(skip(text))]
pub fn build_ssml(
    text: &str,
    voice_short_name: &str,
    pitch: &str,
    rate: &str,
    volume: &str,
) -> String {
    format!(
        r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xmlns:mstts="https://www.w3.org/2001/mstts" xml:lang="en-US">
            <voice name="{}">
                <prosody pitch="{}" rate="{}" volume="{}">{}</prosody>
            </voice>
        </speak>"#,
        escape_str_attribute(voice_short_name),
        escape_str_attribute(pitch),
        escape_str_attribute(rate),
        escape_str_attribute(volume),
        escape_str_pcdata(text)
    )
}

/// Request audio data asynchronously
///
/// # Arguments
/// * `output_format` - eg: "audio-24khz-48kbitrate-mono-mp3"
///
/// See https://learn.microsoft.com/en-us/azure/ai-services/speech-service/rest-text-to-speech?tabs=streaming#audio-outputs
#[instrument(skip(ssml))]
pub async fn request_audio(ssml: &str, output_format: &str) -> anyhow::Result<BytesMut> {
    let (ws_stream, _) = connect_async(SYNTH_URL).await?;
    let (mut write, mut read) = ws_stream.split();

    let request_id = random_request_id();
    debug!("Generated request ID: {}", request_id);

    // Send config
    write
        .send(Message::Text(format!(
            "Content-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n{{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":false,\"wordBoundaryEnabled\":true}},\"outputFormat\":\"{}\"}}}}}}}}",
            output_format
        )))
        .await?;

    // Send SSML
    write
        .send(Message::Text(format!(
            "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nPath:ssml\r\n\r\n{}",
            request_id, ssml
        )))
        .await?;

    let mut buf = BytesMut::new();

    while let Some(msg) = read.next().await {
        match msg? {
            Message::Text(s) => {
                if let Some(header_str) = s.splitn(2, "\r\n\r\n").next() {
                    let headers = parse_headers(header_str);
                    if headers
                        .iter()
                        .any(|(k, v)| k == "Path" && v.trim() == "turn.end")
                    {
                        if headers
                            .iter()
                            .any(|(k, v)| k == "X-RequestId" && v.trim() == request_id)
                        {
                            return Ok(buf);
                        }
                        error!("Missing or invalid X-RequestId in turn.end");
                        return Err(anyhow!("Path:turn.end missing X-RequestId header"));
                    }
                }
            }
            Message::Binary(s) => {
                let header_len = (s[0] as usize) << 8 | s[1] as usize;
                if s.len() >= header_len + 2 {
                    let headers = parse_headers(String::from_utf8_lossy(&s[2..header_len]));
                    if headers
                        .iter()
                        .any(|(k, v)| k == "Path" && v.trim() == "audio")
                        && headers
                            .iter()
                            .any(|(k, v)| k == "X-RequestId" && v.trim() == request_id)
                    {
                        buf.extend_from_slice(&s[(header_len + 2)..]);
                    }
                }
            }
            _ => {}
        }
    }

    Err(anyhow!("WebSocket connection closed unexpectedly"))
}
