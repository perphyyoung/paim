import { invoke } from "@tauri-apps/api/core";

export interface Prompt {
  id: string;
  title: string;
  content: string;
  content_translate: string;
  created_at: string;
  updated_at: string;
  is_deleted: boolean;
  deleted_at: string | null;
  is_favorite: boolean;
  is_safe: boolean;
  note: string;
}

export const listPrompts = (): Promise<Prompt[]> => invoke("list_prompts");
export const createPrompt = (
  content: string,
  title?: string | null
): Promise<Prompt> => invoke("create_prompt", { content, title });
export const updatePromptTitle = (
  id: string,
  title?: string | null
): Promise<Prompt> => invoke("update_prompt_title", { id, title });
export const deletePrompt = (id: string): Promise<void> =>
  invoke("delete_prompt", { id });