<script lang="ts">
  // The app's raccoon-pilot badge, used as branding in the About panel and as
  // the empty-state illustration on the map. (Hand-drawn silhouettes never read
  // as clearly as this does — reuse the real thing.)
  import iconUrl from "../../assets/icon.png";

  export let size: number | string = 120;
  export let title = "RaccTrack";
</script>

<img class="rm" src={iconUrl} alt={title} width={size} height={size} />

<style>
  .rm {
    display: block;
    flex: 0 0 auto;
    object-fit: contain;
  }
</style>
