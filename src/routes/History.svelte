<script lang="ts">
  /** Every recording, on this computer or in Google Drive. Newest first. */
  import { onMount } from "svelte";
  import type { RecordingEntry } from "../lib/types";
  import { errorMessage, invoke, mediaUrl } from "../lib/ipc";
  import { getLocale, t } from "../lib/stores/i18n.svelte";
  import { getPlatform } from "../lib/platform.svelte";
  import { navigate } from "../lib/router.svelte";
  import { formatBytes } from "../lib/format";

  let entries = $state<RecordingEntry[] | null>(null);
  let driveError = $state<string | null>(null);
  let playing = $state<{ name: string; src: string } | null>(null);

  const when = (e: RecordingEntry) => e.modifiedMs ?? (e.driveCreated ? Date.parse(e.driveCreated) : 0);
  const label = (e: RecordingEntry) => {
    const ms = when(e);
    // In the language of the interface, not of the operating system.
    return ms ? new Date(ms).toLocaleString(getLocale() === "zh" ? "zh-CN" : "en-GB", { dateStyle: "medium", timeStyle: "short" }) : "";
  };
  /** The file manager is called by its own name on each system, as on the Summary screen. */
  const revealLabel = $derived(getPlatform() === "windows" ? t("summary.revealWin") : getPlatform() === "macos" ? t("summary.revealMac") : t("summary.revealLinux"));

  async function play(e: RecordingEntry) {
    if (!e.path) return;
    const src = await mediaUrl(e.path);
    if (src) playing = { name: e.name, src };
  }

  onMount(() => {
    void invoke("list_recordings")
      .then((h) => {
        entries = [...h.entries].sort((a, b) => when(b) - when(a));
        driveError = h.driveError;
      })
      .catch((e) => {
        entries = [];
        driveError = errorMessage(e);
      });
  });
</script>

<main class="page wrap">
  <div class="head">
    <h1>{t("history.title")}</h1>
    <button type="button" class="btn" onclick={() => navigate("/")}>{t("common.back")}</button>
  </div>
  {#if driveError}<p class="err" data-testid="history-error">{t("history.driveError", { detail: driveError })}</p>{/if}
  {#if playing}
    <!-- svelte-ignore a11y_media_has_caption -->
    <video class="player" controls autoplay src={playing.src} aria-label={playing.name}></video>
  {/if}
  {#if entries === null}
    <p class="muted" aria-busy="true">{t("common.loading")}</p>
  {:else if entries.length === 0}
    <p class="muted">{t("history.empty")}</p>
  {:else}
    <ul class="list">
      {#each entries as e (e.name)}
        <li class="row-item" data-testid="history-row">
          <div class="info">
            <strong class="name">{e.name}</strong>
            <span class="muted">{label(e)} · {formatBytes(e.bytes)}</span>
          </div>
          <div class="tags">
            {#if e.path}<span class="chip">{t("home.saveLocal")}</span>{/if}
            {#if e.driveUrl}<span class="chip chip-ok">{t("home.saveDrive")}</span>{/if}
          </div>
          <div class="acts">
            {#if e.path}
              <button type="button" class="btn btn-sm" onclick={() => play(e)}>{t("history.play")}</button>
              <button type="button" class="btn btn-sm" onclick={() => e.path && invoke("reveal_in_file_manager", { path: e.path })}>{revealLabel}</button>
            {/if}
            {#if e.driveUrl}
              <button type="button" class="btn btn-sm" onclick={() => e.driveUrl && invoke("open_drive_link", { url: e.driveUrl })}>{t("summary.openDrive")}</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  .wrap { display: flex; flex-direction: column; gap: var(--space-4); max-width: 880px; margin: 0 auto; padding: var(--space-6) var(--space-4); }
  .head { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
  .head h1 { margin: 0; }
  .player { width: 100%; max-height: 50vh; border-radius: var(--radius-l); background: var(--color-surface-3); }
  .err { margin: 0; padding: var(--space-2) var(--space-3); border-radius: var(--radius-m); background: var(--color-warn-soft); color: var(--color-warn); }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-2); }
  .row-item { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-3); padding: var(--space-3); border-radius: var(--radius-m); background: var(--color-surface-2); }
  .info { flex: 1 1 240px; min-width: 0; display: flex; flex-direction: column; gap: var(--space-1); }
  .name { word-break: break-all; }
  .tags, .acts { display: flex; flex-wrap: wrap; gap: var(--space-2); }
</style>
