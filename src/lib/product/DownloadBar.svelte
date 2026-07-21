<script lang="ts">
  import {
    downloadProgressPercent,
    formatBytes,
    type DownloadBatchResult,
    type DownloadProgress,
    type SearchResult,
  } from '../music/model';
  import type { QueueStats, RetryTarget } from './downloadView';

  export let selectedCount: number;
  export let searching: boolean;
  export let downloading: boolean;
  export let result: SearchResult | null;
  export let downloadProgress: DownloadProgress | null;
  export let downloadResult: DownloadBatchResult | null;
  export let downloadError: string;
  export let stats: QueueStats;
  export let cancellingScope: 'current' | 'all' | null;
  export let retryingTarget: RetryTarget;
  export let rateLimitSeconds: number;
  export let onCancelCurrent: () => void;
  export let onCancelAll: () => void;
  export let onRetryAll: () => void;
  export let onOpenDirectory: () => void;
  export let onDownloadSelected: () => void;

  let progressPercent: number | null = null;
  $: progressPercent = downloadProgress
    ? downloadProgressPercent(downloadProgress)
    : null;
</script>

<footer aria-label="下载队列">
  <div class="selected-summary">
    <div class="selected-summary-line">
      <span>已选择</span><strong>{selectedCount}</strong><span>首</span>
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
        {progressPercent !== null ? ` · ${progressPercent}%` : ''}
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
      <strong>下载队列已处理完成</strong><span title={downloadResult.directory}
        >{downloadResult.directory}</span
      >
    {/if}
    <div class="queue-stats" aria-label="队列统计" aria-live="polite">
      <span class="waiting">等待 <b>{stats.waiting}</b></span>
      <span class="current">当前 <b>{stats.current}</b></span>
      <span class="succeeded">成功 <b>{stats.succeeded}</b></span>
      <span class="skipped">跳过 <b>{stats.skipped}</b></span>
      <span class="failed">失败 <b>{stats.failed}</b></span>
      <span class="cancelled">取消 <b>{stats.cancelled}</b></span>
      <span class="total">总计 <b>{stats.total}</b></span>
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
        onclick={onCancelCurrent}
        >{cancellingScope === 'current' ? '取消中…' : '取消当前'}</button
      >
      <button
        class="cancel all"
        type="button"
        disabled={cancellingScope !== null}
        onclick={onCancelAll}
        >{cancellingScope === 'all' ? '停止中…' : '取消全部'}</button
      >
    {:else}
      {#if downloadResult && downloadResult.failed + downloadResult.cancelled > 0}<button
          class="retry-all"
          type="button"
          onclick={onRetryAll}>重试未完成项</button
        >{/if}
      {#if downloadResult}<button
          class="open"
          type="button"
          onclick={onOpenDirectory}>打开目录</button
        >{/if}
      <button
        class="download"
        type="button"
        disabled={searching || !result || selectedCount === 0}
        onclick={onDownloadSelected}
        >{`下载所选${selectedCount ? ` ${selectedCount} 首` : ''}`}</button
      >
    {/if}
  </div>
</footer>

<style>
button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

.download-copy {
    display: grid;
    flex: 1 1 auto;
    min-width: 0;
    gap: 2px;
  }

.download-copy span {
    overflow: hidden;
    max-width: 560px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

.footer-actions {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 7px;
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

footer {
    display: flex;
    align-items: center;
    overflow-y: auto;
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
    display: flex;
    flex: 0 0 auto;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #263746;
    font-size: 0.75rem;
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
    flex-wrap: nowrap;
    align-items: flex-end;
    gap: 3px 7px;
    height: 32px;
    margin-top: 0;
    padding-bottom: 5px;
  }

.queue-stats span {
    overflow: visible;
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
    display: block;
    max-width: 620px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #c44f48;
    font-size: 0.72rem;
  }

.open,
.download,
.cancel,
.retry-all {
    white-space: nowrap;
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

@media (max-height: 850px) and (min-width: 800px) {
  footer {
        min-height: 58px;
        padding-block: 6px;
      }
}

</style>
