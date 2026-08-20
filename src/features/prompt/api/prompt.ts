import { invoke } from "@tauri-apps/api/core";

export interface Prompt {
  id: number;
  content: string;
  title?: string | null;
  created_at: string;
  updated_at: string;
}

export const listPrompts = (): Promise<Prompt[]> => invoke("list_prompts");
export const createPrompt = (
  content: string,
  title?: string | null
): Promise<Prompt> => invoke("create_prompt", { content, title });
export const updatePromptTitle = (
  id: number,
  title?: string | null
): Promise<Prompt> => invoke("update_prompt_title", { id, title });
export const deletePrompt = (id: number): Promise<void> =>
  invoke("delete_prompt", { id });