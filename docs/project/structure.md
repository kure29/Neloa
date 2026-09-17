---
title: 目录与命令
---

# 目录与命令

```text
src/                         React 界面与 Tauri 调用桥
src-tauri/                   客户端 Rust 核心及原生工程
relay/                       自建中继服务与 Docker Compose
crates/neloa-relay-protocol/ 客户端和中继共享协议
docs/                        本文档站（VitePress）
.github/workflows/           持续集成与安装包构建
```

## 界面层

```text
src/
  bridge.ts        typed Tauri commands/events，以及浏览器预览模拟
  lib/useNeloa.ts  全部应用状态与动作；两个外壳共用
  ui/              图标、基础组件、浮层
  views/           RadarView | HistoryView | SettingsView —— 与外壳无关
  shell/           DesktopShell（标题栏） | MobileShell（标签栏、安全区）
  styles/          tokens | base | components | desktop | mobile
```

`views/` 与 `ui/` 里没有平台分支，唯一的例外是通过 `useShell()` 让弹层在桌面端显示为居中对话框、在移动端显示为底部面板。外壳由前端根据 `?platform=`、指针类型、视口宽度和 user agent 选择；Rust 后端报告 `macos`、`windows`、`linux`、`ios` 或 `android`，协议行为与外壳无关。

界面颜色只来自 `src/styles/tokens.css` 里的一套自定义属性，每个变量都有一个亮色值和一个暗色值，因此组件规则从不判断配色方案。本文档站也沿用这同一套 token。

## 文档是真源在哪里

每个主题只有一个真源，文档站不复制它们：

| 主题 | 真源 |
| --- | --- |
| 使用手册（安装、配对、传输、剪贴板、中继、排查） | `docs/guide/` |
| 架构与威胁模型 | `docs/en/reference/architecture.md`（英文原文） |
| 中继部署与线协议 | `relay/README.md` |
| 移动端构建与验收 | `docs/build/mobile.md`（中文原文） |
| Windows 构建 | `docs/build/windows.md`（中文原文） |

架构、移动端与 Windows 构建文档现在直接位于 VitePress 对应页面，不再通过额外的包装文件引入。中继文档仍从 `relay/README.md` 引入，以避免维护第二份协议说明。

## 本地预览文档

```bash
npm run docs:dev      # 本地预览，默认 http://localhost:5173/Neloa/
npm run docs:build    # 生成静态站点到 docs/.vitepress/dist
npm run docs:preview  # 预览构建结果
```
