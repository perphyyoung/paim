<script setup lang="ts">
import PromptList from "@/features/prompt/components/PromptList.vue";
import TagList from "@/features/tag/components/TagList.vue";
</script>

<template>
  <div class="h-full overflow-y-auto pb-6">
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <PromptList />
      <TagList />
    </div>
  </div>
</template>