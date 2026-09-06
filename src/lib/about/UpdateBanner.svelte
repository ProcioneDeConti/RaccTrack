<script lang="ts">
  import { onDestroy } from "svelte";
  import { updateInfo } from "../state";
  import { openExternal } from "../api/backend";
  import Icon from "../ui/Icon.svelte";

  // Remembers the version the user has dismissed, so the banner doesn't
  // nag every launch — but does come back for the *next* release.
  const DISMISS_KEY = "racctrack:update-dismissed";

  let dismissed = "";
  try {
    dismissed = localStorage.getItem(DISMISS_KEY) ?? "";
  } catch {
    /* private mode / storage disabled — just never persist */
  }

  $: info = $updateInfo;
  $: visible = !!info && info.newer && info.latest !== dismissed;

  function dismiss() {
    if (info) {
      dismissed = info.latest;
      try {
        localStorage.setItem(DISMISS_KEY, info.latest);
      } catch {
        /* ignore */
      }
    }
  }

  function download() {
    if (info) void openExternal(info.assetUrl ?? info.url);
  }
  function whatsNew() {
    if (info) void openExternal(info.url);
  }

  // Nothing to clean up, but keep the lifecycle symmetric with siblings.
  onDestroy(() => {});
</script>

{#if visible && info}
  <div class="banner" role="status">
    <Icon name="arrow-up" size={14} />
    <span class="msg">
      <strong>RaccTrack v{info.latest}</strong> is available — you're on v{info.current}.
    </span>
    <span class="actions">
      <button class="link" on:click={whatsNew}>What's new</button>
      <button class="primary" on:click={download}>Download</button>
      <button class="close" title="Dismiss" on:click={dismiss}>
        <Icon name="x" size={13} />
      </button>
    </span>
  </div>
{/if}

<style>
  .banner {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: min(680px, calc(100% - 24px));
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    border-radius: 8px;
    padding: 8px 10px 8px 12px;
    z-index: 25;
    box-shadow: var(--shadow-pop);
    font-size: 12px;
  }
  .msg {
    color: var(--text-dim);
  }
  .msg strong {
    color: var(--text);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
  }
  button {
    font-size: 11px;
  }
  .link {
    border: none;
    background: transparent;
    color: var(--accent);
    padding: 2px 4px;
  }
  .link:hover {
    text-decoration: underline;
  }
  .primary {
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--bg);
    border-radius: var(--radius-sm);
    padding: 3px 10px;
    font-weight: 600;
  }
  .close {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    padding: 2px;
  }
  .close:hover {
    color: var(--text);
  }
</style>
