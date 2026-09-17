/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{vue,ts}"],
  theme: {
    extend: {
      // 全站默认字体跟随 CSS 变量（preflight 的 html 字体取 fontFamily.sans），
      // 由 utils/font.ts 的字体家族设置写入；默认栈在 styles.css 的 :root 兜底。
      fontFamily: {
        sans: ["var(--font-family)"],
      },
    },
  },
  plugins: [],
};