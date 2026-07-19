<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import { isWindows } from './windowChrome';

  const appWindow = getCurrentWindow();
  let maximized = false;

  async function syncMaximizedState() {
    try {
      maximized = await appWindow.isMaximized();
    } catch {
      maximized = false;
    }
  }

  onMount(() => {
    if (!isWindows) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void syncMaximizedState();
    void appWindow.onResized(() => void syncMaximizedState()).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  function minimizeWindow() {
    void appWindow.minimize();
  }

  async function toggleMaximizedWindow() {
    await appWindow.toggleMaximize();
    await syncMaximizedState();
  }

  function closeWindow() {
    void appWindow.close();
  }
</script>

{#if isWindows}
  <div class="window-controls" aria-label="窗口控制">
    <button type="button" aria-label="最小化" title="最小化" onclick={minimizeWindow}>
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path d="M3 8h10"></path>
      </svg>
    </button>
    <button
      type="button"
      aria-label={maximized ? '还原窗口' : '最大化'}
      title={maximized ? '还原窗口' : '最大化'}
      onclick={() => void toggleMaximizedWindow()}
    >
      {#if maximized}
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M5 3.5h7.5V11M3.5 5H11v7.5H3.5z"></path>
        </svg>
      {:else}
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <rect x="3.5" y="3.5" width="9" height="9" rx="0.8"></rect>
        </svg>
      {/if}
    </button>
    <button
      class="close-window"
      type="button"
      aria-label="关闭"
      title="关闭"
      onclick={closeWindow}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path d="m4 4 8 8M12 4l-8 8"></path>
      </svg>
    </button>
  </div>
{/if}

<style>
  .window-controls {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 2px;
    height: 32px;
    margin-left: 4px;
    user-select: none;
  }
  button {
    display: grid;
    width: 38px;
    height: 32px;
    place-items: center;
    cursor: default;
    border: 0;
    border-radius: 8px;
    background: transparent;
    padding: 0;
    color: #5b6c7a;
  }
  button:hover {
    background: #eaf4fa;
    color: #173c58;
  }
  button:active {
    background: #dcecf7;
  }
  button:focus-visible {
    outline: 2px solid #168be84d;
    outline-offset: -2px;
  }
  .close-window:hover {
    background: #e6534d;
    color: #fff;
  }
  .close-window:active {
    background: #c93f3a;
  }
  svg {
    display: block;
    width: 14px;
    height: 14px;
    overflow: visible;
    fill: none;
    stroke: currentColor;
    stroke-linecap: square;
    stroke-linejoin: miter;
    stroke-width: 1.25;
  }
</style>
