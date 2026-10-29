<template>
  <div class="wrap">
    <MyButton />
    <button @click="go">Go</button>
    <template v-if="ok"><span>ok</span></template>
    <slot />
  </div>
</template>

<script setup>
import MyButton from './MyButton.vue';
</script>
