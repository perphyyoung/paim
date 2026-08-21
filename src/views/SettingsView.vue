<script setup lang="ts">
import { appVersion } from "@/version";
</script>

<template>
  <section
    class="rounded-lg border border-gray-200 bg-white p-6 shadow-sm dark:border-gray-700 dark:bg-gray-800"
  >
    <h2 class="mb-4 text-lg font-semibold text-gray-800 dark:text-gray-100">
      设置
    </h2>

    <dl class="divide-y divide-gray-100 dark:divide-gray-700">
      <div class="flex justify-between py-3">
        <dt class="text-gray-600 dark:text-gray-400">版本</dt>
        <dd class="text-gray-800 dark:text-gray-100">v{{ appVersion }}</dd>
      </div>
    </dl>
  </section>
</template>