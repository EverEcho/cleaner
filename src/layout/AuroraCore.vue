<script setup lang="ts">
import { useId } from 'vue';

withDefaults(defineProps<{ active?: boolean }>(), { active: false });
const id = useId().replace(/:/g, '');
</script>

<template>
  <div class="aurora-core" :class="{ active }" aria-hidden="true">
    <div class="core-atmosphere" />
    <svg class="core-scene" viewBox="0 0 400 400" fill="none" xmlns="http://www.w3.org/2000/svg">
      <defs>
        <radialGradient :id="`${id}-sphere`" cx=".29" cy=".24" r=".82">
          <stop offset="0" stop-color="#e8deff" />
          <stop offset=".21" stop-color="#c0a2ff" />
          <stop offset=".5" stop-color="#8754d9" />
          <stop offset=".82" stop-color="#342b79" />
          <stop offset="1" stop-color="#171b43" />
        </radialGradient>
        <linearGradient :id="`${id}-orbit`" x1="56" y1="135" x2="340" y2="263" gradientUnits="userSpaceOnUse">
          <stop stop-color="#b69aff" stop-opacity=".06" />
          <stop offset=".35" stop-color="#baa0ff" stop-opacity=".85" />
          <stop offset=".69" stop-color="#7fddeb" />
          <stop offset="1" stop-color="#7fddeb" stop-opacity=".08" />
        </linearGradient>
        <linearGradient :id="`${id}-spark`" x1="169" y1="167" x2="231" y2="233" gradientUnits="userSpaceOnUse">
          <stop stop-color="#fff" /><stop offset="1" stop-color="#c2b4f1" />
        </linearGradient>
        <radialGradient :id="`${id}-shadow`"><stop stop-color="#8066e1" stop-opacity=".22" /><stop offset="1" stop-color="#8066e1" stop-opacity="0" /></radialGradient>
        <filter :id="`${id}-glow`" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="3" /></filter>
      </defs>
      <ellipse cx="201" cy="325" rx="125" ry="23" :fill="`url(#${id}-shadow)`" />
      <g class="distant-orbit">
        <ellipse cx="200" cy="200" rx="170" ry="147" stroke="#aa8ee3" stroke-opacity=".1" stroke-dasharray="2 9" transform="rotate(-27 200 200)" />
        <circle cx="67" cy="111" r="2" fill="#a88ce4" fill-opacity=".6" />
        <circle cx="340" cy="127" r="1.4" fill="#75d7e5" fill-opacity=".7" />
        <circle cx="294" cy="311" r="1.5" fill="#bd9de8" fill-opacity=".4" />
      </g>
      <g class="back-orbit">
        <ellipse cx="200" cy="200" rx="148" ry="58" transform="rotate(-34 200 200)" :stroke="`url(#${id}-orbit)`" stroke-width="1.1" />
        <ellipse cx="200" cy="200" rx="143" ry="63" transform="rotate(43 200 200)" stroke="#b398ed" stroke-opacity=".27" stroke-width=".8" />
      </g>
      <g class="sphere-body">
        <circle cx="200" cy="200" r="75" :fill="`url(#${id}-sphere)`" />
        <circle cx="200" cy="200" r="74.5" stroke="#c0a7f9" stroke-opacity=".22" />
        <path d="M143 165C165 137 199 128 225 137" stroke="#e7d8ff" stroke-opacity=".5" stroke-width="1.4" stroke-linecap="round" />
        <path d="M229 151L212 189L236 204L204 212L185 246L190 212L165 199L191 187L201 162L209 182L229 151Z" :fill="`url(#${id}-spark)`" fill-opacity=".12" :filter="`url(#${id}-glow)`" />
        <path d="M200 167L207 190L231 200L207 207L200 232L191 208L170 200L191 190L200 167Z" :fill="`url(#${id}-spark)`" />
        <path d="M216 170L220 180L230 183L220 187L216 197L213 187L203 183L213 180L216 170Z" fill="white" fill-opacity=".82" />
      </g>
      <path class="front-orbit" d="M64 236C80 256 148 250 221 222C296 193 348 152 339 130" :stroke="`url(#${id}-orbit)`" stroke-width="2.3" stroke-linecap="round" />
      <path d="M104 110C83 130 121 182 188 231C256 280 322 303 340 283" stroke="#b6a0fb" stroke-opacity=".45" stroke-width="1.1" stroke-linecap="round" />
      <g class="orbit-particle">
        <circle cx="303" cy="167" r="7" fill="#7fe8ec" fill-opacity=".18" :filter="`url(#${id}-glow)`" />
        <circle cx="303" cy="167" r="3" fill="#a7f5f6" />
        <circle cx="101" cy="143" r="3.5" fill="#c2a8ff" />
      </g>
      <path d="M305 74V86M299 80H311" stroke="#9ba2cd" stroke-opacity=".5" stroke-linecap="round" />
      <path d="M84 284V292M80 288H88" stroke="#b7a5d6" stroke-opacity=".5" stroke-linecap="round" />
      <circle cx="155" cy="63" r="1" fill="#cabce9" fill-opacity=".6" />
      <circle cx="333" cy="245" r="1" fill="#cabce9" fill-opacity=".5" />
    </svg>
  </div>
</template>

<style scoped>
.aurora-core { width: 100%; aspect-ratio: 1; position: relative; isolation: isolate; }
.core-atmosphere { position: absolute; inset: 13%; border-radius: 50%; background: radial-gradient(circle at 47% 47%, #9360e629, #8155d915 42%, #66d2e507 58%, transparent 71%); filter: blur(17px); animation: atmosphere 7s ease-in-out infinite; }
.core-scene { width: 100%; height: 100%; overflow: visible; position: relative; }
.sphere-body { transform-origin: 200px 200px; animation: core-breathe 7s ease-in-out infinite; }
.orbit-particle { animation: particle-shimmer 4s ease-in-out infinite; }
.distant-orbit { transform-origin: 200px 200px; animation: distant-drift 90s linear infinite; }
.active .core-atmosphere { animation-duration: 2.5s; }
.active .sphere-body { animation-duration: 2.5s; }
.active .distant-orbit { animation-duration: 20s; }
.active .orbit-particle { animation-duration: 1.6s; }
@keyframes core-breathe { 0%, 100% { transform: translateY(3px); } 50% { transform: translateY(-4px); } }
@keyframes atmosphere { 0%, 100% { opacity: .7; transform: scale(.97); } 50% { opacity: 1; transform: scale(1.07); } }
@keyframes particle-shimmer { 0%, 100% { opacity: .65; } 50% { opacity: 1; } }
@keyframes distant-drift { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .core-atmosphere, .sphere-body, .orbit-particle, .distant-orbit { animation: none; } }
</style>
