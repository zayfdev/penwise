<script>
  let count = 0;
</script>

<div class="counter">
  <button on:click={() => count++}>{count}</button>
  <Counter />
  <svelte:component this={C} />
  <slot />
</div>

<style>
  div {
    color: red;
  }
</style>
