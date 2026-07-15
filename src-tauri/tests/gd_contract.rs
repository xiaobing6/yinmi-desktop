use yinmi_lib::music::contract::{
    AudioAvailability, AudioUnavailableReason, ContractError, EncodedComponent, GdOperation,
    GdSource, PaginationDecision, PaginationProbe, ParsedSearchPage, ProbeSong, SearchOperation,
    SignatureValue, StopReason, parse_audio_response, parse_lyric_response, parse_picture_response,
    parse_search_page, render_form_body,
};

const SIG: &str = "fixture-signature";

#[test]
fn renders_six_official_bodies_in_exact_order() {
    let name = EncodedComponent::encode("周杰伦");
    let id = EncodedComponent::encode("123456");
    let signature = SignatureValue::try_from(SIG).unwrap();

    let cases = [
        (
            GdOperation::Search {
                operation: SearchOperation::Track,
                count: 20,
                source: GdSource::NeteaseMusic,
                page: 1,
                name: name.clone(),
            },
            "types=search&count=20&source=netease&pages=1&name=%E5%91%A8%E6%9D%B0%E4%BC%A6&s=fixture-signature",
        ),
        (
            GdOperation::Search {
                operation: SearchOperation::Album,
                count: 20,
                source: GdSource::NeteaseMusic,
                page: 1,
                name: name.clone(),
            },
            "types=search_album&count=20&source=netease&pages=1&name=%E5%91%A8%E6%9D%B0%E4%BC%A6&s=fixture-signature",
        ),
        (
            GdOperation::Search {
                operation: SearchOperation::Playlist,
                count: 20,
                source: GdSource::NeteaseMusic,
                page: 1,
                name,
            },
            "types=search_playlist&count=20&source=netease&pages=1&name=%E5%91%A8%E6%9D%B0%E4%BC%A6&s=fixture-signature",
        ),
        (
            GdOperation::Url {
                id: id.clone(),
                source: GdSource::NeteaseMusic,
                bitrate: 320,
            },
            "types=url&id=123456&source=netease&br=320&s=fixture-signature",
        ),
        (
            GdOperation::Pic {
                id: id.clone(),
                source: GdSource::NeteaseMusic,
                size: 300,
            },
            "types=pic&id=123456&source=netease&size=300&s=fixture-signature",
        ),
        (
            GdOperation::Lyric {
                id,
                source: GdSource::NeteaseMusic,
            },
            "types=lyric&id=123456&source=netease&s=fixture-signature",
        ),
    ];

    for (operation, expected) in cases {
        assert_eq!(render_form_body(&operation, &signature), expected);
    }
}

#[test]
fn matches_javascript_component_encoding_without_double_encoding() {
    assert_eq!(
        EncodedComponent::encode("A B!'()*/?=%").as_str(),
        "A%20B%21%27%28%29%2A%2F%3F%3D%25"
    );
    assert_eq!(
        EncodedComponent::encode("id/42?x=1%").as_str(),
        "id%2F42%3Fx%3D1%25"
    );
    assert!(!EncodedComponent::encode("A B").as_str().contains("%2520"));
}

#[test]
fn rejects_unsafe_signature_values() {
    for value in ["", "has&separator", "has=separator", "has\ncontrol"] {
        assert!(matches!(
            SignatureValue::try_from(value),
            Err(ContractError::InvalidSignature)
        ));
    }
    let too_long = "x".repeat(129);
    assert!(matches!(
        SignatureValue::try_from(too_long.as_str()),
        Err(ContractError::InvalidSignature)
    ));
}

#[test]
fn exposes_the_encoded_signature_input_for_every_operation() {
    let encoded = EncodedComponent::encode("id/42");
    let operations = [
        GdOperation::Search {
            operation: SearchOperation::Track,
            count: 20,
            source: GdSource::NeteaseMusic,
            page: 1,
            name: encoded.clone(),
        },
        GdOperation::Url {
            id: encoded.clone(),
            source: GdSource::NeteaseMusic,
            bitrate: 320,
        },
        GdOperation::Pic {
            id: encoded.clone(),
            source: GdSource::NeteaseMusic,
            size: 300,
        },
        GdOperation::Lyric {
            id: encoded,
            source: GdSource::NeteaseMusic,
        },
    ];

    for operation in operations {
        assert_eq!(operation.signature_input().as_str(), "id%2F42");
    }
}

#[test]
fn maps_all_registered_sources_and_pins_the_default() {
    let cases = [
        (
            GdSource::NeteaseMusic,
            "netease_music",
            "网易云音乐",
            "netease",
        ),
        (GdSource::QqMusic, "qq_music", "QQ 音乐", "tencent"),
        (GdSource::KuwoMusic, "kuwo_music", "酷我音乐", "kuwo"),
        (GdSource::Tidal, "tidal", "TIDAL", "tidal"),
        (GdSource::Qobuz, "qobuz", "Qobuz", "qobuz"),
        (GdSource::Joox, "joox", "JOOX", "joox"),
        (
            GdSource::BilibiliMusic,
            "bilibili_music",
            "哔哩哔哩",
            "bilibili",
        ),
        (GdSource::AppleMusic, "apple_music", "Apple Music", "apple"),
        (
            GdSource::YoutubeMusic,
            "youtube_music",
            "YouTube Music",
            "ytmusic",
        ),
        (GdSource::Spotify, "spotify", "Spotify", "spotify"),
    ];

    for (source, internal_code, display_name, wire_value) in cases {
        assert_eq!(source.internal_code(), internal_code);
        assert_eq!(source.display_name(), display_name);
        assert_eq!(source.wire_value(), wire_value);
    }
    assert_eq!(GdSource::DEFAULT, GdSource::NeteaseMusic);
}

#[test]
fn normalizes_mixed_records_and_skips_bad_rows() {
    let report = parse_search_page(include_bytes!("fixtures/gd/search_mixed.json")).unwrap();
    assert_eq!(report.raw_records, 4);
    assert_eq!(report.songs.len(), 2);
    assert_eq!(report.skipped_records, 2);
    assert_eq!(report.songs[0].duration_ms, Some(123_000));
    assert!(report.songs[0].has_hires);
    assert_eq!(report.songs[1].id, "9007199254740991");
    assert_eq!(report.songs[1].artists, ["歌手乙", "歌手丙"]);
    assert_eq!(report.songs[1].duration_ms, Some(10_000));
    assert!(!report.songs[1].has_hires);
}

#[test]
fn distinguishes_empty_from_incompatible() {
    assert!(
        parse_search_page(include_bytes!("fixtures/gd/search_empty.json"))
            .unwrap()
            .songs
            .is_empty()
    );
    assert!(matches!(
        parse_search_page(include_bytes!("fixtures/gd/search_incompatible.json")),
        Err(ContractError::InvalidTopLevel)
    ));
}

#[test]
fn rejects_nonempty_search_pages_without_a_valid_song() {
    assert!(matches!(
        parse_search_page(
            br#"[{"id":9007199254740992,"name":"too large","artist":"artist","source":"netease"}]"#
        ),
        Err(ContractError::NoValidSongs)
    ));
}

#[test]
fn parses_audio_success_and_unavailable_variants() {
    let available =
        parse_audio_response(include_bytes!("fixtures/gd/url_success.json"), 320).unwrap();
    let AudioAvailability::Available(location) = available else {
        panic!("normal HTTPS URL must be available");
    };
    assert_eq!(location.url.as_str(), "https://cdn.example.invalid/audio");
    assert_eq!(location.reported_bitrate, Some(320));
    assert_eq!(location.size_bytes, Some(1024));
    assert_eq!(location.source.as_deref(), Some("netease"));

    assert_eq!(
        parse_audio_response(include_bytes!("fixtures/gd/url_empty.json"), 320).unwrap(),
        AudioAvailability::Unavailable(AudioUnavailableReason::EmptyUrl)
    );
    assert_eq!(
        parse_audio_response(include_bytes!("fixtures/gd/url_lower_bitrate.json"), 320).unwrap(),
        AudioAvailability::Unavailable(AudioUnavailableReason::LowerBitrate {
            requested: 320,
            reported: 128,
        })
    );
}

#[test]
fn keeps_audio_available_when_bitrate_is_missing() {
    let availability =
        parse_audio_response(include_bytes!("fixtures/gd/url_missing_bitrate.json"), 320).unwrap();
    let AudioAvailability::Available(location) = availability else {
        panic!("missing reported bitrate must remain available");
    };
    assert_eq!(location.reported_bitrate, None);
}

#[test]
fn parses_picture_and_keeps_lyric_fields_separate() {
    let picture = parse_picture_response(include_bytes!("fixtures/gd/pic_success.json")).unwrap();
    assert_eq!(
        picture.url.as_str(),
        "https://cdn.example.invalid/cover.jpg"
    );

    let lyrics = parse_lyric_response(include_bytes!("fixtures/gd/lyric_success.json")).unwrap();
    assert_eq!(lyrics.original.as_deref(), Some("[00:00.00]测试"));
    assert_eq!(lyrics.translated.as_deref(), Some("[00:00.00]Test"));
    assert_eq!(lyrics.original_to_write(), Some("[00:00.00]测试"));
    assert!(!lyrics.original_to_write().unwrap().contains("Test"));
}

#[test]
fn empty_original_lyric_creates_no_attachment() {
    let lyrics = parse_lyric_response(include_bytes!("fixtures/gd/lyric_empty.json")).unwrap();
    assert_eq!(lyrics.original, None);
    assert_eq!(lyrics.translated, None);
    assert_eq!(lyrics.original_to_write(), None);
}

#[test]
fn accepts_missing_lyric_fields_as_no_attachment() {
    let lyrics = parse_lyric_response(br#"{}"#).unwrap();
    assert_eq!(lyrics.original, None);
    assert_eq!(lyrics.translated, None);
    assert_eq!(lyrics.original_to_write(), None);
}

#[test]
fn maps_explicit_errors_without_exposing_the_upstream_message() {
    let fixture = include_bytes!("fixtures/gd/explicit_error.json");

    let audio_error = parse_audio_response(fixture, 320).unwrap_err();
    assert!(matches!(audio_error, ContractError::UpstreamFailure));
    assert_eq!(
        audio_error.to_string(),
        "upstream returned an explicit error"
    );
    assert!(matches!(
        parse_picture_response(fixture),
        Err(ContractError::UpstreamFailure)
    ));
    assert!(matches!(
        parse_lyric_response(fixture),
        Err(ContractError::UpstreamFailure)
    ));
}

#[test]
fn requires_absolute_credential_free_https_resource_urls() {
    for url in [
        "http://cdn.example.invalid/audio",
        "/relative/audio",
        "https://user:pass@cdn.example.invalid/audio",
        "https://@cdn.example.invalid/audio",
        "HTTPS://@cdn.example.invalid/audio",
    ] {
        let body = format!(r#"{{"url":"{url}"}}"#);
        assert!(matches!(
            parse_audio_response(body.as_bytes(), 320),
            Err(ContractError::InvalidUrl)
        ));
        assert!(matches!(
            parse_picture_response(body.as_bytes()),
            Err(ContractError::InvalidUrl)
        ));
    }
}

#[test]
fn rejects_malformed_or_unrecognized_response_shapes() {
    for body in [
        b"not json".as_slice(),
        br#"[]"#,
        br#"{"url":42}"#,
        br#"{"url":"https://cdn.example.invalid/audio","br":"320"}"#,
        br#"{"url":"https://cdn.example.invalid/audio","br":null}"#,
        br#"{"url":"https://cdn.example.invalid/audio","size":-1}"#,
        br#"{"url":"https://cdn.example.invalid/audio","source":7}"#,
    ] {
        assert!(matches!(
            parse_audio_response(body, 320),
            Err(ContractError::InvalidTopLevel)
        ));
    }

    assert!(matches!(
        parse_picture_response(br#"{"unexpected":true}"#),
        Err(ContractError::InvalidTopLevel)
    ));
    for body in [
        br#"{"unexpected":true}"#.as_slice(),
        br#"{"lyric":7}"#,
        br#"{"lyric":null}"#,
        br#"{"lyric":"ok","tlyric":7}"#,
    ] {
        assert!(matches!(
            parse_lyric_response(body),
            Err(ContractError::InvalidTopLevel)
        ));
    }
}

fn probe_song(source: &str, id: impl ToString, name: &str) -> ProbeSong {
    ProbeSong {
        id: id.to_string(),
        name: name.to_owned(),
        artists: vec!["artist".to_owned()],
        artist_display: "artist".to_owned(),
        album: None,
        source: source.to_owned(),
        url_id: None,
        pic_id: None,
        lyric_id: None,
        duration_ms: None,
        has_hires: false,
    }
}

fn parsed_page(songs: Vec<ProbeSong>) -> ParsedSearchPage {
    ParsedSearchPage {
        raw_records: songs.len(),
        skipped_records: 0,
        songs,
    }
}

fn raw_empty_page() -> ParsedSearchPage {
    ParsedSearchPage {
        raw_records: 0,
        skipped_records: 0,
        songs: Vec::new(),
    }
}

#[test]
fn pagination_stops_when_target_is_reached() {
    let mut probe = PaginationProbe::new(2, 1);

    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![
                probe_song("netease", "one", "first"),
                probe_song("netease", "two", "second"),
            ]),
            false,
        ))),
        PaginationDecision::Complete {
            reason: StopReason::TargetReached,
            incomplete: false,
        }
    );
    assert_eq!(probe.songs.len(), 2);
}

#[test]
fn pagination_never_overshoots_the_remaining_target() {
    let mut probe = PaginationProbe::new(2, 10);

    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![
                probe_song("netease", "one", "first"),
                probe_song("netease", "two", "second"),
                probe_song("netease", "three", "must not be retained"),
            ]),
            false,
        ))),
        PaginationDecision::Complete {
            reason: StopReason::TargetReached,
            incomplete: false,
        }
    );
    assert_eq!(probe.songs.len(), 2);
    assert_eq!(probe.songs[0].id, "one");
    assert_eq!(probe.songs[1].id, "two");
}

#[test]
fn pagination_stops_on_a_raw_empty_page() {
    let mut probe = PaginationProbe::new(5, 1);

    assert_eq!(
        probe.push_page(Ok((raw_empty_page(), false))),
        PaginationDecision::Complete {
            reason: StopReason::RawEmptyPage,
            incomplete: false,
        }
    );
}

#[test]
fn pagination_stops_on_an_explicit_no_more_signal() {
    let mut probe = PaginationProbe::new(5, 1);

    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![probe_song("netease", "one", "first")]),
            true,
        ))),
        PaginationDecision::Complete {
            reason: StopReason::ExplicitNoMore,
            incomplete: false,
        }
    );
}

#[test]
fn pagination_stops_when_a_page_adds_no_new_songs() {
    let mut probe = PaginationProbe::new(5, 2);
    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![probe_song("netease", "one", "first")]),
            false,
        ))),
        PaginationDecision::Continue { next_page: 2 }
    );

    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![probe_song("netease", "one", "duplicate")]),
            false,
        ))),
        PaginationDecision::Complete {
            reason: StopReason::NoNewSongs,
            incomplete: false,
        }
    );
    assert_eq!(probe.songs[0].name, "first");
}

#[test]
fn pagination_marks_the_safety_page_limit_incomplete() {
    let mut probe = PaginationProbe::new(5, 1);

    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![probe_song("netease", "one", "first")]),
            false,
        ))),
        PaginationDecision::Complete {
            reason: StopReason::SafetyPageLimit,
            incomplete: true,
        }
    );
}

#[test]
fn pagination_fails_when_the_first_page_fails() {
    let mut probe = PaginationProbe::new(5, 10);

    assert_eq!(
        probe.push_page(Err(ContractError::InvalidTopLevel)),
        PaginationDecision::Failed {
            reason: StopReason::FirstPageFailed,
        }
    );
}

#[test]
fn pagination_returns_incomplete_results_when_a_later_page_fails() {
    let mut probe = PaginationProbe::new(5, 10);
    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![probe_song("netease", "one", "first")]),
            false,
        ))),
        PaginationDecision::Continue { next_page: 2 }
    );

    assert_eq!(
        probe.push_page(Err(ContractError::InvalidTopLevel)),
        PaginationDecision::Complete {
            reason: StopReason::LaterPageFailed,
            incomplete: true,
        }
    );
    assert_eq!(probe.songs.len(), 1);
}

#[test]
fn pagination_keeps_the_first_record_for_each_source_and_id() {
    let mut probe = PaginationProbe::new(10, 10);

    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![
                probe_song("netease", "same", "first wins"),
                probe_song("netease", "same", "duplicate loses"),
                probe_song("tencent", "same", "different source"),
            ]),
            false,
        ))),
        PaginationDecision::Continue { next_page: 2 }
    );
    assert_eq!(probe.songs.len(), 2);
    assert_eq!(probe.songs[0].name, "first wins");
    assert_eq!(probe.songs[1].name, "different source");
}

#[test]
fn pagination_stops_exactly_at_one_thousand_unique_songs() {
    let mut probe = PaginationProbe::new(1_000, 20);

    for page_number in 1_u16..=10 {
        let first_id = usize::from(page_number - 1) * 100;
        let songs = (first_id..first_id + 100)
            .map(|id| probe_song("netease", id, "generated"))
            .collect();
        let decision = probe.push_page(Ok((parsed_page(songs), false)));

        if page_number < 10 {
            assert_eq!(
                decision,
                PaginationDecision::Continue {
                    next_page: page_number + 1,
                }
            );
        } else {
            assert_eq!(
                decision,
                PaginationDecision::Complete {
                    reason: StopReason::TargetReached,
                    incomplete: false,
                }
            );
        }
    }

    assert_eq!(probe.songs.len(), 1_000);
    assert_eq!(probe.songs.first().unwrap().id, "0");
    assert_eq!(probe.songs.last().unwrap().id, "999");
}

#[test]
fn pagination_treats_normal_exhaustion_below_target_as_complete() {
    let mut probe = PaginationProbe::new(5, 10);
    assert_eq!(
        probe.push_page(Ok((
            parsed_page(vec![
                probe_song("netease", "one", "first"),
                probe_song("netease", "two", "second"),
            ]),
            false,
        ))),
        PaginationDecision::Continue { next_page: 2 }
    );

    assert_eq!(
        probe.push_page(Ok((raw_empty_page(), false))),
        PaginationDecision::Complete {
            reason: StopReason::RawEmptyPage,
            incomplete: false,
        }
    );
    assert_eq!(probe.songs.len(), 2);
}
