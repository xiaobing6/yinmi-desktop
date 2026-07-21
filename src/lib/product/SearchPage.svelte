<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getVersion } from '@tauri-apps/api/app';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import {
    SEARCH_MODES,
    SOURCES,
    errorText,
    type DownloadBatchResult,
    type DownloadItemResult,
    type DownloadProgress,
    type DownloadStateSnapshot,
    type ExistingAudioScan,
    type RateLimitNotice,
    type SearchCompleteEvent,
    type SearchMode,
    type SearchResult,
    type SearchStateSnapshot,
    type Song,
    type SourceCode,
  } from '../music/model';
  import DownloadBar from './DownloadBar.svelte';
  import FolderOpenIcon from './icons/FolderOpenIcon.svelte';
  import RuntimeTools from './RuntimeTools.svelte';
  import SearchResults from './SearchResults.svelte';
  import WindowControls from './WindowControls.svelte';
  import {
    buildQueueStats,
    summarizeItems,
    type QueueStats,
    type RetryTarget,
  } from './downloadView';
  import { titlebar } from './windowChrome';

  let keyword = '';
  let source: SourceCode = 'netease_music';
  let mode: SearchMode = 'track';
  let count = 20;
  let searching = false;
  let result: SearchResult | null = null;
  let errorMessage = '';
  let selected = new Set<string>();
  let requestSerial = 0;
  let existingAudio = new Map<string, string[]>();
  let rateLimitSeconds = 0;
  let appVersion = '';

  let bitrate = 320;
  let embedCover = true;
  let downloadLyrics = true;
  let defaultDirectory = '';
  let baseDirectory = '';
  let directoryLoading = true;
  let directoryMessage = '';

  let downloading = false;
  let downloadProgress: DownloadProgress | null = null;
  let downloadResult: DownloadBatchResult | null = null;
  let downloadError = '';
  let downloadItems = new Map<string, DownloadItemResult>();
  let cancellingScope: 'current' | 'all' | null = null;
  let retryingTarget: RetryTarget = null;
  let retryPendingSongIds = new Set<string>();

  onMount(() => {
    let disposed = false;
    const detach: Array<() => void> = [];
    void getVersion().then((value) => {
      if (!disposed) appVersion = value;
    });
    void invoke<string>('music_get_default_directory')
      .then((directory) => {
        if (disposed) return;
        defaultDirectory = directory;
        if (!baseDirectory.trim()) baseDirectory = directory;
        void scanExisting();
      })
      .catch((error) => {
        if (!disposed)
          directoryMessage = `无法读取默认目录：${errorText(error)}`;
      })
      .finally(() => {
        if (!disposed) directoryLoading = false;
      });
    void listen<DownloadProgress>('music-download-progress', (event) => {
      downloading = true;
      if (event.payload.completedItem) {
        downloadItems = new Map(downloadItems).set(
          event.payload.completedItem.songId,
          event.payload.completedItem,
        );
        if (retryPendingSongIds.has(event.payload.completedItem.songId)) {
          const pending = new Set(retryPendingSongIds);
          pending.delete(event.payload.completedItem.songId);
          retryPendingSongIds = pending;
        }
      }
      downloadProgress = event.payload;
      if (event.payload.state === 'finished') cancellingScope = null;
    }).then((stop) => {
      if (disposed) stop();
      else detach.push(stop);
    });
    void listen<DownloadBatchResult>('music-download-complete', (event) => {
      mergeDownloadResult(event.payload);
      downloading = false;
      cancellingScope = null;
      retryingTarget = null;
      retryPendingSongIds = new Set();
      void scanExisting();
    }).then((stop) => {
      if (disposed) stop();
      else detach.push(stop);
    });
    void listen<RateLimitNotice>('music-rate-limit', (event) => {
      rateLimitSeconds = event.payload.waitSeconds;
    }).then((stop) => {
      if (disposed) stop();
      else detach.push(stop);
    });
    void listen<SearchCompleteEvent>('music-search-complete', (event) => {
      searching = false;
      if (event.payload.result) {
        errorMessage = '';
        applySearchSnapshot(event.payload.result);
      } else if (event.payload.error) {
        errorMessage = event.payload.error.message;
      }
    }).then((stop) => {
      if (disposed) stop();
      else detach.push(stop);
    });
    void invoke<SearchStateSnapshot>('music_get_search_snapshot').then(
      (snapshot) => {
        if (disposed) return;
        if (snapshot.active) searching = true;
        if (snapshot.result && !result) {
          applySearchSnapshot(snapshot.result, true);
        }
      },
    );
    void invoke<DownloadStateSnapshot>('music_get_download_snapshot').then(
      (snapshot) => {
        if (!disposed) applyDownloadSnapshot(snapshot);
      },
    );
    return () => {
      disposed = true;
      for (const stop of detach) stop();
    };
  });

  function applySearchSnapshot(value: SearchResult, restoreForm = false) {
    result = value;
    existingAudio = new Map();
    if (restoreForm) {
      keyword = value.keyword;
      source = value.source;
      mode = value.mode;
      count = value.requestedCount;
    }
    void scanExisting();
  }

  function applyDownloadSnapshot(snapshot: DownloadStateSnapshot) {
    if (snapshot.lastResult) {
      downloadItems = new Map();
      mergeDownloadResult(snapshot.lastResult);
    }
    if (snapshot.active && snapshot.progress) {
      downloadItems = new Map(
        snapshot.activeItems.map((item) => [item.songId, item]),
      );
      downloading = true;
      downloadProgress = snapshot.progress;
    } else if (!snapshot.active) {
      downloading = false;
    }
  }

  async function scanExisting() {
    const current = result;
    const directory = baseDirectory.trim() || defaultDirectory;
    if (!current || !directory) return;
    try {
      const scan = await invoke<ExistingAudioScan>('music_scan_existing', {
        request: {
          searchRequestId: current.requestId,
          baseDirectory: directory,
        },
      });
      if (result?.requestId === scan.searchRequestId) {
        existingAudio = new Map(
          scan.items.map((item) => [item.songId, item.extensions]),
        );
        directoryMessage = '';
      }
    } catch (error) {
      if (result?.requestId === current.requestId) {
        directoryMessage = `无法扫描已有文件：${errorText(error)}`;
      }
    }
  }

  const keyOf = (song: Song) => `${song.source}:${song.id}`;
  const downloadableSongs = () =>
    result?.songs.filter((song) => song.urlId) ?? [];
  const selectedSongs = () =>
    downloadableSongs().filter((song) => selected.has(keyOf(song)));
  let downloadableCount = 0;
  $: downloadableCount =
    result?.songs.filter((song) => song.urlId).length ?? 0;

  function clearResultState() {
    result = null;
    selected = new Set();
    errorMessage = '';
    downloadResult = null;
    downloadProgress = null;
    downloadError = '';
    downloadItems = new Map();
    existingAudio = new Map();
    cancellingScope = null;
    retryingTarget = null;
    retryPendingSongIds = new Set();
  }

  function resetResultState() {
    requestSerial += 1;
    clearResultState();
  }

  function searchSettingChanged() {
    if (
      result ||
      errorMessage ||
      selected.size ||
      downloadResult ||
      downloadItems.size
    ) {
      resetResultState();
    }
  }

  function mergeDownloadResult(value: DownloadBatchResult) {
    const merged = new Map([
      ...downloadItems,
      ...value.items.map((item) => [item.songId, item] as const),
    ]);
    downloadItems = merged;

    const items = [...merged.values()];
    const summary = summarizeItems(items);
    downloadResult = {
      ...value,
      total: items.length,
      ...summary,
      items,
    };
  }

  let queueStats: QueueStats;
  $: queueStats = buildQueueStats(
    downloading,
    downloadProgress,
    retryingTarget,
    downloadItems,
    retryPendingSongIds,
    downloadResult,
    selected.size,
  );

  async function search() {
    if (downloading) return;
    const normalizedCount = Math.min(
      1000,
      Math.max(1, Math.trunc(Number(count) || 20)),
    );
    count = normalizedCount;
    const context = {
      keyword: keyword.trim(),
      source,
      mode,
      count: normalizedCount,
    };
    const serial = ++requestSerial;
    searching = true;
    clearResultState();
    try {
      const value = await invoke<SearchResult>('music_search', {
        request: context,
      });
      if (serial === requestSerial) applySearchSnapshot(value);
    } catch (error) {
      if (serial === requestSerial) errorMessage = errorText(error);
    } finally {
      if (serial === requestSerial) searching = false;
    }
  }

  function toggle(song: Song) {
    if (!song.urlId || downloading) return;
    const key = keyOf(song);
    selected = selected.has(key)
      ? new Set([...selected].filter((value) => value !== key))
      : new Set([...selected, key]);
  }

  function toggleAll() {
    if (!result || downloading) return;
    const downloadable = downloadableSongs();
    selected =
      selected.size === downloadable.length
        ? new Set()
        : new Set(downloadable.map(keyOf));
  }

  async function downloadSelected() {
    const songs = selectedSongs();
    if (!result || songs.length === 0 || downloading) return;
    downloading = true;
    retryingTarget = null;
    retryPendingSongIds = new Set();
    downloadError = '';
    downloadResult = null;
    downloadProgress = {
      batchId: 0,
      completed: 0,
      total: songs.length,
      currentSongId: songs[0].id,
      currentName: songs[0].name,
      succeeded: 0,
      skipped: 0,
      failed: 0,
      cancelled: 0,
      state: 'preparing',
      completedItem: null,
      currentDownloadedBytes: 0,
      currentTotalBytes: null,
      bytesPerSecond: 0,
    };
    try {
      const value = await invoke<DownloadBatchResult>('music_download_batch', {
        request: {
          searchRequestId: result.requestId,
          songIds: songs.map((song) => song.id),
          bitrate: Number(bitrate),
          embedCover,
          downloadLyrics,
          baseDirectory: baseDirectory.trim(),
        },
      });
      downloadItems = new Map();
      mergeDownloadResult(value);
    } catch (error) {
      downloadError = errorText(error);
    } finally {
      downloading = false;
      cancellingScope = null;
    }
  }

  async function retryFailed(songId: string | null) {
    if (downloading) return;
    const retryable = [...downloadItems.values()].filter((item) =>
      songId
        ? item.songId === songId &&
          (item.state === 'failed' || item.state === 'cancelled')
        : item.state === 'failed' || item.state === 'cancelled',
    );
    if (retryable.length === 0) return;

    downloading = true;
    retryingTarget = songId ?? 'all';
    retryPendingSongIds = new Set(retryable.map((item) => item.songId));
    downloadError = '';
    downloadProgress = {
      batchId: 0,
      completed: 0,
      total: retryable.length,
      currentSongId: retryable[0].songId,
      currentName: retryable[0].name,
      succeeded: 0,
      skipped: 0,
      failed: 0,
      cancelled: 0,
      state: 'preparing',
      completedItem: null,
      currentDownloadedBytes: 0,
      currentTotalBytes: null,
      bytesPerSecond: 0,
    };
    try {
      const value = await invoke<DownloadBatchResult>('music_retry_failed', {
        songId,
      });
      mergeDownloadResult(value);
    } catch (error) {
      downloadError = errorText(error);
    } finally {
      downloading = false;
      retryingTarget = null;
      retryPendingSongIds = new Set();
      cancellingScope = null;
    }
  }

  async function restoreDefaultPreferences() {
    if (downloading || directoryLoading) return;
    bitrate = 320;
    embedCover = true;
    downloadLyrics = true;
    if (defaultDirectory) {
      baseDirectory = defaultDirectory;
      directoryMessage = '';
      void scanExisting();
      return;
    }
    directoryLoading = true;
    directoryMessage = '';
    try {
      defaultDirectory = await invoke<string>('music_get_default_directory');
      baseDirectory = defaultDirectory;
      void scanExisting();
    } catch (error) {
      directoryMessage = `无法读取默认目录：${errorText(error)}`;
    } finally {
      directoryLoading = false;
    }
  }

  async function chooseDirectory() {
    if (downloading || directoryLoading) return;
    directoryMessage = '';
    try {
      const selectedDirectory = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: baseDirectory || defaultDirectory || undefined,
        title: '选择音乐保存目录',
      });
      if (typeof selectedDirectory === 'string') {
        baseDirectory = selectedDirectory;
        void scanExisting();
      }
    } catch (error) {
      directoryMessage = `无法选择目录：${errorText(error)}`;
    }
  }

  async function cancelCurrent() {
    if (!downloading || cancellingScope) return;
    cancellingScope = 'current';
    try {
      await invoke('music_cancel_current_download');
    } catch (error) {
      downloadError = errorText(error);
      cancellingScope = null;
    }
  }

  async function cancelAll() {
    if (!downloading || cancellingScope) return;
    cancellingScope = 'all';
    try {
      await invoke('music_cancel_all_downloads');
    } catch (error) {
      downloadError = errorText(error);
      cancellingScope = null;
    }
  }

  async function openDirectory() {
    downloadError = '';
    try {
      await invoke('music_open_download_directory');
    } catch (error) {
      downloadError = errorText(error);
    }
  }
</script>

<svelte:head><title>音觅</title></svelte:head>

<main class="shell">
  <header class="topbar" use:titlebar>
    <div class="mark" aria-hidden="true"><i></i><i></i><b></b></div>
    <div class="brand-copy">
      <h1>音觅</h1>
      <p>发现音乐，也收藏音乐</p>
    </div>
    <div
      class:active={searching || downloading}
      class:downloading
      class="signal-rail"
      aria-label={downloading ? '正在下载' : searching ? '正在搜索' : '准备就绪'}
    >
      <b></b>
      <span>{downloading ? '正在下载' : searching ? '正在搜索' : '准备就绪'}</span>
    </div>
    <RuntimeTools />
    <span class="version">v{appVersion || '0.1.3'}</span>
    <WindowControls />
  </header>

  <div class="workspace">
  <aside class="search-panel" aria-label="搜索与下载设置">
    <header class="panel-heading">
      <span>音乐搜索</span>
      <h2>今天想听什么？</h2>
      <p>输入一个线索，从多个音源找到合适的版本。</p>
    </header>
    <div class="section-label"><span>搜索条件</span><b>发现音乐</b></div>
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void search();
      }}
    >
      <label class="keyword"
        ><span>搜索关键词</span><input
          bind:value={keyword}
          maxlength="200"
          placeholder="歌曲、歌手、专辑或歌单"
          required
          disabled={searching || downloading}
          oninput={searchSettingChanged}
        /></label
      >
      <label
        ><span>音源</span><select
          bind:value={source}
          disabled={searching || downloading}
          onchange={searchSettingChanged}
          >{#each SOURCES as item (item[0])}<option value={item[0]}
              >{item[1]}</option
            >{/each}</select
        ></label
      >
      <label
        ><span>匹配方式</span><select
          bind:value={mode}
          disabled={searching || downloading}
          onchange={searchSettingChanged}
          >{#each SEARCH_MODES as item (item[0])}<option value={item[0]}
              >{item[1]}</option
            >{/each}</select
        ></label
      >
      <label
        ><span>数量</span><input
          bind:value={count}
          type="number"
          min="1"
          max="1000"
          required
          disabled={searching || downloading}
          oninput={searchSettingChanged}
        /></label
      >
      <button
        class="primary"
        type="submit"
        disabled={searching || downloading || keyword.trim().length === 0}
        >{searching ? '正在搜索…' : '开始搜索'}</button
      >
    </form>
    <p class="hint">数量范围 1–1000，三种匹配方式均返回歌曲列表。</p>
    <div class="section-label download-label">
      <span>保存偏好</span>
      <div class="section-actions">
        <b>带回本地</b>
        <button
          class="reset-directory"
          type="button"
          disabled={downloading || directoryLoading}
          onclick={() => void restoreDefaultPreferences()}>恢复默认</button
        >
      </div>
    </div>
    <div class="download-settings" aria-label="下载设置">
      <label class="quality-setting">
        <span>音质</span>
        <select
          bind:value={bitrate}
          disabled={downloading}
          aria-label="下载音质"
        >
          <option value={128}>128 kbps</option>
          <option value={192}>192 kbps</option>
          <option value={320}>320 kbps</option>
          <option value={740}>740 无损</option>
          <option value={999}>999 Hi-Res</option>
        </select>
      </label>
      <label class="toggle-setting">
        <input
          type="checkbox"
          bind:checked={embedCover}
          disabled={downloading}
        />
        <span><strong>嵌入封面</strong><small>写入音频标签</small></span>
      </label>
      <label class="toggle-setting">
        <input
          type="checkbox"
          bind:checked={downloadLyrics}
          disabled={downloading}
        />
        <span><strong>下载歌词</strong><small>保存原始 LRC</small></span>
      </label>
      <div class="directory-setting">
        <label for="download-directory">保存位置</label>
        <span class="directory-control">
          <input
            id="download-directory"
            bind:value={baseDirectory}
            disabled={downloading || directoryLoading}
            placeholder={directoryLoading
              ? '正在读取默认目录…'
              : '留空时使用系统音乐目录'}
            aria-describedby="directory-message"
            title={baseDirectory}
            onchange={() => void scanExisting()}
          />
          <button
            type="button"
            disabled={downloading || directoryLoading}
            onclick={() => void chooseDirectory()}
            ><FolderOpenIcon size={15} /><span>选择</span></button
          >
        </span>
      </div>
    </div>
    <p
      id="directory-message"
      class:directory-error={directoryMessage}
      class="directory-message"
      aria-live="polite"
    >
      {directoryMessage || '每次下载会在此目录下按搜索关键词建立文件夹。'}
    </p>
  </aside>

  <SearchResults
    {searching}
    {downloading}
    {result}
    {errorMessage}
    {selected}
    {downloadableCount}
    {existingAudio}
    {rateLimitSeconds}
    {downloadProgress}
    {downloadItems}
    {retryingTarget}
    {retryPendingSongIds}
    onSearch={() => void search()}
    onToggleAll={toggleAll}
    onToggle={toggle}
    onRetry={(songId) => void retryFailed(songId)}
  />
  </div>

  <DownloadBar
    selectedCount={selected.size}
    {searching}
    {downloading}
    {result}
    {downloadProgress}
    {downloadResult}
    {downloadError}
    stats={queueStats}
    {cancellingScope}
    {retryingTarget}
    {rateLimitSeconds}
    onCancelCurrent={() => void cancelCurrent()}
    onCancelAll={() => void cancelAll()}
    onRetryAll={() => void retryFailed(null)}
    onOpenDirectory={() => void openDirectory()}
    onDownloadSelected={() => void downloadSelected()}
  />
</main>

<style>
.mark i,
.mark b {
    position: absolute;
    border-radius: 50%;
  }

label {
    display: grid;
    gap: 6px;
  }

button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

.quality-setting,
.directory-setting {
    min-width: 0;
    grid-column: 1 / -1;
  }

.directory-setting {
    display: grid;
    gap: 6px;
  }

.directory-message.directory-error {
    color: #b23e37;
  }

@media (max-height: 560px) and (min-width: 800px) {
  .topbar p {
        display: none;
      }

  .panel-heading {
        display: none;
      }

  .hint,
  .directory-message {
        display: none;
      }

  .download-settings {
        margin-top: 6px;
        padding-top: 6px;
      }
}

.directory-control {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 7px;
  }

@keyframes status-breathe {
    50% {
      opacity: 0.45;
      transform: scale(0.75);
    }
  }

.shell {
    display: grid;
    height: 100vh;
    --ink: var(--yinmi-text);
    --paper: var(--yinmi-background);
    --line: var(--yinmi-border);
    --signal: var(--yinmi-brand);
    --signal-dark: var(--yinmi-primary-hover);
    --action: var(--yinmi-primary-hover);
    --action-hover: var(--yinmi-primary-pressed);
    grid-template-rows: auto minmax(0, 1fr) auto;
    background: var(--paper);
    color: var(--ink);
    font-family: 'Segoe UI Variable Text', 'Microsoft YaHei UI', 'PingFang SC', sans-serif;
  }

.topbar {
    --topbar-inline-padding: clamp(20px, 2.5vw, 34px);
    --window-controls-edge-offset: calc(8px - var(--topbar-inline-padding));
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 68px;
    border-bottom: 1px solid var(--line);
    background: var(--yinmi-surface-raised);
    padding: 10px var(--topbar-inline-padding);
    color: var(--ink);
    box-shadow: var(--yinmi-shadow-subtle);
    backdrop-filter: blur(20px);
    user-select: none;
  }

.brand-copy {
    flex: 0 0 auto;
  }

.topbar h1 {
    margin: 0;
    color: var(--ink);
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: 1.32rem;
    font-weight: 750;
    letter-spacing: 0.1em;
  }

.topbar p {
    margin: 1px 0 0;
    color: var(--yinmi-text-secondary);
    font-size: 0.7rem;
  }

.mark {
    position: relative;
    width: 43px;
    height: 43px;
    border: 0;
    border-radius: 11px;
    background: var(--signal);
    box-shadow: var(--yinmi-shadow-raised);
  }

.mark i {
    border: 3px solid #aeb7c8;
  }

.mark i:first-child {
    inset: 8px;
    border-color: #f7fbff;
  }

.mark i:nth-child(2) {
    inset: 15px;
    border-color: #a8deff;
  }

.mark b {
    right: 3px;
    top: 3px;
    width: 10px;
    height: 10px;
    border: 2px solid #fff;
    background: #64d2af;
    box-shadow: none;
  }

.signal-rail {
    display: flex;
    align-items: center;
    gap: 7px;
    width: max-content;
    height: 32px;
    margin-left: 14px;
    border: 1px solid var(--yinmi-border);
    border-radius: 999px;
    background: var(--yinmi-surface-muted);
    padding: 0 11px;
    color: var(--yinmi-text-secondary);
    font: 650 0.68rem/1 'Microsoft YaHei UI', sans-serif;
    letter-spacing: 0.03em;
  }

.signal-rail b {
    border-radius: 50%;
    width: 6px;
    height: 6px;
    background: var(--yinmi-success);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--yinmi-success) 12%, transparent);
  }

.signal-rail.active {
    color: var(--signal-dark);
  }

.signal-rail.active b {
    animation: status-breathe 1.25s ease-in-out infinite;
    background: var(--signal);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--yinmi-primary) 12%, transparent);
  }

.version {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    height: 32px;
    border-left: 1px solid var(--line);
    padding-left: 12px;
    color: var(--yinmi-text-muted);
    font: 600 0.66rem/1 'Cascadia Code', Consolas, monospace;
  }

.workspace {
    display: grid;
    min-height: 0;
    overflow: hidden;
    grid-template-columns: minmax(300px, 326px) minmax(0, 1fr);
    gap: 14px;
    background: var(--paper);
    padding: 14px;
  }

.search-panel {
    min-height: 0;
    overflow: auto;
    scrollbar-width: thin;
    border: 1px solid var(--line);
    border-radius: var(--yinmi-radius-lg);
    background: var(--yinmi-surface-muted);
    padding: 23px 21px 28px;
    box-shadow: var(--yinmi-shadow-subtle);
    scrollbar-color: #bdcad5 transparent;
  }

.panel-heading {
    margin-bottom: 25px;
  }

.panel-heading span,
.section-label span {
    color: var(--signal-dark);
    font: 700 0.68rem/1 'Microsoft YaHei UI', sans-serif;
    letter-spacing: 0.08em;
  }

.panel-heading h2 {
    margin: 8px 0 7px;
    color: var(--ink);
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: 1.5rem;
    font-weight: 750;
    letter-spacing: -0.035em;
  }

.panel-heading p {
    margin: 0;
    line-height: 1.65;
    color: var(--yinmi-text-secondary);
    font-size: 0.73rem;
  }

.section-label {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--yinmi-border);
    padding-bottom: 8px;
  }

.section-label b {
    color: var(--yinmi-text-secondary);
    font-size: 0.68rem;
    font-weight: 600;
  }

.section-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

.reset-directory {
    border: 0;
    border-left: 1px solid var(--yinmi-border-strong);
    border-radius: 0;
    background: transparent;
    padding: 0 0 0 8px;
    color: var(--signal-dark);
    font-size: 0.68rem;
    font-weight: 650;
    line-height: 1;
  }

.reset-directory:hover:not(:disabled) {
    color: var(--action-hover);
  }

.section-label.download-label {
    margin-top: 25px;
    border-top: 0;
    padding-top: 0;
  }

form {
    display: grid;
    align-items: end;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 11px 9px;
  }

form .keyword {
    grid-column: 1 / -1;
  }

form label:nth-of-type(2),
form label:nth-of-type(3) {
    grid-column: auto;
  }

form label > span,
.quality-setting > span,
.directory-setting > label {
    letter-spacing: 0.02em;
    color: var(--yinmi-text-secondary);
    font-size: 0.69rem;
    font-weight: 650;
  }

input:not([type='checkbox']),
select {
    width: 100%;
    outline: none;
    font: inherit;
    height: 40px;
    border: 1px solid var(--yinmi-border-strong);
    border-radius: var(--yinmi-radius-sm);
    background: var(--yinmi-surface);
    padding: 0 11px;
    color: var(--ink);
  }

select {
    appearance: none;
    background-image: url('./icons/chevron-down.svg');
    background-repeat: no-repeat;
    background-position: right 12px center;
    background-size: 14px 14px;
    padding-right: 36px;
  }

.keyword input {
    height: 46px;
    border-color: var(--yinmi-border-strong);
    background: var(--yinmi-surface);
    font-size: 0.88rem;
    box-shadow: var(--yinmi-shadow-subtle);
  }

input::placeholder {
    color: var(--yinmi-text-muted);
  }

button {
    cursor: pointer;
    border: 0;
    font: inherit;
    font-weight: 700;
    border-radius: var(--yinmi-radius-sm);
  }

.primary {
    padding: 0 14px;
    height: 40px;
    border: 1px solid transparent;
    border-radius: var(--yinmi-radius-pill);
    background: var(--action);
    color: #fff;
    box-shadow: var(--yinmi-shadow-subtle);
  }

.primary:hover:not(:disabled) {
    background: var(--action-hover);
    transform: translateY(-1px);
  }

.hint {
    margin: 8px 0 0;
    line-height: 1.5;
    color: var(--yinmi-text-muted);
    font-size: 0.65rem;
  }

.download-settings {
    display: grid;
    grid-template-columns: 1fr 1fr;
    align-items: end;
    gap: 9px;
  }

.toggle-setting {
    display: flex;
    align-items: center;
    border: 1px solid var(--yinmi-border);
    cursor: pointer;
    gap: 9px;
    padding: 0 10px;
    height: 42px;
    border-radius: var(--yinmi-radius-sm);
    background: var(--yinmi-surface);
  }

.toggle-setting:hover {
    border-color: var(--yinmi-primary-soft-hover);
  }

.toggle-setting input {
    flex: 0 0 auto;
    width: 15px;
    height: 15px;
    accent-color: var(--yinmi-primary);
  }

.toggle-setting span {
    display: grid;
    letter-spacing: 0;
    text-transform: none;
    color: var(--yinmi-text);
  }

.toggle-setting strong {
    font-size: 0.74rem;
  }

.toggle-setting small {
    color: var(--yinmi-text-muted);
    font-size: 0.64rem;
  }

.directory-control button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--yinmi-border-strong);
    padding: 0 12px;
    white-space: nowrap;
    border-radius: var(--yinmi-radius-sm);
    background: var(--yinmi-surface);
    color: var(--signal-dark);
    font-size: 0.72rem;
  }

.directory-control button:hover:not(:disabled) {
    border-color: var(--yinmi-primary-soft-hover);
    background: var(--yinmi-primary-soft);
  }

.directory-message {
    margin: 7px 0 0;
    line-height: 1.45;
    color: var(--yinmi-text-muted);
    font-size: 0.66rem;
  }

@media (max-width: 1120px) and (min-width: 800px) {
  .workspace {
        grid-template-columns: 280px minmax(0, 1fr);
      }

  .search-panel {
        padding-inline: 17px;
      }
}

@media (max-height: 850px) and (min-width: 800px) {
  .topbar {
        min-height: 66px;
        padding-block: 8px;
      }

  .search-panel {
        padding-block: 20px 16px;
      }

  .panel-heading {
        margin-bottom: 22px;
      }

  .section-label.download-label {
        margin-top: 22px;
      }

  form {
        gap: 10px 9px;
      }
}

@media (max-height: 620px) and (min-width: 800px) {
  .topbar {
        min-height: 58px;
      }

  .search-panel {
        border-radius: var(--yinmi-radius-md);
        padding-block: 13px;
      }

  .panel-heading {
        margin-bottom: 14px;
      }

  .panel-heading h2 {
        margin-block: 6px 4px;
      }

  .panel-heading p {
        line-height: 1.4;
      }

  .section-label {
        margin-bottom: 8px;
        padding-bottom: 6px;
      }

  .section-label.download-label {
        margin-top: 14px;
      }

  form {
        gap: 8px 9px;
      }

  label,
  .directory-setting {
        gap: 4px;
      }

  input:not([type='checkbox']),
  select,
  .primary {
        height: 38px;
      }

  .keyword input {
        height: 42px;
      }

  .hint {
        margin-top: 5px;
        line-height: 1.4;
      }

  .download-settings {
        gap: 7px 9px;
      }

  .toggle-setting {
        height: 38px;
      }

  .directory-message {
        margin-top: 4px;
        line-height: 1.35;
      }
}

@media (prefers-reduced-motion: reduce) {
  .primary {
        transition: none;
      }
}
</style>
