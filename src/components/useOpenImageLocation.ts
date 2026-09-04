import { commands } from "@/bindings";
import { useToast } from "./useToast";

// 图像专用：按图像 id 查库定位真实文件，并在资源管理器中打开其保存位置
export function useOpenImageLocation() {
  const { showToast } = useToast();

  async function openImageLocation(id: string) {
    try {
      await commands.openImageLocation(id);
    } catch (e) {
      showToast(`打开保存位置失败：${e}`, "error");
    }
  }

  return { openImageLocation };
}
