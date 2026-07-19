<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getVersion } from '@tauri-apps/api/app';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import {
    SEARCH_MODES,
    SOURCES,
    downloadProgressPercent,
    errorText,
    formatBytes,
    formatDuration,
    sourceLabel,
    stopReasonLabel,
    type DownloadBatchResult,
    type DownloadItemResult,
    type DownloadProgress,
    type DownloadState,
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
  import RuntimeTools from './RuntimeTools.svelte';
  import WindowControls from './WindowControls.svelte';
  import { titlebar } from './windowChrome';

  type RetryTarget = string | 'all' | null;

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

  function resetResultState() {
    requestSerial += 1;
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

  function countItems(items: DownloadItemResult[], state: DownloadState) {
    return items.filter((item) => item.state === state).length;
  }

  function mergeDownloadResult(value: DownloadBatchResult) {
    const merged = new Map([
      ...downloadItems,
      ...value.items.map((item) => [item.songId, item] as const),
    ]);
    downloadItems = merged;

    const items = [...merged.values()];
    downloadResult = {
      ...value,
      total: items.length,
      succeeded: countItems(items, 'success'),
      skipped: countItems(items, 'skipped'),
      failed: countItems(items, 'failed'),
      cancelled: countItems(items, 'cancelled'),
      items,
    };
  }

  function queueStats() {
    if (downloading && downloadProgress) {
      const current =
        downloadProgress.state !== 'finished' &&
        downloadProgress.completed < downloadProgress.total
          ? 1
          : 0;
      if (retryingTarget !== null) {
        const settledItems = [...downloadItems.values()].filter(
          (item) => !retryPendingSongIds.has(item.songId),
        );
        return {
          waiting: Math.max(0, retryPendingSongIds.size - current),
          current,
          succeeded: countItems(settledItems, 'success'),
          skipped: countItems(settledItems, 'skipped'),
          failed: countItems(settledItems, 'failed'),
          cancelled: countItems(settledItems, 'cancelled'),
          total: downloadItems.size,
        };
      }
      return {
        waiting: Math.max(
          0,
          downloadProgress.total - downloadProgress.completed - current,
        ),
        current,
        succeeded: downloadProgress.succeeded,
        skipped: downloadProgress.skipped,
        failed: downloadProgress.failed,
        cancelled: downloadProgress.cancelled,
        total: downloadProgress.total,
      };
    }
    return {
      waiting: 0,
      current: 0,
      succeeded: downloadResult?.succeeded ?? 0,
      skipped: downloadResult?.skipped ?? 0,
      failed: downloadResult?.failed ?? 0,
      cancelled: downloadResult?.cancelled ?? 0,
      total: downloadResult?.total ?? selected.size,
    };
  }

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
    errorMessage = '';
    result = null;
    selected = new Set();
    downloadResult = null;
    downloadProgress = null;
    downloadError = '';
    downloadItems = new Map();
    cancellingScope = null;
    retryingTarget = null;
    retryPendingSongIds = new Set();
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
      <i></i>
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
      <label class="directory-setting">
        <span>保存位置</span>
        <span class="directory-control">
          <input
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
            onclick={() => void chooseDirectory()}>选择</button
          >
        </span>
      </label>
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

  <section
    class="results"
    aria-busy={searching || downloading}
    aria-live="polite"
  >
    <div class="heading">
      <div>
        <span>音乐候选</span>
        <h2>
          {result?.keyword ??
            (searching ? '正在寻找合适的版本' : '从一首想听的歌开始')}
        </h2>
      </div>
      <div class="count">
        <span class="result-total">
          <strong>{result?.returnedCount ?? 0}</strong><small>首歌曲</small>
        </span>
        <span class="selected-count">已选 <b>{selected.size}</b></span>
      </div>
    </div>

    {#if errorMessage}
      <div class="message error" role="alert">
        <strong>搜索未完成</strong><span>{errorMessage}</span><button
          type="button"
          onclick={() => void search()}>重试</button
        >
      </div>
    {:else if searching}
      <div class="message">
        <i class="pulse"></i><strong
          >{rateLimitSeconds
            ? `请求额度冷却中，约 ${rateLimitSeconds} 秒后继续`
            : '正在获取搜索结果'}</strong
        ><span>首次搜索或大量结果可能需要一些时间。</span>
      </div>
    {:else if result && result.songs.length === 0}
      <div class="message">
        <strong>没有找到歌曲</strong><span
          >换一个关键词、音源或匹配方式再试。</span
        >
      </div>
    {:else if result}
      <div class="toolbar">
        <button type="button" onclick={toggleAll} disabled={downloading}
          >{selected.size === downloadableSongs().length && selected.size > 0
            ? '取消全选'
            : '全选可下载歌曲'}</button
        ><span
          >来源：{result.sourceName}{result.skippedRecords
            ? ` · 跳过 ${result.skippedRecords} 条无效记录`
            : ''}</span
        >
      </div>
      {#if result.incomplete}
        <div class="partial-warning" role="status" aria-live="polite">
          <strong>已返回部分结果</strong>
          <span
            >{stopReasonLabel(
              result.stopReason,
            )}，当前歌曲仍可正常选择和下载。</span
          >
        </div>
      {/if}
      <div class="table-wrap">
        <table>
          <thead
            ><tr
              ><th></th><th>#</th><th>歌曲 / 歌手</th><th>专辑 / 音源</th><th
                >时长</th
              ><th>下载状态</th></tr
            ></thead
          >
          <tbody
            >{#each result.songs as song, index (keyOf(song))}
              {@const item = downloadItems.get(song.id)}
              {@const isCurrent =
                downloading &&
                downloadProgress?.state !== 'finished' &&
                downloadProgress?.currentSongId === song.id}
              {@const isRetryPending =
                downloading &&
                retryingTarget !== null &&
                retryPendingSongIds.has(song.id)}
              <tr class:selected={selected.has(keyOf(song))}>
                <td
                  ><input
                    class="check"
                    type="checkbox"
                    aria-label={`选择 ${song.name}`}
                    checked={selected.has(keyOf(song))}
                    disabled={!song.urlId || downloading}
                    onchange={() => toggle(song)}
                  /></td
                >
                <td class="index">{index + 1}</td>
                <td class="track-info">
                  <strong class="name">{song.name}</strong>
                  <small>{song.artistDisplay}</small>
                </td>
                <td class="album-info">
                  <strong>{song.album ?? '—'}</strong>
                  <small class="source-name">{sourceLabel(song.source)}</small>
                </td>
                <td class="duration">{formatDuration(song.durationMs)}</td>
                <td class="status-cell">
                  <div class="status-line">
                    {#if isCurrent}<span class="status current"
                        >{retryingTarget ? '重试中' : '下载中'}</span
                      >
                    {:else if isRetryPending}<span class="status waiting"
                        >等待</span
                      >
                    {:else if item?.state === 'success'}<span
                        class="status success">已下载</span
                      >
                    {:else if item?.state === 'skipped'}<span
                        class="status skipped">已存在</span
                      >
                    {:else if item?.state === 'failed'}<span
                        class="status failed"
                        title={item.message ?? ''}>失败</span
                      >
                    {:else if item?.state === 'cancelled'}<span
                        class="status cancelled">已取消</span
                      >
                    {:else if downloading && selected.has(keyOf(song))}<span
                        class="status waiting">等待</span
                      >
                    {:else if existingAudio.has(song.id)}<span
                        class="status skipped"
                        title={`本地已有 ${existingAudio
                          .get(song.id)
                          ?.join('、')
                          .toUpperCase()}`}
                        >本地已有 {existingAudio
                          .get(song.id)
                          ?.join('/')
                          .toUpperCase()}</span
                      >
                    {:else}<span class="status idle">—</span>{/if}
                    {#if !isRetryPending && (item?.state === 'failed' || item?.state === 'cancelled')}
                      <button
                        class="retry-item"
                        type="button"
                        aria-label={`重试下载 ${song.name}`}
                        disabled={downloading}
                        onclick={() => void retryFailed(song.id)}>重试</button
                      >
                    {/if}
                  </div>
                  {#if !isRetryPending && item?.message && (item.state === 'failed' || item.state === 'cancelled')}
                    <small class="item-message" title={item.message}
                      >{item.message}</small
                    >
                  {/if}
                  {#if item?.warnings?.length}
                    <details class="warnings">
                      <summary>提示 {item.warnings.length}</summary>
                      <div>
                        {#each item.warnings as warning, index (index)}<p>
                            {warning}
                          </p>{/each}
                      </div>
                    </details>
                  {/if}
                </td>
              </tr>{/each}</tbody
          >
        </table>
      </div>
    {:else}
      <div class="empty-signal">
        <div class="empty-record" aria-hidden="true">
          <i></i><i></i><i></i><b></b>
        </div>
        <strong>先搜一首歌</strong>
        <span>歌名、歌手、专辑，任何一个线索都可以。</span>
      </div>
    {/if}
  </section>
  </div>

  <footer aria-label="下载队列">
    <div class="selected-summary">
      <div class="selected-summary-line">
        <span>已选择</span><strong>{selected.size}</strong><span>首</span>
      </div>
    </div>
    <div class="download-copy">
      {#if downloading && downloadProgress}
        <strong
          >{cancellingScope
            ? '正在取消并清理临时文件…'
            : `${retryingTarget ? '正在重试' : '正在下载'} · ${downloadProgress.currentName}`}</strong
        >
        <span>
          {formatBytes(
            downloadProgress.currentDownloadedBytes,
          )}{downloadProgress.currentTotalBytes
            ? ` / ${formatBytes(downloadProgress.currentTotalBytes)}`
            : ''}
          {downloadProgress.bytesPerSecond
            ? ` · ${formatBytes(downloadProgress.bytesPerSecond)}/s`
            : ''}
          {downloadProgressPercent(downloadProgress) !== null
            ? ` · ${downloadProgressPercent(downloadProgress)}%`
            : ''}
          {rateLimitSeconds ? ` · 限流冷却约 ${rateLimitSeconds} 秒` : ''}
        </span>
        {#if downloadProgress.currentTotalBytes}
          <progress
            value={downloadProgress.currentDownloadedBytes}
            max={downloadProgress.currentTotalBytes}
          ></progress>
        {:else}
          <progress></progress>
        {/if}
      {:else if downloadResult}
        <strong>下载队列已处理完成</strong><span
          title={downloadResult.directory}>{downloadResult.directory}</span
        >
      {/if}
      <div class="queue-stats" aria-label="队列统计" aria-live="polite">
        <span class="waiting">等待 <b>{queueStats().waiting}</b></span>
        <span class="current">当前 <b>{queueStats().current}</b></span>
        <span class="succeeded">成功 <b>{queueStats().succeeded}</b></span>
        <span class="skipped">跳过 <b>{queueStats().skipped}</b></span>
        <span class="failed">失败 <b>{queueStats().failed}</b></span>
        <span class="cancelled">取消 <b>{queueStats().cancelled}</b></span>
        <span class="total">总计 <b>{queueStats().total}</b></span>
      </div>
      {#if downloadError}<span class="footer-error" role="alert"
          >{downloadError}</span
        >{/if}
    </div>
    <div class="footer-actions">
      {#if downloading}
        <button
          class="cancel"
          type="button"
          disabled={cancellingScope !== null}
          onclick={() => void cancelCurrent()}
          >{cancellingScope === 'current' ? '取消中…' : '取消当前'}</button
        >
        <button
          class="cancel all"
          type="button"
          disabled={cancellingScope !== null}
          onclick={() => void cancelAll()}
          >{cancellingScope === 'all' ? '停止中…' : '取消全部'}</button
        >
      {:else}
        {#if downloadResult && downloadResult.failed + downloadResult.cancelled > 0}<button
            class="retry-all"
            type="button"
            onclick={() => void retryFailed(null)}>重试未完成项</button
          >{/if}
        {#if downloadResult}<button
            class="open"
            type="button"
            onclick={() => void openDirectory()}>打开目录</button
          >{/if}
        <button
          class="download"
          type="button"
          disabled={searching || !result || selected.size === 0}
          onclick={() => void downloadSelected()}
          >{`下载所选${selected.size ? ` ${selected.size} 首` : ''}`}</button
        >
      {/if}
    </div>
  </footer>
</main>

<style>
  :global(*) {
    box-sizing: border-box;
  }
  :global(body) {
    overflow: hidden;
  }
  .shell {
    --ink: #171b1a;
    --ink-soft: #252b29;
    --paper: #f1f3ef;
    --panel: #e7eae4;
    --line: #c7cec6;
    --muted: #68746e;
    --signal: #63dc91;
    --signal-dark: #155d36;
    --alert: #d75a4d;
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr) auto;
    height: 100vh;
    background: var(--paper);
    color: var(--ink);
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 70px;
    border-bottom: 1px solid #343a37;
    background: var(--ink);
    padding: 10px clamp(18px, 2.6vw, 36px);
    color: #f7faf5;
  }
  .topbar h1 {
    margin: 0;
    font-family: 'Bahnschrift SemiCondensed', 'Arial Narrow', sans-serif;
    font-size: 1.55rem;
    font-weight: 650;
    letter-spacing: 0.16em;
  }
  .topbar p {
    margin: 2px 0 0;
    color: #9ea9a3;
    font-size: 0.74rem;
  }
  .version {
    flex: 0 0 auto;
    color: #85918b;
    font:
      600 0.66rem ui-monospace,
      monospace;
  }
  .mark {
    position: relative;
    width: 44px;
    height: 44px;
    border: 1px solid #44504a;
    border-radius: 5px;
    background: #232a27;
  }
  .mark i,
  .mark b {
    position: absolute;
    border-radius: 50%;
  }
  .mark i {
    border: 1px solid #a9b4ae;
  }
  .mark i:first-child {
    inset: 9px;
  }
  .mark i:nth-child(2) {
    inset: 17px;
  }
  .mark b {
    right: 8px;
    top: 8px;
    width: 9px;
    height: 9px;
    background: var(--signal);
    box-shadow: 0 0 0 4px #63dc911c;
  }
  .signal-rail {
    display: grid;
    grid-template-columns: auto minmax(80px, 1fr) 8px;
    align-items: center;
    gap: 12px;
    min-height: 22px;
    background: #222725;
    padding: 0 clamp(18px, 2.6vw, 36px);
    color: #829087;
    font: 650 0.58rem/1 'Cascadia Mono', ui-monospace, monospace;
    letter-spacing: 0.14em;
  }
  .signal-rail i {
    height: 1px;
    background: #424b46;
  }
  .signal-rail b {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #5a665f;
  }
  .signal-rail.active {
    color: var(--signal);
  }
  .signal-rail.active i {
    background: linear-gradient(90deg, var(--signal) 0 34%, #424b46 34% 100%);
    background-size: 180% 100%;
    animation: signal-scan 1.4s linear infinite;
  }
  .signal-rail.active b {
    background: var(--signal);
    box-shadow: 0 0 10px #63dc91a6;
  }
  .signal-rail.downloading i {
    background: var(--signal);
    animation: none;
  }
  .workspace {
    display: grid;
    grid-template-columns: minmax(300px, 342px) minmax(0, 1fr);
    min-height: 0;
    overflow: hidden;
  }
  .search-panel {
    min-height: 0;
    overflow: auto;
    border-right: 1px solid var(--line);
    background: var(--panel);
    padding: 20px 22px 24px;
  }
  .panel-heading {
    margin-bottom: 22px;
  }
  .panel-heading span,
  .section-label span,
  .heading > div > span {
    color: var(--muted);
    font: 650 0.62rem/1 'Cascadia Mono', ui-monospace, monospace;
    letter-spacing: 0.14em;
  }
  .panel-heading h2 {
    margin: 5px 0 6px;
    font-family: 'Bahnschrift SemiCondensed', 'Arial Narrow', sans-serif;
    font-size: 1.45rem;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .panel-heading p {
    margin: 0;
    color: var(--muted);
    font-size: 0.72rem;
    line-height: 1.55;
  }
  .section-label {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 10px;
    border-bottom: 1px solid var(--line);
    padding-bottom: 7px;
  }
  .section-label b {
    color: #354039;
    font-size: 0.68rem;
    font-weight: 650;
  }
  .section-label.download-label {
    margin-top: 24px;
  }
  form {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 82px;
    align-items: end;
    gap: 11px 9px;
  }
  form .keyword,
  form label:nth-of-type(2),
  form label:nth-of-type(3) {
    grid-column: 1 / -1;
  }
  label {
    display: grid;
    gap: 6px;
  }
  form label > span,
  .quality-setting > span,
  .directory-setting > span:first-child {
    color: #56635c;
    font-size: 0.67rem;
    font-weight: 650;
    letter-spacing: 0.05em;
  }
  input:not([type='checkbox']),
  select {
    width: 100%;
    height: 38px;
    border: 1px solid #aeb7ae;
    border-radius: 3px;
    outline: none;
    background: #fbfcfa;
    padding: 0 11px;
    color: var(--ink);
    font: inherit;
  }
  input:focus,
  select:focus,
  button:focus-visible {
    border-color: #258752;
    box-shadow: 0 0 0 3px #63dc9129;
  }
  button {
    cursor: pointer;
    border: 0;
    border-radius: 3px;
    font: inherit;
    font-weight: 700;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  .primary {
    height: 38px;
    background: var(--signal);
    padding: 0 14px;
    color: #122319;
  }
  .primary:hover:not(:disabled),
  .download:hover:not(:disabled) {
    background: #7ae6a2;
  }
  .hint {
    margin: 7px 0 0;
    color: var(--muted);
    font-size: 0.68rem;
    line-height: 1.5;
  }
  .download-settings {
    display: grid;
    grid-template-columns: 1fr 1fr;
    align-items: end;
    gap: 10px 8px;
  }
  .quality-setting,
  .directory-setting {
    min-width: 0;
  }
  .quality-setting,
  .directory-setting {
    grid-column: 1 / -1;
  }
  .toggle-setting {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 38px;
    border: 1px solid #b6beb6;
    border-radius: 3px;
    background: #f7f9f5;
    padding: 0 10px;
    cursor: pointer;
  }
  .toggle-setting input,
  .check {
    flex: 0 0 auto;
    width: 15px;
    height: 15px;
    accent-color: #258752;
  }
  .toggle-setting span {
    display: grid;
    color: #38443d;
    letter-spacing: 0;
    text-transform: none;
  }
  .toggle-setting strong {
    font-size: 0.72rem;
  }
  .toggle-setting small {
    color: var(--muted);
    font-size: 0.61rem;
  }
  .directory-control {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 6px;
  }
  .directory-control button {
    border: 1px solid #aeb7ae;
    background: #f9faf8;
    padding: 0 12px;
    color: #344a3c;
    font-size: 0.7rem;
    white-space: nowrap;
  }
  .directory-message {
    margin: 5px 0 0;
    color: var(--muted);
    font-size: 0.66rem;
    line-height: 1.45;
  }
  .directory-message.directory-error {
    color: #b23e37;
  }
  .results {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: var(--paper);
    padding: 20px clamp(18px, 2.4vw, 34px) 16px;
  }
  .heading {
    display: flex;
    flex: 0 0 auto;
    align-items: flex-end;
    justify-content: space-between;
    margin-bottom: 14px;
  }
  .heading h2 {
    margin: 5px 0 0;
    font-family: 'Bahnschrift SemiCondensed', 'Arial Narrow', sans-serif;
    font-size: 1.55rem;
    font-weight: 600;
    letter-spacing: 0.01em;
  }
  .count {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .count strong {
    color: #1b7c47;
    font:
      700 1.9rem/1 ui-monospace,
      monospace;
  }
  .count small {
    color: var(--muted);
  }
  .message {
    display: grid;
    place-items: center;
    align-content: center;
    gap: 8px;
    height: calc(100% - 70px);
    min-height: 220px;
    border: 1px dashed #b3bcb3;
    color: var(--muted);
  }
  .message strong {
    color: var(--ink);
  }
  .message.error {
    border-color: #d9a39d;
    background: #fff6f4;
  }
  .message button {
    background: var(--ink);
    padding: 8px 18px;
    color: #fff;
  }
  .pulse {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: var(--signal);
    animation: pulse 1.1s ease-in-out infinite;
  }
  .toolbar {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    min-height: 39px;
    border: 1px solid var(--line);
    border-bottom: 0;
    background: #f8faf6;
    padding: 0 12px;
    color: var(--muted);
    font-size: 0.74rem;
  }
  .toolbar button {
    background: transparent;
    color: #166e3f;
  }
  .partial-warning {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    border: 1px solid #dfc47e;
    border-bottom: 0;
    background: #fff8e5;
    padding: 5px 12px;
    color: #7d6428;
    font-size: 0.72rem;
  }
  .partial-warning strong {
    color: #6a4e12;
    white-space: nowrap;
  }
  .table-wrap {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--line);
    background: #fdfefc;
  }
  table {
    width: 100%;
    min-width: 960px;
    border-collapse: collapse;
    font-size: 0.8rem;
  }
  th {
    position: sticky;
    top: 0;
    background: #dde2dc;
    color: #536159;
    padding: 10px 12px;
    text-align: left;
    font-size: 0.68rem;
  }
  td {
    max-width: 250px;
    overflow: hidden;
    border-top: 1px solid #e1e5df;
    padding: 10px 12px;
    color: #58635e;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  tr.selected td {
    background: #e8f5ec;
  }
  .index {
    color: #93a0ac;
  }
  .name {
    color: var(--ink);
    font-weight: 700;
  }
  .duration {
    font-family: ui-monospace, monospace;
  }
  .source-name {
    color: #397250;
    font-size: 0.72rem;
  }
  .status {
    font-size: 0.7rem;
    font-weight: 700;
  }
  .status-line {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .status-cell {
    min-width: 170px;
    overflow: visible;
  }
  .status.current {
    color: #15834a;
  }
  .status.waiting {
    color: #687d90;
  }
  .status.success {
    color: #13785c;
  }
  .status.skipped {
    color: #8a641d;
  }
  .status.failed {
    color: #b63f38;
  }
  .status.cancelled {
    color: #7b8792;
  }
  .status.idle {
    color: #a1abb4;
  }
  .retry-item {
    background: #e1eee5;
    padding: 3px 7px;
    color: #17683d;
    font-size: 0.64rem;
  }
  .item-message {
    display: block;
    max-width: 180px;
    overflow: hidden;
    margin-top: 3px;
    color: #a8524c;
    font-size: 0.61rem;
    text-overflow: ellipsis;
  }
  .warnings {
    position: relative;
    margin-top: 3px;
    color: #8a641d;
    font-size: 0.62rem;
  }
  .warnings summary {
    width: fit-content;
    cursor: pointer;
  }
  .warnings div {
    position: absolute;
    z-index: 5;
    right: 0;
    width: min(320px, 60vw);
    border: 1px solid #dbc47f;
    border-radius: 4px;
    background: #fffbed;
    padding: 7px 9px;
    box-shadow: 0 8px 24px #20364a26;
    white-space: normal;
  }
  .warnings p {
    margin: 0 0 4px;
  }
  .warnings p:last-child {
    margin-bottom: 0;
  }
  .empty-signal {
    display: grid;
    place-items: center;
    align-content: center;
    gap: 8px;
    flex: 1 1 auto;
    min-height: 220px;
    border: 1px solid var(--line);
    background-color: #f7f9f5;
    background-image:
      linear-gradient(#dfe4de 1px, transparent 1px),
      linear-gradient(90deg, #dfe4de 1px, transparent 1px);
    background-size: 32px 32px;
    color: var(--muted);
  }
  .empty-signal strong {
    color: #354039;
    font-family: 'Bahnschrift SemiCondensed', 'Arial Narrow', sans-serif;
    font-size: 1rem;
    letter-spacing: 0.04em;
  }
  .empty-signal > span {
    font-size: 0.72rem;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 92px;
    max-height: 116px;
    overflow-y: auto;
    border-top: 1px solid #39413d;
    background: var(--ink);
    padding: 10px clamp(18px, 2.6vw, 36px);
    color: #dbe6ef;
  }
  .selected-summary {
    display: flex;
    flex: 0 0 auto;
    align-items: baseline;
    gap: 7px;
  }
  footer span {
    color: #9fb0bf;
    font-size: 0.72rem;
  }
  .selected-summary strong {
    color: #fff;
    font:
      700 1.5rem ui-monospace,
      monospace;
  }
  .download-copy {
    display: grid;
    flex: 1 1 auto;
    min-width: 0;
    gap: 2px;
  }
  .download-copy strong {
    overflow: hidden;
    color: #ecf4fa;
    font-size: 0.76rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .download-copy span {
    overflow: hidden;
    max-width: 560px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .download-copy progress {
    width: min(520px, 42vw);
    height: 5px;
    margin-top: 4px;
    border: 0;
    accent-color: var(--signal);
  }
  .queue-stats {
    display: flex;
    flex-wrap: wrap;
    gap: 3px 10px;
    margin-top: 1px;
  }
  .queue-stats span {
    overflow: visible;
    color: #8fa5b7;
    font-size: 0.65rem;
  }
  .queue-stats b {
    color: #eef6fb;
    font-family: ui-monospace, monospace;
  }
  .footer-error {
    display: block;
    max-width: 620px;
    overflow: hidden;
    color: #ffb2ab !important;
    font-size: 0.68rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .footer-actions {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 7px;
  }
  .open,
  .download,
  .cancel,
  .retry-all {
    padding: 9px 14px;
    white-space: nowrap;
  }
  .open {
    background: #28445e;
    color: #e6f0f7;
  }
  .download {
    background: var(--signal);
    color: #102b20;
  }
  .retry-all {
    background: #285b78;
    color: #e8f5fc;
  }
  .cancel {
    background: #36526c;
    color: #f0f5f8;
  }
  .cancel.all {
    background: #914940;
    color: #fff;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
      transform: scale(0.8);
    }
  }
  @keyframes signal-scan {
    to {
      background-position: -180% 0;
    }
  }
  @media (max-width: 1050px) and (min-width: 761px) {
    .workspace {
      grid-template-columns: 280px minmax(0, 1fr);
    }
    .search-panel {
      padding-inline: 16px;
    }
    .panel-heading p {
      display: none;
    }
    footer {
      gap: 10px;
    }
    .selected-summary {
      display: none;
    }
  }
  @media (max-width: 760px) {
    :global(body) {
      overflow: auto;
    }
    .shell {
      height: auto;
      min-height: 100vh;
      grid-template-rows: auto auto auto auto;
    }
    .workspace {
      grid-template-columns: 1fr;
      overflow: visible;
    }
    .search-panel {
      overflow: visible;
      border-right: 0;
      border-bottom: 1px solid var(--line);
      padding: 18px;
    }
    .results {
      min-height: 560px;
      padding-inline: 12px;
    }
    footer {
      flex-wrap: wrap;
      align-items: center;
      gap: 8px;
      min-height: 116px;
      max-height: none;
      padding: 8px 12px;
    }
    .selected-summary {
      display: none;
    }
    .download-copy > span {
      max-width: 360px;
    }
    .footer-actions {
      margin-left: auto;
    }
  }
  @media (max-width: 520px) {
    .version,
    .topbar p,
    .hint,
    .directory-message,
    .download-copy > span {
      display: none;
    }
    .download-settings {
      grid-template-columns: 1fr;
    }
    .toggle-setting,
    .directory-setting {
      grid-column: 1 / -1;
    }
    .toolbar,
    .partial-warning {
      align-items: flex-start;
      flex-direction: column;
      padding-block: 6px;
    }
    .footer-actions {
      width: 100%;
    }
    .footer-actions button {
      flex: 1 1 auto;
      padding-block: 8px;
    }
  }
  @media (max-height: 560px) and (min-width: 761px) {
    .shell {
      grid-template-rows: auto auto minmax(0, 1fr) auto;
    }
    .topbar {
      min-height: 54px;
      padding-block: 6px;
    }
    .topbar p {
      display: none;
    }
    .mark {
      width: 36px;
      height: 36px;
      border-radius: 10px;
    }
    .mark i:first-child {
      inset: 7px;
    }
    .mark i:nth-child(2) {
      inset: 13px;
    }
    .search-panel {
      padding-block: 12px;
    }
    .panel-heading {
      display: none;
    }
    .section-label.download-label {
      margin-top: 12px;
    }
    form,
    label {
      gap: 3px;
    }
    input:not([type='checkbox']),
    select,
    .primary,
    .toggle-setting {
      height: 32px;
    }
    .hint,
    .directory-message {
      display: none;
    }
    .download-settings {
      margin-top: 6px;
      padding-top: 6px;
    }
    .results {
      padding-top: 7px;
    }
    .heading {
      margin-bottom: 6px;
    }
    .heading h2 {
      font-size: 1.05rem;
    }
    .count strong {
      font-size: 1.4rem;
    }
    .toolbar {
      min-height: 31px;
    }
    .message {
      height: 100%;
      min-height: 0;
    }
    th,
    td {
      padding-block: 7px;
    }
    footer {
      min-height: 82px;
      max-height: 96px;
      padding-block: 6px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse,
    .signal-rail.active i {
      animation: none;
    }
  }

  /* Listening-room visual system */
  .shell {
    --ink: #101827;
    --ink-soft: #1b2638;
    --paper: #eef1f7;
    --panel: #ffffff;
    --line: #dfe4ed;
    --muted: #717b8d;
    --signal: #8b7cff;
    --signal-dark: #6757e8;
    --apricot: #ff9a76;
    --alert: #d95f58;
    grid-template-rows: auto minmax(0, 1fr) auto;
    background: var(--paper);
    color: var(--ink);
    font-family: 'Segoe UI Variable Text', 'Microsoft YaHei UI',
      'PingFang SC', sans-serif;
  }
  .topbar {
    gap: 13px;
    min-height: 72px;
    border-bottom: 0;
    background: var(--ink);
    padding: 11px clamp(20px, 2.8vw, 38px);
    color: #f8f9fc;
  }
  .brand-copy {
    flex: 0 0 auto;
  }
  .topbar h1 {
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: 1.25rem;
    font-weight: 720;
    letter-spacing: 0.11em;
  }
  .topbar p {
    margin-top: 1px;
    color: #8f9aae;
    font-size: 0.64rem;
  }
  .mark {
    width: 42px;
    height: 42px;
    border: 1px solid #ffffff24;
    border-radius: 50%;
    background: var(--ink-soft);
  }
  .mark i {
    border-color: #aeb7c8;
  }
  .mark i:first-child {
    inset: 9px;
  }
  .mark i:nth-child(2) {
    inset: 16px;
  }
  .mark b {
    right: 4px;
    top: 4px;
    width: 9px;
    height: 9px;
    background: var(--apricot);
    box-shadow: 0 0 0 4px #ff9a7618;
  }
  .signal-rail {
    display: flex;
    grid-template-columns: none;
    align-items: center;
    gap: 7px;
    width: max-content;
    min-height: 28px;
    margin-left: 12px;
    border: 1px solid #ffffff13;
    border-radius: 999px;
    background: #ffffff08;
    padding: 0 11px;
    color: #929db0;
    font: 600 0.62rem/1 'Microsoft YaHei UI', sans-serif;
    letter-spacing: 0.04em;
  }
  .signal-rail b {
    width: 6px;
    height: 6px;
    background: #6f7a8e;
  }
  .signal-rail i {
    display: none;
  }
  .signal-rail.active {
    color: #d4cffc;
  }
  .signal-rail.active b {
    background: var(--signal);
    box-shadow: 0 0 0 4px #8b7cff20;
    animation: status-breathe 1.25s ease-in-out infinite;
  }
  .version {
    border-left: 1px solid #ffffff13;
    padding-left: 12px;
    color: #707c91;
    font: 600 0.62rem/1 'Cascadia Code', Consolas, monospace;
  }
  .workspace {
    grid-template-columns: minmax(294px, 326px) minmax(0, 1fr);
    gap: 16px;
    min-height: 0;
    overflow: hidden;
    background: var(--paper);
    padding: 16px 18px;
  }
  .search-panel {
    border: 1px solid #e2e6ef;
    border-radius: 20px;
    background: var(--panel);
    padding: 23px 22px 25px;
    box-shadow: 0 10px 34px #25324b0a;
    scrollbar-color: #cbd1de transparent;
    scrollbar-width: thin;
  }
  .panel-heading {
    margin-bottom: 25px;
  }
  .panel-heading span,
  .section-label span,
  .heading > div > span {
    color: #8b7cff;
    font: 700 0.64rem/1 'Microsoft YaHei UI', sans-serif;
    letter-spacing: 0.1em;
  }
  .panel-heading h2 {
    margin: 8px 0 7px;
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: 1.45rem;
    font-weight: 720;
    letter-spacing: -0.035em;
  }
  .panel-heading p {
    color: #7b8596;
    font-size: 0.7rem;
    line-height: 1.65;
  }
  .section-label {
    margin-bottom: 13px;
    border-bottom: 0;
    padding-bottom: 0;
  }
  .section-label b {
    color: #9ba4b3;
    font-size: 0.64rem;
    font-weight: 500;
  }
  .section-label.download-label {
    margin-top: 27px;
    border-top: 1px solid var(--line);
    padding-top: 21px;
  }
  form {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px 10px;
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
  .directory-setting > span:first-child {
    color: #626d7e;
    font-size: 0.67rem;
    font-weight: 650;
    letter-spacing: 0.02em;
  }
  input:not([type='checkbox']),
  select {
    height: 42px;
    border: 1px solid #d8deea;
    border-radius: 10px;
    background: #f8f9fc;
    padding: 0 12px;
    color: #1b2433;
    font-family: inherit;
  }
  .keyword input {
    height: 50px;
    background: #f4f2ff;
    border-color: #d8d2ff;
    font-size: 0.86rem;
  }
  input::placeholder {
    color: #9da5b3;
  }
  input:focus,
  select:focus,
  button:focus-visible {
    border-color: var(--signal);
    outline: none;
    box-shadow: 0 0 0 3px #8b7cff24;
  }
  button {
    border-radius: 9px;
  }
  .primary {
    height: 42px;
    border: 1px solid transparent;
    background: var(--ink);
    color: white;
    transition:
      transform 160ms ease,
      background 160ms ease;
  }
  .primary:hover:not(:disabled) {
    background: #26334a;
    transform: translateY(-1px);
  }
  .hint {
    margin-top: 8px;
    color: #8c95a4;
    font-size: 0.63rem;
  }
  .download-settings {
    gap: 10px;
  }
  .toggle-setting {
    gap: 9px;
    height: 44px;
    border-color: #dfe4ed;
    border-radius: 11px;
    background: #f8f9fc;
    padding: 0 10px;
  }
  .toggle-setting input,
  .check {
    accent-color: var(--signal-dark);
  }
  .toggle-setting span {
    color: #3d4756;
  }
  .toggle-setting strong {
    font-size: 0.69rem;
  }
  .toggle-setting small {
    color: #929bab;
    font-size: 0.58rem;
  }
  .directory-control {
    gap: 7px;
  }
  .directory-control button {
    border-color: #d8deea;
    border-radius: 9px;
    background: #f4f5f9;
    color: #465166;
    font-size: 0.66rem;
  }
  .directory-message {
    margin-top: 7px;
    color: #8c95a4;
    font-size: 0.62rem;
  }
  .results {
    border: 1px solid #e2e6ef;
    border-radius: 20px;
    background: #fff;
    padding: 24px 24px 20px;
    box-shadow: 0 10px 34px #25324b0a;
  }
  .heading {
    align-items: center;
    margin-bottom: 18px;
  }
  .heading h2 {
    margin-top: 7px;
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: clamp(1.35rem, 2vw, 1.75rem);
    font-weight: 720;
    letter-spacing: -0.035em;
  }
  .count {
    gap: 7px;
    border: 1px solid #e6e9f0;
    border-radius: 13px;
    background: #f8f9fc;
    padding: 9px 12px;
  }
  .count strong {
    color: var(--signal-dark);
    font: 720 1.35rem/1 'Segoe UI Variable Display', sans-serif;
  }
  .count small {
    color: #7e8797;
    font-size: 0.65rem;
  }
  .message {
    height: calc(100% - 78px);
    border: 0;
    border-radius: 16px;
    background: #f8f9fc;
    color: #7e8797;
  }
  .message.error {
    border: 1px solid #f0cbc6;
    background: #fff5f3;
  }
  .message button {
    border-radius: 999px;
    background: var(--ink);
  }
  .pulse {
    background: var(--signal);
  }
  .toolbar {
    min-height: 42px;
    border: 1px solid #e5e8ef;
    border-bottom: 0;
    border-radius: 12px 12px 0 0;
    background: #f8f9fc;
    padding: 0 13px;
    color: #7d8797;
    font-size: 0.68rem;
  }
  .toolbar button {
    color: var(--signal-dark);
  }
  .partial-warning {
    border-color: #f2d9a8;
    background: #fff9ec;
  }
  .table-wrap {
    border-color: #e5e8ef;
    border-radius: 0 0 12px 12px;
    background: #fff;
    scrollbar-color: #c9cfdb transparent;
    scrollbar-width: thin;
  }
  table {
    min-width: 840px;
    font-size: 0.75rem;
  }
  th {
    background: #f0f2f7;
    color: #7c8697;
    padding: 10px 9px;
    font-size: 0.63rem;
    font-weight: 650;
  }
  td {
    max-width: 220px;
    border-top-color: #eceef3;
    padding: 10px 9px;
    color: #677183;
  }
  tbody tr {
    transition: background 140ms ease;
  }
  tbody tr:hover td {
    background: #fafaff;
  }
  tr.selected td,
  tr.selected:hover td {
    background: #f0efff;
  }
  .name {
    color: #202a3b;
    font-weight: 700;
  }
  .source-name {
    color: var(--signal-dark);
  }
  .status.current,
  .status.success {
    color: #6757e8;
  }
  .retry-item {
    border-radius: 999px;
    background: #eeecff;
    color: #6757e8;
  }
  .empty-signal {
    gap: 9px;
    border: 0;
    border-radius: 16px;
    background-color: #f8f9fc;
    background-image: none;
    color: #8993a3;
  }
  .empty-signal strong {
    margin-top: 8px;
    color: #283247;
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .empty-signal > span {
    font-size: 0.7rem;
  }
  .empty-record {
    position: relative;
    width: 142px;
    height: 142px;
    margin-bottom: 4px;
    border: 1px solid #dfe2ee;
    border-radius: 50%;
    background: #f1f0ff;
    box-shadow: 0 24px 50px #48406516;
  }
  .empty-record::before,
  .empty-record::after,
  .empty-record i,
  .empty-record b {
    position: absolute;
    border-radius: 50%;
    content: '';
  }
  .empty-record::before {
    inset: 16px;
    border: 1px solid #d4d0ef;
  }
  .empty-record::after {
    inset: 34px;
    border: 1px solid #d4d0ef;
  }
  .empty-record i:first-child {
    inset: 50px;
    background: var(--signal);
    box-shadow: 0 0 0 10px #8b7cff12;
  }
  .empty-record i:nth-child(2) {
    inset: 65px;
    background: #fff;
  }
  .empty-record i:nth-child(3) {
    right: 14px;
    top: 22px;
    width: 10px;
    height: 10px;
    background: var(--apricot);
    box-shadow: 0 0 0 7px #ff9a7614;
  }
  .empty-record b {
    right: 22px;
    bottom: 22px;
    width: 38px;
    height: 2px;
    border-radius: 2px;
    background: #b3accf;
    transform: rotate(-36deg);
    transform-origin: right;
  }
  footer {
    gap: 18px;
    min-height: 78px;
    max-height: 104px;
    border-top: 0;
    background: var(--ink);
    padding: 10px clamp(20px, 2.8vw, 38px);
  }
  footer span {
    color: #909caf;
    font-size: 0.67rem;
  }
  .selected-summary {
    gap: 6px;
    border-right: 1px solid #ffffff13;
    padding-right: 18px;
  }
  .selected-summary strong {
    color: var(--apricot);
    font: 720 1.4rem/1 'Segoe UI Variable Display', sans-serif;
  }
  .download-copy strong {
    color: #f1f3f7;
    font-size: 0.7rem;
  }
  .download-copy progress {
    accent-color: var(--signal);
  }
  .queue-stats span {
    color: #7f8ba0;
    font-size: 0.6rem;
  }
  .queue-stats b {
    color: #cbd2de;
  }
  .open,
  .download,
  .cancel,
  .retry-all {
    border-radius: 999px;
    padding: 10px 16px;
  }
  .open,
  .retry-all,
  .cancel {
    background: #ffffff12;
    color: #e6e9ef;
  }
  .download {
    background: var(--signal);
    color: #fff;
  }
  .download:hover:not(:disabled) {
    background: #9b8eff;
  }
  .cancel.all {
    background: #b5514e;
  }
  @keyframes status-breathe {
    50% {
      opacity: 0.45;
      transform: scale(0.75);
    }
  }
  @media (max-width: 1120px) and (min-width: 761px) {
    .workspace {
      grid-template-columns: 280px minmax(0, 1fr);
      padding-inline: 12px;
    }
    .search-panel {
      padding-inline: 17px;
    }
    table {
      min-width: 740px;
    }
    th:nth-child(5),
    td:nth-child(5),
    th:nth-child(6),
    td:nth-child(6) {
      display: none;
    }
  }
  @media (max-width: 760px) {
    .shell {
      grid-template-rows: auto auto auto;
    }
    .topbar {
      min-height: 66px;
      padding-inline: 18px;
    }
    .signal-rail {
      margin-left: auto;
    }
    .workspace {
      grid-template-columns: 1fr;
      gap: 12px;
      overflow: visible;
      padding: 12px;
    }
    .search-panel {
      border: 1px solid #e2e6ef;
      border-radius: 17px;
    }
    .results {
      min-height: 520px;
      border-radius: 17px;
      padding: 20px 15px 16px;
    }
    footer {
      min-height: 110px;
      padding: 10px 14px;
    }
  }
  @media (max-width: 540px) {
    .topbar p,
    .version {
      display: none;
    }
    .mark {
      width: 38px;
      height: 38px;
    }
    .signal-rail {
      padding-inline: 9px;
    }
    form {
      grid-template-columns: 1fr;
    }
    form .keyword,
    form label:nth-of-type(2),
    form label:nth-of-type(3),
    form label:nth-of-type(4),
    form .primary {
      grid-column: 1;
    }
    .count {
      padding: 8px 9px;
    }
    .count small {
      display: none;
    }
    .empty-record {
      width: 122px;
      height: 122px;
    }
    .empty-record i:first-child {
      inset: 42px;
    }
    .empty-record i:nth-child(2) {
      inset: 56px;
    }
  }
  @media (max-height: 620px) and (min-width: 761px) {
    .topbar {
      min-height: 58px;
      padding-block: 7px;
    }
    .workspace {
      padding-block: 10px;
    }
    .search-panel,
    .results {
      border-radius: 15px;
      padding-block: 14px;
    }
    footer {
      min-height: 70px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .signal-rail.active b,
    .primary,
    tbody tr {
      animation: none;
      transition: none;
    }
  }

  /* Yinmi blue-and-white music library */
  .shell {
    --ink: #1d1d1f;
    --ink-soft: #334252;
    --paper: #f6fafe;
    --panel: #f2f8fd;
    --line: #dce5ee;
    --muted: #6e7886;
    --signal: #168be8;
    --signal-dark: #0876d1;
    --action: #0876d1;
    --action-hover: #066fc4;
    --apricot: #64d2af;
    --alert: #d75b53;
    grid-template-rows: auto minmax(0, 1fr) auto;
    background: var(--paper);
    color: var(--ink);
    font-family: 'Segoe UI Variable Text', 'Microsoft YaHei UI',
      'PingFang SC', sans-serif;
  }
  .topbar {
    gap: 12px;
    min-height: 72px;
    border-bottom: 1px solid var(--line);
    background: #ffffffed;
    padding: 10px clamp(20px, 2.5vw, 34px);
    color: var(--ink);
    box-shadow: 0 1px 12px #168be810;
    backdrop-filter: blur(18px);
    user-select: none;
  }
  .brand-copy {
    flex: 0 0 auto;
  }
  .topbar h1 {
    color: var(--ink);
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: 1.32rem;
    font-weight: 750;
    letter-spacing: 0.1em;
  }
  .topbar p {
    color: #718295;
    font-size: 0.7rem;
  }
  .mark {
    width: 43px;
    height: 43px;
    border: 0;
    border-radius: 11px;
    background: var(--signal);
    box-shadow: 0 8px 24px #168be824;
  }
  .mark i {
    border-width: 3px;
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
    border: 1px solid #d6e9f7;
    border-radius: 999px;
    background: #f2f9fe;
    padding: 0 11px;
    color: #5f7385;
    font: 650 0.68rem/1 'Microsoft YaHei UI', sans-serif;
    letter-spacing: 0.03em;
  }
  .signal-rail b {
    width: 6px;
    height: 6px;
    background: #64d2af;
    box-shadow: 0 0 0 4px #64d2af1d;
  }
  .signal-rail i {
    display: none;
  }
  .signal-rail.active {
    color: var(--signal-dark);
  }
  .signal-rail.active b {
    background: var(--signal);
    box-shadow: 0 0 0 4px #168be819;
  }
  .version {
    display: flex;
    align-items: center;
    height: 32px;
    border-left: 1px solid var(--line);
    padding-left: 12px;
    color: #8a98a6;
    font: 600 0.66rem/1 'Cascadia Code', Consolas, monospace;
  }
  .workspace {
    grid-template-columns: minmax(300px, 326px) minmax(0, 1fr);
    gap: 0;
    background: #fff;
    padding: 0;
  }
  .search-panel {
    border: 0;
    border-right: 1px solid var(--line);
    border-radius: 0;
    background: #f2f7fb;
    padding: 23px 21px 28px;
    box-shadow: none;
    scrollbar-color: #bdcad5 transparent;
  }
  .panel-heading {
    margin-bottom: 25px;
  }
  .panel-heading span,
  .section-label span,
  .heading > div > span {
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
    color: #718295;
    font-size: 0.73rem;
  }
  .section-label {
    margin-bottom: 12px;
    border-bottom: 1px solid #dce5ee;
    padding-bottom: 8px;
  }
  .section-label b {
    color: #6d7b89;
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
    border-left: 1px solid #cfdae4;
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
  .directory-setting > span:first-child {
    color: #566879;
    font-size: 0.69rem;
    font-weight: 650;
  }
  input:not([type='checkbox']),
  select {
    height: 40px;
    border: 1px solid #cedae5;
    border-radius: 8px;
    background: #fff;
    padding: 0 11px;
    color: var(--ink);
  }
  .keyword input {
    height: 46px;
    border-color: #b9d9ef;
    background: #fff;
    font-size: 0.88rem;
    box-shadow: 0 4px 16px #168be809;
  }
  input::placeholder {
    color: #9aa7b3;
  }
  input:focus,
  select:focus,
  button:focus-visible {
    border-color: var(--signal);
    outline: none;
    box-shadow: 0 0 0 3px #168be821;
  }
  button {
    border-radius: 8px;
  }
  .primary {
    height: 40px;
    border: 1px solid transparent;
    background: var(--action);
    color: #fff;
    transition:
      transform 150ms ease,
      background 150ms ease;
  }
  .primary:hover:not(:disabled) {
    background: var(--action-hover);
    transform: translateY(-1px);
  }
  .hint {
    color: #83919f;
    font-size: 0.65rem;
  }
  .download-settings {
    gap: 9px;
  }
  .toggle-setting {
    height: 42px;
    border-color: #d4dfe8;
    border-radius: 8px;
    background: #fff;
  }
  .toggle-setting input,
  .check {
    accent-color: var(--signal);
  }
  .toggle-setting span {
    color: #3c4d5d;
  }
  .toggle-setting strong {
    font-size: 0.74rem;
  }
  .toggle-setting small {
    color: #8795a2;
    font-size: 0.64rem;
  }
  .directory-control button {
    border-color: #cedae5;
    border-radius: 8px;
    background: #fff;
    color: var(--signal-dark);
    font-size: 0.72rem;
  }
  .directory-message {
    color: #83919f;
    font-size: 0.66rem;
  }
  .results {
    border: 0;
    border-radius: 0;
    background: #fff;
    padding: 24px clamp(20px, 2.4vw, 32px) 18px;
    box-shadow: none;
  }
  .heading {
    align-items: center;
    margin-bottom: 17px;
  }
  .heading h2 {
    margin-top: 7px;
    color: var(--ink);
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: clamp(1.5rem, 2vw, 1.8rem);
    font-weight: 750;
    letter-spacing: -0.04em;
  }
  .count {
    display: flex;
    align-items: baseline;
    justify-content: end;
    gap: 7px;
    border: 0;
    border-radius: 0;
    background: transparent;
    padding: 0;
  }
  .count strong {
    color: var(--signal);
    font: 720 1.9rem/1 'Segoe UI Variable Display', sans-serif;
    font-variant-numeric: tabular-nums;
  }
  .result-total {
    display: flex;
    align-items: baseline;
    gap: 5px;
    white-space: nowrap;
  }
  .count small {
    display: block;
    flex: 0 0 auto;
    color: #6e7886;
    font-size: 0.7rem;
    white-space: nowrap;
  }
  .selected-count {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: baseline;
    gap: 3px;
    border: 0;
    border-left: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
    padding-left: 8px;
    color: #687b8c;
    font-size: 0.68rem;
    font-weight: 650;
    white-space: nowrap;
  }
  .selected-count b {
    display: inline-block;
    width: 3ch;
    color: var(--signal-dark);
    font: 750 0.72rem/1 'Segoe UI Variable Text', sans-serif;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .message {
    height: calc(100% - 76px);
    border: 1px solid #e0e9f0;
    border-radius: 12px;
    background: #f8fbfd;
    color: #71808e;
  }
  .message button {
    background: var(--action);
  }
  .message.error {
    border-color: #efcbc6;
    background: #fff8f7;
  }
  .pulse {
    background: var(--signal);
  }
  .toolbar {
    min-height: 40px;
    border: 1px solid var(--line);
    border-bottom: 0;
    border-radius: 12px 12px 0 0;
    background: #f8fbfd;
    color: #6f7e8d;
    font-size: 0.76rem;
  }
  .toolbar button {
    color: var(--signal-dark);
  }
  .partial-warning {
    border-color: #ead7a5;
    background: #fffaf0;
  }
  .table-wrap {
    border-color: var(--line);
    border-radius: 0 0 12px 12px;
    background: #fff;
    scrollbar-color: #bdcad5 transparent;
  }
  table {
    min-width: 710px;
    table-layout: fixed;
    font-size: 0.78rem;
  }
  th {
    background: #f2f6f9;
    color: #687787;
    padding: 9px 10px;
    font-size: 0.67rem;
    font-weight: 650;
  }
  th:first-child,
  td:first-child {
    width: 42px;
    padding-inline: 12px 4px;
  }
  th:nth-child(2),
  td:nth-child(2) {
    width: 56px;
    padding-inline: 6px;
    text-align: center;
  }
  th:nth-child(3),
  td:nth-child(3) {
    width: 34%;
  }
  th:nth-child(4),
  td:nth-child(4) {
    width: auto;
  }
  th:nth-child(5),
  td:nth-child(5) {
    width: 70px;
    text-align: center;
  }
  th:nth-child(6),
  td:nth-child(6) {
    width: 120px;
  }
  td {
    max-width: none;
    border-top-color: #e7edf2;
    padding: 9px 10px;
    color: #637383;
  }
  tbody tr:hover td {
    background: #f7fbfe;
  }
  tr.selected td,
  tr.selected:hover td {
    background: #eaf6ff;
  }
  .track-info,
  .album-info {
    overflow: hidden;
    white-space: normal;
  }
  .track-info strong,
  .track-info small,
  .album-info strong,
  .album-info small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track-info strong,
  .album-info strong {
    color: #273746;
    font-size: 0.78rem;
    font-weight: 650;
  }
  .track-info small,
  .album-info small {
    margin-top: 3px;
    color: #8794a0;
    font-size: 0.67rem;
  }
  .track-info .name {
    color: var(--ink);
    font-weight: 720;
  }
  .album-info .source-name {
    color: var(--signal-dark);
  }
  .duration {
    color: #536575;
  }
  .index {
    font-variant-numeric: tabular-nums;
    text-overflow: clip;
  }
  .status-cell {
    min-width: 0;
  }
  .status {
    font-size: 0.72rem;
  }
  .retry-item,
  .item-message,
  .warnings {
    font-size: 0.66rem;
  }
  .status.current,
  .status.success {
    color: var(--signal-dark);
  }
  .status.skipped {
    color: #668775;
  }
  .retry-item {
    border-radius: 999px;
    background: #e3f2fc;
    color: var(--signal-dark);
  }
  .empty-signal {
    border: 1px solid #e0e9f0;
    border-radius: 12px;
    background: #f8fbfd;
    color: #7b8996;
  }
  .empty-signal strong {
    color: #273746;
    font-size: 1.1rem;
  }
  .empty-signal > span {
    font-size: 0.76rem;
  }
  .empty-record {
    border-color: #d6e8f5;
    background: #edf8ff;
    box-shadow: 0 20px 46px #168be812;
  }
  .empty-record::before,
  .empty-record::after {
    border-color: #c5e1f3;
  }
  .empty-record i:first-child {
    background: var(--signal);
    box-shadow: 0 0 0 10px #168be810;
  }
  .empty-record i:nth-child(3) {
    background: #64d2af;
    box-shadow: 0 0 0 7px #64d2af18;
  }
  .empty-record b {
    background: #7fbce4;
  }
  footer {
    gap: 16px;
    min-height: 95px;
    max-height: 108px;
    border-top: 1px solid var(--line);
    background: #ffffffef;
    padding: 8px clamp(20px, 2.5vw, 34px);
    color: var(--ink);
    box-shadow: 0 -8px 24px #168be808;
    backdrop-filter: blur(18px);
  }
  footer span {
    color: #6f7f8e;
    font-size: 0.72rem;
  }
  .selected-summary {
    width: 124px;
    align-items: center;
    gap: 0;
    height: 32px;
    border-right: 1px solid var(--line);
    padding-right: 17px;
  }
  .selected-summary span {
    flex: 0 0 auto;
    line-height: 1;
    white-space: nowrap;
  }
  .selected-summary-line {
    display: flex;
    align-items: flex-end;
    gap: 5px;
    height: 32px;
    padding-bottom: 5px;
    white-space: nowrap;
  }
  .selected-summary strong {
    display: inline-block;
    min-width: 3ch;
    color: var(--signal);
    font: 720 1.4rem/1 'Segoe UI Variable Display', sans-serif;
    font-variant-numeric: tabular-nums;
    text-align: center;
  }
  .download-copy strong {
    color: #263746;
    font-size: 0.75rem;
  }
  .download-copy progress {
    accent-color: var(--signal);
  }
  .queue-stats {
    flex-wrap: nowrap;
    align-items: flex-end;
    gap: 3px 7px;
    height: 32px;
    min-height: 32px;
    margin-top: 0;
    padding-bottom: 5px;
  }
  .queue-stats span {
    display: inline-flex;
    align-items: flex-end;
    width: auto;
    gap: 2px;
    color: #80909f;
    font-size: 0.76rem;
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
  }
  .queue-stats b {
    display: inline-block;
    min-width: 3ch;
    color: currentColor;
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: 0.8rem;
    font-weight: 750;
    font-variant-numeric: tabular-nums;
    line-height: 1;
    text-align: right;
  }
  .queue-stats .waiting {
    color: #718191;
  }
  .queue-stats .current {
    color: var(--signal-dark);
  }
  .queue-stats .succeeded {
    color: #258367;
  }
  .queue-stats .skipped {
    color: #9a6a18;
  }
  .queue-stats .failed {
    color: #c44f48;
  }
  .queue-stats .cancelled {
    color: #7c8793;
  }
  .queue-stats .total {
    color: #344f65;
  }
  .footer-error {
    color: #c44f48 !important;
    font-size: 0.72rem;
  }
  .open,
  .download,
  .cancel,
  .retry-all {
    border-radius: 8px;
    padding: 10px 16px;
  }
  .open,
  .retry-all,
  .cancel {
    border: 1px solid #cfe0ed;
    background: #f2f8fc;
    color: var(--signal-dark);
  }
  .download {
    min-width: 144px;
    background: var(--action);
    color: #fff;
    box-shadow: 0 8px 22px #168be825;
  }
  .download:hover:not(:disabled) {
    background: var(--action-hover);
  }
  .cancel.all {
    border-color: #ebc3bf;
    background: #fff5f4;
    color: #bc4d46;
  }
  @media (max-width: 1120px) and (min-width: 761px) {
    .workspace {
      grid-template-columns: 280px minmax(0, 1fr);
    }
    .search-panel {
      padding-inline: 17px;
    }
    table {
      min-width: 680px;
    }
    th:nth-child(5),
    td:nth-child(5),
    th:nth-child(6),
    td:nth-child(6) {
      display: table-cell;
    }
  }
  @media (max-width: 760px) {
    .topbar {
      padding-inline: 16px;
    }
    .workspace {
      grid-template-columns: 1fr;
      gap: 0;
      padding: 0;
    }
    .search-panel {
      border: 0;
      border-bottom: 1px solid var(--line);
      border-radius: 0;
    }
    .results {
      border-radius: 0;
    }
  }
  @media (max-width: 540px) {
    .count small {
      display: none;
    }
  }
  @media (max-height: 850px) and (min-width: 761px) {
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
    .results {
      padding-top: 22px;
    }
    footer {
      min-height: 58px;
      padding-block: 6px;
    }
  }
  @media (max-height: 620px) and (min-width: 761px) {
    .topbar {
      min-height: 58px;
    }
    .search-panel,
    .results {
      border-radius: 0;
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
    label {
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
    footer {
      min-height: 58px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .primary,
    tbody tr {
      transition: none;
    }
  }
</style>
