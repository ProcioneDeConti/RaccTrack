<script lang="ts">
  import Icon from "./Icon.svelte";

  export let title: string;
  export let onClose: (() => void) | null = null;
  export let width = 320;
  /** "left" docks flush against the rail; "right" is the selection side. */
  export let side: "left" | "right" = "left";
  /** Set false when the body manages its own edge-to-edge layout (tables etc.). */
  export let bodyPad = true;
</script>

<aside class="panel {side}" style="--panel-w:{width}px">
  <header>
    <h2>{title}</h2>
    <div class="actions"><slot name="actions" /></div>
    {#if onClose}
      <button class="close" on:click={onClose} aria-label="Close">
        <Icon name="x" size={14} />
      </button>
    {/if}
  </header>
  <div class="body" class:pad={bodyPad}><slot /></div>
</aside>

<style>
  .panel {
    position: absolute;
    top: 14px;
    bottom: 14px;
    width: var(--panel-w);
    max-width: calc(100vw - var(--rail-w) - 40px);
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    z-index: 11;
    box-shadow: var(--shadow-panel);
    overflow: hidden;
  }
  .panel.left {
    left: calc(var(--rail-w) + 10px);
  }
  .panel.right {
    right: 14px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 auto;
    padding: 8px 10px 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  header h2 {
    margin: 0;
    font-size: var(--fs-base);
    font-weight: 600;
  }
  .actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .close {
    border: none;
    background: transparent;
    display: inline-flex;
    align-items: center;
    padding: 2px;
    color: var(--text-dim);
  }
  .close:hover {
    color: var(--text);
  }
  .body {
    flex: 1 1 auto;
    overflow-y: auto;
    min-height: 0;
  }
  .body.pad {
    padding: 10px 12px;
  }
</style>
