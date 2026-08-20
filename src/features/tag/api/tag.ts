import { invoke } from "@tauri-apps/api/core";

export interface Tag {
  id: number;
  name: string;
}

export const listTags = (): Promise<Tag[]> => invoke("list_tags");
export const createTag = (name: string): Promise<Tag> =>
  invoke("create_tag", { name });
export const deleteTag = (id: number): Promise<void> =>
  invoke("delete_tag", { id });