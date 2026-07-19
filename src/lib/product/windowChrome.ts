import { getCurrentWindow } from '@tauri-apps/api/window';

const interactiveElements =
  'button, a, input, select, textarea, summary, [role="button"], [contenteditable="true"]';

export const isWindows = /Windows/i.test(navigator.userAgent);

function handleTitlebarMouseDown(event: MouseEvent) {
  if (!isWindows || event.button !== 0) return;
  const target = event.target;
  if (!(target instanceof Element) || target.closest(interactiveElements)) return;

  const appWindow = getCurrentWindow();
  if (event.detail === 2) void appWindow.toggleMaximize();
  else void appWindow.startDragging();
}

export function titlebar(node: HTMLElement) {
  if (!isWindows) return;
  node.addEventListener('mousedown', handleTitlebarMouseDown);
  return {
    destroy() {
      node.removeEventListener('mousedown', handleTitlebarMouseDown);
    },
  };
}
