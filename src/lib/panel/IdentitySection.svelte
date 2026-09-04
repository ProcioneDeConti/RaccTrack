<script lang="ts">
  import type { AircraftDetail, Aircraft } from "../api/types";

  export let detail: AircraftDetail | null;
  export let live: Aircraft | null;
  export let hex: string | null;

  $: td = detail?.typeDetails ?? null;
  $: engineLine =
    td && td.engines && td.engType
      ? `${td.engines} × ${td.engType.toLowerCase()}`
      : null;
  $: builtYear = detail?.built ? parseInt(detail.built, 10) : NaN;
  $: builtLine = Number.isFinite(builtYear)
    ? `${builtYear} (${new Date().getFullYear() - builtYear} yr)`
    : null;
</script>

<section class="panel-section">
  <h4>Identity</h4>
  <dl class="kv">
    <dt>Registration</dt>
    <dd>{detail?.aircraft.registration ?? live?.registration ?? "—"}</dd>
    <dt>Type</dt>
    <dd>
      {detail?.aircraft.typeCode ?? live?.typeCode ?? "—"}
      {#if detail?.aircraft.description}— {detail.aircraft.description}{/if}
    </dd>
    {#if engineLine}
      <dt>Engines</dt><dd>{engineLine}</dd>
    {/if}
    {#if td?.wtc}
      <dt>Wake category</dt><dd>{td.wtc}</dd>
    {/if}
    {#if builtLine}
      <dt>Built</dt><dd>{builtLine}</dd>
    {/if}
    <dt>Operator</dt>
    <dd>
      {#if detail?.operator}
        {detail.operator.name}
        {#if detail.operator.telephony}
          <span class="muted">· “{detail.operator.telephony}”</span>
        {/if}
      {:else}
        {detail?.ownerOperator ?? "—"}
      {/if}
    </dd>
    {#if detail?.operator && detail.ownerOperator && detail.ownerOperator !== detail.operator.name}
      <dt>Registered to</dt><dd>{detail.ownerOperator}</dd>
    {/if}
    <dt>Country</dt><dd>{detail?.country ?? "—"}</dd>
    <dt>ICAO hex</dt><dd class="mono">{hex}</dd>
  </dl>
</section>

<style>
  .muted {
    color: var(--text-dim);
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: var(--text-dim);
  }
</style>
