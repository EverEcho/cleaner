<script setup lang="ts">
import logo from '../assets/brand/appclear.png';
withDefaults(defineProps<{size?:number}>(),{size:48});
</script>
<template><img :src="logo" alt="" aria-hidden="true" :width="size" :height="size" class="app-logo" draggable="false" /></template>
<style scoped>.app-logo{display:block;flex-shrink:0;object-fit:contain}</style>
