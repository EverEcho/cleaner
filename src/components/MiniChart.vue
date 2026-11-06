<script setup lang="ts">
import { computed } from 'vue';
const props=defineProps<{values:(number|null)[];max?:number;label:string}>();
const path=computed(()=>{const values=props.values.slice(-30),max=props.max||Math.max(1,...values.filter((v):v is number=>v!=null));let gap=true;return values.map((v,i)=>{if(v==null){gap=true;return '';}const x=i*240/Math.max(1,values.length-1),y=34-Math.min(1,Math.max(0,v/max))*30;const point=`${gap?'M':'L'}${x.toFixed(1)} ${y.toFixed(1)}`;gap=false;return point;}).join(' ');});
</script>
<template><svg viewBox="0 0 240 40" preserveAspectRatio="none" role="img" :aria-label="label"><path d="M0 36H240" stroke="currentColor" opacity=".12"/><path :d="path" stroke="currentColor" fill="none" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" vector-effect="non-scaling-stroke"/></svg></template>
<style scoped>svg{display:block;width:100%;height:34px;overflow:visible}</style>
