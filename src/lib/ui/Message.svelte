<script lang="ts">
  import Icon from "./Icon.svelte";

  /** loading = spinner, error = red + alert icon, empty = plain dim text. */
  export let kind: "loading" | "empty" | "error" = "empty";
  export let onRetry: (() => void) | null = null;
</script>

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

<style>
  .msg {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 12px;
    font-size: 12px;
    color: var(--text-dim);
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
