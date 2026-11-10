<script lang="ts">
  import { onMount } from "svelte";
  import { bindRouter, getRoute, navigate } from "./lib/router.svelte";
  import { initPlatform } from "./lib/platform.svelte";
  import { loadSettings, getSettings, isLoaded } from "./lib/stores/settings.svelte";
  import { bindRecorderEvents } from "./lib/stores/recorder.svelte";
  import { bindDriveEvents, refreshDrive } from "./lib/stores/drive.svelte";
  import { t } from "./lib/stores/i18n.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import Main from "./routes/Main.svelte";
  import Settings from "./routes/Settings.svelte";
  import Summary from "./routes/Summary.svelte";
  import Floating from "./routes/Floating.svelte";
  import Marker from "./routes/Marker.svelte";
  import History from "./routes/History.svelte";

  const route = $derived(getRoute());
  const isFloating = $derived(route === "/floating" || route === "/marker");

  onMount(() => {
    const unbindRouter = bindRouter();
    document.body.dataset.window = getRoute() === "/floating" || getRoute() === "/marker" ? "floating" : "main";
    void initPlatform();
    const unbindEvents = bindRecorderEvents((r) => {
      if (getRoute() !== "/floating" && r) navigate("/summary");
    });
    const unbindDrive = bindDriveEvents();
    void refreshDrive();
    void loadSettings();
    return () => {
      unbindRouter();
      unbindEvents();
      unbindDrive();
    };
  });
</script>

{#if route === "/marker"}
  <Marker />
{:else if isFloating}
  <Floating />
{:else if !isLoaded()}
  <div class="page muted">{t("common.loading")}</div>
{:else if route === "/settings"}
  <Settings />
{:else if route === "/summary"}
  <Summary />
{:else if route === "/history"}
  <History />
{:else}
  <Main settings={getSettings()} />
{/if}
{#if !isFloating}<Toast />{/if}
