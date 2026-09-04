// 提示词命令封装：类型安全绑定转调（bindings 由 tauri-specta 自动生成）。
import { commands, type Prompt } from "@/bindings";

export type { Prompt } from "@/bindings";

export const listPrompts = () => commands.listPrompts();
export const createPrompt = (content: string, title?: string | null) =>
  commands.createPrompt(content, title ?? null);
export const deletePrompt = (id: string) => commands.deletePrompt(id);
