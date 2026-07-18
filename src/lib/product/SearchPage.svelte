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
  let renderedSongCount = 150;
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
    renderedSongCount = Math.min(150, value.songs.length);
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

  function handleTableScroll(event: Event) {
    const viewport = event.currentTarget as HTMLElement;
    if (
      result &&
      renderedSongCount < result.songs.length &&
      viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < 240
    ) {
      renderedSongCount = Math.min(
        result.songs.length,
        renderedSongCount + 150,
      );
    }
  }

  const renderedSongs = () => result?.songs.slice(0, renderedSongCount) ?? [];

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
    renderedSongCount = 150;
    cancellingScope = null;
    retryingTarget = null;
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
      cancellingScope = null;
    }
  }

  async function restoreDefaultDirectory() {
    if (downloading) return;
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
  <header class="topbar">
    <div class="mark" aria-hidden="true"><i></i><i></i><b></b></div>
    <div>
      <h1>音觅</h1>
      <p>跨音源批量搜索与下载工作台</p>
    </div>
    <RuntimeTools />
    <span class="version">DESKTOP{appVersion ? ` · ${appVersion}` : ''}</span>
  </header>

  <div
    class:active={searching || downloading}
    class:downloading
    class="signal-rail"
    aria-hidden="true"
  >
    <span>{downloading ? 'TRANSFER' : searching ? 'SCANNING' : 'READY'}</span>
    <i></i>
    <b></b>
  </div>

  <div class="workspace">
  <aside class="search-panel" aria-label="搜索与下载设置">
    <header class="panel-heading">
      <span>CONTROL DECK</span>
      <h2>搜索与下载</h2>
      <p>设置一次，然后从结果中挑选要带回本地的歌曲。</p>
    </header>
    <div class="section-label"><span>DISCOVERY</span><b>搜索设置</b></div>
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
    <div class="section-label download-label"
      ><span>OUTPUT</span><b>下载设置</b></div
    >
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
          <button
            type="button"
            disabled={downloading || directoryLoading}
            onclick={() => void restoreDefaultDirectory()}>恢复默认</button
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
        <span>RESULT MONITOR</span>
        <h2>
          {result?.keyword ??
            (searching ? '正在连接音乐服务' : '从一个关键词开始')}
        </h2>
      </div>
      <div class="count">
        <strong>{result?.returnedCount ?? 0}</strong><small
          >首歌曲 · 已选 {selected.size}</small
        >
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
      <div class="table-wrap" onscroll={handleTableScroll}>
        <table>
          <thead
            ><tr
              ><th></th><th>#</th><th>歌曲</th><th>艺人</th><th>音源</th><th
                >专辑</th
              ><th>时长</th><th>能力</th><th>下载状态</th></tr
            ></thead
          >
          <tbody
            >{#each renderedSongs() as song, index (keyOf(song))}
              {@const item = downloadItems.get(song.id)}
              {@const isCurrent =
                downloading && downloadProgress?.currentSongId === song.id}
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
                <td class="index">{index + 1}</td><td class="name"
                  >{song.name}</td
                ><td>{song.artistDisplay}</td><td class="source-name"
                  >{sourceLabel(song.source)}</td
                ><td>{song.album ?? '—'}</td><td class="duration"
                  >{formatDuration(song.durationMs)}</td
                >
                <td
                  ><div class="badges">
                    {#if song.urlId}<span>音频</span>{/if}{#if song.picId}<span
                        >封面</span
                      >{/if}{#if song.lyricId}<span>歌词</span
                      >{/if}{#if song.hasHires}<span>Hi-Res</span
                      >{/if}{#if !song.urlId}<span class="muted">不可下载</span
                      >{/if}
                  </div></td
                >
                <td class="status-cell">
                  <div class="status-line">
                    {#if isCurrent}<span class="status current"
                        >{retryingTarget ? '重试中' : '下载中'}</span
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
                    {#if item?.state === 'failed' || item?.state === 'cancelled'}
                      <button
                        class="retry-item"
                        type="button"
                        aria-label={`重试下载 ${song.name}`}
                        disabled={downloading}
                        onclick={() => void retryFailed(song.id)}>重试</button
                      >
                    {/if}
                  </div>
                  {#if item?.message && (item.state === 'failed' || item.state === 'cancelled')}
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
        <div class="waveform" aria-hidden="true">
          <i></i><i></i><i></i><i></i><i></i><i></i><i></i><i></i><i></i
          ><i></i><i></i><i></i><i></i><i></i><i></i>
        </div>
        <strong>等待搜索信号</strong>
        <span>输入关键词并选择音源，结果会显示在这里。</span>
      </div>
    {/if}
  </section>
  </div>

  <footer aria-label="下载队列">
    <div class="selected-summary">
      <span>已选择</span><strong>{selected.size}</strong><span>首歌曲</span>
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
      {:else}
        <strong
          >音质档位 {bitrate} · {embedCover ? '嵌入封面' : '不嵌入封面'} · {downloadLyrics
            ? '下载歌词'
            : '不下载歌词'}</strong
        ><span title={baseDirectory}
          >{baseDirectory || '系统音乐目录'}/{result?.keyword ??
            '搜索关键词'}</span
        >
      {/if}
      <div class="queue-stats" aria-label="队列统计" aria-live="polite">
        <span>等待 <b>{queueStats().waiting}</b></span>
        <span>当前 <b>{queueStats().current}</b></span>
        <span>成功 <b>{queueStats().succeeded}</b></span>
        <span>跳过 <b>{queueStats().skipped}</b></span>
        <span>失败 <b>{queueStats().failed}</b></span>
        <span>取消 <b>{queueStats().cancelled}</b></span>
        <span>总计 <b>{queueStats().total}</b></span>
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
  .directory-control button:last-child {
    grid-column: 1 / -1;
    min-height: 30px;
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
  .badges {
    display: flex;
    gap: 4px;
  }
  .badges span {
    border-radius: 3px;
    background: #e2eee5;
    padding: 3px 5px;
    color: #356849;
    font-size: 0.62rem;
  }
  .badges .muted {
    background: #f0f1f2;
    color: #89939c;
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
  .waveform {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 72px;
    margin-bottom: 7px;
    border-inline: 1px solid #bec6be;
    padding-inline: 18px;
  }
  .waveform i {
    width: 3px;
    height: 14px;
    background: #6b7b72;
  }
  .waveform i:nth-child(2),
  .waveform i:nth-child(14) {
    height: 22px;
  }
  .waveform i:nth-child(3),
  .waveform i:nth-child(6),
  .waveform i:nth-child(11),
  .waveform i:nth-child(13) {
    height: 36px;
  }
  .waveform i:nth-child(4),
  .waveform i:nth-child(10) {
    height: 54px;
    background: #2c8a54;
  }
  .waveform i:nth-child(5),
  .waveform i:nth-child(9) {
    height: 28px;
  }
  .waveform i:nth-child(7) {
    height: 62px;
    background: var(--signal-dark);
  }
  .waveform i:nth-child(8) {
    height: 44px;
    background: #2c8a54;
  }
  .waveform i:nth-child(12) {
    height: 18px;
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
</style>
