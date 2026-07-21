<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { confirm } from '@tauri-apps/plugin-dialog';
  import {
    LogLevel,
    attachLogger,
    error as logError,
    info as logInfo,
  } from '@tauri-apps/plugin-log';
  import { relaunch } from '@tauri-apps/plugin-process';
  import {
    check,
    type DownloadEvent,
    type Update,
  } from '@tauri-apps/plugin-updater';
  import { onMount, tick } from 'svelte';
  import { errorText } from '../common/error';

  type UpdateState =
    | 'idle'
    | 'checking'
    | 'current'
    | 'downloading'
    | 'ready'
    | 'installing'
    | 'error';

  interface RuntimeLog {
    id: number;
    time: string;
    level: LogLevel;
    message: string;
  }
  interface AppActivityStatus {
    musicDownloadActive: boolean;
    updateDownloadActive: boolean;
  }

  const MAX_LOGS = 2_000;
  let logs: RuntimeLog[] = [];
  let nextLogId = 0;
  let logDialog: HTMLDialogElement;
  let logViewport: HTMLDivElement;
  let logSearch = '';
  let logLevel = 'all';
  let autoScroll = true;
  let copied = false;
  let runtimeError = '';
  let version = '';
  let updateState: UpdateState = 'idle';
  let update: Update | null = null;
  let updateBytes = 0;
  let updateTotal: number | null = null;
  let updateMessage = '';
  let updateDismissed = false;

  const levelName = (level: LogLevel) =>
    ({
      [LogLevel.Trace]: 'TRACE',
      [LogLevel.Debug]: 'DEBUG',
      [LogLevel.Info]: 'INFO',
      [LogLevel.Warn]: 'WARN',
      [LogLevel.Error]: 'ERROR',
    })[level] ?? 'INFO';

  function filterLogs(
    entries: RuntimeLog[],
    selectedLevel: string,
    searchText: string,
  ) {
    const query = searchText.trim().toLocaleLowerCase();
    return entries.filter(
      (entry) =>
        (selectedLevel === 'all' ||
          levelName(entry.level) === selectedLevel) &&
        (!query || entry.message.toLocaleLowerCase().includes(query)),
    );
  }

  let visibleLogs: RuntimeLog[] = [];
  $: visibleLogs = filterLogs(logs, logLevel, logSearch);

  function formatBytes(value: number) {
    if (value < 1024 * 1024) return `${(value / 1024).toFixed(0)} KiB`;
    return `${(value / 1024 / 1024).toFixed(1)} MiB`;
  }

  async function scrollLogs() {
    if (!autoScroll || !logDialog?.open) return;
    await tick();
    logViewport?.scrollTo({ top: logViewport.scrollHeight });
  }

  function appendLog(level: LogLevel, message: string) {
    const now = new Date();
    logs = [
      ...logs,
      {
        id: ++nextLogId,
        time: now.toLocaleTimeString('zh-CN', { hour12: false }),
        level,
        message,
      },
    ].slice(-MAX_LOGS);
    void scrollLogs();
  }

  async function checkForUpdate() {
    if (updateState === 'checking' || updateState === 'downloading') return;
    updateState = 'checking';
    updateMessage = '';
    updateDismissed = false;
    try {
      update?.close().catch(() => undefined);
      update = await check({ timeout: 15_000 });
      if (!update) {
        updateState = 'current';
        await logInfo(`当前已是最新版本 ${version}`);
        return;
      }
      updateState = 'downloading';
      updateBytes = 0;
      updateTotal = null;
      await invoke('app_set_update_active', { active: true });
      await logInfo(`发现新版本 ${update.version}，开始下载更新`);
      await update.download((event: DownloadEvent) => {
        if (event.event === 'Started') {
          updateTotal = event.data.contentLength ?? null;
        } else if (event.event === 'Progress') {
          updateBytes += event.data.chunkLength;
        }
      });
      await invoke('app_set_update_active', { active: false });
      updateState = 'ready';
      await logInfo(`版本 ${update.version} 已下载并通过签名验证`);
    } catch (error) {
      await invoke('app_set_update_active', { active: false }).catch(
        () => undefined,
      );
      updateState = 'error';
      updateMessage = '暂时无法检查或下载更新，不影响搜索和下载。';
      await logError(`更新失败：${errorText(error)}`);
    }
  }

  async function installUpdate() {
    if (!update || updateState !== 'ready') return;
    updateMessage = '';
    try {
      const activity = await invoke<AppActivityStatus>(
        'app_get_activity_status',
      );
      let cancelMusicDownloads = false;
      if (activity.musicDownloadActive) {
        cancelMusicDownloads = await confirm(
          '仍有歌曲正在下载。现在安装更新会取消剩余歌曲，并等待临时文件清理完成。',
          {
            title: '安装音觅更新',
            kind: 'warning',
            okLabel: '取消下载并安装',
            cancelLabel: '继续下载歌曲',
          },
        );
        if (!cancelMusicDownloads) return;
      }
      updateState = 'installing';
      await invoke('app_prepare_restart', { cancelMusicDownloads });
      await update.install();
      await relaunch();
    } catch (error) {
      await invoke('app_cancel_exit').catch(() => undefined);
      updateState = 'error';
      updateMessage = `更新安装失败：${errorText(error)}`;
      await logError(updateMessage);
    }
  }

  async function openLogDirectory() {
    runtimeError = '';
    try {
      await invoke('app_open_log_directory');
    } catch (error) {
      runtimeError = errorText(error);
    }
  }

  async function copyLogs() {
    try {
      await navigator.clipboard.writeText(
        visibleLogs
          .map(
            (entry) =>
              `${entry.time} ${levelName(entry.level)} ${entry.message}`,
          )
          .join('\n'),
      );
      copied = true;
      setTimeout(() => (copied = false), 1_500);
    } catch (error) {
      runtimeError = `复制失败：${errorText(error)}`;
    }
  }

  async function openLogs() {
    runtimeError = '';
    logDialog.showModal();
    await scrollLogs();
  }

  onMount(() => {
    let disposed = false;
    let detach: (() => void) | undefined;
    let detachExit: (() => void) | undefined;
    void getVersion().then((value) => (version = value));
    void attachLogger((entry) => appendLog(entry.level, entry.message)).then(
      (stop) => {
        if (disposed) stop();
        else detach = stop;
      },
    );
    void logInfo('音觅界面已启动');
    void listen('app-exit-blocked', async () => {
      const activity = await invoke<AppActivityStatus>(
        'app_get_activity_status',
      ).catch(() => ({
        musicDownloadActive: true,
        updateDownloadActive: updateState === 'downloading',
      }));
      const accepted = await confirm(
        activity.updateDownloadActive
          ? '更新包仍在下载。确认退出后会取消剩余歌曲，但会等待更新下载进入安全状态再退出。'
          : '仍有歌曲正在下载。退出会取消剩余歌曲，并等待临时文件清理完成。',
        {
          title: '退出音觅',
          kind: 'warning',
          okLabel: '取消任务并退出',
          cancelLabel: '继续下载',
        },
      );
      try {
        await invoke(accepted ? 'app_confirm_exit' : 'app_cancel_exit');
      } catch (error) {
        runtimeError = errorText(error);
        await logError(`退出失败：${runtimeError}`);
      }
    }).then((stop) => {
      if (disposed) stop();
      else detachExit = stop;
    });
    const timer = setTimeout(() => void checkForUpdate(), 900);
    return () => {
      disposed = true;
      clearTimeout(timer);
      detach?.();
      detachExit?.();
      update?.close().catch(() => undefined);
    };
  });
</script>

<div class="runtime-tools">
  {#if !updateDismissed && updateState === 'downloading' && update}
    <span class="update-status" aria-live="polite">
      正在下载 {update.version} · {formatBytes(updateBytes)}{updateTotal
        ? ` / ${formatBytes(updateTotal)}`
        : ''}
    </span>
  {:else if !updateDismissed && updateState === 'ready' && update}
    <span class="update-status ready">新版本 {update.version} 已就绪</span>
    <button
      class="update-action"
      type="button"
      onclick={() => void installUpdate()}>重启安装</button
    >
    <button class="quiet" type="button" onclick={() => (updateDismissed = true)}
      >稍后</button
    >
  {:else if !updateDismissed && updateState === 'installing'}
    <span class="update-status" aria-live="polite">正在准备安装更新…</span>
  {:else if !updateDismissed && updateState === 'error'}
    <button
      class="update-error"
      type="button"
      title={updateMessage}
      onclick={() => void checkForUpdate()}>更新检查失败 · 重试</button
    >
  {/if}
  <button class="logs-button" type="button" onclick={() => void openLogs()}
    >运行日志</button
  >
</div>

<dialog bind:this={logDialog} class="log-drawer" aria-labelledby="log-title">
  <header>
    <div>
      <span>RUNTIME LOG</span>
      <h2 id="log-title">运行日志</h2>
    </div>
    <button
      class="close"
      type="button"
      aria-label="关闭运行日志"
      onclick={() => logDialog.close()}>×</button
    >
  </header>
  <div class="log-controls">
    <label>
      <span>级别</span>
      <select bind:value={logLevel} aria-label="日志级别">
        <option value="all">全部</option>
        <option value="ERROR">错误</option>
        <option value="WARN">警告</option>
        <option value="INFO">信息</option>
        <option value="DEBUG">调试</option>
        <option value="TRACE">跟踪</option>
      </select>
    </label>
    <label class="search">
      <span>搜索</span>
      <input bind:value={logSearch} placeholder="筛选日志内容" />
    </label>
    <label class="auto"
      ><input type="checkbox" bind:checked={autoScroll} />自动滚动</label
    >
  </div>
  <div class="log-viewport" bind:this={logViewport} aria-live="polite">
    {#if visibleLogs.length === 0}
      <p class="empty">当前筛选条件下没有日志。</p>
    {:else}
      {#each visibleLogs as entry (entry.id)}
        <div
          data-level={levelName(entry.level)}
          class:error={entry.level === LogLevel.Error}
          class:warn={entry.level === LogLevel.Warn}
        >
          <time>{entry.time}</time><b>{levelName(entry.level)}</b><span
            >{entry.message}</span
          >
        </div>
      {/each}
    {/if}
  </div>
  {#if runtimeError}<p class="runtime-error" role="alert">
      {runtimeError}
    </p>{/if}
  <footer>
    <span
      >显示 {visibleLogs.length} / {logs.length} 条，内存最多保留 {MAX_LOGS} 条</span
    >
    <button type="button" onclick={() => void checkForUpdate()}>检查更新</button
    >
    <button type="button" onclick={() => void openLogDirectory()}
      >打开日志目录</button
    >
    <button type="button" onclick={() => void copyLogs()}
      >{copied ? '已复制' : '复制全部'}</button
    >
  </footer>
</dialog>

<style>
  .runtime-tools {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    height: 32px;
    min-width: 0;
    margin-left: auto;
  }

  button {
    cursor: pointer;
    border: 0;
    border-radius: var(--yinmi-radius-sm);
    font: inherit;
    font-weight: 700;
  }

  .log-controls label {
    display: grid;
    gap: 5px;
  }

  .log-controls .auto {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    color: var(--yinmi-text-secondary);
    font-size: 0.72rem;
    white-space: nowrap;
  }

  .runtime-error {
    margin: 0;
    background: var(--yinmi-error-surface);
    padding: 9px 18px;
    color: var(--yinmi-error-foreground);
    font-size: 0.75rem;
  }

  .log-drawer footer span {
    margin-right: auto;
    color: var(--yinmi-text-secondary);
    font-size: 0.68rem;
  }

  @media (max-width: 900px) {
    .update-status,
    .update-error {
      display: none;
    }
  }

  .log-drawer h2 {
    margin: 2px 0 0;
    font-size: 1.25rem;
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
  }

  .logs-button,
  .quiet,
  .update-action,
  .update-error {
    padding: 0 12px;
    white-space: nowrap;
    min-height: 32px;
    border-radius: var(--yinmi-radius-pill);
    font-size: 0.72rem;
  }

  .logs-button {
    border: 1px solid var(--yinmi-border);
    background: var(--yinmi-surface-muted);
    color: var(--yinmi-primary-hover);
  }

  .logs-button:hover {
    border-color: var(--yinmi-primary-soft-hover);
    background: var(--yinmi-primary-soft);
  }

  .update-action {
    background: var(--yinmi-primary-hover);
    color: #fff;
  }

  .update-action:hover {
    background: var(--yinmi-primary-pressed);
  }

  .quiet {
    background: transparent;
    color: var(--yinmi-text-secondary);
  }

  .update-status {
    overflow: hidden;
    max-width: 320px;
    font-size: 0.72rem;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--yinmi-text-secondary);
  }

  .update-status.ready {
    font-weight: 700;
    color: var(--yinmi-primary-hover);
  }

  .update-error {
    border: 1px solid color-mix(in srgb, var(--yinmi-error) 24%, white);
    background: var(--yinmi-error-surface);
    color: var(--yinmi-error-foreground);
  }

  .log-drawer {
    padding: 0;
    width: min(730px, calc(100vw - 32px));
    height: min(740px, calc(100vh - 32px));
    margin: auto 16px auto auto;
    overflow: hidden;
    border: 1px solid var(--yinmi-border);
    border-radius: var(--yinmi-radius-lg);
    background: var(--yinmi-surface);
    color: var(--yinmi-text);
    box-shadow: var(--yinmi-shadow-overlay);
  }

  .log-drawer::backdrop {
    background: rgba(29, 29, 31, 0.24);
    backdrop-filter: blur(2px);
  }

  .log-drawer[open] {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr) auto auto;
    animation: log-drawer-in 250ms var(--yinmi-ease);
  }

  .log-drawer header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--yinmi-border);
    background: var(--yinmi-surface);
    padding: 20px 24px 18px;
  }

  .log-drawer header span,
  .log-controls span {
    font-size: 0.65rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    font-family: 'Microsoft YaHei UI', sans-serif;
    color: var(--yinmi-primary-hover);
  }

  .close {
    width: 38px;
    height: 38px;
    position: relative;
    display: grid;
    flex: 0 0 38px;
    place-items: center;
    padding: 0;
    border: 1px solid var(--yinmi-border);
    border-radius: 50%;
    background: var(--yinmi-surface-muted);
    color: transparent;
    font-size: 0;
  }

  .close::before,
  .close::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 50%;
    width: 12px;
    height: 1.6px;
    border-radius: 999px;
    background: var(--yinmi-text-secondary);
    transform: translate(-50%, -50%) rotate(45deg);
  }

  .close::after {
    transform: translate(-50%, -50%) rotate(-45deg);
  }

  .close:hover {
    border-color: var(--yinmi-primary-soft-hover);
    background: var(--yinmi-primary-soft);
  }

  .log-controls {
    display: grid;
    grid-template-columns: 120px 1fr auto;
    align-items: end;
    gap: 10px;
    border-bottom: 1px solid var(--yinmi-border);
    background: var(--yinmi-surface-muted);
    padding: 12px 20px 13px;
  }

  .log-controls select,
  .log-controls input {
    width: 100%;
    height: 36px;
    border: 1px solid var(--yinmi-border-strong);
    padding: 0 9px;
    border-radius: var(--yinmi-radius-sm);
    background: var(--yinmi-surface);
    color: var(--yinmi-text);
  }

  .log-controls select {
    appearance: none;
    background-image: url('./icons/chevron-down.svg');
    background-repeat: no-repeat;
    background-position: right 12px center;
    background-size: 14px 14px;
    padding-right: 36px;
  }

  .log-controls .auto input {
    width: 15px;
    height: 15px;
    accent-color: var(--yinmi-primary);
  }

  .log-viewport {
    overflow: auto;
    padding: 12px 0;
    font: 12px/1.55 ui-monospace, 'Cascadia Code', Consolas, monospace;
    background: var(--yinmi-surface-muted);
    color: var(--yinmi-text);
    scrollbar-color: #b9cad7 transparent;
  }

  .log-viewport > div {
    display: grid;
    grid-template-columns: 76px 54px minmax(0, 1fr);
    gap: 8px;
    position: relative;
    border-bottom: 1px solid var(--yinmi-border);
    padding: 5px 20px 5px 22px;
  }

  .log-viewport > div::before {
    content: '';
    position: absolute;
    top: 6px;
    bottom: 6px;
    left: 9px;
    width: 2px;
    border-radius: 999px;
    background: var(--yinmi-border-strong);
  }

  .log-viewport > div:hover {
    background: var(--yinmi-primary-soft);
  }

  .log-viewport time {
    color: var(--yinmi-text-muted);
  }

  .log-viewport b {
    color: var(--yinmi-primary-hover);
  }

  .log-viewport span {
    overflow-wrap: anywhere;
    white-space: pre-wrap;
    color: var(--yinmi-text);
  }

  .log-viewport [data-level='TRACE']::before {
    background: #84929e;
  }

  .log-viewport [data-level='TRACE'] b {
    color: #6f7d88;
  }

  .log-viewport [data-level='DEBUG']::before {
    background: #7868d8;
  }

  .log-viewport [data-level='DEBUG'] b {
    color: #6555c6;
  }

  .log-viewport [data-level='INFO']::before {
    background: var(--yinmi-primary);
  }

  .log-viewport [data-level='INFO'] b {
    color: var(--yinmi-primary-hover);
  }

  .log-viewport .warn {
    background: var(--yinmi-warning-surface);
  }

  .log-viewport .warn::before {
    background: #d79a2f;
  }

  .log-viewport .warn b {
    color: var(--yinmi-warning);
  }

  .log-viewport .error {
    background: var(--yinmi-error-surface);
  }

  .log-viewport .error::before {
    background: var(--yinmi-error);
  }

  .log-viewport .error b {
    color: var(--yinmi-error-foreground);
  }

  .empty {
    margin: 40px 18px;
    text-align: center;
    color: var(--yinmi-text-muted);
  }

  .log-drawer footer {
    display: flex;
    align-items: center;
    gap: 8px;
    border-top: 1px solid var(--yinmi-border);
    background: var(--yinmi-surface);
    padding: 11px 20px;
  }

  .log-drawer footer button {
    padding: 8px 10px;
    font-size: 0.72rem;
    border-radius: var(--yinmi-radius-pill);
    border: 1px solid var(--yinmi-border);
    background: var(--yinmi-surface-muted);
    color: var(--yinmi-primary-hover);
  }

  .log-drawer footer button:hover {
    border-color: var(--yinmi-primary-soft-hover);
    background: var(--yinmi-primary-soft);
  }

  @keyframes log-drawer-in {
    from {
      opacity: 0;
      transform: translateX(14px) scale(0.992);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .log-drawer[open] {
      animation: none;
    }
  }
</style>
