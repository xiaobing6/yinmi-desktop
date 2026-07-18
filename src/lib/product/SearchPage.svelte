<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import RuntimeTools from './RuntimeTools.svelte';

  type SourceCode =
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
  type SearchMode = 'track' | 'album' | 'playlist';
  type DownloadState = 'success' | 'skipped' | 'failed' | 'cancelled';
  type DownloadProgressState = 'preparing' | 'downloading' | 'finished';
  type RetryTarget = string | 'all' | null;

  interface Song {
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
  interface SearchResult {
    keyword: string;
    source: SourceCode;
    sourceName: string;
    returnedCount: number;
    skippedRecords: number;
    incomplete: boolean;
    stopReason: string;
    songs: Song[];
  }
  interface DownloadProgress {
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
    currentDownloadedBytes: number;
    currentTotalBytes: number | null;
    bytesPerSecond: number;
  }
  interface DownloadItemResult {
    songId: string;
    name: string;
    state: DownloadState;
    path: string | null;
    bytes: number;
    code: string | null;
    message: string | null;
    warnings: string[];
  }
  interface DownloadBatchResult {
    batchId: number;
    directory: string;
    total: number;
    succeeded: number;
    skipped: number;
    failed: number;
    cancelled: number;
    items: DownloadItemResult[];
  }

  const sources: Array<[SourceCode, string]> = [
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
  const modes: Array<[SearchMode, string]> = [
    ['track', '单曲匹配'],
    ['album', '专辑匹配'],
    ['playlist', '歌单匹配'],
  ];

  let keyword = '';
  let source: SourceCode = 'netease_music';
  let mode: SearchMode = 'track';
  let count = 20;
  let searching = false;
  let result: SearchResult | null = null;
  let errorMessage = '';
  let selected = new Set<string>();
  let requestSerial = 0;

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
    let unlisten: (() => void) | undefined;
    void invoke<string>('music_get_default_directory')
      .then((directory) => {
        if (disposed) return;
        defaultDirectory = directory;
        if (!baseDirectory.trim()) baseDirectory = directory;
      })
      .catch((error) => {
        if (!disposed)
          directoryMessage = `无法读取默认目录：${errorText(error)}`;
      })
      .finally(() => {
        if (!disposed) directoryLoading = false;
      });
    void listen<DownloadProgress>('music-download-progress', (event) => {
      if (downloading) {
        downloadProgress = event.payload;
        if (event.payload.state === 'finished') cancellingScope = null;
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  const keyOf = (song: Song) => `${song.source}:${song.id}`;
  const downloadableSongs = () =>
    result?.songs.filter((song) => song.urlId) ?? [];
  const selectedSongs = () =>
    downloadableSongs().filter((song) => selected.has(keyOf(song)));

  function sourceLabel(code: string) {
    return sources.find(([value]) => value === code)?.[1] ?? code;
  }

  function errorText(error: unknown) {
    return typeof error === 'object' && error !== null && 'message' in error
      ? String((error as { message: unknown }).message)
      : String(error);
  }

  function duration(value: number | null) {
    if (value === null) return '--:--';
    const seconds = Math.max(0, Math.floor(value / 1000));
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  }

  function bytes(value: number) {
    if (value < 1024) return `${value} B`;
    if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
    if (value < 1024 * 1024 * 1024)
      return `${(value / 1024 / 1024).toFixed(1)} MiB`;
    return `${(value / 1024 / 1024 / 1024).toFixed(2)} GiB`;
  }

  function stopReasonText(value: string) {
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

  function resetResultState() {
    requestSerial += 1;
    result = null;
    selected = new Set();
    errorMessage = '';
    downloadResult = null;
    downloadProgress = null;
    downloadError = '';
    downloadItems = new Map();
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
    const merged = new Map(downloadItems);
    for (const item of value.items) merged.set(item.songId, item);
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
      if (serial === requestSerial) result = value;
    } catch (error) {
      if (serial === requestSerial) errorMessage = errorText(error);
    } finally {
      if (serial === requestSerial) searching = false;
    }
  }

  function toggle(song: Song) {
    if (!song.urlId || downloading) return;
    const next = new Set(selected);
    const key = keyOf(song);
    next.has(key) ? next.delete(key) : next.add(key);
    selected = next;
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
          keyword: result.keyword,
          source: result.source,
          songs,
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
      return;
    }
    directoryLoading = true;
    directoryMessage = '';
    try {
      defaultDirectory = await invoke<string>('music_get_default_directory');
      baseDirectory = defaultDirectory;
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
      if (typeof selectedDirectory === 'string')
        baseDirectory = selectedDirectory;
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
      <p>搜索音乐，把喜欢的声音带回本地</p>
    </div>
    <RuntimeTools />
    <span class="version">DESKTOP · 0.1.0</span>
  </header>

  <section class="search-panel" aria-label="搜索设置">
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
          >{#each sources as item}<option value={item[0]}>{item[1]}</option
            >{/each}</select
        ></label
      >
      <label
        ><span>匹配方式</span><select
          bind:value={mode}
          disabled={searching || downloading}
          onchange={searchSettingChanged}
          >{#each modes as item}<option value={item[0]}>{item[1]}</option
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
    <p class="hint">
      三种模式均返回歌曲列表。搜索数量范围 1–1000，默认使用网易云音乐。
    </p>
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
  </section>

  <section
    class="results"
    aria-busy={searching || downloading}
    aria-live="polite"
  >
    <div class="heading">
      <div>
        <span>搜索结果</span>
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
        <i class="pulse"></i><strong>正在获取搜索结果</strong><span
          >首次搜索可能需要几秒钟。</span
        >
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
            >{stopReasonText(
              result.stopReason,
            )}，当前歌曲仍可正常选择和下载。</span
          >
        </div>
      {/if}
      <div class="table-wrap">
        <table>
          <thead
            ><tr
              ><th></th><th>#</th><th>歌曲</th><th>艺人</th><th>音源</th><th
                >专辑</th
              ><th>时长</th><th>能力</th><th>下载状态</th></tr
            ></thead
          >
          <tbody
            >{#each result.songs as song, index (keyOf(song))}
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
                  >{duration(song.durationMs)}</td
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
                        {#each item.warnings as warning}<p>{warning}</p>{/each}
                      </div>
                    </details>
                  {/if}
                </td>
              </tr>{/each}</tbody
          >
        </table>
      </div>
    {:else}
      <div class="radar" aria-hidden="true"><i></i><i></i><b></b></div>
    {/if}
  </section>

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
          {bytes(
            downloadProgress.currentDownloadedBytes,
          )}{downloadProgress.currentTotalBytes
            ? ` / ${bytes(downloadProgress.currentTotalBytes)}`
            : ''}
          {downloadProgress.bytesPerSecond
            ? ` · ${bytes(downloadProgress.bytesPerSecond)}/s`
            : ''}
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
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr) auto;
    height: 100vh;
    padding-bottom: 96px;
    background: #f6f9fc;
    color: #16283e;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 68px;
    border-bottom: 1px solid #d3dee8;
    background: #fff;
    padding: 10px clamp(16px, 3vw, 40px);
  }
  .topbar h1 {
    margin: 0;
    font-size: 1.45rem;
    letter-spacing: 0.12em;
  }
  .topbar p {
    margin: 2px 0 0;
    color: #718091;
    font-size: 0.78rem;
  }
  .version {
    flex: 0 0 auto;
    color: #8997a5;
    font:
      600 0.66rem ui-monospace,
      monospace;
  }
  .mark {
    position: relative;
    width: 44px;
    height: 44px;
    border-radius: 13px;
    background: #1478c9;
  }
  .mark i,
  .mark b {
    position: absolute;
    border-radius: 50%;
  }
  .mark i {
    border: 2px solid #ffffffb8;
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
    background: #4ac58b;
  }
  .search-panel {
    border-bottom: 1px solid #c7d8e7;
    background: #e7f0f8;
    padding: 12px clamp(16px, 3vw, 40px) 10px;
  }
  form {
    display: grid;
    grid-template-columns:
      minmax(200px, 1.8fr) minmax(120px, 0.8fr) minmax(120px, 0.8fr)
      76px auto;
    align-items: end;
    gap: 9px;
  }
  label {
    display: grid;
    gap: 6px;
  }
  form label > span,
  .quality-setting > span,
  .directory-setting > span:first-child,
  .heading > div > span {
    color: #607487;
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  input:not([type='checkbox']),
  select {
    width: 100%;
    height: 38px;
    border: 1px solid #aabfd1;
    border-radius: 5px;
    outline: none;
    background: #fffffff0;
    padding: 0 11px;
    color: #16283e;
    font: inherit;
  }
  input:focus,
  select:focus,
  button:focus-visible {
    border-color: #1478c9;
    box-shadow: 0 0 0 3px #1478c929;
  }
  button {
    cursor: pointer;
    border: 0;
    border-radius: 5px;
    font: inherit;
    font-weight: 700;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  .primary {
    height: 38px;
    background: #1478c9;
    padding: 0 22px;
    color: #fff;
  }
  .hint {
    margin: 7px 0 0;
    color: #6c7d8d;
    font-size: 0.74rem;
  }
  .download-settings {
    display: grid;
    grid-template-columns: 135px 145px 145px minmax(260px, 1fr);
    align-items: end;
    gap: 9px;
    margin-top: 9px;
    border-top: 1px solid #c7d8e7;
    padding-top: 9px;
  }
  .quality-setting,
  .directory-setting {
    min-width: 0;
  }
  .toggle-setting {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 38px;
    border: 1px solid #b8cbd9;
    border-radius: 5px;
    background: #f8fbfdf0;
    padding: 0 10px;
    cursor: pointer;
  }
  .toggle-setting input,
  .check {
    flex: 0 0 auto;
    width: 15px;
    height: 15px;
    accent-color: #1478c9;
  }
  .toggle-setting span {
    display: grid;
    color: #385269;
    letter-spacing: 0;
    text-transform: none;
  }
  .toggle-setting strong {
    font-size: 0.72rem;
  }
  .toggle-setting small {
    color: #7a8d9d;
    font-size: 0.61rem;
  }
  .directory-control {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 6px;
  }
  .directory-control button {
    border: 1px solid #aabfd1;
    background: #fff;
    padding: 0 12px;
    color: #315b7b;
    font-size: 0.7rem;
    white-space: nowrap;
  }
  .directory-message {
    margin: 5px 0 0;
    color: #708292;
    font-size: 0.66rem;
    text-align: right;
  }
  .directory-message.directory-error {
    color: #b23e37;
  }
  .results {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    padding: 14px clamp(16px, 3vw, 40px) 0;
  }
  .heading {
    display: flex;
    flex: 0 0 auto;
    align-items: flex-end;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .heading h2 {
    margin: 3px 0 0;
    font-size: 1.28rem;
  }
  .count {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .count strong {
    color: #1478c9;
    font:
      700 1.9rem/1 ui-monospace,
      monospace;
  }
  .count small {
    color: #738292;
  }
  .message {
    display: grid;
    place-items: center;
    align-content: center;
    gap: 8px;
    height: calc(100% - 70px);
    min-height: 220px;
    border: 1px dashed #bdcad5;
    color: #718091;
  }
  .message strong {
    color: #263b50;
  }
  .message.error {
    border-color: #ddaeaa;
    background: #fff7f6;
  }
  .message button {
    background: #16283e;
    padding: 8px 18px;
    color: #fff;
  }
  .pulse {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: #1478c9;
    animation: pulse 1.1s ease-in-out infinite;
  }
  .toolbar {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    min-height: 39px;
    border: 1px solid #d4dee7;
    border-bottom: 0;
    background: #fff;
    padding: 0 12px;
    color: #718091;
    font-size: 0.74rem;
  }
  .toolbar button {
    background: transparent;
    color: #1478c9;
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
    border: 1px solid #d4dee7;
    background: #fff;
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
    background: #edf3f8;
    color: #66798a;
    padding: 10px 12px;
    text-align: left;
    font-size: 0.68rem;
  }
  td {
    max-width: 250px;
    overflow: hidden;
    border-top: 1px solid #e6edf3;
    padding: 10px 12px;
    color: #566777;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  tr.selected td {
    background: #f0f7fd;
  }
  .index {
    color: #93a0ac;
  }
  .name {
    color: #16283e;
    font-weight: 700;
  }
  .duration {
    font-family: ui-monospace, monospace;
  }
  .source-name {
    color: #3f7198;
    font-size: 0.72rem;
  }
  .badges {
    display: flex;
    gap: 4px;
  }
  .badges span {
    border-radius: 3px;
    background: #e9f2fa;
    padding: 3px 5px;
    color: #386d96;
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
    color: #1478c9;
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
    background: #e6f1fa;
    padding: 3px 7px;
    color: #176da9;
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
  .radar {
    position: relative;
    width: 170px;
    height: 170px;
    margin: min(8vh, 60px) auto;
    border: 1px solid #d2e0eb;
    border-radius: 50%;
  }
  .radar:before,
  .radar:after,
  .radar i,
  .radar b {
    content: '';
    position: absolute;
    border-radius: 50%;
  }
  .radar:before {
    inset: 29px;
    border: 1px solid #c8dae8;
  }
  .radar:after {
    inset: 60px;
    border: 1px solid #bdd4e5;
  }
  .radar i {
    left: 50%;
    top: 50%;
    width: 1px;
    height: 78px;
    border-radius: 0;
    background: #d3e0ea;
    transform-origin: top;
  }
  .radar i:first-child {
    transform: rotate(45deg);
  }
  .radar i:nth-child(2) {
    transform: rotate(135deg);
  }
  .radar b {
    right: 25px;
    top: 35px;
    width: 9px;
    height: 9px;
    background: #4ac58b;
  }
  footer {
    position: fixed;
    z-index: 10;
    right: 0;
    bottom: 0;
    left: 0;
    display: flex;
    align-items: center;
    gap: 16px;
    height: 96px;
    overflow-y: auto;
    background: #16283e;
    padding: 9px clamp(16px, 3vw, 40px);
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
    accent-color: #4ac58b;
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
    background: #4ac58b;
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
    background: #8f443f;
    color: #fff;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
      transform: scale(0.8);
    }
  }
  @media (max-width: 760px) {
    .shell {
      padding-bottom: 126px;
    }
    form {
      grid-template-columns: 1fr 1fr;
    }
    .keyword {
      grid-column: 1/-1;
    }
    .primary {
      grid-column: 2;
    }
    .download-settings {
      grid-template-columns: 1fr 1fr;
    }
    .directory-setting {
      grid-column: 1/-1;
    }
    .results {
      padding-inline: 12px;
    }
    footer {
      align-items: stretch;
      gap: 8px;
      height: 126px;
      padding: 8px 12px;
    }
    .selected-summary {
      display: none;
    }
    .download-copy > span {
      max-width: 360px;
    }
    .footer-actions {
      flex-direction: column;
      justify-content: center;
    }
    .footer-actions button {
      width: 100%;
      padding-block: 7px;
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
    .directory-setting {
      grid-column: auto;
    }
    .toolbar,
    .partial-warning {
      align-items: flex-start;
      flex-direction: column;
      padding-block: 6px;
    }
  }
  @media (max-height: 560px) and (min-width: 761px) {
    .shell {
      padding-bottom: 90px;
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
      padding-block: 7px 6px;
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
      height: 90px;
      padding-block: 6px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse {
      animation: none;
    }
  }
</style>
