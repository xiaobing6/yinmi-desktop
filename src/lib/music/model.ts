export type SourceCode =
  | 'netease_music'
  | 'qq_music'
  | 'kuwo_music'
  | 'tidal'
  | 'qobuz'
  | 'joox'
  | 'bilibili_music'
  | 'apple_music'
  | 'youtube_music'
  | 'spotify';

export type SearchMode = 'track' | 'album' | 'playlist';
export type DownloadState = 'success' | 'skipped' | 'failed' | 'cancelled';
export type DownloadProgressState = 'preparing' | 'downloading' | 'finished';

export interface Song {
  id: string;
  name: string;
  artistDisplay: string;
  album: string | null;
  source: string;
  urlId: string | null;
  picId: string | null;
  lyricId: string | null;
  durationMs: number | null;
  hasHires: boolean;
}

export interface SearchResult {
  requestId: number;
  keyword: string;
  source: SourceCode;
  sourceName: string;
  mode: SearchMode;
  requestedCount: number;
  returnedCount: number;
  skippedRecords: number;
  incomplete: boolean;
  stopReason: string;
  songs: Song[];
}

export interface SearchStateSnapshot {
  active: boolean;
  result: SearchResult | null;
}

export interface SearchCompleteEvent {
  result: SearchResult | null;
  error: { code: string; message: string } | null;
}

export interface DownloadProgress {
  batchId: number;
  completed: number;
  total: number;
  currentSongId: string;
  currentName: string;
  succeeded: number;
  skipped: number;
  failed: number;
  cancelled: number;
  state: DownloadProgressState;
  completedItem: DownloadItemResult | null;
  currentDownloadedBytes: number;
  currentTotalBytes: number | null;
  bytesPerSecond: number;
}

export interface DownloadItemResult {
  songId: string;
  name: string;
  state: DownloadState;
  path: string | null;
  bytes: number;
  code: string | null;
  message: string | null;
  warnings: string[];
}

export interface DownloadBatchResult {
  batchId: number;
  directory: string;
  total: number;
  succeeded: number;
  skipped: number;
  failed: number;
  cancelled: number;
  items: DownloadItemResult[];
}

export interface DownloadStateSnapshot {
  active: boolean;
  progress: DownloadProgress | null;
  activeItems: DownloadItemResult[];
  lastResult: DownloadBatchResult | null;
}

export interface ExistingAudioScan {
  searchRequestId: number;
  directory: string;
  items: Array<{ songId: string; extensions: string[] }>;
}

export interface RateLimitNotice {
  waitSeconds: number;
}

export const SOURCES: ReadonlyArray<readonly [SourceCode, string]> = [
  ['netease_music', '网易云音乐'],
  ['qq_music', 'QQ 音乐'],
  ['kuwo_music', '酷我音乐'],
  ['tidal', 'TIDAL'],
  ['qobuz', 'Qobuz'],
  ['joox', 'JOOX'],
  ['bilibili_music', '哔哩哔哩'],
  ['apple_music', 'Apple Music'],
  ['youtube_music', 'YouTube Music'],
  ['spotify', 'Spotify'],
];

export const SEARCH_MODES: ReadonlyArray<readonly [SearchMode, string]> = [
  ['track', '单曲匹配'],
  ['album', '专辑匹配'],
  ['playlist', '歌单匹配'],
];

export function sourceLabel(code: string) {
  const aliases: Record<string, SourceCode> = {
    netease: 'netease_music',
    tencent: 'qq_music',
    kuwo: 'kuwo_music',
    bilibili: 'bilibili_music',
    apple: 'apple_music',
    ytmusic: 'youtube_music',
  };
  const internal = aliases[code] ?? code;
  return SOURCES.find(([value]) => value === internal)?.[1] ?? code;
}

export function errorText(error: unknown) {
  return typeof error === 'object' && error !== null && 'message' in error
    ? String((error as { message: unknown }).message)
    : String(error);
}

export function formatDuration(value: number | null) {
  if (value === null) return '--:--';
  const seconds = Math.max(0, Math.floor(value / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

export function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  if (value < 1024 * 1024 * 1024) {
    return `${(value / 1024 / 1024).toFixed(1)} MiB`;
  }
  return `${(value / 1024 / 1024 / 1024).toFixed(2)} GiB`;
}

export function downloadProgressPercent(progress: DownloadProgress) {
  if (!progress.currentTotalBytes) return null;
  return Math.min(
    100,
    Math.round(
      (progress.currentDownloadedBytes / progress.currentTotalBytes) * 100,
    ),
  );
}

export function stopReasonLabel(value: string) {
  const reasons: Record<string, string> = {
    target_reached: '已达到请求数量',
    empty_page: '音源没有更多结果',
    no_new_items: '后续页面没有新歌曲',
    safety_limit: '已达到安全分页上限',
    page_error: '后续页面请求失败',
    first_page_error: '首个页面请求失败',
  };
  return reasons[value] ?? value;
}
