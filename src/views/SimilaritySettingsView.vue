<script setup lang="ts">
// 相似度设置页面：从「设置面板的相似度页签」独立出来，入口在侧栏底部、通用设置的上方一格。
// 独占一页（与 SettingsView 平级），壳由 App.vue 的 PanelShell 提供；
// 内容沿用 SimilaritySection 整块（服务地址 / 连通性测试 / 索引并发 + 图像与提示词两个索引分组）。
//
// 定高（h-[80vh]）+ 内容区自滚：面板里有多处**打开后才异步长出来**的块（状态行、上次索引、
// 混维提示），而壳是垂直居中——高度一变上下边缘同时位移，观感就是「打开后底部跳一下」。
// 定高后外框自首帧即最终尺寸，异步内容只往内部滚动区里填。通用设置内容全静态，故仍按内容撑高。
import SimilaritySection from "@/features/similarity/SimilaritySection.vue";
</script>

<template>
  <section
    class="flex h-[80vh] flex-col rounded-lg border p-6 shadow-sm border-gray-700 bg-gray-800"
  >
    <div class="mb-4 flex h-6 shrink-0 items-center">
      <h2 class="text-lg font-semibold text-gray-100">相似度设置</h2>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto">
      <SimilaritySection />
    </div>
  </section>
</template>
