import { useEffect } from "react";

import { onAndroidBack } from "../bridge";
import type { NeloaState } from "../lib/useNeloa";
import { DeviceList, PeerDetail, startPairingFlow } from "./DevicePane";

/**
 * The phone's device tab: the device list, and the chosen device pushed over
 * it as its own screen. The detail is the same PeerDetail the desktop pane
 * shows; the shell header supplies the way back.
 */
export function MobileDevicesView({ app }: { app: NeloaState }) {
  const peer = app.selectedPeer;
  const { setSelectedPeerId } = app;
  const detailOpen = Boolean(peer);

  // Android's back button leaves the detail first. The listener only exists
  // while the detail is open, so on the list the system default (leave the
  // app) still applies.
  useEffect(() => {
    if (!detailOpen || app.platform !== "android") return;
    let disposed = false;
    let stop = () => {};
    void onAndroidBack(() => setSelectedPeerId(null)).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    return () => {
      disposed = true;
      stop();
    };
  }, [detailOpen, app.platform, setSelectedPeerId]);

  if (!peer) {
    return (
      <div className="mobile-devices">
        <DeviceList app={app} />
      </div>
    );
  }

  return (
    <section className="split-detail mobile-detail" aria-label="设备详情">
      <PeerDetail
        key={peer.id}
        app={app}
        peer={peer}
        onPair={() => startPairingFlow(app, peer.id)}
      />
    </section>
  );
}
