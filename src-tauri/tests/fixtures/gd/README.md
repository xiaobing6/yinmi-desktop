# GD contract fixtures

Every file in this directory is a hand-authored minimal contract sample derived from design commit `5893d4340a4815677da79f74223642ac855519e7` and official page version `2026.06.16`.

| Fixture | Contract rule proved |
| --- | --- |
| `search_mixed.json` | Valid string and safe-integer IDs normalize, supported artist shapes normalize, and invalid rows are skipped whole. |
| `search_empty.json` | A raw empty search page is a compatible successful response. |
| `search_incompatible.json` | A non-array search response is incompatible. |
| `url_success.json` | A normal HTTPS audio location includes bitrate, size, and source metadata. |
| `url_empty.json` | An explicitly empty audio URL means unavailable audio. |
| `url_lower_bitrate.json` | An explicitly lower reported bitrate means unavailable at the requested quality. |
| `url_missing_bitrate.json` | A missing reported bitrate does not by itself make an HTTPS audio URL unavailable. |
| `pic_success.json` | A normal HTTPS picture URL parses. |
| `lyric_success.json` | Original and translated lyrics remain separate, with the original eligible for writing. |
| `lyric_empty.json` | Empty original lyrics produce no attachment payload. |
| `explicit_error.json` | An explicit upstream error is mapped to the stable contract error without exposing its message. |

No raw third-party song row or signature is stored in these fixtures.
