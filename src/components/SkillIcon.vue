<script setup lang="ts">
import { computed, ref, watch } from "vue";
import SkillGlyph from "./SkillGlyph.vue";

const props = withDefaults(defineProps<{
  name: string;
  identity: string;
  description?: string;
  src?: string;
  fallbackSrc?: string;
}>(), { description: "", src: "", fallbackSrc: "" });

// 颜色只由技能标识决定，刷新、搜索和翻译描述都不会改变它。
const palettes = [
  ["#edf3ff", "#3567c8", "#d9e5fc"],
  ["#eaf6f1", "#25836b", "#d2ebe1"],
  ["#fff3e6", "#b76b27", "#f5e2ca"],
  ["#f2edfb", "#8056b2", "#e5daf5"],
  ["#fceef2", "#b75175", "#f4dce5"],
  ["#eaf5fa", "#2e7f9f", "#d4eaf3"],
];
const palette = computed(() => {
  let hash = 0;
  for (const char of props.identity) hash = (Math.imul(hash, 31) + char.codePointAt(0)!) | 0;
  const [background, color, borderColor] = palettes[(hash >>> 0) % palettes.length];
  return { background, color, borderColor };
});
const failed = ref(false);
const fallbackFailed = ref(false);
const loaded = ref(false);
watch(() => [props.src, props.fallbackSrc], () => { failed.value = false; fallbackFailed.value = false; loaded.value = false; });
const imageSource = computed(() => (!failed.value ? props.src : "") || (!fallbackFailed.value ? props.fallbackSrc : ""));
function onImageError() {
  loaded.value = false;
  if (props.src && !failed.value) failed.value = true;
  else fallbackFailed.value = true;
}
</script>

<template>
  <span class="skill-icon" :class="{ 'has-image': imageSource && loaded }" :style="imageSource && loaded ? undefined : palette" aria-hidden="true">
    <img v-if="imageSource" :src="imageSource" :style="{opacity:loaded?1:0}" alt="" loading="lazy" referrerpolicy="no-referrer" @load="loaded=true" @error="onImageError" />
    <SkillGlyph v-if="!imageSource || !loaded" :label="name" :description="description" />
  </span>
</template>

<style scoped>
.skill-icon {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border: 1px solid;
  border-radius: 10px;
  overflow: hidden;
}
.skill-icon svg { width: 56%; height: 56%; }
.skill-icon.has-image { background: #fff; border-color: var(--border-soft); }
.skill-icon img { position:absolute; inset:0; width: 100%; height: 100%; object-fit: contain; display: block; }
</style>
