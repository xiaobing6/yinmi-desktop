<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { exit } from '@tauri-apps/plugin-process';
  import { onMount } from 'svelte';
  import SearchPage from './lib/product/SearchPage.svelte';
  import WindowControls from './lib/product/WindowControls.svelte';
  import { titlebar } from './lib/product/windowChrome';

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
  const errorText = (error: unknown) =>
    typeof error === 'object' && error !== null && 'message' in error
      ? String((error as { message: unknown }).message)
      : String(error);

  async function initializeProduct() {
    starting = true;
    startupError = '';
    startupStages = freshStages();
    try {
      await Promise.all([
        invoke('app_initialize'),
        new Promise((resolve) => window.setTimeout(resolve, 380)),
      ]);
      productReady = true;
    } catch (error) {
      startupError = errorText(error);
      startupStages = startupStages.map((stage) =>
        stage.state === 'working' ? { ...stage, state: 'error' } : stage,
      );
    } finally {
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
    <div class="splash-window-controls"><WindowControls /></div>
    <div class="splash-atmosphere" aria-hidden="true">
      <i class="cloud cloud-one"></i>
      <i class="cloud cloud-two"></i>
      <i class="orbit-line orbit-line-one"></i>
      <i class="orbit-line orbit-line-two"></i>
      <span class="music-bubble bubble-one">♪</span>
      <span class="music-bubble bubble-two">♫</span>
      <span class="music-bubble bubble-three">♪</span>
      <div class="sound-planet">
        <i class="planet-ring ring-one"></i>
        <i class="planet-ring ring-two"></i>
        <i class="planet-core"></i>
        <b></b>
      </div>
    </div>

    <header class="splash-brand" use:titlebar>
      <div class="splash-mark" aria-hidden="true">
        <i></i><i></i><b></b>
      </div>
      <div><strong>音觅</strong><span>YINMI DESKTOP</span></div>
    </header>

    <section class="splash-copy">
      <span class="eyebrow">跨音源音乐搜索与下载</span>
      <h1>音觅</h1>
      <p class="splash-lead">找到想听的，也把喜欢的收藏到本地。</p>

      <div class:error-card={Boolean(startupError)} class="startup-card">
        <div class="startup-status">
          <i aria-hidden="true"></i>
          <div>
            <strong>{startupError ? '启动遇到问题' : '正在准备音乐服务…'}</strong>
            <span>{startupError || (starting ? '正在连接所需组件，请稍候' : '启动未完成')}</span>
          </div>
        </div>
        <ul class="startup-stages" aria-label="启动进度">
          {#each startupStages as stage (stage.id)}
            <li
              class:done={stage.state === 'done'}
              class:error={stage.state === 'error'}
            >
              <i aria-hidden="true"></i><span>{stage.label}</span><small
                >{stage.state === 'done'
                  ? stage.detail || '已就绪'
                  : stage.state === 'working'
                    ? '准备中'
                    : stage.state === 'error'
                      ? '失败'
                      : '等待'}</small
              >
            </li>
          {/each}
        </ul>
        {#if startupError}
          <div class="splash-actions">
            <button type="button" onclick={() => void initializeProduct()}
              >重新尝试</button
            >
            <button
              class="secondary"
              type="button"
              onclick={() => void invoke('app_open_log_directory')}
              >查看日志</button
            >
            <button class="quit" type="button" onclick={() => void exit(1)}
              >退出</button
            >
          </div>
        {/if}
      </div>
    </section>

    <p class="splash-footnote">搜索 · 挑选 · 下载 · 收藏</p>
  </main>
{:else}
  <SearchPage />
{/if}

<style>
  :global(*) {
    box-sizing: border-box;
  }
  main.splash {
    --brand: #168be8;
    --brand-deep: #0876d1;
    --sky: #a8deff;
    --mint: #64d2af;
    --ink: #1d1d1f;
    --muted: #6e7886;
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    overflow: hidden;
    background: #f3faff;
    padding: clamp(26px, 4.5vw, 56px) clamp(30px, 6vw, 82px);
    color: var(--ink);
  }
  .splash-atmosphere {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background:
      radial-gradient(circle at 78% 42%, #d9f1ff 0 19%, transparent 48%),
      radial-gradient(circle at 100% 0, #cceaff 0 8%, transparent 31%);
  }
  .cloud {
    position: absolute;
    border-radius: 50%;
    background: #fff;
    filter: blur(2px);
    opacity: 0.68;
  }
  .cloud::before,
  .cloud::after {
    position: absolute;
    border-radius: inherit;
    background: inherit;
    content: '';
  }
  .cloud-one {
    right: 4%;
    top: 11%;
    width: 210px;
    height: 68px;
  }
  .cloud-one::before {
    left: 35px;
    top: -54px;
    width: 112px;
    height: 112px;
  }
  .cloud-one::after {
    right: 10px;
    top: -28px;
    width: 88px;
    height: 88px;
  }
  .cloud-two {
    right: 30%;
    bottom: 3%;
    width: 180px;
    height: 52px;
    opacity: 0.4;
  }
  .cloud-two::before {
    left: 18px;
    top: -34px;
    width: 78px;
    height: 78px;
  }
  .cloud-two::after {
    right: 18px;
    top: -46px;
    width: 96px;
    height: 96px;
  }
  .orbit-line {
    position: absolute;
    right: -5%;
    top: 52%;
    width: min(68vw, 870px);
    height: min(25vw, 310px);
    border: 2px solid #168be82a;
    border-radius: 50%;
    transform: rotate(-13deg);
  }
  .orbit-line-two {
    right: 3%;
    width: min(55vw, 710px);
    height: min(18vw, 230px);
    border-color: #ffffffb8;
    transform: rotate(8deg);
  }
  .sound-planet {
    position: absolute;
    right: clamp(90px, 14vw, 220px);
    top: 49%;
    width: clamp(245px, 29vw, 420px);
    aspect-ratio: 1;
    border-radius: 50%;
    background: var(--brand);
    box-shadow:
      0 34px 70px #168be82d,
      inset 0 0 0 2px #ffffff42;
    transform: translateY(-50%);
    animation: planet-float 4.8s ease-in-out infinite;
  }
  .sound-planet::before,
  .sound-planet::after,
  .planet-ring,
  .planet-core,
  .sound-planet b {
    position: absolute;
    border-radius: 50%;
    content: '';
  }
  .sound-planet::before {
    inset: 17%;
    border: clamp(18px, 2vw, 30px) solid #f7fbff;
  }
  .sound-planet::after {
    inset: 34%;
    border: clamp(14px, 1.7vw, 24px) solid var(--sky);
  }
  .planet-ring.ring-one {
    inset: -10% -28%;
    border: 3px solid #ffffffd9;
    transform: rotate(-16deg) scaleY(0.35);
  }
  .planet-ring.ring-two {
    inset: -18% -16%;
    border: 2px solid #168be84d;
    transform: rotate(14deg) scaleY(0.45);
  }
  .planet-core {
    inset: 46%;
    z-index: 2;
    background: #f7fbff;
  }
  .sound-planet b {
    z-index: 3;
    right: 1%;
    top: 2%;
    width: 15%;
    aspect-ratio: 1;
    border: 7px solid #f7fbff;
    background: var(--mint);
  }
  .music-bubble {
    position: absolute;
    display: grid;
    place-items: center;
    border: 1px solid #ffffffd9;
    border-radius: 50%;
    background: #ffffff80;
    color: var(--brand);
    box-shadow: 0 14px 36px #168be814;
    font-family: 'Segoe UI Symbol', sans-serif;
    backdrop-filter: blur(10px);
  }
  .bubble-one {
    right: 7%;
    top: 17%;
    width: 74px;
    height: 74px;
    font-size: 1.8rem;
  }
  .bubble-two {
    right: 40%;
    top: 20%;
    width: 48px;
    height: 48px;
    font-size: 1.15rem;
  }
  .bubble-three {
    right: 8%;
    bottom: 12%;
    width: 54px;
    height: 54px;
    font-size: 1.25rem;
  }
  .splash-brand {
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 12px;
    width: max-content;
  }
  .splash-window-controls {
    position: absolute;
    z-index: 10;
    top: 14px;
    right: 18px;
  }
  .splash-brand > div:last-child {
    display: grid;
    gap: 1px;
  }
  .splash-brand strong {
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: 1.1rem;
    letter-spacing: 0.12em;
  }
  .splash-brand span {
    color: #698094;
    font: 600 0.57rem/1.2 'Cascadia Code', Consolas, monospace;
    letter-spacing: 0.1em;
  }
  .splash-mark {
    position: relative;
    width: 44px;
    height: 44px;
    border-radius: 11px;
    background: var(--brand);
    box-shadow: 0 8px 24px #168be828;
  }
  .splash-mark i,
  .splash-mark b {
    position: absolute;
    border-radius: 50%;
  }
  .splash-mark i:first-child {
    inset: 8px;
    border: 3px solid #f7fbff;
  }
  .splash-mark i:nth-child(2) {
    inset: 15px;
    border: 3px solid var(--sky);
  }
  .splash-mark b {
    right: 3px;
    top: 3px;
    width: 10px;
    height: 10px;
    border: 2px solid #fff;
    background: var(--mint);
  }
  .splash-copy {
    z-index: 3;
    width: min(570px, 51vw);
    margin-block: auto;
    padding: 44px 0 28px;
  }
  .eyebrow {
    display: block;
    margin-bottom: 11px;
    color: var(--brand-deep);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.09em;
  }
  main.splash h1 {
    margin: 0;
    font-family: 'Segoe UI Variable Display', 'Microsoft YaHei UI', sans-serif;
    font-size: clamp(3.5rem, 6vw, 5.7rem);
    font-weight: 760;
    letter-spacing: -0.07em;
    line-height: 1;
  }
  .splash-lead {
    margin: 17px 0 27px;
    color: #52677a;
    font-size: clamp(0.88rem, 1.1vw, 1.02rem);
  }
  .startup-card {
    width: min(590px, 100%);
    border: 1px solid #ffffffd9;
    border-radius: 18px;
    background: #ffffffb8;
    padding: 16px 17px;
    box-shadow: 0 18px 50px #168be812;
    backdrop-filter: blur(18px);
  }
  .startup-card.error-card {
    border-color: #ee9c948f;
    background: #fff9f8d9;
  }
  .startup-status {
    display: grid;
    grid-template-columns: 17px minmax(0, 1fr);
    align-items: center;
    gap: 11px;
  }
  .startup-status > i {
    width: 12px;
    height: 12px;
    border: 2px solid var(--brand);
    border-top-color: transparent;
    border-radius: 50%;
    animation: status-spin 0.85s linear infinite;
  }
  .error-card .startup-status > i {
    border-color: #d75b53;
    animation: none;
  }
  .startup-status div {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .startup-status strong {
    font-size: 0.8rem;
  }
  .startup-status span {
    overflow: hidden;
    color: #728293;
    font-size: 0.68rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .startup-stages {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 7px;
    margin: 14px 0 0;
    padding: 0;
    list-style: none;
  }
  .startup-stages li {
    display: grid;
    grid-template-columns: 7px minmax(0, 1fr);
    gap: 3px 7px;
    align-items: center;
    border: 1px solid #dbeaf5;
    border-radius: 9px;
    background: #f8fcffcc;
    padding: 7px 8px;
    color: #41586c;
    font-size: 0.62rem;
  }
  .startup-stages li > i {
    width: 6px;
    height: 6px;
    border: 1px solid #9aabba;
    border-radius: 50%;
  }
  .startup-stages li.done > i {
    border-color: var(--brand);
    background: var(--brand);
  }
  .startup-stages li.error > i {
    border-color: #d75b53;
    background: #d75b53;
  }
  .startup-stages small {
    grid-column: 2;
    overflow: hidden;
    color: #8a99a7;
    font-size: 0.55rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .splash-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-top: 13px;
  }
  .splash-actions button {
    cursor: pointer;
    border: 0;
    border-radius: 9px;
    background: var(--brand-deep);
    padding: 9px 14px;
    color: #fff;
    font: 700 0.68rem/1 'Segoe UI Variable Text', 'Microsoft YaHei UI', sans-serif;
  }
  .splash-actions .secondary,
  .splash-actions .quit {
    border: 1px solid #d6e3ec;
    background: #fff;
    color: #52677a;
  }
  .splash-actions button:focus-visible {
    outline: 3px solid #168be82e;
    outline-offset: 2px;
  }
  .splash-footnote {
    z-index: 3;
    margin: 0;
    color: #8091a0;
    font-size: 0.65rem;
    letter-spacing: 0.1em;
  }
  @keyframes status-spin {
    to {
      transform: rotate(1turn);
    }
  }
  @keyframes planet-float {
    50% {
      transform: translateY(calc(-50% - 8px));
    }
  }
  @media (max-width: 900px) {
    main.splash {
      padding-inline: 36px;
    }
    .splash-copy {
      width: min(540px, 62vw);
    }
    .sound-planet {
      right: -4vw;
      opacity: 0.76;
    }
    .music-bubble {
      opacity: 0.6;
    }
  }
  @media (max-width: 680px) {
    .splash-copy {
      width: 100%;
    }
    .splash-atmosphere {
      opacity: 0.26;
    }
    .sound-planet {
      right: -20vw;
      width: 75vw;
    }
    .startup-stages {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-height: 590px) and (min-width: 681px) {
    main.splash {
      padding-block: 20px;
    }
    .splash-copy {
      padding-block: 18px;
    }
    main.splash h1 {
      font-size: 3rem;
    }
    .splash-lead {
      margin-block: 9px 14px;
    }
    .startup-stages {
      margin-top: 9px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .startup-status > i,
    .sound-planet {
      animation: none;
    }
  }
</style>
