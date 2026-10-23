/** 相似度结果行：卡片 + 分数；结果页与图像选择界面共用。 */
export interface ResultRow<T> {
  card: T;
  score: number;
}
