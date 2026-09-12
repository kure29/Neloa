package app.neloa.desktop

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import io.crates.keyring.Keyring

class MainActivity : TauriActivity() {
  private var multicastLock: WifiManager.MulticastLock? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    // android-native-keyring-store reads ndk-context during Rust setup. Tauri
    // keeps its own Android context, so initialise the crate's context before
    // super.onCreate() can start the Rust application.
    Keyring.initializeNdkContext(applicationContext)
    super.onCreate(savedInstanceState)

    val wifiManager = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
    multicastLock = wifiManager?.createMulticastLock("neloa-mdns")?.apply {
      setReferenceCounted(false)
      acquire()
    }
  }

  override fun onDestroy() {
    multicastLock?.let { lock ->
      if (lock.isHeld) lock.release()
    }
    multicastLock = null
    super.onDestroy()
  }
}
