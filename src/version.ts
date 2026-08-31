// 由 vite.config.ts 的 define 注入 __APP_VERSION__（构建时替换为版本字符串）。
// 在模块内声明该标识符，导出 appVersion 作为统一入口。
declare const __APP_VERSION__: string;

export const appVersion: string = __APP_VERSION__;
