package app.neloa.desktop

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  private var multicastLock: WifiManager.MulticastLock? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
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
