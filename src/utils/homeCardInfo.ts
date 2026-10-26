/**
 * 主页卡片信息显示开关（提示词/图像主页共用，全局单份状态；相似度结果与回收站不读它）。
 *
 * 开启时主页卡片显示正文 / 标签 / 排序字段标题；
 * 关闭时主页卡片仅剩背景图与悬浮按钮行（对齐 pm 的「信息」开关）。
 * localStorage 持久化（key: homeCardInfoVisible，默认显示）。
 */
import { ref } from "vue";
import { readLocalStorageBoolean, writeLocalStorageBoolean } from "@/utils/localStorage";

const KEY = "homeCardInfoVisible";

const homeCardInfoVisible = ref(readLocalStorageBoolean(KEY, true));

function toggleHomeCardInfo() {
  homeCardInfoVisible.value = !homeCardInfoVisible.value;
  writeLocalStorageBoolean(KEY, homeCardInfoVisible.value);
}

export { homeCardInfoVisible, toggleHomeCardInfo };
