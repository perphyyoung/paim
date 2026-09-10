<script setup lang="ts">
import { useToast, type ToastType } from "./useToast";

const { toasts, dismissToast } = useToast();

// 设计三色：info/success 绿、warning 琥珀、error 红（描边/文字/辉光同步换色）
const TYPE_CLASSES: Record<ToastType, string> = {
  info: "border-[#00ff88] text-[#00ff88] shadow-[0_0_20px_rgba(0,255,136,0.4),inset_0_0_10px_rgba(0,255,136,0.1)]",
  success:
    "border-[#00ff88] text-[#00ff88] shadow-[0_0_20px_rgba(0,255,136,0.4),inset_0_0_10px_rgba(0,255,136,0.1)]",
  warning:
    "border-[#ffb020] text-[#ffb020] shadow-[0_0_20px_rgba(255,176,32,0.4),inset_0_0_10px_rgba(255,176,32,0.1)]",
  error:
    "border-[#ff5c5c] text-[#ff5c5c] shadow-[0_0_20px_rgba(255,92,92,0.4),inset_0_0_10px_rgba(255,92,92,0.1)]",
};
</script>

<template>
  <Teleport to="body">
    <!-- toast 全部居中显示：底部易被忽略，居中确保被注意到（错误/警告停留 4s） -->
    <!-- z-[130]：永驻最高层，不被确认弹窗（110）/顶层模态（120）遮挡；容器不拦截点击，toast 本体可点 -->
    <div
      class="pointer-events-none fixed inset-0 z-[130] flex flex-col items-center justify-center gap-2"
    >
      <!-- 动画参考 cm：入场弹性上滑，出场下滑淡出 -->
      <!-- 出场（离场）期间 pointer-events-none：淡出中的 toast 不再拦截鼠标点击
             （居中 toast 离场要 300ms，期间挡住底下元素；e2e 06 用例 7 覆盖该行为） -->
      <TransitionGroup
        enter-active-class="transition-all duration-[400ms] ease-[cubic-bezier(0.34,1.56,0.64,1)]"
        enter-from-class="opacity-0 translate-y-5 scale-95"
        leave-active-class="pointer-events-none transition-all duration-300 ease-in"
        leave-to-class="opacity-0 translate-y-5 scale-95"
      >
        <!-- 样式参考 cm ToastModal：深底 + 按类型换色的描边与内外辉光，点击可提前关闭 -->
        <div
          v-for="t in toasts"
          :key="t.id"
          class="pointer-events-auto cursor-pointer rounded-lg border-2 bg-[#0a0a0a] px-5 py-3 text-sm"
          :class="TYPE_CLASSES[t.type]"
          @click="dismissToast(t.id)"
        >
          {{ t.message }}
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>
