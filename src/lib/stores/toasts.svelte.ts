/** Transient notices (warnings, errors, confirmations). */
export type ToastKind = "info" | "warn" | "error" | "ok";
export interface Toast { id: number; kind: ToastKind; text: string }

let items = $state<Toast[]>([]);
let next = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function getToasts(): Toast[] {
  return items;
}

export function toast(kind: ToastKind, text: string, ttlMs = 5000): number {
  const id = next++;
  items = [...items, { id, kind, text }];
  if (ttlMs > 0) timers.set(id, setTimeout(() => dismiss(id), ttlMs));
  return id;
}

export function dismiss(id: number): void {
  const t = timers.get(id);
  if (t) clearTimeout(t);
  timers.delete(id);
  items = items.filter((x) => x.id !== id);
}

export function clearToasts(): void {
  for (const t of timers.values()) clearTimeout(t);
  timers.clear();
  items = [];
}
