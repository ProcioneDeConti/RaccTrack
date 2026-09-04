<script lang="ts">
  import Icon from "./Icon.svelte";
  import RaccoonMark from "./RaccoonMark.svelte";

  /** loading = spinner, error = red + alert icon, empty = raccoon + dim text. */
  export let kind: "loading" | "empty" | "error" = "empty";
  export let onRetry: (() => void) | null = null;
  /** empty variant: show the raccoon mark above the text (panel-level empties). */
  export let mascot = false;
</script>

{#if kind === "empty" && mascot}
  <div class="msg empty stacked">
    <span class="racc"><RaccoonMark size={44} /></span>
    <span class="text"><slot /></span>
  </div>
{:else}
  <div class="msg {kind}">
    {#if kind === "loading"}
      <span class="spin"><Icon name="refresh-cw" size={13} /></span>
    {:else if kind === "error"}
      <Icon name="alert-triangle" size={13} />
    {/if}
    <span class="text"><slot /></span>
    {#if onRetry}
      <button class="retry" on:click={onRetry}>Retry</button>
    {/if}
  </div>
{/if}

<style>
  .msg {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 12px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .msg.stacked {
    flex-direction: column;
    text-align: center;
    gap: 8px;
    padding: 18px 12px;
  }
  .racc {
    color: var(--text-dim);
    opacity: 0.35;
  }
  .msg.error {
    color: var(--emergency);
  }
  .text {
    flex: 1 1 auto;
    min-width: 0;
  }
  .msg :global(svg) {
    flex: 0 0 auto;
  }
  .spin {
    display: inline-flex;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .retry {
    flex: 0 0 auto;
    border: none;
    background: transparent;
    color: var(--accent);
    font-size: 12px;
    padding: 2px 4px;
  }
</style>
