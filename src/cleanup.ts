export interface ActionItemResult { path: string; status: 'success' | 'failed' | 'skipped'; message: string }
export interface ActionReport { items: ActionItemResult[]; succeeded: number; failed: number; skipped: number }
export interface FileScanReport<T> { items: T[]; warnings: string[]; roots: string[]; files_scanned: number; cancelled: boolean }
export async function copyPath(path: string) { await navigator.clipboard.writeText(path); }
