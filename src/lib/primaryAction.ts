import type { PeerDevice, SelectedFile } from "../types";
import { formatBytes, peerIsCompatible } from "./format";

export interface PrimaryAction {
  label: string;
  disabled: boolean;
  /** One line under the composer explaining what the button will do, or why it can't. */
  hint: string;
  tone: "neutral" | "warn" | "ok";
}

/**
 * The send button has seven distinct states. Resolving them in one place keeps
 * the label, the disabled flag and the explanatory line from drifting apart.
 */
export function resolvePrimaryAction(input: {
  peer: PeerDevice | null;
  trusted: boolean;
  files: SelectedFile[];
  busyPairing: boolean;
  busyFile: boolean;
}): PrimaryAction {
  const { peer, trusted, files, busyPairing, busyFile } = input;

  if (busyPairing) {
    return { label: "建立通道…", disabled: true, hint: "正在与对方协商加密会话", tone: "neutral" };
  }
  if (busyFile) {
    return { label: "正在加入队列…", disabled: true, hint: "正在为所选文件建立加密传输", tone: "neutral" };
  }
  if (!peer) {
    return { label: "发送", disabled: true, hint: "先在雷达中选择一台设备", tone: "neutral" };
  }
  if (!peerIsCompatible(peer)) {
    return {
      label: "版本不兼容",
      disabled: true,
      hint: `${peer.name} 的协议版本不兼容，请更新对方的 Neloa`,
      tone: "warn",
    };
  }
  if (!trusted) {
    return {
      label: "配对",
      disabled: false,
      hint: `先与 ${peer.name} 核对六位数字，配对后再选择文件`,
      tone: "warn",
    };
  }
  if (files.length === 0) {
    return {
      label: "选择文件",
      disabled: false,
      hint: `已与 ${peer.name} 配对，下一步选择要发送的文件`,
      tone: "ok",
    };
  }
  const totalSize = files.reduce((total, file) => total + file.size, 0);
  return {
    label: files.length === 1 ? "发送" : `发送 ${files.length} 个文件`,
    disabled: false,
    hint: `将向 ${peer.name} 发送 ${formatBytes(totalSize)}，端到端加密`,
    tone: "ok",
  };
}
