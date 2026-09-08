/**
 * 卡片信息显示开关（提示词/图像主页共用，全局单份状态）。
 *
 * 开启时卡片显示 row2 内容预览 / row3 标签 / row4 排序字段；
 * 关闭时仅剩背景图与悬浮按钮行（对齐 pm 的「信息」开关）。
 * localStorage 持久化（key: cardInfoVisible，默认显示）。
 */
import { ref } from "vue";

const KEY = "cardInfoVisible";

const cardInfoVisible = ref(localStorage.getItem(KEY) !== "0");

function toggleCardInfo() {
  cardInfoVisible.value = !cardInfoVisible.value;
  localStorage.setItem(KEY, cardInfoVisible.value ? "1" : "0");
}

export { cardInfoVisible, toggleCardInfo };
