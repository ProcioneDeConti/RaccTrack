<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import { disclaimerOpen } from "../state";
  import { updateSettings } from "../api/backend";

  /** True on the first-launch showing (no dismiss-without-acknowledging);
   *  false when reopened later from About, where a plain close is fine. */
  export let firstLaunch = false;

  function acknowledge() {
    disclaimerOpen.set(false);
    void updateSettings({ disclaimerAcknowledged: true });
  }
</script>

<div class="backdrop" role="presentation">
  <div class="modal" role="dialog" aria-label="Safety disclaimer">
    <header>
      <div class="title">
        <Icon name="alert-triangle" size={15} />
        <strong>Safety &amp; Data Disclaimer</strong>
      </div>
      {#if !firstLaunch}
        <button class="close" on:click={acknowledge} aria-label="Close">
          <Icon name="x" size={15} />
        </button>
      {/if}
    </header>

    <div class="body">
      <p class="open-source">
        RaccTrack's source is open and licensed <b>Apache-2.0</b> — nothing here
        overrides those rights (commercial use, modification, distribution,
        derivation). This is a supplemental safety and liability disclaimer
        about the compiled app and its real-time data feeds.
      </p>

      <p class="warn">
        <b
          >THE APPLICATION IS PROVIDED FOR INFORMATIONAL AND ENTERTAINMENT
          PURPOSES ONLY. IT IS STRICTLY PROHIBITED TO USE THIS APPLICATION FOR
          REAL-WORLD NAVIGATION, AIR TRAFFIC CONTROL, AIRCRAFT SEPARATION, OR
          ANY SAFETY-CRITICAL AVIATION DECISIONS.</b
        >
      </p>

      <p>
        <b>Data accuracy is not guaranteed.</b> The app relies on community-driven
        ADS-B feeds, amateur receiver networks, and third-party aggregators, which
        are subject to coverage holes, signal loss, network latency, and
        incomplete or erroneous telemetry. Procione DeConti (Developer) assumes
        no responsibility for the accuracy, completeness, or timeliness of the
        data shown. Never rely on this app for situational awareness while
        piloting an aircraft or operating a drone.
      </p>

      <p>
        You agree not to use the app's real-time data to perform commercial or
        professional flight dispatch/planning, to aid in piloting or
        controlling any aircraft, UAV/drone, or surface vehicle, or as a
        replacement for certified aviation hardware or official ATC
        directives.
      </p>

      <p class="fine">
        You assume all risk in interpreting data drawn largely from
        non-certified, public, and hobbyist sources. To the maximum extent
        permitted by law, Procione DeConti (Developer) and community data
        contributors are not liable for any damages arising from use of the
        app. Full text in
        <a
          href="https://github.com/ProcioneDeConti/RaccTrack/blob/main/DISCLAIMER.md"
          target="_blank" rel="noreferrer">DISCLAIMER.md</a
        >.
      </p>
    </div>

    <footer>
      <button class="ack" on:click={acknowledge}>I Understand</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .modal {
    width: min(460px, 100%);
    max-height: min(600px, 100%);
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--emergency);
  }
  .title {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
  }
  .close {
    border: none;
    background: transparent;
    color: var(--text-dim);
    display: inline-flex;
    padding: 2px;
  }
  .close:hover {
    color: var(--text);
  }
  .body {
    padding: 12px 14px;
    overflow-y: auto;
    font-size: 12px;
    line-height: 1.55;
    color: var(--text);
  }
  .body p {
    margin: 0 0 10px;
  }
  .body p:last-child {
    margin-bottom: 0;
  }
  .open-source {
    color: var(--text-dim);
  }
  .warn {
    color: var(--emergency);
  }
  .fine {
    font-size: 11px;
    color: var(--text-dim);
  }
  .fine a {
    color: var(--accent);
  }
  footer {
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: flex-end;
  }
  .ack {
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: var(--radius-sm);
    padding: 7px 16px;
    font-size: 12px;
    font-weight: 600;
  }
</style>
