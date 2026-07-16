use std::{
    collections::HashSet,
    fmt::{self, Write},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedComponent(Box<str>);

impl EncodedComponent {
    pub fn encode(raw: &str) -> Self {
        let mut output = String::with_capacity(raw.len());
        for byte in raw.as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                output.push(char::from(*byte));
            } else {
                write!(&mut output, "%{byte:02X}").expect("writing to String cannot fail");
            }
        }
        Self(output.into_boxed_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignatureValue(Box<str>);

impl TryFrom<&str> for SignatureValue {
    type Error = ContractError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > 128
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() || matches!(byte, b'&' | b'='))
        {
            return Err(ContractError::InvalidSignature);
        }
        Ok(Self(value.into()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchOperation {
    Track,
    Album,
    Playlist,
}

impl SearchOperation {
    const fn wire_type(self) -> &'static str {
        match self {
            Self::Track => "search",
            Self::Album => "search_album",
            Self::Playlist => "search_playlist",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GdSource {
    NeteaseMusic,
    QqMusic,
    KuwoMusic,
    Tidal,
    Qobuz,
    Joox,
    BilibiliMusic,
    AppleMusic,
    YoutubeMusic,
    Spotify,
}

impl GdSource {
    pub const DEFAULT: Self = Self::NeteaseMusic;

    pub const fn internal_code(self) -> &'static str {
        match self {
            Self::NeteaseMusic => "netease_music",
            Self::QqMusic => "qq_music",
            Self::KuwoMusic => "kuwo_music",
            Self::Tidal => "tidal",
            Self::Qobuz => "qobuz",
            Self::Joox => "joox",
            Self::BilibiliMusic => "bilibili_music",
            Self::AppleMusic => "apple_music",
            Self::YoutubeMusic => "youtube_music",
            Self::Spotify => "spotify",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::NeteaseMusic => "网易云音乐",
            Self::QqMusic => "QQ 音乐",
            Self::KuwoMusic => "酷我音乐",
            Self::Tidal => "TIDAL",
            Self::Qobuz => "Qobuz",
            Self::Joox => "JOOX",
            Self::BilibiliMusic => "哔哩哔哩",
            Self::AppleMusic => "Apple Music",
            Self::YoutubeMusic => "YouTube Music",
            Self::Spotify => "Spotify",
        }
    }

    pub const fn wire_value(self) -> &'static str {
        match self {
            Self::NeteaseMusic => "netease",
            Self::QqMusic => "tencent",
            Self::KuwoMusic => "kuwo",
            Self::Tidal => "tidal",
            Self::Qobuz => "qobuz",
            Self::Joox => "joox",
            Self::BilibiliMusic => "bilibili",
            Self::AppleMusic => "apple",
            Self::YoutubeMusic => "ytmusic",
            Self::Spotify => "spotify",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchCount(u16);

impl SearchCount {
    pub const DEFAULT: Self = Self(20);
    pub const MIN: u16 = 1;
    pub const MAX: u16 = 1_000;

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for SearchCount {
    type Error = ContractError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Self(value))
            .ok_or(ContractError::InvalidSearchCount)
    }
}

impl fmt::Display for SearchCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

#[derive(Clone, Debug)]
pub enum GdOperation {
    Search {
        operation: SearchOperation,
        count: SearchCount,
        source: GdSource,
        page: u16,
        name: EncodedComponent,
    },
    Url {
        id: EncodedComponent,
        source: GdSource,
        bitrate: u16,
    },
    Pic {
        id: EncodedComponent,
        source: GdSource,
        size: u16,
    },
    Lyric {
        id: EncodedComponent,
        source: GdSource,
    },
}

impl GdOperation {
    pub fn signature_input(&self) -> &EncodedComponent {
        match self {
            Self::Search { name, .. } => name,
            Self::Url { id, .. } | Self::Pic { id, .. } | Self::Lyric { id, .. } => id,
        }
    }
}

pub fn render_form_body(operation: &GdOperation, signature: &SignatureValue) -> String {
    let signature = &signature.0;
    match operation {
        GdOperation::Search {
            operation,
            count,
            source,
            page,
            name,
        } => format!(
            "types={}&count={count}&source={}&pages={page}&name={}&s={signature}",
            operation.wire_type(),
            source.wire_value(),
            name.as_str(),
        ),
        GdOperation::Url {
            id,
            source,
            bitrate,
        } => format!(
            "types=url&id={}&source={}&br={bitrate}&s={signature}",
            id.as_str(),
            source.wire_value()
        ),
        GdOperation::Pic { id, source, size } => format!(
            "types=pic&id={}&source={}&size={size}&s={signature}",
            id.as_str(),
            source.wire_value()
        ),
        GdOperation::Lyric { id, source } => format!(
            "types=lyric&id={}&source={}&s={signature}",
            id.as_str(),
            source.wire_value()
        ),
    }
}

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("search count must be between 1 and 1000")]
    InvalidSearchCount,
    #[error("top-level response is not an array")]
    InvalidTopLevel,
    #[error("non-empty response contained no valid songs")]
    NoValidSongs,
    #[error("upstream returned an explicit error")]
    UpstreamFailure,
    #[error("response URL is invalid or not HTTPS")]
    InvalidUrl,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ProbeSongKey {
    pub source: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbeSong {
    pub id: String,
    pub name: String,
    pub artists: Vec<String>,
    pub artist_display: String,
    pub album: Option<String>,
    pub source: String,
    pub url_id: Option<String>,
    pub pic_id: Option<String>,
    pub lyric_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub has_hires: bool,
}

#[derive(Clone, Debug)]
pub struct ParsedSearchPage {
    pub raw_records: usize,
    pub skipped_records: usize,
    pub songs: Vec<ProbeSong>,
}

fn wire_string(value: &Value) -> Option<String> {
    match value {
        Value::String(string) => Some(string.clone()),
        Value::Number(number) => number
            .as_u64()
            .filter(|number| *number <= 9_007_199_254_740_991)
            .map(|number| number.to_string()),
        _ => None,
    }
}

fn artists(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(string) if !string.is_empty() => Some(vec![string.clone()]),
        Value::Array(items) if !items.is_empty() => items
            .iter()
            .map(|item| {
                item.as_str()
                    .filter(|string| !string.is_empty())
                    .map(str::to_owned)
            })
            .collect::<Option<Vec<_>>>(),
        _ => None,
    }
}

fn duration_ms(value: Option<&Value>) -> Option<u64> {
    let seconds = match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(string) => string.parse().ok(),
        _ => None,
    }?;
    seconds.checked_mul(1_000)
}

pub fn parse_search_page(bytes: &[u8]) -> Result<ParsedSearchPage, ContractError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ContractError::InvalidTopLevel)?;
    let rows = value.as_array().ok_or(ContractError::InvalidTopLevel)?;
    let mut songs = Vec::new();

    for row in rows {
        let Some(object) = row.as_object() else {
            continue;
        };
        let (Some(id), Some(name), Some(source), Some(artists)) = (
            object.get("id").and_then(wire_string),
            object
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            object
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_owned),
            object.get("artist").and_then(artists),
        ) else {
            continue;
        };
        let album = match object.get("album") {
            Some(Value::String(album)) => Some(album.clone()),
            Some(Value::Null) | None => None,
            _ => continue,
        };
        let extra = object.get("extra_data").and_then(Value::as_object);
        songs.push(ProbeSong {
            id,
            name,
            artist_display: artists.join(", "),
            artists,
            album,
            source,
            url_id: object.get("url_id").and_then(wire_string),
            pic_id: object.get("pic_id").and_then(wire_string),
            lyric_id: object.get("lyric_id").and_then(wire_string),
            duration_ms: duration_ms(extra.and_then(|extra| extra.get("duration"))),
            has_hires: extra
                .and_then(|extra| extra.get("has_hires"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }

    if !rows.is_empty() && songs.is_empty() {
        return Err(ContractError::NoValidSongs);
    }
    Ok(ParsedSearchPage {
        raw_records: rows.len(),
        skipped_records: rows.len() - songs.len(),
        songs,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioUnavailableReason {
    EmptyUrl,
    LowerBitrate { requested: u32, reported: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioLocation {
    pub url: url::Url,
    pub reported_bitrate: Option<u32>,
    pub size_bytes: Option<u64>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioAvailability {
    Available(AudioLocation),
    Unavailable(AudioUnavailableReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PictureLocation {
    pub url: url::Url,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LyricPayload {
    pub original: Option<String>,
    pub translated: Option<String>,
}

impl LyricPayload {
    pub fn original_to_write(&self) -> Option<&str> {
        self.original.as_deref()
    }
}

fn response_object(bytes: &[u8]) -> Result<serde_json::Map<String, Value>, ContractError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ContractError::InvalidTopLevel)?;
    let object = value
        .as_object()
        .cloned()
        .ok_or(ContractError::InvalidTopLevel)?;
    if object.contains_key("error") {
        return Err(ContractError::UpstreamFailure);
    }
    Ok(object)
}

fn optional_u32(value: Option<&Value>) -> Result<Option<u32>, ContractError> {
    match value {
        None => Ok(None),
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .map(Some)
            .ok_or(ContractError::InvalidTopLevel),
        Some(_) => Err(ContractError::InvalidTopLevel),
    }
}

fn optional_u64(value: Option<&Value>) -> Result<Option<u64>, ContractError> {
    match value {
        None => Ok(None),
        Some(Value::Number(number)) => number
            .as_u64()
            .map(Some)
            .ok_or(ContractError::InvalidTopLevel),
        Some(_) => Err(ContractError::InvalidTopLevel),
    }
}

fn optional_string(value: Option<&Value>) -> Result<Option<String>, ContractError> {
    match value {
        None => Ok(None),
        Some(Value::String(string)) => Ok(Some(string.clone())),
        Some(_) => Err(ContractError::InvalidTopLevel),
    }
}

fn nonempty_string(value: Option<&Value>) -> Result<Option<String>, ContractError> {
    optional_string(value).map(|value| value.filter(|string| !string.is_empty()))
}

fn parse_resource_url(raw: &str) -> Result<url::Url, ContractError> {
    let has_userinfo = raw
        .split_once("://")
        .and_then(|(_, remainder)| remainder.split(['/', '?', '#']).next())
        .is_some_and(|authority| authority.contains('@'));
    let url = url::Url::parse(raw).map_err(|_| ContractError::InvalidUrl)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || has_userinfo
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ContractError::InvalidUrl);
    }
    Ok(url)
}

pub fn parse_audio_response(
    bytes: &[u8],
    requested_bitrate: u32,
) -> Result<AudioAvailability, ContractError> {
    let object = response_object(bytes)?;
    let raw_url = object
        .get("url")
        .and_then(Value::as_str)
        .ok_or(ContractError::InvalidTopLevel)?;
    if raw_url.is_empty() {
        return Ok(AudioAvailability::Unavailable(
            AudioUnavailableReason::EmptyUrl,
        ));
    }

    let reported_bitrate = optional_u32(object.get("br"))?;
    let size_bytes = optional_u64(object.get("size"))?;
    let source = optional_string(object.get("source"))?;
    let url = parse_resource_url(raw_url)?;

    if let Some(reported) = reported_bitrate
        && reported < requested_bitrate
    {
        return Ok(AudioAvailability::Unavailable(
            AudioUnavailableReason::LowerBitrate {
                requested: requested_bitrate,
                reported,
            },
        ));
    }

    Ok(AudioAvailability::Available(AudioLocation {
        url,
        reported_bitrate,
        size_bytes,
        source,
    }))
}

pub fn parse_picture_response(bytes: &[u8]) -> Result<PictureLocation, ContractError> {
    let object = response_object(bytes)?;
    let raw_url = object
        .get("url")
        .and_then(Value::as_str)
        .ok_or(ContractError::InvalidTopLevel)?;
    Ok(PictureLocation {
        url: parse_resource_url(raw_url)?,
    })
}

pub fn parse_lyric_response(bytes: &[u8]) -> Result<LyricPayload, ContractError> {
    let object = response_object(bytes)?;
    if !object.is_empty() && !object.contains_key("lyric") && !object.contains_key("tlyric") {
        return Err(ContractError::InvalidTopLevel);
    }
    let original = nonempty_string(object.get("lyric"))?;
    let translated = nonempty_string(object.get("tlyric"))?;
    Ok(LyricPayload {
        original,
        translated,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StopReason {
    TargetReached,
    RawEmptyPage,
    ExplicitNoMore,
    NoNewSongs,
    SafetyPageLimit,
    FirstPageFailed,
    LaterPageFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaginationDecision {
    Continue {
        next_page: u16,
    },
    Complete {
        reason: StopReason,
        incomplete: bool,
    },
    Failed {
        reason: StopReason,
    },
}

pub struct PaginationProbe {
    pub target_unique: usize,
    pub max_pages: u16,
    next_page: u16,
    seen: HashSet<ProbeSongKey>,
    pub songs: Vec<ProbeSong>,
}

impl PaginationProbe {
    pub fn new(target_unique: usize, max_pages: u16) -> Self {
        Self {
            target_unique,
            max_pages,
            next_page: 1,
            seen: HashSet::new(),
            songs: Vec::new(),
        }
    }

    pub fn push_page(
        &mut self,
        page: Result<(ParsedSearchPage, bool), ContractError>,
    ) -> PaginationDecision {
        let (page, explicit_no_more) = match page {
            Ok(page) => page,
            Err(_) if self.songs.is_empty() => {
                return PaginationDecision::Failed {
                    reason: StopReason::FirstPageFailed,
                };
            }
            Err(_) => {
                return PaginationDecision::Complete {
                    reason: StopReason::LaterPageFailed,
                    incomplete: true,
                };
            }
        };

        if page.raw_records == 0 {
            return PaginationDecision::Complete {
                reason: StopReason::RawEmptyPage,
                incomplete: false,
            };
        }

        let previous_unique = self.songs.len();
        for song in page.songs {
            let key = ProbeSongKey {
                source: song.source.clone(),
                id: song.id.clone(),
            };
            if self.seen.insert(key) {
                self.songs.push(song);
                if self.songs.len() >= self.target_unique {
                    return PaginationDecision::Complete {
                        reason: StopReason::TargetReached,
                        incomplete: false,
                    };
                }
            }
        }

        if explicit_no_more {
            return PaginationDecision::Complete {
                reason: StopReason::ExplicitNoMore,
                incomplete: false,
            };
        }

        if self.songs.len() == previous_unique {
            return PaginationDecision::Complete {
                reason: StopReason::NoNewSongs,
                incomplete: false,
            };
        }

        if self.next_page >= self.max_pages {
            return PaginationDecision::Complete {
                reason: StopReason::SafetyPageLimit,
                incomplete: true,
            };
        }

        self.next_page += 1;
        PaginationDecision::Continue {
            next_page: self.next_page,
        }
    }
}
