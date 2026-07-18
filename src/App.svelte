<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { exit } from '@tauri-apps/plugin-process';
  import { onMount } from 'svelte';
  import SearchPage from './lib/product/SearchPage.svelte';

  let productReady = false;
  let startupError = '';
  let starting = true;
  type StartupStageState = 'pending' | 'working' | 'done' | 'error';
  interface StartupProgress {
    id: string;
    label: string;
    state: 'working' | 'done';
    detail: string | null;
  }
  interface StartupStage {
    id: string;
    label: string;
    state: StartupStageState;
    detail: string | null;
  }
  const freshStages = (): StartupStage[] => [
    { id: 'log', label: '运行日志', state: 'pending', detail: null },
    { id: 'webview', label: '系统 WebView', state: 'pending', detail: null },
    { id: 'signature', label: '音乐签名环境', state: 'pending', detail: null },
    { id: 'source', label: '固定音源', state: 'pending', detail: null },
    { id: 'download', label: '下载引擎', state: 'pending', detail: null },
    { id: 'update', label: '应用更新', state: 'pending', detail: null },
  ];
  let startupStages = freshStages();
  let showStartupDetails = false;
  const errorText = (error: unknown) =>
    typeof error === 'object' && error !== null && 'message' in error
      ? String((error as { message: unknown }).message)
      : String(error);

  async function initializeProduct() {
    starting = true;
    startupError = '';
    startupStages = freshStages();
    showStartupDetails = false;
    const detailsTimer = window.setTimeout(
      () => (showStartupDetails = true),
      800,
    );
    try {
      await Promise.all([
        invoke('app_initialize'),
        new Promise((resolve) => window.setTimeout(resolve, 380)),
      ]);
      productReady = true;
    } catch (error) {
      startupError = errorText(error);
      showStartupDetails = true;
      startupStages = startupStages.map((stage) =>
        stage.state === 'working' ? { ...stage, state: 'error' } : stage,
      );
    } finally {
      clearTimeout(detailsTimer);
      starting = false;
    }
  }

  onMount(() => {
    let disposed = false;
    let detach: (() => void) | undefined;
    void listen<StartupProgress>('app-startup-progress', (event) => {
      startupStages = startupStages.map((stage) =>
        stage.id === event.payload.id ? { ...stage, ...event.payload } : stage,
      );
    }).then(
      (stop) => {
        if (disposed) stop();
        else {
          detach = stop;
          void initializeProduct();
        }
      },
      () => void initializeProduct(),
    );
    return () => {
      disposed = true;
      detach?.();
    };
  });
</script>

{#if !productReady}
  <main class="splash" aria-live="polite" aria-label="正在启动音觅">
    <div class="splash-mark" aria-hidden="true"><i></i><i></i><b></b></div>
    <h1>音觅</h1>
    <p>{startupError || (starting ? '正在准备音乐服务…' : '启动未完成')}</p>
    {#if showStartupDetails}
      <ul class="startup-stages" aria-label="启动进度">
        {#each startupStages as stage (stage.id)}
          <li
            class:done={stage.state === 'done'}
            class:error={stage.state === 'error'}
          >
            <i aria-hidden="true"></i><span>{stage.label}</span><small
              >{stage.state === 'done'
                ? stage.detail || '完成'
                : stage.state === 'working'
                  ? '正在初始化'
                  : stage.state === 'error'
                    ? '失败'
                    : '等待'}</small
            >
          </li>
        {/each}
      </ul>
    {/if}
    {#if startupError}
      <div class="splash-actions">
        <button type="button" onclick={() => void initializeProduct()}
          >重试</button
        >
        <button
          type="button"
          onclick={() => void invoke('app_open_log_directory')}
          >打开日志目录</button
        >
        <button class="quit" type="button" onclick={() => void exit(1)}
          >退出</button
        >
      </div>
    {/if}
  </main>
{:else}
  <SearchPage />
{/if}

<style>
  main.splash {
    display: grid;
    place-content: center;
    place-items: center;
    min-height: 100vh;
    background: #f1f3ef;
    color: #171b1a;
  }
  main.splash h1 {
    margin: 16px 0 2px;
    font-family: 'Bahnschrift SemiCondensed', 'Arial Narrow', sans-serif;
    font-size: 1.55rem;
    letter-spacing: 0.16em;
  }
  main.splash p {
    max-width: min(720px, 86vw);
    margin: 0;
    color: #68746e;
    font-size: 0.76rem;
    line-height: 1.6;
    overflow-wrap: anywhere;
    text-align: center;
  }
  .startup-stages {
    display: grid;
    width: min(520px, 84vw);
    margin: 18px 0 0;
    padding: 0;
    list-style: none;
  }
  .startup-stages li {
    display: grid;
    grid-template-columns: 12px 120px minmax(0, 1fr);
    align-items: center;
    gap: 9px;
    border-top: 1px solid #d4d9d3;
    padding: 7px 3px;
    color: #68746e;
    font-size: 0.72rem;
    text-align: left;
  }
  .startup-stages i {
    width: 8px;
    height: 8px;
    border: 2px solid #a9b2aa;
    border-radius: 50%;
  }
  .startup-stages li.done i {
    border-color: #258752;
    background: #258752;
  }
  .startup-stages li.error i {
    border-color: #b63f38;
    background: #b63f38;
  }
  .startup-stages small {
    overflow: hidden;
    color: #7d8781;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .splash-actions {
    display: flex;
    gap: 8px;
    margin-top: 18px;
  }
  .splash-actions button {
    cursor: pointer;
    border: 0;
    border-radius: 3px;
    background: #63dc91;
    padding: 9px 15px;
    color: #122319;
    font:
      700 0.75rem/1 system-ui,
      sans-serif;
  }
  .splash-actions .quit {
    background: #dfe4de;
    color: #39453e;
  }
  .splash-mark {
    position: relative;
    width: 58px;
    height: 58px;
    border: 1px solid #3d4842;
    border-radius: 6px;
    background: #222725;
    box-shadow: 0 12px 30px #171b1a24;
  }
  .splash-mark i,
  .splash-mark b {
    position: absolute;
    border-radius: 50%;
  }
  .splash-mark i {
    border: 1px solid #a9b4ae;
    animation: splash-pulse 0.8s ease-out both;
  }
  .splash-mark i:first-child {
    inset: 11px;
  }
  .splash-mark i:nth-child(2) {
    inset: 21px;
    animation-delay: 0.08s;
  }
  .splash-mark b {
    right: 10px;
    top: 10px;
    width: 10px;
    height: 10px;
    background: #63dc91;
    box-shadow: 0 0 0 4px #63dc911c;
  }
  @keyframes splash-pulse {
    from {
      opacity: 0;
      transform: scale(0.7);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .splash-mark i {
      animation: none;
    }
  }
</style>
