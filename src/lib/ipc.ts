/**
 * Typed IPC. Components never touch @tauri-apps/api directly; they call `invoke`/`subscribe`
 * here. Outside Tauri (vitest, plain `npm run dev`) the in-memory mock in ./ipc-mock.ts answers.
 */
import type { Commands, Events } from "./types";
import { tCode } from "./stores/i18n.svelte";

export type CommandName = keyof Commands;
export type CommandArgs<K extends CommandName> = Parameters<Commands[K]>;
export type CommandResult<K extends CommandName> = ReturnType<Commands[K]>;
export type EventName = keyof Events;
export type UnlistenFn = () => void;

export interface Transport {
  invoke<K extends CommandName>(cmd: K, args?: CommandArgs<K>[0]): Promise<CommandResult<K>>;
  listen<K extends EventName>(event: K, cb: (payload: Events[K]) => void): Promise<UnlistenFn>;
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let transportPromise: Promise<Transport> | null = null;

async function loadTauriTransport(): Promise<Transport> {
  const [{ invoke: tauriInvoke }, { listen: tauriListen }] = await Promise.all([
    import("@tauri-apps/api/core"),
    import("@tauri-apps/api/event"),
  ]);
  return {
    invoke: (cmd, args) => tauriInvoke(cmd, args as Record<string, unknown> | undefined),
    listen: (event, cb) => tauriListen(event, (e) => cb(e.payload as never)),
  };
}

function getTransport(): Promise<Transport> {
  if (!transportPromise) {
    transportPromise = isTauri() ? loadTauriTransport() : import("./ipc-mock").then((m) => m.mock);
  }
  return transportPromise;
}

/** Replace the transport (tests). `null` restores auto-detection. */
export function setTransport(t: Transport | null): void {
  transportPromise = t ? Promise.resolve(t) : null;
}

export async function invoke<K extends CommandName>(cmd: K, ...args: CommandArgs<K>): Promise<CommandResult<K>> {
  const transport = await getTransport();
  return transport.invoke(cmd, args[0] as CommandArgs<K>[0]);
}

export async function listen<K extends EventName>(event: K, cb: (payload: Events[K]) => void): Promise<UnlistenFn> {
  const transport = await getTransport();
  return transport.listen(event, cb);
}

/**
 * Synchronous subscribe for `$effect` bodies: returns an unlisten that is safe to call
 * before the underlying listener has been attached.
 */
export function subscribe<K extends EventName>(event: K, cb: (payload: Events[K]) => void): UnlistenFn {
  let disposed = false;
  let unlisten: UnlistenFn | null = null;
  listen(event, cb)
    .then((un) => {
      if (disposed) un();
      else unlisten = un;
    })
    .catch((err) => console.error(`listen(${event}) failed`, err));
  return () => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  };
}

/** URL a <video> can load for a local file; null outside Tauri. */
export async function mediaUrl(path: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { convertFileSrc } = await import("@tauri-apps/api/core");
  return convertFileSrc(path);
}

export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "code" in err && "detail" in err) {
    const { code, detail } = err as { code: string; detail: string };
    const key = code.replace(/^error\./, "");
    return tCode("error", key, detail);
  }
  if (err && typeof err === "object" && "message" in err) return String((err as { message: unknown }).message);
  return String(err);
}
