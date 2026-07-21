import type {
  DownloadBatchResult,
  DownloadItemResult,
  DownloadProgress,
} from '../music/model';

export type RetryTarget = string | 'all' | null;

export interface QueueStats {
  waiting: number;
  current: number;
  succeeded: number;
  skipped: number;
  failed: number;
  cancelled: number;
  total: number;
}

export function summarizeItems(items: DownloadItemResult[]) {
  const summary = { succeeded: 0, skipped: 0, failed: 0, cancelled: 0 };
  for (const item of items) {
    if (item.state === 'success') summary.succeeded += 1;
    else summary[item.state] += 1;
  }
  return summary;
}

export function buildQueueStats(
  isDownloading: boolean,
  progress: DownloadProgress | null,
  retryTarget: RetryTarget,
  itemsBySong: Map<string, DownloadItemResult>,
  pendingSongIds: Set<string>,
  resultSummary: DownloadBatchResult | null,
  selectedCount: number,
): QueueStats {
  if (isDownloading && progress) {
    const current =
      progress.state !== 'finished' && progress.completed < progress.total
        ? 1
        : 0;
    if (retryTarget !== null) {
      const settledItems = [...itemsBySong.values()].filter(
        (item) => !pendingSongIds.has(item.songId),
      );
      return {
        waiting: Math.max(0, pendingSongIds.size - current),
        current,
        ...summarizeItems(settledItems),
        total: itemsBySong.size,
      };
    }
    return {
      waiting: Math.max(0, progress.total - progress.completed - current),
      current,
      succeeded: progress.succeeded,
      skipped: progress.skipped,
      failed: progress.failed,
      cancelled: progress.cancelled,
      total: progress.total,
    };
  }
  return {
    waiting: 0,
    current: 0,
    succeeded: resultSummary?.succeeded ?? 0,
    skipped: resultSummary?.skipped ?? 0,
    failed: resultSummary?.failed ?? 0,
    cancelled: resultSummary?.cancelled ?? 0,
    total: resultSummary?.total ?? selectedCount,
  };
}
