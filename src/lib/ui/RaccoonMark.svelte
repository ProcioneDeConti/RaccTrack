<script lang="ts">
  // Raccoon head — rounded-triangle ears, cheeks tapering to a pointed snout,
  // the pale bandit mask + dark eyes + snub nose cut from the silhouette.
  // Single `currentColor` fill; the cut-outs read as the background showing
  // through, so it works on any panel colour.
  export let size: number | string = 120;
  export let title = "RaccTrack";

  const uid = "rm-" + Math.random().toString(36).slice(2, 8);
</script>

<svg
  class="rm"
  viewBox="0 0 120 116"
  width={size}
  height={size}
  role="img"
  aria-label={title}
>
  <defs>
    <mask id={uid}>
      <rect width="120" height="116" fill="#fff" />
      <!-- bandit mask: band across the eyes, slight dip over the nose -->
      <path
        d="M28 46 Q28 39 37 38 Q50 36 58 45 Q60 47 62 45 Q70 36 83 38
           Q92 39 92 46 Q92 60 80 62 Q66 63 60 53 Q54 63 40 62 Q28 60 28 46 Z"
        fill="#000"
      />
      <circle cx="45" cy="50" r="4.4" fill="#fff" />
      <circle cx="75" cy="50" r="4.4" fill="#fff" />
      <path d="M55 79 h10 l-3.5 7 a3.2 3.2 0 0 1 -3 0 z" fill="#000" />
    </mask>
  </defs>
  <g fill="currentColor" mask="url(#{uid})">
    <!-- ears -->
    <path d="M18 6 C24 20 32 30 44 34 C45 24 47 16 51 10 C41 3 28 0 18 6 Z" />
    <path d="M102 6 C96 20 88 30 76 34 C75 24 73 16 69 10 C79 3 92 0 102 6 Z" />
    <!-- head: wide cheeks tapering to a pointed snout -->
    <path
      d="M60 12 C42 12 31 18 25 30 C18 42 16 55 20 67 C24 81 34 95 48 104
         C52 107 56 109 60 109 C64 109 68 107 72 104 C86 95 96 81 100 67
         C104 55 102 42 95 30 C89 18 78 12 60 12 Z"
    />
  </g>
</svg>

<style>
  .rm {
    display: block;
    flex: 0 0 auto;
  }
</style>
