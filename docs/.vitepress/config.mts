import { defineConfig } from "vitepress";

/**
 * Neloa 说明书。
 *
 * 语言：`/Neloa/` 是简体中文，`/Neloa/en/` 是英文。两个 locale 各自完整，
 * 页面上不会出现中英混排。
 *
 * 内容分工：`docs/` 承载使用手册。`ARCHITECTURE.md`、`relay/README.md`、
 * `MOBILE_BUILD.md`、`WINDOWS_BUILD.md` 是各自主题的唯一真源；在它们本身的语言里
 * 用 VitePress 的文件引入（`<!--@include-->`）原样呈现，另一种语言使用译文。
 */

type Lang = "zh" | "en";

interface Copy {
  guide: string;
  reference: string;
  build: string;
  project: string;
  groups: [string, string, string, string, string];
  items: {
    intro: string;
    install: string;
    pairing: string;
    transfer: string;
    clipboard: string;
    relay: string;
    troubleshooting: string;
    architecture: string;
    relayReference: string;
    buildStart: string;
    buildWindows: string;
    buildMobile: string;
    structure: string;
  };
  outline: string;
  editLink: string;
  prev: string;
  next: string;
  lastUpdated: string;
}

const ZH: Copy = {
  guide: "指南",
  reference: "参考",
  build: "构建",
  project: "项目",
  groups: ["开始使用", "使用", "参考", "构建与发布", "项目"],
  items: {
    intro: "认识 Neloa",
    install: "下载与安装",
    pairing: "配对与设备信任",
    transfer: "发送与接收文件",
    clipboard: "剪贴板同步",
    relay: "自建中继",
    troubleshooting: "故障排查",
    architecture: "架构与安全边界",
    relayReference: "中继服务与线协议",
    buildStart: "从源码构建",
    buildWindows: "Windows 安装程序",
    buildMobile: "Android 与 iOS",
    structure: "目录与命令",
  },
  outline: "本页目录",
  editLink: "在 GitHub 上编辑此页",
  prev: "上一节",
  next: "下一节",
  lastUpdated: "最后更新",
};

const EN: Copy = {
  guide: "Guide",
  reference: "Reference",
  build: "Build",
  project: "Project",
  groups: ["Getting started", "Using Neloa", "Reference", "Build and release", "Project"],
  items: {
    intro: "What Neloa is",
    install: "Download and install",
    pairing: "Pairing and device trust",
    transfer: "Sending and receiving files",
    clipboard: "Clipboard sync",
    relay: "Self-hosted relay",
    troubleshooting: "Troubleshooting",
    architecture: "Architecture and security",
    relayReference: "Relay service and wire protocol",
    buildStart: "Building from source",
    buildWindows: "Windows installer",
    buildMobile: "Android and iOS",
    structure: "Layout and commands",
  },
  outline: "On this page",
  editLink: "Edit this page on GitHub",
  prev: "Previous",
  next: "Next",
  lastUpdated: "Last updated",
};

/** 两个 locale 的导航与侧栏结构完全一致，只有文案和链接前缀不同。 */
function localeTheme(lang: Lang, copy: Copy) {
  const at = (path: string) => (lang === "zh" ? path : `/en${path}`);

  return {
    nav: [
      { text: copy.guide, link: at("/guide/"), activeMatch: "^/(en/)?guide/" },
      {
        text: copy.reference,
        link: at("/reference/architecture"),
        activeMatch: "^/(en/)?reference/",
      },
      { text: copy.build, link: at("/build/"), activeMatch: "^/(en/)?build/" },
      {
        text: copy.project,
        link: at("/project/structure"),
        activeMatch: "^/(en/)?project/",
      },
    ],

    sidebar: [
      {
        text: copy.groups[0],
        items: [
          { text: copy.items.intro, link: at("/guide/") },
          { text: copy.items.install, link: at("/guide/install") },
          { text: copy.items.pairing, link: at("/guide/pairing") },
        ],
      },
      {
        text: copy.groups[1],
        items: [
          { text: copy.items.transfer, link: at("/guide/transfer") },
          { text: copy.items.clipboard, link: at("/guide/clipboard") },
          { text: copy.items.relay, link: at("/guide/relay") },
          { text: copy.items.troubleshooting, link: at("/guide/troubleshooting") },
        ],
      },
      {
        text: copy.groups[2],
        items: [
          { text: copy.items.architecture, link: at("/reference/architecture") },
          { text: copy.items.relayReference, link: at("/reference/relay") },
        ],
      },
      {
        text: copy.groups[3],
        items: [
          { text: copy.items.buildStart, link: at("/build/") },
          { text: copy.items.buildWindows, link: at("/build/windows") },
          { text: copy.items.buildMobile, link: at("/build/mobile") },
        ],
      },
      {
        text: copy.groups[4],
        items: [{ text: copy.items.structure, link: at("/project/structure") }],
      },
    ],

    outline: { level: [2, 3] as [number, number], label: copy.outline },

    editLink: {
      pattern: "https://github.com/kure29/Neloa/edit/main/docs/:path",
      text: copy.editLink,
    },

    docFooter: { prev: copy.prev, next: copy.next },

    lastUpdated: {
      text: copy.lastUpdated,
      formatOptions: { dateStyle: "short" as const, timeStyle: "short" as const },
    },
  };
}

export default defineConfig({
  title: "Neloa",

  /**
   * GitHub Pages 项目站点为 "/Neloa/"。改用自定义域名时这里要改，
   * `head` 里的 favicon 路径也要一起改，否则静态资源会 404。
   */
  base: "/Neloa/",
  cleanUrls: true,

  head: [
    ["link", { rel: "icon", href: "/Neloa/neloa-icon.svg" }],
    ["meta", { name: "theme-color", media: "(prefers-color-scheme: light)", content: "#ffffff" }],
    ["meta", { name: "theme-color", media: "(prefers-color-scheme: dark)", content: "#0d111b" }],
    ["meta", { name: "color-scheme", content: "light dark" }],
  ],

  markdown: {
    theme: { light: "github-light", dark: "github-dark" },
    lineNumbers: false,
  },

  locales: {
    root: {
      label: "简体中文",
      lang: "zh-CN",
      description: "无需账号，在你的设备之间安全传输文件与剪贴板。",
      themeConfig: localeTheme("zh", ZH),
    },
    en: {
      label: "English",
      lang: "en-US",
      link: "/en/",
      description:
        "Encrypted file and clipboard transfer between your own devices, without an account.",
      themeConfig: localeTheme("en", EN),
    },
  },

  themeConfig: {
    siteTitle: "Neloa",
    logo: "/neloa-icon.svg",
    socialLinks: [{ icon: "github", link: "https://github.com/kure29/Neloa" }],
    search: { provider: "local" },
    footer: {
      message: "Apache License 2.0",
      copyright: "Neloa",
    },
  },
});
