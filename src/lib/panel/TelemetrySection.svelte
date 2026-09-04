<script lang="ts">
  import type { Aircraft } from "../api/types";
  import Icon from "../ui/Icon.svelte";
  import { altitude, speed, verticalRate, degrees, age, squawkMeaning } from "../format";

  export let live: Aircraft | null;

  $: sqMeaning = squawkMeaning(live?.squawk ?? null);
  $: vr = live?.baroRate ?? live?.geomRate ?? 0;
</script>

<section class="panel-section">
  <h4>Live telemetry</h4>
  <dl class="kv">
    <dt>Altitude (baro)</dt><dd>{altitude(live?.altBaro ?? null)}</dd>
    <dt>Altitude (geom)</dt><dd>{altitude(live?.altGeom ?? null)}</dd>
    <dt>Ground speed</dt><dd>{speed(live?.groundSpeed ?? null)}</dd>
    <dt>IAS / TAS</dt><dd>{speed(live?.ias ?? null)} / {speed(live?.tas ?? null)}</dd>
    <dt>Mach</dt><dd>{live?.mach ?? "—"}</dd>
    <dt>Track</dt><dd>{degrees(live?.track ?? null)}</dd>
    <dt>Heading</dt><dd>{degrees(live?.trueHeading ?? live?.magHeading ?? null)}</dd>
    <dt>Vertical rate</dt>
    <dd class="vr">
      {#if vr > 0}<Icon name="arrow-up" size={11} />
      {:else if vr < 0}<Icon name="arrow-down" size={11} />{/if}
      {verticalRate(live?.baroRate ?? live?.geomRate ?? null)}
    </dd>
    <dt>Squawk</dt>
    <dd>
      {live?.squawk ?? "—"}{#if sqMeaning}
        <span class="muted">— {sqMeaning}</span>{/if}
    </dd>
    <dt>Selected alt</dt><dd>{altitude(live?.navAltitude ?? null)}</dd>
    <dt>On ground</dt><dd>{live?.onGround ? "yes" : "no"}</dd>
    <dt>Position source</dt><dd>{live?.positionSource ?? "—"}</dd>
    <dt>Signal</dt>
    <dd>{live?.rssi ?? "—"} dBFS · {live?.messages ?? "—"} msgs</dd>
    <dt>Last message</dt><dd>{age(live?.seen ?? null)}</dd>
    <dt>Feed</dt><dd>{live?.source ?? "—"}</dd>
  </dl>
</section>

<style>
  .muted {
    color: var(--text-dim);
  }
  dd.vr :global(svg) {
    display: inline-block;
    vertical-align: middle;
    color: var(--text-dim);
  }
</style>
