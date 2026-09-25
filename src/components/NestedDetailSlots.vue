<script setup lang="ts">
/**
 * 嵌套详情槽位（页面级渲染）：把 `useNestedDetails` 的两个槽渲染成真正的详情弹窗。
 *
 * - 每类至多一个：槽为空就不渲染；`:key` 跟内容 id 走 → 换内容即换实例，
 *   详情快照（useDetailSnapshot）不会停在旧 id（历史坑见 docs/lessons.md 第 25 节）；
 * - 这里只管渲染，并把「内容变了 / 换了图 / 安全联动」回报给栈的 `revision` / `safeSynced`；
 *   业务反应（重拉关联数据、标主页过期、回写卡片）由**宿主详情**订阅这两个信号完成 ——
 *   只有它知道自己要重拉哪份数据（槽被页面持有，回调链没法表达「谁打开的」）。
 */
import ImageDetailModal from "@/features/image/components/ImageDetailModal.vue";
import PromptDetailModal from "@/features/prompt/components/PromptDetailModal.vue";
import { useNestedDetails } from "@/composables/useNestedDetails";

const nested = useNestedDetails();
</script>

<template>
  <!-- 嵌套图像槽 -->
  <ImageDetailModal
    v-if="nested.image.value"
    :key="nested.image.value.card.id"
    :open="true"
    :images="[nested.image.value.card]"
    :order="[nested.image.value.card.id]"
    :initial-index="0"
    :thumbs="nested.image.value.thumbs"
    :z="nested.imageZ.value"
    is-nested
    @close="nested.closeNested('image')"
    @update="nested.reportChanged()"
    @replaced="(p) => nested.replaceNestedImage(p.image)"
    @safe-synced="(s) => nested.reportSafeSynced(s)"
  />

  <!-- 嵌套提示词槽 -->
  <PromptDetailModal
    v-if="nested.prompt.value"
    :key="nested.prompt.value.cards[0]?.id ?? 'none'"
    :open="true"
    :prompts="nested.prompt.value.cards"
    :order="[nested.prompt.value.cards[0]?.id ?? '']"
    :initial-index="0"
    :tag-names="nested.prompt.value.tagNames"
    :all-tags="nested.prompt.value.allTags"
    :z="nested.promptZ.value"
    is-nested
    @close="nested.closeNested('prompt')"
    @updated="nested.reportChanged()"
    @safe-synced="(s) => nested.reportSafeSynced(s)"
  />
</template>
