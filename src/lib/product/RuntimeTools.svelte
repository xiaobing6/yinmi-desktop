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

  const errorText = (value: unknown) =>
    typeof value === 'object' && value !== null && 'message' in value
      ? String((value as { message: unknown }).message)
      : String(value);

  function filteredLogs() {
    const query = logSearch.trim().toLocaleLowerCase();
    return logs.filter(
      (entry) =>
        (logLevel === 'all' || levelName(entry.level) === logLevel) &&
        (!query || entry.message.toLocaleLowerCase().includes(query)),
    );
  }

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
        filteredLogs()
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
    {#if filteredLogs().length === 0}
      <p class="empty">当前筛选条件下没有日志。</p>
    {:else}
      {#each filteredLogs() as entry (entry.id)}
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
      >显示 {filteredLogs().length} / {logs.length} 条，内存最多保留 {MAX_LOGS} 条</span
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
    border-radius: 3px;
    font: inherit;
    font-weight: 700;
  }
  button:focus-visible {
    outline: 3px solid #63dc9142;
    outline-offset: 2px;
  }
  .logs-button,
  .quiet,
  .update-action,
  .update-error {
    min-height: 34px;
    padding: 0 12px;
    white-space: nowrap;
  }
  .logs-button {
    border: 1px solid #4d5a53;
    background: #29302d;
    color: #e1e7e2;
  }
  .update-action {
    background: #63dc91;
    color: #122319;
  }
  .quiet {
    background: transparent;
    color: #a0aca5;
  }
  .update-error {
    background: #5a302c;
    color: #ffc2ba;
  }
  .update-status {
    overflow: hidden;
    max-width: 320px;
    color: #a0aca5;
    font-size: 0.72rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .update-status.ready {
    color: #63dc91;
    font-weight: 700;
  }
  .log-drawer {
    width: min(720px, 92vw);
    height: min(720px, 88vh);
    margin: auto 0 auto auto;
    border: 0;
    border-left: 1px solid #c9d7e3;
    padding: 0;
    background: #f7fafc;
    color: #16283e;
    box-shadow: -18px 0 50px #172b3f38;
  }
  .log-drawer::backdrop {
    background: #13263875;
  }
  .log-drawer[open] {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr) auto auto;
  }
  .log-drawer header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid #d5e0e9;
    background: #fff;
    padding: 20px 22px;
  }
  .log-drawer header span,
  .log-controls span {
    color: #718091;
    font-size: 0.65rem;
    font-weight: 700;
    letter-spacing: 0.1em;
  }
  .log-drawer h2 {
    margin: 2px 0 0;
    font-size: 1.25rem;
  }
  .close {
    width: 38px;
    height: 38px;
    background: #edf3f7;
    color: #35536c;
    font-size: 1.45rem;
  }
  .log-controls {
    display: grid;
    grid-template-columns: 120px 1fr auto;
    align-items: end;
    gap: 10px;
    border-bottom: 1px solid #d7e1e9;
    padding: 12px 18px;
  }
  .log-controls label {
    display: grid;
    gap: 5px;
  }
  .log-controls select,
  .log-controls input {
    width: 100%;
    height: 36px;
    border: 1px solid #b9cbd9;
    border-radius: 4px;
    background: #fff;
    padding: 0 9px;
    color: #16283e;
  }
  .log-controls .auto {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    color: #607487;
    font-size: 0.72rem;
    white-space: nowrap;
  }
  .log-controls .auto input {
    width: 15px;
    height: 15px;
  }
  .log-viewport {
    overflow: auto;
    background: #102131;
    padding: 12px 0;
    color: #d8e4ec;
    font:
      12px/1.55 ui-monospace,
      'Cascadia Code',
      Consolas,
      monospace;
  }
  .log-viewport > div {
    display: grid;
    grid-template-columns: 76px 54px minmax(0, 1fr);
    gap: 8px;
    padding: 3px 16px;
  }
  .log-viewport > div:hover {
    background: #ffffff0a;
  }
  .log-viewport time {
    color: #7990a1;
  }
  .log-viewport b {
    color: #6fb5e7;
  }
  .log-viewport .warn b {
    color: #edc56b;
  }
  .log-viewport .error b {
    color: #ff8e86;
  }
  .log-viewport span {
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .empty {
    margin: 40px 18px;
    color: #8397a7;
    text-align: center;
  }
  .runtime-error {
    margin: 0;
    background: #fff0ee;
    padding: 9px 18px;
    color: #a43e37;
    font-size: 0.75rem;
  }
  .log-drawer footer {
    display: flex;
    align-items: center;
    gap: 8px;
    border-top: 1px solid #d5e0e9;
    background: #fff;
    padding: 12px 18px;
  }
  .log-drawer footer span {
    margin-right: auto;
    color: #718091;
    font-size: 0.68rem;
  }
  .log-drawer footer button {
    background: #e8f0f6;
    padding: 8px 10px;
    color: #31516d;
    font-size: 0.72rem;
  }
  @media (max-width: 900px) {
    .update-status,
    .update-error {
      display: none;
    }
  }
  @media (max-width: 620px) {
    .log-controls {
      grid-template-columns: 100px 1fr;
    }
    .log-controls .auto {
      grid-column: 1/-1;
    }
    .log-drawer footer {
      flex-wrap: wrap;
    }
    .log-drawer footer span {
      flex-basis: 100%;
    }
  }

  .logs-button,
  .quiet,
  .update-action,
  .update-error {
    min-height: 32px;
    border-radius: 999px;
    padding-inline: 13px;
    font-size: 0.68rem;
  }
  button:focus-visible {
    outline-color: #8b7cff52;
  }
  .logs-button {
    border: 1px solid #ffffff18;
    background: #ffffff0a;
    color: #c6cedb;
  }
  .logs-button:hover {
    background: #ffffff12;
    color: #fff;
  }
  .update-action {
    background: #8b7cff;
    color: #fff;
  }
  .quiet {
    color: #929db0;
  }
  .update-status {
    color: #929db0;
  }
  .update-status.ready {
    color: #b9b0ff;
  }
  .update-error {
    background: #673d47;
    color: #ffc5b7;
  }
  .log-drawer {
    width: min(730px, 94vw);
    height: min(740px, 90vh);
    border-left-color: #dfe4ed;
    border-radius: 22px 0 0 22px;
    background: #f6f7fb;
    color: #101827;
    box-shadow: -22px 0 70px #10182733;
  }
  .log-drawer::backdrop {
    background: #10182780;
  }
  .log-drawer header {
    border-bottom-color: #e5e8ef;
    padding: 21px 24px;
  }
  .log-drawer header span,
  .log-controls span {
    color: #8b7cff;
    font-family: 'Microsoft YaHei UI', sans-serif;
  }
  .log-drawer h2 {
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
  }
  .close {
    border-radius: 50%;
    background: #f0f1f6;
    color: #59657a;
  }
  .log-controls {
    border-bottom-color: #e5e8ef;
    padding: 13px 20px;
  }
  .log-controls select,
  .log-controls input {
    border-color: #d8deea;
    border-radius: 9px;
    background: #fff;
    color: #101827;
  }
  .log-viewport {
    background: #101827;
    color: #dfe4ed;
  }
  .log-viewport b {
    color: #a69cff;
  }
  .log-drawer footer {
    border-top-color: #e5e8ef;
    padding: 12px 20px;
  }
  .log-drawer footer button {
    border-radius: 999px;
    background: #eeecff;
    color: #5f53c7;
  }

  /* Blue-and-white application chrome */
  button:focus-visible {
    outline-color: #168be83d;
  }
  .logs-button,
  .quiet,
  .update-action,
  .update-error {
    min-height: 32px;
    border-radius: 8px;
    padding-inline: 12px;
    font-size: 0.72rem;
  }
  .logs-button {
    border: 1px solid #cfe0ed;
    background: #f8fbfd;
    color: #0876d1;
  }
  .logs-button:hover {
    border-color: #b7d7ec;
    background: #edf7fd;
    color: #0876d1;
  }
  .update-action {
    background: #0876d1;
    color: #fff;
  }
  .quiet {
    background: transparent;
    color: #718295;
  }
  .update-status {
    color: #718295;
  }
  .update-status.ready {
    color: #0876d1;
  }
  .update-error {
    border: 1px solid #efc9c5;
    background: #fff6f5;
    color: #bd4e47;
  }
  .log-drawer {
    width: min(730px, calc(100vw - 32px));
    height: min(740px, calc(100vh - 32px));
    margin: auto 16px auto auto;
    overflow: hidden;
    border: 1px solid #d7e5ef;
    border-radius: 18px;
    background: #fff;
    color: #1d1d1f;
    box-shadow:
      0 28px 80px #31556f2b,
      0 4px 18px #168be812;
  }
  .log-drawer::backdrop {
    background: #46677f42;
    backdrop-filter: blur(2px);
  }
  .log-drawer[open] {
    animation: log-drawer-in 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .log-drawer header {
    border-bottom-color: #dce5ee;
    background: #fff;
    padding: 20px 24px 18px;
  }
  .log-drawer header span,
  .log-controls span {
    color: #0876d1;
  }
  .close {
    position: relative;
    display: grid;
    flex: 0 0 38px;
    place-items: center;
    padding: 0;
    border: 1px solid #dce9f2;
    border-radius: 50%;
    background: #f2f8fc;
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
    background: #587084;
    transform: translate(-50%, -50%) rotate(45deg);
  }
  .close::after {
    transform: translate(-50%, -50%) rotate(-45deg);
  }
  .close:hover {
    border-color: #c7dfef;
    background: #e8f4fb;
  }
  .log-controls {
    border-bottom-color: #dce5ee;
    background: #f7fbfe;
    padding: 12px 20px 13px;
  }
  .log-controls select,
  .log-controls input {
    border-color: #cedae5;
    border-radius: 8px;
    background: #fff;
    color: #1d1d1f;
  }
  .log-controls .auto input {
    accent-color: #168be8;
  }
  .log-viewport {
    background: #f8fbfd;
    color: #30475a;
    scrollbar-color: #b9cad7 transparent;
  }
  .log-viewport > div {
    position: relative;
    border-bottom: 1px solid #e8f0f5;
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
    background: #c4d2dc;
  }
  .log-viewport > div:hover {
    background: #eef7fc;
  }
  .log-viewport time {
    color: #8294a3;
  }
  .log-viewport b {
    color: #0876d1;
  }
  .log-viewport span {
    color: #2d4254;
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
    background: #168be8;
  }
  .log-viewport [data-level='INFO'] b {
    color: #0876d1;
  }
  .log-viewport .warn {
    background: #fffaf0;
  }
  .log-viewport .warn::before {
    background: #d79a2f;
  }
  .log-viewport .warn b {
    color: #a66b08;
  }
  .log-viewport .error {
    background: #fff7f6;
  }
  .log-viewport .error::before {
    background: #d75b53;
  }
  .log-viewport .error b {
    color: #c34d46;
  }
  .empty {
    color: #7b8c9a;
  }
  .log-drawer footer {
    border-top-color: #dce5ee;
    background: #fff;
    padding: 11px 20px;
  }
  .log-drawer footer button {
    border-radius: 8px;
    border: 1px solid #d6e6f1;
    background: #f3f9fd;
    color: #0876d1;
  }
  .log-drawer footer button:hover {
    border-color: #bcd9eb;
    background: #eaf5fc;
  }
  @keyframes log-drawer-in {
    from {
      opacity: 0;
      transform: translateX(14px) scale(0.992);
    }
  }
  @media (max-width: 620px) {
    .log-drawer {
      width: calc(100vw - 16px);
      height: calc(100vh - 16px);
      margin-right: 8px;
      border-radius: 15px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .log-drawer[open] {
      animation: none;
    }
  }
</style>
