mod synthesize;
#[cfg(feature = "voice_list")]
mod voice_list;

pub use synthesize::{build_ssml, request_audio};
#[cfg(feature = "voice_list")]
pub use voice_list::get_voice_list;

// Re-export common types
pub use bytes::BytesMut;
