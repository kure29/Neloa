package com.kure29.neloa

import android.content.Context
import android.Manifest
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import io.crates.keyring.Keyring

class MainActivity : TauriActivity() {
  private var multicastLock: WifiManager.MulticastLock? = null
  private var wifiDirectBridge: WifiDirectBridge? = null

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

    wifiDirectBridge = WifiDirectBridge(this).also { bridge ->
      bridge.activate()
      requestWifiDirectPermission()
    }
  }

  fun requestWifiDirectPermission() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return
    val permission = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      Manifest.permission.NEARBY_WIFI_DEVICES
    } else {
      Manifest.permission.ACCESS_FINE_LOCATION
    }
    if (checkSelfPermission(permission) != PackageManager.PERMISSION_GRANTED) {
      requestPermissions(arrayOf(permission), WIFI_DIRECT_PERMISSION_REQUEST)
    }
  }

  override fun onRequestPermissionsResult(
    requestCode: Int,
    permissions: Array<out String>,
    grantResults: IntArray,
  ) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    if (requestCode == WIFI_DIRECT_PERMISSION_REQUEST) {
      wifiDirectBridge?.permissionsChanged()
    }
  }

  override fun onDestroy() {
    multicastLock?.let { lock ->
      if (lock.isHeld) lock.release()
    }
    multicastLock = null
    wifiDirectBridge?.close()
    wifiDirectBridge = null
    super.onDestroy()
  }

  companion object {
    private const val WIFI_DIRECT_PERMISSION_REQUEST = 48632
  }
}
