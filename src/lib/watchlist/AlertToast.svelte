<script lang="ts">
  import { lastAlert } from "./watchStore";
  import { selectedHex } from "../state";
  import Icon from "../ui/Icon.svelte";

  let visible = false;
  let timer: number | undefined;

  const unsub = lastAlert.subscribe((a) => {
    if (!a) return;
    visible = true;
    if (timer) clearTimeout(timer);
    timer = window.setTimeout(() => (visible = false), 8000);
  });
  import { onDestroy } from "svelte";
  onDestroy(() => {
    unsub();
    if (timer) clearTimeout(timer);
  });
</script>

{#if visible && $lastAlert}
  <button
    class="toast"
    class:emg={$lastAlert.emergency}
    on:click={() => {
      selectedHex.set($lastAlert.hex);
      visible = false;
    }}
  >
    <strong>
      {#if $lastAlert.emergency}<Icon name="alert-triangle" size={13} /> Emergency{:else}Watch hit{/if}
    </strong>
    <span>{$lastAlert.hex} — {$lastAlert.reason}</span>
  </button>
{/if}

<style>
  .toast {
    position: absolute;
    bottom: 14px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    border-radius: 8px;
    padding: 8px 14px;
    z-index: 20;
    box-shadow: var(--shadow-pop);
  }
  .toast.emg {
    border-color: var(--emergency);
  }
  .toast span {
    font-size: 12px;
    color: var(--text-dim);
  }
  .toast strong {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
</style>
