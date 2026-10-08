import type { NeloaState } from "../lib/useNeloa";
import type { PeerDevice, TransportPreference } from "../types";
import { Icon } from "./icons";
import { cx } from "./kit";

const PEER_TO_PEER_CAPABILITY = "transport-peer-to-peer-wifi";
const BLUETOOTH_CAPABILITY = "transport-bluetooth";

export const TRANSPORT_LABELS: Record<TransportPreference, string> = {
  ask: "未选择",
  lan: "局域网",
  peerToPeer: "点对点 Wi-Fi",
  bluetooth: "蓝牙",
  relay: "中继",
};

/**
 * The per-device route picker. Neloa never chooses a route on the user's
 * behalf, so an unchosen route is drawn as needing attention rather than
 * defaulted.
 */
export function TransportSelect({
  app,
  peer,
  displayName,
  id,
  className,
}: {
  app: NeloaState;
  peer: PeerDevice;
  displayName: string;
  id: string;
  className?: string;
}) {
  const preference = app.transportPreferenceFor(peer.id);
  const peerToPeerAvailable = peer.capabilities.includes(PEER_TO_PEER_CAPABILITY)
    && peer.peerToPeerAvailable;
  const bluetoothAvailable = peer.capabilities.includes(BLUETOOTH_CAPABILITY)
    && peer.bluetoothAvailable;

  return (
    <label
      className={cx("peer-transport-control", preference === "ask" && "needs-choice", className)}
      title="选择这台设备的连接方式"
    >
      <span className="sr-only">{displayName} 的连接方式</span>
      <select
        id={id}
        value={preference}
        aria-label={`${displayName} 的连接方式`}
        disabled={app.busyAction !== null}
        onClick={(event) => event.stopPropagation()}
        onChange={(event) => {
          event.stopPropagation();
          void app.configureTransportPreference(peer.id, event.target.value as TransportPreference);
        }}
      >
        <option value="ask" disabled>选择连接方式</option>
        <option value="lan" disabled={peer.addresses.length === 0}>局域网</option>
        <option value="peerToPeer" disabled={!peerToPeerAvailable}>
          {peerToPeerAvailable ? "点对点 Wi-Fi" : "点对点 Wi-Fi（不可用）"}
        </option>
        <option value="bluetooth" disabled={!bluetoothAvailable}>
          {bluetoothAvailable ? "蓝牙" : "蓝牙（不可用）"}
        </option>
        <option value="relay" disabled={!peer.relayAvailable}>中继</option>
      </select>
      <Icon name="chevron" size={13} />
    </label>
  );
}
