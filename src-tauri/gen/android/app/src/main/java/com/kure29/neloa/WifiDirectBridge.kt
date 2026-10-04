package com.kure29.neloa

import android.Manifest
import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.wifi.p2p.WifiP2pConfig
import android.net.wifi.p2p.WifiP2pInfo
import android.net.wifi.p2p.WifiP2pManager
import android.net.wifi.p2p.nsd.WifiP2pDnsSdServiceInfo
import android.net.wifi.p2p.nsd.WifiP2pDnsSdServiceRequest
import android.os.Build
import androidx.annotation.Keep
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.Collections
import java.util.concurrent.Executors

@Keep
class WifiDirectBridge(private val activity: MainActivity) {
  private data class LocalConfig(
    val id: String,
    val name: String,
    val platform: String,
    val version: String,
    val protocolVersion: Int,
    val minProtocolVersion: Int,
    val capabilities: List<String>,
    val port: Int,
    val rendezvousPort: Int,
  )

  private val manager = activity.getSystemService(Context.WIFI_P2P_SERVICE) as? WifiP2pManager
  private val channel = manager?.initialize(activity, activity.mainLooper) {
    emitError(null, "Android 点对点 Wi-Fi 通道已断开")
  }
  private val io = Executors.newCachedThreadPool()
  private val connectedPeerIds = Collections.synchronizedSet(mutableSetOf<String>())
  private var serviceRequest: WifiP2pDnsSdServiceRequest? = null
  private var receiverRegistered = false
  @Volatile private var localConfig: LocalConfig? = null
  @Volatile private var activeGroupKey: String? = null
  @Volatile private var rendezvousServer: ServerSocket? = null

  private val receiver = object : BroadcastReceiver() {
    override fun onReceive(context: Context?, intent: Intent?) {
      when (intent?.action) {
        WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION -> requestConnectionInfo()
        WifiP2pManager.WIFI_P2P_DISCOVERY_CHANGED_ACTION -> {
          val state = intent.getIntExtra(
            WifiP2pManager.EXTRA_DISCOVERY_STATE,
            WifiP2pManager.WIFI_P2P_DISCOVERY_STOPPED,
          )
          if (state == WifiP2pManager.WIFI_P2P_DISCOVERY_STOPPED && localConfig != null) {
            discoverServices()
          }
        }
      }
    }
  }

  init {
    nativeRegister()
  }

  private external fun nativeRegister()
  private external fun nativeEvent(json: String)

  fun activate() {
    activity.runOnUiThread {
      registerReceiver()
      if (hasRequiredPermission()) startDiscoveryPipeline()
    }
  }

  fun permissionsChanged() {
    activity.runOnUiThread {
      if (hasRequiredPermission()) startDiscoveryPipeline()
    }
  }

  @Keep
  fun startWifiDirect(configJson: String) {
    val parsed = try {
      parseConfig(configJson)
    } catch (error: Exception) {
      emitError(null, "点对点 Wi-Fi 配置无效：${error.message ?: "未知错误"}")
      return
    }
    localConfig = parsed
    activity.runOnUiThread {
      registerReceiver()
      if (hasRequiredPermission()) startDiscoveryPipeline()
    }
  }

  @Keep
  @SuppressLint("MissingPermission")
  fun connectWifiDirect(peerId: String, deviceAddress: String) {
    activity.runOnUiThread {
      if (!hasRequiredPermission()) {
        emitError(peerId, "请先允许 Neloa 使用附近设备权限")
        activity.requestWifiDirectPermission()
        return@runOnUiThread
      }
      val wifiManager = manager
      val wifiChannel = channel
      if (wifiManager == null || wifiChannel == null) {
        emitError(peerId, "这台 Android 设备不支持 Wi-Fi Direct")
        return@runOnUiThread
      }
      @Suppress("DEPRECATION")
      val config = WifiP2pConfig().apply {
        this.deviceAddress = deviceAddress
        groupOwnerIntent = 0
      }
      wifiManager.connect(wifiChannel, config, actionListener(
        onSuccess = { requestConnectionInfo() },
        onFailure = { reason -> emitError(peerId, "无法建立点对点 Wi-Fi：${reasonText(reason)}") },
      ))
    }
  }

  fun close() {
    localConfig = null
    clearRoutes()
    if (receiverRegistered) {
      runCatching { activity.unregisterReceiver(receiver) }
      receiverRegistered = false
    }
    val wifiManager = manager
    val wifiChannel = channel
    if (wifiManager != null && wifiChannel != null) {
      serviceRequest?.let { request -> wifiManager.removeServiceRequest(wifiChannel, request, null) }
      wifiManager.clearLocalServices(wifiChannel, null)
      wifiManager.removeGroup(wifiChannel, null)
    }
    io.shutdownNow()
  }

  private fun parseConfig(json: String): LocalConfig {
    val value = JSONObject(json)
    val capabilities = value.getJSONArray("capabilities")
    return LocalConfig(
      id = value.getString("id"),
      name = value.getString("name"),
      platform = value.getString("platform"),
      version = value.getString("version"),
      protocolVersion = value.getInt("protocolVersion"),
      minProtocolVersion = value.getInt("minProtocolVersion"),
      capabilities = buildList {
        for (index in 0 until capabilities.length()) add(capabilities.getString(index))
      },
      port = value.getInt("port"),
      rendezvousPort = value.getInt("rendezvousPort"),
    )
  }

  private fun registerReceiver() {
    if (receiverRegistered) return
    val filter = IntentFilter().apply {
      addAction(WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION)
      addAction(WifiP2pManager.WIFI_P2P_DISCOVERY_CHANGED_ACTION)
      addAction(WifiP2pManager.WIFI_P2P_STATE_CHANGED_ACTION)
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      activity.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
    } else {
      activity.registerReceiver(receiver, filter)
    }
    receiverRegistered = true
  }

  private fun hasRequiredPermission(): Boolean {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return true
    val permission = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      Manifest.permission.NEARBY_WIFI_DEVICES
    } else {
      Manifest.permission.ACCESS_FINE_LOCATION
    }
    return activity.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED
  }

  @SuppressLint("MissingPermission")
  private fun startDiscoveryPipeline() {
    val config = localConfig ?: return
    val wifiManager = manager ?: run {
      emitError(null, "这台 Android 设备不支持 Wi-Fi Direct")
      return
    }
    val wifiChannel = channel ?: return

    wifiManager.setDnsSdResponseListeners(
      wifiChannel,
      { _, _, _ -> },
      { _, record, device -> emitPeer(record, device.deviceAddress) },
    )
    wifiManager.clearLocalServices(wifiChannel, actionListener(
      onSuccess = {
        wifiManager.clearServiceRequests(wifiChannel, actionListener(
          onSuccess = {
            val record = mapOf(
              "id" to config.id,
              "name" to config.name,
              "platform" to config.platform,
              "version" to config.version,
              "protocolVersion" to config.protocolVersion.toString(),
              "minProtocolVersion" to config.minProtocolVersion.toString(),
              "capabilities" to config.capabilities.joinToString(","),
              "port" to config.port.toString(),
            )
            val service = WifiP2pDnsSdServiceInfo.newInstance(
              "Neloa-${config.id.take(8)}",
              "_neloa._udp",
              record,
            )
            wifiManager.addLocalService(wifiChannel, service, actionListener(
              onSuccess = {
                serviceRequest = WifiP2pDnsSdServiceRequest.newInstance()
                wifiManager.addServiceRequest(wifiChannel, serviceRequest, actionListener(
                  onSuccess = { discoverServices() },
                  onFailure = { reason -> emitError(null, "无法添加点对点发现请求：${reasonText(reason)}") },
                ))
              },
              onFailure = { reason -> emitError(null, "无法广播点对点服务：${reasonText(reason)}") },
            ))
          },
          onFailure = { reason -> emitError(null, "无法重置点对点发现请求：${reasonText(reason)}") },
        ))
      },
      onFailure = { reason -> emitError(null, "无法重置点对点服务：${reasonText(reason)}") },
    ))
  }

  @SuppressLint("MissingPermission")
  private fun discoverServices() {
    val wifiManager = manager ?: return
    val wifiChannel = channel ?: return
    if (!hasRequiredPermission() || localConfig == null || serviceRequest == null) return
    wifiManager.discoverServices(wifiChannel, actionListener(
      onSuccess = {},
      onFailure = { reason -> emitError(null, "无法发现点对点设备：${reasonText(reason)}") },
    ))
  }

  private fun emitPeer(record: Map<String, String>, deviceAddress: String) {
    val peerId = record["id"]?.takeIf { it.isNotBlank() } ?: return
    if (peerId == localConfig?.id) return
    val capabilities = JSONArray()
    record["capabilities"]
      .orEmpty()
      .split(',')
      .map(String::trim)
      .filter(String::isNotEmpty)
      .forEach(capabilities::put)
    val peer = JSONObject()
      .put("id", peerId)
      .put("name", record["name"] ?: "Neloa Device")
      .put("platform", record["platform"] ?: "android")
      .put("version", record["version"] ?: "unknown")
      .put("protocolVersion", record["protocolVersion"]?.toIntOrNull() ?: 0)
      .put("minProtocolVersion", record["minProtocolVersion"]?.toIntOrNull() ?: 0)
      .put("capabilities", capabilities)
      .put("deviceAddress", deviceAddress)
    emit(JSONObject().put("type", "peer").put("peer", peer))
  }

  @SuppressLint("MissingPermission")
  private fun requestConnectionInfo() {
    val wifiManager = manager ?: return
    val wifiChannel = channel ?: return
    wifiManager.requestConnectionInfo(wifiChannel) { info ->
      if (info.groupFormed && info.groupOwnerAddress != null) {
        handleGroup(info)
      } else {
        clearRoutes()
      }
    }
  }

  private fun handleGroup(info: WifiP2pInfo) {
    val config = localConfig ?: return
    val ownerAddress = info.groupOwnerAddress ?: return
    val key = "${info.isGroupOwner}:${ownerAddress.hostAddress}"
    if (activeGroupKey == key) return
    clearRendezvousOnly()
    activeGroupKey = key
    if (info.isGroupOwner) {
      startOwnerRendezvous(config, ownerAddress)
    } else {
      startClientRendezvous(config, ownerAddress)
    }
  }

  private fun startOwnerRendezvous(config: LocalConfig, ownerAddress: InetAddress) {
    io.execute {
      try {
        val server = ServerSocket().apply {
          reuseAddress = true
          bind(InetSocketAddress(ownerAddress, config.rendezvousPort))
        }
        rendezvousServer = server
        while (!server.isClosed && !Thread.currentThread().isInterrupted) {
          val socket = server.accept()
          io.execute { acceptRendezvousClient(config, socket) }
        }
      } catch (error: Exception) {
        if (activeGroupKey != null) emitError(null, "点对点地址交换服务已停止：${error.message ?: "未知错误"}")
      }
    }
  }

  private fun acceptRendezvousClient(config: LocalConfig, socket: Socket) {
    socket.use { client ->
      runCatching {
        client.soTimeout = 5_000
        val reader = BufferedReader(InputStreamReader(client.getInputStream(), Charsets.UTF_8))
        val writer = BufferedWriter(OutputStreamWriter(client.getOutputStream(), Charsets.UTF_8))
        val peerId = reader.readLine()?.takeIf(::validPeerId) ?: return
        writer.write(config.id)
        writer.newLine()
        writer.flush()
        emitRoute(peerId, client.inetAddress.hostAddress ?: return, true)
      }.onFailure { error ->
        emitError(null, "无法交换点对点设备地址：${error.message ?: "未知错误"}")
      }
    }
  }

  private fun startClientRendezvous(config: LocalConfig, ownerAddress: InetAddress) {
    io.execute {
      var lastError: Exception? = null
      repeat(40) {
        if (Thread.currentThread().isInterrupted || activeGroupKey == null) return@execute
        try {
          Socket().use { socket ->
            socket.connect(InetSocketAddress(ownerAddress, config.rendezvousPort), 1_500)
            socket.soTimeout = 5_000
            val reader = BufferedReader(InputStreamReader(socket.getInputStream(), Charsets.UTF_8))
            val writer = BufferedWriter(OutputStreamWriter(socket.getOutputStream(), Charsets.UTF_8))
            writer.write(config.id)
            writer.newLine()
            writer.flush()
            val ownerId = reader.readLine()?.takeIf(::validPeerId)
              ?: throw IllegalStateException("对端返回了无效设备 ID")
            emitRoute(ownerId, ownerAddress.hostAddress ?: throw IllegalStateException("缺少组所有者地址"), true)
            return@execute
          }
        } catch (error: Exception) {
          lastError = error
          Thread.sleep(500)
        }
      }
      emitError(null, "无法交换点对点设备地址：${lastError?.message ?: "连接超时"}")
    }
  }

  private fun validPeerId(value: String): Boolean =
    value.length in 1..128 && value.none { it.isWhitespace() || it.isISOControl() }

  private fun emitRoute(peerId: String, address: String, connected: Boolean) {
    if (connected) connectedPeerIds.add(peerId) else connectedPeerIds.remove(peerId)
    emit(JSONObject()
      .put("type", "route")
      .put("peerId", peerId)
      .put("address", address)
      .put("connected", connected))
  }

  private fun clearRendezvousOnly() {
    runCatching { rendezvousServer?.close() }
    rendezvousServer = null
  }

  private fun clearRoutes() {
    activeGroupKey = null
    clearRendezvousOnly()
    synchronized(connectedPeerIds) {
      connectedPeerIds.toList().forEach { peerId -> emitRoute(peerId, "", false) }
      connectedPeerIds.clear()
    }
  }

  private fun emitError(peerId: String?, message: String) {
    emit(JSONObject()
      .put("type", "error")
      .put("peerId", peerId ?: JSONObject.NULL)
      .put("message", message))
  }

  private fun emit(value: JSONObject) {
    runCatching { nativeEvent(value.toString()) }
  }

  private fun actionListener(
    onSuccess: () -> Unit,
    onFailure: (Int) -> Unit,
  ) = object : WifiP2pManager.ActionListener {
    override fun onSuccess() = onSuccess.invoke()
    override fun onFailure(reason: Int) = onFailure.invoke(reason)
  }

  private fun reasonText(reason: Int): String = when (reason) {
    WifiP2pManager.P2P_UNSUPPORTED -> "设备不支持"
    WifiP2pManager.BUSY -> "系统服务繁忙"
    WifiP2pManager.ERROR -> "系统服务错误"
    else -> "错误码 $reason"
  }
}
