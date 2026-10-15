<script setup lang="ts">
/**
 * 设置面板的共用悬浮壳：全屏遮罩 + 居中 dialog + 右上角 ✕。
 * 「通用设置」（SettingsView）与「相似度设置」（SimilaritySettingsView）同形，故收成一份。
 * 只管开合与定位，内容由调用方通过插槽给出；关闭只 emit `close`，
 * 主页脏标记的广播由 App.vue 统一发（对两个开关各挂一个 watch）。
 *
 * 注意：壳是**垂直居中**的，面板高度一变会让上下边缘同时位移。内容若含「打开后才异步长出来」
 * 的块，请让面板自身定高（内容区自滚），否则会表现为「打开后底部跳一下」
 * （反例与修法见 SimilaritySettingsView 的定高说明）。
 */
defineProps<{
  /** 是否打开 */
  open: boolean;
  /** 无障碍标签：dialog 的 aria-label，也是 e2e 的定位点 */
  label: string;
  /** 面板宽度类；缺省与通用设置同宽 */
  widthClass?: string;
}>();

defineEmits<{ close: [] }>();
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40"
      @click.self="$emit('close')"
    >
      <div
        class="relative"
        :class="widthClass ?? 'w-[50vw] max-w-[50vw]'"
        role="dialog"
        aria-modal="true"
        :aria-label="label"
      >
        <slot />
        <button
          type="button"
          class="absolute -top-2 -right-2 flex h-7 w-7 items-center justify-center rounded-full bg-gray-600 text-white hover:bg-gray-500"
          title="关闭"
          @click="$emit('close')"
        >
          ✕
        </button>
      </div>
    </div>
  </Teleport>
</template>
