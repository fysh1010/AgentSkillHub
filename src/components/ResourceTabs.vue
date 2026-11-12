<script setup lang="ts">
defineProps<{ modelValue: string; items: { id: string; name: string; count?: number }[]; label: string }>();
defineEmits<{ 'update:modelValue': [value: string] }>();
</script>

<template>
  <nav class="resource-tabs" :aria-label="label" role="tablist">
    <button v-for="item in items" :key="item.id" type="button" role="tab" :class="{ active:modelValue === item.id }" :aria-label="item.name" :aria-selected="modelValue === item.id" @click="$emit('update:modelValue', item.id)">
      {{ item.name }}<span v-if="item.count !== undefined">{{ item.count }}</span>
    </button>
  </nav>
</template>

<style scoped>
.resource-tabs { display:flex; align-items:center; gap:24px; border-bottom:1px solid var(--border); margin-bottom:22px; }
.resource-tabs button { display:flex; align-items:center; gap:8px; position:relative; padding:0 2px 12px; border:0; background:none; font:inherit; font-size:12px; color:var(--muted); cursor:pointer; }
.resource-tabs button.active { color:var(--text); font-weight:600; }
.resource-tabs button.active::after { content:''; position:absolute; height:2px; bottom:-1px; left:0; right:0; background:var(--accent); border-radius:2px; }
.resource-tabs button > span { padding:0 6px; background:var(--panel-2); color:var(--muted); border-radius:4px; font-size:10px; font-weight:400; }
.resource-tabs button:focus-visible { outline:2px solid var(--accent); outline-offset:4px; }
</style>
