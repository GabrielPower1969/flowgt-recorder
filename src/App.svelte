<script lang="ts">
  import { onMount } from "svelte";
  import { bindRouter, getRoute, navigate } from "./lib/router.svelte";
  import { initPlatform } from "./lib/platform.svelte";
  import { loadSettings, getSettings, isLoaded } from "./lib/stores/settings.svelte";
  import { bindRecorderEvents } from "./lib/stores/recorder.svelte";
  import { t } from "./lib/stores/i18n.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import Onboarding from "./routes/Onboarding.svelte";
  import Main from "./routes/Main.svelte";
  import Settings from "./routes/Settings.svelte";
  import Summary from "./routes/Summary.svelte";
  import Floating from "./routes/Floating.svelte";

  const route = $derived(getRoute());
  const isFloating = $derived(route === "/floating");

  onMount(() => {
    const unbindRouter = bindRouter();
    document.body.dataset.window = getRoute() === "/floating" ? "floating" : "main";
    void initPlatform();
    const unbindEvents = bindRecorderEvents((r) => {
      if (getRoute() !== "/floating" && r) navigate("/summary");
    });
    void loadSettings().then((s) => {
      if (!s.onboardingDone && getRoute() === "/") navigate("/onboarding");
    });
    return () => {
      unbindRouter();
      unbindEvents();
    };
  });
</script>

{#if isFloating}
  <Floating />
{:else if !isLoaded()}
  <div class="page muted">{t("common.loading")}</div>
{:else if route === "/onboarding"}
  <Onboarding />
{:else if route === "/settings"}
  <Settings />
{:else if route === "/summary"}
  <Summary />
{:else}
  <Main settings={getSettings()} />
{/if}
{#if !isFloating}<Toast />{/if}
