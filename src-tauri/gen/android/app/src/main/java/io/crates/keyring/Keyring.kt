package io.crates.keyring

import android.content.Context

/**
 * JNI bridge required by android-native-keyring-store.
 *
 * The native symbol is linked into Neloa's Rust library. Loading the same
 * library again later from Tauri's generated Rust bridge is safe.
 */
class Keyring {
  companion object {
    init {
      System.loadLibrary("neloa_lib")
    }

    external fun initializeNdkContext(context: Context)
  }
}
