<script lang="ts">
  import {
    formatDuration,
    sourceLabel,
    stopReasonLabel,
    type DownloadItemResult,
    type DownloadProgress,
    type SearchResult,
    type Song,
  } from '../music/model';
  import type { RetryTarget } from './downloadView';

  export let searching: boolean;
  export let downloading: boolean;
  export let result: SearchResult | null;
  export let errorMessage: string;
  export let selected: Set<string>;
  export let downloadableCount: number;
  export let existingAudio: Map<string, string[]>;
  export let rateLimitSeconds: number;
  export let downloadProgress: DownloadProgress | null;
  export let downloadItems: Map<string, DownloadItemResult>;
  export let retryingTarget: RetryTarget;
  export let retryPendingSongIds: Set<string>;
  export let onSearch: () => void;
  export let onToggleAll: () => void;
  export let onToggle: (song: Song) => void;
  export let onRetry: (songId: string) => void;

  const keyOf = (song: Song) => `${song.source}:${song.id}`;
</script>

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
        onclick={onSearch}>重试</button
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
      <strong>没有找到歌曲</strong><span>换一个关键词、音源或匹配方式再试。</span>
    </div>
  {:else if result}
    <div class="toolbar">
      <button type="button" onclick={onToggleAll} disabled={downloading}
        >{selected.size === downloadableCount && selected.size > 0
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
                  onchange={() => onToggle(song)}
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
                  {:else if isRetryPending}<span class="status waiting">等待</span>
                  {:else if item?.state === 'success'}<span class="status success"
                      >已下载</span
                    >
                  {:else if item?.state === 'skipped'}<span class="status skipped"
                      >已存在</span
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
                      onclick={() => onRetry(song.id)}>重试</button
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

<style>
button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

.message strong {
    color: var(--ink);
  }

.partial-warning strong {
    color: #6a4e12;
    white-space: nowrap;
  }

tr.selected td {
    background: #e8f5ec;
  }

.status-line {
    display: flex;
    align-items: center;
    gap: 7px;
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

.status.failed {
    color: #b63f38;
  }

.status.cancelled {
    color: #7b8792;
  }

.status.idle {
    color: #a1abb4;
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

@keyframes pulse {
    50% {
      opacity: 0.35;
      transform: scale(0.8);
    }
  }

@media (max-height: 560px) and (min-width: 800px) {
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
}

@media (prefers-reduced-motion: reduce) {
  .pulse {
        animation: none;
      }
}

tbody tr {
    transition: background 140ms ease;
  }

.name {
    color: #202a3b;
    font-weight: 700;
  }

.source-name {
    font-size: 0.72rem;
    color: var(--signal-dark);
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

.empty-record i:nth-child(2) {
    inset: 65px;
    background: #fff;
  }

@media (max-width: 1120px) and (min-width: 800px) {
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

@media (max-height: 620px) and (min-width: 800px) {
  .results {
        border-radius: 15px;
        padding-block: 14px;
      }
}

@media (prefers-reduced-motion: reduce) {
  tbody tr {
        animation: none;
        transition: none;
      }
}

.heading > div > span {
    color: var(--signal-dark);
    font: 700 0.68rem/1 'Microsoft YaHei UI', sans-serif;
    letter-spacing: 0.08em;
  }

button:focus-visible {
    border-color: var(--signal);
    outline: none;
    box-shadow: 0 0 0 3px #168be821;
  }

button {
    cursor: pointer;
    border: 0;
    font: inherit;
    font-weight: 700;
    border-radius: 8px;
  }

.check {
    flex: 0 0 auto;
    width: 15px;
    height: 15px;
    accent-color: var(--signal);
  }

.results {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    border: 0;
    border-radius: 0;
    background: #fff;
    padding: 24px clamp(20px, 2.4vw, 32px) 18px;
    box-shadow: none;
  }

.heading {
    display: flex;
    flex: 0 0 auto;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 17px;
  }

.heading h2 {
    margin: 5px 0 0;
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
    display: grid;
    place-items: center;
    align-content: center;
    gap: 8px;
    min-height: 220px;
    height: calc(100% - 76px);
    border: 1px solid #e0e9f0;
    border-radius: 12px;
    background: #f8fbfd;
    color: #71808e;
  }

.message button {
    padding: 8px 18px;
    color: #fff;
    border-radius: 999px;
    background: var(--action);
  }

.message.error {
    border: 1px solid #f0cbc6;
    border-color: #efcbc6;
    background: #fff8f7;
  }

.pulse {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    animation: pulse 1.1s ease-in-out infinite;
    background: var(--signal);
  }

.toolbar {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    padding: 0 13px;
    min-height: 40px;
    border: 1px solid var(--line);
    border-bottom: 0;
    border-radius: 12px 12px 0 0;
    background: #f8fbfd;
    color: #6f7e8d;
    font-size: 0.76rem;
  }

.toolbar button {
    background: transparent;
    color: var(--signal-dark);
  }

.partial-warning {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    border: 1px solid #dfc47e;
    border-bottom: 0;
    padding: 5px 12px;
    color: #7d6428;
    font-size: 0.72rem;
    border-color: #ead7a5;
    background: #fffaf0;
  }

.table-wrap {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--line);
    scrollbar-width: thin;
    border-color: var(--line);
    border-radius: 0 0 12px 12px;
    background: #fff;
    scrollbar-color: #bdcad5 transparent;
  }

table {
    width: 100%;
    border-collapse: collapse;
    min-width: 710px;
    table-layout: fixed;
    font-size: 0.78rem;
  }

th {
    position: sticky;
    top: 0;
    text-align: left;
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
    overflow: hidden;
    border-top: 1px solid #e1e5df;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    font-family: ui-monospace, monospace;
    color: #536575;
  }

.index {
    color: #93a0ac;
    font-variant-numeric: tabular-nums;
    text-overflow: clip;
  }

.status-cell {
    overflow: visible;
    min-width: 0;
  }

.status {
    font-weight: 700;
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
    padding: 3px 7px;
    font-size: 0.64rem;
    border-radius: 999px;
    background: #e3f2fc;
    color: var(--signal-dark);
  }

.empty-signal {
    display: grid;
    place-items: center;
    align-content: center;
    flex: 1 1 auto;
    min-height: 220px;
    background-size: 32px 32px;
    gap: 9px;
    background-color: #f8f9fc;
    background-image: none;
    border: 1px solid #e0e9f0;
    border-radius: 12px;
    background: #f8fbfd;
    color: #7b8996;
  }

.empty-signal strong {
    margin-top: 8px;
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: #273746;
    font-size: 1.1rem;
  }

.empty-signal > span {
    font-size: 0.76rem;
  }

.empty-record {
    position: relative;
    width: 142px;
    height: 142px;
    margin-bottom: 4px;
    border: 1px solid #dfe2ee;
    border-radius: 50%;
    border-color: #d6e8f5;
    background: #edf8ff;
    box-shadow: 0 20px 46px #168be812;
  }

.empty-record::before,
.empty-record::after {
    border-color: #c5e1f3;
  }

.empty-record i:first-child {
    inset: 50px;
    background: var(--signal);
    box-shadow: 0 0 0 10px #168be810;
  }

.empty-record i:nth-child(3) {
    right: 14px;
    top: 22px;
    width: 10px;
    height: 10px;
    background: #64d2af;
    box-shadow: 0 0 0 7px #64d2af18;
  }

.empty-record b {
    right: 22px;
    bottom: 22px;
    width: 38px;
    height: 2px;
    border-radius: 2px;
    transform: rotate(-36deg);
    transform-origin: right;
    background: #7fbce4;
  }

@media (max-width: 1120px) and (min-width: 800px) {
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

@media (max-height: 850px) and (min-width: 800px) {
  .results {
        padding-top: 22px;
      }
}

@media (max-height: 620px) and (min-width: 800px) {
  .results {
        border-radius: 0;
        padding-block: 13px;
      }
}

@media (prefers-reduced-motion: reduce) {
  tbody tr {
        transition: none;
      }
}
</style>
