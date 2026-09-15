/*
 * 用 theme-without-fonts：默认主题会把 Inter 一起打包进来，而 Neloa 的字体栈是
 * 系统栈（见 src/styles/tokens.css 的 --font-ui）。留一份用不到的字体只会白占带宽。
 */
import DefaultTheme from "vitepress/theme-without-fonts";

import "./neloa.css";

/**
 * 站点沿用应用本身的 "white & signal" 视觉系统：颜色、圆角、字体栈与动效曲线
 * 全部来自 `src/styles/tokens.css`，不在文档里另起一套。
 */
export default DefaultTheme;
