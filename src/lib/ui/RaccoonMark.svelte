<script lang="ts">
  // Front-facing raccoon head — pointed ears, pale bandit mask (cut from the
  // silhouette), dark eyes, snub nose. Single `currentColor` fill; the mask
  // reads as the background showing through, so it works on any panel colour.
  export let size: number | string = 120;
  export let title = "RaccTrack";

  const uid = "rm-" + Math.random().toString(36).slice(2, 8);
</script>

<svg
  class="rm"
  viewBox="0 0 100 100"
  width={size}
  height={size}
  role="img"
  aria-label={title}
>
  <defs>
    <mask id={uid}>
      <rect width="100" height="100" fill="#fff" />
      <path
        d="M25 39 C20 44 21 54 28 56 C35 58 43 52 45 45 L55 45 C57 52 65 58 72 56 C79 54 80 44 75 39 C68 34 58 37 50 43 C42 37 32 34 25 39 Z"
        fill="#000"
      />
      <circle cx="37" cy="46" r="3.6" fill="#fff" />
      <circle cx="63" cy="46" r="3.6" fill="#fff" />
      <path d="M45.5 73 h9 l-3 6.5 a3 3 0 0 1 -3 0 z" fill="#000" />
    </mask>
  </defs>
  <g fill="currentColor" mask="url(#{uid})">
    <path d="M18 5 C8 10 6 31 16 42 C25 30 31 23 42 19 C36 8 28 3 18 5 Z" />
    <path d="M82 5 C92 10 94 31 84 42 C75 30 69 23 58 19 C64 8 72 3 82 5 Z" />
    <path
      d="M50 11 C28 11 16 30 16 54 C16 79 31 95 50 95 C69 95 84 79 84 54 C84 30 72 11 50 11 Z"
    />
  </g>
</svg>

<style>
  .rm {
    display: block;
    flex: 0 0 auto;
  }
</style>
