<script setup lang="ts">
import { useToast } from "./useToast";

const { toasts, dismissToast } = useToast();
</script>

<template>
  <Teleport to="body">
    <!-- z-[130]：永驻最高层，不被确认弹窗（110）/顶层模态（120）遮挡 -->
    <div
      class="pointer-events-none fixed inset-x-0 bottom-6 z-[130] flex flex-col items-center gap-2"
    >
      <!-- 动画参考 cm：入场弹性上滑，出场下滑淡出 -->
      <TransitionGroup
        enter-active-class="transition-all duration-[400ms] ease-[cubic-bezier(0.34,1.56,0.64,1)]"
        enter-from-class="opacity-0 translate-y-5 scale-95"
        leave-active-class="transition-all duration-300 ease-in"
        leave-to-class="opacity-0 translate-y-5 scale-95"
      >
        <!-- 样式参考 cm ToastModal：深底 + 绿色描边与内外辉光，点击可提前关闭 -->
        <div
          v-for="t in toasts"
          :key="t.id"
          class="pointer-events-auto cursor-pointer rounded-lg border-2 border-[#00ff88] bg-[#0a0a0a] px-5 py-3 text-sm text-[#00ff88] shadow-[0_0_20px_rgba(0,255,136,0.4),inset_0_0_10px_rgba(0,255,136,0.1)]"
          @click="dismissToast(t.id)"
        >
          {{ t.message }}
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>
